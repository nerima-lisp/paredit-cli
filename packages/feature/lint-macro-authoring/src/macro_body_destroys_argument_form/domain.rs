//! `macro-body-destroys-argument-form` detection: a macro expander applying a
//! destructive operator directly to one of its own parameters.
//!
//! A macro's parameters are bound to *the caller's source*. `&body` is a tail
//! of the very list the reader built for the call site; `&whole` is that list.
//! Destroying it does not produce a wrong answer once — it edits the program,
//! and every later expansion of that call site sees the edited version.
//!
//! # The run
//!
//! SBCL 2.6.0, `(defmacro bad (&body forms) `(progn ,@(nreverse forms)))`,
//! expanding the *same* source form twice:
//!
//! ```text
//! source before      : (BAD 1 2 3)
//! first expansion    : (PROGN 3 2 1)
//! source after first : (BAD 1)
//! second expansion   : (PROGN 1)
//! source after second: (BAD 1)
//! ```
//!
//! The call site `(bad 1 2 3)` has become `(bad 1)` in memory, and the second
//! expansion produces `(progn 1)` — a different program, silently. The same run
//! with `(defmacro collect-rev (&body forms) `(list ,@(nreverse forms)))`:
//!
//! ```text
//! expand A: (LIST 3 2 1)
//! expand B: (LIST 1)
//! expand C: (LIST 1)
//! ```
//!
//! **SBCL emits no diagnostic of any kind.** Not an error, not a warning, not a
//! style warning. This is the rule this package exists for: unlike every
//! lambda-list misplacement — each of which SBCL rejects outright at `defmacro`
//! time, which is why none of them is a rule here — nothing in the
//! implementation is going to tell anyone about this one.
//!
//! It stays hidden because a single `compile` expands each call site once, so
//! the defect is invisible until something expands twice: `macroexpand` from an
//! editor, an interpreted load, a `compile-file` followed by a later
//! re-expansion, or simply the macro being used at two call sites in a file
//! whose reader shared structure between them.
//!
//! # Why this is decidable, and where it stops
//!
//! The judgement is local and syntactic: a destructive operator whose relevant
//! argument is **the bare parameter symbol**. `(nreverse forms)` destroys the
//! caller's list; `(nreverse (copy-list forms))` and `(sort (copy-seq x) …)`
//! pass a fresh list and are not reported, because their argument is a form
//! rather than the symbol.
//!
//! Two guards make that sound rather than merely plausible, and both cost
//! findings:
//!
//! - a parameter the expander **rebinds or reassigns** anywhere in its body is
//!   dropped, because `(let ((body (copy-list body))) … (nreverse body))` is
//!   correct code and this analysis is not flow-sensitive. See
//!   [`crate::support::names_bound_within`].
//! - only parameters that are **always** the caller's structure are considered
//!   — required, `&rest`/`&body`, `&whole`. An `&optional` or `&key` parameter
//!   may hold a default the expander built itself. See
//!   [`crate::support::caller_supplied_parameters`].
//!
//! What is not covered: destruction through an alias
//! (`(let ((x body)) (nreverse x))` — `x` is a rebinding of the *same* list,
//! but the rebound-name guard drops `x` rather than tracking it), and
//! destruction inside a function the expander calls. Both need flow analysis;
//! neither is guessed at here.
//!
//! # Scope
//!
//! Common Lisp only. The operators are CLHS's.

use std::path::Path;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_lint_engine::LintResult;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, SyntaxTree};
use paredit_core_syntax::view_query::{is_paren_list, list_head, symbol_in, unqualified};
use serde_json::{Value, json};

use crate::support::{
    caller_supplied_parameters, definition_lambda_list, definition_name,
    for_each_evaluated_subview, is_unevaluated_at, names_bound_within, variable_name,
};

/// The two definition forms whose parameters are bound to caller source.
///
/// `define-compiler-macro` is included because its parameters are destructured
/// from the call form exactly as `defmacro`'s are, and a compiler macro that
/// destroys them corrupts the call site the same way.
pub const DEFINITION_HEADS: [&str; 2] = ["defmacro", "define-compiler-macro"];

