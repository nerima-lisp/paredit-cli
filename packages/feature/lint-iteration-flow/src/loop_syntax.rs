//! A deliberately conservative reader for the `loop` macro's top-level tokens.
//!
//! `loop` is the one standard macro with a grammar of its own rather than an
//! S-expression shape, and that grammar is enormous. This module does **not**
//! implement it. It answers one narrow question — "for each top-level token of
//! this `loop` form, is it a clause keyword or an operand?" — and refuses to
//! answer at all whenever the question is not safely decidable.
//!
//! # Why the refusals matter
//!
//! Every clause keyword is an ordinary symbol, so `sum`, `count`, `do`, and
//! `it` are all legal variable names. `(loop for count from 1 to 3 collect
//! count)` is correct, idiomatic code in which the token `count` is a variable
//! twice and a keyword never. Reading it as an accumulation clause would make
//! every rule built on this module warn about working code. Three guards
//! prevent that:
//!
//! - **Bound names are never keywords.** A first pass collects every name
//!   introduced by `for`/`as`/`with`/`into`/`named`/`and`, including every
//!   symbol of a destructuring pattern such as `for (sum . rest) in alist`; a
//!   later pass classifies a token as a keyword only if its text is *not* one
//!   of those names. The first pass deliberately over-collects (it cannot know
//!   whether an `and` joins variable clauses or main clauses), which can only
//!   *lose* findings.
//! - **Operand positions are never keywords.** The bound-name guard alone
//!   covers only the name a clause *introduces*; a variable merely **read**
//!   after `to`, `below`, `from`, `=`, `in`, … is bound somewhere else
//!   entirely and the first pass never sees it. `(loop for i from 1 to count
//!   collect i)` is the everyday shape that needs this: `count` there is a
//!   parameter, not an accumulation clause. So the token following any of
//!   `OPERAND_INTRODUCERS` is forced to [`TokenRole::Operand`] regardless of
//!   its spelling. Like the first pass this over-collects, and like the first
//!   pass over-collecting can only lose findings.
//! - **An unmodelled sub-grammar aborts the whole form.** See below.
//!
//! A reader prefix on a token has the same effect: `'collect` is quoted data
//! and never the `collect` clause, so `symbol_word` refuses any prefixed
//! atom.
//!
//! # What this reader does not attempt
//!
//! [`read_loop_clauses`] returns `None` — meaning "report nothing about this
//! form" — for all of these:
//!
//! - **Iteration paths** (`being the hash-keys of h using (hash-value v)`).
//!   The `being` sub-grammar has its own token positions and its own name
//!   bindings, and mis-reading `the`, `each`, `of`, or a path noun as a clause
//!   keyword would be a false positive in exactly the code that is hardest to
//!   review. A form containing a top-level `being` is skipped entirely.
//! - **Simple loop** (`(loop (do-thing))`), which has no clauses at all.
//! - **Quoted or quasiquoted forms**, which are data or a macro template
//!   rather than a call. Only a reader prefix *on the `loop` form itself* is
//!   visible from a single node, so this check catches `'(loop …)` and nothing
//!   deeper. The `loop` nested inside `'(progn (loop …))` is excluded one
//!   layer up instead, by [`crate::support::for_each_evaluated_subview`],
//!   which every rule in this package walks with.
//! - **`of-type` type specifiers and compound `and` clauses** are tokenized
//!   but never interpreted; their operands simply stay operands. A
//!   destructuring pattern is read only far enough to harvest the names it
//!   binds — its shape is not modelled.
//! - **Nested clause structure.** `when`/`if`/`else`/`end` are recognised as
//!   keywords but their nesting is not tracked, so no rule here may depend on
//!   which conditional branch a clause sits in.
//!
//! Scope: Common Lisp only.

use paredit_core_syntax::sexpr::reader::atom_symbol_text;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView};
use paredit_core_syntax::view_query::{list_head, unqualified};

/// Clause keywords that open a clause of their own.
///
/// `named` is included even though it may appear only once and only first;
/// that restriction is a rule's business, not the tokenizer's.
const CLAUSE_KEYWORDS: [&str; 34] = [
    "named",
    "with",
    "for",
    "as",
    "initially",
    "finally",
    "do",
    "doing",
    "return",
    "collect",
    "collecting",
    "append",
    "appending",
    "nconc",
    "nconcing",
    "count",
    "counting",
    "sum",
    "summing",
    "maximize",
    "maximizing",
    "minimize",
    "minimizing",
    "if",
    "when",
    "unless",
    "else",
    "end",
    "while",
    "until",
    "repeat",
    "always",
    "never",
    "thereis",
];

