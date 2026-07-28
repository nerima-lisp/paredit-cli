//! The single pass that turns a parsed file into a binding table.

use std::collections::HashSet;

use crate::lexical_scope::BoundName;
use paredit_core_syntax::common_lisp::{
    common_lisp_dynamic_binding_is_declared, common_lisp_reader_conditional_kind,
    common_lisp_reader_label_kind, is_common_lisp_declaration_form,
};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::emacs_lisp::EmacsLispOperator;
use paredit_core_syntax::sexpr::reader::apply_reader_prefix_context;
use paredit_core_syntax::sexpr::{
    ByteSpan, ExpressionKind, ExpressionView, SymbolName, SyntaxTree,
};

use super::super::model::{
    BindingDraft, BindingId, BindingKind, BindingTable, BindingTableBuilder, OpacityCause,
    OpacityCauseKind, ScopeId, SpecialBinding,
};
use super::emacs_lisp::EmacsLispDynamicScope;
use super::opacity::head_has_registered_semantics;
use super::scope_stack::{Namespace, ScopeStack, SymbolIdentity};
use super::special_names::SpecialNameIndex;

/// Builds the binding table for one parsed file.
///
/// Common Lisp, Emacs Lisp, Scheme and Racket are analysed; every other
/// dialect gets an empty table rather than a guessed one. The binding forms
/// differ enough that a shared traversal would have to assume, and this layer
/// records only what it can prove. An empty table reads as "nothing is
/// statically known", which every consumer already has to handle.
///
/// None of the three is a special case of the Common Lisp walk. Scheme is a
/// Lisp-1 with case-sensitive identifiers and no `declare`/`special`
/// machinery; Emacs Lisp is a Lisp-2 that is also case-sensitive, has no
/// package syntax, and decides whether a `let` is lexical from a comment on
/// line 1. So the dispatch, the name comparison, the namespace rule and the
/// dynamic-binding rule are all asked of the dialect rather than assumed.
///
/// `input` is read for Emacs Lisp, and only there: the `lexical-binding`
/// setting that decides what every `let` in the file means is a comment, so it
/// is absent from the tree. Elsewhere the parameter is only the assertion
/// below — pinning that the spans the table records index into the string the
/// caller will slice with.
///
/// `input` *is* read for Emacs Lisp, and only there: whether a plain `let`
/// binds lexically or dynamically in that dialect is decided by the
/// `lexical-binding` setting on the file's first line, which is a comment and
/// therefore absent from the tree. For Common Lisp the parameter is still only
/// the assertion below — pinning that the spans the table records index into
/// the string the caller will slice with.
#[must_use]
pub fn build_binding_table(dialect: Dialect, tree: &SyntaxTree, input: &str) -> BindingTable {
    let builder = BindingTableBuilder::new();
    if !matches!(
        dialect,
        Dialect::CommonLisp | Dialect::EmacsLisp | Dialect::Scheme | Dialect::Racket
    ) {
        return builder.finish();
    }

    let document = tree.root_view();
    debug_assert!(
        document.span.end().get() <= input.len(),
        "the tree must have been parsed from `input`, since every span the \
         table records is an index into it"
    );

    let mut walk = Walk {
        dialect,
        // Only Common Lisp has a way to declare a binding special, so the
        // scan is skipped entirely elsewhere rather than run to find nothing.
        specials: if dialect == Dialect::CommonLisp {
            SpecialNameIndex::scan(&document)
        } else {
            SpecialNameIndex::empty()
        },
        // Emacs Lisp answers the same question a different way, from the file
        // header and the `defvar`s in the file.
        dynamic_scope: EmacsLispDynamicScope::of(dialect, tree, input),
        document: &document,
        builder,
        stack: ScopeStack::new(dialect),
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
    /// Which language's rules the walk is applying. Decides the binder
    /// dispatch, name equality, whether the namespaces are separate, and
    /// whether `(declare ...)` means anything.
    pub(super) dialect: Dialect,
    /// Emacs Lisp's file-level answer to "does a plain `let` bind lexically?".
    /// Inert for every other dialect.
    pub(super) dynamic_scope: EmacsLispDynamicScope,
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
        if !head.is_some_and(|head| head_has_registered_semantics(self.dialect, head)) {
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
            if !started && self.is_declaration_form(form) {
                continue;
            }
            started = true;
            self.form(form, scope, 0);
        }
    }

    /// Whether `form` is a leading declaration this dialect does not evaluate.
    ///
    /// Common Lisp and Emacs Lisp both have one, and they are not the same
    /// form: Emacs Lisp spells it `(declare …)` or `(cl-declare …)` and reads
    /// the head case-sensitively. Either way the contents are metadata —
    /// `(declare (indent 1))` names no function `indent` — so walking into one
    /// marks every enclosing binding opaque on a head that is not a head.
    ///
    /// Scheme has neither, so a leading form spelled that way is an ordinary
    /// call and its symbols are ordinary references.
    fn is_declaration_form(&self, form: &ExpressionView) -> bool {
        let Some(head) = form.children.first().and_then(head_text) else {
            return false;
        };
        match self.dialect {
            Dialect::CommonLisp => is_common_lisp_declaration_form(head),
            Dialect::EmacsLisp => EmacsLispOperator::from_head(head)
                .is_some_and(EmacsLispOperator::is_declaration_form),
            _ => false,
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
        // binding special. See `SpecialBinding`. Scheme has no such
        // declaration at all, and its `specials` index is empty, so the
        // condition short-circuits to `Lexical` there.
        let special =
            if kind == BindingKind::Variable && self.binding_is_dynamic(target, head, &name) {
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

    /// Whether a variable binding of `name` by `head` is dynamically scoped.
    ///
    /// Two dialects reach the same conclusion by opposite routes. Common Lisp
    /// requires a `declaim`/`proclaim`/`defvar`/`declare` that names the
    /// symbol, and the walk verifies it reaches this binding. Emacs Lisp asks
    /// a file-level question first — without `lexical-binding: t` on line 1,
    /// *every* variable binding in the file is dynamic — and only then looks
    /// for a `defvar`. Scheme has no such concept at all, and its `specials`
    /// index is empty, so the Common Lisp path short-circuits to `false`.
    fn binding_is_dynamic(&self, target: &ExpressionView, head: &str, name: &SymbolName) -> bool {
        match self.dialect {
            Dialect::EmacsLisp => self
                .dynamic_scope
                .is_dynamic(EmacsLispOperator::from_head(head), name.as_str()),
            _ => {
                self.specials.may_be_declared_special(name)
                    && common_lisp_dynamic_binding_is_declared(self.document, target, name)
            }
        }
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