/// A destructive CLHS operator, and which of its arguments it is permitted to
/// destroy.
///
/// `Argument(n)` means the operator may destroy the form at child `n`.
/// `AllArguments` is `nconc`/`nreconc`, which may destroy every argument but
/// the last.
#[derive(Debug, Clone, Copy)]
enum Destroys {
    Argument(usize),
    /// Every argument except the final one, which is only linked to.
    AllButLast,
}

/// The operators this rule reports, and the argument each one destroys.
///
/// Deliberately restricted to operators CLHS explicitly permits to destroy
/// their argument — the `n`-prefixed family, `sort`/`stable-sort`,
/// `delete`/`delete-duplicates`, the `rplac` pair and the sequence fillers.
/// `push`, `pop`, `setf` and `setq` of a **bare name** are absent on purpose:
/// they re-point the expander's own local variable and leave the caller's list
/// untouched, which is ordinary correct code.
const DESTRUCTIVE: &[(&str, Destroys)] = &[
    ("nreverse", Destroys::Argument(1)),
    ("nbutlast", Destroys::Argument(1)),
    ("nsubst", Destroys::Argument(3)),
    ("nsubst-if", Destroys::Argument(3)),
    ("nsubst-if-not", Destroys::Argument(3)),
    ("nsublis", Destroys::Argument(2)),
    ("nsubstitute", Destroys::Argument(3)),
    ("nsubstitute-if", Destroys::Argument(3)),
    ("nsubstitute-if-not", Destroys::Argument(3)),
    ("nunion", Destroys::Argument(1)),
    ("nintersection", Destroys::Argument(1)),
    ("nset-difference", Destroys::Argument(1)),
    ("nset-exclusive-or", Destroys::Argument(1)),
    ("nconc", Destroys::AllButLast),
    ("nreconc", Destroys::Argument(1)),
    ("sort", Destroys::Argument(1)),
    ("stable-sort", Destroys::Argument(1)),
    ("merge", Destroys::Argument(2)),
    ("delete", Destroys::Argument(2)),
    ("delete-if", Destroys::Argument(2)),
    ("delete-if-not", Destroys::Argument(2)),
    ("delete-duplicates", Destroys::Argument(1)),
    ("remf", Destroys::Argument(1)),
    ("rplaca", Destroys::Argument(1)),
    ("rplacd", Destroys::Argument(1)),
    ("fill", Destroys::Argument(1)),
    ("replace", Destroys::Argument(1)),
    ("map-into", Destroys::Argument(1)),
];

/// The `setf` places that mutate the structure a name designates rather than
/// the name itself, and which child of the place holds that name.
const MUTATING_PLACES: &[(&str, usize)] = &[
    ("car", 1),
    ("cdr", 1),
    ("caar", 1),
    ("cadr", 1),
    ("cdar", 1),
    ("cddr", 1),
    ("first", 1),
    ("rest", 1),
    ("second", 1),
    ("third", 1),
    ("nth", 2),
    ("nthcdr", 2),
    ("elt", 1),
    ("aref", 1),
    ("svref", 1),
    ("subseq", 1),
    ("getf", 1),
];

fn destroys_for(head: &str) -> Option<Destroys> {
    let name = unqualified(head).to_ascii_lowercase();
    DESTRUCTIVE
        .iter()
        .find(|(operator, _)| *operator == name)
        .map(|(_, destroys)| *destroys)
}

fn mutating_place_index(head: &str) -> Option<usize> {
    let name = unqualified(head).to_ascii_lowercase();
    MUTATING_PLACES
        .iter()
        .find(|(place, _)| *place == name)
        .map(|(_, index)| *index)
}

