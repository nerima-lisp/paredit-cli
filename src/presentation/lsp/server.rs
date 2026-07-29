//! The Language Server Protocol handler.
//!
//! State is a document map and two booleans. There is no work queue, no
//! cancellation, and no background thread: every request this server answers is
//! a parse and a walk of one document, which is microseconds, and a concurrency
//! design bought for latency nobody would notice is a source of bugs nobody
//! asked for.

use std::path::PathBuf;

use serde_json::{Value, json};

use paredit_core_jsonrpc::{Handler, Outcome, Request, ResponseError, error_codes};
use paredit_core_syntax::sexpr::{ByteOffset, ByteSpan, SymbolName, SyntaxTree};

use super::documents::{Documents, PositionEncoding, path_from_uri};
use super::features;

/// A synthetic path for a document with no file behind it.
///
/// Lint findings carry the path they were found in, and an `untitled:` buffer
/// has none. Naming it explicitly is better than an empty path, which reads as
/// the current directory in every message that prints one.
const UNTITLED: &str = "<untitled>";

#[derive(Debug, Default)]
pub(crate) struct Server {
    documents: Documents,
    encoding: PositionEncoding,
    initialized: bool,
    shutting_down: bool,
}

impl Handler for Server {
    fn handle(&mut self, request: Request, notify: &mut dyn FnMut(&str, Value)) -> Outcome {
        // The protocol requires a server to refuse everything before
        // `initialize` and everything after `shutdown`, and clients do send
        // both — a race on startup, and a queued request during teardown.
        if !self.initialized && !matches!(request.method.as_str(), "initialize" | "exit") {
            return self.refuse(
                &request,
                error_codes::INVALID_REQUEST,
                "the server has not been initialized",
            );
        }
        if self.shutting_down && request.method != "exit" {
            return self.refuse(
                &request,
                error_codes::INVALID_REQUEST_AFTER_SHUTDOWN,
                "the server is shutting down",
            );
        }

        match request.method.as_str() {
            "initialize" => self.initialize(&request.params),
            "initialized" => Outcome::Silent,
            "shutdown" => {
                self.shutting_down = true;
                Outcome::Reply(Value::Null)
            }
            "exit" => Outcome::Stop(None),
            "textDocument/didOpen" => self.did_open(&request.params, notify),
            "textDocument/didChange" => self.did_change(&request.params, notify),
            "textDocument/didSave" => self.publish(&text_document_uri(&request.params), notify),
            "textDocument/didClose" => self.did_close(&request.params, notify),
            "textDocument/documentSymbol" => self.document_symbols(&request.params),
            "textDocument/selectionRange" => self.selection_ranges(&request.params),
            "textDocument/foldingRange" => self.folding_ranges(&request.params),
            "textDocument/documentHighlight" => self.document_highlights(&request.params),
            "textDocument/formatting" => self.formatting(&request.params),
            "textDocument/prepareRename" => self.prepare_rename(&request.params),
            "textDocument/rename" => self.rename(&request.params),
            "textDocument/codeAction" => self.code_actions(&request.params),
            // `$/`-prefixed methods are the protocol's own housekeeping. A
            // server that does not implement one must ignore the notification
            // and error the request, which is what falling through does.
            other if other.starts_with("$/") && request.is_notification() => Outcome::Silent,
            other => {
                if request.is_notification() {
                    Outcome::Silent
                } else {
                    Outcome::Fail(ResponseError::method_not_found(other))
                }
            }
        }
    }
}

impl Server {
    fn refuse(&self, request: &Request, code: i64, message: &str) -> Outcome {
        if request.is_notification() {
            Outcome::Silent
        } else {
            Outcome::Fail(ResponseError::new(code, message))
        }
    }

