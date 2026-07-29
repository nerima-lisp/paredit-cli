//! Unused function-parameter detection: a declared parameter with no
//! unshadowed reference anywhere in its definition's body.
//!
//! Parameter names are extracted with the same validated lambda-list parser
//! the parameter add/remove/reorder refactors use
//! ([`crate::function_parameter::domain::list_lambda_list_parameter_names`]),
//! and usage is checked with the same shadow-aware traversal `inline-let`
//! relies on to avoid false captures
//! ([`paredit_core_semantics::lexical_scope::collect_unshadowed_symbol_references`]).
//! Reusing both keeps this report's notion of "parameter" and "reference"
//! identical to the refactors that already depend on them, rather than
//! drifting from a second, independently-written lambda-list parser.

use std::path::PathBuf;

use paredit_core_cli::CliResult;

use crate::function_parameter::domain::list_lambda_list_parameter_names_from;
use paredit_core_semantics::lexical_scope::collect_unshadowed_symbol_references;
use paredit_core_syntax::common_lisp::CommonLispPackageDeclarationForm;
use paredit_core_syntax::definition::{DefinitionCategory, definition_shape};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{
    ByteSpan, Delimiter, ExpressionKind, ExpressionView, Path, SymbolName, SyntaxTree,
};

fn atom_text(view: &ExpressionView) -> Option<&str> {
    (view.kind == ExpressionKind::Atom)
        .then_some(view.text.as_deref())
        .flatten()
}

fn atom_child(view: &ExpressionView, index: usize) -> Option<&str> {
    view.children.get(index).and_then(atom_text)
}

fn list_head(view: &ExpressionView) -> Option<&str> {
    if view.kind != ExpressionKind::List || view.delimiter != Some(Delimiter::Paren) {
        return None;
    }

    atom_child(view, 0)
}

/// Whether `name` follows the cross-dialect convention for a deliberately
/// unused parameter (a bare `_`, or a leading underscore), which should never
/// be flagged.
fn is_conventionally_ignored(name: &str) -> bool {
    name == "_" || name.starts_with('_')
}

#[derive(Debug, Clone)]
pub struct UnusedParameterReportItem {
    pub definition_path: String,
    pub definition_span: ByteSpan,
    pub head: String,
    pub definition_name: Option<String>,
    pub category: DefinitionCategory,
    pub parameter_name: String,
}

#[derive(Debug)]
pub struct UnusedParameterReportFile {
    pub path: PathBuf,
    pub dialect: Dialect,
    /// Callable definitions with a parseable lambda list that were checked.
    pub checked_definition_count: usize,
    pub unused_parameters: Vec<UnusedParameterReportItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct UnusedParameterReportPolicyOptions {
    fail_on_unused: bool,
}

impl UnusedParameterReportPolicyOptions {
    #[must_use]
    pub const fn new(fail_on_unused: bool) -> Self {
        Self { fail_on_unused }
    }

