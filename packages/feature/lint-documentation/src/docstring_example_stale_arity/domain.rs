//! A worked example inside a docstring that calls the very function it
//! documents with an argument count that function's lambda list rejects.
//!
//! ```text
//! (defun scale (factor)          ; was (defun scale (x factor) …)
//!   "Scale X by FACTOR.
//!
//! Example: (scale 3 2) => 6"
//!   …)
//! ```
//!
//! The example is the part of a docstring a reader trusts most and the part
//! nothing checks. It survives every rename and every signature change this
//! tool performs — string contents are deliberately never rewritten — and no
//! compiler, no test, and no other rule in this suite will ever read it. A
//! stale example is a confident wrong answer that costs its reader a debugging
//! session.
//!
//! Unlike almost everything else about a docstring, this is *decidable*: the
//! example is a parenthesized call, the lambda list is right there, and
//! `DefinitionShape::lambda_parameter_arity` already answers "what argument
//! counts does this accept" in the language's own terms — separating required
//! from `&optional`/`&key` and recognising that `&rest`/`&body` removes the
//! upper bound.
//!
//! # What this is not
//!
//! Not a general docstring/parameter agreement check — that already exists in
//! `paredit-feature-code-metrics`'s `docstring_report`, which compares the
//! upper-cased *words* of a docstring against the lambda list. This reads
//! *calls*, and reads them for arity rather than for naming.
//!
//! Not a check of examples that call anything else. `(scale (double x) 2)`
//! says nothing checkable about `double`, whose lambda list is in another file.
//!
//! # Limits, deliberately
//!
//! Every one of these is a false *negative* bought to avoid a false positive.
//!
//! - **Only calls to the definition's own name**, matched unqualified and
//!   case-insensitively.
//! - **Only `defun` and `defmacro`.** A `defmethod`'s lambda list is one of
//!   several congruent ones on a generic function, so an example written
//!   against the generic may legitimately not match the method in hand.
//! - **A placeholder anywhere in the example silences it.** `(scale x …)`,
//!   `(scale 1 2 ...)`, `(scale &rest args)` and friends are illustrations of
//!   a shape, not calls with a countable arity.
//! - **A `&key` or `&allow-other-keys` lambda list is checked for
//!   under-supply only.** The upper bound of a keyword lambda list is not a
//!   number: `&allow-other-keys`, and keywords a `&rest` forwards, both make
//!   an over-long call legal.
//! - **A lambda list containing a reader conditional is not checked at all.**
//!   `#+sbcl` folds into a single atom the arity model would count as a
//!   parameter, and the real arity is build-dependent.
//! - **An unbalanced example is skipped**, since a truncated docstring says
//!   nothing about arity.

use paredit_core_syntax::definition::definition_shape;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionKind, ExpressionView};
use paredit_core_syntax::view_query::{
    atom_text, is_paren_list, list_head, symbol_in, unqualified,
};

use crate::support::{
    DocstringPlace, docstring_view_of, has_child_string_literal, string_literal_text,
};

/// One example call whose argument count the lambda list rejects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaleExample {
    /// The docstring literal's span. The example itself lives *inside* a string
    /// atom, so it has no span of its own in the tree; pointing at the
    /// docstring is the finest honest granularity.
    pub span: ByteSpan,
    /// The definition's name, as written.
    pub name: String,
    /// The example call, verbatim.
    pub example: String,
    /// How many arguments the example supplies.
    pub supplied: usize,
    /// The fewest arguments the lambda list accepts.
    pub minimum: usize,
    /// The most it accepts, or `None` when unbounded.
    pub maximum: Option<usize>,
}

impl StaleExample {
    /// The sentence the rule reports.
    #[must_use]
    pub fn message(&self) -> String {
        let accepts = match self.maximum {
            Some(maximum) if maximum == self.minimum => format!("exactly {maximum}"),
            Some(maximum) => format!("{} to {maximum}", self.minimum),
            None => format!("at least {}", self.minimum),
        };
        format!(
            "the docstring example {} calls {} with {} argument(s), but its lambda list accepts \
             {accepts}",
            self.example, self.name, self.supplied
        )
    }
}

/// Examines one definition and reports every stale example in its docstring.
#[must_use]
pub fn examine(view: &ExpressionView) -> Vec<StaleExample> {
    read(view).unwrap_or_default()
}

