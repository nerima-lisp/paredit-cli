//! `ignore-declaration-conflict`: an `ignore` declaration that contradicts the
//! code around it.
//!
//! Two mistakes, one shape. `(declare (ignore x))` followed by a body that uses
//! `x` is a promise the body immediately breaks — most implementations signal a
//! warning, some an error, and the reader is left unsure which of the two
//! statements to believe. `(declare (ignore y))` where `y` is not a parameter
//! at all is usually a rename that updated the lambda list and not the
//! declaration, and it silences nothing.
//!
//! They are one rule because they are one reading: gather the lambda list,
//! gather the declared-ignored names, gather the body's references, and compare.
//! Splitting them would mean two rules doing the same three walks.
//!
//! Not pedantic and not a warning: the first case is a compile-time error in
//! several implementations, and the second is dead text that hides a real
//! unused parameter.
//!
//! Report-only. Which of the two statements is wrong — the declaration or the
//! body — is exactly what the rule cannot know.
//!
//! Scope: Common Lisp only.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, ReaderPrefix};
use paredit_core_syntax::view_query::{atom_text, is_paren_list, list_head, symbol_in, symbol_is};

pub const META: RuleMeta = RuleMeta::new(
    "ignore-declaration-conflict",
    RuleCategory::Declaration,
    Severity::Error,
    "an (ignore x) declaration whose variable the body uses, or which names no parameter at all",
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "`(declare (ignore x))` states that the body will not refer to `x`. A body that refers to \
         it anyway makes the two statements contradict, which most implementations diagnose; a \
         declaration naming something the lambda list does not bind silences nothing and hides \
         the parameter that really is unused.",
    )
    .with_example(
        "(defun f (a b) (declare (ignore b)) (+ a b))",
        "(defun f (a b) (declare (ignorable b)) (+ a b))",
    )
    .with_caveat(
        "`ignorable` is the declaration for \"may or may not be used\" and is never reported.",
    ),
);

const HEADS: [NormalizedHead; 4] = [
    NormalizedHead::new("defun"),
    NormalizedHead::new("defmacro"),
    NormalizedHead::new("defmethod"),
    NormalizedHead::new("lambda"),
];

/// Which part of a lambda list an element sits in.
///
/// The sections do not share a shape: a required parameter is a name (or, in a
/// macro lambda list, a nested pattern), while an `&optional`/`&key`/`&aux`
/// element is `(name default supplied-p)` — three slots of which the *middle* one
/// is an expression and not a binding at all. Reading every sublist the same way
/// is what made the rule miss `supplied-p` variables and report them as unbound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    Required,
    Rest,
    Defaulted,
    Marker,
}

/// The section a lambda-list keyword opens, or `None` if it is not one.
///
/// Doubling as the "is this a lambda-list keyword" test keeps the two answers
/// from disagreeing. Compared with [`symbol_in`], so an uppercase `&OPTIONAL` or
/// a qualified `cl:&optional` — both legal, since `&optional` is read as an
/// ordinary symbol — cannot be recognised as a keyword while falling through to
/// the wrong section.
fn section_of(keyword: &str) -> Option<Section> {
    if symbol_in(keyword, &["&optional", "&key", "&aux"]) {
        Some(Section::Defaulted)
    } else if symbol_in(keyword, &["&rest", "&body"]) {
        Some(Section::Rest)
    } else if symbol_in(keyword, &["&whole", "&environment"]) {
        Some(Section::Required)
    } else if symbol_is(keyword, "&allow-other-keys") {
        Some(Section::Marker)
    } else {
        None
    }
}

/// Calls `visit` with every name a lambda list *binds*.
///
/// `destructuring` says whether a sublist in required or `&rest` position is a
/// nested pattern (a macro lambda list, CLHS 3.4.4) or a `defmethod` specialiser
/// `(var type)`, where only the first element is a binding and the second names a
/// class. Recursing into a specialiser would make `(declare (ignore square))`
/// silently correct, so the two cases must not share a branch.
fn for_each_parameter_name(
    lambda_list: &ExpressionView,
    destructuring: bool,
    visit: &mut impl FnMut(&str),
) {
    let mut section = Section::Required;
    for element in &lambda_list.children {
        if let Some(text) = atom_text(element) {
            match section_of(text) {
                Some(opened) => section = opened,
                None if section != Section::Marker => visit(text),
                None => {}
            }
            continue;
        }
        match section {
            Section::Required | Section::Rest => {
                if destructuring {
                    for_each_parameter_name(element, true, visit);
                } else if let Some(name) = element.children.first().and_then(atom_text) {
                    visit(name);
                }
            }
            Section::Defaulted => {
                // `(name default supplied-p)`. Index 1 is an expression
                // evaluated in the enclosing scope, never a binding, so it is
                // the one slot that must not be walked for names.
                match element.children.first() {
                    None => {}
                    Some(first) => match atom_text(first) {
                        Some(name) => visit(name),
                        // `&key ((:external internal) default)` binds the
                        // *second* element; the first is the keyword a caller
                        // passes. In a macro lambda list the same slot may
                        // instead be a nested pattern.
                        None if destructuring => for_each_parameter_name(first, true, visit),
                        None => {
                            if let Some(name) = first.children.get(1).and_then(atom_text) {
                                visit(name);
                            }
                        }
                    },
                }
                if let Some(supplied) = element.children.get(2).and_then(atom_text) {
                    visit(supplied);
                }
            }
            Section::Marker => {}
        }
    }
}

