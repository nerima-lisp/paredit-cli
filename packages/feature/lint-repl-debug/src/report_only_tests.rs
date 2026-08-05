//! Regression tests for the four rules demoted to [`Fixability::ReportOnly`]
//! in this package: `leftover-format-debug-marker`, `leftover-step-call`,
//! `leftover-time-benchmark-call` and `leftover-trace-call`.
//!
//! # Why these assert source rather than `fix.is_none()`
//!
//! Following the precedent set by `leftover-inspect-call` (#138): the property
//! that matters is that a `fix apply` run leaves the file byte-for-byte alone,
//! not that a particular `Option` is `None`. [`rewrite`] therefore runs the real
//! dispatch, splices in every fix the rule attached exactly as `fix apply`
//! would, and returns the resulting source. With no fixes attached that is the
//! identity — so a fix reintroduced through *any* path fails these tests, not
//! only a restored `report_fixed` call.
//!
//! Each rule additionally carries an anti-over-suppression control asserting the
//! findings themselves are unchanged. Without it every assertion here would pass
//! for a rule that silently stopped reporting, which is the failure mode a
//! fixability demotion is most likely to introduce.

use std::path::Path;

use paredit_core_lint_engine::engine::{build_head_index, collect_lint_outcomes};
use paredit_core_lint_engine::model::Fixability;
use paredit_core_lint_engine::policy::RuleSelection;
use paredit_core_lint_engine::rule::{RuleCatalog, RuleEntry};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::SyntaxTree;

/// Runs one rule over `source` and returns `(finding messages, rewritten
/// source)`, with every attached fix applied.
///
/// `entries` is `'static` because [`RuleCatalog`] borrows for that lifetime;
/// each rule's module below owns a one-element `static` of its own.
fn rewrite(entries: &'static [RuleEntry], source: &str, dialect: Dialect) -> (Vec<String>, String) {
    let catalog = RuleCatalog::new(entries);
    let index = build_head_index(catalog);
    let tree = SyntaxTree::parse_with_dialect(source, dialect).expect("parse");
    let outcomes = collect_lint_outcomes(
        catalog,
        &index,
        Path::new("probe.lisp"),
        dialect,
        &tree,
        source,
        RuleSelection::All,
    )
    .expect("dispatch");

    let mut messages = Vec::new();
    let mut edits = Vec::new();
    for outcome in outcomes {
        let (finding, fix) = outcome.into_parts();
        messages.push(finding.message);
        if let Some(fix) = fix {
            for replacement in fix.replacements() {
                edits.push((replacement.span(), replacement.text().to_owned()));
            }
        }
    }
    edits.sort_by_key(|(span, _)| std::cmp::Reverse(span.start().get()));
    let mut rewritten = source.to_owned();
    for (span, text) in edits {
        rewritten.replace_range(span.start().get()..span.end().get(), &text);
    }
    (messages, rewritten)
}

mod step {
    use super::{Dialect, Fixability, RuleEntry, rewrite};
    use crate::leftover_step_call::rule::{META, RULE};

    static ENTRIES: [RuleEntry; 1] = [RuleEntry::new(&META, &RULE)];

    fn run(source: &str) -> (Vec<String>, String) {
        rewrite(&ENTRIES, source, Dialect::CommonLisp)
    }

    #[test]
    fn the_rule_is_report_only() {
        assert_eq!(META.fixability(), Fixability::ReportOnly);
    }

    /// The corpus case this rule was demoted for, reduced from ACL2's
    /// `books/models/jvm/m1/m1.lisp`: `step` is the machine's own
    /// state-transition function, defined at *package* level where no lexical
    /// binding table can see it. The fix turned `(equal s (step s))` into
    /// `(equal s s)` — a halt predicate that is unconditionally true — and
    /// `(run (cdr sched) (step s))` into `(run (cdr sched) s)`, a machine that
    /// never advances.
    #[test]
    fn a_package_level_machine_step_is_reported_and_the_source_is_left_alone() {
        let source = "(defun step (s)\n  (do-inst (next-inst s) s))\n\
                      (defun haltedp (s)\n  (equal s (step s)))\n\
                      (defun run (sched s)\n  (if (endp sched)\n      s\n    (run (cdr sched) (step s))))";
        let (messages, rewritten) = run(source);
        assert_eq!(
            rewritten, source,
            "the rule rewrote a machine model's own step function"
        );
        assert_eq!(messages.len(), 2, "both call sites must still be reported");
    }

    /// A binding form `support::binding_position` does not model. The fix
    /// deleted the loop variable outright, leaving `(dolist xs …)`, which is
    /// broken source that still parses.
    #[test]
    fn a_dolist_binding_named_step_is_never_rewritten() {
        let source = "(defun f (xs)\n  (dolist (step xs)\n    (use step)))";
        let (_, rewritten) = run(source);
        assert_eq!(rewritten, source);
    }