/// Keywords that are part of a clause rather than the head of one.
const SUBCLAUSE_KEYWORDS: [&str; 17] = [
    "from", "upfrom", "downfrom", "to", "upto", "downto", "below", "above", "by", "in", "on", "=",
    "then", "across", "of-type", "into", "and",
];

/// Clause keywords after which the very next token names a variable rather
/// than being a keyword in its own right.
///
/// `and` is here because in a variable clause (`for x = 1 and y = 2`) the next
/// token is a name. In a main clause (`collect x and count y`) it is not, and
/// treating it as one over-collects — which only suppresses findings.
const NAME_INTRODUCERS: [&str; 6] = ["for", "as", "with", "into", "named", "and"];

/// Keywords after which the very next token is an operand — a form, a literal,
/// or a variable being *read* — and therefore cannot be a clause keyword
/// whatever it is spelled.
///
/// This is the guard the bound-name pass cannot provide. `count`, `end`,
/// `sum`, and `on` are ordinary variable names, and
/// `(loop for i from 1 to count …)` reads one of them in operand position
/// without ever binding it inside the `loop`.
///
/// Every keyword here takes exactly one operand form, so marking exactly the
/// next token is right rather than merely safe. `of-type`, `into`, and `and`
/// are deliberately absent — `into` and `and` are handled by
/// [`NAME_INTRODUCERS`] instead, and `of-type`'s operand is a type specifier
/// this reader does not model. `do`/`doing`/`initially`/`finally` are absent
/// because they take *one or more* forms, so "the next token" does not
/// delimit them.
const OPERAND_INTRODUCERS: [&str; 17] = [
    "from", "upfrom", "downfrom", "to", "upto", "downto", "below", "above", "by", "in", "on",
    "across", "=", "then", "repeat", "while", "until",
];

/// Clause keywords that may appear only after every variable clause.
///
/// This is narrower than the CLHS `main-clause` production, deliberately. The
/// grammar groups `while`, `until`, and `repeat` with `always`/`never`/
/// `thereis` as `termination-test`s, but implementations do not treat the two
/// halves alike: SBCL accepts `(loop repeat n for x = (random 10) collect x)`
/// and `(loop for x in l while (p x) for y = (f x) collect y)`, both stock
/// idioms, while rejecting the same shape written after `do` or `always`. What
/// actually closes the variable-clause section is *body code*, so only body
/// code is listed here.
///
/// `initially` and `finally` are absent for a second reason: the CLHS grammar
/// admits both as a `variable-clause` *and* as a `main-clause`, so neither one
/// closes the section either.
const BODY_CLAUSE_KEYWORDS: [&str; 25] = [
    "do",
    "doing",
    "return",
    "collect",
    "collecting",
    "append",
    "appending",
    "nconc",
    "nconcing",
    "count",
    "counting",
    "sum",
    "summing",
    "maximize",
    "maximizing",
    "minimize",
    "minimizing",
    "if",
    "when",
    "unless",
    "else",
    "end",
    "always",
    "never",
    "thereis",
];

/// Clause keywords that introduce a loop variable.
pub const VARIABLE_CLAUSE_KEYWORDS: [&str; 3] = ["with", "for", "as"];

/// Accumulation verbs that build a list.
pub const LIST_ACCUMULATORS: [&str; 6] = [
    "collect",
    "collecting",
    "append",
    "appending",
    "nconc",
    "nconcing",
];

/// Accumulation verbs that build a number.
///
/// `maximize`/`minimize` sit here with `sum`/`count` because the only conflict
/// this package reports is list-versus-number; see
/// [`accumulation_kind`].
pub const NUMBER_ACCUMULATORS: [&str; 8] = [
    "sum",
    "summing",
    "count",
    "counting",
    "maximize",
    "maximizing",
    "minimize",
    "minimizing",
];

#[must_use]
pub fn is_clause_keyword(word: &str) -> bool {
    CLAUSE_KEYWORDS.contains(&word)
}

#[must_use]
pub fn is_subclause_keyword(word: &str) -> bool {
    SUBCLAUSE_KEYWORDS.contains(&word)
}