/// Whether `lambda_list` binds `name`.
fn lambda_list_binds(lambda_list: &ExpressionView, destructuring: bool, name: &str) -> bool {
    let mut found = false;
    for_each_parameter_name(lambda_list, destructuring, &mut |parameter| {
        found |= parameter.eq_ignore_ascii_case(name);
    });
    found
}

/// Whether a lambda list is one this rule cannot enumerate.
///
/// A lambda list spliced in from a macro template — `` `(lambda ,args …) `` — or
/// one carrying an unquoted element — `(stream ,@arg-names)` — binds names that
/// are not in the text. Neither can prove a declared name *unbound*, which is the
/// only claim that needs the full list.
fn lambda_list_is_opaque(lambda_list: &ExpressionView) -> bool {
    if !is_paren_list(lambda_list) {
        return true;
    }
    fn opaque_element(view: &ExpressionView) -> bool {
        if !view.reader_prefixes.is_empty() {
            return true;
        }
        // A reader conditional folds itself and the form it guards into a single
        // atom, so `#+sbcl x` hides whether `x` is bound at all.
        if atom_text(view).is_some_and(|text| text.starts_with('#')) {
            return true;
        }
        view.children.iter().any(opaque_element)
    }
    lambda_list.children.iter().any(opaque_element)
}

/// What is wrong with one `ignore` declaration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IgnoreConflict {
    /// Declared ignored, then used.
    UsedAnyway,
    /// Declared ignored, but not bound by the lambda list.
    NotBound,
}

impl IgnoreConflict {
    #[must_use]
    pub const fn as_message(self) -> &'static str {
        match self {
            Self::UsedAnyway => {
                "is declared ignored and then used, which most implementations diagnose"
            }
            Self::NotBound => {
                "is declared ignored but is not a parameter, so the declaration silences nothing"
            }
        }
    }
}

/// One contradictory declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConflictingIgnore {
    pub span: ByteSpan,
    pub variable: String,
    pub conflict: IgnoreConflict,
}

// -- evaluation context ------------------------------------------------------

/// How much of the reader syntax between a node and the definition being checked
/// says "this is not a reference to that definition's variables".
///
/// Two counters, because `'` and `` ` `` are not the same thing. A comma inside
/// `'(…)` is a comma character in a literal list, so `hard` never clears; a comma
/// inside `` `(…) `` escapes back to code, so `quasi` counts up and down.
///
/// `quasi` is **signed**, and that is the whole point. Depth above zero is
/// template data: the `from-end` in `` `(if from-end …) `` is a symbol in emitted
/// code, not a use of the enclosing function's parameter. Depth *below* zero is
/// an unquote that escaped out of a template the definition itself sits in:
/// in `` `(lambda (width x) (declare (ignore width)) (f ,(g width) x)) `` the
/// `width` inside `,(…)` is evaluated where the template is built, so it names
/// the *enclosing* scope's variable and not this lambda's parameter. Only depth
/// exactly zero is a reference to what this definition binds. A saturating
/// counter cannot tell the second case from the first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct QuoteState {
    hard: bool,
    quasi: i32,
}

impl QuoteState {
    const EVALUATED: Self = Self {
        hard: false,
        quasi: 0,
    };

    /// Whether a node here names a variable of the definition under test.
    const fn is_reference_scope(self) -> bool {
        !self.hard && self.quasi == 0
    }

    /// The state inside a node, given the state outside it and its own prefixes.
    ///
    /// `#'`, `#.`, `#+` and the rest are deliberately neutral: none of them turns
    /// code into data.
    fn after_prefixes(mut self, view: &ExpressionView) -> Self {
        for prefix in &view.reader_prefixes {
            match prefix {
                ReaderPrefix::Quote => self.hard = true,
                ReaderPrefix::Quasiquote => self.quasi += 1,
                ReaderPrefix::Unquote | ReaderPrefix::UnquoteSplicing => self.quasi -= 1,
                _ => {}
            }
        }
        self
    }
}

/// The long-hand `(quote …)`, which hand-written code and macro output both
/// spell out where the reader would have produced `'…`.
fn is_quote_form(view: &ExpressionView) -> bool {
    list_head(view).is_some_and(|head| symbol_is(head, "quote"))
}

fn is_declare(view: &ExpressionView) -> bool {
    list_head(view).is_some_and(|head| symbol_is(head, "declare"))
}