    #[must_use]
    pub const fn fail_on_unused(self) -> bool {
        self.fail_on_unused
    }
}

#[derive(Debug)]
pub struct UnusedParameterReportPolicy {
    pub fail_on_unused: bool,
    pub checked_definition_count: usize,
    pub unused_parameter_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

pub fn build_unused_parameter_report(
    path: PathBuf,
    dialect: Dialect,
    tree: &SyntaxTree,
    input: &str,
) -> CliResult<UnusedParameterReportFile> {
    let mut unused_parameters = Vec::new();
    let mut checked_definition_count = 0;

    for index in 0..tree.root_children().len() {
        let form_path = Path::root_child(index);
        let view = tree.select_path(&form_path)?.view();
        let Some(head) = list_head(&view) else {
            continue;
        };
        if dialect.common_lisp_package_declaration_form_for_head(head)
            == Some(CommonLispPackageDeclarationForm::InPackage)
        {
            continue;
        }
        let Some(shape) = definition_shape(dialect, &view, head) else {
            continue;
        };
        if !shape.category.is_callable() {
            continue;
        }
        let Some(lambda_list) = shape.lambda_list(&view) else {
            continue;
        };
        // A definition whose lambda list this parser cannot classify (e.g. an
        // unsupported marker) is skipped rather than failing the whole scan:
        // one unusual macro's parameter DSL should not block reporting on
        // every other definition in the file.
        let Ok(parameter_names) = list_lambda_list_parameter_names_from(
            dialect,
            lambda_list,
            shape.lambda_list_first_parameter_index(),
        ) else {
            continue;
        };
        checked_definition_count += 1;

        let body_forms = shape.body_forms(&view);
        for name in parameter_names {
            if is_conventionally_ignored(&name) {
                continue;
            }
            let Ok(symbol) = SymbolName::new(name.clone()) else {
                continue;
            };
            let mut references = Vec::new();
            for body_form in body_forms {
                collect_unshadowed_symbol_references(
                    dialect,
                    body_form,
                    &symbol,
                    input,
                    &mut references,
                );
            }
            if references.is_empty() {
                unused_parameters.push(UnusedParameterReportItem {
                    definition_path: form_path.to_string(),
                    definition_span: view.span,
                    head: head.to_owned(),
                    definition_name: shape.name(&view).map(ToOwned::to_owned),
                    category: shape.category,
                    parameter_name: name,
                });
            }
        }
    }

    Ok(UnusedParameterReportFile {
        path,
        dialect,
        checked_definition_count,
        unused_parameters,
    })
}

#[must_use]
pub fn evaluate_unused_parameter_policy(
    options: UnusedParameterReportPolicyOptions,
    reports: &[UnusedParameterReportFile],
) -> UnusedParameterReportPolicy {
    let checked_definition_count = reports
        .iter()
        .map(|report| report.checked_definition_count)
        .sum::<usize>();
    let unused_parameter_count = reports
        .iter()
        .map(|report| report.unused_parameters.len())
        .sum::<usize>();

    let mut violations = Vec::new();
    if options.fail_on_unused() && unused_parameter_count > 0 {
        violations.push(format!(
            "unused_parameter_count {unused_parameter_count} exceeds 0"
        ));
    }

    UnusedParameterReportPolicy {
        fail_on_unused: options.fail_on_unused(),
        checked_definition_count,
        unused_parameter_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> UnusedParameterReportFile {
        let tree = SyntaxTree::parse(input).expect("parse input");
        build_unused_parameter_report(
            PathBuf::from("test.lisp"),
            Dialect::CommonLisp,
            &tree,
            input,
        )
        .expect("build unused parameter report")
    }

    #[test]
    fn flags_a_parameter_never_referenced_in_the_body() {
        let report = report("(defun f (x y) (+ x 1))");

        assert_eq!(report.checked_definition_count, 1);
        assert_eq!(report.unused_parameters.len(), 1);
        assert_eq!(report.unused_parameters[0].parameter_name, "y");
        assert_eq!(
            report.unused_parameters[0].definition_name.as_deref(),
            Some("f")
        );
    }

    #[test]
    fn does_not_flag_parameters_used_in_the_body() {
        let report = report("(defun f (x y) (+ x y))");
        assert!(report.unused_parameters.is_empty());
    }

    #[test]
    fn does_not_flag_conventionally_ignored_names() {
        let report = report("(defun f (x _ _ignored) (+ x 1))");
        assert!(report.unused_parameters.is_empty());
    }

    #[test]
    fn does_not_flag_parameters_shadowed_and_then_used_under_a_new_binding() {
        // `x` the parameter is never referenced; only the inner `let`'s `x`
        // is used, so the outer parameter must still be flagged.
        let report = report("(defun f (x) (let ((x 1)) (+ x 1)))");
        assert_eq!(report.unused_parameters.len(), 1);
        assert_eq!(report.unused_parameters[0].parameter_name, "x");
    }

    #[test]
    fn handles_optional_and_key_parameters() {
        let report = report("(defun f (x &optional y &key z) (+ x z))");
        assert_eq!(report.unused_parameters.len(), 1);
        assert_eq!(report.unused_parameters[0].parameter_name, "y");
    }

    #[test]
    fn ignores_in_package_and_non_callable_forms() {
        let report = report("(in-package :foo)\n(defvar *x* 1)\n(defun f (x) (+ x 1))");
        assert_eq!(report.checked_definition_count, 1);
        assert!(report.unused_parameters.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let report = report("(defun f (x y) (+ x 1))");

        let quiet = evaluate_unused_parameter_policy(
            UnusedParameterReportPolicyOptions::new(false),
            std::slice::from_ref(&report),
        );
        assert!(quiet.passed);
        assert_eq!(quiet.unused_parameter_count, 1);

        let strict = evaluate_unused_parameter_policy(
            UnusedParameterReportPolicyOptions::new(true),
            std::slice::from_ref(&report),
        );
        assert!(!strict.passed);
    }
}

// --- the edit side: turning the report above into `(declare (ignore ...))` ---

/// One definition's planned `(declare (ignore ...))` insertion.
#[derive(Debug, Clone)]
pub struct IgnoreDeclarationItem {
    pub definition_path: String,
    pub definition_name: Option<String>,
    /// The parameters this declaration will name, in lambda-list order.
    pub parameter_names: Vec<String>,
    /// Where the new form is spliced in: a zero-width point, not a
    /// replacement.
    pub insert_at: usize,
}

#[derive(Debug)]
pub struct IgnoreDeclarationPlan {
    pub path: PathBuf,
    pub dialect: Dialect,
    pub items: Vec<IgnoreDeclarationItem>,
    pub rewritten: String,
}

/// Plans a `(declare (ignore ...))` for every definition the unused-parameter
/// report flags.
///
/// The names come from [`build_unused_parameter_report`] rather than a second
/// scan, so this can never disagree with the report about which parameters
/// are unused — the gap this closes is that the report had no write side at
/// all, not that its answer needed refining.
///
/// Scoped to the Common Lisp family, which is the only place `declare` means
/// this. Other dialects get an empty plan rather than a refusal: a workspace
/// sweep should skip a `.clj` file, not stop at it.
pub fn plan_ignore_declarations(
    path: PathBuf,
    dialect: Dialect,
    tree: &SyntaxTree,
    input: &str,
) -> CliResult<IgnoreDeclarationPlan> {
    let report = build_unused_parameter_report(path.clone(), dialect, tree, input)?;
    if !matches!(dialect, Dialect::CommonLisp | Dialect::EmacsLisp) {
        return Ok(IgnoreDeclarationPlan {
            path,
            dialect,
            items: Vec::new(),
            rewritten: input.to_owned(),
        });
    }

    // One item per definition, not per parameter: `(declare (ignore x y))` is
    // one form naming both, and two separate declarations of the same body
    // would be a rewrite nobody writes by hand.
    let mut items: Vec<IgnoreDeclarationItem> = Vec::new();
    for unused in &report.unused_parameters {
        if let Some(existing) = items
            .iter_mut()
            .find(|item| item.definition_path == unused.definition_path)
        {
            existing.parameter_names.push(unused.parameter_name.clone());
            continue;
        }
        let form_path: Path = unused.definition_path.parse()?;
        let view = tree.select_path(&form_path)?.view();
        let Some(head) = list_head(&view) else {
            continue;
        };
        let Some(shape) = definition_shape(dialect, &view, head) else {
            continue;
        };
        let Some(insert_at) = declaration_insertion_offset(&view, shape.body_forms(&view)) else {
            continue;
        };
        items.push(IgnoreDeclarationItem {
            definition_path: unused.definition_path.clone(),
            definition_name: unused.definition_name.clone(),
            parameter_names: vec![unused.parameter_name.clone()],
            insert_at,
        });
    }

    // Applied last-first so an earlier insertion never shifts a later offset.
    let mut ordered = items.clone();
    ordered.sort_by_key(|item| std::cmp::Reverse(item.insert_at));
    let mut rewritten = input.to_owned();
    for item in &ordered {
        let names = item.parameter_names.join(" ");
        let declaration = format!("(declare (ignore {names}))");
        let insertion = match own_line_indent_at(&rewritten, item.insert_at) {
            // The body form the declaration is placed above starts its own
            // line, so the declaration takes a line of its own and is aligned
            // with it.
            Some(indent) => format!("{declaration}\n{indent}"),
            // A single-line definition. Breaking the line here would leave the
            // rest of the definition at column 0 —
            // `(defun f (x y) (declare (ignore y))\nx)` — so the declaration
            // stays on the line it was inserted into instead.
            None => inline_declaration(&rewritten, item.insert_at, &declaration),
        };
        rewritten.insert_str(item.insert_at, &insertion);
    }

    Ok(IgnoreDeclarationPlan {
        path,
        dialect,
        items,
        rewritten,
    })
}

/// Where a new declaration goes: after the lambda list, past a docstring and
/// past any declarations already there.
///
/// Returns `None` for a definition with no body at all, which has no position
/// that is inside it and after its lambda list.
fn declaration_insertion_offset(view: &ExpressionView, body: &[ExpressionView]) -> Option<usize> {
    let mut index = 0usize;
    // A leading string is a docstring only when something follows it. As the
    // sole body form it is the definition's return value, and a declaration
    // inserted after it would be dead code below the return.
    if body.len() > 1 && body[0].kind == ExpressionKind::Atom {
        let is_string = body[0]
            .text
            .as_deref()
            .is_some_and(|text| text.starts_with('"'));
        if is_string {
            index = 1;
        }
    }
    // Several `declare` forms may sit at a body's head, so an existing one is
    // something to insert *after* rather than to merge into.
    while body
        .get(index)
        .and_then(list_head)
        .is_some_and(|head| head.eq_ignore_ascii_case("declare"))
    {
        index += 1;
    }
    body.get(index).map_or_else(
        || view.span.end().get().checked_sub(1),
        |form| Some(form.span.start().get()),
    )
}

/// The literal indentation of the line `offset` sits on, for keeping an
/// inserted form aligned with the one it was placed above.
///
/// `None` when something other than whitespace precedes `offset` on its line,
/// which is exactly the case where an inserted newline would strand the rest
/// of the definition on an unindented line. Returning the source's own
/// indentation rather than a count of spaces keeps a tab-indented file
/// tab-indented.
fn own_line_indent_at(input: &str, offset: usize) -> Option<&str> {
    let line_start = input[..offset].rfind('\n').map_or(0, |index| index + 1);
    let indent = &input[line_start..offset];
    indent.chars().all(char::is_whitespace).then_some(indent)
}

/// The declaration plus whatever spacing keeps it a separate form from its
/// neighbours on the line it is spliced into.
///
/// Both sides are checked because the two insertion points differ: a body form
/// already has a space before it and needs one after, while the no-body case
/// inserts against the definition's own closing delimiter and needs the
/// opposite.
fn inline_declaration(input: &str, offset: usize, declaration: &str) -> String {
    let needs_space_before = input[..offset]
        .chars()
        .next_back()
        .is_some_and(|character| {
            !character.is_whitespace() && !matches!(character, '(' | '[' | '{')
        });
    let needs_space_after = input[offset..].chars().next().is_some_and(|character| {
        !character.is_whitespace() && !matches!(character, ')' | ']' | '}')
    });
    let before = if needs_space_before { " " } else { "" };
    let after = if needs_space_after { " " } else { "" };
    format!("{before}{declaration}{after}")
}

#[cfg(test)]
mod ignore_declaration_tests {
    use super::*;

    fn plan(source: &str) -> IgnoreDeclarationPlan {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp)
            .expect("fixture parses as Common Lisp");
        plan_ignore_declarations(
            PathBuf::from("fixture.lisp"),
            Dialect::CommonLisp,
            &tree,
            source,
        )
        .expect("planning succeeds")
    }

    #[test]
    fn declares_an_unused_parameter_after_the_lambda_list() {
        let planned = plan("(defun f (x y)\n  x)\n");
        assert_eq!(
            planned.rewritten,
            "(defun f (x y)\n  (declare (ignore y))\n  x)\n"
        );
        assert_eq!(planned.items.len(), 1);
        assert_eq!(planned.items[0].parameter_names, vec!["y".to_owned()]);
    }

    #[test]
    fn a_docstring_keeps_its_place_above_the_declaration() {
        let planned = plan("(defun f (x y)\n  \"Doc.\"\n  x)\n");
        assert_eq!(
            planned.rewritten,
            "(defun f (x y)\n  \"Doc.\"\n  (declare (ignore y))\n  x)\n"
        );
    }

    #[test]
    fn an_existing_declaration_is_followed_rather_than_merged_into() {
        // Several `declare` forms may head one body, so appending is valid and
        // leaves the existing declaration's own specs untouched.
        let planned = plan("(defun f (x y)\n  (declare (type fixnum x))\n  x)\n");
        assert_eq!(
            planned.rewritten,
            "(defun f (x y)\n  (declare (type fixnum x))\n  (declare (ignore y))\n  x)\n"
        );
    }

    #[test]
    fn a_parameter_already_declared_ignored_is_not_declared_twice() {
        // The report counts the name's appearance inside `(declare (ignore y))`
        // as a reference, so it never reaches the edit side. Declaring it a
        // second time would be a duplicate declaration, which is an error.
        let source = "(defun f (x y)\n  (declare (ignore y))\n  x)\n";
        let planned = plan(source);
        assert!(planned.items.is_empty());
        assert_eq!(planned.rewritten, source);
    }

    #[test]
    fn several_unused_parameters_share_one_declaration() {
        let planned = plan("(defun f (x y z)\n  x)\n");
        assert_eq!(planned.items.len(), 1);
        assert_eq!(
            planned.items[0].parameter_names,
            vec!["y".to_owned(), "z".to_owned()]
        );
        assert_eq!(
            planned.rewritten,
            "(defun f (x y z)\n  (declare (ignore y z))\n  x)\n"
        );
    }

    #[test]
    fn two_definitions_are_both_rewritten_without_shifting_each_other() {
        let planned = plan("(defun f (x y)\n  x)\n\n(defun g (a b)\n  a)\n");
        assert_eq!(planned.items.len(), 2);
        assert_eq!(
            planned.rewritten,
            "(defun f (x y)\n  (declare (ignore y))\n  x)\n\n(defun g (a b)\n  (declare (ignore b))\n  a)\n"
        );
    }

    #[test]
    fn a_lone_string_body_is_a_return_value_not_a_docstring() {
        // `(defun f (x y) "text")` returns the string; a declaration inserted
        // after it would sit below the definition's only form. Both parameters
        // are unused here precisely because the body references neither.
        let planned = plan("(defun f (x y)\n  \"text\")\n");
        assert_eq!(
            planned.rewritten,
            "(defun f (x y)\n  (declare (ignore x y))\n  \"text\")\n"
        );
    }

    #[test]
    fn a_single_line_definition_stays_on_one_line() {
        // Every other fixture here is multi-line, which is why this went
        // unnoticed: the insertion assumed its offset began a line, and
        // produced `(defun f (x y) (declare (ignore y))\nx)` — the body left
        // at column 0, outside the header it belongs under.
        let planned = plan("(defun f (x y) x)\n");
        assert_eq!(
            planned.rewritten,
            "(defun f (x y) (declare (ignore y)) x)\n"
        );
    }

    #[test]
    fn a_single_line_definition_with_no_body_keeps_its_delimiters_tidy() {
        let planned = plan("(defun f (x y))\n");
        assert_eq!(
            planned.rewritten,
            "(defun f (x y) (declare (ignore x y)))\n"
        );
    }

    #[test]
    fn a_tab_indented_body_keeps_its_own_indentation() {
        let planned = plan("(defun f (x y)\n\tx)\n");
        assert_eq!(
            planned.rewritten,
            "(defun f (x y)\n\t(declare (ignore y))\n\tx)\n"
        );
    }

    #[test]
    fn the_rewrite_still_parses() {
        for source in [
            "(defun f (x y)\n  x)\n",
            "(defun f (x y)\n  \"Doc.\"\n  x)\n",
            "(defun f (x y z)\n  x)\n",
            "(defun f (x y) x)\n",
            "(defun f (x y))\n",
        ] {
            let planned = plan(source);
            SyntaxTree::parse_with_dialect(&planned.rewritten, Dialect::CommonLisp)
                .expect("a planned rewrite must reparse");
        }
    }
}
