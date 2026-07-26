//! The automatic rewrite a fixable rule attaches to a finding.

use crate::domain::sexpr::ByteSpan;

/// One byte region replaced by new text.
///
/// Spans come straight from the tree, so a fix never has to re-derive offsets
/// and can copy untouched sub-forms verbatim from the source — the discipline
/// that keeps formatting intact (see the module docs of
/// [`crate::domain::rename`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Replacement {
    span: ByteSpan,
    text: String,
}

impl Replacement {
    pub fn new(span: ByteSpan, text: impl Into<String>) -> Self {
        Self {
            span,
            text: text.into(),
        }
    }

    pub const fn span(&self) -> ByteSpan {
        self.span
    }

    pub fn text(&self) -> &str {
        &self.text
    }
}

/// An automatic fix: one or more byte-region edits applied together.
///
/// Most rules need a single edit; some (flipping `when`/`unless` while
/// unwrapping its negated test) need several disjoint ones. The first
/// replacement is stored separately so "a fix with no edits" — which would
/// count as an applied fix that changes nothing and stall the fixpoint loop —
/// cannot be built.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleFix {
    description: String,
    first: Replacement,
    rest: Vec<Replacement>,
}

impl RuleFix {
    /// The common case: replace one region.
    pub fn single(span: ByteSpan, text: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            description: description.into(),
            first: Replacement::new(span, text),
            rest: Vec::new(),
        }
    }

    /// Several disjoint edits applied as one fix.
    pub fn multi(
        description: impl Into<String>,
        first: Replacement,
        rest: impl IntoIterator<Item = Replacement>,
    ) -> Self {
        Self {
            description: description.into(),
            first,
            rest: rest.into_iter().collect(),
        }
    }

    pub fn description(&self) -> &str {
        &self.description
    }

    /// How many regions this fix edits. Always at least one.
    pub fn replacement_count(&self) -> usize {
        1 + self.rest.len()
    }

    /// Every replacement, in the order the rule produced them.
    pub fn replacements(&self) -> impl Iterator<Item = &Replacement> {
        std::iter::once(&self.first).chain(&self.rest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::sexpr::ByteOffset;

    fn span(start: usize, end: usize) -> ByteSpan {
        ByteSpan::new(ByteOffset::new(start), ByteOffset::new(end))
    }

    #[test]
    fn a_single_fix_has_exactly_one_replacement() {
        let fix = RuleFix::single(span(0, 3), "x", "Unwrap");
        assert_eq!(fix.replacement_count(), 1);
        assert_eq!(fix.description(), "Unwrap");
        assert_eq!(fix.replacements().next().expect("one edit").text(), "x");
    }

    #[test]
    fn a_multi_fix_preserves_replacement_order() {
        let fix = RuleFix::multi(
            "Flip",
            Replacement::new(span(1, 5), "unless"),
            [Replacement::new(span(7, 14), "ready")],
        );
        let texts: Vec<&str> = fix.replacements().map(Replacement::text).collect();
        assert_eq!(texts, vec!["unless", "ready"]);
    }
}