/// Whether `word` opens body code, and so may appear only after every variable
/// clause. See `BODY_CLAUSE_KEYWORDS` for why this is narrower than the
/// CLHS `main-clause` production.
#[must_use]
pub fn is_body_clause_keyword(word: &str) -> bool {
    BODY_CLAUSE_KEYWORDS.contains(&word)
}

#[must_use]
pub fn is_variable_clause_keyword(word: &str) -> bool {
    VARIABLE_CLAUSE_KEYWORDS.contains(&word)
}

/// The result type an accumulation verb builds, or `None` for a non-verb.
///
/// Only two kinds are distinguished. CLHS forbids accumulating into one
/// variable with verbs of different *types*, and while implementations differ
/// on whether `sum` and `count` may share a target, every implementation
/// rejects mixing a list verb with a numeric one. Reporting only the
/// list-versus-number case is the difference between a rule that is right
/// everywhere and one that is right on SBCL.
#[must_use]
pub fn accumulation_kind(word: &str) -> Option<&'static str> {
    if LIST_ACCUMULATORS.contains(&word) {
        Some("list")
    } else if NUMBER_ACCUMULATORS.contains(&word) {
        Some("number")
    } else {
        None
    }
}

/// What one top-level token of a `loop` form was read as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenRole {
    /// A clause or sub-clause keyword.
    Keyword,
    /// Anything else: a variable name, a literal, a compound form.
    Operand,
}

/// One top-level token of a `loop` form.
#[derive(Debug)]
pub struct LoopToken<'a> {
    /// The token's index among the `loop` form's children (so `1` is the
    /// first token after the `loop` head itself).
    pub child_index: usize,
    pub view: &'a ExpressionView,
    /// The token's symbol text, lowercased and package-unqualified, for a
    /// token that reads as a bare symbol. `None` for a compound form, a
    /// string, or a number.
    pub word: Option<String>,
    pub role: TokenRole,
}

impl LoopToken<'_> {
    /// The token's text when it was read as a keyword, else `None`.
    #[must_use]
    pub fn keyword(&self) -> Option<&str> {
        match self.role {
            TokenRole::Keyword => self.word.as_deref(),
            TokenRole::Operand => None,
        }
    }

    /// The token's text when it was read as a bare-symbol operand — a variable
    /// name, typically — else `None`.
    #[must_use]
    pub fn operand_symbol(&self) -> Option<&str> {
        match self.role {
            TokenRole::Operand => self.word.as_deref(),
            TokenRole::Keyword => None,
        }
    }

    #[must_use]
    pub const fn span(&self) -> ByteSpan {
        self.view.span
    }
}

/// One `loop` form's top-level tokens, already classified.
#[derive(Debug)]
pub struct LoopClauses<'a> {
    pub tokens: Vec<LoopToken<'a>>,
}

impl<'a> LoopClauses<'a> {
    /// The token at `position`, where `position` indexes [`Self::tokens`].
    #[must_use]
    pub fn token(&self, position: usize) -> Option<&LoopToken<'a>> {
        self.tokens.get(position)
    }

    /// The bare-symbol operand immediately after `position`, if there is one.
    ///
    /// This is the shape every "keyword then name" clause fragment shares:
    /// `into acc`, `with v`, `named outer`.
    #[must_use]
    pub fn symbol_after(&self, position: usize) -> Option<&LoopToken<'a>> {
        let next = self.tokens.get(position + 1)?;
        next.operand_symbol().is_some().then_some(next)
    }
}

/// The token text of one child, lowercased and package-unqualified, for a
/// child that reads as a bare symbol.
///
/// A string literal's `text` retains its quotes and a number's its digits, so
/// neither can collide with a keyword. `unqualified` folds `cl:count` onto
/// `count`, which is the spelling a package-qualified codebase uses.
///
/// A reader prefix disqualifies the atom outright, exactly as it does in
/// [`crate::support::plain_variable_name`]: `'collect` is the *symbol*
/// `collect` passed as data, and `#'count` is a function object. Neither is
/// the clause keyword it is spelled like.
fn symbol_word(view: &ExpressionView) -> Option<String> {
    if !view.reader_prefixes.is_empty() {
        return None;
    }
    let text = atom_symbol_text(view)?;
    if text.is_empty() || text.starts_with('"') {
        return None;
    }
    Some(unqualified(text).to_ascii_lowercase())
}