fn read(view: &ExpressionView) -> Option<Vec<StaleExample>> {
    if !is_paren_list(view) {
        return None;
    }
    let head = list_head(view)?;
    if !symbol_in(head, &["defun", "defmacro"]) {
        return None;
    }
    // The one question about a docstring that costs nothing: an example is a
    // *parenthesized* call, so a docstring containing no `(` holds none,
    // whatever the lambda list below would have said. Asked of raw source,
    // because unescaping neither introduces a paren nor removes one — `\(` is
    // still a `(` — so the raw bytes and the unescaped text agree exactly.
    //
    // Asked of every direct child rather than of the docstring, because
    // finding *the* docstring means building a `DefinitionShape` first, and the
    // docstring is always a direct child.
    //
    // Everything below allocates: a `DefinitionShape`, two `String`s for the
    // name, and the unescaped docstring. On `clean/forms/*` — a file of
    // definitions whose docstrings are ordinary prose — this guard is what
    // keeps all of it unpaid.
    if !has_child_string_literal(view, |literal| literal.contains('(')) {
        return None;
    }

    let shape = definition_shape(Dialect::CommonLisp, view, head)?;
    let docstring_view = docstring_view_of(shape, DocstringPlace::BodyHead, view)?;
    let name = shape.name(view)?.to_owned();
    // An empty or qualifier-only name cannot head a call.
    let bare = unqualified(&name).to_ascii_lowercase();
    if bare.is_empty() {
        return None;
    }

    let parameters = shape.lambda_parameters(view)?;
    // A reader conditional folds into one atom the arity model counts as a
    // parameter, and the real arity is build-dependent either way.
    if parameters.iter().any(is_reader_conditional) {
        return None;
    }
    let keyword_lambda_list = parameters.iter().any(|parameter| {
        atom_text(parameter).is_some_and(|text| {
            text.eq_ignore_ascii_case("&key") || text.eq_ignore_ascii_case("&allow-other-keys")
        })
    });

    let (minimum, maximum) = shape.lambda_parameter_arity(view)?;
    // With `&key` in play the upper bound is not a number: `&allow-other-keys`
    // and forwarded keywords both make a longer call legal. Under-supply is
    // still decidable, so only that half is checked.
    let maximum = if keyword_lambda_list { None } else { maximum };

    let docstring = string_literal_text(docstring_view)?;

    let mut findings = Vec::new();
    for example in examples_calling(&docstring, &bare) {
        let Some(supplied) = example.argument_count else {
            continue;
        };
        let too_few = supplied < minimum;
        let too_many = maximum.is_some_and(|maximum| supplied > maximum);
        if too_few || too_many {
            findings.push(StaleExample {
                span: docstring_view.span,
                name: name.clone(),
                example: example.text,
                supplied,
                minimum,
                maximum,
            });
        }
    }
    Some(findings)
}

/// A reader-conditional atom (`#+feature`/`#-feature`), which the dialect-aware
/// reader folds together with what follows it into a single atom.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    view.kind == ExpressionKind::Atom
        && atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

/// One parenthesized call found in a docstring.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Example {
    text: String,
    /// `None` when the example carries a placeholder and so has no countable
    /// arity.
    argument_count: Option<usize>,
}

/// Tokens that mark an example as an illustration of a *shape* rather than a
/// call with a countable argument count.
///
/// Deliberately generous. Missing one costs a false positive on a docstring
/// that is doing exactly what a docstring should; including one too many costs
/// a missed finding on an example somebody wrote oddly.
fn is_placeholder(token: &str) -> bool {
    token.starts_with('&')
        || token.starts_with("...")
        || token.ends_with("...")
        || token.contains('…')
        || token == "*"
        || token == "etc"
        || token == "etc."
        || token.eq_ignore_ascii_case("args")
        || token.eq_ignore_ascii_case("rest")
        || token.eq_ignore_ascii_case("more")
}

