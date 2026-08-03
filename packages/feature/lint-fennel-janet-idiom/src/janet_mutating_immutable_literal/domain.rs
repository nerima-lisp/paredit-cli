//! `janet-mutating-immutable-literal` detection: a Janet mutation applied to a
//! literal Janet cannot mutate.
//!
//! Janet pairs every mutable container with an immutable twin and separates
//! them by exactly one character, the `@` prefix: `@[…]` array / `[…]` tuple,
//! `@{…}` table / `{…}` struct, `@"…"` buffer / `"…"` string. Dropping the `@`
//! is a one-keystroke mistake that produces code which reads correctly and
//! panics on the first call: `janet_put` ends its `switch` with
//! `janet_panicf("expected %T, got %v", JANET_TFLAG_ARRAY | JANET_TFLAG_BUFFER
//! | JANET_TFLAG_TABLE, ds)` (`src/core/value.c:764-769`), and the `array/*`
//! and `buffer/*` families type-check their first argument the same way.
//!
//! The rule only fires on a literal written at the call site, where the type is
//! decided by the source text and nothing else can change it. A symbol, a call,
//! or anything else in the target position is never judged — the value it holds
//! is exactly the question the rule declines to answer.

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, Delimiter, ExpressionKind, ExpressionView, SyntaxTree};

use crate::support::{head_symbol, is_immutable_janet_literal};

pub const DIALECTS: [Dialect; 1] = [Dialect::Janet];

/// What a mutating operator requires of its first argument.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Requirement {
    /// `put`/`update` and friends: array, buffer, or table.
    MutableDataStructure,
    /// `array/*`: an array specifically.
    Array,
    /// `buffer/*`: a buffer specifically.
    Buffer,
}

impl Requirement {
    #[must_use]
    pub const fn describe(self) -> &'static str {
        match self {
            Self::MutableDataStructure => "an array, buffer, or table",
            Self::Array => "an array",
            Self::Buffer => "a buffer",
        }
    }

    /// How to spell the mutable twin of the literal that was written.
    #[must_use]
    pub const fn remedy(self) -> &'static str {
        match self {
            Self::Buffer => "write the buffer literal `@\"…\"`",
            _ => "prefix the literal with `@`",
        }
    }
}

/// The operators this rule knows, each with what it needs.
///
/// Restricted to the ones whose first argument is unambiguously the mutated
/// container. `array/concat`'s later arguments may be tuples and that is legal,
/// so only index 1 is ever inspected.
pub const MUTATORS: [(&str, Requirement); 17] = [
    ("put", Requirement::MutableDataStructure),
    ("put-in", Requirement::MutableDataStructure),
    ("update", Requirement::MutableDataStructure),
    ("update-in", Requirement::MutableDataStructure),
    ("array/push", Requirement::Array),
    ("array/pop", Requirement::Array),
    ("array/concat", Requirement::Array),
    ("array/insert", Requirement::Array),
    ("array/remove", Requirement::Array),
    ("array/fill", Requirement::Array),
    ("array/clear", Requirement::Array),
    ("array/ensure", Requirement::Array),
    ("array/trim", Requirement::Array),
    ("buffer/push", Requirement::Buffer),
    ("buffer/push-string", Requirement::Buffer),
    ("buffer/clear", Requirement::Buffer),
    ("buffer/format", Requirement::Buffer),
];

/// The requirement `head` imposes, if it is one of the known mutators.
#[must_use]
pub fn requirement_for(head: &str) -> Option<Requirement> {
    MUTATORS
        .iter()
        .find(|(name, _)| *name == head)
        .map(|(_, requirement)| *requirement)
}

/// The literal kind that was written, for the message.
#[must_use]
const fn literal_name(view: &ExpressionView) -> &'static str {
    match view.kind {
        ExpressionKind::List => match view.delimiter {
            Some(Delimiter::Bracket) => "a tuple literal",
            Some(Delimiter::Brace) => "a struct literal",
            _ => "an immutable literal",
        },
        _ => "a string literal",
    }
}

/// One mutation applied to something that cannot be mutated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImmutableMutation {
    pub span: ByteSpan,
    pub target_span: ByteSpan,
    pub head: String,
    pub requirement: Requirement,
    pub literal: &'static str,
}

