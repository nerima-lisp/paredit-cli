//! One name introduced by one binding form.

use crate::domain::sexpr::{ByteSpan, SymbolName};

use super::facts::{BindingKind, OpacityCause, ScopeOpacity, SpecialBinding};
use super::ids::ScopeId;

/// A single binding: what was bound, where, and everything the value and type
/// layers need to decide whether they may say anything about it.
///
/// Fields are private and set through [`BindingDraft`] because several of them
/// are only knowable at different points of the build — references and
/// assignments accumulate while the scope is walked — and a half-filled
/// `Binding` must not escape the builder.
#[derive(Debug, Clone)]
pub struct Binding {
    name: SymbolName,
    kind: BindingKind,
    scope: ScopeId,
    definition: ByteSpan,
    binder_head: Option<String>,
    init_form: Option<ByteSpan>,
    references: Vec<ByteSpan>,
    assignments: Vec<ByteSpan>,
    special: SpecialBinding,
    opacity: ScopeOpacity,
    opacity_cause: Option<OpacityCause>,
}

impl Binding {
    /// The bound name, normalized by the dialect's rules (case-folded for
    /// Common Lisp).
    pub const fn name(&self) -> &SymbolName {
        &self.name
    }

    pub const fn kind(&self) -> BindingKind {
        self.kind
    }

    pub const fn scope(&self) -> ScopeId {
        self.scope
    }

    /// The span of the atom that spells the name in the binding form.
    pub const fn definition(&self) -> ByteSpan {
        self.definition
    }

    /// The head of the form that introduced it (`let`, `defun`, …), or `None`
    /// for a binding with no operator (a top-level definition's own name).
    pub fn binder_head(&self) -> Option<&str> {
        self.binder_head.as_deref()
    }

    /// The span of the initial-value form, when the binder has one.
    pub const fn init_form(&self) -> Option<ByteSpan> {
        self.init_form
    }

    /// Every reference that resolves to this binding, in source order. The
    /// defining occurrence is not a reference and is excluded.
    pub fn references(&self) -> &[ByteSpan] {
        &self.references
    }

    /// Every site that reassigns this binding (`setq`, `incf`, `push`, …).
    /// A non-empty list is what stops the value layer from propagating.
    pub fn assignments(&self) -> &[ByteSpan] {
        &self.assignments
    }

    pub const fn special(&self) -> SpecialBinding {
        self.special
    }

    pub const fn opacity(&self) -> ScopeOpacity {
        self.opacity
    }

    /// The first region that cost this binding's scope its transparency, in
    /// walk order, or `None` when the scope is transparent.
    ///
    /// Only the first is kept. A scope can enclose any number of opaque
    /// regions, and holding them all would grow a `Vec` per binding for
    /// evidence no lint rule reads; a corpus histogram wants exactly one
    /// attribution per binding anyway. "First in walk order" is the earliest
    /// such region in the scope, which is a well-defined choice rather than an
    /// arbitrary one.
    pub const fn opacity_cause(&self) -> Option<OpacityCause> {
        self.opacity_cause
    }

    /// Whether a constant value may be propagated through this binding.
    ///
    /// All four conditions must hold: never reassigned, no opaque region in
    /// scope, lexically bound, and an initial value to propagate. Any one of
    /// them failing means a caller could observe a different value than the
    /// initial form suggests, so the answer is no.
    pub fn is_propagatable(&self) -> bool {
        self.assignments.is_empty()
            && self.opacity.is_transparent()
            && self.special.is_lexical()
            && self.init_form.is_some()
    }
}

/// A binding under construction.
#[derive(Debug, Clone)]
pub struct BindingDraft {
    binding: Binding,
}

impl BindingDraft {
    pub const fn new(
        name: SymbolName,
        kind: BindingKind,
        scope: ScopeId,
        definition: ByteSpan,
    ) -> Self {
        Self {
            binding: Binding {
                name,
                kind,
                scope,
                definition,
                binder_head: None,
                init_form: None,
                references: Vec::new(),
                assignments: Vec::new(),
                special: SpecialBinding::Lexical,
                opacity: ScopeOpacity::Transparent,
                opacity_cause: None,
            },
        }
    }

