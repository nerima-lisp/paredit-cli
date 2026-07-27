use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::Result;

use crate::call_report::domain::{CallReportItem, build_call_report};
use paredit_core_syntax::common_lisp::{
    common_lisp_operator_head_eq, common_lisp_symbol_reference_eq,
    common_lisp_symbol_reference_needle,
};
use paredit_core_syntax::definition::{DefinitionCategory, definition_shape};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{
    ByteSpan, ExpressionKind, ExpressionView, Path, SymbolName, SyntaxTree,
};

#[derive(Debug)]
pub struct SignatureReportSource {
    pub path: PathBuf,
    pub dialect: Dialect,
    pub tree: SyntaxTree,
}

impl SignatureReportSource {
    #[must_use]
    pub const fn new(path: PathBuf, dialect: Dialect, tree: SyntaxTree) -> Self {
        Self {
            path,
            dialect,
            tree,
        }
    }
}

#[derive(Debug)]
pub struct SignatureReportFile {
    pub path: PathBuf,
    pub dialect: Dialect,
    pub definitions: Vec<SignatureDefinitionItem>,
    pub calls: Vec<SignatureCallItem>,
}

impl SignatureReportFile {
    #[must_use]
    pub const fn new(
        path: PathBuf,
        dialect: Dialect,
        definitions: Vec<SignatureDefinitionItem>,
        calls: Vec<SignatureCallItem>,
    ) -> Self {
        Self {
            path,
            dialect,
            definitions,
            calls,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SignatureDefinitionItem {
    pub path: Path,
    pub span: ByteSpan,
    pub head: String,
    pub name: Option<String>,
    pub category: DefinitionCategory,
    pub parameter_count: Option<usize>,
    /// Minimum and maximum call arity. `None` maximum means unbounded.
    pub parameter_arity: Option<(usize, Option<usize>)>,
}

#[derive(Debug)]
pub struct SignatureCallItem {
    pub call: CallReportItem,
    /// Minimum and maximum arity of the matched definition.
    pub expected_parameter_arity: Option<(usize, Option<usize>)>,
    pub status: SignatureCallStatus,
}

impl SignatureCallItem {
    #[must_use]
    pub const fn new(
        call: CallReportItem,
        expected_parameter_arity: Option<(usize, Option<usize>)>,
        status: SignatureCallStatus,
    ) -> Self {
        Self {
            call,
            expected_parameter_arity,
            status,
        }
    }

    #[must_use]
    pub const fn is_mismatch(&self) -> bool {
        matches!(
            self.status,
            SignatureCallStatus::MissingArguments | SignatureCallStatus::ExtraArguments
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SignatureCallStatus {
    Exact,
    MissingArguments,
    ExtraArguments,
    UnknownDefinition,
    AmbiguousDefinition,
}

impl SignatureCallStatus {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::MissingArguments => "missing-arguments",
            Self::ExtraArguments => "extra-arguments",
            Self::UnknownDefinition => "unknown-definition",
            Self::AmbiguousDefinition => "ambiguous-definition",
        }
    }

    #[must_use]
    pub const fn is_mismatch(self) -> bool {
        matches!(self, Self::MissingArguments | Self::ExtraArguments)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignatureReportPolicy {
    pub fail_on_mismatch: bool,
    pub require_definitions: Option<usize>,
    pub require_calls: Option<usize>,
    pub definition_count: usize,
    pub call_count: usize,
    pub mismatch_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

#[must_use]
pub fn evaluate_signature_report_policy(
    definition_count: usize,
    statuses: &[SignatureCallStatus],
    fail_on_mismatch: bool,
    require_definitions: Option<usize>,
    require_calls: Option<usize>,
) -> SignatureReportPolicy {
    let call_count = statuses.len();
    let mismatch_count = statuses
        .iter()
        .filter(|status| status.is_mismatch())
        .count();
    let mut violations = Vec::new();

    if fail_on_mismatch && mismatch_count > 0 {
        violations.push(format!(
            "--fail-on-mismatch found {mismatch_count} incompatible call(s)"
        ));
    }
    if let Some(required) = require_definitions {
        if definition_count < required {
            violations.push(format!(
                "--require-definitions expected at least {required}, found {definition_count}"
            ));
        }
    }
    if let Some(required) = require_calls {
        if call_count < required {
            violations.push(format!(
                "--require-calls expected at least {required}, found {call_count}"
            ));
        }
    }

    SignatureReportPolicy {
        fail_on_mismatch,
        require_definitions,
        require_calls,
        definition_count,
        call_count,
        mismatch_count,
        passed: violations.is_empty(),
        violations,
    }
}

/// Classify a call against definitions grouped by Common Lisp symbol identity.
pub fn classify_signature_call(
    definitions_by_name: &BTreeMap<String, Vec<(usize, Option<usize>)>>,
    call: &CallReportItem,
) -> (Option<(usize, Option<usize>)>, SignatureCallStatus) {
    let arities = definitions_by_name
        .get(&common_lisp_symbol_reference_needle(&call.head))
        .map(Vec::as_slice)
        .unwrap_or_default();
    let [(min, max)] = arities else {
        return if arities.is_empty() {
            (None, SignatureCallStatus::UnknownDefinition)
        } else {
            (None, SignatureCallStatus::AmbiguousDefinition)
        };
    };

    let status = if call.argument_count < *min {
        SignatureCallStatus::MissingArguments
    } else if max.is_some_and(|max| call.argument_count > max) {
        SignatureCallStatus::ExtraArguments
    } else {
        SignatureCallStatus::Exact
    };
    (Some((*min, *max)), status)
}

pub fn build_signature_reports(
    sources: Vec<SignatureReportSource>,
    symbol: Option<&SymbolName>,
) -> Result<Vec<SignatureReportFile>> {
    let mut parsed = Vec::with_capacity(sources.len());
    let mut definitions_by_name = BTreeMap::<String, Vec<(usize, Option<usize>)>>::new();

    for source in sources {
        let definitions = collect_signature_definitions(&source.tree, source.dialect, symbol)?;
        let calls = build_call_report(&source.tree, source.dialect, symbol, false)?;

        for definition in &definitions {
            let Some(name) = &definition.name else {
                continue;
            };
            let Some(arity) = definition.parameter_arity else {
                continue;
            };
            definitions_by_name
                .entry(common_lisp_symbol_reference_needle(name))
                .or_default()
                .push(arity);
        }

        parsed.push((source.path, source.dialect, definitions, calls));
    }

    Ok(parsed
        .into_iter()
        .map(|(path, dialect, definitions, calls)| {
            SignatureReportFile::new(
                path,
                dialect,
                definitions,
                calls
                    .into_iter()
                    .map(|call| {
                        let (expected_parameter_arity, status) =
                            classify_signature_call(&definitions_by_name, &call);
                        SignatureCallItem::new(call, expected_parameter_arity, status)
                    })
                    .collect(),
            )
        })
        .collect())
}

fn collect_signature_definitions(
    tree: &SyntaxTree,
    dialect: Dialect,
    symbol: Option<&SymbolName>,
) -> Result<Vec<SignatureDefinitionItem>> {
    let mut definitions = Vec::new();

    for index in 0..tree.root_children().len() {
        let path = Path::root_child(index);
        let view = tree.select_path(&path)?.view();
        let Some(head) = list_head(&view) else {
            continue;
        };
        let Some(shape) = definition_shape(dialect, &view, head) else {
            continue;
        };
        let is_symbol_macro_definition = shape.category == DefinitionCategory::Variable
            && common_lisp_operator_head_eq(head, "define-symbol-macro");

        if !shape.category.is_callable() && !is_symbol_macro_definition {
            continue;
        }

        let name = shape.name(&view).map(ToOwned::to_owned);
        if name.as_deref().is_none_or(|name| {
            symbol.is_some_and(|target| !common_lisp_symbol_reference_eq(name, target.as_str()))
        }) {
            continue;
        }

        let (parameter_count, parameter_arity) = if is_symbol_macro_definition {
            (None, None)
        } else {
            let Some(parameter_count) = shape.lambda_parameter_count(&view) else {
                continue;
            };
            let parameter_arity = shape.lambda_parameter_arity(&view);
            (Some(parameter_count), parameter_arity)
        };

        definitions.push(SignatureDefinitionItem {
            path: path.clone(),
            span: view.span,
            head: head.to_owned(),
            name,
            category: shape.category,
            parameter_count,
            parameter_arity,
        });
    }

    Ok(definitions)
}

fn list_head(view: &ExpressionView) -> Option<&str> {
    view.children.first().and_then(atom_text)
}

fn atom_text(view: &ExpressionView) -> Option<&str> {
    (view.kind == ExpressionKind::Atom)
        .then_some(view.text.as_deref())
        .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evaluates_mismatch_and_thresholds() {
        let statuses = [
            SignatureCallStatus::MissingArguments,
            SignatureCallStatus::Exact,
        ];
        let policy = evaluate_signature_report_policy(1, &statuses, true, Some(2), Some(3));

        assert_eq!(policy.mismatch_count, 1);
        assert_eq!(policy.violations.len(), 3);
        assert!(!policy.passed);
    }
}
