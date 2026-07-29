//! JSON-RPC 2.0 messages, reduced to what a server needs.
//!
//! A server receives requests and notifications and answers the first kind.
//! It never *makes* a request, so the client-side half of the protocol — the
//! outstanding-call table, the response correlation — is absent, and nothing
//! here has to be `Send` or synchronised.

use serde_json::{Value, json};

/// The subset of JSON-RPC error codes these servers use.
pub mod error_codes {
    pub const PARSE_ERROR: i64 = -32700;
    pub const INVALID_REQUEST: i64 = -32600;
    pub const METHOD_NOT_FOUND: i64 = -32601;
    pub const INVALID_PARAMS: i64 = -32602;
    pub const INTERNAL_ERROR: i64 = -32603;
    /// The Language Server Protocol's own extension, for a request that arrives
    /// after `shutdown`.
    pub const INVALID_REQUEST_AFTER_SHUTDOWN: i64 = -32600;
    /// LSP's `RequestFailed`: the request was well-formed and the server could
    /// not carry it out.
    pub const REQUEST_FAILED: i64 = -32803;
}

/// One incoming call.
#[derive(Debug, Clone)]
pub struct Request {
    pub method: String,
    pub params: Value,
    /// `None` for a notification, which by definition takes no response — and
    /// answering one anyway is a protocol violation that some clients treat as
    /// fatal.
    pub id: Option<Value>,
}

impl Request {
    #[must_use]
    pub const fn is_notification(&self) -> bool {
        self.id.is_none()
    }

    /// Parses one JSON value as a request.
    ///
    /// Returns `None` for anything that is not one — a response, a batch, a
    /// bare array — rather than erroring, because these servers issue no
    /// requests and so have nothing to correlate a response with. Silently
    /// ignoring is what the specification asks of a party that receives a
    /// message it did not solicit.
    #[must_use]
    pub fn from_value(value: &Value) -> Option<Self> {
        let object = value.as_object()?;
        let method = object.get("method")?.as_str()?.to_owned();
        Some(Self {
            method,
            params: object.get("params").cloned().unwrap_or(Value::Null),
            id: object.get("id").cloned().filter(|id| !id.is_null()),
        })
    }
}

/// A failed request.
#[derive(Debug, Clone)]
pub struct ResponseError {
    pub code: i64,
    pub message: String,
    pub data: Option<Value>,
}

impl ResponseError {
    #[must_use]
    pub fn new(code: i64, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }

    #[must_use]
    pub fn method_not_found(method: &str) -> Self {
        Self::new(
            error_codes::METHOD_NOT_FOUND,
            format!("method not found: {method}"),
        )
    }

    #[must_use]
    pub fn invalid_params(detail: impl Into<String>) -> Self {
        Self::new(error_codes::INVALID_PARAMS, detail)
    }

    #[must_use]
    pub fn to_value(&self) -> Value {
        let mut value = json!({ "code": self.code, "message": self.message });
        if let Some(data) = &self.data {
            value["data"] = data.clone();
        }
        value
    }
}

/// What a handler decided about one message.
#[derive(Debug)]
pub enum Outcome {
    /// Answer the request with this result.
    Reply(Value),
    /// Answer the request with this error.
    Fail(ResponseError),
    /// Write nothing. The right outcome for every notification, and for a
    /// request the server deliberately leaves unanswered.
    Silent,
    /// Answer, then stop reading. `exit` and a closed transport both end here.
    Stop(Option<Value>),
}

/// What a server implements.
///
/// `notify` is how a handler pushes a message the client did not ask for —
/// diagnostics, progress — without owning the transport. The transport supplies
/// it as a callback because it, not the handler, knows the framing.
pub trait Handler {
    fn handle(&mut self, request: Request, notify: &mut dyn FnMut(&str, Value)) -> Outcome;
}

/// Builds a JSON-RPC response envelope.
#[must_use]
pub fn response(id: Value, result: Result<Value, ResponseError>) -> Value {
    match result {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err(error) => json!({ "jsonrpc": "2.0", "id": id, "error": error.to_value() }),
    }
}

/// Builds a JSON-RPC notification envelope.
#[must_use]
pub fn notification(method: &str, params: Value) -> Value {
    json!({ "jsonrpc": "2.0", "method": method, "params": params })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_message_without_an_id_is_a_notification() {
        let request = Request::from_value(&json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {},
        }))
        .expect("parses");
        assert!(request.is_notification());
    }

    /// A null id is JSON-RPC's "no id", not an id whose value is null.
    /// Treating it as an id would have the server answer a notification.
    #[test]
    fn a_null_id_is_no_id() {
        let request =
            Request::from_value(&json!({ "method": "exit", "id": null })).expect("parses");
        assert!(request.is_notification());
    }

    #[test]
    fn a_response_is_not_mistaken_for_a_request() {
        assert!(Request::from_value(&json!({ "id": 1, "result": {} })).is_none());
        assert!(Request::from_value(&json!([1, 2, 3])).is_none());
    }

    #[test]
    fn missing_params_read_as_null_rather_than_failing() {
        let request =
            Request::from_value(&json!({ "method": "shutdown", "id": 1 })).expect("parses");
        assert_eq!(request.params, Value::Null);
    }

    #[test]
    fn an_error_response_carries_the_code_and_message() {
        let value = response(
            json!(7),
            Err(ResponseError::method_not_found("textDocument/telepathy")),
        );
        assert_eq!(value["id"], 7);
        assert_eq!(value["error"]["code"], error_codes::METHOD_NOT_FOUND);
        assert!(
            value["error"]["message"]
                .as_str()
                .is_some_and(|message| message.contains("telepathy")),
            "{value}"
        );
        assert!(value.get("result").is_none(), "{value}");
    }
}
