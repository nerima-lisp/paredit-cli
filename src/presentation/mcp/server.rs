//! The Model Context Protocol handler.
//!
//! MCP is JSON-RPC with a fixed method set: `initialize`, then `tools/list`,
//! `tools/call`, `resources/list`, `resources/read`. The framing is
//! newline-delimited rather than length-prefixed, which is the only wire-level
//! difference from the language server.

use serde_json::{Value, json};

use paredit_core_jsonrpc::{Handler, Outcome, Request, ResponseError, error_codes};

use super::tools;

/// The protocol revision this server implements.
///
/// Answered verbatim rather than echoing the client's: the specification says a
/// server that does not support the requested version replies with one it does,
/// and echoing would claim support for whatever a future client asks for.
const PROTOCOL_VERSION: &str = "2024-11-05";

#[derive(Debug, Default)]
pub(super) struct Server {
    /// Refuses every command that would write a file. For an agent given a
    /// repository it should analyse and not edit.
    pub read_only: bool,
}

impl Handler for Server {
    fn handle(&mut self, request: Request, _notify: &mut dyn FnMut(&str, Value)) -> Outcome {
        match request.method.as_str() {
            "initialize" => Outcome::Reply(self.capabilities()),
            // The client's acknowledgement. A notification, so it is answered
            // with silence — replying to it is a protocol violation.
            "notifications/initialized" | "notifications/cancelled" => Outcome::Silent,
            "ping" => Outcome::Reply(json!({})),
            "tools/list" => Outcome::Reply(self.tool_list()),
            "tools/call" => self.call(&request.params),
            "resources/list" => Outcome::Reply(resource_list()),
            "resources/read" => read_resource(&request.params),
            // Declared empty rather than unimplemented: a client that sees the
            // capability and gets a method error logs an error every session.
            "prompts/list" => Outcome::Reply(json!({ "prompts": [] })),
            "resources/templates/list" => Outcome::Reply(json!({ "resourceTemplates": [] })),
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
    fn capabilities(&self) -> Value {
        json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": {
                "tools": { "listChanged": false },
                "resources": { "subscribe": false, "listChanged": false },
            },
            "serverInfo": { "name": "paredit", "version": env!("CARGO_PKG_VERSION") },
            "instructions": if self.read_only {
                "Structure-editing for Lisp source. This server is read-only: every command \
                 that would modify a file is refused. Use paredit_capabilities to discover \
                 the full command surface, then paredit_run to reach a command with no \
                 dedicated tool."
            } else {
                "Structure-editing for Lisp source. Prefer these tools to hand-editing \
                 balanced delimiters. Use paredit_capabilities to discover the full command \
                 surface, then paredit_run to reach a command with no dedicated tool. \
                 Commands that write take an explicit --write flag; without it they print \
                 the result and change nothing."
            },
        })
    }

    fn tool_list(&self) -> Value {
        let listed = tools::TOOLS
            .iter()
            .map(|tool| {
                json!({
                    "name": tool.name,
                    "description": tool.description,
                    "inputSchema": (tool.schema)(),
                    "annotations": {
                        // Under --read-only nothing writes, so saying otherwise
                        // would have a client warn about an operation this
                        // server will refuse.
                        "readOnlyHint": !tool.writes || self.read_only,
                        "destructiveHint": tool.writes && !self.read_only,
                        "idempotentHint": !tool.writes,
                        "openWorldHint": false,
                    },
                })
            })
            .collect::<Vec<_>>();
        json!({ "tools": listed })
    }

    fn call(&self, params: &Value) -> Outcome {
        let Some(name) = params["name"].as_str() else {
            return Outcome::Fail(ResponseError::invalid_params("name is required"));
        };
        let Some(tool) = tools::find(name) else {
            return Outcome::Fail(ResponseError::new(
                error_codes::INVALID_PARAMS,
                format!("unknown tool: {name}"),
            ));
        };
        let arguments = params
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| json!({}));

        let argv = match tools::argv_for(tool, &arguments) {
            Ok(argv) => argv,
            Err(detail) => return Outcome::Fail(ResponseError::invalid_params(detail)),
        };

        // Refused before the process starts, not after. `--read-only` is a
        // promise about the filesystem, and a check that ran afterwards would
        // be a report.
        if self.read_only && tools::writes(&argv) {
            return Outcome::Reply(json!({
                "content": [{
                    "type": "text",
                    "text": format!(
                        "refused: this server was started with --read-only, and `paredit {}` \
                         would modify a file. Re-run the command without its writing flag to \
                         see what it would do.",
                        argv.join(" "),
                    ),
                }],
                "isError": true,
            }));
        }

        match tools::run(&argv) {
            Ok(output) => Outcome::Reply(tools::call_result(&output, tools::writes(&argv))),
            Err(detail) => Outcome::Fail(ResponseError::new(error_codes::INTERNAL_ERROR, detail)),
        }
    }
}

/// The resources a client can read without calling a tool.
///
/// The catalog is a resource rather than only a tool because it is *reference*:
/// a client can attach it to context once instead of spending a tool call on it
/// every session.
fn resource_list() -> Value {
    json!({
        "resources": [
            {
                "uri": "paredit://capabilities",
                "name": "Command catalog",
                "description": "Every command, flag, default, enum value, and the per-dialect \
                                support matrix, generated from the argument parser itself.",
                "mimeType": "application/json",
            },
            {
                "uri": "paredit://capabilities-schema",
                "name": "Command catalog JSON Schema",
                "description": "The draft 2020-12 schema the catalog conforms to.",
                "mimeType": "application/schema+json",
            },
            {
                "uri": "paredit://lint-rules",
                "name": "Lint rule reference",
                "description": "Every lint rule with its category, severity, dialects, \
                                rationale, and whether it can be fixed automatically.",
                "mimeType": "text/markdown",
            },
        ],
    })
}

