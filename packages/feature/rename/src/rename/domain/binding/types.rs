use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, SymbolName};

use super::rewrite::rewrite_clojure_keys_map_pattern;

#[derive(Debug, Clone)]
pub struct BindingRenameParts {
    pub form: String,
    pub form_span: ByteSpan,
    pub binding_span: ByteSpan,
    pub binding_edit: BindingEdit,
    pub reference_spans: Vec<ByteSpan>,
    pub shadowed_scope_count: usize,
}

#[derive(Debug, Clone)]
pub struct BindingGroup {
    pub names: Vec<ParameterNameSpan>,
    pub value: Option<ExpressionView>,
}

#[derive(Debug, Clone)]
pub struct ParameterNameSpan {
    pub name: String,
    pub name_span: ByteSpan,
    pub binding_edit: BindingEdit,
}

#[derive(Debug, Clone)]
pub struct BindingEdit {
    pub span: ByteSpan,
    kind: BindingEditKind,
}

#[derive(Debug, Clone)]
enum BindingEditKind {
    RenameAtom,
    RewriteBareSlotSpec {
        slot_name: String,
    },
    RewriteClojureKeysMap {
        map_pattern: ExpressionView,
        renamed_name: String,
    },
}

impl BindingEdit {
    pub const fn rename_atom(span: ByteSpan) -> Self {
        Self {
            span,
            kind: BindingEditKind::RenameAtom,
        }
    }

    pub const fn bare_slot_spec(span: ByteSpan, slot_name: String) -> Self {
        Self {
            span,
            kind: BindingEditKind::RewriteBareSlotSpec { slot_name },
        }
    }

    pub const fn clojure_keys_map(
        map_pattern: ExpressionView,
        span: ByteSpan,
        renamed_name: String,
    ) -> Self {
        Self {
            span,
            kind: BindingEditKind::RewriteClojureKeysMap {
                map_pattern,
                renamed_name,
            },
        }
    }

    pub fn replacement(&self, input: &str, to: &SymbolName) -> String {
        match &self.kind {
            BindingEditKind::RenameAtom => to.as_str().to_owned(),
            BindingEditKind::RewriteBareSlotSpec { slot_name } => {
                format!("({} {})", to.as_str(), slot_name)
            }
            BindingEditKind::RewriteClojureKeysMap {
                map_pattern,
                renamed_name,
            } => rewrite_clojure_keys_map_pattern(input, map_pattern, renamed_name, to),
        }
    }
}
