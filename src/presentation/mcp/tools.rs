//! The tools `paredit mcp` offers, and how one is run.
//!
//! ## Why not one tool per command
//!
//! There are 314 commands. Handing a model 314 tool descriptions costs
//! thousands of tokens of context before it has read a line of the user's code,
//! and it makes tool selection *harder*: the difference between
//! `inspect duplicate-cond-tests` and `inspect duplicate-case-keys` is not one
//! a model should be spending attention on when `inspect lint` runs both.
//!
//! So the catalogue below is small and deliberately chosen — the operations an
//! agent reaches for constantly — plus one `run` tool that reaches the other
//! 300, with `paredit://capabilities` as the resource that describes them. That
//! is the same shape the CLI itself has: a few things you remember, and a
//! catalogue for the rest.
//!
//! ## Why a subprocess
//!
//! Each call re-executes this binary. The commands write their results to
//! stdout, and on this server stdout *is* the protocol channel — so running
//! them in-process would need the output redirected, which in a crate that
//! denies `unsafe_code` means threading a writer through 314 command
//! implementations. Re-executing costs a process launch, buys byte-identical
//! behaviour with the CLI for free, and cannot corrupt the channel.

use std::process::Command;

use serde_json::{Value, json};

/// One tool in the catalogue.
pub(super) struct Tool {
    pub name: &'static str,
    pub description: &'static str,
    /// The fixed leading arguments. The tool's own inputs are appended.
    pub argv: &'static [&'static str],
    pub schema: fn() -> Value,
    /// Whether this tool can modify a file. Reported to the client as
    /// `readOnlyHint`, and refused outright under `--read-only`.
    pub writes: bool,
}

/// A JSON Schema for a tool taking one required file path.
fn one_file(description: &str) -> Value {
    json!({
        "type": "object",
        "required": ["path"],
        "additionalProperties": false,
        "properties": { "path": { "type": "string", "description": description } },
    })
}

fn files_schema() -> Value {
    json!({
        "type": "object",
        "required": ["paths"],
        "additionalProperties": false,
        "properties": {
            "paths": {
                "type": "array",
                "items": { "type": "string" },
                "minItems": 1,
                "description": "Files or directories to scan, scanned recursively.",
            },
        },
    })
}

pub(super) const TOOLS: &[Tool] = &[
    Tool {
        name: "paredit_check",
        description: "Check that a Lisp file is a balanced S-expression document. Run before \
                      and after edits made by other means, to catch delimiter mistakes early.",
        argv: &["inspect", "check", "--output", "json", "--file"],
        schema: || one_file("The Lisp source file to validate."),
        writes: false,
    },
    Tool {
        name: "paredit_outline",
        description: "List a file's top-level forms with child-index paths, byte spans, and \
                      definition status. These paths are what `edit`/`refactor` commands \
                      select with — call this first.",
        argv: &["inspect", "outline", "--output", "json", "--file"],
        schema: || one_file("The Lisp source file to outline."),
        writes: false,
    },
    Tool {
        name: "paredit_lint",
        description: "Run every within-file logic-bug lint rule; findings include rule, \
                      severity, category, and fixability. Prefer this over individual rule \
                      commands.",
        argv: &["inspect", "lint", "--output", "json"],
        schema: files_schema,
        writes: false,
    },
    Tool {
        name: "paredit_format",
        description: "Print a file in canonical S-expression layout without writing it, to \
                      check whether a rewrite is needed.",
        argv: &["edit", "format", "--file"],
        schema: || one_file("The Lisp source file to format."),
        writes: false,
    },
    Tool {
        name: "paredit_diff",
        description: "Compare two Lisp documents by parse, not lines: inserted, deleted, or \
                      replaced forms. Whitespace and comments are ignored, so reformatting \
                      alone reports no changes.",
        argv: &["inspect", "diff", "--output", "json"],
        schema: || {
            json!({
                "type": "object",
                "required": ["old", "new"],
                "additionalProperties": false,
                "properties": {
                    "old": { "type": "string", "description": "The document to compare from." },
                    "new": { "type": "string", "description": "The document to compare to." },
                },
            })
        },
        writes: false,
    },
    Tool {
        name: "paredit_capabilities",
        description: "Machine-readable catalog of every command, flag, default, and enum \
                      value, plus per-dialect support. Read before using `paredit_run` for a \
                      command not listed here.",
        argv: &[
            "inspect",
            "capabilities",
            "--output",
            "json",
            "--schema-version",
            "3",
        ],
        schema: || {
            json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {},
            })
        },
        writes: false,
    },
    Tool {
        name: "paredit_run",
        description: "Run any paredit command by its argument vector, e.g. [\"refactor\", \
                      \"rename-at\", \"--file\", \"a.lisp\", \"--at\", \"42\", \"--to\", \
                      \"new-name\"]. See `paredit_capabilities` for commands and flags. \
                      Writing commands are refused under --read-only.",
        argv: &[],
        schema: || {
            json!({
                "type": "object",
                "required": ["args"],
                "additionalProperties": false,
                "properties": {
                    "args": {
                        "type": "array",
                        "items": { "type": "string" },
                        "minItems": 1,
                        "description": "The argument vector, without the leading `paredit`.",
                    },
                },
            })
        },
        writes: true,
    },
];