/// Every balanced parenthesized form in `text` whose head symbol is `name`.
///
/// A hand-rolled scan rather than a re-parse, because a docstring's contents
/// are prose with forms embedded in it: `Example: (scale 3 2) => 6.` is not a
/// Lisp file and handing it to the reader would fail on the prose. The scan
/// only has to be right about three things — nesting, string literals, and
/// character literals — and each of those is what a naive scan gets wrong.
fn examples_calling(text: &str, name: &str) -> Vec<Example> {
    // The overwhelmingly common docstring never names its own function at all.
    // Settling that with a byte scan keeps the `Vec<char>` below — the only
    // allocation on this path — off every such docstring.
    if !mentions(text, name) {
        return Vec::new();
    }
    let characters: Vec<char> = text.chars().collect();
    let mut examples = Vec::new();
    let mut index = 0;

    while index < characters.len() {
        if characters[index] != '(' {
            index += 1;
            continue;
        }
        match scan_form(&characters, index) {
            Some((end, tokens)) => {
                if tokens
                    .first()
                    .is_some_and(|head| unqualified(head).eq_ignore_ascii_case(name))
                {
                    let arguments = &tokens[1..];
                    examples.push(Example {
                        text: characters[index..end].iter().collect(),
                        argument_count: arguments
                            .iter()
                            .all(|token| !is_placeholder(token))
                            .then_some(arguments.len()),
                    });
                }
                // Continue *inside* the form rather than past it, so a nested
                // `(scale …)` in `(list (scale 1) (scale 2))` is still seen.
                index += 1;
            }
            // Unbalanced from here on: a truncated docstring says nothing about
            // arity.
            None => index += 1,
        }
    }
    examples
}

/// Scans the balanced form beginning at `start`, returning the index just past
/// its closing paren and its top-level tokens (the head first).
///
/// A nested form counts as one token. `None` when the form does not close.
fn scan_form(characters: &[char], start: usize) -> Option<(usize, Vec<String>)> {
    let mut depth = 0_usize;
    let mut tokens: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut index = start;

    while index < characters.len() {
        let character = characters[index];

        // `#\(` and `#\)` are character literals, not delimiters. Consuming
        // the escaped character is what keeps them from unbalancing the scan.
        if character == '#'
            && characters.get(index + 1) == Some(&'\\')
            && index + 2 < characters.len()
        {
            if depth == 1 {
                current.push_str("#\\");
                current.push(characters[index + 2]);
            }
            index += 3;
            continue;
        }

        // A string literal inside the example: its contents are not tokens and
        // its parens are not delimiters.
        if character == '"' {
            let end = scan_string(characters, index)?;
            if depth == 1 {
                current.extend(&characters[index..end]);
            }
            index = end;
            continue;
        }

        match character {
            '(' => {
                depth += 1;
                if depth > 1 {
                    current.push('(');
                }
            }
            ')' => {
                depth -= 1;
                if depth == 0 {
                    if !current.is_empty() {
                        tokens.push(std::mem::take(&mut current));
                    }
                    return Some((index + 1, tokens));
                }
                current.push(')');
            }
            _ if character.is_whitespace() => {
                // Whitespace separates tokens only at the top level; inside a
                // nested form it is part of that one token.
                if depth == 1 {
                    if !current.is_empty() {
                        tokens.push(std::mem::take(&mut current));
                    }
                } else {
                    current.push(character);
                }
            }
            _ => current.push(character),
        }
        index += 1;
    }
    None
}

/// Whether `text` contains `needle` at all, ignoring ASCII case.
///
/// A byte scan, not a parse: no allocation, and it reads each byte once. Used
/// only as a *negative* guard — an answer of `true` may come from prose, in
/// which case the real scan runs and finds nothing, exactly as it would have
/// without the guard.
fn mentions(text: &str, needle: &str) -> bool {
    let needle = needle.as_bytes();
    text.len() >= needle.len()
        && text
            .as_bytes()
            .windows(needle.len())
            .any(|window| window.eq_ignore_ascii_case(needle))
}

