//! Running a caller's own check against the tree, and reading a Lisp
//! implementation's answer.
//!
//! Everything else in this workspace decides whether a rewrite is safe by
//! reasoning about the text. That reasoning is good and it is not the same
//! thing as the code still working. Two escapes from that limit live here:
//!
//! - [`VerificationCommand`] runs whatever the caller trusts — a test suite, a
//!   build, a linter — after a write, so a refactor can be rolled back on the
//!   evidence of the project's own checks rather than on the tool's opinion.
//!   This is the mechanism behind "run the tests and undo if they fail", and
//!   also the honest way to reach runtime equivalence checking without wiring
//!   a specific implementation into the tool.
//! - [`sbcl`] parses SBCL's `compile-file` diagnostics into structured
//!   findings, so "the compiler now warns about something it did not warn
//!   about before" is comparable across a refactor.
//!
//! The parser is the part with logic worth testing, and it is tested against
//! recorded output. The runner is deliberately thin: it spawns, waits with an
//! optional budget, and reports. It never interprets an exit status as
//! permission to do anything — the caller decides that.

use std::io::{self, Read};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use thiserror::Error;

/// A command that could not be run at all.
#[derive(Debug, Error)]
pub enum ExternalError {
    #[error("verification command is empty")]
    EmptyCommand,
    #[error("failed to spawn verification command {command:?}: {source}")]
    Spawn {
        command: String,
        #[source]
        source: io::Error,
    },
    #[error("failed to wait for verification command {command:?}: {source}")]
    Wait {
        command: String,
        #[source]
        source: io::Error,
    },
}

/// How a verification command finished.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerificationOutcome {
    pub command: String,
    /// The exit status, or `None` when the process was killed by a signal or
    /// by the deadline.
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub duration_ms: u64,
    pub stdout: String,
    pub stderr: String,
}

impl VerificationOutcome {
    /// Whether the command reported success.
    ///
    /// A timeout is a failure, and so is termination by signal: neither is
    /// evidence that the check passed, and treating "no exit code" as success
    /// would silently disarm the gate.
    #[must_use]
    pub fn passed(&self) -> bool {
        !self.timed_out && self.exit_code == Some(0)
    }

    /// A one-line reason a caller can put in an error.
    #[must_use]
    pub fn failure_reason(&self) -> String {
        if self.timed_out {
            return format!(
                "verification command {:?} exceeded its budget after {}ms",
                self.command, self.duration_ms
            );
        }
        match self.exit_code {
            Some(0) => format!("verification command {:?} succeeded", self.command),
            Some(code) => format!(
                "verification command {:?} exited with status {code}",
                self.command
            ),
            None => format!(
                "verification command {:?} was terminated by a signal",
                self.command
            ),
        }
    }

    #[must_use]
    pub fn to_json(&self) -> Value {
        json!({
            "command": self.command,
            "exit_code": self.exit_code,
            "timed_out": self.timed_out,
            "duration_ms": self.duration_ms,
            "passed": self.passed(),
            "stdout": self.stdout,
            "stderr": self.stderr,
        })
    }
}

/// A caller-supplied check to run after a write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerificationCommand {
    command: String,
    working_directory: Option<PathBuf>,
    budget: Option<Duration>,
}

impl VerificationCommand {
    /// Builds a command from the text a caller typed.
    ///
    /// The text is handed to the platform shell rather than split here. A
    /// project's check is routinely a pipeline or a `&&` chain (`make && cargo
    /// test`), and a half-shell that splits on spaces but does not understand
    /// quoting is worse than either extreme: it silently mis-runs commands
    /// that look fine. Running under the shell is the same contract as
    /// `git bisect run` and is documented as such.
    pub fn new(command: impl Into<String>) -> Result<Self, ExternalError> {
        let command = command.into();
        if command.trim().is_empty() {
            return Err(ExternalError::EmptyCommand);
        }
        Ok(Self {
            command,
            working_directory: None,
            budget: None,
        })
    }

    #[must_use]
    pub fn in_directory(mut self, directory: impl Into<PathBuf>) -> Self {
        self.working_directory = Some(directory.into());
        self
    }

    #[must_use]
    pub const fn with_budget(mut self, budget: Option<Duration>) -> Self {
        self.budget = budget;
        self
    }

    #[must_use]
    pub fn command_text(&self) -> &str {
        &self.command
    }