/// The flags that make a command write.
///
/// Checked by name rather than by asking the command, because the check has to
/// happen *before* the command runs and there is no dry-run that reports "I
/// would have written".
pub(super) const WRITING_FLAGS: &[&str] = &["--write", "--fix", "--apply", "--in-place"];

const VALUE_FLAGS: &[&str] = &[
    "--color",
    "--new-file-mode",
    "--encoding",
    "--timeout-ms",
    "--max-input-bytes",
    "--max-file-bytes",
    "--max-total-bytes",
    "--max-files",
    "--jobs",
];

/// The command paths that write without any flag saying so.
///
/// A flag list was the whole gate until the `fix` namespace existed, and it
/// was sound while every writing command spelled the write as a flag.
/// `paredit fix apply` spells it in the command *name* — which was the point
/// of moving the auto-fixer out from under `inspect lint --fix` — and so
/// carries none of the four flags above. Under `--read-only` it therefore ran,
/// and wrote.
///
/// `refactor create-checkpoint` and `refactor delete-checkpoint` write and
/// delete a checkpoint file the same way, and `config init` writes
/// `paredit.toml` unconditionally unless `--dry-run` is given — none of the
/// three carry a flag from [`WRITING_FLAGS`] either. This list is the
/// command-path-sequence counterpart of
/// [`crate::presentation::cli::capabilities::classify`]'s
/// `WRITE_VERB_COMMANDS`, which independently names all four; the two lists
/// are checked against each other by a contract test so they cannot diverge
/// silently again.
///
/// Matched as a subcommand *sequence* at the front of the argument vector
/// rather than by asking "is this word anywhere in argv": `--rule apply` must
/// not be mistaken for `fix apply`, and neither must a file named `fix`.
const WRITING_COMMANDS: &[&[&str]] = &[
    &["fix", "apply"],
    &["refactor", "create-checkpoint"],
    &["refactor", "delete-checkpoint"],
    &["config", "init"],
];

pub(super) fn find(name: &str) -> Option<&'static Tool> {
    TOOLS.iter().find(|tool| tool.name == name)
}

/// Builds the argument vector for one call.
pub(super) fn argv_for(tool: &Tool, arguments: &Value) -> Result<Vec<String>, String> {
    let mut argv: Vec<String> = tool.argv.iter().map(|part| (*part).to_owned()).collect();
    match tool.name {
        "paredit_run" => {
            let args = arguments["args"]
                .as_array()
                .ok_or_else(|| "args must be an array of strings".to_owned())?;
            if args.is_empty() {
                return Err("args must not be empty".to_owned());
            }
            for arg in args {
                argv.push(
                    arg.as_str()
                        .ok_or_else(|| "args must be an array of strings".to_owned())?
                        .to_owned(),
                );
            }
        }
        "paredit_lint" => {
            let paths = arguments["paths"]
                .as_array()
                .ok_or_else(|| "paths must be an array of strings".to_owned())?;
            if paths.is_empty() {
                return Err("paths must not be empty".to_owned());
            }
            for path in paths {
                argv.push(
                    path.as_str()
                        .ok_or_else(|| "paths must be an array of strings".to_owned())?
                        .to_owned(),
                );
            }
        }
        "paredit_diff" => {
            for key in ["old", "new"] {
                argv.push(
                    arguments[key]
                        .as_str()
                        .ok_or_else(|| format!("{key} must be a string path"))?
                        .to_owned(),
                );
            }
        }
        "paredit_capabilities" => {}
        _ => {
            argv.push(
                arguments["path"]
                    .as_str()
                    .ok_or_else(|| "path must be a string".to_owned())?
                    .to_owned(),
            );
        }
    }
    Ok(argv)
}

