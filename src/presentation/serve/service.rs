//! The methods `paredit serve` answers, and the cache that makes it worth
//! running.
//!
//! A CLI invocation re-reads and re-parses a file every time. Over a repository
//! and a loop that is the dominant cost, and it is entirely repeated work: the
//! file did not change between the outline call and the lint call. This server
//! exists to do that work once.
//!
//! The cache is keyed on the file's modification time and length, not on its
//! contents. Hashing the contents would mean reading the file — which is most
//! of what the cache is avoiding — while `stat` is one syscall. The tradeoff is
//! that a rewrite which preserves both the length and the timestamp goes
//! unnoticed; `paredit/invalidate` exists for the caller who has just done one.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde_json::{Value, json};

use paredit_core_jsonrpc::{Outcome, ResponseError, error_codes};
use paredit_core_syntax::sexpr::SyntaxTree;

use crate::lint::report::{
    LintPassRequest, RuleFilter, resolve_active_rules, rule_severity, run_lint_pass,
};
use paredit_core_syntax::dialect::Dialect;

/// What a file looked like when it was last analyzed.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Stamp {
    modified: Option<SystemTime>,
    length: u64,
}

#[derive(Debug, Clone)]
struct Entry {
    stamp: Stamp,
    dialect: Dialect,
    /// `Err` holds the parse failure, which is as worth caching as a success:
    /// a caller polling a file that does not parse would otherwise re-read it
    /// every call.
    analysis: Result<Analysis, String>,
}

#[derive(Debug, Clone)]
struct Analysis {
    outline: Vec<Value>,
    findings: Vec<Value>,
}

#[derive(Debug, Default)]
pub(super) struct Cache {
    entries: BTreeMap<PathBuf, Entry>,
    hits: u64,
    misses: u64,
}

/// What the cache has done so far, for the two callers that report it:
/// `paredit/cache` and `GET /metrics`.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct Counters {
    pub entries: usize,
    pub hits: u64,
    pub misses: u64,
}

impl Cache {
    fn stamp(path: &Path) -> Option<Stamp> {
        let metadata = std::fs::metadata(path).ok()?;
        Some(Stamp {
            modified: metadata.modified().ok(),
            length: metadata.len(),
        })
    }

    /// The analysis of `path`, computed once per (mtime, length).
    fn entry(&mut self, path: &Path) -> Result<Entry, String> {
        let stamp = Self::stamp(path).ok_or_else(|| format!("cannot read {}", path.display()))?;
        if let Some(cached) = self.entries.get(path) {
            if cached.stamp == stamp {
                self.hits += 1;
                return Ok(cached.clone());
            }
        }
        self.misses += 1;

        let text = std::fs::read_to_string(path)
            .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
        let dialect = Dialect::detect_in_source(Some(path), None, &text);
        let analysis = analyze(path, &text, dialect);
        let entry = Entry {
            stamp,
            dialect,
            analysis,
        };
        self.entries.insert(path.to_owned(), entry.clone());
        Ok(entry)
    }

    fn invalidate(&mut self, path: Option<&Path>) -> usize {
        match path {
            Some(path) => usize::from(self.entries.remove(path).is_some()),
            None => {
                let count = self.entries.len();
                self.entries.clear();
                count
            }
        }
    }

    pub(super) fn counters(&self) -> Counters {
        Counters {
            entries: self.entries.len(),
            hits: self.hits,
            misses: self.misses,
        }
    }

    fn stats(&self) -> Value {
        let counters = self.counters();
        json!({
            "entries": counters.entries,
            "hits": counters.hits,
            "misses": counters.misses,
        })
    }
}

fn analyze(path: &Path, text: &str, dialect: Dialect) -> Result<Analysis, String> {
    let tree = SyntaxTree::parse_with_dialect(text, dialect).map_err(|error| error.to_string())?;
    let outline = tree
        .outline(|head| dialect.is_definition_head(head))
        .into_iter()
        .map(|entry| {
            json!({
                "path": entry.path.to_string(),
                "head": entry.head,
                "definition_like": entry.definition_like,
                "span": {
                    "start": entry.span.start().get(),
                    "end": entry.span.end().get(),
                },
            })
        })
        .collect();

    let active = resolve_active_rules(&RuleFilter::default(), &[]).unwrap_or_default();
    let findings = run_lint_pass(
        path,
        dialect,
        &tree,
        text,
        LintPassRequest {
            active: &active,
            settings: None,
            measure: false,
        },
    )
    .map_err(|error| error.to_string())?
    .findings
    .into_iter()
    .filter(|finding| active.contains(&finding.rule))
    .map(|finding| {
        json!({
            "rule": finding.rule,
            "severity": rule_severity(finding.rule).as_str(),
            "message": finding.message,
            "span": {
                "start": finding.span.start().get(),
                "end": finding.span.end().get(),
            },
        })
    })
    .collect();

    Ok(Analysis { outline, findings })
}

/// Answers one JSON-RPC call.
pub(super) fn dispatch(cache: &mut Cache, method: &str, params: &Value) -> Outcome {
    match method {
        "paredit/version" => Outcome::Reply(json!({
            "name": "paredit",
            "version": env!("CARGO_PKG_VERSION"),
        })),
        "paredit/analyze" => analyze_method(cache, params),
        "paredit/outline" => project(cache, params, |analysis| json!(analysis.outline)),
        "paredit/lint" => project(cache, params, |analysis| json!(analysis.findings)),
        "paredit/invalidate" => {
            let path = params["path"].as_str().map(PathBuf::from);
            let cleared = cache.invalidate(path.as_deref());
            Outcome::Reply(json!({ "cleared": cleared }))
        }
        "paredit/cache" => Outcome::Reply(cache.stats()),
        other => Outcome::Fail(ResponseError::method_not_found(other)),
    }
}

