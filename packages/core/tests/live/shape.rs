//! The shape of a JSON answer: its key paths, whatever the values, so a live answer can be held
//! against the recording of the same read.
//!
//! Arrays read as `[]`. An object's keys read as `*` when they are data rather than field names
//! (coins, addresses, ids: anything not lowerCamelCase, or more than [`MAX_FIELDS`] of them).
//! A path under an empty array, an empty object or a `null` in the recording is not counted as
//! added: the recording had nothing there to compare with.

use std::collections::BTreeSet;

use serde_json::Value;

/// More keys than this and an object is a map, not a record.
const MAX_FIELDS: usize = 40;

#[derive(Debug, Default)]
struct Shape {
    paths: BTreeSet<String>,
    /// Where the recording held nothing to compare: empty arrays and objects, and `null`s.
    open: BTreeSet<String>,
}

/// The key paths `live` has that `recorded` lacks: what the venue has added since.
pub fn added(recorded: &Value, live: &Value) -> Vec<String> {
    let recorded = shape(recorded);
    shape(live)
        .paths
        .into_iter()
        .filter(|path| !recorded.paths.contains(path))
        .filter(|path| !recorded.open.iter().any(|open| under(path, open)))
        .collect()
}

/// Whether `path` is `open` or inside it.
fn under(path: &str, open: &str) -> bool {
    open.is_empty()
        || path.strip_prefix(open).is_some_and(|rest| rest.is_empty() || rest.starts_with('.') || rest.starts_with('['))
}

fn shape(value: &Value) -> Shape {
    let mut shape = Shape::default();
    walk(value, "", &mut shape);
    shape
}

fn walk(value: &Value, path: &str, shape: &mut Shape) {
    match value {
        Value::Object(fields) => {
            if fields.is_empty() {
                shape.open.insert(path.to_owned());
            }
            let map = fields.len() > MAX_FIELDS || fields.keys().any(|key| !is_field(key));
            for (key, inner) in fields {
                let name = if map { "*" } else { key.as_str() };
                let inner_path = if path.is_empty() { name.to_owned() } else { format!("{path}.{name}") };
                shape.paths.insert(inner_path.clone());
                walk(inner, &inner_path, shape);
            }
        }
        Value::Array(items) => {
            let inner_path = format!("{path}[]");
            if items.is_empty() {
                shape.open.insert(inner_path.clone());
            }
            for item in items {
                walk(item, &inner_path, shape);
            }
        }
        Value::Null => {
            shape.open.insert(path.to_owned());
        }
        Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}

/// lowerCamelCase, as the venue names its fields.
fn is_field(key: &str) -> bool {
    key.starts_with(|first: char| first.is_ascii_lowercase()) && key.chars().all(|c| c.is_ascii_alphanumeric())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::added;

    #[test]
    fn a_new_field_is_added_and_new_values_are_not() {
        let recorded = json!({"universe": [{"name": "BTC", "szDecimals": 5}], "mids": {"BTC": "1"}, "vault": null});
        let live = json!({
            "universe": [{"name": "ETH", "szDecimals": 4, "growthMode": "on"}],
            "mids": {"ETH": "2", "SOL": "3"},
            "vault": {"name": "HLP"},
        });
        assert_eq!(added(&recorded, &live), ["universe[].growthMode"]);
    }
}