/// Whether an argument vector asks for a write.
///
/// Two questions, because a write is spelled two ways: as a flag on a command
/// that can also not write (`edit wrap --write`), and as the command itself
/// (`fix apply`). Missing the second is how `--read-only` stopped being a
/// promise.
pub(super) fn writes(argv: &[String]) -> bool {
    if argv.iter().any(|arg| WRITING_FLAGS.contains(&arg.as_str())) {
        return true;
    }
    WRITING_COMMANDS.iter().any(|path| leads_with(argv, path))
}

/// Whether `argv` begins with `path`, allowing global flags before or within it.
///
/// The leading words, not any words: `paredit inspect lint --explain fix` has
/// `fix` in it and writes nothing.
fn leads_with(argv: &[String], path: &[&str]) -> bool {
    let mut args = argv.iter();

    path.iter().all(|expected| {
        loop {
            let Some(argument) = args.next() else {
                return false;
            };

            if argument == expected {
                return true;
            }

            if !argument.starts_with('-') {
                return false;
            }

            if value_flag(argument) && args.next().is_none() {
                return false;
            }
        }
    })
}

fn value_flag(argument: &str) -> bool {
    let (flag, has_inline_value) = argument
        .split_once('=')
        .map_or((argument, false), |(flag, _)| (flag, true));

    !has_inline_value && VALUE_FLAGS.contains(&flag)
}

/// What running a command produced.
pub(super) struct Output {
    pub stdout: String,
    pub stderr: String,
    pub code: i32,
}

fn lossy_text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// Runs `paredit` with `argv`, as a subprocess of this binary.
pub(super) fn run(argv: &[String]) -> Result<Output, String> {
    let executable = std::env::current_exe()
        .map_err(|error| format!("cannot locate the paredit binary: {error}"))?;
    let output = Command::new(executable)
        .args(argv)
        .output()
        .map_err(|error| format!("cannot run paredit: {error}"))?;
    Ok(Output {
        stdout: lossy_text(&output.stdout),
        stderr: lossy_text(&output.stderr),
        // A signal leaves no code. Reporting -1 rather than 0 keeps a killed
        // command from reading as a successful one.
        code: output.status.code().unwrap_or(-1),
    })
}