/// Whether `view` is a `(loop …)` form, however its head is spelled.
///
/// Says nothing about whether this reader can model it; that is
/// [`read_loop_clauses`]'s answer.
#[must_use]
pub fn is_loop_form(view: &ExpressionView) -> bool {
    list_head(view).is_some_and(|head| unqualified(head).eq_ignore_ascii_case("loop"))
}

/// The denominator every `loop` rule in this package reports beside its
/// findings.
///
/// Two numbers rather than one, because they answer different questions. A
/// file with ten `loop` forms of which this reader modelled two has eight
/// forms nothing was looked for in, and a single "forms scanned" count would
/// present that as eight clean forms.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct LoopScan {
    /// Every `(loop …)` form encountered.
    pub loop_form_count: usize,
    /// Those of them [`read_loop_clauses`] agreed to read.
    pub modelled_loop_form_count: usize,
}

impl LoopScan {
    /// Records one `loop` form and reads it, or records it as unmodelled.
    pub fn read<'a>(&mut self, view: &'a ExpressionView) -> Option<LoopClauses<'a>> {
        if !is_loop_form(view) {
            return None;
        }
        self.loop_form_count += 1;
        let clauses = read_loop_clauses(view)?;
        self.modelled_loop_form_count += 1;
        Some(clauses)
    }
}

/// Every symbol in a destructuring pattern, appended to `names`.
///
/// `for (a (b . c)) in pairs` binds `a`, `b`, and `c`; any of them may be
/// spelled like a clause keyword, and every one of them must therefore stop
/// reading as one. Lambda-list markers such as `&optional` are collected too,
/// which is harmless: they are not clause keywords either.
fn collect_pattern_names(view: &ExpressionView, names: &mut Vec<String>) {
    if let Some(word) = symbol_word(view) {
        names.push(word);
    }
    for child in &view.children {
        collect_pattern_names(child, names);
    }
}

/// Reads one `loop` form's top-level tokens, or `None` when this reader
/// declines to model the form.
///
/// See the module doc for the complete list of declines. Callers must treat
/// `None` as "report nothing", never as "nothing found".
#[must_use]
pub fn read_loop_clauses<'a>(view: &'a ExpressionView) -> Option<LoopClauses<'a>> {
    if !is_loop_form(view) {
        return None;
    }
    // A quoted or quasiquoted `loop` is data or a macro template, not a call.
    if !view.reader_prefixes.is_empty() {
        return None;
    }

    let children = view.children.get(1..)?;
    let words: Vec<Option<String>> = children.iter().map(symbol_word).collect();

    // The `being` iteration-path sub-grammar is not modelled; see the module
    // doc. Bailing on the whole form is the only safe response, because the
    // path's own tokens would otherwise read as clause keywords.
    if words.iter().any(|word| word.as_deref() == Some("being")) {
        return None;
    }

    // A simple loop has no clauses at all, so nothing here applies to it.
    if !words
        .iter()
        .any(|word| word.as_deref().is_some_and(is_clause_keyword))
    {
        return None;
    }

    // Pass 1: every name a clause keyword introduces. Over-collecting is safe
    // (it only demotes a would-be keyword to an operand); under-collecting is
    // not.
    let mut bound_names: Vec<String> = Vec::new();
    for (index, word) in words.iter().enumerate() {
        let Some(word) = word.as_deref() else {
            continue;
        };
        if !NAME_INTRODUCERS.contains(&word) {
            continue;
        }
        // Written without a `let` chain: this workspace's 1.85 MSRV rejects
        // them even though edition 2024 makes them look available.
        match words.get(index + 1) {
            Some(Some(name)) => bound_names.push(name.clone()),
            // `for (k . v) in alist` destructures; every symbol in the pattern
            // is a variable name, and one of them may well be spelled like a
            // clause keyword.
            _ => {
                if let Some(pattern) = children.get(index + 1) {
                    collect_pattern_names(pattern, &mut bound_names);
                }
            }
        }
    }

    // Pass 2: every position holding an operand of the keyword before it. Like
    // pass 1 this reads raw token text without yet knowing what is a keyword,
    // and like pass 1 that over-collects in the safe direction: a position
    // wrongly marked here only demotes a would-be keyword to an operand.
    let mut operand_positions = vec![false; words.len()];
    for (index, word) in words.iter().enumerate() {
        let introduces_operand = word
            .as_deref()
            .is_some_and(|word| OPERAND_INTRODUCERS.contains(&word));
        if !introduces_operand {
            continue;
        }
        if let Some(slot) = operand_positions.get_mut(index + 1) {
            *slot = true;
        }
    }

    // Pass 3: classify. Neither a bound name nor an operand position ever
    // reads as a keyword.
    let tokens = children
        .iter()
        .enumerate()
        .zip(words.iter())
        .map(|((offset, child), word)| {
            let role = match word.as_deref() {
                Some(text)
                    if !operand_positions[offset]
                        && !bound_names.iter().any(|name| name == text)
                        && (is_clause_keyword(text) || is_subclause_keyword(text)) =>
                {
                    TokenRole::Keyword
                }
                _ => TokenRole::Operand,
            };
            LoopToken {
                child_index: offset + 1,
                view: child,
                word: word.clone(),
                role,
            }
        })
        .collect();

    Some(LoopClauses { tokens })
}