/// The index just past the closing `"` of the string literal starting at
/// `start`, honouring `\"` escapes. `None` when it does not close.
fn scan_string(characters: &[char], start: usize) -> Option<usize> {
    let mut index = start + 1;
    while index < characters.len() {
        match characters[index] {
            '\\' => index += 2,
            '"' => return Some(index + 1),
            _ => index += 1,
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::sexpr::SyntaxTree;

    fn found(source: &str) -> Vec<StaleExample> {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        let Some(form) = tree.root_view().children.first().cloned() else {
            return Vec::new();
        };
        examine(&form)
    }

    // --- positive

    #[test]
    fn flags_an_example_supplying_too_few_arguments() {
        let items = found(
            "(defun scale (x factor) \"Scale X by FACTOR. Example: (scale 3) => 6\" (* x factor))",
        );
        assert_eq!(items.len(), 1, "{items:?}");
        assert_eq!(items[0].name, "scale");
        assert_eq!(items[0].example, "(scale 3)");
        assert_eq!(items[0].supplied, 1);
        assert_eq!(items[0].minimum, 2);
        assert_eq!(items[0].maximum, Some(2));
    }

    #[test]
    fn flags_an_example_supplying_too_many_arguments() {
        let items = found("(defun scale (factor) \"Example: (scale 3 2) => 6\" factor)");
        assert_eq!(items.len(), 1, "{items:?}");
        assert_eq!(items[0].supplied, 2);
        assert_eq!(items[0].maximum, Some(1));
    }

    #[test]
    fn flags_a_stale_example_in_a_macro() {
        let items = found("(defmacro twice (form) \"Example: (twice a b)\" form)");
        assert_eq!(items.len(), 1, "{items:?}");
    }

    #[test]
    fn reports_every_stale_example_in_one_docstring() {
        let items = found(
            "(defun scale (x factor) \"Examples: (scale 3) and (scale 1 2 3).\" (* x factor))",
        );
        assert_eq!(items.len(), 2, "{items:?}");
    }

    #[test]
    fn a_nested_example_is_still_read() {
        // The outer form is not a `scale` call; the inner one is, and is stale.
        let items = found("(defun scale (x factor) \"Try (list (scale 3)).\" (* x factor))");
        assert_eq!(items.len(), 1, "{items:?}");
        assert_eq!(items[0].example, "(scale 3)");
    }

    #[test]
    fn an_example_with_a_nested_argument_counts_that_argument_once() {
        // `(scale (double x))` supplies one argument, not two.
        let items = found("(defun scale (x factor) \"Try (scale (double x)).\" (* x factor))");
        assert_eq!(items.len(), 1, "{items:?}");
        assert_eq!(items[0].supplied, 1);
    }

    // --- near-miss negatives

    #[test]
    fn a_correct_example_is_not_reported() {
        assert!(found(
            "(defun scale (x factor) \"Scale X by FACTOR. Example: (scale 3 2) => 6\" (* x factor))"
        )
        .is_empty());
    }

    #[test]
    fn an_optional_parameter_makes_a_shorter_call_correct() {
        assert!(found("(defun scale (x &optional factor) \"Example: (scale 3)\" x)").is_empty());
        assert!(found("(defun scale (x &optional factor) \"Example: (scale 3 2)\" x)").is_empty());
    }

    #[test]
    fn a_rest_parameter_removes_the_upper_bound() {
        assert!(
            found("(defun total (&rest numbers) \"Example: (total 1 2 3 4 5)\" numbers)")
                .is_empty()
        );
    }

    #[test]
    fn a_body_parameter_removes_the_upper_bound_for_a_macro() {
        assert!(
            found(
                "(defmacro when-let (binding &body body) \"Example: (when-let (x 1) a b c)\" body)"
            )
            .is_empty()
        );
    }

    /// The upper bound of a keyword lambda list is not a number, so only
    /// under-supply is checked.
    #[test]
    fn a_keyword_lambda_list_is_checked_for_under_supply_only() {
        // Over-supply: legal, because keywords the rule cannot enumerate may be
        // accepted.
        assert!(
            found("(defun render (x &key stream) \"Example: (render 1 :stream s :pretty t)\" x)")
                .is_empty()
        );
        // Under-supply: still decidable.
        let items = found("(defun render (x &key stream) \"Example: (render)\" x)");
        assert_eq!(items.len(), 1, "{items:?}");
        assert_eq!(items[0].supplied, 0);
        assert_eq!(items[0].maximum, None);
    }

    #[test]
    fn an_allow_other_keys_lambda_list_is_treated_the_same_way() {
        assert!(found(
            "(defun render (x &key stream &allow-other-keys) \"Example: (render 1 :a 1 :b 2)\" x)"
        )
        .is_empty());
    }

    /// A placeholder means the example illustrates a shape, not a call.
    #[test]
    fn an_example_carrying_a_placeholder_is_not_counted() {
        for docstring in [
            "Example: (scale 3 ...)",
            "Example: (scale ... 2)",
            "Example: (scale x &rest more)",
            "Example: (scale 1 2 3 …)",
            "Example: (scale a etc.)",
            "Example: (scale 1 args)",
        ] {
            assert!(
                found(&format!(
                    "(defun scale (x factor) \"{docstring}\" (* x factor))"
                ))
                .is_empty(),
                "wrongly reported: {docstring}"
            );
        }
    }

    #[test]
    fn a_lambda_list_carrying_a_reader_conditional_is_not_checked() {
        assert!(found("(defun scale (x #+sbcl factor) \"Example: (scale 3)\" x)").is_empty());
    }

    #[test]
    fn an_unbalanced_example_is_skipped() {
        assert!(found("(defun scale (x factor) \"Example: (scale 3\" (* x factor))").is_empty());
    }

    #[test]
    fn a_call_to_some_other_function_is_not_checked() {
        // `double`'s lambda list is not in hand, so its arity is unknowable.
        assert!(
            found("(defun scale (x factor) \"See (double 1 2 3 4).\" (* x factor))").is_empty()
        );
    }

    #[test]
    fn a_definition_with_no_docstring_is_not_checked() {
        assert!(found("(defun scale (x factor) (* x factor))").is_empty());
    }

    /// A lone string body is the function's return value.
    #[test]
    fn a_lone_string_body_is_not_read_as_a_docstring() {
        assert!(found("(defun scale () \"(scale 1 2 3)\")").is_empty());
    }

    #[test]
    fn a_defmethod_is_not_checked() {
        // A method's lambda list is one of several congruent ones.
        assert!(found("(defmethod area ((s square)) \"Example: (area)\" 1)").is_empty());
    }

    #[test]
    fn a_form_that_is_not_a_definition_is_not_checked() {
        assert!(found("(let ((x \"(scale 1 2 3)\")) x)").is_empty());
    }

    // --- the scanner's own traps

    #[test]
    fn a_paren_inside_a_string_in_the_example_is_not_a_delimiter() {
        // The `)` inside the embedded string must not close the call early.
        let items = found("(defun emit (x) \"Example: (emit \\\"a ) b\\\" extra)\" x)");
        assert_eq!(items.len(), 1, "{items:?}");
        assert_eq!(items[0].supplied, 2);
    }

    #[test]
    fn a_character_literal_paren_is_not_a_delimiter() {
        let items = found("(defun emit (x) \"Example: (emit #\\\\( 2)\" x)");
        assert_eq!(items.len(), 1, "{items:?}");
        assert_eq!(items[0].supplied, 2);
    }

    #[test]
    fn the_head_is_matched_case_insensitively_and_past_a_qualifier() {
        assert_eq!(
            found("(defun scale (x factor) \"Example: (SCALE 3)\" (* x factor))").len(),
            1
        );
        assert_eq!(
            found("(defun scale (x factor) \"Example: (app:scale 3)\" (* x factor))").len(),
            1
        );
    }

    #[test]
    fn an_empty_example_call_counts_zero_arguments() {
        let items = found("(defun scale (x factor) \"Example: (scale)\" (* x factor))");
        assert_eq!(items.len(), 1, "{items:?}");
        assert_eq!(items[0].supplied, 0);
    }

    #[test]
    fn the_message_names_the_example_the_supplied_count_and_the_accepted_range() {
        let items = found("(defun scale (x factor) \"Example: (scale 3)\" (* x factor))");
        let message = items[0].message();
        assert!(message.contains("(scale 3)"), "{message}");
        assert!(message.contains("1 argument(s)"), "{message}");
        assert!(message.contains("exactly 2"), "{message}");
    }

    #[test]
    fn the_message_says_at_least_when_the_upper_bound_is_unbounded() {
        let items = found("(defun total (a &rest more) \"Example: (total)\" more)");
        assert_eq!(items.len(), 1, "{items:?}");
        assert!(items[0].message().contains("at least 1"), "{items:?}");
    }
}
