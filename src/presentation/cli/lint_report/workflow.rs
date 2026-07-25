use anyhow::{Context, Result};

use crate::application::usecase::cons_to_list_report::collect_cons_to_lists;
use crate::application::usecase::constant_if_test_report::collect_constant_if_tests;
use crate::application::usecase::de_morgan_report::collect_de_morgans;
use crate::application::usecase::eq_char_comparison_report::collect_eq_char_comparisons;
use crate::application::usecase::eq_number_comparison_report::collect_eq_number_comparisons;
use crate::application::usecase::explicit_nil_return_report::collect_explicit_nil_returns;
use crate::application::usecase::explicit_step_delta_report::collect_explicit_step_deltas;
use crate::application::usecase::funcall_lambda_report::collect_funcall_lambdas;
use crate::application::usecase::if_to_or_report::collect_if_to_ors;
use crate::application::usecase::lint_report::{
    CATEGORIES, LintFinding, LintPolicyOptions, LintSuppressions, Severity, collect_lint_findings,
    evaluate_lint_policy, resolve_active_rules, rule_category, rule_severity,
    summarize_lint_findings,
};
use crate::application::usecase::manual_incf_report::collect_manual_incfs;
use crate::application::usecase::manual_push_report::collect_manual_pushes;
use crate::application::usecase::manual_pushnew_report::collect_manual_pushnews;
use crate::application::usecase::negated_comparison_report::collect_negated_comparisons;
use crate::application::usecase::negated_if_report::collect_negated_ifs;
use crate::application::usecase::negated_step_delta_report::collect_negated_step_deltas;
use crate::application::usecase::negated_when_unless_report::collect_negated_when_unless;
use crate::application::usecase::nested_boolean_report::collect_nested_booleans;
use crate::application::usecase::nested_cxr_report::collect_nested_cxrs;
use crate::application::usecase::nested_progn_report::collect_nested_progns;
use crate::application::usecase::nested_unless_report::collect_nested_unlesses;
use crate::application::usecase::nested_when_report::collect_nested_whens;
use crate::application::usecase::nil_comparison_report::collect_nil_comparisons;
use crate::application::usecase::nth_constant_index_report::collect_nth_constant_indexes;
use crate::application::usecase::one_armed_if_report::collect_one_armed_ifs;
use crate::application::usecase::one_step_arithmetic_report::collect_one_step_arithmetic;
use crate::application::usecase::redundant_apply_report::collect_redundant_applies;
use crate::application::usecase::redundant_body_progn_report::collect_redundant_body_progns;
use crate::application::usecase::redundant_boolean_identity_report::collect_redundant_boolean_identities;
use crate::application::usecase::redundant_eql_test_report::collect_redundant_eql_tests;
use crate::application::usecase::redundant_funcall_report::collect_redundant_funcalls;
use crate::application::usecase::redundant_identity_key_report::collect_redundant_identity_keys;
use crate::application::usecase::redundant_identity_report::collect_redundant_identities;
use crate::application::usecase::redundant_if_nil_report::collect_redundant_if_nils;
use crate::application::usecase::redundant_let_star_report::collect_redundant_let_stars;
use crate::application::usecase::redundant_progn_report::collect_redundant_progns;
use crate::application::usecase::redundant_quote_report::collect_redundant_quotes;
use crate::application::usecase::sharp_quoted_lambda_report::collect_sharp_quoted_lambdas;
use crate::application::usecase::sign_comparison_report::collect_sign_comparisons;
use crate::application::usecase::single_clause_cond_report::collect_single_clause_conds;
use crate::application::usecase::single_operand_arithmetic_report::collect_single_operand_arithmetic;
use crate::application::usecase::single_operand_boolean_report::collect_single_operand_booleans;
use crate::application::usecase::single_value_bind_report::collect_single_value_binds;
use crate::application::usecase::verbose_negation_report::collect_verbose_negations;
use crate::domain::sexpr::{ByteOffset, ByteSpan, SyntaxTree};
use crate::presentation::cli::lint_report::args::LintReportArgs;
use crate::presentation::cli::lint_report::baseline::{BaselineEntry, LintBaseline};
use crate::presentation::cli::lint_report::render::{
    LintFileFix, LintFix, LintFixPlanEntry, LintReplacement, LintSarifResult, LintStats,
    print_lint_fix_plan, print_lint_fix_report, print_lint_github_annotation, print_lint_report,
    print_lint_rule_catalog, print_lint_sarif, print_lint_stats, print_lint_unused_suppressions,
};
use crate::presentation::cli::shared::{
    apply_byte_span_edits, expand_input_files, read_input_dialect_and_tree, stable_text_hash,
    unified_diff, write_file_with_rollback,
};

/// The 1-based line and byte-based column of a byte offset in `text`.
fn line_and_column(text: &str, offset: usize) -> (usize, usize) {
    let clamped = offset.min(text.len());
    let bytes = text.as_bytes();
    let mut line = 1;
    let mut last_newline = None;
    for (index, byte) in bytes.iter().enumerate().take(clamped) {
        if *byte == b'\n' {
            line += 1;
            last_newline = Some(index);
        }
    }
    let column = match last_newline {
        Some(newline) => clamped - newline,
        None => clamped + 1,
    };
    (line, column)
}

/// A SARIF `primaryLocationLineHash` for a finding: a hash of the rule and the
/// trimmed content of its source line, so it stays stable when unrelated lines
/// are inserted or removed above it. An occurrence suffix disambiguates two
/// findings that share the same rule and line text.
fn line_fingerprint(
    rule: &str,
    text: &str,
    line: usize,
    seen: &mut std::collections::HashMap<String, usize>,
) -> String {
    let line_text = text
        .lines()
        .nth(line.saturating_sub(1))
        .unwrap_or("")
        .trim();
    let base = stable_text_hash(&format!("{rule}\n{line_text}"));
    let index = seen.entry(base.clone()).or_insert(0);
    let fingerprint = format!("{base}:{index}");
    *index += 1;
    fingerprint
}

/// Drops findings silenced by an inline `; paredit:ignore` directive in the
/// file's own source, so a suppression comment applies uniformly across every
/// output mode (report, SARIF, GitHub, and fix).
fn retain_unsuppressed(findings: Vec<LintFinding>, text: &str) -> Vec<LintFinding> {
    let suppressions = LintSuppressions::parse(text);
    if suppressions.is_empty() {
        return findings;
    }
    findings
        .into_iter()
        .filter(|finding| {
            let (line, _) = line_and_column(text, finding.span.start().get());
            !suppressions.is_suppressed(finding.rule, line)
        })
        .collect()
}

/// A finding's baseline identity hash: a hash of its *trimmed source line*, so
/// the identity survives unrelated edits elsewhere in the file (the rule and
/// path complete the key).
fn finding_content_hash(text: &str, finding: &LintFinding) -> String {
    let (line, _) = line_and_column(text, finding.span.start().get());
    let line_text = text
        .lines()
        .nth(line.saturating_sub(1))
        .unwrap_or("")
        .trim();
    stable_text_hash(line_text)
}