/// The names declared `ignore` in the leading declarations of `body`.
///
/// `ignorable` is deliberately excluded: it means "this may or may not be
/// used", which is precisely the statement neither half of this rule can
/// contradict.
fn ignored_names(body: &[ExpressionView]) -> Vec<(String, ByteSpan)> {
    let mut names = Vec::new();
    for form in body {
        if !list_head(form).is_some_and(|head| symbol_in(head, &["declare"])) {
            // Declarations only lead a body; anything else ends the section.
            break;
        }
        for specifier in form.children.iter().skip(1) {
            if !list_head(specifier).is_some_and(|head| symbol_in(head, &["ignore"])) {
                continue;
            }
            for name in specifier.children.iter().skip(1) {
                // `(declare (ignore ,@dummies))` in a macro template names no
                // variable in *this* text: the splice is filled in when the
                // template is built. Reading `,@dummies` as an identifier
                // reported it as a parameter that does not exist.
                if !name.reader_prefixes.is_empty() {
                    continue;
                }
                if let Some(text) = atom_text(name) {
                    names.push((text.to_owned(), name.span));
                }
            }
        }
    }
    names
}

// -- shadowing ---------------------------------------------------------------

/// Where a nested binding form keeps its lambda list, when its head is one this
/// rule knows binds by lambda list.
fn nested_lambda_list_index(head: &str) -> Option<(usize, bool)> {
    if symbol_is(head, "lambda") {
        Some((1, false))
    } else if symbol_in(head, &["defun", "defmethod"]) {
        Some((2, false))
    } else if symbol_in(head, &["defmacro", "define-compiler-macro"]) {
        Some((2, true))
    } else {
        None
    }
}

/// The `with-…` macros whose first argument opens a binding whose *first*
/// element is a variable: `(with-open-file (stream path) …)`.
///
/// A curated list rather than a `with-` prefix test on purpose. `(with-simple-
/// restart (continue "…") …)` has exactly the same shape and binds nothing —
/// `continue` names a restart — so a prefix test would stop walking a body that
/// may well contain the use being looked for. Every entry here is a CLHS macro
/// whose first argument is specified as `(var …)`. A leading `%` is accepted
/// because implementations spell their internal twin that way, as SBCL does with
/// `%with-output-to-string`.
const WITH_VARIABLE_BINDERS: [&str; 6] = [
    "with-open-file",
    "with-open-stream",
    "with-input-from-string",
    "with-output-to-string",
    "with-hash-table-iterator",
    "with-package-iterator",
];

fn is_with_variable_binder(head: &str) -> bool {
    let bare = head.strip_prefix('%').unwrap_or(head);
    symbol_in(bare, &WITH_VARIABLE_BINDERS)
}

/// The sub-forms of `view` that are still evaluated in the *enclosing* scope,
/// when `view` rebinds `name`.
///
/// `None` means `view` does not rebind `name` and should be walked normally.
/// `Some(regions)` means the rest of `view` belongs to an inner binding of the
/// same name and must not be searched — but `regions` still must be, because an
/// initialiser runs before the binding it feeds: in `(let ((x (f x))) …)` the
/// inner `x` shadows nothing yet, and the argument to `f` is the outer one.
///
/// Known gap: a `flet`/`labels` clause whose lambda list rebinds `name` is not
/// modelled, so a use inside such a clause is still counted. That errs towards
/// reporting, which is the safe direction for a guard whose failure mode is
/// silence.
fn shadowing_outer_regions<'a>(
    view: &'a ExpressionView,
    name: &str,
) -> Option<Vec<&'a ExpressionView>> {
    let head = list_head(view)?;

    if let Some((index, destructuring)) = nested_lambda_list_index(head) {
        let lambda_list = view.children.get(index)?;
        if !lambda_list_binds(lambda_list, destructuring, name) {
            return None;
        }
        // Only the default expressions survive: `(lambda (a &optional (b x)) …)`
        // evaluates `x` outside. Every other part of the lambda list is a
        // binding occurrence, and counting those as uses is what made a nested
        // `(lambda (posn) (declare (ignore posn)) …)` report its own parameter.
        return Some(defaulted_expressions(lambda_list));
    }

    if symbol_in(head, &["let", "let*", "do", "do*"]) {
        let bindings = view.children.get(1)?;
        if !is_paren_list(bindings) {
            return None;
        }
        let binds = bindings.children.iter().any(|binding| {
            atom_text(binding)
                .map_or_else(|| binding.children.first().and_then(atom_text), Some)
                .is_some_and(|bound| bound.eq_ignore_ascii_case(name))
        });
        if !binds {
            return None;
        }
        // The initialiser of every clause, `(var init …)` index 1. `do`'s index
        // 2 step form is evaluated in the new scope, so it stays pruned.
        return Some(
            bindings
                .children
                .iter()
                .filter_map(|binding| binding.children.get(1))
                .collect(),
        );
    }

    if symbol_in(head, &["multiple-value-bind", "destructuring-bind"]) {
        let pattern = view.children.get(1)?;
        if !lambda_list_binds(pattern, true, name) {
            return None;
        }
        // The value form at index 2 is evaluated before the binding takes hold.
        return Some(view.children.get(2).into_iter().collect());
    }

    if symbol_in(head, &["dolist", "dotimes", "with-slots", "with-accessors"])
        || is_with_variable_binder(head)
    {
        let clause = view.children.get(1)?;
        if !is_paren_list(clause) {
            return None;
        }
        let binds = if symbol_in(head, &["with-slots", "with-accessors"]) {
            // `(with-slots (a (b slot)) instance …)` binds every entry.
            clause.children.iter().any(|entry| {
                atom_text(entry)
                    .map_or_else(|| entry.children.first().and_then(atom_text), Some)
                    .is_some_and(|bound| bound.eq_ignore_ascii_case(name))
            })
        } else {
            clause
                .children
                .first()
                .and_then(atom_text)
                .is_some_and(|bound| bound.eq_ignore_ascii_case(name))
        };
        if !binds {
            return None;
        }
        // Everything after the bound name in the clause — the list to iterate,
        // the pathname, the instance — is evaluated outside the new binding.
        let mut regions: Vec<&ExpressionView> = clause.children.iter().skip(1).collect();
        if symbol_in(head, &["with-slots", "with-accessors"]) {
            regions = view.children.get(2).into_iter().collect();
        }
        return Some(regions);
    }

    None
}

