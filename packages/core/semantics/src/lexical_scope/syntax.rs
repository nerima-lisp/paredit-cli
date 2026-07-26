use crate::domain::sexpr::reader::{
    atom_symbol_span as sexpr_atom_symbol_span, atom_symbol_text as sexpr_atom_symbol_text,
};
use crate::domain::sexpr::{ByteSpan, ExpressionKind, ExpressionView};

pub(super) fn atom_text(view: &ExpressionView) -> Option<&str> {
    (view.kind == ExpressionKind::Atom && view.reader_prefixes.is_empty())
        .then_some(view.text.as_deref())
        .flatten()
}

pub(super) fn atom_symbol_text(view: &ExpressionView) -> Option<&str> {
    sexpr_atom_symbol_text(view)
}

pub(super) fn atom_symbol_span(view: &ExpressionView) -> Option<ByteSpan> {
    sexpr_atom_symbol_span(view)
}