/// The gate-failure message for a set of reported finding rules, honoring both
/// `--fail-on-finding` (any finding) and `--fail-on <severity>` (findings at or
/// above a severity). Returns `None` when neither gate trips. Shared by the
/// SARIF and GitHub paths; the default report uses `evaluate_lint_policy`, which
/// applies the same two rules.
fn gate_message(finding_rules: &[&'static str], args: &LintReportArgs) -> Option<String> {
    let mut messages = Vec::new();
    if args.fail_on_finding && !finding_rules.is_empty() {
        messages.push(format!("finding_count {} exceeds 0", finding_rules.len()));
    }
    if let Some(threshold) = args.fail_on.map(Severity::from) {
        let count = finding_rules
            .iter()
            .filter(|rule| rule_severity(rule).at_least(threshold))
            .count();
        if count > 0 {
            messages.push(format!(
                "{count} finding(s) at severity {} or higher",
                threshold.as_str()
            ));
        }
    }
    (!messages.is_empty()).then(|| messages.join("; "))
}

/// Loads the `--baseline` file, or `None` when the flag is absent.
fn load_baseline(args: &LintReportArgs) -> Result<Option<LintBaseline>> {
    match &args.baseline {
        None => Ok(None),
        Some(path) => {
            let text = std::fs::read_to_string(path)
                .with_context(|| format!("reading baseline file {}", path.display()))?;
            Ok(Some(LintBaseline::parse(&text)?))
        }
    }
}

/// Drops findings recorded in `baseline` (matched by path + rule + line-content
/// hash), so only new findings survive. A no-op when `baseline` is `None`.
fn retain_unbaselined(
    findings: Vec<LintFinding>,
    text: &str,
    baseline: Option<&LintBaseline>,
) -> Vec<LintFinding> {
    let Some(baseline) = baseline else {
        return findings;
    };
    findings
        .into_iter()
        .filter(|finding| {
            let path = finding.path.display().to_string();
            let hash = finding_content_hash(text, finding);
            !baseline.contains(&path, finding.rule, &hash)
        })
        .collect()
}

/// Computes the automatic fixes available for the active rules in one file,
/// keyed by `(rule, byte_start, byte_end)` so each finding can look up its own
/// fix. Only rules with an unambiguous replacement contribute; today those are
/// `redundant-quote` (drop the quote, keeping the self-evaluating literal),
/// `redundant-progn` (replace the wrapper with its single body form, or `nil`
/// when the progn is empty), `single-operand-boolean` (replace `(and X)`/
/// `(or X)` with `X`), `single-operand-arithmetic` (replace `(+ X)`/`(* X)`
/// with `X`), `nested-progn` / `redundant-body-progn` (splice a progn's
/// body into the enclosing progn/implicit-body form), `redundant-if-nil` (drop
/// the redundant `nil` else), `redundant-let-star` (rewrite a ≤1-binding
/// `let*` head to `let`), `single-clause-cond` (rewrite a one-clause
/// `(cond (test body…))` as `(when test body…)`), `redundant-funcall`
/// (delete `funcall #'` so
/// `(funcall #'foo …)` becomes `(foo …)`), `funcall-lambda` (drop `funcall`
/// so `(funcall (lambda …) …)` becomes `((lambda …) …)`), `sharp-quoted-lambda`
/// (strip the redundant `#'` so `#'(lambda …)` becomes `(lambda …)`),
/// `redundant-eql-test` (delete an explicit default `:test #'eql`),
/// `redundant-identity-key` (delete an explicit default `:key #'identity`),
/// `redundant-identity` (replace
/// `(identity x)` with `x`), `cons-to-list` (rewrite `(cons a nil)` as
/// `(list a)`), `verbose-negation` (rewrite `(- 0 x)` / `(* x -1)` as `(- x)`),
/// `negated-when-unless` (a two-edit fix: flip the
/// `when`/`unless` head and drop the `(not …)`), `one-armed-if` (swap an
/// else-less `if` head for `when`), `manual-incf` (rewrite `(setf x (1+ x))` as
/// `(incf x)`), `manual-push` (rewrite `(setf x (cons e x))` as `(push e x)`),
/// `manual-pushnew` (rewrite `(setf x (adjoin e x))` as `(pushnew e x)`),
/// `explicit-step-delta` (drop the default `1` delta so `(incf x 1)` becomes
/// `(incf x)`), `negated-step-delta` (flip the operator so `(incf x -1)`
/// becomes `(decf x 1)`), `explicit-nil-return` (drop the default `nil` result so
/// `(return nil)` becomes `(return)`), `single-value-bind` (rewrite a one-variable
/// `(multiple-value-bind (x) f body)` as `(let ((x f)) body)`),
/// `if-to-or` (rewrite `(if x x y)` as `(or x y)`), `one-step-arithmetic`
/// (rewrite `(+ x 1)` as `(1+ x)` and `(- x 1)` as `(1- x)`),
/// `nested-boolean` (splice a same-operator `(or …)`/`(and …)` into its
/// enclosing `or`/`and`), `nested-when` (merge `(when a (when b body))` into
/// `(when (and a b) body)`), `nested-unless` (merge
/// `(unless a (unless b body))` into `(unless (or a b) body)`),
/// `nested-cxr` (collapse `(car (cdr x))` into `(cadr x)`),
/// `nth-constant-index` (rewrite `(nth 0 x)` as `(first x)`),
/// `redundant-apply` (rewrite `(apply #'f (list a b))` as `(f a b)`),
/// `sign-comparison` (rewrite `(= x 0)` as `(zerop x)`),
/// `negated-comparison` (rewrite `(not (= a b))` as `(/= a b)`),
/// `negated-if` (rewrite `(if (not c) a b)` as `(if c b a)`),
/// `constant-if-test` (drop the dead branch of `(if t a b)` / `(if nil a b)`),
/// `redundant-boolean-identity` (drop `t` from `and` / `nil` from `or`),
/// `de-morgan` (collapse `(and (not a) (not b))` into `(not (or a b))`),
/// `nil-comparison` (rewrite
/// `(eq X nil)` as `(null X)`), and `eq-number-comparison` /
/// `eq-char-comparison` (rewrite the `eq` head to `eql`). Fixes that substitute
/// a form use the exact source bytes of that form so reader prefixes and spacing
/// survive the rewrite.
fn collect_lint_fixes(
    file: &std::path::Path,
    dialect: crate::domain::dialect::Dialect,
    tree: &crate::domain::sexpr::SyntaxTree,
    text: &str,
    active: &[&str],
) -> Result<std::collections::HashMap<(&'static str, usize, usize), LintFix>> {
    let mut fixes = std::collections::HashMap::new();
    // Exact source of a span, for fixes that move a subform verbatim.
    let slice = |span: crate::domain::sexpr::ByteSpan| {
        text.get(span.start().get()..span.end().get())
            .unwrap_or_default()
            .to_owned()
    };

    if active.contains(&"redundant-quote") {
        let (_, items) = collect_redundant_quotes(file, dialect, tree)?;
        for item in items {
            let (start, end) = (item.span.start().get(), item.span.end().get());
            fixes.insert(
                ("redundant-quote", start, end),
                one_edit(
                    start,
                    end,
                    item.literal,
                    "Remove the redundant quote".to_owned(),
                ),
            );
        }
    }

    if active.contains(&"redundant-progn") {
        let (_, items) = collect_redundant_progns(file, dialect, tree)?;
        for item in items {
            let (start, end) = (item.span.start().get(), item.span.end().get());
            // An empty progn is `nil`; a single-form progn becomes that form,
            // copied verbatim from source to preserve reader prefixes/spacing.
            let replacement = item.inner_span.map_or_else(|| "nil".to_owned(), &slice);
            fixes.insert(
                ("redundant-progn", start, end),
                one_edit(
                    start,
                    end,
                    replacement,
                    "Unwrap the redundant progn".to_owned(),
                ),
            );
        }
    }

    if active.contains(&"nested-progn") {
        let (_, items) = collect_nested_progns(file, dialect, tree)?;
        for item in items {
            let (start, end) = (item.span.start().get(), item.span.end().get());
            // Splice the inner progn's body (exact source) in place of the
            // whole `(progn …)` wrapper.
            fixes.insert(
                ("nested-progn", start, end),
                one_edit(
                    start,
                    end,
                    slice(item.body_span),
                    "Splice the nested progn into the enclosing progn".to_owned(),
                ),
            );
        }
    }

    if active.contains(&"redundant-body-progn") {
        let (_, items) = collect_redundant_body_progns(file, dialect, tree)?;
        for item in items {
            let (start, end) = (item.span.start().get(), item.span.end().get());
            // Splice the progn's body (exact source) in place of the wrapper.
            fixes.insert(
                ("redundant-body-progn", start, end),
                one_edit(
                    start,
                    end,
                    slice(item.body_span),
                    format!("Splice the progn into the enclosing {}", item.parent),
                ),
            );
        }
    }

    if active.contains(&"redundant-if-nil") {
        let (_, items) = collect_redundant_if_nils(file, dialect, tree)?;
        for item in items {
            let (start, end) = (item.span.start().get(), item.span.end().get());
            // Delete the ` nil` else (from the then branch's end to the nil's).
            let removal_start = item.removal_span.start().get();
            let removal_end = item.removal_span.end().get();
            fixes.insert(
                ("redundant-if-nil", start, end),
                LintFix {
                    description: "Drop the redundant nil else branch".to_owned(),
                    replacements: vec![LintReplacement {
                        byte_offset: removal_start,
                        byte_length: removal_end - removal_start,
                        text: String::new(),
                    }],
                },
            );
        }
    }

    if active.contains(&"redundant-identity") {
        let (_, items) = collect_redundant_identities(file, dialect, tree)?;
        for item in items {
            let (start, end) = (item.span.start().get(), item.span.end().get());
            // Replace `(identity X)` with X's exact source.
            fixes.insert(
                ("redundant-identity", start, end),
                one_edit(
                    start,
                    end,
                    slice(item.inner_span),
                    "Drop the redundant identity call".to_owned(),
                ),
            );
        }
    }

    if active.contains(&"redundant-funcall") {
        let (_, items) = collect_redundant_funcalls(file, dialect, tree)?;
        for item in items {
            let (start, end) = (item.span.start().get(), item.span.end().get());
            // Delete `funcall ` and the `#'` prefix in one cut, from the funcall
            // head up to the callee symbol, leaving `(foo …)` byte-identical.
            let removal_start = item.removal_span.start().get();
            let removal_end = item.removal_span.end().get();
            fixes.insert(
                ("redundant-funcall", start, end),
                LintFix {
                    description: format!("Rewrite (funcall #'{} …) as a direct call", item.callee),
                    replacements: vec![LintReplacement {
                        byte_offset: removal_start,
                        byte_length: removal_end - removal_start,
                        text: String::new(),
                    }],
                },
            );
        }
    }

    if active.contains(&"funcall-lambda") {
        let (_, items) = collect_funcall_lambdas(file, dialect, tree)?;
        for item in items {
            let (start, end) = (item.span.start().get(), item.span.end().get());
            // Delete `funcall ` from the head up to the lambda form, leaving the
            // lambda in operator position: ((lambda …) …).
            let removal_start = item.head_span.start().get();
            let removal_end = item.lambda_span.start().get();
            fixes.insert(
                ("funcall-lambda", start, end),
                LintFix {
                    description: "Apply the lambda directly, dropping funcall".to_owned(),
                    replacements: vec![LintReplacement {
                        byte_offset: removal_start,
                        byte_length: removal_end - removal_start,
                        text: String::new(),
                    }],
                },
            );
        }
    }

    if active.contains(&"sharp-quoted-lambda") {
        let (_, items) = collect_sharp_quoted_lambdas(file, dialect, tree)?;
        for item in items {
            let (start, end) = (item.span.start().get(), item.span.end().get());
            // Strip the leading `#'` (tolerating rare whitespace) from the form.
            let whole = slice(item.span);
            let text = whole
                .strip_prefix("#'")
                .unwrap_or(whole.as_str())
                .trim_start()
                .to_owned();
            fixes.insert(
                ("sharp-quoted-lambda", start, end),
                one_edit(
                    start,
                    end,
                    text,
                    "Drop the redundant #' before lambda".to_owned(),
                ),
            );
        }
    }

    if active.contains(&"redundant-eql-test") {
        let (_, items) = collect_redundant_eql_tests(file, dialect, tree)?;
        for item in items {
            let (start, end) = (item.span.start().get(), item.span.end().get());
            // Delete the redundant ` :test #'eql` argument pair.
            let removal_start = item.removal_span.start().get();
            let removal_end = item.removal_span.end().get();
            fixes.insert(
                ("redundant-eql-test", start, end),
                LintFix {
                    description: "Drop the redundant :test #'eql".to_owned(),
                    replacements: vec![LintReplacement {
                        byte_offset: removal_start,
                        byte_length: removal_end - removal_start,
                        text: String::new(),
                    }],
                },
            );
        }
    }

    if active.contains(&"redundant-identity-key") {
        let (_, items) = collect_redundant_identity_keys(file, dialect, tree)?;
        for item in items {
            let (start, end) = (item.span.start().get(), item.span.end().get());
            // Delete the redundant ` :key #'identity` argument pair.
            let removal_start = item.removal_span.start().get();
            let removal_end = item.removal_span.end().get();
            fixes.insert(
                ("redundant-identity-key", start, end),
                LintFix {
                    description: "Drop the redundant :key #'identity".to_owned(),
                    replacements: vec![LintReplacement {
                        byte_offset: removal_start,
                        byte_length: removal_end - removal_start,
                        text: String::new(),
                    }],
                },
            );
        }
    }

    if active.contains(&"de-morgan") {
        let (_, items) = collect_de_morgans(file, dialect, tree)?;
        for item in items {
            let (start, end) = (item.span.start().get(), item.span.end().get());
            // Rewrite `(and (not a) (not b))` as `(not (or a b))`, copying each
            // negation's inner operand.
            let inners: Vec<String> = item.inner_spans.iter().map(|s| slice(*s)).collect();
            let text = format!("(not ({} {}))", item.opposite, inners.join(" "));
            fixes.insert(
                ("de-morgan", start, end),
                one_edit(
                    start,
                    end,
                    text,
                    format!("Collapse the {} of negations via De Morgan", item.operator),
                ),
            );
        }
    }

    if active.contains(&"redundant-boolean-identity") {
        let (_, items) = collect_redundant_boolean_identities(file, dialect, tree)?;
        for item in items {
            let (start, end) = (item.span.start().get(), item.span.end().get());
            // Reconstruct `(op kept…)` from the surviving operands, or collapse
            // to the bare identity when every operand was the identity.
            let kept: Vec<String> = item.kept_spans.iter().map(|s| slice(*s)).collect();
            let text = if kept.is_empty() {
                item.identity.to_owned()
            } else {
                format!("({} {})", item.operator, kept.join(" "))
            };
            fixes.insert(
                ("redundant-boolean-identity", start, end),
                one_edit(
                    start,
                    end,
                    text,
                    format!(
                        "Drop the redundant {} operand from {}",
                        item.identity, item.operator
                    ),
                ),
            );
        }
    }

    if active.contains(&"single-operand-boolean") {
        let (_, items) = collect_single_operand_booleans(file, dialect, tree)?;
        for item in items {
            let (start, end) = (item.span.start().get(), item.span.end().get());
            // Replace the wrapper with its sole operand, copied verbatim.
            fixes.insert(
                ("single-operand-boolean", start, end),
                one_edit(
                    start,
                    end,
                    slice(item.inner_span),
                    format!("Unwrap the single-operand {}", item.operator),
                ),
            );
        }
    }

    if active.contains(&"single-operand-arithmetic") {
        let (_, items) = collect_single_operand_arithmetic(file, dialect, tree)?;
        for item in items {
            let (start, end) = (item.span.start().get(), item.span.end().get());
            // Replace the wrapper with its sole operand, copied verbatim.
            fixes.insert(
                ("single-operand-arithmetic", start, end),
                one_edit(
                    start,
                    end,
                    slice(item.inner_span),
                    format!("Unwrap the single-operand {}", item.operator),
                ),
            );
        }
    }

    if active.contains(&"negated-when-unless") {
        let (_, items) = collect_negated_when_unless(file, dialect, tree)?;
        for item in items {
            let (start, end) = (item.span.start().get(), item.span.end().get());
            // Two disjoint edits: flip the head macro and drop the negation,
            // leaving the body and all spacing byte-identical.
            let replacements = vec![
                LintReplacement {
                    byte_offset: item.head_span.start().get(),
                    byte_length: item.head_span.end().get() - item.head_span.start().get(),
                    text: item.suggested_head.to_owned(),
                },
                LintReplacement {
                    byte_offset: item.test_span.start().get(),
                    byte_length: item.test_span.end().get() - item.test_span.start().get(),
                    text: slice(item.inner_span),
                },
            ];
            fixes.insert(
                ("negated-when-unless", start, end),
                LintFix {
                    description: format!(
                        "Rewrite {} ({} …) as {}",
                        item.head, item.negator, item.suggested_head
                    ),
                    replacements,
                },
            );
        }
    }

    // eq -> eql for eq-against-a-literal: eql is a strict superset of eq that
    // compares numbers and characters correctly, so replacing just the `eq`
    // head is a safe repair whenever these rules fire.
    if active.contains(&"one-armed-if") {
        let (_, items) = collect_one_armed_ifs(file, dialect, tree)?;
        for item in items {
            let (start, end) = (item.span.start().get(), item.span.end().get());
            let (head_start, head_end) = (item.head_span.start().get(), item.head_span.end().get());
            fixes.insert(
                ("one-armed-if", start, end),
                one_edit(
                    head_start,
                    head_end,
                    "when".to_owned(),
                    "Rewrite the else-less if as when".to_owned(),
                ),
            );
        }
    }

    if active.contains(&"verbose-negation") {
        let (_, items) = collect_verbose_negations(file, dialect, tree)?;
        for item in items {
            let (start, end) = (item.span.start().get(), item.span.end().get());
            // Rewrite as unary `(- X)`, copying X's source.
            fixes.insert(
                ("verbose-negation", start, end),
                one_edit(
                    start,
                    end,
                    format!("(- {})", slice(item.operand_span)),
                    "Use unary (- x) for negation".to_owned(),
                ),
            );
        }
    }

    if active.contains(&"redundant-let-star") {
        let (_, items) = collect_redundant_let_stars(file, dialect, tree)?;
        for item in items {
            let (start, end) = (item.span.start().get(), item.span.end().get());
            let (head_start, head_end) = (item.head_span.start().get(), item.head_span.end().get());
            // Rewrite just the head symbol: a ≤1-binding let* is exactly let.
            fixes.insert(
                ("redundant-let-star", start, end),
                one_edit(
                    head_start,
                    head_end,
                    "let".to_owned(),
                    "Rewrite the redundant let* as let".to_owned(),
                ),
            );
        }
    }

    if active.contains(&"single-clause-cond") {
        let (_, items) = collect_single_clause_conds(file, dialect, tree)?;
        for item in items {
            let (start, end) = (item.span.start().get(), item.span.end().get());
            // Wrap the clause interior verbatim: (cond (test body…)) -> (when test body…).
            fixes.insert(
                ("single-clause-cond", start, end),
                one_edit(
                    start,
                    end,
                    format!("(when {})", slice(item.clause_inner_span).trim()),
                    "Rewrite the single-clause cond as when".to_owned(),
                ),
            );
        }
    }

    if active.contains(&"nested-unless") {
        let (_, items) = collect_nested_unlesses(file, dialect, tree)?;
        for item in items {
            let (start, end) = (item.span.start().get(), item.span.end().get());
            // Merge the two tests: (unless a (unless b body)) -> (unless (or a b) body).
            let or = format!(
                "(or {} {})",
                slice(item.outer_test_span),
                slice(item.inner_test_span)
            );
            let text = match item.inner_body_span {
                Some(body) => format!("(unless {} {})", or, slice(body)),
                None => format!("(unless {})", or),
            };
            fixes.insert(
                ("nested-unless", start, end),
                one_edit(
                    start,
                    end,
                    text,
                    "Merge the nested unless tests with or".to_owned(),
                ),
            );
        }
    }

    if active.contains(&"nested-when") {
        let (_, items) = collect_nested_whens(file, dialect, tree)?;
        for item in items {
            let (start, end) = (item.span.start().get(), item.span.end().get());
            // Merge the two tests: (when a (when b body)) -> (when (and a b) body).
            let and = format!(
                "(and {} {})",
                slice(item.outer_test_span),
                slice(item.inner_test_span)
            );
            let text = match item.inner_body_span {
                Some(body) => format!("(when {} {})", and, slice(body)),
                None => format!("(when {})", and),
            };
            fixes.insert(
                ("nested-when", start, end),
                one_edit(
                    start,
                    end,
                    text,
                    "Merge the nested when tests with and".to_owned(),
                ),
            );
        }
    }

    if active.contains(&"nested-boolean") {
        let (_, items) = collect_nested_booleans(file, dialect, tree)?;
        for item in items {
            let (start, end) = (item.span.start().get(), item.span.end().get());
            // Splice the inner operands in place of the nested (op …) wrapper.
            fixes.insert(
                ("nested-boolean", start, end),
                one_edit(
                    start,
                    end,
                    slice(item.inner_span).trim().to_owned(),
                    "Flatten the nested same-operator and/or".to_owned(),
                ),
            );
        }
    }

    if active.contains(&"single-value-bind") {
        let (_, items) = collect_single_value_binds(file, dialect, tree)?;
        for item in items {
            let (start, end) = (item.span.start().get(), item.span.end().get());
            // Rewrite as a plain let: (multiple-value-bind (x) f body) -> (let ((x f)) body).
            let binding = format!("({} {})", slice(item.var_span), slice(item.form_span));
            let text = match item.body_span {
                Some(body) => format!("(let ({}) {})", binding, slice(body)),
                None => format!("(let ({}))", binding),
            };
            fixes.insert(
                ("single-value-bind", start, end),
                one_edit(
                    start,
                    end,
                    text,
                    "Rewrite single-value multiple-value-bind as let".to_owned(),
                ),
            );
        }
    }

    if active.contains(&"explicit-nil-return") {
        let (_, items) = collect_explicit_nil_returns(file, dialect, tree)?;
        for item in items {
            let (start, end) = (item.span.start().get(), item.span.end().get());
            // Drop the redundant nil result, preserving the operator (and block).
            let text = match item.block_span {
                Some(block) => format!("({} {})", slice(item.head_span), slice(block)),
                None => format!("({})", slice(item.head_span)),
            };
            fixes.insert(
                ("explicit-nil-return", start, end),
                one_edit(start, end, text, "Drop the explicit nil result".to_owned()),
            );
        }
    }

    if active.contains(&"explicit-step-delta") {
        let (_, items) = collect_explicit_step_deltas(file, dialect, tree)?;
        for item in items {
            let (start, end) = (item.span.start().get(), item.span.end().get());
            // Drop the redundant delta: (incf place 1) -> (incf place).
            fixes.insert(
                ("explicit-step-delta", start, end),
                one_edit(
                    start,
                    end,
                    format!("({} {})", slice(item.head_span), slice(item.place_span)),
                    "Drop the explicit default delta of 1".to_owned(),
                ),
            );
        }
    }

    if active.contains(&"negated-step-delta") {
        let (_, items) = collect_negated_step_deltas(file, dialect, tree)?;
        for item in items {
            let (start, end) = (item.span.start().get(), item.span.end().get());
            // Flip the operator and drop the sign: (incf x -5) -> (decf x 5).
            let magnitude = slice(item.delta_span);
            let magnitude = magnitude.strip_prefix('-').unwrap_or(&magnitude);
            let text = format!(
                "({} {} {})",
                item.opposite,
                slice(item.place_span),
                magnitude
            );
            fixes.insert(
                ("negated-step-delta", start, end),
                one_edit(
                    start,
                    end,
                    text,
                    format!("Use {} with a positive delta", item.opposite),
                ),
            );
        }
    }

    if active.contains(&"cons-to-list") {
        let (_, items) = collect_cons_to_lists(file, dialect, tree)?;
        for item in items {
            let (start, end) = (item.span.start().get(), item.span.end().get());
            // Rewrite as `(list ELEMENT [TAIL_ELEMENTS])`.
            let element = slice(item.element_span);
            let text = match item.tail_elements_span {
                Some(tail) => format!("(list {} {})", element, slice(tail)),
                None => format!("(list {element})"),
            };
            fixes.insert(
                ("cons-to-list", start, end),
                one_edit(start, end, text, "Rewrite the cons as a list".to_owned()),
            );
        }
    }

    if active.contains(&"manual-incf") {
        let (_, items) = collect_manual_incfs(file, dialect, tree)?;
        for item in items {
            let (start, end) = (item.span.start().get(), item.span.end().get());
            // Reconstruct `(incf V)` / `(incf V D)` / `(decf …)` from exact
            // source slices of the variable and (when present) the delta.
            let place = slice(item.place_span);
            let text = match item.delta_span {
                Some(delta) => format!("({} {} {})", item.suggested_head, place, slice(delta)),
                None => format!("({} {})", item.suggested_head, place),
            };
            fixes.insert(
                ("manual-incf", start, end),
                one_edit(
                    start,
                    end,
                    text,
                    format!("Rewrite the setf as {}", item.suggested_head),
                ),
            );
        }
    }

    if active.contains(&"manual-push") {
        let (_, items) = collect_manual_pushes(file, dialect, tree)?;
        for item in items {
            let (start, end) = (item.span.start().get(), item.span.end().get());
            // Reconstruct `(push E P)` from exact source slices of the pushed
            // element and the place variable.
            let text = format!(
                "(push {} {})",
                slice(item.element_span),
                slice(item.place_span)
            );
            fixes.insert(
                ("manual-push", start, end),
                one_edit(start, end, text, "Rewrite the setf as push".to_owned()),
            );
        }
    }

    if active.contains(&"redundant-apply") {
        let (_, items) = collect_redundant_applies(file, dialect, tree)?;
        for item in items {
            let (start, end) = (item.span.start().get(), item.span.end().get());
            // Reconstruct the direct call `(callee args…)`, copying the list's
            // element source; an empty `(list)` yields a zero-argument call.
            let text = match item.args_span {
                Some(args) => format!("({} {})", item.callee, slice(args)),
                None => format!("({})", item.callee),
            };
            fixes.insert(
                ("redundant-apply", start, end),
                one_edit(
                    start,
                    end,
                    text,
                    format!(
                        "Rewrite (apply #'{} (list …)) as a direct call",
                        item.callee
                    ),
                ),
            );
        }
    }

    if active.contains(&"nth-constant-index") {
        let (_, items) = collect_nth_constant_indexes(file, dialect, tree)?;
        for item in items {
            let (start, end) = (item.span.start().get(), item.span.end().get());
            // Rewrite `(nth N x)` as `(ordinal x)`, copying the list source.
            fixes.insert(
                ("nth-constant-index", start, end),
                one_edit(
                    start,
                    end,
                    format!("({} {})", item.ordinal, slice(item.list_span)),
                    format!(
                        "Use ({} …) instead of nth with a constant index",
                        item.ordinal
                    ),
                ),
            );
        }
    }

    if active.contains(&"nested-cxr") {
        let (_, items) = collect_nested_cxrs(file, dialect, tree)?;
        for item in items {
            let (start, end) = (item.span.start().get(), item.span.end().get());
            // Collapse to the combined accessor over the innermost argument.
            let text = format!("({} {})", item.combined, slice(item.arg_span));
            fixes.insert(
                ("nested-cxr", start, end),
                one_edit(
                    start,
                    end,
                    text,
                    format!("Combine the nested accessors into {}", item.combined),
                ),
            );
        }
    }

    if active.contains(&"manual-pushnew") {
        let (_, items) = collect_manual_pushnews(file, dialect, tree)?;
        for item in items {
            let (start, end) = (item.span.start().get(), item.span.end().get());
            // Reconstruct `(pushnew E P KW…)` by reusing adjoin's operand list.
            let text = format!("(pushnew {})", slice(item.args_span));
            fixes.insert(
                ("manual-pushnew", start, end),
                one_edit(start, end, text, "Rewrite the setf as pushnew".to_owned()),
            );
        }
    }

    if active.contains(&"constant-if-test") {
        let (_, items) = collect_constant_if_tests(file, dialect, tree)?;
        for item in items {
            let (start, end) = (item.span.start().get(), item.span.end().get());
            // Replace the whole form with the live branch (or `nil` for a false
            // one-armed if), dropping the dead branch.
            let text = match item.result_span {
                Some(span) => slice(span),
                None => "nil".to_owned(),
            };
            fixes.insert(
                ("constant-if-test", start, end),
                one_edit(
                    start,
                    end,
                    text,
                    format!("Drop the dead branch of the constant {} test", item.test),
                ),
            );
        }
    }

    if active.contains(&"negated-if") {
        let (_, items) = collect_negated_ifs(file, dialect, tree)?;
        for item in items {
            let (start, end) = (item.span.start().get(), item.span.end().get());
            // Rewrite `(if (not X) A B)` as `(if X B A)`: drop the negation and
            // swap the branches, copying each subform's exact source.
            let text = format!(
                "(if {} {} {})",
                slice(item.test_span),
                slice(item.else_span),
                slice(item.then_span)
            );
            fixes.insert(
                ("negated-if", start, end),
                one_edit(
                    start,
                    end,
                    text,
                    "Drop the negated test and swap the if branches".to_owned(),
                ),
            );
        }
    }

    if active.contains(&"one-step-arithmetic") {
        let (_, items) = collect_one_step_arithmetic(file, dialect, tree)?;
        for item in items {
            let (start, end) = (item.span.start().get(), item.span.end().get());
            // Rewrite as the unary shorthand: (+ x 1) -> (1+ x), (- x 1) -> (1- x).
            let text = format!("({} {})", item.shorthand, slice(item.operand_span));
            fixes.insert(
                ("one-step-arithmetic", start, end),
                one_edit(
                    start,
                    end,
                    text,
                    format!("Use the {} shorthand", item.shorthand),
                ),
            );
        }
    }

    if active.contains(&"if-to-or") {
        let (_, items) = collect_if_to_ors(file, dialect, tree)?;
        for item in items {
            let (start, end) = (item.span.start().get(), item.span.end().get());
            // Rewrite `(if x x y)` as `(or x y)`, evaluating x once.
            let text = format!("(or {} {})", slice(item.test_span), slice(item.else_span));
            fixes.insert(
                ("if-to-or", start, end),
                one_edit(
                    start,
                    end,
                    text,
                    "Rewrite (if x x y) as (or x y)".to_owned(),
                ),
            );
        }
    }

    if active.contains(&"negated-comparison") {
        let (_, items) = collect_negated_comparisons(file, dialect, tree)?;
        for item in items {
            let (start, end) = (item.span.start().get(), item.span.end().get());
            // Rewrite `(not (OP a b))` as `(COMPLEMENT a b)`, copying operands.
            fixes.insert(
                ("negated-comparison", start, end),
                one_edit(
                    start,
                    end,
                    format!("({} {})", item.complement, slice(item.operands_span)),
                    format!("Use the complement {} instead of negating", item.complement),
                ),
            );
        }
    }

    if active.contains(&"sign-comparison") {
        let (_, items) = collect_sign_comparisons(file, dialect, tree)?;
        for item in items {
            let (start, end) = (item.span.start().get(), item.span.end().get());
            // Rewrite the whole form as `(predicate X)`, copying X's source.
            fixes.insert(
                ("sign-comparison", start, end),
                one_edit(
                    start,
                    end,
                    format!("({} {})", item.predicate, slice(item.operand_span)),
                    format!("Use ({} X) instead of comparing against 0", item.predicate),
                ),
            );
        }
    }

    if active.contains(&"nil-comparison") {
        let (_, items) = collect_nil_comparisons(file, dialect, tree)?;
        for item in items {
            let (start, end) = (item.span.start().get(), item.span.end().get());
            // Rewrite the whole form as `(null X)`, copying X's exact source.
            fixes.insert(
                ("nil-comparison", start, end),
                one_edit(
                    start,
                    end,
                    format!("(null {})", slice(item.operand_span)),
                    format!("Rewrite ({} X nil) as (null X)", item.operator),
                ),
            );
        }
    }

    if active.contains(&"eq-number-comparison") {
        let (_, items) = collect_eq_number_comparisons(file, dialect, tree)?;
        for item in items {
            let (start, end) = (item.span.start().get(), item.span.end().get());
            let (head_start, head_end) = (item.head_span.start().get(), item.head_span.end().get());
            fixes.insert(
                ("eq-number-comparison", start, end),
                one_edit(
                    head_start,
                    head_end,
                    "eql".to_owned(),
                    "Compare with eql (eq is unreliable on numbers)".to_owned(),
                ),
            );
        }
    }

    if active.contains(&"eq-char-comparison") {
        let (_, items) = collect_eq_char_comparisons(file, dialect, tree)?;
        for item in items {
            let (start, end) = (item.span.start().get(), item.span.end().get());
            let (head_start, head_end) = (item.head_span.start().get(), item.head_span.end().get());
            fixes.insert(
                ("eq-char-comparison", start, end),
                one_edit(
                    head_start,
                    head_end,
                    "eql".to_owned(),
                    "Compare with eql (eq is unreliable on characters)".to_owned(),
                ),
            );
        }
    }

    Ok(fixes)
}

/// Builds a single-edit fix (the common case: one byte region replaced).
fn one_edit(start: usize, end: usize, text: String, description: String) -> LintFix {
    LintFix {
        description,
        replacements: vec![LintReplacement {
            byte_offset: start,
            byte_length: end.saturating_sub(start),
            text,
        }],
    }
}

pub(in crate::presentation::cli) fn lint_report(args: LintReportArgs) -> Result<()> {
    // Resolve the selected rules first so `--list-rules` can honor the same
    // `--rule`/`--exclude`/`--category` selectors as a scan (validation of rule
    // and category names happens here, before any file is read).
    let active = resolve_active_rules(&args.rules, &args.exclude, &args.categories)?;

    if args.list_rules {
        return print_lint_rule_catalog(&active, args.output);
    }

    let files = expand_input_files(&args.files, args.dialect)?;

    if args.sarif {
        return lint_report_sarif(&args, &files, &active);
    }

    if args.github {
        return lint_report_github(&args, &files, &active);
    }

    if args.stats {
        return lint_report_stats(&args, &files, &active);
    }

    if args.report_unused_suppressions {
        return lint_report_unused_suppressions(&args, &files);
    }

    if let Some(out_path) = args.write_baseline.clone() {
        return lint_report_write_baseline(&args, &files, &active, &out_path);
    }

    if args.fix_plan {
        return lint_report_fix_plan(&args, &files, &active);
    }

    if args.fix {
        return lint_report_fix(&args, &files, &active);
    }

    let baseline = load_baseline(&args)?;
    let mut findings = Vec::new();

    for file in &files {
        let (input, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let file_findings = collect_lint_findings(file, dialect, &tree)?;
        let file_findings = retain_unsuppressed(file_findings, &input.text);
        let file_findings = retain_unbaselined(file_findings, &input.text, baseline.as_ref());
        findings.extend(file_findings);
    }

    let summary = summarize_lint_findings(findings, &active);
    let policy = evaluate_lint_policy(
        LintPolicyOptions::new(args.fail_on_finding, args.fail_on.map(Severity::from)),
        &summary,
    );
    let policy_passed = policy.passed;
    let policy_message = policy.violations.join("; ");

    print_lint_report(&summary, &policy, args.output)?;

    if !policy_passed {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "lint-report policy failed: {policy_message}"
        )));
    }

    Ok(())
}

