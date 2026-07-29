//! Running an implementation over one file, and comparing runs.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use paredit_core_safety::external::sbcl::{self, Diagnostic};
use paredit_core_safety::external::{ExternalError, VerificationOutcome};
use serde_json::{Value, json};

use super::domain::Implementation;

/// The version of the baseline file this build reads and writes.
pub const BASELINE_SCHEMA_VERSION: u64 = 1;

/// A recorded set of diagnostics, keyed by the identity that survives being
/// compiled from a different directory.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Baseline {
    identities: BTreeSet<String>,
}

impl Baseline {
    #[must_use]
    pub fn from_diagnostics<'a>(diagnostics: impl IntoIterator<Item = &'a Diagnostic>) -> Self {
        Self {
            identities: diagnostics.into_iter().map(Diagnostic::identity).collect(),
        }
    }

    /// Whether this diagnostic was already present.
    #[must_use]
    pub fn contains(&self, diagnostic: &Diagnostic) -> bool {
        self.identities.contains(&diagnostic.identity())
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.identities.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.identities.is_empty()
    }

    #[must_use]
    pub fn to_json(&self) -> Value {
        json!({
            "schema_version": BASELINE_SCHEMA_VERSION,
            "diagnostic_count": self.identities.len(),
            "diagnostics": self.identities.iter().collect::<Vec<_>>(),
        })
    }

    /// Reads a baseline, refusing a schema this build does not understand.
    pub fn from_json(value: &Value) -> Result<Self, String> {
        let version = value
            .get("schema_version")
            .and_then(Value::as_u64)
            .ok_or_else(|| "baseline is missing schema_version".to_owned())?;
        if version != BASELINE_SCHEMA_VERSION {
            return Err(format!(
                "baseline has schema version {version}, expected {BASELINE_SCHEMA_VERSION}"
            ));
        }
        let diagnostics = value
            .get("diagnostics")
            .and_then(Value::as_array)
            .ok_or_else(|| "baseline field diagnostics must be an array".to_owned())?;
        Ok(Self {
            identities: diagnostics
                .iter()
                .filter_map(|entry| entry.as_str().map(str::to_owned))
                .collect(),
        })
    }
}

/// What one file's compilation produced.
#[derive(Debug)]
pub struct CompilationOutcome {
    pub diagnostics: Vec<Diagnostic>,
    /// The implementation's exit status, for a caller that wants to
    /// distinguish "compiled with warnings" from "did not compile".
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    /// Present when the transcript could not be attributed to diagnostics at
    /// all, so a caller can see what actually came back.
    pub unparsed_transcript: Option<String>,
}

/// Compiles one file and reads the implementation's diagnostics.
///
/// The fasl goes into `scratch`, which the caller owns and removes. Compiling
/// into the source tree would make an `inspect` command write files, and
/// compiling into a shared directory would make two concurrent runs collide.
pub fn compile_and_read(
    implementation: Implementation,
    program: &str,
    source: &Path,
    scratch: &Path,
    budget: Option<Duration>,
) -> Result<CompilationOutcome, ExternalError> {
    let fasl = scratch.join(format!(
        "{}.fasl",
        source
            .file_name()
            .map_or_else(|| "source".into(), |name| name.to_string_lossy())
    ));

    let command = match implementation {
        Implementation::Sbcl => sbcl::compile_file_command(program, source, &fasl)?,
    };
    let outcome = command.with_budget(budget).run()?;

    Ok(read_outcome(&outcome))
}

/// Turns a finished run into diagnostics.
///
/// Split out so the transcript handling is testable without an implementation
/// installed, which is the whole reason the parser lives in core.
#[must_use]
pub fn read_outcome(outcome: &VerificationOutcome) -> CompilationOutcome {
    // SBCL writes its compiler notes to standard error and its progress to
    // standard output, but a `--eval` form's output ordering varies with
    // buffering, so both are read.
    let mut diagnostics = sbcl::parse_diagnostics(&outcome.stderr);
    diagnostics.extend(sbcl::parse_diagnostics(&outcome.stdout));

    // A failed run that produced no diagnostic is the case worth surfacing:
    // it means the implementation refused for a reason the parser could not
    // attribute, and silently reporting "no findings" would read as success.
    let unparsed_transcript = (!outcome.passed() && diagnostics.is_empty()).then(|| {
        let mut transcript = outcome.stderr.trim().to_owned();
        if transcript.is_empty() {
            transcript = outcome.stdout.trim().to_owned();
        }
        transcript
    });

    CompilationOutcome {
        diagnostics,
        exit_code: outcome.exit_code,
        timed_out: outcome.timed_out,
        unparsed_transcript,
    }
}

