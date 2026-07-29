use serde_json::{Map, Value, json};

use crate::application::usecase::emacs_lisp_file_report::supports_emacs_lisp_file_report_dialect;
use crate::domain::dialect::{Dialect, SemanticOperation};
use crate::domain::inline_function::supports_inline_function_dialect;
use crate::domain::inline_let::supports_inline_let_dialect;
use crate::domain::rename::supports_rename_at_dialect;

pub(super) const DIALECTS: [&str; 10] = [
    "common-lisp",
    "emacs-lisp",
    "lfe",
    "scheme",
    "racket",
    "clojure",
    "hy",
    "carp",
    "janet",
    "fennel",
];

#[derive(Clone, Copy, Debug)]
enum CommandCategory {
    Introspection,
    Format,
    Structural,
    Semantic,
}

impl CommandCategory {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Introspection => "introspection",
            Self::Format => "format",
            Self::Structural => "structural",
            Self::Semantic => "semantic",
        }
    }
}

/// What a command needs from a dialect before its analysis means anything.
///
/// Every command in this tool parses the same balanced-parens tree, so the
/// interesting question is not whether it runs — nearly all of them exit zero
/// for every dialect — but whether it knows anything about the dialect it was
/// pointed at. A lint rule written against Common Lisp operators runs happily
/// over a Fennel file and reports nothing, which reads like a clean bill of
/// health and is not one.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CapabilityTier {
    /// Needs only the balanced-parens tree. Structural edits and formatting
    /// are complete for every dialect this build parses.
    Syntax,
    /// Needs the dialect's lexical scope and definition shapes.
    Scope,
    /// Needs the shared Common Lisp and Emacs Lisp form vocabulary. These
    /// commands carry an explicit `supports only Common Lisp and Emacs Lisp`
    /// gate rather than failing on the shape of the form.
    CommonLispFamily,
    /// Needs the Common Lisp operator, package and system model. This is where
    /// the lint rules and the package and CLOS reports live.
    CommonLispSemantics,
}