/// Emits findings from the active rules as SARIF 2.1.0, computing each
/// finding's line/column from its source file, then applies the same
/// `--fail-on-finding` gate as the standard report.
fn lint_report_sarif(
    args: &LintReportArgs,
    files: &[std::path::PathBuf],
    active: &[&str],
) -> Result<()> {
    let baseline = load_baseline(args)?;
    let mut results = Vec::new();
    // Disambiguates identical (rule, line-content) fingerprints so two findings
    // on look-alike lines get distinct stable ids.
    let mut fingerprint_seen: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();

    for file in files {
        let (input, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let fixes = collect_lint_fixes(file, dialect, &tree, &input.text, active)?;
        let findings =
            retain_unsuppressed(collect_lint_findings(file, dialect, &tree)?, &input.text);
        let findings = retain_unbaselined(findings, &input.text, baseline.as_ref());
        for finding in findings {
            if !active.contains(&finding.rule) {
                continue;
            }
            let start = finding.span.start().get();
            let end = finding.span.end().get();
            let (start_line, start_column) = line_and_column(&input.text, start);
            let fingerprint =
                line_fingerprint(finding.rule, &input.text, start_line, &mut fingerprint_seen);
            let fix = fixes.get(&(finding.rule, start, end)).cloned();
            results.push(LintSarifResult {
                rule: finding.rule,
                message: finding.message,
                path: finding.path.display().to_string(),
                start_line,
                start_column,
                byte_offset: start,
                byte_length: end.saturating_sub(start),
                fingerprint,
                fix,
            });
        }
    }

    let finding_rules: Vec<&'static str> = results.iter().map(|result| result.rule).collect();
    print_lint_sarif(&results)?;

    if let Some(message) = gate_message(&finding_rules, args) {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "lint-report policy failed: {message}"
        )));
    }

    Ok(())
}