fn read_resource(params: &Value) -> Outcome {
    let Some(uri) = params["uri"].as_str() else {
        return Outcome::Fail(ResponseError::invalid_params("uri is required"));
    };
    let (argv, mime): (&[&str], &str) = match uri {
        "paredit://capabilities" => (
            &[
                "inspect",
                "capabilities",
                "--output",
                "json",
                "--schema-version",
                "3",
            ],
            "application/json",
        ),
        "paredit://capabilities-schema" => (
            &[
                "inspect",
                "capabilities",
                "--emit",
                "schema",
                "--schema-version",
                "3",
            ],
            "application/schema+json",
        ),
        "paredit://lint-rules" => (&["inspect", "lint", "--docs"], "text/markdown"),
        other => {
            return Outcome::Fail(ResponseError::new(
                error_codes::INVALID_PARAMS,
                format!("unknown resource: {other}"),
            ));
        }
    };

    let argv: Vec<String> = argv.iter().map(|part| (*part).to_owned()).collect();
    match tools::run(&argv) {
        Ok(output) => Outcome::Reply(json!({
            "contents": [{ "uri": uri, "mimeType": mime, "text": output.stdout }],
        })),
        Err(detail) => Outcome::Fail(ResponseError::new(error_codes::INTERNAL_ERROR, detail)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(method: &str, params: Value) -> Request {
        Request {
            method: method.to_owned(),
            params,
            id: Some(json!(1)),
        }
    }

    fn call(server: &mut Server, method: &str, params: Value) -> Value {
        let mut notify = |_: &str, _: Value| {};
        match server.handle(request(method, params), &mut notify) {
            Outcome::Reply(value) => value,
            other => panic!("expected a reply from {method}, got {other:?}"),
        }
    }

    #[test]
    fn initialize_answers_the_version_this_server_implements() {
        let mut server = Server::default();
        // A client asking for a version this server does not speak must be told
        // what it does speak, not have its own version echoed back.
        let result = call(
            &mut server,
            "initialize",
            json!({ "protocolVersion": "2099-01-01" }),
        );
        assert_eq!(result["protocolVersion"], PROTOCOL_VERSION);
        assert!(result["capabilities"]["tools"].is_object(), "{result}");
        assert!(result["capabilities"]["resources"].is_object(), "{result}");
    }

    /// The catalogue is small on purpose. If it grows toward the command count,
    /// the reason for having a `run` tool has been forgotten.
    #[test]
    fn the_tool_catalogue_stays_small_enough_to_choose_from() {
        let mut server = Server::default();
        let listed = call(&mut server, "tools/list", json!({}));
        let count = listed["tools"].as_array().expect("tools").len();
        assert!(
            (1..=12).contains(&count),
            "{count} tools is past what a model should have to choose between; \
             reach the rest through paredit_run"
        );
    }

    #[test]
    fn a_read_only_server_reports_every_tool_as_read_only() {
        let mut server = Server { read_only: true };
        let listed = call(&mut server, "tools/list", json!({}));
        for tool in listed["tools"].as_array().expect("tools") {
            assert_eq!(
                tool["annotations"]["readOnlyHint"], true,
                "{}",
                tool["name"]
            );
            assert_eq!(tool["annotations"]["destructiveHint"], false);
        }
    }

    /// The refusal has to happen before the process starts. A check that ran
    /// afterwards would be a report, not a promise.
    #[test]
    fn a_read_only_server_refuses_a_writing_command_without_running_it() {
        let mut server = Server { read_only: true };
        let result = call(
            &mut server,
            "tools/call",
            json!({
                "name": "paredit_run",
                "arguments": { "args": ["edit", "format", "--file", "a.lisp", "--write"] },
            }),
        );
        assert_eq!(result["isError"], true);
        let text = result["content"][0]["text"].as_str().unwrap_or_default();
        assert!(text.contains("read-only"), "{text}");
    }

    #[test]
    fn an_unknown_tool_is_named_in_the_error() {
        let mut server = Server::default();
        let mut notify = |_: &str, _: Value| {};
        let outcome = server.handle(
            request("tools/call", json!({ "name": "paredit_telepathy" })),
            &mut notify,
        );
        match outcome {
            Outcome::Fail(error) => assert!(error.message.contains("telepathy"), "{error:?}"),
            other => panic!("expected a failure, got {other:?}"),
        }
    }

    #[test]
    fn the_resource_list_offers_the_catalog_and_the_rule_reference() {
        let mut server = Server::default();
        let listed = call(&mut server, "resources/list", json!({}));
        let uris: Vec<&str> = listed["resources"]
            .as_array()
            .expect("resources")
            .iter()
            .filter_map(|resource| resource["uri"].as_str())
            .collect();
        assert!(uris.contains(&"paredit://capabilities"), "{uris:?}");
        assert!(uris.contains(&"paredit://lint-rules"), "{uris:?}");
    }

    /// A client that sees a capability and gets a method error logs one every
    /// session. Declaring the empty list is cheaper than the noise.
    #[test]
    fn prompts_are_declared_empty_rather_than_unimplemented() {
        let mut server = Server::default();
        assert_eq!(
            call(&mut server, "prompts/list", json!({}))["prompts"],
            json!([])
        );
    }
}