fn path_of(params: &Value) -> Result<PathBuf, ResponseError> {
    params["path"]
        .as_str()
        .map(PathBuf::from)
        .ok_or_else(|| ResponseError::invalid_params("path is required"))
}

fn analyze_method(cache: &mut Cache, params: &Value) -> Outcome {
    let path = match path_of(params) {
        Ok(path) => path,
        Err(error) => return Outcome::Fail(error),
    };
    match cache.entry(&path) {
        Err(detail) => Outcome::Fail(ResponseError::new(error_codes::INVALID_PARAMS, detail)),
        Ok(entry) => Outcome::Reply(match &entry.analysis {
            Ok(analysis) => json!({
                "path": path.display().to_string(),
                "dialect": entry.dialect.label(),
                "parsed": true,
                "outline": analysis.outline,
                "findings": analysis.findings,
            }),
            // A parse failure is a result, not a transport error: the caller
            // asked what this file looks like, and "it does not parse, here is
            // why" is the answer.
            Err(error) => json!({
                "path": path.display().to_string(),
                "dialect": entry.dialect.label(),
                "parsed": false,
                "error": error,
            }),
        }),
    }
}

fn project(cache: &mut Cache, params: &Value, pick: fn(&Analysis) -> Value) -> Outcome {
    let path = match path_of(params) {
        Ok(path) => path,
        Err(error) => return Outcome::Fail(error),
    };
    match cache.entry(&path) {
        Err(detail) => Outcome::Fail(ResponseError::new(error_codes::INVALID_PARAMS, detail)),
        Ok(entry) => match &entry.analysis {
            Ok(analysis) => Outcome::Reply(pick(analysis)),
            Err(error) => Outcome::Fail(ResponseError::new(
                error_codes::REQUEST_FAILED,
                error.clone(),
            )),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    fn temp_file(name: &str, source: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("paredit-serve-{name}"));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let path = dir.join("core.lisp");
        let mut file = std::fs::File::create(&path).expect("create fixture");
        file.write_all(source.as_bytes()).expect("write fixture");
        path
    }

    fn reply(outcome: Outcome) -> Value {
        match outcome {
            Outcome::Reply(value) => value,
            other => panic!("expected a reply, got {other:?}"),
        }
    }

    /// The reason this server exists: the second call for an unchanged file
    /// does no work.
    #[test]
    fn a_second_call_for_an_unchanged_file_is_a_cache_hit() {
        let path = temp_file("hit", "(defun f (x) (if (not x) 1 2))\n");
        let params = json!({ "path": path.display().to_string() });
        let mut cache = Cache::default();

        reply(dispatch(&mut cache, "paredit/analyze", &params));
        reply(dispatch(&mut cache, "paredit/outline", &params));
        reply(dispatch(&mut cache, "paredit/lint", &params));

        let stats = reply(dispatch(&mut cache, "paredit/cache", &json!({})));
        assert_eq!(stats["misses"], 1, "{stats}");
        assert_eq!(stats["hits"], 2, "{stats}");
    }

    /// The cache is keyed on a stamp, and a caller who has just rewritten a
    /// file in a way the stamp cannot see needs a way to say so.
    #[test]
    fn invalidation_forces_the_next_call_to_re_read() {
        let path = temp_file("invalidate", "(defun f () 1)\n");
        let params = json!({ "path": path.display().to_string() });
        let mut cache = Cache::default();

        reply(dispatch(&mut cache, "paredit/analyze", &params));
        let cleared = reply(dispatch(&mut cache, "paredit/invalidate", &params));
        assert_eq!(cleared["cleared"], 1);

        reply(dispatch(&mut cache, "paredit/analyze", &params));
        let stats = reply(dispatch(&mut cache, "paredit/cache", &json!({})));
        assert_eq!(stats["misses"], 2, "{stats}");
        assert_eq!(stats["hits"], 0, "{stats}");
    }

    /// "It does not parse, and here is why" is the answer to the question that
    /// was asked, not a failure of the call.
    #[test]
    fn an_unparseable_file_analyzes_to_a_reported_failure() {
        let path = temp_file("unparseable", "(defun f (x)\n");
        let result = reply(dispatch(
            &mut Cache::default(),
            "paredit/analyze",
            &json!({ "path": path.display().to_string() }),
        ));
        assert_eq!(result["parsed"], false);
        assert!(
            result["error"].as_str().is_some_and(|e| !e.is_empty()),
            "{result}"
        );
    }

    #[test]
    fn a_missing_file_is_an_invalid_parameter_rather_than_a_panic() {
        let outcome = dispatch(
            &mut Cache::default(),
            "paredit/analyze",
            &json!({ "path": "/nonexistent/paredit/nowhere.lisp" }),
        );
        match outcome {
            Outcome::Fail(error) => assert_eq!(error.code, error_codes::INVALID_PARAMS),
            other => panic!("expected a failure, got {other:?}"),
        }
    }

    #[test]
    fn an_unknown_method_is_reported_by_name() {
        match dispatch(&mut Cache::default(), "paredit/telepathy", &json!({})) {
            Outcome::Fail(error) => assert!(error.message.contains("telepathy"), "{error:?}"),
            other => panic!("expected a failure, got {other:?}"),
        }
    }
}