/// Emits the machine-readable fix plan: for each file, every fixable finding's
/// exact byte-region replacements, without writing anything. Uses the same
/// per-file structure as the SARIF path (so byte offsets are unambiguous) and
/// honors inline suppressions and `--baseline`, matching the set `--fix` would
/// touch on its first pass. Unlike `--fix`, this does not iterate to a fixpoint:
/// re-running after applying one entry surfaces any fix it unlocks. Exits 0.
fn lint_report_fix_plan(
    args: &LintReportArgs,
    files: &[std::path::PathBuf],
    active: &[&str],
) -> Result<()> {
    let baseline = load_baseline(args)?;
    let mut entries = Vec::new();

    for file in files {
        let (input, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let fixes = collect_lint_fixes(file, dialect, &tree, &input.text, active)?;
        let suppressions = LintSuppressions::parse(&input.text);
        let findings =
            retain_unsuppressed(collect_lint_findings(file, dialect, &tree)?, &input.text);
        let findings = retain_unbaselined(findings, &input.text, baseline.as_ref());
        for finding in findings {
            if !active.contains(&finding.rule) {
                continue;
            }
            let start = finding.span.start().get();
            let end = finding.span.end().get();
            // A fix is keyed by (rule, form-span); a finding without one is a
            // report-only rule and simply contributes no plan entry.
            let Some(fix) = fixes.get(&(finding.rule, start, end)).cloned() else {
                continue;
            };
            // Skip a fix the suppression scan would silence, so the plan lists
            // exactly what `--fix` would apply.
            if !suppressions.is_empty() {
                let (line, _) = line_and_column(&input.text, start);
                if suppressions.is_suppressed(finding.rule, line) {
                    continue;
                }
            }
            entries.push(LintFixPlanEntry {
                rule: finding.rule,
                path: finding.path.display().to_string(),
                byte_offset: start,
                byte_length: end.saturating_sub(start),
                fix,
            });
        }
    }

    print_lint_fix_plan(&entries, args.output)?;
    Ok(())
}

/// The most fix passes to run over one file before giving up. Each pass applies
/// at least one fix and strictly shrinks the source (fixes only remove
/// wrappers/quotes), so the loop converges well within this bound; the cap is a
/// backstop against a pathological rule, not an expected limit.
const MAX_FIX_PASSES: usize = 64;

/// Applies every available auto-fix for the active rules to each input file,
/// iterating to a fixpoint so that fixes exposed by earlier fixes (e.g.
/// unwrapping `(progn (or x))` into `(or x)` then into `x`) are also applied.
/// Only rules with a registered fix contribute; the rest are ignored here.
///
/// With `args.diff` this is a preview: a unified diff of each changed file is
/// printed and nothing is written. With `args.check` nothing is written either,
/// but a non-empty pending set fails the run (exit 3) — a CI gate that a build
/// keeps green by having no auto-fixable lint left. Otherwise each rewrite is
/// reparsed and persisted with `write_file_with_rollback`, so a malformed result
/// can never overwrite good source.
fn lint_report_fix(
    args: &LintReportArgs,
    files: &[std::path::PathBuf],
    active: &[&str],
) -> Result<()> {
    let mut file_fixes = Vec::new();
    let mut fixes_applied = 0;

    for file in files {
        let (input, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let mut text = input.text.clone();
        let mut tree = tree;
        let mut per_rule: std::collections::BTreeMap<&'static str, usize> =
            std::collections::BTreeMap::new();
        let mut applied = 0;

        for _ in 0..MAX_FIX_PASSES {
            let mut fixes = collect_lint_fixes(file, dialect, &tree, &text, active)?;
            // Re-parse suppressions each pass: line numbers shift as edits land,
            // but the directive comment and its form move together.
            let suppressions = LintSuppressions::parse(&text);
            if !suppressions.is_empty() {
                fixes.retain(|(rule, start, _end), _| {
                    let (line, _) = line_and_column(&text, *start);
                    !suppressions.is_suppressed(rule, line)
                });
            }
            if fixes.is_empty() {
                break;
            }

            // Choose a non-overlapping subset, preferring the earliest-starting
            // (outermost) fix on any overlap; nested fixes it shadows are caught
            // on the next pass once the outer form has been rewritten.
            // Each candidate occupies its finding span [start, end); its edits
            // (one, or several for a multi-region fix) all fall within it.
            let mut candidates: Vec<(&'static str, usize, usize, Vec<LintReplacement>)> = fixes
                .into_iter()
                .map(|((rule, start, end), fix)| (rule, start, end, fix.replacements))
                .collect();
            candidates.sort_by_key(|(_, start, end, _)| (*start, *end));

            let mut edits = Vec::new();
            let mut chosen_rules = Vec::new();
            let mut last_end = 0;
            for (rule, start, end, replacements) in candidates {
                if start < last_end {
                    continue;
                }
                last_end = end;
                for replacement in replacements {
                    let edit_start = replacement.byte_offset;
                    let edit_end = replacement.byte_offset + replacement.byte_length;
                    edits.push((
                        ByteSpan::new(ByteOffset::new(edit_start), ByteOffset::new(edit_end)),
                        replacement.text,
                    ));
                }
                chosen_rules.push(rule);
            }

            let rewritten = apply_byte_span_edits(&text, edits)?;
            // Guard the rewrite before adopting it, mirroring the write path.
            tree = SyntaxTree::parse_with_dialect(&rewritten, dialect)
                .context("refusing to fix: rewritten source does not reparse")?;
            text = rewritten;
            for rule in chosen_rules {
                *per_rule.entry(rule).or_insert(0) += 1;
                applied += 1;
            }
        }

        if applied > 0 && text != input.text {
            if args.diff {
                // Preview only: the unified diff is the payload (stdout, so it
                // pipes to a file/pager), and nothing is written.
                print!("{}", unified_diff(file, &input.text, &text));
            } else if !args.check {
                write_file_with_rollback(file.clone(), text)?;
            }
            fixes_applied += applied;
            file_fixes.push(LintFileFix {
                path: file.display().to_string(),
                applied,
                per_rule: per_rule.into_iter().collect(),
            });
        }
    }

    // --check is a CI gate: nothing is written, and a non-empty pending set
    // fails the build (exit 3). Combines with --diff, which already printed the
    // diffs above. Checked before the --diff early return so its exit code wins.
    if args.check {
        if fixes_applied > 0 {
            return Err(crate::presentation::cli::gate::gate_failure(format!(
                "{fixes_applied} auto-fixable finding(s) across {} file(s) are not applied; \
                 run `inspect lint --fix` to apply them",
                file_fixes.len()
            )));
        }
        eprintln!("no pending auto-fixes");
        return Ok(());
    }

    if args.diff {
        // Keep stdout pure diff (a JSON/text summary would corrupt it); the
        // one-line tally goes to stderr, and nothing was written.
        eprintln!(
            "{fixes_applied} fix(es) across {} file(s) — preview only, nothing written",
            file_fixes.len()
        );
        return Ok(());
    }

    print_lint_fix_report(&file_fixes, fixes_applied, args.output)
}

/// Writes the current findings (for the active rules, after suppression) to a
/// baseline file, so a later `--baseline` run can gate only on new findings.
/// Prints a one-line summary and exits 0.
fn lint_report_write_baseline(
    args: &LintReportArgs,
    files: &[std::path::PathBuf],
    active: &[&str],
    out_path: &std::path::Path,
) -> Result<()> {
    let mut entries = Vec::new();

    for file in files {
        let (input, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let findings =
            retain_unsuppressed(collect_lint_findings(file, dialect, &tree)?, &input.text);
        for finding in findings {
            if !active.contains(&finding.rule) {
                continue;
            }
            entries.push(BaselineEntry {
                path: finding.path.display().to_string(),
                rule: finding.rule.to_owned(),
                hash: finding_content_hash(&input.text, &finding),
            });
        }
    }

    let baseline = LintBaseline::from_entries(entries);
    let entry_count = baseline.len();
    std::fs::write(out_path, baseline.to_json()?)
        .with_context(|| format!("writing baseline file {}", out_path.display()))?;

    match args.output {
        crate::presentation::cli::OutputFormat::Text => {
            println!("baseline_written\t{}", out_path.display());
            println!("entry_count\t{entry_count}");
        }
        crate::presentation::cli::OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "schema_version": 1,
                    "baseline_written": out_path.display().to_string(),
                    "entry_count": entry_count,
                }))?
            );
        }
    }

    Ok(())
}

