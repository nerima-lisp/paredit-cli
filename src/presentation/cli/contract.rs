use serde_json::{Map, Value, json};

use crate::domain::dialect::Dialect;
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

#[derive(Clone, Copy)]
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SupportStatus {
    Supported,
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
            Self::Unsupported => "unsupported",
            Self::Unknown => "unknown",
        }
    }

    const fn dispatch_decision(self) -> Result<(), DispatchDenial> {
        match self {
            Self::Supported => Ok(()),
            Self::Unsupported => Err(DispatchDenial::Unsupported),
            Self::Unknown => Err(DispatchDenial::Unknown),
        }
    }
}

const INTROSPECTION_COMMANDS: [&str; 133] = [
    "inspect check",
    "inspect dialect",
    "inspect stats",
    "inspect agent-report",
    "inspect capabilities",
    "inspect outline",
    "inspect form",
    "inspect find-symbol",
    "inspect symbols",
    "inspect calls",
    "inspect signature",
    "inspect call-graph",
    "inspect impact",
    "inspect workspace",
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
    "inspect class-cycles",
    "inspect struct-cycles",
    "inspect system-conflicts",
    "inspect duplicate-setf-places",
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
    "inspect negated-comparison",
    "inspect negated-if",
    "inspect negated-step-delta",
    "inspect if-to-or",
    "inspect constant-if-test",
    "inspect cons-to-list",
    "inspect verbose-negation",
    "inspect negated-when-unless",
    "inspect nested-boolean",
    "inspect nested-cxr",
    "inspect nthcdr-zero",
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
    "inspect single-value-bind",
    "inspect sign-comparison",
    "inspect format-missing-destination",
    "inspect literal-place",
    "inspect eq-char-comparison",
    "inspect redundant-apply",
    "inspect redundant-eql-test",
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
    "inspect empty-body",
    "inspect empty-let",
    "inspect identity-arithmetic",
];

const FORMAT_COMMANDS: [&str; 2] = ["edit format", "edit repair-unclosed-lists"];

const STRUCTURAL_COMMANDS: [&str; 17] = [
    "edit select",
    "edit replace",
    "edit kill",
    "edit wrap",
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
];

const SEMANTIC_COMMANDS: [&str; 78] = [
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

const COMMAND_GROUPS: [(&[&str], CommandCategory); 4] = [
    (&INTROSPECTION_COMMANDS, CommandCategory::Introspection),
    (&FORMAT_COMMANDS, CommandCategory::Format),
    (&STRUCTURAL_COMMANDS, CommandCategory::Structural),
    (&SEMANTIC_COMMANDS, CommandCategory::Semantic),
];

const STATUS_VALUES: [SupportStatus; 3] = [
    SupportStatus::Supported,
    SupportStatus::Unsupported,
    SupportStatus::Unknown,
];

pub(super) fn support_status(command_path: &str, dialect: &str) -> SupportStatus {
    if !DIALECTS.contains(&dialect) || !contains_command(command_path) {
        return SupportStatus::Unknown;
    }

    let Ok(dialect) = dialect.parse::<Dialect>() else {
        return SupportStatus::Unknown;
    };

    let supported = match command_path {
        "refactor rename-at" => supports_rename_at_dialect(dialect),
        "refactor inline-function" => supports_inline_function_dialect(dialect),
        "refactor inline-let" => supports_inline_let_dialect(dialect),
        _ => return SupportStatus::Unknown,
    };

    if supported {
        SupportStatus::Supported
    } else {
        SupportStatus::Unsupported
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

pub(super) fn dialect_contract_report() -> Value {
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
                            Value::String(support_status(path, dialect).as_str().to_owned()),
                        )
                    })
                    .collect::<Map<String, Value>>();

                json!({
                    "path": path,
                    "category": category.as_str(),
                    "support": support,
                })
            })
        })
        .collect::<Vec<_>>();

    json!({
        "command_count": commands.len(),
        "dialect_count": DIALECTS.len(),
        "cell_count": commands.len() * DIALECTS.len(),
        "categories": ["introspection", "format", "structural", "semantic"],
        "statuses": STATUS_VALUES.map(SupportStatus::as_str),
        "dialects": DIALECTS,
        "commands": commands,
    })
}

fn contains_command(command_path: &str) -> bool {
    COMMAND_GROUPS
        .iter()
        .any(|(paths, _)| paths.contains(&command_path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn support_status_decision_is_fail_closed() {
        assert_eq!(SupportStatus::Supported.dispatch_decision(), Ok(()));
        assert_eq!(
            SupportStatus::Unsupported.dispatch_decision(),
            Err(DispatchDenial::Unsupported)
        );
        assert_eq!(
            SupportStatus::Unknown.dispatch_decision(),
            Err(DispatchDenial::Unknown)
        );
    }

    #[test]
    fn dispatch_adapter_allows_only_verified_supported_cells() {
        let supported = [
            ("refactor rename-at", "common-lisp"),
            ("refactor inline-function", "common-lisp"),
            ("refactor inline-function", "emacs-lisp"),
            ("refactor inline-let", "common-lisp"),
            ("refactor inline-let", "emacs-lisp"),
            ("refactor inline-let", "scheme"),
            ("refactor inline-let", "clojure"),
            ("refactor inline-let", "janet"),
            ("refactor inline-let", "fennel"),
        ];
        for (command_path, dialect) in supported {
            assert_eq!(dispatch_decision(command_path, dialect), Ok(()));
            assert!(dispatch_allowed(command_path, dialect));
        }

        let unsupported = [
            ("refactor rename-at", "emacs-lisp"),
            ("refactor rename-at", "scheme"),
            ("refactor rename-at", "clojure"),
            ("refactor rename-at", "janet"),
            ("refactor rename-at", "fennel"),
            ("refactor inline-function", "scheme"),
            ("refactor inline-function", "clojure"),
            ("refactor inline-function", "janet"),
            ("refactor inline-function", "fennel"),
        ];
        for (command_path, dialect) in unsupported {
            assert_eq!(
                dispatch_decision(command_path, dialect),
                Err(DispatchDenial::Unsupported)
            );
            assert!(!dispatch_allowed(command_path, dialect));
        }

        let unknown = [
            ("refactor rename-at", "unknown"),
            ("refactor inline-function", "unknown"),
            ("refactor inline-let", "unknown"),
            ("refactor rename-symbol", "scheme"),
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