    /// Runs the command and captures its output.
    ///
    /// Both pipes are drained by their own thread. Polling the child while
    /// leaving its output in the pipe buffer deadlocks the moment the command
    /// prints more than a pipe's worth — which a test suite does within the
    /// first second — so the reader threads are what make the budget usable
    /// rather than an optimisation.
    pub fn run(&self) -> Result<VerificationOutcome, ExternalError> {
        let started = Instant::now();
        let mut builder = shell_command(&self.command);
        if let Some(directory) = &self.working_directory {
            builder.current_dir(directory);
        }
        let mut child = builder
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|source| ExternalError::Spawn {
                command: self.command.clone(),
                source,
            })?;

        let stdout = child.stdout.take().map(drain);
        let stderr = child.stderr.take().map(drain);

        let (status, timed_out) = self.wait_within_budget(&mut child)?;
        let duration = started.elapsed();

        Ok(VerificationOutcome {
            command: self.command.clone(),
            exit_code: status,
            timed_out,
            duration_ms: u64::try_from(duration.as_millis()).unwrap_or(u64::MAX),
            stdout: stdout.map(join_reader).unwrap_or_default(),
            stderr: stderr.map(join_reader).unwrap_or_default(),
        })
    }

    /// Waits for the child, killing it if the budget runs out.
    ///
    /// Returns the exit code (absent when the child was killed or signalled)
    /// and whether the budget is what ended it.
    fn wait_within_budget(&self, child: &mut Child) -> Result<(Option<i32>, bool), ExternalError> {
        let Some(budget) = self.budget else {
            let status = child.wait().map_err(|source| ExternalError::Wait {
                command: self.command.clone(),
                source,
            })?;
            return Ok((status.code(), false));
        };

        let deadline = Instant::now() + budget;
        loop {
            match child.try_wait() {
                Ok(Some(status)) => return Ok((status.code(), false)),
                Ok(None) => {}
                Err(source) => {
                    return Err(ExternalError::Wait {
                        command: self.command.clone(),
                        source,
                    });
                }
            }
            if Instant::now() >= deadline {
                // A kill failure is not fatal: the child may have exited
                // between the poll and the kill, which `wait` below settles.
                let _ = child.kill();
                let _ = child.wait();
                return Ok((None, true));
            }
            thread::sleep(POLL_INTERVAL.min(deadline.saturating_duration_since(Instant::now())));
        }
    }
}

/// How often an armed budget checks on the child.
///
/// Short enough that a timeout is reported promptly, long enough that waiting
/// on a multi-minute test suite costs nothing measurable.
const POLL_INTERVAL: Duration = Duration::from_millis(20);

/// Reads a pipe to end on its own thread.
fn drain(mut source: impl Read + Send + 'static) -> thread::JoinHandle<String> {
    thread::spawn(move || {
        let mut buffer = Vec::new();
        // A read failure means the output is unavailable, not that the command
        // failed; the exit status is the verdict, and losing the transcript
        // must not turn a passing check into an error.
        let _ = source.read_to_end(&mut buffer);
        String::from_utf8_lossy(&buffer).into_owned()
    })
}

/// Collects a drained pipe, tolerating a panicked reader thread.
fn join_reader(handle: thread::JoinHandle<String>) -> String {
    handle.join().unwrap_or_default()
}

#[cfg(unix)]
fn shell_command(command: &str) -> Command {
    let mut builder = Command::new("/bin/sh");
    builder.arg("-c").arg(command);
    builder
}

#[cfg(not(unix))]
fn shell_command(command: &str) -> Command {
    let mut builder = Command::new("cmd");
    builder.arg("/C").arg(command);
    builder
}

/// Reading SBCL's own opinion of a file.
///
/// `compile-file` is the closest thing Common Lisp has to a type checker, and
/// its diagnostics cover exactly the class of mistake a syntactic refactor can
/// introduce: an undefined variable a rename missed, an arity a signature
/// change broke, a `defmethod` with no matching generic. Comparing the set
/// before a refactor with the set after is a stronger safety argument than any
/// analysis in this workspace can make alone.
///
/// The parser is the whole of the logic here and runs without SBCL installed.
pub mod sbcl {
    use super::{ExternalError, VerificationCommand};
    use serde_json::{Value, json};
    use std::path::Path;