    fn initialize(&mut self, params: &Value) -> Outcome {
        self.initialized = true;
        // LSP 3.17: the client offers the encodings it supports and the server
        // picks one, answering with its choice. Preferring utf-8 when offered
        // makes every position translation a byte count, which is both exact
        // and free; utf-16 is the fallback the protocol guarantees.
        self.encoding = params["capabilities"]["general"]["positionEncodings"]
            .as_array()
            .and_then(|offered| {
                let names: Vec<&str> = offered.iter().filter_map(Value::as_str).collect();
                ["utf-8", "utf-32", "utf-16"]
                    .into_iter()
                    .find(|preferred| names.contains(preferred))
                    .and_then(PositionEncoding::parse)
            })
            .unwrap_or(PositionEncoding::Utf16);

        Outcome::Reply(json!({
            "capabilities": {
                "positionEncoding": self.encoding.label(),
                // Full sync. Incremental sync would save bytes on a large file
                // and cost an incremental text-mutation implementation whose
                // bugs present as an editor and a server disagreeing about the
                // buffer — the worst class of bug this server could have.
                "textDocumentSync": { "openClose": true, "change": 1, "save": true },
                "documentSymbolProvider": true,
                "selectionRangeProvider": true,
                "foldingRangeProvider": true,
                "documentHighlightProvider": true,
                "documentFormattingProvider": true,
                "renameProvider": { "prepareProvider": true },
                "codeActionProvider": { "codeActionKinds": ["quickfix"] },
            },
            "serverInfo": { "name": "paredit", "version": env!("CARGO_PKG_VERSION") },
        }))
    }

    fn did_open(&mut self, params: &Value, notify: &mut dyn FnMut(&str, Value)) -> Outcome {
        let document = &params["textDocument"];
        let (Some(uri), Some(text)) = (document["uri"].as_str(), document["text"].as_str()) else {
            return Outcome::Silent;
        };
        self.documents.open(
            uri.to_owned(),
            text.to_owned(),
            document["version"].as_i64().unwrap_or(0),
        );
        self.publish(uri, notify)
    }

    fn did_change(&mut self, params: &Value, notify: &mut dyn FnMut(&str, Value)) -> Outcome {
        let Some(uri) = params["textDocument"]["uri"].as_str() else {
            return Outcome::Silent;
        };
        // Full sync: the last change in the array carries the whole document.
        let Some(text) = params["contentChanges"]
            .as_array()
            .and_then(|changes| changes.last())
            .and_then(|change| change["text"].as_str())
        else {
            return Outcome::Silent;
        };
        self.documents.change(
            uri,
            text.to_owned(),
            params["textDocument"]["version"].as_i64().unwrap_or(0),
        );
        self.publish(uri, notify)
    }

    fn did_close(&mut self, params: &Value, notify: &mut dyn FnMut(&str, Value)) -> Outcome {
        let Some(uri) = params["textDocument"]["uri"].as_str() else {
            return Outcome::Silent;
        };
        self.documents.close(uri);
        // An empty list is what clears the client's diagnostics for a closed
        // document. Sending nothing leaves them on screen for a file the user
        // can no longer see.
        notify(
            "textDocument/publishDiagnostics",
            json!({ "uri": uri, "diagnostics": [] }),
        );
        Outcome::Silent
    }

    fn publish(&self, uri: &str, notify: &mut dyn FnMut(&str, Value)) -> Outcome {
        let Some(document) = self.documents.get(uri) else {
            return Outcome::Silent;
        };
        let path = path_from_uri(uri).unwrap_or_else(|| PathBuf::from(UNTITLED));
        let mut payload = json!({
            "uri": uri,
            "diagnostics": features::diagnostics(document, &path, self.encoding),
        });
        // Versioned diagnostics let a client drop a result computed against a
        // buffer it has already edited past.
        payload["version"] = json!(document.version);
        notify("textDocument/publishDiagnostics", payload);
        Outcome::Silent
    }

    /// The document and its parse, or the error to answer with.
    fn parsed(
        &self,
        params: &Value,
    ) -> Result<(&super::documents::Document, SyntaxTree), ResponseError> {
        let uri = text_document_uri(params);
        let document = self
            .documents
            .get(&uri)
            .ok_or_else(|| ResponseError::invalid_params(format!("no open document at {uri}")))?;
        let tree = SyntaxTree::parse_with_dialect(&document.text, document.dialect)
            .map_err(|error| ResponseError::new(error_codes::REQUEST_FAILED, error.to_string()))?;
        Ok((document, tree))
    }

    fn document_symbols(&self, params: &Value) -> Outcome {
        match self.parsed(params) {
            Ok((document, tree)) => Outcome::Reply(Value::Array(features::document_symbols(
                document,
                &tree,
                self.encoding,
            ))),
            // An unparseable document has no outline, and an error would make
            // the outline pane show a failure while the user is mid-edit. An
            // empty list is the honest and quiet answer.
            Err(_) => Outcome::Reply(Value::Array(Vec::new())),
        }
    }