#[derive(Debug, Clone)]
pub struct MacroBodyDestroysArgumentFormItem {
    /// The span of the destructive call, which is the site the reader has to
    /// look at.
    pub span: ByteSpan,
    /// The macro being defined.
    pub definition: String,
    /// The parameter whose structure is destroyed.
    pub parameter: String,
    /// The operator that destroys it.
    pub operator: String,
}

impl Finding for MacroBodyDestroysArgumentFormItem {
    fn kind(&self) -> &'static str {
        "macro-body-destroys-argument-form"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("macro={}", self.definition),
            format!("parameter={}", self.parameter),
            format!("operator={}", self.operator),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("macro", json!(self.definition)),
            ("parameter", json!(self.parameter)),
            ("operator", json!(self.operator)),
        ]
    }

    fn message(&self) -> String {
        format!(
            "{} applies the destructive operator {} directly to its parameter {}, which is bound \
             to the caller's own source structure: the call site is edited in place, so a second \
             expansion of it produces a different program (SBCL 2.6.0 expands `(bad 1 2 3)` to \
             `(progn 3 2 1)`, leaves the source reading `(bad 1)`, and expands it again to \
             `(progn 1)` with no diagnostic at all). Copy first — `({} (copy-list {}))` — or use \
             the non-destructive operator",
            self.definition, self.operator, self.parameter, self.operator, self.parameter
        )
    }
}

/// Whether `view` is the bare symbol `name`.
fn is_bare_parameter(view: &ExpressionView, name: &str) -> bool {
    !is_paren_list(view) && variable_name(view).is_some_and(|found| found == name)
}

/// The parameter a destructive call destroys, if it destroys one directly.
fn destroyed_parameter(call: &ExpressionView, parameters: &[String]) -> Option<(String, String)> {
    let head = list_head(call)?;
    let operator = unqualified(head).to_ascii_lowercase();

    if let Some(destroys) = destroys_for(head) {
        let candidates: Vec<&ExpressionView> = match destroys {
            Destroys::Argument(index) => call.children.get(index).into_iter().collect(),
            // The last argument of `nconc` is only linked to, never destroyed.
            Destroys::AllButLast => {
                let last = call.children.len().saturating_sub(1);
                call.children.iter().take(last).skip(1).collect()
            }
        };
        for candidate in candidates {
            if let Some(name) = parameters
                .iter()
                .find(|name| is_bare_parameter(candidate, name))
            {
                return Some((name.clone(), operator));
            }
        }
        return None;
    }

    // `(setf (car forms) …)`: the place mutates what `forms` designates.
    if !symbol_in(head, &["setf", "psetf"]) {
        return None;
    }
    for place in call.children.iter().skip(1).step_by(2) {
        let Some(place_head) = list_head(place) else {
            continue;
        };
        let Some(index) = mutating_place_index(place_head) else {
            continue;
        };
        let Some(target) = place.children.get(index) else {
            continue;
        };
        if let Some(name) = parameters
            .iter()
            .find(|name| is_bare_parameter(target, name))
        {
            return Some((
                name.clone(),
                format!("setf {}", unqualified(place_head).to_ascii_lowercase()),
            ));
        }
    }
    None
}