/// The default-value expressions of a lambda list — the one part of it that is
/// evaluated in the enclosing scope.
fn defaulted_expressions(lambda_list: &ExpressionView) -> Vec<&ExpressionView> {
    let mut section = Section::Required;
    let mut found = Vec::new();
    for element in &lambda_list.children {
        if let Some(text) = atom_text(element) {
            if let Some(opened) = section_of(text) {
                section = opened;
            }
            continue;
        }
        if section == Section::Defaulted {
            if let Some(default) = element.children.get(1) {
                found.push(default);
            }
        }
    }
    found
}

// -- uses --------------------------------------------------------------------

/// Whether any form in `body` refers to the variable `name`.
///
/// Three things that look like a reference and are not:
///
/// * a symbol in **operator position**. Common Lisp is a Lisp-2, so the `signal`
///   in `(signal int)` names a function and cannot contradict `(declare (ignore
///   signal))` on a parameter of that name.
/// * a symbol reached only as **data**, or through an unquote that escaped the
///   template this definition sits in — see [`QuoteState`].
/// * a symbol under a form that **rebinds** it — see [`shadowing_outer_regions`].
///
/// Nested `(declare …)` forms are skipped outright: naming a variable in a
/// declaration is not using it, and counting it would let `(declare (ignore x))`
/// followed by `(declare (ignorable x))` report itself.
fn body_uses(body: &[ExpressionView], name: &str) -> bool {
    body.iter()
        .any(|form| !is_declare(form) && references(form, name, QuoteState::EVALUATED, false))
}

fn references(view: &ExpressionView, name: &str, outer: QuoteState, is_operator: bool) -> bool {
    let state = outer.after_prefixes(view);
    if let Some(text) = atom_text(view) {
        return state.is_reference_scope() && !is_operator && text.eq_ignore_ascii_case(name);
    }
    if is_declare(view) {
        return false;
    }
    if let Some(regions) = shadowing_outer_regions(view, name) {
        return regions
            .into_iter()
            .any(|region| references(region, name, state, false));
    }
    let inside = if is_quote_form(view) {
        QuoteState {
            hard: true,
            ..state
        }
    } else {
        state
    };
    let is_call = is_paren_list(view);
    view.children
        .iter()
        .enumerate()
        .any(|(index, child)| references(child, name, inside, is_call && index == 0))
}

/// Every contradictory `ignore` declaration in one definition.
#[must_use]
pub fn examine(view: &ExpressionView) -> Vec<ConflictingIgnore> {
    let Some(head) = list_head(view) else {
        return Vec::new();
    };
    if !symbol_in(head, &["defun", "defmacro", "defmethod", "lambda"]) {
        return Vec::new();
    }
    let Some(list_index) = lambda_list_index(view, head) else {
        return Vec::new();
    };
    let Some(lambda_list) = view.children.get(list_index) else {
        return Vec::new();
    };
    let destructuring = symbol_in(head, &["defmacro"]);
    let opaque = lambda_list_is_opaque(lambda_list);
    let mut parameters = Vec::new();
    if !opaque {
        for_each_parameter_name(lambda_list, destructuring, &mut |name| {
            parameters.push(name.to_owned());
        });
    }
    let body = view.children.get(list_index + 1..).unwrap_or(&[]);

    ignored_names(body)
        .into_iter()
        .filter_map(|(name, span)| {
            let bound = parameters
                .iter()
                .any(|parameter| parameter.eq_ignore_ascii_case(&name));
            let conflict = if bound {
                if body_uses(body, &name) {
                    IgnoreConflict::UsedAnyway
                } else {
                    return None;
                }
            } else if opaque {
                // A lambda list this rule cannot enumerate cannot witness that a
                // name is *unbound*. The other half of the rule still can, but
                // only for names it did see, and by construction it saw none.
                return None;
            } else {
                IgnoreConflict::NotBound
            };
            Some(ConflictingIgnore {
                span,
                variable: name,
                conflict,
            })
        })
        .collect()
}