    /// An ACL2 `define` parameter list: `(step svex-env-p)` names a parameter
    /// `step` guarded by `svex-env-p`. The fix deleted the parameter *name*.
    #[test]
    fn an_acl2_define_parameter_list_is_never_rewritten() {
        let source = "(define counter-step-preconds ((step svex-env-p))\n  (declare (xargs :guard t))\n  step)";
        let (_, rewritten) = run(source);
        assert_eq!(rewritten, source);
    }

    /// Anti-over-suppression control: the demotion must not have silenced the
    /// rule, nor widened it.
    #[test]
    fn the_findings_themselves_are_unchanged_by_the_demotion() {
        let source = "(step (compute))\n(+ 1 2)";
        let (messages, rewritten) = run(source);
        assert_eq!(messages, vec!["step is a leftover stepping wrapper"]);
        assert_eq!(rewritten, source);

        for quiet in [
            "'(step (compute))",
            "(step)",
            "(step a b)",
            "(fboundp 'step)",
        ] {
            let (messages, rewritten) = run(quiet);
            assert!(messages.is_empty(), "{quiet} became a finding");
            assert_eq!(rewritten, quiet);
        }
    }
}

mod time_benchmark {
    use super::{Dialect, Fixability, RuleEntry, rewrite};
    use crate::leftover_time_benchmark_call::rule::{META, RULE};

    static ENTRIES: [RuleEntry; 1] = [RuleEntry::new(&META, &RULE)];

    fn run(source: &str) -> (Vec<String>, String) {
        rewrite(&ENTRIES, source, Dialect::CommonLisp)
    }

    #[test]
    fn the_rule_is_report_only() {
        assert_eq!(META.fixability(), Fixability::ReportOnly);
    }

    /// The dominant corpus class: the timing report is what the file exists to
    /// produce. Reduced from `3bz/bench.lisp` and
    /// `unicode/test-performance/tests.lsp`. Unwrapping deletes the only output
    /// such a file has.
    #[test]
    fn a_benchmark_body_is_reported_and_the_source_is_left_alone() {
        let source = "(defun bench (v)\n  (time (decompress v))\n  (finish-output))";
        let (messages, rewritten) = run(source);
        assert_eq!(rewritten, source, "the rule deleted a benchmark's timing");
        assert_eq!(messages, vec!["time is a leftover benchmarking wrapper"]);
    }

    /// A CFFI binder: `(time 'timespec)` binds the variable `time`. The fix
    /// produced `(with-foreign-object 'timespec …)`, deleting the variable name.
    /// Reduced from `osicat/mach/mach.lisp`.
    #[test]
    fn a_with_foreign_object_binding_named_time_is_never_rewritten() {
        let source = "(defun clock-get-time (clock-service)\n  (with-foreign-object (time 'timespec)\n    (%clock-get-time clock-service time)))";
        let (_, rewritten) = run(source);
        assert_eq!(rewritten, source);
    }

    /// A `dolist` binding, the same gap `leftover-step-call` hits. Reduced from
    /// `local-time/test/benchmarks.lisp`.
    #[test]
    fn a_dolist_binding_named_time_is_never_rewritten() {
        let source = "(defun f (times transitions)\n  (dolist (time times)\n    (transition-position time transitions)))";
        let (_, rewritten) = run(source);
        assert_eq!(rewritten, source);
    }

    /// An ACL2 `defthm` `:instance` hint is a free-variable substitution alist,
    /// so `(time '0)` binds the theorem's `time`. The fix collapsed it to `'0`.
    /// Reduced from `workshops/2009/verbeek-schmaltz/…/GeNoC.lisp`.
    #[test]
    fn an_acl2_instance_substitution_pair_is_never_rewritten() {
        let source = "(defthm v-ids\n  (implies (p x) (q x))\n  :hints ((\"Goal\" :use ((:instance thm (time '0) (m (f trs)))))))";
        let (_, rewritten) = run(source);
        assert_eq!(rewritten, source);
    }

    #[test]
    fn the_findings_themselves_are_unchanged_by_the_demotion() {
        for quiet in [
            "'(time (compute))",
            "(time)",
            "(time a b)",
            "(fboundp 'time)",
        ] {
            let (messages, rewritten) = run(quiet);
            assert!(messages.is_empty(), "{quiet} became a finding");
            assert_eq!(rewritten, quiet);
        }
    }
}

mod trace {
    use super::{Dialect, Fixability, RuleEntry, rewrite};
    use crate::leftover_trace_call::rule::{META, RULE};

    static ENTRIES: [RuleEntry; 1] = [RuleEntry::new(&META, &RULE)];

    fn run(source: &str) -> (Vec<String>, String) {
        rewrite(&ENTRIES, source, Dialect::CommonLisp)
    }

    #[test]
    fn the_rule_is_report_only() {
        assert_eq!(META.fixability(), Fixability::ReportOnly);
    }