    fn selection_ranges(&self, params: &Value) -> Outcome {
        let Ok((document, tree)) = self.parsed(params) else {
            return Outcome::Reply(Value::Null);
        };
        let ranges = params["positions"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .map(|position| {
                let offset = self.offset(document, position);
                let chain = features::selection_chain(&tree, offset);
                features::selection_range_value(document, &chain, self.encoding)
            })
            .collect::<Vec<_>>();
        Outcome::Reply(Value::Array(ranges))
    }

    fn folding_ranges(&self, params: &Value) -> Outcome {
        match self.parsed(params) {
            Ok((document, tree)) => Outcome::Reply(Value::Array(features::folding_ranges(
                document,
                &tree,
                self.encoding,
            ))),
            Err(_) => Outcome::Reply(Value::Array(Vec::new())),
        }
    }

    fn document_highlights(&self, params: &Value) -> Outcome {
        let Ok((document, tree)) = self.parsed(params) else {
            return Outcome::Reply(Value::Null);
        };
        let offset = self.offset(document, &params["position"]);
        let index = tree.atom_occurrence_index();
        let Some(selected) = index
            .occurrences()
            .iter()
            .find(|occurrence| occurrence.span.contains(ByteOffset::new(offset)))
        else {
            return Outcome::Reply(Value::Array(Vec::new()));
        };
        let name = selected.text;
        let highlights = index
            .occurrences()
            .iter()
            .filter(|occurrence| occurrence.text == name)
            .map(|occurrence| {
                json!({
                    "range": features::range(document, occurrence.span, self.encoding),
                    "kind": 1,
                })
            })
            .collect::<Vec<_>>();
        Outcome::Reply(Value::Array(highlights))
    }

    fn formatting(&self, params: &Value) -> Outcome {
        let (document, tree) = match self.parsed(params) {
            Ok(parsed) => parsed,
            Err(error) => return Outcome::Fail(error),
        };
        // The client's own indent setting, so the server does not fight the
        // editor's configuration.
        let indent = params["options"]["tabSize"]
            .as_u64()
            .and_then(|size| usize::try_from(size).ok())
            .filter(|size| *size > 0)
            .unwrap_or(2);
        Outcome::Reply(Value::Array(features::formatting_edits(
            document,
            &tree,
            indent,
            self.encoding,
        )))
    }

    fn prepare_rename(&self, params: &Value) -> Outcome {
        let Ok((document, tree)) = self.parsed(params) else {
            return Outcome::Reply(Value::Null);
        };
        let offset = self.offset(document, &params["position"]);
        let index = tree.atom_occurrence_index();
        index
            .occurrences()
            .iter()
            .find(|occurrence| occurrence.span.contains(ByteOffset::new(offset)))
            .map_or(Outcome::Reply(Value::Null), |occurrence| {
                Outcome::Reply(json!({
                    "range": features::range(document, occurrence.span, self.encoding),
                    "placeholder": occurrence.text,
                }))
            })
    }

    /// `refactor rename-at`, as a `WorkspaceEdit`.
    ///
    /// Single-file: the plan this dispatches to resolves the namespace and
    /// lexical scope of whatever occupies the offset, and its answer is about
    /// that document. A rename that should cross files is `refactor
    /// rename-symbol` over the workspace, which is a different operation with
    /// a different safety story, and quietly widening this one to it would
    /// rewrite files the user never opened.
    fn rename(&self, params: &Value) -> Outcome {
        let (document, _) = match self.parsed(params) {
            Ok(parsed) => parsed,
            Err(error) => return Outcome::Fail(error),
        };
        let uri = text_document_uri(params);
        let offset = self.offset(document, &params["position"]);
        let Some(new_name) = params["newName"].as_str() else {
            return Outcome::Fail(ResponseError::invalid_params("newName is required"));
        };
        let Ok(to) = SymbolName::new(new_name) else {
            return Outcome::Fail(ResponseError::invalid_params(format!(
                "{new_name:?} is not a valid symbol"
            )));
        };

        let plan = paredit_feature_rename::rename::usecase::plan_rename_at(
            paredit_feature_rename::rename::usecase::RenameAtRequest {
                input: &document.text,
                dialect: document.dialect,
                at: ByteOffset::new(offset),
                to,
            },
        );
        match plan {
            Ok(plan) => {
                let edits = plan
                    .occurrences
                    .iter()
                    .map(|span| {
                        json!({
                            "range": features::range(document, *span, self.encoding),
                            "newText": plan.to.as_str(),
                        })
                    })
                    .collect::<Vec<_>>();
                Outcome::Reply(json!({ "changes": { uri: edits } }))
            }
            // The refusal reason is what makes a rename this tool declines
            // actionable — "supports only Common Lisp", "the selection is not a
            // symbol" — so it goes to the user rather than being flattened into
            // a generic failure.
            Err(error) => Outcome::Fail(ResponseError::new(
                error_codes::REQUEST_FAILED,
                error.to_string(),
            )),
        }
    }

    fn code_actions(&self, params: &Value) -> Outcome {
        let Ok((document, _)) = self.parsed(params) else {
            return Outcome::Reply(Value::Array(Vec::new()));
        };
        let uri = text_document_uri(params);
        let path = path_from_uri(&uri).unwrap_or_else(|| PathBuf::from(UNTITLED));
        let selected = ByteSpan::new(
            ByteOffset::new(self.offset(document, &params["range"]["start"])),
            ByteOffset::new(self.offset(document, &params["range"]["end"])),
        );
        Outcome::Reply(Value::Array(features::code_actions(
            document,
            &path,
            &uri,
            selected,
            self.encoding,
        )))
    }

    fn offset(&self, document: &super::documents::Document, position: &Value) -> usize {
        let line = position["line"].as_u64().unwrap_or(0) as usize;
        let character = position["character"].as_u64().unwrap_or(0) as usize;
        document.offset_of(line, character, self.encoding)
    }

    #[cfg(test)]
    pub(crate) fn open_uris(&self) -> Vec<String> {
        self.documents.uris()
    }
}

fn text_document_uri(params: &Value) -> String {
    params["textDocument"]["uri"]
        .as_str()
        .unwrap_or_default()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(method: &str, params: Value, id: Option<i64>) -> Request {
        Request {
            method: method.to_owned(),
            params,
            id: id.map(|id| json!(id)),
        }
    }

    /// Drives a server through initialization and one open document.
    fn opened(text: &str) -> (Server, Vec<(String, Value)>) {
        let mut server = Server::default();
        let mut sent: Vec<(String, Value)> = Vec::new();
        let mut notify = |method: &str, params: Value| {
            sent.push((method.to_owned(), params));
        };
        server.handle(request("initialize", json!({}), Some(1)), &mut notify);
        server.handle(
            request(
                "textDocument/didOpen",
                json!({
                    "textDocument": {
                        "uri": "file:///tmp/core.lisp",
                        "languageId": "lisp",
                        "version": 1,
                        "text": text,
                    },
                }),
                None,
            ),
            &mut notify,
        );
        (server, sent)
    }

    fn call(server: &mut Server, method: &str, params: Value) -> Value {
        let mut notify = |_: &str, _: Value| {};
        match server.handle(request(method, params, Some(2)), &mut notify) {
            Outcome::Reply(value) => value,
            other => panic!("expected a reply from {method}, got {other:?}"),
        }
    }

    fn error_of(server: &mut Server, method: &str, params: Value) -> ResponseError {
        let mut notify = |_: &str, _: Value| {};
        match server.handle(request(method, params, Some(3)), &mut notify) {
            Outcome::Fail(error) => error,
            other => panic!("expected a failure from {method}, got {other:?}"),
        }
    }

    const DOCUMENT: &str = "file:///tmp/core.lisp";

    #[test]
    fn initialize_advertises_the_features_this_server_implements() {
        let mut server = Server::default();
        let capabilities = call(&mut server, "initialize", json!({}))["capabilities"].clone();
        for provider in [
            "documentSymbolProvider",
            "selectionRangeProvider",
            "foldingRangeProvider",
            "documentFormattingProvider",
            "documentHighlightProvider",
        ] {
            assert_eq!(capabilities[provider], true, "{provider}");
        }
        assert_eq!(capabilities["renameProvider"]["prepareProvider"], true);
        assert_eq!(
            capabilities["codeActionProvider"]["codeActionKinds"][0],
            "quickfix"
        );
    }

    /// Clients race the server on startup and queue requests during teardown.
    /// Both are protocol errors the server must name rather than serve.
    #[test]
    fn requests_before_initialize_and_after_shutdown_are_refused() {
        let mut server = Server::default();
        let early = error_of(&mut server, "textDocument/documentSymbol", json!({}));
        assert!(early.message.contains("not been initialized"), "{early:?}");

        call(&mut server, "initialize", json!({}));
        call(&mut server, "shutdown", Value::Null);
        let late = error_of(&mut server, "textDocument/documentSymbol", json!({}));
        assert!(late.message.contains("shutting down"), "{late:?}");
    }

    #[test]
    fn exit_stops_the_loop() {
        let mut server = Server::default();
        call(&mut server, "initialize", json!({}));
        let mut notify = |_: &str, _: Value| {};
        assert!(matches!(
            server.handle(request("exit", Value::Null, None), &mut notify),
            Outcome::Stop(_)
        ));
    }

    /// An unknown *notification* must be ignored and an unknown *request*
    /// answered with an error. Answering a notification is a protocol
    /// violation some clients treat as fatal.
    #[test]
    fn an_unknown_notification_is_ignored_and_an_unknown_request_errors() {
        let mut server = Server::default();
        call(&mut server, "initialize", json!({}));
        let mut notify = |_: &str, _: Value| {};
        assert!(matches!(
            server.handle(request("$/setTrace", json!({}), None), &mut notify),
            Outcome::Silent
        ));
        assert!(matches!(
            server.handle(
                request("textDocument/telepathy", json!({}), Some(9)),
                &mut notify
            ),
            Outcome::Fail(_)
        ));
    }

    #[test]
    fn opening_a_document_publishes_its_diagnostics() {
        let (_, sent) = opened("(defun f (x) (if (not x) 1 2))\n");
        let (method, params) = sent.last().expect("a notification");
        assert_eq!(method, "textDocument/publishDiagnostics");
        assert_eq!(params["uri"], DOCUMENT);
        assert!(
            params["diagnostics"]
                .as_array()
                .expect("diagnostics")
                .iter()
                .any(|diagnostic| diagnostic["code"] == "negated-if"),
            "{params}"
        );
    }

    /// Closing must clear what was published, or the client keeps showing
    /// diagnostics for a file the user cannot see.
    #[test]
    fn closing_a_document_clears_its_diagnostics() {
        let (mut server, _) = opened("(defun f (x) (if (not x) 1 2))\n");
        let mut sent: Vec<(String, Value)> = Vec::new();
        let mut notify = |method: &str, params: Value| sent.push((method.to_owned(), params));
        server.handle(
            request(
                "textDocument/didClose",
                json!({ "textDocument": { "uri": DOCUMENT } }),
                None,
            ),
            &mut notify,
        );
        assert_eq!(sent[0].1["diagnostics"], json!([]));
        assert!(server.open_uris().is_empty());
    }

    #[test]
    fn a_change_republishes_against_the_new_text() {
        let (mut server, _) = opened("(defun f (x) (if (not x) 1 2))\n");
        let mut sent: Vec<(String, Value)> = Vec::new();
        let mut notify = |method: &str, params: Value| sent.push((method.to_owned(), params));
        server.handle(
            request(
                "textDocument/didChange",
                json!({
                    "textDocument": { "uri": DOCUMENT, "version": 2 },
                    "contentChanges": [{ "text": "(defun f (x) (if x 2 1))\n" }],
                }),
                None,
            ),
            &mut notify,
        );
        assert_eq!(sent[0].1["diagnostics"], json!([]));
        assert_eq!(sent[0].1["version"], 2);
    }

    #[test]
    fn selection_range_expands_outward_from_the_caret() {
        let (mut server, _) = opened("(defun f (x) (+ x 1))\n");
        let value = call(
            &mut server,
            "textDocument/selectionRange",
            json!({
                "textDocument": { "uri": DOCUMENT },
                "positions": [{ "line": 0, "character": 16 }],
            }),
        );
        let first = &value[0];
        assert_eq!(first["range"]["start"]["character"], 16);
        assert_eq!(first["parent"]["range"]["start"]["character"], 13);
        assert_eq!(first["parent"]["parent"]["range"]["start"]["character"], 0);
    }

    #[test]
    fn document_symbols_list_the_definitions() {
        let (mut server, _) = opened("(defun alpha () 1)\n(defun beta () 2)\n");
        let symbols = call(
            &mut server,
            "textDocument/documentSymbol",
            json!({ "textDocument": { "uri": DOCUMENT } }),
        );
        let names: Vec<&str> = symbols
            .as_array()
            .expect("symbols")
            .iter()
            .filter_map(|symbol| symbol["name"].as_str())
            .collect();
        assert_eq!(names, vec!["defun", "defun"]);
    }

    /// An unparseable buffer is the normal state while typing. An outline pane
    /// showing an error for it is worse than one showing nothing.
    #[test]
    fn an_unparseable_document_yields_an_empty_outline_rather_than_an_error() {
        let (mut server, _) = opened("(defun f (x)\n");
        let symbols = call(
            &mut server,
            "textDocument/documentSymbol",
            json!({ "textDocument": { "uri": DOCUMENT } }),
        );
        assert_eq!(symbols, json!([]));
    }

    #[test]
    fn rename_rewrites_every_occurrence_of_the_selected_binding() {
        let (mut server, _) = opened("(defun f (limit)\n  (+ limit limit))\n");
        let edit = call(
            &mut server,
            "textDocument/rename",
            json!({
                "textDocument": { "uri": DOCUMENT },
                "position": { "line": 0, "character": 11 },
                "newName": "bound",
            }),
        );
        let edits = edit["changes"][DOCUMENT].as_array().expect("edits");
        assert_eq!(edits.len(), 3);
        assert!(edits.iter().all(|edit| edit["newText"] == "bound"));
    }

    #[test]
    fn prepare_rename_reports_the_symbol_under_the_caret() {
        let (mut server, _) = opened("(defun render-pane (limit) limit)\n");
        let prepared = call(
            &mut server,
            "textDocument/prepareRename",
            json!({
                "textDocument": { "uri": DOCUMENT },
                "position": { "line": 0, "character": 9 },
            }),
        );
        assert_eq!(prepared["placeholder"], "render-pane");
    }

    #[test]
    fn a_code_action_carries_the_edit_that_applies_the_fix() {
        let (mut server, _) = opened("(defun f (x) (progn x))\n");
        let actions = call(
            &mut server,
            "textDocument/codeAction",
            json!({
                "textDocument": { "uri": DOCUMENT },
                "range": {
                    "start": { "line": 0, "character": 13 },
                    "end": { "line": 0, "character": 13 },
                },
                "context": { "diagnostics": [] },
            }),
        );
        let actions = actions.as_array().expect("actions");
        assert!(!actions.is_empty(), "{actions:?}");
        assert_eq!(actions[0]["kind"], "quickfix");
        assert!(
            !actions[0]["edit"]["changes"][DOCUMENT]
                .as_array()
                .expect("edits")
                .is_empty()
        );
    }

    /// The client picks the encoding; the server must answer with the one it
    /// chose and then use it. Advertising utf-16 and computing utf-8 offsets is
    /// the classic version of this bug.
    #[test]
    fn the_negotiated_encoding_is_answered_and_then_honoured() {
        let mut server = Server::default();
        let result = call(
            &mut server,
            "initialize",
            json!({ "capabilities": { "general": { "positionEncodings": ["utf-8", "utf-16"] } } }),
        );
        assert_eq!(result["capabilities"]["positionEncoding"], "utf-8");

        let mut notify = |_: &str, _: Value| {};
        server.handle(
            request(
                "textDocument/didOpen",
                json!({
                    "textDocument": {
                        "uri": DOCUMENT,
                        "version": 1,
                        "text": "(f \"🎈\" target)\n",
                    },
                }),
                None,
            ),
            &mut notify,
        );
        // `target` starts at byte 11 in utf-8 terms; in utf-16 it would be 9.
        let prepared = call(
            &mut server,
            "textDocument/prepareRename",
            json!({
                "textDocument": { "uri": DOCUMENT },
                "position": { "line": 0, "character": 12 },
            }),
        );
        assert_eq!(prepared["placeholder"], "target");
    }

    #[test]
    fn formatting_uses_the_clients_tab_size() {
        let (mut server, _) = opened("(defun f (x)\n(+ x 1))\n");
        let edits = call(
            &mut server,
            "textDocument/formatting",
            json!({
                "textDocument": { "uri": DOCUMENT },
                "options": { "tabSize": 4, "insertSpaces": true },
            }),
        );
        let text = edits[0]["newText"].as_str().expect("formatted text");
        assert!(text.contains("\n    (+ x 1)"), "{text}");
    }

    #[test]
    fn a_request_for_an_unopened_document_says_so() {
        let mut server = Server::default();
        call(&mut server, "initialize", json!({}));
        let error = error_of(
            &mut server,
            "textDocument/formatting",
            json!({ "textDocument": { "uri": "file:///tmp/never-opened.lisp" } }),
        );
        assert_eq!(error.code, error_codes::INVALID_PARAMS);
    }
}