/// Where a definition keeps its lambda list.
///
/// `lambda` puts it at index 1 and the definers at index 2 — except `defmethod`,
/// which admits qualifiers in between: `(defmethod print-object :around ((x foo)
/// s) …)`. Reading index 2 there lands on `:around`, an atom with no children,
/// which parses as a lambda list binding nothing and reports every declared name
/// as unbound. Qualifiers are non-list objects, so scanning past bare atoms finds
/// the list. An atom carrying a reader prefix stops the scan instead of being
/// skipped: `` `(defmethod ,name ,args …) `` has a spliced lambda list, not a
/// qualifier, and skipping it would land on a body form.
fn lambda_list_index(view: &ExpressionView, head: &str) -> Option<usize> {
    if symbol_is(head, "lambda") {
        return Some(1);
    }
    if !symbol_is(head, "defmethod") {
        return Some(2);
    }
    (2..view.children.len()).find(|&index| {
        view.children
            .get(index)
            .is_some_and(|child| is_paren_list(child) || !child.reader_prefixes.is_empty())
    })
}

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    fn check(
        &self,
        _context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult {
        for conflict in examine(view) {
            sink.report(
                conflict.span,
                format!("{} {}", conflict.variable, conflict.conflict.as_message()),
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::{Path, SyntaxTree};

    fn conflicts(input: &str) -> Vec<(String, IgnoreConflict)> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let view = tree
            .select_path(&Path::root_child(0))
            .expect("root form")
            .view();
        examine(&view)
            .into_iter()
            .map(|item| (item.variable, item.conflict))
            .collect()
    }

    #[test]
    fn flags_an_ignored_parameter_the_body_uses() {
        assert_eq!(
            conflicts("(defun f (a b) (declare (ignore b)) (+ a b))"),
            vec![("b".to_owned(), IgnoreConflict::UsedAnyway)]
        );
    }

    #[test]
    fn flags_an_ignore_of_something_that_is_not_a_parameter() {
        assert_eq!(
            conflicts("(defun f (a) (declare (ignore b)) a)"),
            vec![("b".to_owned(), IgnoreConflict::NotBound)]
        );
    }

    #[test]
    fn accepts_a_genuinely_ignored_parameter() {
        assert!(conflicts("(defun f (a b) (declare (ignore b)) a)").is_empty());
    }

    #[test]
    fn does_not_flag_ignorable() {
        assert!(conflicts("(defun f (a b) (declare (ignorable b)) (+ a b))").is_empty());
    }

    #[test]
    fn reads_a_lambda_and_a_method() {
        assert_eq!(
            conflicts("(lambda (a) (declare (ignore a)) a)"),
            vec![("a".to_owned(), IgnoreConflict::UsedAnyway)]
        );
        assert_eq!(
            conflicts("(defmethod area ((s square) n) (declare (ignore n)) (* n n))"),
            vec![("n".to_owned(), IgnoreConflict::UsedAnyway)]
        );
    }

    #[test]
    fn reads_a_specialised_parameter_as_bound() {
        assert!(
            conflicts("(defmethod area ((s square) n) (declare (ignore s)) (* n n))").is_empty()
        );
    }

    #[test]
    fn reads_several_names_in_one_declaration() {
        let found = conflicts("(defun f (a b) (declare (ignore a b)) (+ a b))");
        assert_eq!(found.len(), 2);
        assert!(found.iter().all(|(_, c)| *c == IgnoreConflict::UsedAnyway));
    }

    #[test]
    fn finds_a_use_nested_deep_in_the_body() {
        assert_eq!(
            conflicts("(defun f (a b) (declare (ignore b)) (when a (list (list b))))"),
            vec![("b".to_owned(), IgnoreConflict::UsedAnyway)]
        );
    }

    #[test]
    fn a_declaration_after_a_body_form_is_not_a_leading_declaration() {
        // Declarations lead a body; anything after a real form is not one, and
        // reading it as one would report a variable nothing declared.
        assert!(conflicts("(defun f (a) a (declare (ignore zzz)))").is_empty());
    }

    #[test]
    fn does_not_flag_a_definition_with_no_declarations() {
        assert!(conflicts("(defun f (a b) (+ a b))").is_empty());
    }

    // -- guards against false positives ------------------------------------
    //
    // Every guard below is paired with a control that must still fire. The
    // failure mode of a suppression guard is silence, and a rule that reports
    // nothing is indistinguishable from a rule with nothing to report, so a
    // "must not fire" assertion on its own proves only that the code is
    // reachable. Each pair differs in the one detail the guard keys on.
    //
    // All 43 corpus false positives that motivated these guards, and the two
    // true positives that survived them, are recorded in the package README.

    fn names(input: &str) -> Vec<String> {
        conflicts(input).into_iter().map(|(n, _)| n).collect()
    }

    /// A macro lambda list may nest patterns arbitrarily (CLHS 3.4.4), and every
    /// symbol in one is a binding.
    #[test]
    fn destructuring_macro_parameters_are_bound() {
        assert!(conflicts("(defmacro m ((a b) c) (declare (ignore a b)) c)").is_empty());
        assert!(conflicts("(defmacro m ((x &rest r) e) (declare (ignore x r)) e)").is_empty());
        assert!(conflicts("(defmacro m ((&key a) (b p)) (declare (ignore a b p)) nil)").is_empty());
    }

    #[test]
    fn a_destructuring_pattern_still_witnesses_an_unbound_name() {
        assert_eq!(
            names("(defmacro m ((a b) c) (declare (ignore zzz)) c)"),
            vec!["zzz".to_owned()]
        );
    }

    /// The counterpart: an ordinary lambda list has no nested patterns, and a
    /// `defmethod` sublist is `(var specialiser)` where the second element names
    /// a *class*. Recursing there would make `(ignore square)` silently correct.
    #[test]
    fn a_defmethod_specialiser_is_not_a_binding() {
        assert_eq!(
            names("(defmethod area ((s square) n) (declare (ignore square)) n)"),
            vec!["square".to_owned()]
        );
    }

    #[test]
    fn a_supplied_p_variable_is_bound() {
        assert!(conflicts("(defun f (a &optional (o 1 op)) (declare (ignore op)) a)").is_empty());
        assert!(conflicts("(defun f (&key (k 2 kp)) (declare (ignore k kp)) nil)").is_empty());
    }

    /// The middle slot of `(name default supplied-p)` is an expression evaluated
    /// in the enclosing scope, not a third binding. Reading it as one would
    /// suppress a genuine report of the symbol that happens to be the default.
    #[test]
    fn an_optional_default_expression_is_not_a_binding() {
        assert_eq!(
            names("(defun f (a &optional (o dflt)) (declare (ignore dflt)) (list a o))"),
            vec!["dflt".to_owned()]
        );
    }

    #[test]
    fn an_explicit_key_name_binds_its_second_element() {
        assert!(
            conflicts("(defun f (&key ((:ext internal) 0)) (declare (ignore internal)) nil)")
                .is_empty()
        );
        // The keyword half is not a variable, so declaring it stays a finding.
        assert_eq!(
            names("(defun f (&key ((:ext internal) 0)) (declare (ignore ext)) internal)"),
            vec!["ext".to_owned()]
        );
    }

    #[test]
    fn whole_and_environment_parameters_are_bound() {
        assert!(
            conflicts("(defmacro m (&whole w a &environment env) (declare (ignore w env)) a)")
                .is_empty()
        );
        assert_eq!(
            names("(defmacro m (&whole w a) (declare (ignore zzz)) (list w a))"),
            vec!["zzz".to_owned()]
        );
    }

    /// The variable *after* `&whole` is bound whether or not the marker is
    /// recognised, so the previous test passes either way. What recognising it
    /// buys is that the marker itself is not recorded as a parameter — without
    /// which every lambda-list keyword would count as a variable of its own name.
    #[test]
    fn a_lambda_list_marker_is_not_itself_a_parameter() {
        assert_eq!(
            names("(defmacro m (&whole w a) (declare (ignore &whole)) (list w a))"),
            vec!["&whole".to_owned()]
        );
        assert_eq!(
            names("(defmacro m (a &environment env) (declare (ignore &environment)) (list a env))"),
            vec!["&environment".to_owned()]
        );
    }

    /// `(declare (ignore ,@dummies))` in a macro template names no variable in
    /// this text; the splice is filled in when the template is built.
    #[test]
    fn a_spliced_declaration_name_is_not_an_identifier() {
        assert!(conflicts("`(lambda (a) (declare (ignore ,@dummies)) a)").is_empty());
    }

    #[test]
    fn a_literal_declaration_name_in_a_template_still_reports() {
        assert_eq!(
            names("`(lambda (a) (declare (ignore zzz)) a)"),
            vec!["zzz".to_owned()]
        );
    }

    /// A lambda list spliced in from a template, or carrying a spliced element,
    /// binds names that are not in the text — so nothing can be called unbound.
    #[test]
    fn an_opaque_lambda_list_cannot_witness_an_unbound_name() {
        assert!(conflicts("`(lambda ,args (declare (ignore zzz)) x)").is_empty());
        assert!(conflicts("`(lambda (a ,@rest) (declare (ignore zzz)) a)").is_empty());
        assert!(conflicts("(defun f (a #+sbcl b) (declare (ignore zzz)) a)").is_empty());
    }

    #[test]
    fn a_readable_lambda_list_of_the_same_shape_still_reports() {
        assert_eq!(
            names("(lambda (args) (declare (ignore zzz)) x)"),
            vec!["zzz".to_owned()]
        );
        assert_eq!(
            names("(lambda (a rest) (declare (ignore zzz)) a)"),
            vec!["zzz".to_owned()]
        );
    }

    /// Inside a template the symbol is part of emitted code, not a use here.
    #[test]
    fn a_use_inside_quoted_data_is_not_a_use() {
        assert!(conflicts("(defun f (a b) (declare (ignore b)) (list a `(x b)))").is_empty());
        assert!(conflicts("(defun f (a b) (declare (ignore b)) (list a 'b))").is_empty());
        assert!(conflicts("(defun f (a b) (declare (ignore b)) (list a (quote b)))").is_empty());
    }

    /// A comma inside `` `(…) `` escapes back to code; the same comma inside
    /// `'(…)` is a comma character in a literal list and escapes nothing. The
    /// two counters exist to tell these apart, and collapsing them into one
    /// depth gets exactly this pair wrong.
    #[test]
    fn an_unquote_escapes_a_quasiquote_but_not_a_hard_quote() {
        assert_eq!(
            names("(defun f (a b) (declare (ignore b)) (list a `(x ,(g b))))"),
            vec!["b".to_owned()]
        );
        assert!(conflicts("(defun f (a b) (declare (ignore b)) (list a '(x ,(g b))))").is_empty());
    }

    #[test]
    fn the_same_use_outside_quoting_still_reports() {
        assert_eq!(
            names("(defun f (a b) (declare (ignore b)) (list a b))"),
            vec!["b".to_owned()]
        );
    }

    /// An unquote escapes *outward*, so it names the scope the template is built
    /// in rather than the lambda the template describes.
    #[test]
    fn an_unquote_escapes_out_of_the_definition_it_sits_in() {
        assert!(
            conflicts("`(lambda (width x) (declare (ignore width)) (f ,(g width) x))").is_empty()
        );
    }

    #[test]
    fn a_plain_template_reference_is_a_use_of_the_generated_binding() {
        assert_eq!(
            names("`(lambda (width x) (declare (ignore width)) (f (g width) x))"),
            vec!["width".to_owned()]
        );
    }

    /// Common Lisp is a Lisp-2: a symbol in operator position names a function.
    #[test]
    fn a_symbol_in_operator_position_is_a_function_name() {
        assert!(
            conflicts("(defun f (signal) (declare (ignore signal)) (signal 'oops))").is_empty()
        );
    }

    #[test]
    fn the_same_symbol_in_argument_position_is_a_variable() {
        assert_eq!(
            names("(defun f (signal) (declare (ignore signal)) (funcall signal))"),
            vec!["signal".to_owned()]
        );
    }

    /// An inner binder rebinds the name; the use belongs to the inner binding.
    #[test]
    fn an_inner_lambda_shadows_the_ignored_parameter() {
        assert!(
            conflicts("(defun f (posn) (declare (ignore posn)) (lambda (posn) (g posn)))")
                .is_empty()
        );
        // A nested lambda's own parameter list is a binding occurrence, not a
        // use — this exact shape appears four times in SBCL's assemblers.
        assert!(
            conflicts(
                "(lambda (segment posn) (declare (ignore posn)) \
                 (h (lambda (segment posn) (declare (ignore posn)) segment)))"
            )
            .is_empty()
        );
    }

    #[test]
    fn an_inner_lambda_that_binds_another_name_does_not_shadow() {
        assert_eq!(
            names("(defun f (posn) (declare (ignore posn)) (lambda (other) (g posn other)))"),
            vec!["posn".to_owned()]
        );
    }

    /// A default expression in the shadowing lambda's own list is still outside
    /// the new binding. The lambda must rebind the very name under test, or the
    /// shadowing branch is never entered and this proves nothing.
    #[test]
    fn a_default_expression_of_a_shadowing_lambda_is_outer() {
        assert_eq!(
            names("(defun f (x) (declare (ignore x)) (lambda (x &optional (y x)) (list x y)))"),
            vec!["x".to_owned()]
        );
    }

    #[test]
    fn let_shadows_but_its_initialiser_does_not() {
        assert!(conflicts("(defun f (x) (declare (ignore x)) (let ((x 1)) x))").is_empty());
        assert_eq!(
            names("(defun f (x) (declare (ignore x)) (let ((x (g x))) x))"),
            vec!["x".to_owned()]
        );
        assert_eq!(
            names("(defun f (x) (declare (ignore x)) (let ((y 1)) (list x y)))"),
            vec!["x".to_owned()]
        );
    }

    #[test]
    fn do_shadows_but_its_initialiser_does_not() {
        assert!(
            conflicts("(defun f (i) (declare (ignore i)) (do ((i 0 (1+ i))) ((> i 3))))")
                .is_empty()
        );
        // Rebinds `i` *and* reads the outer `i` to seed it. The step form
        // `(1+ i)` runs in the new scope and stays pruned.
        assert_eq!(
            names("(defun f (i) (declare (ignore i)) (do ((i i (1+ i))) ((> i 3))))"),
            vec!["i".to_owned()]
        );
        assert_eq!(
            names("(defun f (i) (declare (ignore i)) (do ((j i (1+ j))) ((> j 3))))"),
            vec!["i".to_owned()]
        );
    }

    #[test]
    fn binding_macros_shadow_but_their_value_forms_do_not() {
        assert!(
            conflicts(
                "(defun f (a) (declare (ignore a)) (multiple-value-bind (a b) (g) (list a b)))"
            )
            .is_empty()
        );
        assert_eq!(
            names(
                "(defun f (a) (declare (ignore a)) (multiple-value-bind (a b) (g a) (list a b)))"
            ),
            vec!["a".to_owned()]
        );
        assert_eq!(
            names(
                "(defun f (a) (declare (ignore a)) (multiple-value-bind (x y) (g a) (list x y)))"
            ),
            vec!["a".to_owned()]
        );
        assert!(
            conflicts(
                "(defun f (a) (declare (ignore a)) (destructuring-bind (a b) (g) (list a b)))"
            )
            .is_empty()
        );
        assert_eq!(
            names("(defun f (a) (declare (ignore a)) (destructuring-bind (a b) (g a) (list a b)))"),
            vec!["a".to_owned()]
        );
    }

    #[test]
    fn iteration_macros_shadow_but_their_sequence_forms_do_not() {
        assert!(
            conflicts("(defun f (x) (declare (ignore x)) (dolist (x '(1 2)) (print x)))")
                .is_empty()
        );
        assert_eq!(
            names("(defun f (x) (declare (ignore x)) (dolist (x x) (print x)))"),
            vec!["x".to_owned()]
        );
        assert_eq!(
            names("(defun f (x) (declare (ignore x)) (dolist (y x) (print y)))"),
            vec!["x".to_owned()]
        );
    }

    #[test]
    fn a_with_macro_that_binds_a_stream_shadows_it() {
        assert!(
            conflicts(
                "(defun f (stream) (declare (ignore stream)) \
                 (with-output-to-string (stream) (print 1 stream)))"
            )
            .is_empty()
        );
        // SBCL spells its internal twin with a leading `%`.
        assert!(
            conflicts(
                "(defun f (stream) (declare (ignore stream)) \
                 (%with-output-to-string (stream) (print 1 stream)))"
            )
            .is_empty()
        );
    }

    #[test]
    fn a_with_macro_that_binds_another_name_does_not_shadow() {
        assert_eq!(
            names(
                "(defun f (stream) (declare (ignore stream)) \
                 (with-output-to-string (s) (print stream s)))"
            ),
            vec!["stream".to_owned()]
        );
    }

    /// The binder list is curated rather than a `with-` prefix test, because
    /// `with-simple-restart` has the identical shape and binds nothing — its
    /// first element names a restart. Treating it as a binder would stop the
    /// walk before reaching a real use in its body.
    #[test]
    fn a_with_macro_outside_the_curated_list_is_not_a_binder() {
        assert_eq!(
            names(
                "(defun f (continue) (declare (ignore continue)) \
                 (with-simple-restart (continue \"m\") (list continue)))"
            ),
            vec!["continue".to_owned()]
        );
    }

    #[test]
    fn with_slots_shadows_but_its_instance_form_does_not() {
        assert!(
            conflicts("(defmethod m ((o c) x) (declare (ignore x)) (with-slots (x) o (print x)))")
                .is_empty()
        );
        assert_eq!(
            names("(defmethod m ((o c) x) (declare (ignore x)) (with-slots (y) x (print y)))"),
            vec!["x".to_owned()]
        );
        // Rebinds `x` as a slot *and* reads the outer `x` as the instance.
        assert_eq!(
            names("(defmethod m ((o c) x) (declare (ignore x)) (with-slots (x) x (print x)))"),
            vec!["x".to_owned()]
        );
    }

    /// `defmethod` admits qualifiers between the name and the lambda list.
    #[test]
    fn a_defmethod_qualifier_does_not_displace_the_lambda_list() {
        assert!(
            conflicts("(defmethod print-object :around ((x foo) s) (declare (ignore s)) x)")
                .is_empty()
        );
        assert_eq!(
            names("(defmethod print-object :around ((x foo) s) (declare (ignore zzz)) x)"),
            vec!["zzz".to_owned()]
        );
        assert_eq!(
            names("(defmethod print-object :around ((x foo) s) (declare (ignore s)) (list x s))"),
            vec!["s".to_owned()]
        );
    }

    /// Naming a variable in a declaration is not using it.
    #[test]
    fn a_nested_declaration_is_not_a_use() {
        assert!(
            conflicts("(defun f (a b) (declare (ignore b)) (let ((c 1)) (declare (ignorable b)) (list a c)))")
                .is_empty()
        );
    }

    /// The two findings that survived the corpus audit, in miniature: a
    /// declaration naming a variable no lambda list binds is the shape this rule
    /// exists for, and no guard may swallow it.
    #[test]
    fn an_ignore_naming_nothing_in_an_empty_lambda_list_still_reports() {
        assert_eq!(
            names(
                "(defun %thread-yield () (declare (ignore thread)) (signal-not-implemented 'thread-yield))"
            ),
            vec!["thread".to_owned()]
        );
    }
}