/// Aggregates findings (for the active rules, after suppression and baseline)
/// into a rollup by severity, category, and rule — a lint-debt dashboard. Honors
/// the same `--rule`/`--category`/`--baseline` filters as the standard report.
fn lint_report_stats(
    args: &LintReportArgs,
    files: &[std::path::PathBuf],
    active: &[&str],
) -> Result<()> {
    let baseline = load_baseline(args)?;
    let mut by_rule: std::collections::BTreeMap<&'static str, usize> =
        std::collections::BTreeMap::new();
    let mut files_with_findings = 0;

    for file in files {
        let (input, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let findings =
            retain_unsuppressed(collect_lint_findings(file, dialect, &tree)?, &input.text);
        let findings = retain_unbaselined(findings, &input.text, baseline.as_ref());
        let mut file_had_finding = false;
        for finding in findings {
            if !active.contains(&finding.rule) {
                continue;
            }
            *by_rule.entry(finding.rule).or_insert(0) += 1;
            file_had_finding = true;
        }
        if file_had_finding {
            files_with_findings += 1;
        }
    }

    let finding_count = by_rule.values().sum();

    // Severity rollup (error then warning), always both keys.
    let mut error_count = 0;
    let mut warning_count = 0;
    for (rule, count) in &by_rule {
        match rule_severity(rule) {
            Severity::Error => error_count += count,
            Severity::Warning => warning_count += count,
        }
    }
    let by_severity = vec![("error", error_count), ("warning", warning_count)];

    // Category rollup, one entry per category in CATEGORIES order.
    let by_category = CATEGORIES
        .iter()
        .map(|category| {
            let count = by_rule
                .iter()
                .filter(|(rule, _)| rule_category(rule) == Some(*category))
                .map(|(_, count)| *count)
                .sum();
            (*category, count)
        })
        .collect();

    // Per-rule rollup, most-frequent first (ties broken by rule name).
    let mut by_rule: Vec<(&'static str, usize)> = by_rule.into_iter().collect();
    by_rule.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));

    let stats = LintStats {
        finding_count,
        files_scanned: files.len(),
        files_with_findings,
        by_severity,
        by_category,
        by_rule,
    };
    print_lint_stats(&stats, args.output)
}

