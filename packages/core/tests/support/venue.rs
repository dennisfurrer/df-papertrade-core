//! A fake papertrade behind the [`Http`] wire, for whole journeys through a
//! [`Client`](df_papertrade_core::Client), at the endpoints [`Config::mainnet`] names:
//!
//! - The relayer answers a `GET` from the recording given for its path (query included), and a
//!   `POST` from the answers queued for its path, in order.
//! - HyperEVM's JSON-RPC answers by method, or by the contract an `eth_call` goes to.
//! - Hyperliquid's `/exchange` takes a user-signed action for mainnet only and answers from a
//!   queue.
//! - A failure can stand in for the next answer on a path: an HTTP status with its body.
//!
//! Every request is kept. Anything it was not given an answer for fails the test, naming the
//! request.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::future::{Future, ready};
use std::rc::Rc;

use df_hyperliquid_core::client::{Http, Method, Request, Response, TransportError};
use df_papertrade_core::Config;
use serde_json::Value;

use super::{config, fixture};

/// Clones share one venue, as the client clones its wire.
#[derive(Clone)]
pub struct Venue(Rc<Inner>);

struct Inner {
    config: Config,
    gets: RefCell<Vec<(String, String)>>,
    posts: RefCell<Vec<(String, VecDeque<String>)>>,
    rpc: RefCell<Vec<Rpc>>,
    exchange: RefCell<VecDeque<String>>,
    failures: RefCell<VecDeque<(String, u16, String)>>,
    sent: RefCell<Vec<Sent>>,
}

/// One JSON-RPC answer: for `method`, and for a call to `to` when given.
struct Rpc {
    method: String,
    to: Option<String>,
    answer: Value,
}

/// A request the venue took.
#[derive(Debug, Clone)]
pub enum Sent {
    /// To the relayer: its method, its path from the relayer's root, and its JSON body.
    Relayer {
        method: Method,
        path: String,
        body: Option<Value>,
    },
    Rpc(Value),
    Exchange(Value),
}

impl Venue {
    pub fn new() -> Venue {
        Venue(Rc::new(Inner {
            config: config(),
            gets: RefCell::default(),
            posts: RefCell::default(),
            rpc: RefCell::default(),
            exchange: RefCell::default(),
            failures: RefCell::default(),
            sent: RefCell::default(),
        }))
    }

    /// Answers every `GET` to `path` with the recording at `fixtures/<recording>`.
    #[must_use]
    pub fn get(self, path: &str, recording: &str) -> Venue {
        self.get_text(path, &fixture(recording))
    }

    /// Answers every `GET` to `path` with `answer`.
    #[must_use]
    pub fn get_text(self, path: &str, answer: &str) -> Venue {
        self.0.gets.borrow_mut().push((path.to_owned(), answer.to_owned()));
        self
    }

    /// Queues `answer` for the next `POST` to `path`.
    #[must_use]
    pub fn post(self, path: &str, answer: &str) -> Venue {
        {
            let mut posts = self.0.posts.borrow_mut();
            match posts.iter_mut().find(|(queued, _)| queued == path) {
                Some((_, answers)) => answers.push_back(answer.to_owned()),
                None => posts.push((path.to_owned(), VecDeque::from([answer.to_owned()]))),
            }
        }
        self
    }

    /// Answers every JSON-RPC call to `method` with `answer`, a whole JSON-RPC answer as a node
    /// writes it; its `id` is set to the request's.
    #[must_use]
    pub fn rpc(self, method: &str, answer: &str) -> Venue {
        let answer = serde_json::from_str(answer).unwrap();
        self.0.rpc.borrow_mut().push(Rpc { method: method.to_owned(), to: None, answer });
        self
    }

    /// Answers every `eth_call` to the contract at `to` with `answer`. Give these before a
    /// [`Venue::rpc`] for `eth_call`: the first match wins.
    #[must_use]
    pub fn rpc_call(self, to: &str, answer: &str) -> Venue {
        let answer = serde_json::from_str(answer).unwrap();
        self.0.rpc.borrow_mut().push(Rpc { method: "eth_call".to_owned(), to: Some(to.to_ascii_lowercase()), answer });
        self
    }

    /// Queues `answer` for the next action posted to Hyperliquid's `/exchange`.
    #[must_use]
    pub fn exchange(self, answer: &str) -> Venue {
        self.0.exchange.borrow_mut().push_back(answer.to_owned());
        self
    }

    /// Answers the next request whose URL ends in `path` with HTTP `status` and `body`, in place
    /// of the venue's own answer, which it then needs none of. The request is still taken.
    #[must_use]
    pub fn status(self, path: &str, status: u16, body: &str) -> Venue {
        self.0.failures.borrow_mut().push_back((path.to_owned(), status, body.to_owned()));
        self
    }

    /// Every request, in order.
    pub fn sent(&self) -> Vec<Sent> {
        self.0.sent.borrow().clone()
    }