impl CapabilityTier {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Syntax => "syntax",
            Self::Scope => "scope",
            Self::CommonLispFamily => "common-lisp-family",
            Self::CommonLispSemantics => "common-lisp-semantics",
        }
    }

    /// Resolves this tier against a dialect using the predicates the runtime
    /// itself consults, so the matrix cannot drift from the behaviour.
    ///
    /// The category decides how an unmet tier presents itself. A refactor
    /// refuses outright — `convert-labels-to-flet` answers "supports only
    /// Common Lisp" and exits non-zero. A report does not: it exits zero and
    /// emits an empty finding list, which is why that case is `silent` rather
    /// than `unsupported`.
    fn status(self, dialect: Dialect, category: CommandCategory) -> SupportStatus {
        let satisfied = match self {
            Self::Syntax => dialect != Dialect::Unknown,
            Self::Scope => dialect.supports_semantic_operation(SemanticOperation::RenameBinding),
            Self::CommonLispFamily => {
                matches!(dialect, Dialect::CommonLisp | Dialect::EmacsLisp)
            }
            Self::CommonLispSemantics => dialect == Dialect::CommonLisp,
        };
        if satisfied {
            SupportStatus::Supported
        } else if matches!(category, CommandCategory::Introspection) {
            SupportStatus::Silent
        } else {
            SupportStatus::Unsupported
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SupportStatus {
    Supported,
    /// The command runs and succeeds, but carries no analysis for this dialect,
    /// so an empty result is not evidence of anything. Reported from schema
    /// version 3; earlier versions fold this into `unsupported`.
    Silent,
    Unsupported,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum DispatchDenial {
    Unsupported,
    Unknown,
}

impl SupportStatus {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Supported => "supported",
            Self::Silent => "silent",
            Self::Unsupported => "unsupported",
            Self::Unknown => "unknown",
        }
    }

    /// Collapses the status onto the vocabulary a schema version can express.
    ///
    /// Schema versions 1 and 2 predate `silent`; folding it onto `unsupported`
    /// keeps their documented three-value shape while still telling a consumer
    /// not to trust the command's output for that dialect.
    const fn for_schema(self, version: u8) -> Self {
        match (self, version) {
            (Self::Silent, 1 | 2) => Self::Unsupported,
            (status, _) => status,
        }
    }

    const fn dispatch_decision(self) -> Result<(), DispatchDenial> {
        match self {
            Self::Supported => Ok(()),
            Self::Silent | Self::Unsupported => Err(DispatchDenial::Unsupported),
            Self::Unknown => Err(DispatchDenial::Unknown),
        }
    }
}

const INTROSPECTION_COMMANDS: [&str; 219] = [
    "inspect diff",
    "inspect check",
    "inspect dialect",
    "inspect stats",
    "inspect agent-report",
    "inspect capabilities",
    "inspect outline",
    "inspect form",
    "inspect resolve",
    "inspect find-symbol",
    "inspect symbols",
    "inspect calls",
    "inspect signature",
    "inspect call-graph",
    "inspect impact",
    "inspect workspace",
    "inspect sources",
    "inspect dependencies",
    "inspect packages",
    "inspect definitions",
    "inspect unused-definitions",
    "inspect duplicates",
    "inspect similarity",
    "inspect lets",
    "inspect complexity",
    "inspect naming",
    "inspect reachability",
    "inspect unused-parameters",
    "inspect shadowed-bindings",
    "inspect unused-local-callables",
    "inspect package-boundaries",
    "inspect call-cycles",
    "inspect package-cycles",
    "inspect system-cycles",
    "inspect unused-packages",
    "inspect unused-exports",
    "inspect unused-nicknames",
    "inspect package-conflicts",
    "inspect redefinitions",
    "inspect undefined-packages",
    "inspect api-surface",
    "inspect api-diff",
    "inspect test-map",
    "inspect symbol-index",
    "inspect keyword-arity",
    "inspect unreachable-expressions",
    "inspect external-systems",
    "inspect licenses",
    "inspect serial-consistency",
    "inspect blame",
    "inspect duplication-ratio",
    "inspect cohesion",
    "inspect hotspots",
    "inspect debt-score",
    "inspect indentation",
    "inspect docstrings",
    "inspect todo",
    "inspect line-metrics",
    "inspect macro-expansion",
    "inspect macro-hygiene",
    "inspect loop",
    "inspect format-directives",
    "inspect read-conditionals",
    "inspect read-time-eval",
    "inspect circular-literals",
    "inspect readtable-case",
    "inspect package-locks",
    "inspect method-combination",
    "inspect class-hierarchy",
    "inspect generic-dispatch",
    "inspect restarts",
    "inspect types",
    "inspect narrowing",
    "inspect constants",
    "inspect value-propagation",
    "inspect effects",
    "inspect class-cycles",
    "inspect struct-cycles",
    "inspect system-conflicts",
    "inspect duplicate-setf-places",
    "inspect elisp-file",
    "inspect duplicate-slots",
    "inspect duplicate-methods",
    "inspect duplicate-case-keys",
    "inspect self-assignments",
    "inspect identical-if-branches",
    "inspect duplicate-cond-tests",
    "inspect duplicate-let-bindings",
    "inspect duplicate-boolean-operands",
    "inspect eql-string-comparison",
    "inspect lint",
    "inspect self-comparison",
    "inspect dead-boolean-operand",
    "inspect eq-number-comparison",
    "inspect setf-arity",
    "inspect duplicate-exports",
    "inspect eql-list-comparison",
    "inspect duplicate-parameters",
    "inspect redundant-quote",
    "inspect unreachable-cond-clause",
    "inspect malformed-let-binding",
    "inspect manual-incf",
    "inspect manual-push",
    "inspect manual-pushnew",
    "inspect if-arity",
    "inspect malformed-cond-clause",
    "inspect malformed-case-clause",
    "inspect unreachable-case-clause",
    "inspect malformed-iteration-spec",
    "inspect duplicate-lambda-list-keyword",
    "inspect lambda-list-keyword-order",
    "inspect modify-macro-arity",
    "inspect binds-constant",
    "inspect quoted-case-key",
    "inspect case-nil-key",
    "inspect typecase-nil-key",
    "inspect t-comparison",
    "inspect the-arity",
    "inspect equality-arity",
    "inspect accessor-arity",
    "inspect setq-non-variable",
    "inspect eval-when-situation",
    "inspect exhaustive-case-otherwise",
    "inspect explicit-step-delta",
    "inspect explicit-nil-return",
    "inspect redundant-progn",
    "inspect redundant-prog1",
    "inspect negated-comparison",
    "inspect negated-if",
    "inspect negated-step-delta",
    "inspect if-to-or",
    "inspect if-not",
    "inspect if-to-unless",
    "inspect prog2-to-progn",
    "inspect handler-case-no-clauses",
    "inspect unwind-protect-no-cleanup",
    "inspect constant-if-test",
    "inspect constant-when-test",
    "inspect cons-to-list",
    "inspect double-reverse",
    "inspect append-list-to-cons",
    "inspect list-star-to-cons",
    "inspect values-list-of-list",
    "inspect multiple-value-list-of-values",
    "inspect append-nil",
    "inspect verbose-negation",
    "inspect negated-when-unless",
    "inspect nested-boolean",
    "inspect nested-cxr",
    "inspect nthcdr-zero",
    "inspect subseq-zero",
    "inspect car-nthcdr",
    "inspect car-reverse",
    "inspect nthcdr-small-index",
    "inspect nth-constant-index",
    "inspect nested-progn",
    "inspect nested-unless",
    "inspect nested-when",
    "inspect nil-comparison",
    "inspect one-armed-if",
    "inspect one-step-arithmetic",
    "inspect single-operand-boolean",
    "inspect single-operand-list-op",
    "inspect single-operand-arithmetic",
    "inspect single-arg-comparison",
    "inspect single-clause-cond",
    "inspect cond-t-clause",
    "inspect single-value-bind",
    "inspect sign-comparison",
    "inspect format-missing-destination",
    "inspect format-to-string",
    "inspect format-newline",
    "inspect literal-place",
    "inspect zero-divisor",
    "inspect duplicate-keyword",
    "inspect defpackage-quoted",
    "inspect step-zero",
    "inspect eq-char-comparison",
    "inspect redundant-apply",
    "inspect redundant-eql-test",
    "inspect redundant-start-zero",
    "inspect redundant-end-nil",
    "inspect redundant-from-end-nil",
    "inspect redundant-count-nil",
    "inspect make-hash-table-test",
    "inspect gethash-default",
    "inspect typep-predicate",
    "inspect coerce-to-t",
    "inspect redundant-identity-key",
    "inspect redundant-body-progn",
    "inspect redundant-boolean-identity",
    "inspect de-morgan",
    "inspect destructive-literal",
    "inspect eql-search-literal",
    "inspect redundant-identity",
    "inspect redundant-if-nil",
    "inspect redundant-let-star",
    "inspect redundant-funcall",
    "inspect redundant-the",
    "inspect funcall-lambda",
    "inspect sharp-quoted-lambda",
    "inspect char-op-string",
    "inspect string-case-fold",
    "inspect char-case-fold",
    "inspect nested-string-case",
    "inspect code-char-char-code",
    "inspect last-default-count",
    "inspect butlast-default-count",
    "inspect make-list-default-element",
    "inspect parse-integer-default-radix",
    "inspect getf-default-nil",
    "inspect make-array-default-keyword",
    "inspect nested-char-case",
    "inspect list-star-nil",
    "inspect empty-body",
    "inspect empty-let",
    "inspect identity-arithmetic",
    "inspect redundant-divisor",
    "inspect context-at",
];

const FORMAT_COMMANDS: [&str; 2] = ["edit format", "edit repair-unclosed-lists"];

const STRUCTURAL_COMMANDS: [&str; 29] = [
    "edit select",
    "edit replace",
    "edit kill",
    "edit copy",
    "edit yank",
    "edit wrap",
    "edit unwrap-prefix",
    "edit splice",
    "edit split",
    "edit join",
    "edit splice-killing-backward",
    "edit splice-killing-forward",
    "edit convolute",
    "edit raise",
    "edit transpose-forward",
    "edit transpose-backward",
    "edit slurp-forward",
    "edit slurp-backward",
    "edit barf-forward",
    "edit barf-backward",
    "edit transpose",
    "edit navigate",
    "edit delete-forward",
    "edit delete-backward",
    "edit newline",
    "edit reindent-defun",
    "edit split-string",
    "edit escape-string",
    "edit unescape-string",
];

const SEMANTIC_COMMANDS: [&str; 80] = [
    "refactor step",
    "refactor patch",
    "refactor plan",
    "refactor verify",
    "refactor preview",
    "refactor check",
    "refactor status",
    "refactor apply",
    "refactor diff",
    "refactor workspace-plan",
    "refactor workspace-preview",
    "refactor workspace-execute",
    "refactor remove-definition",
    "refactor remove-unused-definitions",
    "refactor move-definition",
    "refactor split-file",
    "refactor sort-definitions",
    "refactor move-form",
    "refactor insert-top-level",
    "refactor replacement-plan",
    "refactor replace-forms",
    "refactor add-export",
    "refactor sort-package-exports",
    "refactor sort-package-options",
    "refactor merge-package-options",
    "refactor rename-package",
    "refactor rename-at",
    "refactor rename-symbol",
    "refactor rename-in-form",
    "refactor rename-binding",
    "refactor rename-block",
    "refactor rename-tag",
    "refactor remove-unused-block",
    "refactor remove-unused-tag",
    "refactor rename-symbols",
    "refactor rename-function",
    "refactor rename-macrolet",
    "refactor rename-symbol-macro",
    "refactor rename-local-function",
    "refactor replace-function-calls",
    "refactor wrap-function-calls",
    "refactor unwrap-function-calls",
    "refactor unwrap-call",
    "refactor thread-expression",
    "refactor unthread-expression",
    "refactor extract-function",
    "refactor extract-local-function",
    "refactor extract-constant",
    "refactor inline-function",
    "refactor inline-lambda",
    "refactor inline-local-function",
    "refactor inline-symbol-macro",
    "refactor inline-literal-constant",
    "refactor add-function-parameter",
    "refactor move-function-parameter",
    "refactor swap-function-parameters",
    "refactor reorder-function-parameters",
    "refactor remove-function-parameter",
    "refactor introduce-let",
    "refactor inline-let",
    "refactor convert-let-to-let-star",
    "refactor convert-let-star-to-let",
    "refactor convert-do-star-to-do",
    "refactor convert-prog-star-to-prog",
    "refactor merge-nested-let-star",
    "refactor merge-nested-let",
    "refactor merge-nested-flet",
    "refactor split-let-star",
    "refactor split-let",
    "refactor eliminate-empty-binding-form",
    "refactor flatten-progn",
    "refactor convert-if-to-cond",
    "refactor convert-cond-to-if",
    "refactor convert-when-to-if",
    "refactor convert-unless-to-if",
    "refactor convert-if-to-when",
    "refactor convert-if-to-unless",
    "refactor convert-labels-to-flet",
    "refactor convert-flet-to-labels",
    "refactor remove-unused-binding",
];

/// Reports that read only the balanced-parens tree, with no dialect vocabulary
/// beyond the head names the parser already resolves.
///
/// The tier names what a command *needs*, not which dialects it answers for.
/// `inspect elisp-file` reads the parse and the comments beside it and nothing
/// more, yet it answers for Emacs Lisp alone — its own gate says so, the same
/// way `refactor rename-at` sits at the scope tier and is gated to three
/// dialects.
const SYNTAX_TIER_REPORTS: [&str; 12] = [
    "inspect context-at",
    "inspect diff",
    "inspect elisp-file",
    "inspect check",
    "inspect dialect",
    "inspect stats",
    "inspect capabilities",
    "inspect outline",
    "inspect form",
    "inspect find-symbol",
    "inspect workspace",
    "inspect sources",
];

/// Reports built on the dialect-neutral definition and scope shapes, which
/// every dialect with a verified semantic policy provides.
const SCOPE_TIER_REPORTS: [&str; 28] = [
    "inspect resolve",
    "inspect test-map",
    "inspect symbol-index",
    "inspect unreachable-expressions",
    "inspect blame",
    "inspect duplication-ratio",
    "inspect cohesion",
    "inspect hotspots",
    "inspect debt-score",
    "inspect indentation",
    "inspect docstrings",
    "inspect todo",
    "inspect line-metrics",
    "inspect definitions",
    "inspect complexity",
    "inspect naming",
    "inspect call-graph",
    "inspect calls",
    "inspect signature",
    "inspect impact",
    "inspect symbols",
    "inspect unused-definitions",
    "inspect unused-parameters",
    "inspect shadowed-bindings",
    "inspect lets",
    "inspect duplicates",
    "inspect similarity",
    "inspect call-cycles",
];

/// Refactor plumbing that moves whole forms or drives the manifest workflow.
/// None of it reads an operator name, so it is complete for every dialect.
const SYNTAX_TIER_REFACTORS: [&str; 16] = [
    "refactor step",
    "refactor patch",
    "refactor plan",
    "refactor verify",
    "refactor preview",
    "refactor check",
    "refactor status",
    "refactor apply",
    "refactor diff",
    "refactor workspace-plan",
    "refactor workspace-preview",
    "refactor workspace-execute",
    "refactor move-form",
    "refactor insert-top-level",
    "refactor replacement-plan",
    "refactor replace-forms",
];

/// Refactors driven by the dialect-neutral scope and definition shapes.
const SCOPE_TIER_REFACTORS: [&str; 12] = [
    "refactor rename-at",
    "refactor rename-binding",
    "refactor rename-symbol",
    "refactor rename-symbols",
    "refactor rename-in-form",
    "refactor introduce-let",
    "refactor inline-let",
    "refactor extract-function",
    "refactor extract-constant",
    "refactor remove-unused-binding",
    "refactor remove-definition",
    "refactor sort-definitions",
];

/// Refactors whose own gate is `supports only Common Lisp and Emacs Lisp`.
///
/// Read off the validators in `paredit_core_edit`, not inferred: the binding
/// and control-flow conversions share a form vocabulary between the two
/// dialects, while their siblings below are Common Lisp only.
const COMMON_LISP_FAMILY_REFACTORS: [&str; 13] = [
    "refactor convert-let-to-let-star",
    "refactor merge-nested-let",
    "refactor merge-nested-let-star",
    "refactor split-let",
    "refactor split-let-star",
    "refactor eliminate-empty-binding-form",
    "refactor flatten-progn",
    "refactor convert-if-to-cond",
    "refactor convert-cond-to-if",
    "refactor convert-when-to-if",
    "refactor convert-unless-to-if",
    "refactor convert-if-to-when",
    "refactor convert-if-to-unless",
];

/// Returns what `command_path` needs from a dialect.
///
/// Structural editing and formatting are pure paren surgery. The rest defaults
/// to the Common Lisp model, because that is what the lint rules and the
/// package, system and CLOS reports are written against; the lists above name
/// the commands that reach a dialect-neutral layer instead.
fn capability_tier(command_path: &str, category: CommandCategory) -> CapabilityTier {
    match category {
        CommandCategory::Format | CommandCategory::Structural => CapabilityTier::Syntax,
        CommandCategory::Introspection => {
            if SYNTAX_TIER_REPORTS.contains(&command_path) {
                CapabilityTier::Syntax
            } else if SCOPE_TIER_REPORTS.contains(&command_path) {
                CapabilityTier::Scope
            } else {
                CapabilityTier::CommonLispSemantics
            }
        }
        CommandCategory::Semantic => {
            if SYNTAX_TIER_REFACTORS.contains(&command_path) {
                CapabilityTier::Syntax
            } else if SCOPE_TIER_REFACTORS.contains(&command_path) {
                CapabilityTier::Scope
            } else if COMMON_LISP_FAMILY_REFACTORS.contains(&command_path) {
                CapabilityTier::CommonLispFamily
            } else {
                CapabilityTier::CommonLispSemantics
            }
        }
    }
}

const COMMAND_GROUPS: [(&[&str], CommandCategory); 4] = [
    (&INTROSPECTION_COMMANDS, CommandCategory::Introspection),
    (&FORMAT_COMMANDS, CommandCategory::Format),
    (&STRUCTURAL_COMMANDS, CommandCategory::Structural),
    (&SEMANTIC_COMMANDS, CommandCategory::Semantic),
];

const STATUS_VALUES: [SupportStatus; 4] = [
    SupportStatus::Supported,
    SupportStatus::Silent,
    SupportStatus::Unsupported,
    SupportStatus::Unknown,
];

const TIER_VALUES: [CapabilityTier; 4] = [
    CapabilityTier::Syntax,
    CapabilityTier::Scope,
    CapabilityTier::CommonLispFamily,
    CapabilityTier::CommonLispSemantics,
];

const SEMANTIC_OPERATIONS: [SemanticOperation; 3] = [
    SemanticOperation::IntroduceLet,
    SemanticOperation::RenameBinding,
    SemanticOperation::ExtractFunction,
];

pub(super) fn support_status(command_path: &str, dialect: &str) -> SupportStatus {
    if !DIALECTS.contains(&dialect) {
        return SupportStatus::Unknown;
    }
    let Some(category) = command_category(command_path) else {
        return SupportStatus::Unknown;
    };
    let Ok(dialect) = dialect.parse::<Dialect>() else {
        return SupportStatus::Unknown;
    };

    // A command that owns an explicit runtime gate is answered by that gate,
    // so the matrix and the refusal can never disagree.
    let gated = match command_path {
        "refactor rename-at" => Some(supports_rename_at_dialect(dialect)),
        "refactor inline-function" => Some(supports_inline_function_dialect(dialect)),
        "refactor inline-let" => Some(supports_inline_let_dialect(dialect)),
        "inspect elisp-file" => Some(supports_emacs_lisp_file_report_dialect(dialect)),
        _ => None,
    };
    match gated {
        Some(true) => SupportStatus::Supported,
        // A gate that says no presents itself the same way an unmet tier
        // does, and for the same reason: a refactor refuses and exits
        // non-zero, a report exits zero with an empty result. Deciding this
        // here rather than per gate is what keeps `inspect elisp-file` —
        // which reports nothing for a `.lisp` file — `silent` rather than
        // claiming it would refuse.
        Some(false) if matches!(category, CommandCategory::Introspection) => SupportStatus::Silent,
        Some(false) => SupportStatus::Unsupported,
        None => capability_tier(command_path, category).status(dialect, category),
    }
}

#[allow(dead_code)]
pub(super) fn dispatch_decision(command_path: &str, dialect: &str) -> Result<(), DispatchDenial> {
    support_status(command_path, dialect).dispatch_decision()
}

#[allow(dead_code)]
pub(super) fn dispatch_allowed(command_path: &str, dialect: &str) -> bool {
    dispatch_decision(command_path, dialect).is_ok()
}

pub(super) fn dialect_contract_report(schema_version: u8) -> Value {
    let commands = COMMAND_GROUPS
        .iter()
        .flat_map(|(paths, category)| {
            paths.iter().map(move |path| {
                let path = *path;
                let support = DIALECTS
                    .iter()
                    .map(|dialect| {
                        (
                            (*dialect).to_owned(),
                            Value::String(
                                support_status(path, dialect)
                                    .for_schema(schema_version)
                                    .as_str()
                                    .to_owned(),
                            ),
                        )
                    })
                    .collect::<Map<String, Value>>();

                let mut report = json!({
                    "path": path,
                    "category": category.as_str(),
                    "support": support,
                });
                if schema_version >= 3 {
                    report["tier"] =
                        Value::String(capability_tier(path, *category).as_str().to_owned());
                }
                report
            })
        })
        .collect::<Vec<_>>();

    let statuses: Vec<&str> = STATUS_VALUES
        .iter()
        .filter(|status| schema_version >= 3 || **status != SupportStatus::Silent)
        .map(|status| status.as_str())
        .collect();

    let mut report = json!({
        "command_count": commands.len(),
        "dialect_count": DIALECTS.len(),
        "cell_count": commands.len() * DIALECTS.len(),
        "categories": ["introspection", "format", "structural", "semantic"],
        "statuses": statuses,
        "dialects": DIALECTS,
        "commands": commands,
    });
    if schema_version >= 3 {
        report["tiers"] = json!(TIER_VALUES.map(CapabilityTier::as_str));
        report["dialect_depth"] = dialect_depth_report();
    }
    report
}

/// Summarises, per dialect, how many commands land in each status.
///
/// This is the answer to "how deep does this tool actually go for my dialect",
/// which otherwise costs an agent one failed invocation per command. The
/// `silent` count is the important one: those commands succeed and report
/// nothing, which is not the same as finding nothing.
fn dialect_depth_report() -> Value {
    let paths: Vec<(&str, CommandCategory)> = COMMAND_GROUPS
        .iter()
        .flat_map(|(paths, category)| paths.iter().map(move |path| (*path, *category)))
        .collect();

    let entries = DIALECTS
        .iter()
        .map(|dialect| {
            let mut counts = [0usize; STATUS_VALUES.len()];
            for (path, _) in &paths {
                let status = support_status(path, dialect);
                let index = STATUS_VALUES
                    .iter()
                    .position(|candidate| *candidate == status)
                    .unwrap_or(STATUS_VALUES.len() - 1);
                counts[index] += 1;
            }
            let by_status = STATUS_VALUES
                .iter()
                .zip(counts)
                .map(|(status, count)| (status.as_str().to_owned(), json!(count)))
                .collect::<Map<String, Value>>();

            let parsed = dialect.parse::<Dialect>().unwrap_or(Dialect::Unknown);
            json!({
                "dialect": dialect,
                "family": parsed.family(),
                "separates_function_namespace": parsed.separates_function_namespace(),
                "verified_semantic_operations": SEMANTIC_OPERATIONS
                    .iter()
                    .filter(|operation| parsed.supports_semantic_operation(**operation))
                    .map(|operation| operation.label())
                    .collect::<Vec<_>>(),
                "by_status": by_status,
            })
        })
        .collect::<Vec<_>>();

    Value::Array(entries)
}

fn command_category(command_path: &str) -> Option<CommandCategory> {
    COMMAND_GROUPS
        .iter()
        .find(|(paths, _)| paths.contains(&command_path))
        .map(|(_, category)| *category)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn support_status_decision_is_fail_closed() {
        assert_eq!(SupportStatus::Supported.dispatch_decision(), Ok(()));
        for status in [SupportStatus::Silent, SupportStatus::Unsupported] {
            assert_eq!(
                status.dispatch_decision(),
                Err(DispatchDenial::Unsupported),
                "{status:?}"
            );
        }
        assert_eq!(
            SupportStatus::Unknown.dispatch_decision(),
            Err(DispatchDenial::Unknown)
        );
    }

    #[test]
    fn tier_tables_only_name_commands_that_exist() {
        let tables = [
            ("SYNTAX_TIER_REPORTS", SYNTAX_TIER_REPORTS.as_slice()),
            ("SCOPE_TIER_REPORTS", SCOPE_TIER_REPORTS.as_slice()),
            ("SYNTAX_TIER_REFACTORS", SYNTAX_TIER_REFACTORS.as_slice()),
            ("SCOPE_TIER_REFACTORS", SCOPE_TIER_REFACTORS.as_slice()),
            (
                "COMMON_LISP_FAMILY_REFACTORS",
                COMMON_LISP_FAMILY_REFACTORS.as_slice(),
            ),
        ];

        for (table, paths) in tables {
            for path in paths {
                assert!(
                    command_category(path).is_some(),
                    "{table} names {path}, which is not a registered command"
                );
            }
        }
    }

    #[test]
    fn every_cell_is_answered_for_every_known_dialect() {
        for (paths, _) in COMMAND_GROUPS {
            for path in paths {
                for dialect in DIALECTS {
                    assert_ne!(
                        support_status(path, dialect),
                        SupportStatus::Unknown,
                        "{path} / {dialect}"
                    );
                }
            }
        }
    }

    /// Commands whose subject is one dialect, so they say nothing about the
    /// rest — Common Lisp included.
    const DIALECT_SPECIFIC_REPORTS: [&str; 1] = ["inspect elisp-file"];

    #[test]
    fn only_reports_go_silent_and_only_outside_the_dialect_they_are_written_for() {
        for (paths, category) in COMMAND_GROUPS {
            for path in paths {
                for dialect in DIALECTS {
                    let status = support_status(path, dialect);
                    if status != SupportStatus::Silent {
                        continue;
                    }
                    // A refactor without dialect knowledge refuses; only a
                    // report succeeds while knowing nothing.
                    assert!(
                        matches!(category, CommandCategory::Introspection),
                        "{path} / {dialect} is {category:?}, which refuses instead of going silent",
                    );
                    // Common Lisp is what the analysis layer is written
                    // against, so a general report is never silent there. A
                    // report *about another dialect* is the exception, and it
                    // has to be named rather than inferred.
                    if DIALECT_SPECIFIC_REPORTS.contains(path) {
                        continue;
                    }
                    assert_ne!(dialect, "common-lisp", "{path}");
                }
            }
        }
    }

    #[test]
    fn a_dialect_specific_report_is_supported_for_exactly_one_dialect() {
        for path in DIALECT_SPECIFIC_REPORTS {
            let supported = DIALECTS
                .iter()
                .filter(|dialect| support_status(path, dialect) == SupportStatus::Supported)
                .count();
            assert_eq!(supported, 1, "{path}");
        }
    }

    #[test]
    fn older_schema_versions_keep_their_three_value_vocabulary() {
        for version in [1, 2] {
            assert_eq!(
                SupportStatus::Silent.for_schema(version),
                SupportStatus::Unsupported
            );
        }
        assert_eq!(SupportStatus::Silent.for_schema(3), SupportStatus::Silent);
        for status in [
            SupportStatus::Supported,
            SupportStatus::Unsupported,
            SupportStatus::Unknown,
        ] {
            for version in [1, 2, 3] {
                assert_eq!(status.for_schema(version), status, "{status:?}");
            }
        }
    }

    #[test]
    fn dispatch_adapter_allows_only_verified_supported_cells() {
        let supported = [
            ("refactor rename-at", "common-lisp"),
            ("refactor rename-at", "fennel"),
            ("refactor rename-at", "lfe"),
            ("refactor rename-at", "scheme"),
            ("refactor rename-at", "racket"),
            ("refactor inline-function", "common-lisp"),
            ("refactor inline-function", "emacs-lisp"),
            ("refactor inline-let", "common-lisp"),
            ("refactor inline-let", "emacs-lisp"),
            ("refactor inline-let", "scheme"),
            ("refactor inline-let", "clojure"),
            ("refactor inline-let", "janet"),
            ("refactor inline-let", "fennel"),
            // Structural editing and formatting are pure paren surgery.
            ("edit wrap", "carp"),
            ("edit format", "hy"),
            // Reports built on the dialect-neutral shapes.
            ("inspect outline", "janet"),
            ("inspect definitions", "fennel"),
            ("inspect lets", "carp"),
        ];
        for (command_path, dialect) in supported {
            assert_eq!(
                dispatch_decision(command_path, dialect),
                Ok(()),
                "{command_path} / {dialect}"
            );
            assert!(dispatch_allowed(command_path, dialect));
        }

        let unsupported = [
            ("refactor inline-function", "scheme"),
            ("refactor inline-function", "clojure"),
            ("refactor inline-function", "janet"),
            ("refactor inline-function", "fennel"),
            // Parenthesised binding lists are not how these dialects spell a
            // `let`, so the conversions have nothing to rewrite.
            ("refactor convert-let-to-let-star", "clojure"),
            ("refactor split-let", "fennel"),
            ("refactor convert-let-to-let-star", "lfe"),
            // No Common Lisp operator model outside Common Lisp.
            ("refactor convert-labels-to-flet", "janet"),
            ("refactor convert-do-star-to-do", "hy"),
        ];
        for (command_path, dialect) in unsupported {
            assert_eq!(
                dispatch_decision(command_path, dialect),
                Err(DispatchDenial::Unsupported),
                "{command_path} / {dialect}"
            );
            assert!(!dispatch_allowed(command_path, dialect));
        }

        // A report that has no rules for the dialect still runs and still
        // exits zero, so it is denied for dispatch but reported separately.
        let silent = [
            ("inspect lint", "fennel"),
            ("inspect redundant-progn", "lfe"),
            ("inspect packages", "janet"),
        ];
        for (command_path, dialect) in silent {
            assert_eq!(
                support_status(command_path, dialect),
                SupportStatus::Silent,
                "{command_path} / {dialect}"
            );
            assert!(!dispatch_allowed(command_path, dialect));
        }

        let unknown = [
            ("refactor rename-at", "elisp"),
            ("refactor inline-function", "unknown"),
            ("refactor inline-function extra", "common-lisp"),
            ("refactor inline", "common-lisp"),
            ("future command", "common-lisp"),
        ];
        for (command_path, dialect) in unknown {
            assert_eq!(
                dispatch_decision(command_path, dialect),
                Err(DispatchDenial::Unknown)
            );
            assert!(!dispatch_allowed(command_path, dialect));
        }

        for status in STATUS_VALUES {
            assert_eq!(
                status.dispatch_decision().is_ok(),
                status == SupportStatus::Supported
            );
        }
    }
}
