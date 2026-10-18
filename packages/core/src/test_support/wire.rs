//! A scripted [`Http`] wire: each request is answered by the first route that matches its
//! method, URL prefix and body, and every request is kept.

use std::cell::RefCell;
use std::future::Future;
use std::rc::Rc;

use df_hyperliquid_core::client::{Http, Method, Request, Response, TransportError};

/// One scripted answer: method, URL prefix, body substring (case-insensitive), status, body.
pub(crate) type Route = (Method, &'static str, &'static str, u16, String);

/// Matches by body substring too, since JSON-RPC calls share a URL.
#[derive(Default)]
pub(crate) struct Scripted {
    routes: RefCell<Vec<Route>>,
    seen: RefCell<Vec<Request>>,
}

#[derive(Clone, Default)]
pub(crate) struct Wire(Rc<Scripted>);

impl Wire {
    pub(crate) fn on(self, method: Method, url: &'static str, piece: &'static str, status: u16, body: &str) -> Self {
        self.0.routes.borrow_mut().push((method, url, piece, status, body.to_owned()));
        self
    }

    pub(crate) fn sent(&self) -> Vec<Request> {
        self.0.seen.borrow().clone()
    }
}

impl Http for Wire {
    fn send(&self, request: Request) -> impl Future<Output = Result<Response, TransportError>> {
        let body = String::from_utf8_lossy(&request.body).to_lowercase();
        let found = self.0.routes.borrow().iter().find_map(|(method, url, piece, status, answer)| {
            (*method == request.method && request.url.starts_with(url) && body.contains(&piece.to_lowercase()))
                .then(|| Response { status: *status, headers: Vec::new(), body: answer.clone().into_bytes() })
        });
        let answer = found.ok_or_else(|| TransportError::Unreachable(format!("no fixture for {} {body}", request.url)));
        self.0.seen.borrow_mut().push(request);
        std::future::ready(answer)
    }
}