    /// The relayer requests as `METHOD path`, in order.
    pub fn relayed(&self) -> Vec<String> {
        self.sent()
            .into_iter()
            .filter_map(|sent| match sent {
                Sent::Relayer { method, path, .. } => Some(format!("{} {path}", word(method))),
                _ => None,
            })
            .collect()
    }

    /// The bodies posted to the relayer at `path`, in order.
    pub fn posted(&self, path: &str) -> Vec<Value> {
        self.sent()
            .into_iter()
            .filter_map(|sent| match sent {
                Sent::Relayer { method: Method::Post, path: at, body } if at == path => body,
                _ => None,
            })
            .collect()
    }

    /// The JSON-RPC calls, in order.
    pub fn calls(&self) -> Vec<Value> {
        self.sent().into_iter().filter_map(|sent| if let Sent::Rpc(call) = sent { Some(call) } else { None }).collect()
    }

    /// The bodies posted to Hyperliquid's `/exchange`, in order.
    pub fn exchanged(&self) -> Vec<Value> {
        self.sent()
            .into_iter()
            .filter_map(|sent| if let Sent::Exchange(body) = sent { Some(body) } else { None })
            .collect()
    }

    fn answer(&self, request: &Request) -> Response {
        let body = (!request.body.is_empty()).then(|| {
            serde_json::from_slice::<Value>(&request.body).unwrap_or_else(|error| {
                panic!("a body that is not JSON ({error}): {}", String::from_utf8_lossy(&request.body))
            })
        });
        let failure = self.failure(&request.url);
        let failing = failure.is_some();
        let config = &self.0.config;
        let answer = if let Some(path) = request.url.strip_prefix(config.relayer.as_str()) {
            self.relayer(request.method, path, body, failing)
        } else if request.url == config.rpc {
            self.node(body.unwrap_or_else(|| panic!("a JSON-RPC call with no body")), failing)
        } else if request.url == config.exchange {
            self.act(body.unwrap_or_else(|| panic!("an /exchange post with no body")), failing)
        } else {
            panic!("the venue does not serve {}", request.url)
        };
        match failure {
            Some((status, body)) => Response { status, headers: Vec::new(), body: body.into_bytes() },
            None => Response { status: 200, headers: Vec::new(), body: answer.into_bytes() },
        }
    }

    fn failure(&self, url: &str) -> Option<(u16, String)> {
        let mut failures = self.0.failures.borrow_mut();
        let at = failures.iter().position(|(path, ..)| url.ends_with(path.as_str()))?;
        failures.remove(at).map(|(_, status, body)| (status, body))
    }

    /// Each handler takes the request, then answers it unless a failure stands in.
    fn relayer(&self, method: Method, path: &str, body: Option<Value>, failing: bool) -> String {
        self.0.sent.borrow_mut().push(Sent::Relayer { method, path: path.to_owned(), body });
        if failing {
            return String::new();
        }
        let answer = match method {
            Method::Get => self.0.gets.borrow().iter().find(|(at, _)| at == path).map(|(_, answer)| answer.clone()),
            Method::Post => self
                .0
                .posts
                .borrow_mut()
                .iter_mut()
                .find(|(at, _)| at == path)
                .and_then(|(_, answers)| answers.pop_front()),
        };
        answer.unwrap_or_else(|| panic!("the relayer has no answer for {} {path}", word(method)))
    }

    fn node(&self, call: Value, failing: bool) -> String {
        self.0.sent.borrow_mut().push(Sent::Rpc(call.clone()));
        if failing {
            return String::new();
        }
        let method = call["method"].as_str().unwrap_or_else(|| panic!("a JSON-RPC call with no method: {call}"));
        let to = call["params"][0]["to"].as_str().map(str::to_ascii_lowercase);
        let rpc = self.0.rpc.borrow();
        let route = rpc.iter().find(|rpc| rpc.method == method && (rpc.to.is_none() || rpc.to == to));
        let mut answer = route.unwrap_or_else(|| panic!("the node has no answer for {call}")).answer.clone();
        answer["id"] = call["id"].clone();
        answer.to_string()
    }

    fn act(&self, body: Value, failing: bool) -> String {
        let action = &body["action"];
        assert!(action["signatureChainId"].is_string(), "a deposit is a user-signed action: {body}");
        assert_eq!(action["hyperliquidChain"], "Mainnet", "a user-signed action for another chain: {body}");
        assert!(body["nonce"].is_u64() && body["signature"]["r"].is_string(), "unsigned: {body}");
        self.0.sent.borrow_mut().push(Sent::Exchange(body.clone()));
        if failing {
            return String::new();
        }
        let answer = self.0.exchange.borrow_mut().pop_front();
        answer.unwrap_or_else(|| panic!("/exchange has no answer queued for {body}"))
    }
}

impl Default for Venue {
    fn default() -> Self {
        Venue::new()
    }
}

impl Http for Venue {
    fn send(&self, request: Request) -> impl Future<Output = Result<Response, TransportError>> {
        ready(Ok(self.answer(&request)))
    }
}

fn word(method: Method) -> &'static str {
    match method {
        Method::Get => "GET",
        Method::Post => "POST",
    }
}