/// Examines one `defmacro`/`define-compiler-macro`. Shared with the lint
/// suite's rule, which reaches every node through the single dispatch pass.
///
/// The ordering here is load-bearing. The lambda-list read and the body walk
/// are both local to this form; [`is_unevaluated_at`] descends from the root
/// and is called **only once a violation is already in hand**.
pub fn examine_macro_body_destroys_argument_form(
    tree: &SyntaxTree,
    view: &ExpressionView,
    definition_count: &mut usize,
    violations: &mut Vec<MacroBodyDestroysArgumentFormItem>,
) {
    if !is_paren_list(view)
        || !list_head(view).is_some_and(|head| symbol_in(head, &DEFINITION_HEADS))
    {
        return;
    }
    let Some(lambda_list) = definition_lambda_list(view) else {
        return;
    };
    *definition_count += 1;

    let parameters = caller_supplied_parameters(lambda_list);
    if parameters.is_empty() {
        return;
    }

    // The expander body: everything after the lambda list.
    let mut found: Vec<(ByteSpan, String, String)> = Vec::new();
    let mut rebound: Option<Vec<String>> = None;
    for child in view.children.iter().skip(3) {
        for_each_evaluated_subview(child, |node| {
            if !is_paren_list(node) {
                return;
            }
            let Some((parameter, operator)) = destroyed_parameter(node, &parameters) else {
                return;
            };
            // Only now is the shadowing scan worth its cost, and it is computed
            // once per definition rather than once per candidate.
            let shadowed = rebound.get_or_insert_with(|| {
                view.children
                    .iter()
                    .skip(3)
                    .flat_map(names_bound_within)
                    .collect()
            });
            if shadowed.contains(&parameter) {
                return;
            }
            found.push((node.span, parameter, operator));
        });
    }
    if found.is_empty() {
        return;
    }
    if is_unevaluated_at(tree, view.span) {
        return;
    }

    let definition = definition_name(view).unwrap_or_else(|| "?".to_owned());
    for (span, parameter, operator) in found {
        violations.push(MacroBodyDestroysArgumentFormItem {
            span,
            definition: definition.clone(),
            parameter,
            operator,
        });
    }
}