/// Whether `view` is a `(head …)` call to one of `heads`, unquoted.
#[must_use]
pub fn unquoted_call(view: &ExpressionView, heads: &[&str]) -> bool {
    view.reader_prefixes.is_empty()
        && list_head(view).is_some_and(|head| {
            let head = unqualified(head);
            heads.iter().any(|name| head.eq_ignore_ascii_case(name))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::{Path as SexprPath, SyntaxTree};

    /// Runs `check` over the first top-level form's clause reading.
    fn with_clauses<T>(input: &str, check: impl FnOnce(Option<LoopClauses<'_>>) -> T) -> T {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let view = tree
            .select_path(&SexprPath::root_child(0))
            .expect("root form")
            .view();
        check(read_loop_clauses(&view))
    }

    /// The keywords, in order, that a form reads as.
    fn keywords(input: &str) -> Option<Vec<String>> {
        with_clauses(input, |clauses| {
            clauses.map(|clauses| {
                clauses
                    .tokens
                    .iter()
                    .filter_map(|token| token.keyword().map(str::to_owned))
                    .collect()
            })
        })
    }

    #[test]
    fn reads_a_plain_extended_loop() {
        assert_eq!(
            keywords("(loop for x in items collect x)"),
            Some(vec![
                "for".to_owned(),
                "in".to_owned(),
                "collect".to_owned()
            ])
        );
    }

    /// The bound-name guard: a variable *introduced* by a clause and spelled
    /// like a clause keyword must never read as one.
    ///
    /// This covers only the bound case. A name merely read in operand
    /// position needs the separate guard exercised by
    /// [`tests::a_variable_read_after_to_is_never_a_keyword`] — for a long
    /// time this test was the only one here, and the reader was wrong on
    /// `(loop for i from 1 to count …)` the whole time.
    #[test]
    fn a_variable_named_like_a_keyword_is_never_a_keyword() {
        assert_eq!(
            keywords("(loop for count from 1 to 3 collect count)"),
            Some(vec![
                "for".to_owned(),
                "from".to_owned(),
                "to".to_owned(),
                "collect".to_owned(),
            ])
        );
    }

    /// A destructured name spelled like a clause keyword must not read as one
    /// either.
    #[test]
    fn a_destructured_variable_named_like_a_keyword_is_never_a_keyword() {
        assert_eq!(
            keywords("(loop for (sum . rest) in alist collect sum)"),
            Some(vec![
                "for".to_owned(),
                "in".to_owned(),
                "collect".to_owned()
            ])
        );
    }

    #[test]
    fn declines_an_iteration_path_loop() {
        assert_eq!(
            keywords("(loop for k being the hash-keys of table collect k)"),
            None
        );
    }

    #[test]
    fn declines_a_simple_loop() {
        assert_eq!(keywords("(loop (do-thing))"), None);
    }

    #[test]
    fn declines_a_quoted_loop() {
        assert_eq!(keywords("'(loop for x in items collect x)"), None);
    }

    #[test]
    fn declines_a_quasiquoted_loop() {
        assert_eq!(keywords("`(loop for x in items collect x)"), None);
    }

    #[test]
    fn declines_a_non_loop_form() {
        assert_eq!(keywords("(dolist (x items) (print x))"), None);
    }

    #[test]
    fn a_package_qualified_head_and_keyword_still_read() {
        assert_eq!(
            keywords("(cl:loop for x in items cl:collect x)"),
            Some(vec![
                "for".to_owned(),
                "in".to_owned(),
                "collect".to_owned()
            ])
        );
    }

    #[test]
    fn a_string_operand_is_never_a_keyword() {
        // "collect" as a string is data, not a clause.
        assert_eq!(
            keywords("(loop for x in items do (print \"collect\"))"),
            Some(vec!["for".to_owned(), "in".to_owned(), "do".to_owned()])
        );
    }

    #[test]
    fn accumulation_kinds_split_list_from_number() {
        assert_eq!(accumulation_kind("collect"), Some("list"));
        assert_eq!(accumulation_kind("nconcing"), Some("list"));
        assert_eq!(accumulation_kind("sum"), Some("number"));
        assert_eq!(accumulation_kind("minimize"), Some("number"));
        assert_eq!(accumulation_kind("do"), None);
    }

    #[test]
    fn initially_and_finally_are_clauses_but_not_body_clauses() {
        assert!(is_clause_keyword("initially"));
        assert!(is_clause_keyword("finally"));
        assert!(!is_body_clause_keyword("initially"));
        assert!(!is_body_clause_keyword("finally"));
        assert!(is_body_clause_keyword("collect"));
        assert!(is_body_clause_keyword("thereis"));
    }

    /// `while`/`until`/`repeat` are clause keywords but do not close the
    /// variable-clause section; SBCL accepts a `for` after any of them.
    #[test]
    fn termination_tests_are_clauses_but_not_body_clauses() {
        for word in ["while", "until", "repeat"] {
            assert!(is_clause_keyword(word), "{word} must read as a clause");
            assert!(
                !is_body_clause_keyword(word),
                "{word} must not close the variable-clause section"
            );
        }
        // The other half of the CLHS `termination-test` group *does* close it.
        assert!(is_body_clause_keyword("always"));
        assert!(is_body_clause_keyword("never"));
    }

    /// A variable merely *read* in operand position is never a keyword, even
    /// though nothing in the `loop` binds it. `(defun v1 (count) …)` binds it
    /// outside, and this reader never sees that.
    #[test]
    fn a_variable_read_after_to_is_never_a_keyword() {
        assert_eq!(
            keywords("(loop for i from 1 to count for j from 1 to 3 collect (list i j))"),
            Some(vec![
                "for".to_owned(),
                "from".to_owned(),
                "to".to_owned(),
                "for".to_owned(),
                "from".to_owned(),
                "to".to_owned(),
                "collect".to_owned(),
            ])
        );
    }

    #[test]
    fn a_variable_read_after_below_is_never_a_keyword() {
        assert_eq!(
            keywords("(loop for i from 0 below end for j from 0 below 3 collect (cons i j))"),
            Some(vec![
                "for".to_owned(),
                "from".to_owned(),
                "below".to_owned(),
                "for".to_owned(),
                "from".to_owned(),
                "below".to_owned(),
                "collect".to_owned(),
            ])
        );
    }

    #[test]
    fn a_variable_read_after_an_equals_sign_is_never_a_keyword() {
        assert_eq!(
            keywords("(loop with limit = count for x in items collect (* x limit))"),
            Some(vec![
                "with".to_owned(),
                "=".to_owned(),
                "for".to_owned(),
                "in".to_owned(),
                "collect".to_owned(),
            ])
        );
    }

    /// `repeat`, `while`, and `until` each take exactly one operand form, so
    /// the token after them is an operand however it is spelled.
    #[test]
    fn a_variable_read_after_repeat_or_while_is_never_a_keyword() {
        assert_eq!(
            keywords("(loop repeat count collect (random 10))"),
            Some(vec!["repeat".to_owned(), "collect".to_owned()])
        );
        assert_eq!(
            keywords("(loop for x in items while end collect x)"),
            Some(vec![
                "for".to_owned(),
                "in".to_owned(),
                "while".to_owned(),
                "collect".to_owned(),
            ])
        );
    }

    /// A quoted symbol is data. `'collect` passes the symbol `collect` along;
    /// it does not open an accumulation clause.
    #[test]
    fn a_quoted_symbol_operand_is_never_a_keyword() {
        assert_eq!(
            keywords("(loop for x in items collect 'collect)"),
            Some(vec![
                "for".to_owned(),
                "in".to_owned(),
                "collect".to_owned()
            ])
        );
        assert_eq!(
            keywords("(loop for x in items collect #'count)"),
            Some(vec![
                "for".to_owned(),
                "in".to_owned(),
                "collect".to_owned()
            ])
        );
    }
}