    #[must_use]
    pub fn with_binder_head(mut self, head: impl Into<String>) -> Self {
        self.binding.binder_head = Some(head.into());
        self
    }

    #[must_use]
    pub const fn with_init_form(mut self, span: ByteSpan) -> Self {
        self.binding.init_form = Some(span);
        self
    }

    #[must_use]
    pub const fn with_special(mut self, special: SpecialBinding) -> Self {
        self.binding.special = special;
        self
    }

    pub fn push_reference(&mut self, span: ByteSpan) {
        self.binding.references.push(span);
    }

    pub fn push_assignment(&mut self, span: ByteSpan) {
        self.binding.assignments.push(span);
    }

    /// Records that the scope encloses a region this layer cannot see through.
    ///
    /// Opacity itself is absorbing, so repeating the observation is harmless;
    /// the cause is kept only the first time, which is what makes
    /// [`Binding::opacity_cause`] "the earliest such region".
    pub fn observe_opacity(&mut self, cause: OpacityCause) {
        self.binding.opacity = self
            .binding
            .opacity
            .join(ScopeOpacity::ContainsOpaqueRegion);
        self.binding.opacity_cause.get_or_insert(cause);
    }

    pub fn finish(self) -> Binding {
        self.binding
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::semantics::binding::model::facts::OpacityCauseKind;
    use crate::domain::sexpr::ByteOffset;

    fn span(start: usize, end: usize) -> ByteSpan {
        ByteSpan::new(ByteOffset::new(start), ByteOffset::new(end))
    }

    fn cause(start: usize, end: usize) -> OpacityCause {
        OpacityCause::new(OpacityCauseKind::UnknownHead, span(start, end))
    }

    fn draft() -> BindingDraft {
        BindingDraft::new(
            SymbolName::new("x").expect("symbol"),
            BindingKind::Variable,
            ScopeId::FILE,
            span(0, 1),
        )
        .with_binder_head("let")
        .with_init_form(span(2, 3))
    }

    #[test]
    fn a_plain_initialized_binding_propagates() {
        assert!(draft().finish().is_propagatable());
    }

    #[test]
    fn each_disqualifying_fact_blocks_propagation() {
        let mut reassigned = draft();
        reassigned.push_assignment(span(9, 10));
        assert!(!reassigned.finish().is_propagatable());

        let mut opaque = draft();
        opaque.observe_opacity(cause(30, 40));
        assert!(!opaque.finish().is_propagatable());

        let special = draft().with_special(SpecialBinding::DeclaredSpecial);
        assert!(!special.finish().is_propagatable());

        let uninitialized = BindingDraft::new(
            SymbolName::new("x").expect("symbol"),
            BindingKind::Variable,
            ScopeId::FILE,
            span(0, 1),
        );
        assert!(!uninitialized.finish().is_propagatable());
    }

    #[test]
    fn a_transparent_binding_names_no_cause() {
        assert_eq!(draft().finish().opacity_cause(), None);
    }

    #[test]
    fn the_first_observed_cause_is_the_one_kept() {
        // A scope can enclose many opaque regions; the histogram this feeds
        // wants one attribution per binding, and the earliest is the
        // well-defined choice.
        let mut binding = draft();
        binding.observe_opacity(cause(30, 40));
        binding.observe_opacity(OpacityCause::new(
            OpacityCauseKind::QuotedOrReadTime,
            span(50, 60),
        ));
        let binding = binding.finish();
        assert_eq!(binding.opacity_cause(), Some(cause(30, 40)));
        assert_eq!(binding.opacity(), ScopeOpacity::ContainsOpaqueRegion);
    }

    #[test]
    fn references_keep_the_order_they_were_recorded_in() {
        let mut binding = draft();
        binding.push_reference(span(10, 11));
        binding.push_reference(span(20, 21));
        let binding = binding.finish();
        assert_eq!(binding.references(), &[span(10, 11), span(20, 21)]);
    }
}