/// The MCP `tools/call` result for a finished command.
///
/// `isError` is set from the exit status, and the exit status is meaningful
/// here: 3 is a policy gate failing, which is a *result* and not a malfunction.
/// Reporting it as an error would have a model retry a command that worked.
///
/// `writes` is the caller's already-computed answer to "did this argument
/// vector ask for a write" (see `writes()` below). It is threaded through
/// rather than recomputed here because `paredit_run` is the one tool whose
/// argument vector is not known until the call arrives — for every other tool
/// it is `tool.writes` — and the client cares which one actually happened,
/// not just which one this fixed catalogue entry usually means.
pub(super) fn call_result(output: &Output, writes: bool) -> Value {
    let gate_failure = output.code == paredit_core_cli::gate::GATE_FAILURE_EXIT_CODE;
    let mut text = output.stdout.clone();
    if !output.stderr.is_empty() {
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str(&output.stderr);
    }
    json!({
        "content": [{ "type": "text", "text": text }],
        "isError": output.code != 0 && !gate_failure,
        "structuredContent": {
            "exit_code": output.code,
            "gate_failed": gate_failure,
            "writes": writes,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tool_declares_an_object_schema_with_no_stray_properties() {
        for tool in TOOLS {
            let schema = (tool.schema)();
            assert_eq!(schema["type"], "object", "{}", tool.name);
            assert_eq!(
                schema["additionalProperties"], false,
                "{} must reject unknown arguments, or a typo reads as an omission",
                tool.name
            );
        }
    }

    /// A model reads the description to choose a tool. An empty or terse one
    /// is the difference between the right call and three wrong ones.
    #[test]
    fn every_tool_description_says_enough_to_choose_it() {
        for tool in TOOLS {
            assert!(
                tool.description.len() > 60,
                "{} needs a description a model can select on",
                tool.name
            );
        }
    }

    /// This module's own doc comment argues for minimizing what a model has
    /// to read before it has seen a line of the user's code — that is why
    /// there are 7 tools and a `run` escape hatch rather than 358. The same
    /// argument applies to the 7 descriptions themselves.
    ///
    /// 1,200 is a ceiling with headroom over the 1,114-character total these
    /// 7 descriptions summed to right after trimming them for FR-010
    /// (2026-08-01), not an exact-match snapshot: a future minor wording fix
    /// that stays under it should not have to touch this number, but growth
    /// back toward the untrimmed total (1,377) must fail loudly.
    const TOOL_DESCRIPTION_CHARACTER_CEILING: usize = 1_200;

    #[test]
    fn tool_descriptions_stay_under_their_character_budget() {
        let total: usize = TOOLS.iter().map(|tool| tool.description.len()).sum();
        assert!(
            total <= TOOL_DESCRIPTION_CHARACTER_CEILING,
            "the 7 tool descriptions total {total} characters, over the \
             {TOOL_DESCRIPTION_CHARACTER_CEILING}-character budget; this module's whole \
             design argues for minimizing what a model reads before it sees the user's code"
        );
    }

    #[test]
    fn tool_names_are_unique() {
        let mut names: Vec<&str> = TOOLS.iter().map(|tool| tool.name).collect();
        names.sort_unstable();
        let count = names.len();
        names.dedup();
        assert_eq!(names.len(), count);
    }

    #[test]
    fn a_run_call_appends_its_argument_vector() {
        let tool = find("paredit_run").expect("the run tool");
        let argv = argv_for(
            tool,
            &json!({ "args": ["inspect", "check", "--file", "a.lisp"] }),
        )
        .expect("builds");
        assert_eq!(argv, ["inspect", "check", "--file", "a.lisp"]);
    }

    #[test]
    fn argv_for_rejects_invalid_or_empty_run_arguments() {
        let tool = find("paredit_run").expect("the run tool");
        for arguments in [
            json!({}),
            json!({ "args": null }),
            json!({ "args": [] }),
            json!({ "args": ["inspect", 1] }),
        ] {
            let error = argv_for(tool, &arguments).expect_err("invalid args must be refused");
            assert!(error.contains("args"), "{arguments}: {error}");
        }
    }

    #[test]
    fn argv_for_lint_preserves_paths_and_rejects_invalid_boundaries() {
        let tool = find("paredit_lint").expect("the lint tool");
        let argv = argv_for(tool, &json!({ "paths": ["z.lisp", "a.lisp"] })).expect("builds");
        assert_eq!(&argv[4..], ["z.lisp", "a.lisp"]);

        for arguments in [
            json!({}),
            json!({ "paths": null }),
            json!({ "paths": [] }),
            json!({ "paths": ["a.lisp", false] }),
        ] {
            let error = argv_for(tool, &arguments).expect_err("invalid paths must be refused");
            assert!(error.contains("paths"), "{arguments}: {error}");
        }
    }

    #[test]
    fn argv_for_diff_keeps_old_and_new_in_the_declared_order() {
        let tool = find("paredit_diff").expect("the diff tool");
        let argv =
            argv_for(tool, &json!({ "old": "before.lisp", "new": "after.lisp" })).expect("builds");
        assert_eq!(&argv[4..], ["before.lisp", "after.lisp"]);

        for arguments in [
            json!({}),
            json!({ "old": null, "new": "after.lisp" }),
            json!({ "old": "before.lisp", "new": 1 }),
        ] {
            let error = argv_for(tool, &arguments).expect_err("invalid paths must be refused");
            assert!(
                error.contains("old") || error.contains("new"),
                "{arguments}: {error}"
            );
        }
    }

    #[test]
    fn a_one_file_call_appends_the_path_after_the_fixed_flags() {
        let tool = find("paredit_outline").expect("the outline tool");
        let argv = argv_for(tool, &json!({ "path": "src/a.lisp" })).expect("builds");
        assert_eq!(argv.last().map(String::as_str), Some("src/a.lisp"));
        assert!(argv.contains(&"--output".to_owned()));
    }

    #[test]
    fn a_missing_required_argument_is_named_rather_than_defaulted() {
        let tool = find("paredit_outline").expect("the outline tool");
        let error = argv_for(tool, &json!({})).expect_err("no path");
        assert!(error.contains("path"), "{error}");
    }

    #[test]
    fn a_writing_flag_is_recognised_wherever_it_sits_in_the_vector() {
        assert!(writes(&["edit".into(), "format".into(), "--write".into()]));
        assert!(writes(&["inspect".into(), "lint".into(), "--fix".into()]));
        assert!(!writes(&[
            "inspect".into(),
            "lint".into(),
            "--fix-plan".into()
        ]));
        assert!(!writes(&["inspect".into(), "outline".into()]));
    }

    #[test]
    fn writes_recognises_only_complete_writing_command_paths_with_mixed_flags() {
        assert!(writes(&["fix".into(), "apply".into()]));
        assert!(writes(&[
            "--encoding".into(),
            "utf-8".into(),
            "fix".into(),
            "apply".into()
        ]));
        assert!(writes(&[
            "refactor".into(),
            "--dry-run".into(),
            "create-checkpoint".into(),
        ]));
        assert!(writes(&[
            "--progress".into(),
            "config".into(),
            "init".into(),
        ]));

        for argv in [
            &["apply", "fix"][..],
            &["fix"][..],
            &["fix", "apply-now"][..],
            &["refactor", "delete-checkpoints"][..],
            &["inspect", "lint", "--explain", "fix", "apply"][..],
            &["edit", "format", "--write-to"][..],
        ] {
            let argv = argv.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>();
            assert!(!writes(&argv), "{argv:?} must not be treated as a write");
        }
    }

    /// Exit code 3 is a policy gate reporting what it found. A model told that
    /// was an error retries a command that worked.
    #[test]
    fn a_failed_gate_is_a_result_and_not_a_tool_error() {
        let gated = call_result(
            &Output {
                stdout: "finding_count\t2".to_owned(),
                stderr: "lint-report policy failed".to_owned(),
                code: 3,
            },
            false,
        );
        assert_eq!(gated["isError"], false);
        assert_eq!(gated["structuredContent"]["gate_failed"], true);

        let broken = call_result(
            &Output {
                stdout: String::new(),
                stderr: "Error: no such file".to_owned(),
                code: 1,
            },
            false,
        );
        assert_eq!(broken["isError"], true);
    }

    #[test]
    fn call_result_classifies_exit_codes_and_keeps_write_state() {
        for (code, expected_error, expected_gate, writes) in [
            (0, false, false, false),
            (1, true, false, true),
            (2, true, false, false),
            (
                paredit_core_cli::gate::GATE_FAILURE_EXIT_CODE,
                false,
                true,
                true,
            ),
        ] {
            let result = call_result(
                &Output {
                    stdout: String::new(),
                    stderr: String::new(),
                    code,
                },
                writes,
            );
            assert_eq!(result["isError"], expected_error, "exit code {code}");
            assert_eq!(
                result["structuredContent"],
                json!({
                    "exit_code": code,
                    "gate_failed": expected_gate,
                    "writes": writes,
                }),
                "exit code {code}"
            );
        }
    }

    #[test]
    fn call_result_uses_one_text_content_for_every_stream_combination() {
        for (stdout, stderr, expected_text) in [
            ("", "", ""),
            ("stdout", "", "stdout"),
            ("", "stderr", "stderr"),
            ("stdout", "stderr", "stdout\nstderr"),
        ] {
            let result = call_result(
                &Output {
                    stdout: stdout.to_owned(),
                    stderr: stderr.to_owned(),
                    code: 0,
                },
                false,
            );
            assert_eq!(
                result["content"],
                json!([{ "type": "text", "text": expected_text }]),
                "stdout={stdout:?}, stderr={stderr:?}"
            );
        }
    }

    #[test]
    fn invalid_utf8_from_both_streams_is_reported_as_text() {
        let output = Output {
            stdout: lossy_text(b"stdout\x80"),
            stderr: lossy_text(b"stderr\xff"),
            code: 1,
        };
        let result = call_result(&output, false);
        assert_eq!(
            result["content"][0]["text"],
            "stdout\u{fffd}\nstderr\u{fffd}"
        );
    }

    /// `writes` is threaded straight through from the caller into
    /// `structuredContent`, independent of exit status: what matters is
    /// whether the argument vector asked for a write, not whether the write
    /// succeeded.
    #[test]
    fn call_result_surfaces_whether_the_call_asked_for_a_write() {
        let output = Output {
            stdout: String::new(),
            stderr: String::new(),
            code: 0,
        };
        assert_eq!(
            call_result(&output, false)["structuredContent"]["writes"],
            false
        );
        assert_eq!(
            call_result(&output, true)["structuredContent"]["writes"],
            true
        );
    }

    /// stderr carries the reason a command refused. Dropping it leaves a model
    /// with an empty result and no way to correct itself.
    #[test]
    fn a_failures_message_reaches_the_content() {
        let result = call_result(
            &Output {
                stdout: String::new(),
                stderr: "Error: unsupported dialect".to_owned(),
                code: 1,
            },
            false,
        );
        assert!(
            result["content"][0]["text"]
                .as_str()
                .is_some_and(|text| text.contains("unsupported dialect")),
            "{result}"
        );
    }
}