/// Examines one form.
#[must_use]
pub fn examine(dialect: Dialect, view: &ExpressionView) -> Option<ImmutableMutation> {
    if !DIALECTS.contains(&dialect) {
        return None;
    }
    let head = head_symbol(view)?;
    let requirement = requirement_for(head)?;
    let target = view.children.get(1)?;
    if !is_immutable_janet_literal(target) {
        return None;
    }
    Some(ImmutableMutation {
        span: view.span,
        target_span: target.span,
        head: head.to_owned(),
        requirement,
        literal: literal_name(target),
    })
}

/// Every such mutation in one file.
#[must_use]
pub fn collect(dialect: Dialect, tree: &SyntaxTree) -> Vec<ImmutableMutation> {
    let root = tree.root_view();
    let mut found = Vec::new();
    let mut stack: Vec<&ExpressionView> = root.children.iter().collect();
    while let Some(view) = stack.pop() {
        if let Some(item) = examine(dialect, view) {
            found.push(item);
        }
        stack.extend(view.children.iter());
    }
    found.sort_by_key(|item| item.span.start().get());
    found
}

/// Every call to a known mutator, judged or not. The denominator.
#[must_use]
pub fn candidate_count(dialect: Dialect, tree: &SyntaxTree) -> usize {
    if !DIALECTS.contains(&dialect) {
        return 0;
    }
    let root = tree.root_view();
    let mut count = 0;
    let mut stack: Vec<&ExpressionView> = root.children.iter().collect();
    while let Some(view) = stack.pop() {
        if head_symbol(view).is_some_and(|head| requirement_for(head).is_some()) {
            count += 1;
        }
        stack.extend(view.children.iter());
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    fn heads(source: &str) -> Vec<String> {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::Janet).expect("parse");
        collect(Dialect::Janet, &tree)
            .into_iter()
            .map(|item| item.head)
            .collect()
    }

    #[test]
    fn flags_put_on_a_struct_literal() {
        assert_eq!(heads("(put {:a 1} :b 2)"), vec!["put"]);
    }

    #[test]
    fn flags_array_push_on_a_tuple_literal() {
        assert_eq!(heads("(array/push [1 2 3] 4)"), vec!["array/push"]);
    }

    #[test]
    fn flags_buffer_push_on_a_string_literal() {
        assert_eq!(heads("(buffer/push \"seed\" 65)"), vec!["buffer/push"]);
    }

    #[test]
    fn the_at_prefix_is_the_whole_difference() {
        assert!(heads("(put @{:a 1} :b 2)").is_empty());
        assert!(heads("(array/push @[1 2 3] 4)").is_empty());
        assert!(heads("(buffer/push @\"seed\" 65)").is_empty());
    }

    #[test]
    fn a_symbol_or_a_call_in_the_target_is_never_judged() {
        assert!(heads("(put state :b 2)").is_empty());
        assert!(heads("(array/push (make-buf) 4)").is_empty());
    }

    #[test]
    fn only_the_first_argument_is_inspected() {
        // Concatenating a tuple *into* an array is correct Janet.
        assert!(heads("(array/concat @[1] [2 3])").is_empty());
    }

    #[test]
    fn an_unrelated_head_is_not_a_mutator() {
        assert!(heads("(get {:a 1} :a)").is_empty());
        assert!(heads("(length [1 2 3])").is_empty());
        assert!(heads("(string/split \",\" \"a,b\")").is_empty());
    }

    #[test]
    fn other_dialects_are_out_of_scope() {
        // Fennel has no `@` prefix and no `put`; `[1 2 3]` there is a mutable
        // Lua table, so the same source is correct code.
        let tree =
            SyntaxTree::parse_with_dialect("(put {:a 1} :b 2)", Dialect::Fennel).expect("parse");
        assert!(collect(Dialect::Fennel, &tree).is_empty());
    }

    #[test]
    fn the_candidate_count_counts_every_mutator_call() {
        let tree = SyntaxTree::parse_with_dialect(
            "(put @{} :a 1) (put {} :a 1) (get {} :a)",
            Dialect::Janet,
        )
        .expect("parse");
        assert_eq!(candidate_count(Dialect::Janet, &tree), 2);
        assert_eq!(collect(Dialect::Janet, &tree).len(), 1);
    }

    #[test]
    fn the_message_names_which_twin_was_written() {
        let tree = SyntaxTree::parse_with_dialect("(put [1] 0 2)", Dialect::Janet).expect("parse");
        let found = collect(Dialect::Janet, &tree);
        assert_eq!(found[0].literal, "a tuple literal");
        assert_eq!(found[0].requirement, Requirement::MutableDataStructure);
    }
}