/// A scratch directory unique to this process and file index.
#[must_use]
pub fn scratch_directory(index: usize) -> PathBuf {
    std::env::temp_dir().join(format!("paredit-external-{}-{index}", std::process::id()))
}

#[cfg(test)]
mod tests {
    use super::{Baseline, read_outcome};
    use paredit_core_safety::external::VerificationOutcome;
    use paredit_core_safety::external::sbcl::parse_diagnostics;

    fn outcome(stdout: &str, stderr: &str, exit_code: Option<i32>) -> VerificationOutcome {
        VerificationOutcome {
            command: "sbcl".to_owned(),
            exit_code,
            timed_out: false,
            duration_ms: 1,
            stdout: stdout.to_owned(),
            stderr: stderr.to_owned(),
        }
    }

    const TRANSCRIPT: &str = "; in: DEFUN BAR\n; caught WARNING:\n;   undefined variable: X\n";

    #[test]
    fn diagnostics_are_read_from_either_stream() {
        assert_eq!(
            read_outcome(&outcome(TRANSCRIPT, "", Some(0)))
                .diagnostics
                .len(),
            1
        );
        assert_eq!(
            read_outcome(&outcome("", TRANSCRIPT, Some(0)))
                .diagnostics
                .len(),
            1
        );
    }

    /// The case that would otherwise read as a clean file: the implementation
    /// failed and said something the parser did not recognise.
    #[test]
    fn a_failed_run_with_no_diagnostic_keeps_the_transcript() {
        let read = read_outcome(&outcome("", "sbcl: fatal error, heap exhausted\n", Some(1)));

        assert!(read.diagnostics.is_empty());
        assert_eq!(
            read.unparsed_transcript.as_deref(),
            Some("sbcl: fatal error, heap exhausted")
        );
    }

    #[test]
    fn a_successful_run_keeps_no_transcript() {
        assert!(
            read_outcome(&outcome("", TRANSCRIPT, Some(0)))
                .unparsed_transcript
                .is_none()
        );
    }

    #[test]
    fn a_baseline_round_trips_and_recognises_its_members() {
        let diagnostics = parse_diagnostics(TRANSCRIPT);
        let baseline = Baseline::from_diagnostics(&diagnostics);

        assert_eq!(baseline.len(), 1);
        assert!(!baseline.is_empty());
        assert!(baseline.contains(&diagnostics[0]));

        let parsed = Baseline::from_json(&baseline.to_json()).expect("parse baseline");
        assert_eq!(parsed, baseline);
    }

    #[test]
    fn an_empty_baseline_recognises_nothing() {
        let diagnostics = parse_diagnostics(TRANSCRIPT);
        assert!(!Baseline::default().contains(&diagnostics[0]));
    }

    #[test]
    fn a_future_baseline_schema_is_refused() {
        let mut value = Baseline::default().to_json();
        value["schema_version"] = serde_json::json!(99);

        assert_eq!(
            Baseline::from_json(&value),
            Err("baseline has schema version 99, expected 1".to_owned())
        );
    }

    /// A baseline is compared by diagnostic identity, which excludes the file
    /// path — so a before/after pair compiled in two temporary directories
    /// does not report every diagnostic as both removed and added.
    #[test]
    fn a_baseline_matches_across_directories() {
        let before = parse_diagnostics(
            "; file: /tmp/a/x.lisp\n; in: DEFUN BAR\n; caught WARNING:\n;   undefined variable: X\n",
        );
        let after = parse_diagnostics(
            "; file: /tmp/b/x.lisp\n; in: DEFUN BAR\n; caught WARNING:\n;   undefined variable: X\n",
        );

        assert!(Baseline::from_diagnostics(&before).contains(&after[0]));
    }
}