/// Reports every inline `; paredit:ignore` directive that silences no finding
/// (a stale ignore or a typo'd rule name). Detection runs against *all* rules —
/// independent of `--rule`/`--exclude` — so an ignore is "unused" only when it
/// matches no finding the file actually has, and the run exits 3 if any are
/// found so CI can keep the ignore list clean.
fn lint_report_unused_suppressions(
    args: &LintReportArgs,
    files: &[std::path::PathBuf],
) -> Result<()> {
    let mut entries = Vec::new();

    for file in files {
        let (input, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        // Line -> the rules that reported a finding there (across every rule).
        let mut present: std::collections::HashMap<usize, std::collections::HashSet<&'static str>> =
            std::collections::HashMap::new();
        for finding in collect_lint_findings(file, dialect, &tree)? {
            let (line, _) = line_and_column(&input.text, finding.span.start().get());
            present.entry(line).or_default().insert(finding.rule);
        }

        let suppressions = LintSuppressions::parse(&input.text);
        for unused in suppressions.unused_directives(&present) {
            entries.push((file.display().to_string(), unused));
        }
    }

    let unused_count = entries.len();
    print_lint_unused_suppressions(&entries, args.output)?;

    if unused_count > 0 {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "lint-report policy failed: {unused_count} unused suppression(s)"
        )));
    }

    Ok(())
}

