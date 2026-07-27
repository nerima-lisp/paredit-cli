use crate::application::refactor::plan::RefactorPlanTargetKind;
use crate::application::usecase::impact_report::ImpactDefinitionItem;
use crate::application::usecase::impact_report::ImpactReportFile;
use crate::domain::common_lisp::{
    common_lisp_symbol_reference_eq, normalize_common_lisp_operator_head,
};
use crate::domain::definition::DefinitionCategory;

struct TargetKindRule {
    target_kind: RefactorPlanTargetKind,
    category: Option<DefinitionCategory>,
    heads: &'static [&'static str],
}

const TARGET_KIND_RULES: [TargetKindRule; 4] = [
    TargetKindRule {
        target_kind: RefactorPlanTargetKind::SymbolMacro,
        category: None,
        heads: &["define-symbol-macro"],
    },
    TargetKindRule {
        target_kind: RefactorPlanTargetKind::Macro,
        category: Some(DefinitionCategory::Macro),
        heads: &[
            "defmacro",
            "define-modify-macro",
            "define-method-combination",
        ],
    },
    TargetKindRule {
        target_kind: RefactorPlanTargetKind::CompilerMacro,
        category: Some(DefinitionCategory::Macro),
        heads: &["define-compiler-macro"],
    },
    TargetKindRule {
        target_kind: RefactorPlanTargetKind::SetfExpander,
        category: Some(DefinitionCategory::Macro),
        heads: &["define-setf-expander", "defsetf"],
    },
];

pub(super) fn derive_refactor_target_kind(
    files: &[ImpactReportFile],
    symbol: &str,
) -> RefactorPlanTargetKind {
    if let Some(target_kind) = TARGET_KIND_RULES
        .iter()
        .find(|rule| {
            files.iter().any(|file| {
                file.definitions
                    .iter()
                    .any(|definition| matches_target_kind_rule(definition, symbol, rule))
            })
        })
        .map(|rule| rule.target_kind)
    {
        return target_kind;
    }

    if files.iter().any(|file| {
        file.definitions
            .iter()
            .any(|definition| matches_callable_definition(definition, symbol))
    }) {
        return RefactorPlanTargetKind::Callable;
    }

    RefactorPlanTargetKind::Unknown
}

fn matches_target_kind_rule(
    definition: &ImpactDefinitionItem,
    symbol: &str,
    rule: &TargetKindRule,
) -> bool {
    matches_named_definition(definition, symbol)
        && rule
            .category
            .is_none_or(|category| definition.category == category)
        && rule
            .heads
            .contains(&normalize_common_lisp_operator_head(&definition.head))
}

fn matches_callable_definition(definition: &ImpactDefinitionItem, symbol: &str) -> bool {
    matches_named_definition(definition, symbol) && definition.category.is_callable()
}

fn matches_named_definition(definition: &ImpactDefinitionItem, symbol: &str) -> bool {
    definition
        .name
        .as_deref()
        .is_some_and(|name| common_lisp_symbol_reference_eq(name, symbol))
}