    /// How serious SBCL considered a diagnostic.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    pub enum Severity {
        /// A style-warning: legal code SBCL thinks is a mistake.
        Style,
        /// A full warning: almost always a real defect.
        Warning,
        /// An error the compiler recovered from.
        Error,
    }

    impl Severity {
        #[must_use]
        pub const fn label(self) -> &'static str {
            match self {
                Self::Style => "style-warning",
                Self::Warning => "warning",
                Self::Error => "error",
            }
        }

        fn from_caught(text: &str) -> Option<Self> {
            let upper = text.trim().to_ascii_uppercase();
            // Order matters: "STYLE-WARNING" contains "WARNING".
            if upper.contains("STYLE-WARNING") {
                Some(Self::Style)
            } else if upper.contains("ERROR") {
                Some(Self::Error)
            } else if upper.contains("WARNING") {
                Some(Self::Warning)
            } else {
                None
            }
        }
    }

    /// One diagnostic SBCL emitted.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Diagnostic {
        pub severity: Severity,
        /// The file SBCL attributed it to, when it named one.
        pub file: Option<String>,
        /// The enclosing form, as SBCL's `; in:` line spells it.
        pub context: Option<String>,
        pub message: String,
    }

    impl Diagnostic {
        /// A key that identifies the same diagnostic across two compilations.
        ///
        /// Deliberately excludes the file's absolute path, because a
        /// before/after comparison routinely compiles copies in two different
        /// temporary directories, and including the path would report every
        /// diagnostic as both removed and added.
        #[must_use]
        pub fn identity(&self) -> String {
            format!(
                "{}|{}|{}",
                self.severity.label(),
                self.context.as_deref().unwrap_or(""),
                self.message
            )
        }

        #[must_use]
        pub fn to_json(&self) -> Value {
            json!({
                "severity": self.severity.label(),
                "file": self.file,
                "context": self.context,
                "message": self.message,
            })
        }
    }

    /// Parses the diagnostics out of an SBCL compilation transcript.
    ///
    /// SBCL writes its diagnostics as a comment block: a `; file:` line, a
    /// `; in:` line naming the enclosing form, the source excerpt, then
    /// `; caught <CONDITION>:` followed by the indented message. The `file:`
    /// and `in:` lines persist across the diagnostics that follow them, which
    /// is why they are tracked as state rather than matched per finding.
    ///
    /// Anything that is not recognised is dropped rather than guessed at. A
    /// missed diagnostic understates the comparison; a fabricated one would
    /// block a correct refactor.
    #[must_use]
    pub fn parse_diagnostics(transcript: &str) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let mut current_file: Option<String> = None;
        let mut current_context: Option<String> = None;
        let mut pending: Option<(Severity, Vec<String>)> = None;

        for line in transcript.lines() {
            let content = strip_comment_prefix(line);

            if let Some(rest) = content.strip_prefix("file:") {
                flush(
                    &mut pending,
                    &current_file,
                    &current_context,
                    &mut diagnostics,
                );
                current_file = Some(rest.trim().to_owned());
                current_context = None;
                continue;
            }
            if let Some(rest) = content.strip_prefix("in:") {
                flush(
                    &mut pending,
                    &current_file,
                    &current_context,
                    &mut diagnostics,
                );
                current_context = Some(rest.trim().to_owned());
                continue;
            }
            if let Some(rest) = content.strip_prefix("caught ") {
                flush(
                    &mut pending,
                    &current_file,
                    &current_context,
                    &mut diagnostics,
                );
                if let Some(severity) = Severity::from_caught(rest) {
                    pending = Some((severity, Vec::new()));
                }
                continue;
            }
            if content.starts_with("compilation unit") || content.starts_with("wrote ") {
                flush(
                    &mut pending,
                    &current_file,
                    &current_context,
                    &mut diagnostics,
                );
                continue;
            }

            // The message body is the indented run after `caught`; a line that
            // is not indented has left the diagnostic.
            if let Some((_, message)) = pending.as_mut() {
                if is_message_continuation(line) {
                    let trimmed = content.trim();
                    if !trimmed.is_empty() {
                        message.push(trimmed.to_owned());
                    }
                } else {
                    flush(
                        &mut pending,
                        &current_file,
                        &current_context,
                        &mut diagnostics,
                    );
                }
            }
        }
        flush(
            &mut pending,
            &current_file,
            &current_context,
            &mut diagnostics,
        );
        diagnostics
    }

    fn flush(
        pending: &mut Option<(Severity, Vec<String>)>,
        file: &Option<String>,
        context: &Option<String>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let Some((severity, message)) = pending.take() else {
            return;
        };
        if message.is_empty() {
            return;
        }
        diagnostics.push(Diagnostic {
            severity,
            file: file.clone(),
            context: context.clone(),
            message: message.join(" "),
        });
    }

    fn strip_comment_prefix(line: &str) -> &str {
        line.trim_start()
            .strip_prefix(';')
            .map_or(line, str::trim_start)
    }

    /// Whether a raw line continues a diagnostic's message.
    ///
    /// SBCL indents the message under `caught`, as `;   text`. The source
    /// excerpt it prints before a diagnostic uses the same shape, but always
    /// arrives *before* `caught`, so tracking only what follows is enough.
    fn is_message_continuation(line: &str) -> bool {
        line.trim_start()
            .strip_prefix(';')
            .is_some_and(|rest| rest.starts_with("  ") || rest.trim().is_empty())
    }

    /// The diagnostics present after a change but not before it.
    ///
    /// This is the comparison a refactor cares about. Diagnostics that
    /// disappear are reported separately rather than netted off, because a
    /// refactor that silences a warning by deleting the code that raised it is
    /// not the same as one that leaves the warning set unchanged.
    #[must_use]
    pub fn introduced<'a>(before: &[Diagnostic], after: &'a [Diagnostic]) -> Vec<&'a Diagnostic> {
        let known = before
            .iter()
            .map(Diagnostic::identity)
            .collect::<std::collections::BTreeSet<_>>();
        after
            .iter()
            .filter(|diagnostic| !known.contains(&diagnostic.identity()))
            .collect()
    }

    /// The diagnostics present before a change but not after it.
    #[must_use]
    pub fn resolved<'a>(before: &'a [Diagnostic], after: &[Diagnostic]) -> Vec<&'a Diagnostic> {
        introduced(after, before)
    }

    /// Builds the command that compiles one file and prints its diagnostics.
    ///
    /// `--non-interactive` matters: without it a failed compilation drops into
    /// the debugger and the process never exits. The compilation output goes
    /// to a temporary fasl the caller supplies, so the source tree stays
    /// untouched by a check that is supposed to be read-only.
    pub fn compile_file_command(
        program: &str,
        source: &Path,
        fasl_output: &Path,
    ) -> Result<VerificationCommand, ExternalError> {
        VerificationCommand::new(format!(
            "{program} --non-interactive --no-userinit --no-sysinit \
             --eval '(compile-file {} :output-file {})'",
            lisp_string(&source.display().to_string()),
            lisp_string(&fasl_output.display().to_string()),
        ))
    }

    /// Quotes a path as a Common Lisp string literal.
    fn lisp_string(value: &str) -> String {
        let mut quoted = String::with_capacity(value.len() + 2);
        quoted.push('"');
        for character in value.chars() {
            if character == '"' || character == '\\' {
                quoted.push('\\');
            }
            quoted.push(character);
        }
        quoted.push('"');
        quoted
    }

    #[cfg(test)]
    mod tests {
        use super::{Severity, introduced, lisp_string, parse_diagnostics, resolved};

        /// Recorded from `sbcl --non-interactive --eval '(compile-file ...)'`
        /// on a file with one undefined variable and one unused argument.
        const TRANSCRIPT: &str = "\
; compiling file \"/tmp/demo.lisp\" (written 01 JAN 2026):
; compiling (DEFUN BAR ...)
; file: /tmp/demo.lisp
; in: DEFUN BAR
;     (+ MISSING 1)
;
; caught WARNING:
;   undefined variable: COMMON-LISP-USER::MISSING
;
; in: DEFUN BAZ
;     (DEFUN BAZ (UNUSED) 1)
;
; caught STYLE-WARNING:
;   The variable UNUSED is defined but never used.
;
; wrote /tmp/demo.fasl
; compilation finished in 0:00:00.011
";

        #[test]
        fn a_transcript_yields_one_diagnostic_per_caught_block() {
            let diagnostics = parse_diagnostics(TRANSCRIPT);

            assert_eq!(diagnostics.len(), 2, "parsed: {diagnostics:#?}");
            assert_eq!(diagnostics[0].severity, Severity::Warning);
            assert_eq!(diagnostics[0].file.as_deref(), Some("/tmp/demo.lisp"));
            assert_eq!(diagnostics[0].context.as_deref(), Some("DEFUN BAR"));
            assert_eq!(
                diagnostics[0].message,
                "undefined variable: COMMON-LISP-USER::MISSING"
            );
        }

        /// `STYLE-WARNING` contains `WARNING`; classifying it as the latter
        /// would make every unused variable look like a real defect.
        #[test]
        fn a_style_warning_is_not_classified_as_a_warning() {
            let diagnostics = parse_diagnostics(TRANSCRIPT);

            assert_eq!(diagnostics[1].severity, Severity::Style);
            assert_eq!(diagnostics[1].context.as_deref(), Some("DEFUN BAZ"));
            assert_eq!(
                diagnostics[1].message,
                "The variable UNUSED is defined but never used."
            );
        }

        #[test]
        fn an_in_line_updates_the_context_for_the_diagnostics_that_follow() {
            let diagnostics = parse_diagnostics(TRANSCRIPT);
            assert_eq!(diagnostics[0].context.as_deref(), Some("DEFUN BAR"));
            assert_eq!(diagnostics[1].context.as_deref(), Some("DEFUN BAZ"));
            // The `file:` line is stated once and applies to both.
            assert_eq!(diagnostics[1].file.as_deref(), Some("/tmp/demo.lisp"));
        }

        #[test]
        fn a_multi_line_message_is_joined() {
            let diagnostics = parse_diagnostics(
                "; caught ERROR:\n\
                 ;   READ error during COMPILE-FILE:\n\
                 ;     unmatched close parenthesis\n",
            );

            assert_eq!(diagnostics.len(), 1);
            assert_eq!(diagnostics[0].severity, Severity::Error);
            assert_eq!(
                diagnostics[0].message,
                "READ error during COMPILE-FILE: unmatched close parenthesis"
            );
        }

        #[test]
        fn a_clean_compilation_yields_nothing() {
            assert!(
                parse_diagnostics(
                    "; compiling file \"/tmp/ok.lisp\"\n; wrote /tmp/ok.fasl\n\
                     ; compilation finished in 0:00:00.004\n"
                )
                .is_empty()
            );
        }

        #[test]
        fn unrecognised_output_is_dropped_rather_than_guessed_at() {
            assert!(parse_diagnostics("This is not an SBCL transcript at all.\n").is_empty());
            // A `caught` line with no message body is not a diagnostic.
            assert!(parse_diagnostics("; caught WARNING:\n").is_empty());
        }

        #[test]
        fn a_summary_line_ends_the_diagnostic_it_follows() {
            let diagnostics = parse_diagnostics(
                "; caught WARNING:\n\
                 ;   undefined function: FOO\n\
                 ; compilation unit finished\n\
                 ;   caught 1 WARNING condition\n",
            );

            assert_eq!(diagnostics.len(), 1);
            assert_eq!(diagnostics[0].message, "undefined function: FOO");
        }

        #[test]
        fn comparison_reports_each_direction_separately() {
            let before = parse_diagnostics(
                "; in: DEFUN A\n; caught WARNING:\n;   undefined variable: X\n\
                 ; in: DEFUN B\n; caught STYLE-WARNING:\n;   unused variable Y\n",
            );
            let after = parse_diagnostics(
                "; in: DEFUN B\n; caught STYLE-WARNING:\n;   unused variable Y\n\
                 ; in: DEFUN C\n; caught WARNING:\n;   undefined function: Z\n",
            );

            let new = introduced(&before, &after);
            assert_eq!(new.len(), 1);
            assert_eq!(new[0].message, "undefined function: Z");

            let gone = resolved(&before, &after);
            assert_eq!(gone.len(), 1);
            assert_eq!(gone[0].message, "undefined variable: X");
        }

        /// A diagnostic's identity must survive being compiled from a copy in
        /// a different directory, or every before/after run reports churn.
        #[test]
        fn identity_ignores_the_file_it_came_from() {
            let before = parse_diagnostics(
                "; file: /tmp/before/a.lisp\n; in: DEFUN A\n\
                 ; caught WARNING:\n;   undefined variable: X\n",
            );
            let after = parse_diagnostics(
                "; file: /tmp/after/a.lisp\n; in: DEFUN A\n\
                 ; caught WARNING:\n;   undefined variable: X\n",
            );

            assert!(introduced(&before, &after).is_empty());
            assert!(resolved(&before, &after).is_empty());
        }

        #[test]
        fn a_path_with_a_quote_is_escaped_into_the_lisp_form() {
            assert_eq!(lisp_string(r#"/tmp/od"d\path"#), r#""/tmp/od\"d\\path""#);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ExternalError, VerificationCommand};
    #[cfg(unix)]
    use std::time::Duration;

    #[test]
    fn an_empty_command_is_refused_before_spawning() {
        assert!(matches!(
            VerificationCommand::new("   "),
            Err(ExternalError::EmptyCommand)
        ));
    }

    #[cfg(unix)]
    #[test]
    fn a_succeeding_command_passes_and_captures_stdout() {
        let outcome = VerificationCommand::new("printf 'ok'")
            .expect("build command")
            .run()
            .expect("run command");

        assert!(outcome.passed());
        assert_eq!(outcome.exit_code, Some(0));
        assert_eq!(outcome.stdout, "ok");
        assert!(!outcome.timed_out);
    }

    #[cfg(unix)]
    #[test]
    fn a_failing_command_reports_its_status_and_stderr() {
        let outcome = VerificationCommand::new("printf 'boom' >&2; exit 3")
            .expect("build command")
            .run()
            .expect("run command");

        assert!(!outcome.passed());
        assert_eq!(outcome.exit_code, Some(3));
        assert_eq!(outcome.stderr, "boom");
        assert!(
            outcome.failure_reason().contains("exited with status 3"),
            "unexpected reason: {}",
            outcome.failure_reason()
        );
    }

    /// A budget that expires must kill the child and report a timeout, not a
    /// zero exit status. Reporting success here would let a hung test suite
    /// bless a broken refactor.
    #[cfg(unix)]
    #[test]
    fn a_command_that_outlives_its_budget_is_killed_and_reported() {
        let outcome = VerificationCommand::new("sleep 30")
            .expect("build command")
            .with_budget(Some(Duration::from_millis(50)))
            .run()
            .expect("run command");

        assert!(outcome.timed_out);
        assert!(!outcome.passed());
        assert_eq!(outcome.exit_code, None);
        assert!(
            outcome.duration_ms < 10_000,
            "the child should have been killed promptly, took {}ms",
            outcome.duration_ms
        );
        assert!(
            outcome.failure_reason().contains("exceeded its budget"),
            "unexpected reason: {}",
            outcome.failure_reason()
        );
    }

    /// A command that finishes inside its budget must not be reported as timed
    /// out just because a budget was armed.
    #[cfg(unix)]
    #[test]
    fn a_budget_that_is_not_reached_leaves_the_outcome_alone() {
        let outcome = VerificationCommand::new("printf 'quick'")
            .expect("build command")
            .with_budget(Some(Duration::from_secs(30)))
            .run()
            .expect("run command");

        assert!(outcome.passed());
        assert!(!outcome.timed_out);
        assert_eq!(outcome.stdout, "quick");
    }

    /// The reader threads exist for this: a command whose output exceeds the
    /// pipe buffer deadlocks any implementation that polls without draining.
    #[cfg(unix)]
    #[test]
    fn output_larger_than_a_pipe_buffer_does_not_deadlock() {
        let outcome = VerificationCommand::new("head -c 400000 /dev/zero | tr '\\0' 'x'")
            .expect("build command")
            .with_budget(Some(Duration::from_secs(60)))
            .run()
            .expect("run command");

        assert!(outcome.passed());
        assert_eq!(outcome.stdout.len(), 400_000);
    }

    #[cfg(unix)]
    #[test]
    fn a_working_directory_is_honoured() {
        let outcome = VerificationCommand::new("pwd")
            .expect("build command")
            .in_directory("/")
            .run()
            .expect("run command");

        assert!(outcome.passed());
        assert_eq!(outcome.stdout.trim(), "/");
    }

    #[cfg(unix)]
    #[test]
    fn a_shell_pipeline_runs_as_written() {
        let outcome = VerificationCommand::new("true && printf 'chained'")
            .expect("build command")
            .run()
            .expect("run command");

        assert!(outcome.passed());
        assert_eq!(outcome.stdout, "chained");
    }
}
