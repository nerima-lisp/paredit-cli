//! The single pass that turns a parsed file into a binding table.

use std::collections::HashSet;

use crate::domain::common_lisp::{
    common_lisp_dynamic_binding_is_declared, common_lisp_reader_conditional_kind,
    common_lisp_reader_label_kind, is_common_lisp_declaration_form,
};
use crate::domain::dialect::Dialect;
use crate::domain::lexical_scope::BoundName;
use crate::domain::sexpr::reader::apply_reader_prefix_context;
use crate::domain::sexpr::{ByteSpan, ExpressionKind, ExpressionView, SymbolName, SyntaxTree};

use super::super::model::{
    BindingDraft, BindingId, BindingKind, BindingTable, BindingTableBuilder, OpacityCause,
    OpacityCauseKind, ScopeId, SpecialBinding,
};
use super::opacity::head_has_registered_semantics;
use super::scope_stack::{Namespace, ScopeStack};
use super::special_names::SpecialNameIndex;

/// Builds the binding table for one parsed file.
///
/// Only Common Lisp is analysed. Other dialects get an empty table rather than
/// a guessed one: the binding forms differ enough that a shared traversal
/// would have to assume, and this layer records only what it can prove. An
/// empty table reads as "nothing is statically known", which every consumer
/// already has to handle.
///
/// `input` is taken but not read: every fact the table records is a span, and
/// the assertion below is the whole of what the source text is good for here —
/// pinning that those spans index into the string the caller will slice with.
pub fn build_binding_table(dialect: Dialect, tree: &SyntaxTree, input: &str) -> BindingTable {
    let builder = BindingTableBuilder::new();
    if dialect != Dialect::CommonLisp {
        return builder.finish();
    }

    let document = tree.root_view();
    debug_assert!(
        document.span.end().get() <= input.len(),
        "the tree must have been parsed from `input`, since every span the \
         table records is an index into it"
    );

    let mut walk = Walk {
        specials: SpecialNameIndex::scan(&document),
        document: &document,
        builder,
        stack: ScopeStack::default(),
        definitions: HashSet::new(),
    };

    for child in &document.children {
        walk.form(child, ScopeId::FILE, 0);
    }

    walk.builder.finish()
}

/// The walk's state: the table under construction plus the bindings visible at
/// the current position.
pub(super) struct Walk<'a> {
    /// The whole document. The `special` scan needs it: a binding is dynamic
    /// because of a `declaim`/`proclaim`/`defvar` that may sit anywhere in the
    /// file, or a `declare` in an *enclosing* form.
    pub(super) document: &'a ExpressionView,
    /// Which names the file declares special anywhere. Scanned once so the
    /// per-binding check below can be skipped for a name no declaration
    /// mentions, which is almost all of them.
    specials: SpecialNameIndex,
    pub(super) builder: BindingTableBuilder,
    pub(super) stack: ScopeStack,
    /// Every atom span that spells a binding. A binder's own defining
    /// occurrence is not a reference to itself, and span identity is an exact
    /// test because `(span, kind)` pairs are unique across a tree.
    pub(super) definitions: HashSet<ByteSpan>,
}