/// Collects every destructive macro expander in one file, with the number of
/// macro definitions scanned as the denominator beside them.
pub fn build_macro_body_destroys_argument_form_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<MacroBodyDestroysArgumentFormItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("macro_definition_count", json!(0))],
        ));
    }

    let mut definition_count = 0;
    let mut violations = Vec::new();
    let root = tree.root_view();
    let mut stack = vec![&root];
    while let Some(node) = stack.pop() {
        examine_macro_body_destroys_argument_form(
            tree,
            node,
            &mut definition_count,
            &mut violations,
        );
        stack.extend(node.children.iter());
    }
    violations.sort_by_key(|item| item.span.start().get());

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("macro_definition_count", json!(definition_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<MacroBodyDestroysArgumentFormItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_macro_body_destroys_argument_form_report(
            Path::new("test.lisp"),
            Dialect::CommonLisp,
            &tree,
        )
        .expect("build report")
    }

    fn violations(input: &str) -> Vec<MacroBodyDestroysArgumentFormItem> {
        report(input).findings
    }

    // ---- the shape the SBCL run demonstrated -------------------------------

    #[test]
    fn flags_nreverse_of_a_body_parameter() {
        let found = violations("(defmacro bad (&body forms) `(progn ,@(nreverse forms)))");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].parameter, "forms");
        assert_eq!(found[0].operator, "nreverse");
    }

    #[test]
    fn flags_sort_of_a_body_parameter() {
        let found = violations("(defmacro bad (&body body) `(progn ,@(sort body #'string<)))");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].operator, "sort");
    }

    #[test]
    fn flags_a_destroyed_required_parameter() {
        let found = violations("(defmacro bad (clauses) `(cond ,@(nreverse clauses)))");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].parameter, "clauses");
    }

    #[test]
    fn flags_a_destroyed_whole_form() {
        let found = violations("(defmacro bad (&whole w a) (nreverse w) a)");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].parameter, "w");
    }

    #[test]
    fn flags_a_setf_of_a_place_on_a_parameter() {
        let found = violations("(defmacro bad (form) (setf (car form) 'other) form)");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].operator, "setf car");
    }

    #[test]
    fn flags_a_compiler_macro_that_destroys_its_argument() {
        let found = violations("(define-compiler-macro f (&rest args) `(g ,@(nreverse args)))");
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn flags_nconc_of_a_parameter_that_is_not_the_last_argument() {
        let found = violations("(defmacro bad (&body body) `(progn ,@(nconc body (list 1))))");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].operator, "nconc");
    }

    // ---- the permitted neighbours ------------------------------------------

    /// The whole discriminator: a fresh list is the expander's to destroy.
    #[test]
    fn does_not_flag_a_destructive_call_on_a_copy() {
        assert!(
            violations("(defmacro ok (&body forms) `(progn ,@(nreverse (copy-list forms))))")
                .is_empty()
        );
        assert!(
            violations("(defmacro ok (&body b) `(progn ,@(sort (copy-seq b) #'string<)))")
                .is_empty()
        );
        assert!(
            violations("(defmacro ok (&body b) `(progn ,@(nreverse (mapcar #'expand b))))")
                .is_empty()
        );
    }

    /// `nreverse` of a freshly accumulated local is the commonest correct use
    /// of the operator there is.
    #[test]
    fn does_not_flag_nreverse_of_a_local_accumulator() {
        assert!(
            violations(
                "(defmacro ok (&body forms)\n\
                 \x20 (let ((out '()))\n\
                 \x20   (dolist (f forms) (push (list 'eval f) out))\n\
                 \x20   `(progn ,@(nreverse out))))"
            )
            .is_empty()
        );
    }

    /// A rebound parameter is a fresh list from that point on, and this
    /// analysis is not flow-sensitive, so it declines.
    #[test]
    fn does_not_flag_a_parameter_the_expander_rebinds() {
        assert!(
            violations(
                "(defmacro ok (&body body)\n\
                 \x20 (let ((body (copy-list body)))\n\
                 \x20   `(progn ,@(nreverse body))))"
            )
            .is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_parameter_the_expander_reassigns() {
        assert!(
            violations(
                "(defmacro ok (&body body)\n\
                 \x20 (setf body (copy-list body))\n\
                 \x20 `(progn ,@(nreverse body)))"
            )
            .is_empty()
        );
    }

    /// The last argument of `nconc` is linked to, not destroyed.
    #[test]
    fn does_not_flag_a_parameter_in_the_final_position_of_nconc() {
        assert!(violations("(defmacro ok (&body b) `(progn ,@(nconc (list 1) b)))").is_empty());
    }

    /// An `&optional`/`&key` parameter may hold a default the expander built.
    #[test]
    fn does_not_flag_a_defaultable_parameter() {
        assert!(
            violations("(defmacro ok (a &optional (extras (list 1))) (nreverse extras) a)")
                .is_empty()
        );
    }

    /// `setf` of a bare name re-points the expander's own local variable and
    /// leaves the caller's list alone.
    #[test]
    fn does_not_flag_setf_or_push_of_a_bare_parameter_name() {
        assert!(violations("(defmacro ok (&body b) (setf b (list 1)) `(progn ,@b))").is_empty());
        assert!(violations("(defmacro ok (&body b) (push 1 b) `(progn ,@b))").is_empty());
    }

    /// The operator name has to be the *head*, not an argument.
    #[test]
    fn does_not_flag_a_parameter_merely_passed_to_a_harmless_operator() {
        assert!(violations("(defmacro ok (&body b) `(progn ,@(reverse b)))").is_empty());
        assert!(violations("(defmacro ok (&body b) `(progn ,@(remove nil b)))").is_empty());
        assert!(violations("(defmacro ok (&body b) `(progn ,@(butlast b)))").is_empty());
    }

    // ---- the quote model ---------------------------------------------------

    /// The destructive call must be **expander code**. Written plainly in the
    /// template it is part of the expansion, and destroys a list the caller's
    /// own code built at run time.
    #[test]
    fn does_not_flag_a_destructive_call_written_into_the_template() {
        assert!(violations("(defmacro ok (&body b) `(nreverse ,@b))").is_empty());
        assert!(
            violations("(defmacro ok (forms) `(let ((x ,forms)) (nreverse forms)))").is_empty()
        );
    }

    /// …but under an unquote it is expander code again, and the rule must see
    /// it. This is the direction a single depth counter gets wrong.
    #[test]
    fn flags_a_destructive_call_under_an_unquote() {
        assert_eq!(
            violations("(defmacro bad (&body b) `(progn ,(nreverse b)))").len(),
            1
        );
        assert_eq!(
            violations("(defmacro bad (&body b) `(progn ,@(nreverse b)))").len(),
            1
        );
    }

    /// A comma inside a **hard** quote is a comma character in a literal list,
    /// not an escape: there is no backquote for it to escape. `hard` must
    /// therefore never clear, and a single depth counter — which would count
    /// the `'` up and the `,` back down to zero — reads this as code and
    /// reports it.
    ///
    /// This is the walk's half of the two-counter model; the descent's half is
    /// `support::tests::a_comma_inside_a_hard_quote_stays_data`. Both are
    /// needed: they are different functions.
    #[test]
    fn does_not_flag_a_destructive_call_under_a_comma_inside_a_hard_quote() {
        assert!(
            violations("(defmacro ok (&body b) '(progn ,(nreverse b)))").is_empty(),
            "a comma under ' is a character in a literal list, not an escape to code"
        );
        assert!(violations("(defmacro ok (&body b) '(progn ,@(nreverse b)))").is_empty());
    }

    /// The long-hand `(quote …)`, which hand-written expanders and macro output
    /// both spell out. It makes its contents data exactly as `'` does, and the
    /// walk has to know that separately from the reader-prefix case.
    #[test]
    fn does_not_flag_a_destructive_call_under_a_long_hand_quote_form() {
        assert!(
            violations("(defmacro ok (&body b) (quote (progn (nreverse b))))").is_empty(),
            "(quote …) makes its contents data just as ' does"
        );
        assert!(
            violations("(defmacro ok (&body b) (list 'progn (quote (sort b #'string<))))")
                .is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_defmacro_inside_quoted_data() {
        assert!(
            violations("(setf template '(defmacro bad (&body b) `(progn ,@(nreverse b))))")
                .is_empty()
        );
    }

    #[test]
    fn flags_a_defmacro_nested_in_a_larger_evaluated_form() {
        assert_eq!(
            violations(
                "(eval-when (:compile-toplevel)\n\
                 \x20 (defmacro bad (&body b) `(progn ,@(nreverse b))))"
            )
            .len(),
            1
        );
    }

    // ---- report plumbing ---------------------------------------------------

    #[test]
    fn the_denominator_counts_every_definition_scanned_not_only_the_flagged_ones() {
        let scanned = report(
            "(defmacro a (&body b) `(progn ,@b))\n\
             (defmacro b (&body b) `(progn ,@(nreverse b)))\n\
             (define-compiler-macro c (x) x)",
        );
        assert_eq!(scanned.summary, vec![("macro_definition_count", json!(3))]);
        assert_eq!(scanned.findings.len(), 1);
    }

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(defmacro m [b] b)", Dialect::Clojure).expect("parse");
        let built = build_macro_body_destroys_argument_form_report(
            Path::new("a.clj"),
            Dialect::Clojure,
            &tree,
        )
        .expect("build report");
        assert!(!built.dialect_modelled);
        assert!(built.findings.is_empty());
    }

    #[test]
    fn the_message_names_the_parameter_the_operator_and_a_repair() {
        let built = report("(defmacro bad (&body forms) `(progn ,@(nreverse forms)))");
        let message = built.findings[0].message();
        assert!(message.contains("forms"), "{message}");
        assert!(message.contains("nreverse"), "{message}");
        assert!(message.contains("copy-list"), "{message}");
    }

    #[test]
    fn folds_case_the_way_the_reader_would() {
        assert_eq!(
            violations("(DEFMACRO bad (&BODY forms) `(progn ,@(NREVERSE forms)))").len(),
            1
        );
    }

    /// A macro with no caller-supplied parameter cannot destroy one.
    #[test]
    fn a_nullary_macro_is_counted_but_never_flagged() {
        let scanned = report("(defmacro version () \"1.0\")");
        assert_eq!(scanned.summary, vec![("macro_definition_count", json!(1))]);
        assert!(scanned.findings.is_empty());
    }
}