    /// 11 of the 19 fixable corpus findings were this: a test file that calls
    /// `trace` at top level and then asserts on the trace output. Reduced from
    /// SBCL's `tests/trace.impure.lisp`, keeping the *top-level* position that
    /// is what makes the occurrence fixable — nested inside `with-test` the
    /// removal analysis already declines, so that shape would not discriminate.
    /// Deleting the `(trace f1)` makes the assertion below it fail.
    #[test]
    fn a_test_asserting_on_trace_output_is_reported_and_left_alone() {
        let source = "(defun f1 () (incf *count*))\n\
                      (trace f1)\n\
                      (let ((s (with-output-to-string (*trace-output*) (f1))))\n  \
                      (assert (search \"F1 returned\" s)))\n\
                      (untrace f1)\n\
                      (values)";
        let (messages, rewritten) = run(source);
        assert_eq!(
            rewritten, source,
            "the rule deleted the trace a test asserts on"
        );
        assert_eq!(messages.len(), 2, "trace and untrace both stay reported");
    }

    /// A package-qualified head is a different symbol that merely shares a
    /// name: Radiance's `l:trace` is a logging call at trace level, not
    /// `CL:TRACE`. The head match ignores the qualifier entirely.
    #[test]
    fn a_package_qualified_logging_trace_is_reported_and_left_alone() {
        let source = "(defun execute-request (request response)\n  (l:trace :core.request \"Executing request: ~s ~s\" request response)\n  (dispatch request))";
        let (messages, rewritten) = run(source);
        assert_eq!(rewritten, source);
        assert_eq!(messages.len(), 1);
    }

    #[test]
    fn the_findings_themselves_are_unchanged_by_the_demotion() {
        for head in crate::leftover_trace_call::domain::HEADS {
            let source = format!("({head} f)\n(+ 1 2)");
            let (messages, rewritten) = run(&source);
            assert_eq!(messages.len(), 1, "{head} stopped reporting");
            assert_eq!(rewritten, source, "{head} rewrote source");
        }

        for quiet in ["'(trace f)", "(fboundp 'trace)"] {
            let (messages, rewritten) = run(quiet);
            assert!(messages.is_empty(), "{quiet} became a finding");
            assert_eq!(rewritten, quiet);
        }
    }

    /// `untrace` is reported in Emacs Lisp too; the demotion must not have
    /// narrowed the dialect scope along with the fixability.
    #[test]
    fn emacs_lisp_untrace_is_still_reported() {
        let source = "(untrace)\n(message \"x\")";
        let (messages, rewritten) = rewrite(&ENTRIES, source, Dialect::EmacsLisp);
        assert_eq!(messages.len(), 1);
        assert_eq!(rewritten, source);
    }
}

mod format_debug_marker {
    use super::{Dialect, Fixability, RuleEntry, rewrite};
    use crate::leftover_format_debug_marker::rule::{META, RULE};

    static ENTRIES: [RuleEntry; 1] = [RuleEntry::new(&META, &RULE)];

    fn run(source: &str) -> (Vec<String>, String) {
        rewrite(&ENTRIES, source, Dialect::CommonLisp)
    }

    #[test]
    fn the_rule_is_report_only() {
        assert_eq!(META.fixability(), Fixability::ReportOnly);
    }

    /// The worst corpus case: ACL2's `interface-raw.lisp` lost a 638-byte
    /// user-facing error report because the message text mentions the variable
    /// `*check-built-in-constants-debug*`. Reduced here to the same shape.
    #[test]
    fn a_user_facing_error_report_is_reported_and_the_source_is_left_alone() {
        let source = "(defun check ()\n  (when bad\n    (format t \"~%ERROR: Failed check!  Please send this error message to the ~\n             implementors; use *check-built-in-constants-debug* = t for more.~%\")\n    (report bad)))";
        let (messages, rewritten) = run(source);
        assert_eq!(
            rewritten, source,
            "the rule deleted a user-facing error report"
        );
        assert_eq!(
            messages,
            vec!["format's control string carries a DEBUG/DBG marker"]
        );
    }

    /// `contains_debug_marker` requires a boundary only *before* the marker, so
    /// `DEBUG` matches at the start of an ordinary word. These four are the
    /// remaining corpus false positives, and the first is the one a trailing
    /// boundary would also have to catch.
    #[test]
    fn ordinary_prose_beginning_with_debug_is_never_rewritten() {
        for control in [
            "~&You are in the debugger. Commiserations!~%",
            "debug-vregs: ~:S~%",
            "~&Interactive mode (DEBUG_ASDF_TEST) -- Invoke debugger.~%",
            ";; Saving 8x8 debug font.~%",
        ] {
            let source = format!("(defun f (v)\n  (format t \"{control}\" v)\n  (finish))");
            let (messages, rewritten) = run(&source);
            assert_eq!(messages.len(), 1, "{control} stopped being reported");
            assert_eq!(rewritten, source, "{control} was rewritten");
        }
    }

    #[test]
    fn the_findings_themselves_are_unchanged_by_the_demotion() {
        for quiet in [
            "(format t \"~a\" x)",
            "(format nil \"DEBUG ~a\" x)",
            "(format t \"UNDEBUGGABLE\")",
            "'(format t \"DEBUG\")",
        ] {
            let (messages, rewritten) = run(quiet);
            assert!(messages.is_empty(), "{quiet} became a finding");
            assert_eq!(rewritten, quiet);
        }
    }
}