impl Walk<'_> {
    /// Walks one form.
    ///
    /// Mirrors `collect_unshadowed_symbol_references_in_context` branch for
    /// branch. Descent has to match the reference query exactly, because the
    /// table is that query turned inside out and a differential test pins the
    /// two together.
    pub(super) fn form(&mut self, view: &ExpressionView, scope: ScopeId, quasiquote_depth: usize) {
        let Some(quasiquote_depth) = apply_reader_prefix_context(view, quasiquote_depth) else {
            // Two unrelated refusals hide behind that `None`, and they deserve
            // different answers.
            //
            // `#.` splices whatever the reader computed into the program *as
            // code*, so a scope containing one may contain an assignment that
            // is nowhere in the source. Nothing can be concluded there.
            //
            // A top-level quote is the opposite. Nothing inside `'…` is ever
            // evaluated, so it cannot reassign anything: `'(setq x 2)` is a
            // three-element list. Even a `#.` nested inside a quote only ever
            // produces more data. The walk still stops — there are no live
            // references in quoted data, which is why this arm exists — but
            // stopping and distrusting the scope are separate decisions, and
            // conflating them was costing roughly one opaque scope in ten on
            // a real corpus.
            if is_read_time_evaluated(view) {
                self.mark_opaque(OpacityCause::new(
                    OpacityCauseKind::QuotedOrReadTime,
                    view.span,
                ));
            }
            return;
        };

        if view.kind == ExpressionKind::Atom {
            self.atom(view, quasiquote_depth);
            return;
        }

        if self.reader_form(view, scope, quasiquote_depth) {
            return;
        }

        if quasiquote_depth > 0 {
            // Nothing inside a template is a live reference, but an unquote
            // further down drops back to depth 0, so the walk continues.
            for child in &view.children {
                self.form(child, scope, quasiquote_depth);
            }
            return;
        }

        if self.binder(view, scope) {
            return;
        }

        self.call(view, scope);
    }

    /// A form that binds nothing. It may still be a macro call that rebinds or
    /// reassigns, which is what the opacity mark records.
    fn call(&mut self, view: &ExpressionView, scope: ScopeId) {
        let first = view.children.first();
        let head = first.and_then(head_text);

        // A computed head (`((lambda ...) x)`) is as unreadable as an unknown
        // macro, so both land here. They are told apart in the cause: only an
        // unknown *name* is something a transparency table could ever fix.
        if !head.is_some_and(head_has_registered_semantics) {
            let cause = match (head, first) {
                (Some(_), Some(atom)) => {
                    OpacityCause::new(OpacityCauseKind::UnknownHead, atom.span)
                }
                _ => OpacityCause::new(OpacityCauseKind::UnreadableHead, view.span),
            };
            self.mark_opaque(cause);
        }

        if let Some(head) = head {
            self.record_assignments(view, head);
        }

        for (index, child) in view.children.iter().enumerate() {
            // Head position reads the function namespace; every other
            // position reads the value namespace.
            if index == 0 && child.kind == ExpressionKind::Atom {
                match apply_reader_prefix_context(child, 0) {
                    Some(depth) => self.atom_in(child, depth, Namespace::Function),
                    None => self.mark_opaque(OpacityCause::new(
                        OpacityCauseKind::QuotedOrReadTime,
                        child.span,
                    )),
                }
                continue;
            }
            self.form(child, scope, 0);
        }
    }

    /// Walks body forms, skipping the leading `(declare ...)` forms exactly as
    /// `lexical_scope::traversal::body` does: a declaration is not evaluated,
    /// so the symbols in it are not references.
    pub(super) fn body(&mut self, forms: &[ExpressionView], scope: ScopeId) {
        let mut started = false;
        for form in forms {
            if !started
                && form
                    .children
                    .first()
                    .and_then(head_text)
                    .is_some_and(is_common_lisp_declaration_form)
            {
                continue;
            }
            started = true;
            self.form(form, scope, 0);
        }
    }

    /// Records that every binding currently in scope encloses a region this
    /// layer cannot see through, and what that region was.
    pub(super) fn mark_opaque(&mut self, cause: OpacityCause) {
        for id in self.stack.visible_ids() {
            self.builder.draft_mut(id).observe_opacity(cause);
        }
    }

    /// Registers one bound name and makes it visible to the rest of the walk.
    ///
    /// `target` is the binding form itself, which the `special` scan needs so
    /// it can see a `(declare (special ...))` at the head of that form's body.
    pub(super) fn declare(
        &mut self,
        target: &ExpressionView,
        scope: ScopeId,
        head: &str,
        bound: &BoundName,
        kind: BindingKind,
        init_form: Option<ByteSpan>,
    ) -> Option<BindingId> {
        let name = SymbolName::new(bound.name.clone()).ok()?;

        // A naming convention is not a proof; only a real declaration makes a
        // binding special. See `SpecialBinding`.
        let special = if kind == BindingKind::Variable
            && self.specials.may_be_declared_special(&name)
            && common_lisp_dynamic_binding_is_declared(self.document, target, &name)
        {
            SpecialBinding::DeclaredSpecial
        } else {
            SpecialBinding::Lexical
        };

        let mut draft = BindingDraft::new(name, kind, scope, bound.span)
            .with_binder_head(head)
            .with_special(special);
        if let Some(init_form) = init_form {
            draft = draft.with_init_form(init_form);
        }

        let id = self.builder.push_binding(draft);
        self.definitions.insert(bound.span);
        self.stack.push(id, &bound.name, kind);
        Some(id)
    }
}

/// An atom's text, or `None` when it carries a reader prefix.
///
/// Mirrors `lexical_scope::syntax::atom_text`. A prefixed atom is never an
/// operator head: reading `#'(setf x)` as the head `setf` would turn a
/// function designator into a binding form.
pub(super) fn head_text(view: &ExpressionView) -> Option<&str> {
    (view.kind == ExpressionKind::Atom && view.reader_prefixes.is_empty())
        .then_some(view.text.as_deref())
        .flatten()
}

/// Whether the reader evaluates this form's text while reading it.
///
/// True for `#.` alone. That is the one prefix whose result re-enters the
/// program as code, which is what separates it from quotation: `'x` yields a
/// datum no matter what produced it.
pub(super) fn is_read_time_evaluated(view: &ExpressionView) -> bool {
    view.reader_prefixes
        .iter()
        .any(|prefix| prefix.is_opaque_reader_form())
}

/// Whether an atom is a reader dispatch rather than a symbol.
///
/// `#+sbcl`, `#-sbcl`, `#1=`, and `#1#` govern the datum after them, and in a
/// legacy tree they sit as ordinary sibling atoms. Reading one as a symbol
/// would invent a reference.
pub(super) fn is_reader_dispatch(view: &ExpressionView) -> bool {
    common_lisp_reader_conditional_kind(view).is_some()
        || common_lisp_reader_label_kind(view).is_some()
}