/// Emits findings from the active rules as GitHub Actions annotations
/// (`::error file=...,line=...,col=...::rule: message`), which the Actions
/// runner renders as inline annotations on the pull request diff. Applies the
/// same `--fail-on-finding` gate as the standard report.
fn lint_report_github(
    args: &LintReportArgs,
    files: &[std::path::PathBuf],
    active: &[&str],
) -> Result<()> {
    let baseline = load_baseline(args)?;
    let mut finding_rules: Vec<&'static str> = Vec::new();

    for file in files {
        let (input, dialect, tree) = read_input_dialect_and_tree(Some(file.clone()), args.dialect)?;
        let findings =
            retain_unsuppressed(collect_lint_findings(file, dialect, &tree)?, &input.text);
        let findings = retain_unbaselined(findings, &input.text, baseline.as_ref());
        for finding in findings {
            if !active.contains(&finding.rule) {
                continue;
            }
            finding_rules.push(finding.rule);
            let (line, column) = line_and_column(&input.text, finding.span.start().get());
            print_lint_github_annotation(
                &finding.path.display().to_string(),
                line,
                column,
                finding.rule,
                &finding.message,
            );
        }
    }

    if let Some(message) = gate_message(&finding_rules, args) {
        return Err(crate::presentation::cli::gate::gate_failure(format!(
            "lint-report policy failed: {message}"
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::usecase::lint_report::{FIXABLE_RULES, RULES};
    use crate::domain::dialect::Dialect;
    use std::collections::BTreeSet;
    use std::path::PathBuf;

    /// Guards that [`FIXABLE_RULES`] names exactly the rules `collect_lint_fixes`
    /// can actually produce a fix for. Each line below is the minimal trigger
    /// for one fixable rule; a non-fixable trigger (`if-arity`) is included to
    /// prove no stray fix leaks in.
    #[test]
    fn fixable_rules_match_the_fix_engine() {
        let source = concat!(
            "(list '5)\n",                            // redundant-quote
            "(progn only)\n",                         // redundant-progn
            "(progn a (progn b c))\n",                // nested-progn (the inner progn)
            "(when q (progn s t))\n",                 // redundant-body-progn
            "(if c d nil)\n",                         // redundant-if-nil
            "(funcall #'g m)\n",                      // redundant-funcall
            "(funcall (lambda (fx) fx) 9)\n",         // funcall-lambda
            "(mapcar #'(lambda (sq) sq) sqs)\n",      // sharp-quoted-lambda
            "(identity h)\n",                         // redundant-identity
            "(cons e nil)\n",                         // cons-to-list
            "(- 0 amt)\n",                            // verbose-negation
            "(let* ((a 1)) a)\n",                     // redundant-let-star
            "(cond (ok (run)))\n",                    // single-clause-cond
            "(incf tally 1)\n",                       // explicit-step-delta
            "(incf nsd -3)\n",                        // negated-step-delta
            "(return-from blk nil)\n",                // explicit-nil-return
            "(multiple-value-bind (mv) (vals) mv)\n", // single-value-bind
            "(or za (or pb qc))\n",                   // nested-boolean
            "(when wa (when wb (wc)))\n",             // nested-when
            "(unless ua (unless ub (uc)))\n",         // nested-unless
            "(and x)\n",                              // single-operand-boolean
            "(* x)\n",                                // single-operand-arithmetic
            "(when (not r) y)\n",                     // negated-when-unless
            "(if p q)\n",                             // one-armed-if
            "(setf ctr (1+ ctr))\n",                  // manual-incf
            "(setf lst (cons e lst))\n",              // manual-push
            "(setf st (adjoin e st))\n",              // manual-pushnew
            "(car (cdr z))\n",                        // nested-cxr
            "(nth 0 zs)\n",                           // nth-constant-index
            "(apply #'g (list m))\n",                 // redundant-apply
            "(find ret lst :test #'eql)\n",           // redundant-eql-test
            "(sort rik #'< :key #'identity)\n",       // redundant-identity-key
            "(= tally 0)\n",                          // sign-comparison
            "(not (< a b))\n",                        // negated-comparison
            "(if (not c) a b)\n",                     // negated-if
            "(if iv iv jv)\n",                        // if-to-or
            "(+ osa 1)\n",                            // one-step-arithmetic
            "(if t on off)\n",                        // constant-if-test
            "(and p t q)\n",                          // redundant-boolean-identity
            "(and (not p) (not q))\n",                // de-morgan
            "(equal w nil)\n",                        // nil-comparison
            "(eq n 7)\n",                             // eq-number-comparison
            "(eq c #\\a)\n",                          // eq-char-comparison
            "(if a b c d)\n",                         // if-arity — NOT fixable
        );
        let tree =
            crate::domain::sexpr::SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp)
                .expect("parse fixture");
        let active: Vec<&str> = RULES.to_vec();
        let fixes = collect_lint_fixes(
            &PathBuf::from("fixture.lisp"),
            Dialect::CommonLisp,
            &tree,
            source,
            &active,
        )
        .expect("collect fixes");

        let produced: BTreeSet<&str> = fixes.keys().map(|(rule, _, _)| *rule).collect();
        let declared: BTreeSet<&str> = FIXABLE_RULES.iter().copied().collect();
        assert_eq!(
            produced, declared,
            "collect_lint_fixes must produce a fix for exactly the FIXABLE_RULES"
        );
    }
}
