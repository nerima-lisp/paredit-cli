//! Dependency-preserving conversions between Common Lisp `let` forms.

use crate::error::{
    BindingRefusal, ConservativeRefusal, DialectRefusal, DocumentRefusal, EditResult, ShapeRefusal,
};

use paredit_core_semantics::lexical_scope::collect_unshadowed_symbol_references;
use paredit_core_syntax::common_lisp::common_lisp_symbol_reference_eq;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::reader::atom_symbol_text;
use paredit_core_syntax::sexpr::{
    ByteSpan, ExpressionKind, ExpressionView, Path, SymbolName, SyntaxTree,
};

#[derive(Debug, Clone)]
pub struct ConvertLetToLetStarRequest<'a> {
    pub input: &'a str,
    pub dialect: Dialect,
    pub path: Path,
}
#[derive(Debug, Clone)]
pub struct ConvertLetToLetStarPlan {
    pub dialect: Dialect,
    pub path: Path,
    pub form_span: ByteSpan,
    pub binding_names: Vec<SymbolName>,
    pub rewritten: String,
    pub changed: bool,
}

pub fn validate_convert_let_to_let_star_dialect(dialect: Dialect) -> EditResult<()> {
    if !matches!(dialect, Dialect::CommonLisp | Dialect::EmacsLisp) {
        return Err(DialectRefusal::CommonLispAndEmacsLisp {
            operation: "convert-let-to-let-star",
        }
        .into());
    }
    Ok(())
}

pub fn validate_convert_let_star_to_let_dialect(dialect: Dialect) -> EditResult<()> {
    if dialect != Dialect::CommonLisp {
        return Err(DialectRefusal::CurrentlyCommonLispOnly {
            operation: "convert-let-star-to-let",
        }
        .into());
    }
    Ok(())
}

pub fn plan_convert_let_to_let_star(
    request: ConvertLetToLetStarRequest<'_>,
) -> EditResult<ConvertLetToLetStarPlan> {
    validate_convert_let_to_let_star_dialect(request.dialect)?;
    let tree =
        SyntaxTree::parse_with_dialect(request.input, request.dialect).map_err(|source| {
            DocumentRefusal::InputInvalid {
                operation: "convert-let-to-let-star",
                source,
            }
        })?;
    let form = tree.select_path(&request.path)?.view();
    validate_form(
        &form,
        &tree,
        request.dialect,
        "let",
        "convert-let-to-let-star",
    )?;
    let (names, initializers) =
        analyze_bindings(&form, request.dialect, "convert-let-to-let-star")?;
    reject_dependencies(&names, &initializers, &request, "convert-let-to-let-star")?;
    let rewritten = replace_head(request.input, form.children[0].span, "let*");
    parse_output(&rewritten, request.dialect, "convert-let-to-let-star")?;
    Ok(ConvertLetToLetStarPlan {
        dialect: request.dialect,
        path: request.path,
        form_span: form.span,
        binding_names: names,
        changed: rewritten != request.input,
        rewritten,
    })
}

#[derive(Debug, Clone)]
pub struct ConvertLetStarToLetRequest<'a> {
    pub input: &'a str,
    pub dialect: Dialect,
    pub path: Path,
    /// When a real dependency exists, split off the longest independent
    /// prefix of bindings into an outer `let` instead of refusing the
    /// whole conversion.
    pub allow_partial: bool,
}
#[derive(Debug, Clone)]
pub struct ConvertLetStarToLetPlan {
    pub dialect: Dialect,
    pub path: Path,
    pub form_span: ByteSpan,
    pub binding_names: Vec<SymbolName>,
    pub rewritten: String,
    pub changed: bool,
    /// True when the output is a `let` wrapping a nested `let*` for the
    /// remaining (dependent) bindings, rather than a full conversion.
    pub partial: bool,
}

pub fn plan_convert_let_star_to_let(
    request: ConvertLetStarToLetRequest<'_>,
) -> EditResult<ConvertLetStarToLetPlan> {
    validate_convert_let_star_to_let_dialect(request.dialect)?;
    let tree =
        SyntaxTree::parse_with_dialect(request.input, request.dialect).map_err(|source| {
            DocumentRefusal::InputNotAnSexprDocument {
                operation: "convert-let-star-to-let",
                source,
            }
        })?;
    let form = tree.select_path(&request.path)?.view();
    validate_form(
        &form,
        &tree,
        request.dialect,
        "let*",
        "convert-let-star-to-let",
    )?;
    let (names, initializers) =
        analyze_bindings(&form, request.dialect, "convert-let-star-to-let")?;
    // The default path is untouched: refuse on the first real dependency,
    // exactly as before. Only `allow_partial` walks incrementally instead of
    // stopping at the first failure.
    let prefix_len = if request.allow_partial {
        independent_prefix_len(&names, &initializers, request.dialect, request.input)
    } else {
        reject_dependencies(&names, &initializers, &request, "convert-let-star-to-let")?;
        names.len()
    };
    let partial = prefix_len < names.len();
    let rewritten = if partial {
        split_independent_prefix(request.input, &form, prefix_len)
    } else {
        replace_head(request.input, form.children[0].span, "let")
    };
    parse_output(&rewritten, request.dialect, "convert-let-star-to-let")?;
    Ok(ConvertLetStarToLetPlan {
        dialect: request.dialect,
        path: request.path,
        form_span: form.span,
        binding_names: names,
        changed: rewritten != request.input,
        partial,
        rewritten,
    })
}

/// Walks bindings in source order and returns the length of the longest
/// prefix (starting at index 0) whose initializers reference no earlier
/// binding in that same prefix. This is the same per-pair check
/// `reject_dependencies` performs, just walked to find where it first fails
/// instead of refusing on the first failure.
///
/// Binding index 0 has no earlier bindings in `names[..0]` to reference, so
/// the loop body for `index == 0` never runs and this function can never
/// return `0`: the earliest a dependency can be found is at `index == 1`,
/// which yields a prefix length of `1`. See
/// `first_binding_can_never_shrink_the_prefix_to_zero` below.
fn independent_prefix_len(
    names: &[SymbolName],
    initializers: &[Option<ExpressionView>],
    dialect: Dialect,
    input: &str,
) -> usize {
    for (index, initializer) in initializers.iter().enumerate() {
        let Some(initializer) = initializer else {
            continue;
        };
        for earlier in &names[..index] {
            let mut references = Vec::new();
            collect_unshadowed_symbol_references(
                dialect,
                initializer,
                earlier,
                input,
                &mut references,
            );
            if !references.is_empty() {
                return index;
            }
        }
    }
    names.len()
}

/// Splits a `let*` form into `(let (prefix) (let* (remainder) body))` at
/// `prefix_len`, reusing `let_star_composition::plan`'s span-slicing
/// technique: each binding's original source text is copied verbatim via
/// `ByteSpan`, never reformatted or re-printed from a parsed representation.
fn split_independent_prefix(input: &str, form: &ExpressionView, prefix_len: usize) -> String {
    let bindings = &form.children[1];
    let outer = bindings.children[..prefix_len]
        .iter()
        .map(|binding| binding.span.slice(input))
        .collect::<Vec<_>>()
        .join(" ");
    let inner = bindings.children[prefix_len..]
        .iter()
        .map(|binding| binding.span.slice(input))
        .collect::<Vec<_>>()
        .join(" ");
    let body = &input[bindings.span.end().get()..form.span.end().get() - 1];
    let replacement = format!("(let ({outer}) (let* ({inner}){body}))");
    replace_head(input, form.span, &replacement)
}

fn validate_form(
    form: &ExpressionView,
    tree: &SyntaxTree,
    dialect: Dialect,
    expected: &str,
    operation: &'static str,
) -> EditResult<()> {
    if tree.has_comment_in(form.span) {
        return Err(ConservativeRefusal::Comments { operation }.into());
    }
    if form.kind != ExpressionKind::List
        || !form.reader_prefixes.is_empty()
        || !form
            .children
            .first()
            .and_then(atom_symbol_text)
            .is_some_and(|head| symbol_eq(dialect, head, expected))
    {
        return Err(ShapeRefusal::NotPlainExpectedForm {
            operation,
            expected: expected.to_owned(),
        }
        .into());
    }
    if contains_headed_form(dialect, form, "declare") {
        return Err(ConservativeRefusal::Declarations { operation }.into());
    }
    let bindings = form
        .children
        .get(1)
        .ok_or(BindingRefusal::MissingBindingList { operation })?;
    if bindings.kind != ExpressionKind::List || !bindings.reader_prefixes.is_empty() {
        return Err(BindingRefusal::NotPlainBindingList { operation }.into());
    }
    Ok(())
}

fn analyze_bindings(
    form: &ExpressionView,
    dialect: Dialect,
    operation: &'static str,
) -> EditResult<(Vec<SymbolName>, Vec<Option<ExpressionView>>)> {
    let bindings = &form.children[1];
    let mut names = Vec::with_capacity(bindings.children.len());
    let mut initializers = Vec::with_capacity(bindings.children.len());
    for binding in &bindings.children {
        let (name, initializer) = parse_binding(binding, operation)?;
        if names
            .iter()
            .any(|old: &SymbolName| symbol_eq(dialect, old.as_str(), name.as_str()))
        {
            return Err(BindingRefusal::DuplicateBindingNames { operation }.into());
        }
        names.push(name);
        initializers.push(initializer);
    }
    Ok((names, initializers))
}

fn parse_binding(
    binding: &ExpressionView,
    operation: &'static str,
) -> EditResult<(SymbolName, Option<ExpressionView>)> {
    if binding.kind == ExpressionKind::Atom {
        return Ok((plain_symbol(binding, operation)?, None));
    }
    if binding.kind != ExpressionKind::List
        || !binding.reader_prefixes.is_empty()
        || !(1..=2).contains(&binding.children.len())
    {
        return Err(BindingRefusal::Destructuring { operation }.into());
    }
    Ok((
        plain_symbol(&binding.children[0], operation)?,
        binding.children.get(1).cloned(),
    ))
}

fn plain_symbol(view: &ExpressionView, operation: &'static str) -> EditResult<SymbolName> {
    if view.kind != ExpressionKind::Atom || !view.reader_prefixes.is_empty() {
        return Err(BindingRefusal::NotPlainBindingName { operation }.into());
    }
    SymbolName::new(atom_symbol_text(view).ok_or(BindingRefusal::BindingName)?)
        .map_err(|source| BindingRefusal::InvalidBindingName { source }.into())
}

fn reject_dependencies<R>(
    names: &[SymbolName],
    initializers: &[Option<ExpressionView>],
    request: &R,
    operation: &'static str,
) -> EditResult<()>
where
    R: LetRequest + ?Sized,
{
    for (index, initializer) in initializers.iter().enumerate() {
        let Some(initializer) = initializer else {
            continue;
        };
        for earlier in &names[..index] {
            let mut references = Vec::new();
            collect_unshadowed_symbol_references(
                request.dialect(),
                initializer,
                earlier,
                request.input(),
                &mut references,
            );
            if !references.is_empty() {
                return Err(BindingRefusal::ReferencesEarlierBinding {
                    operation,
                    earlier: earlier.to_string(),
                }
                .into());
            }
        }
    }
    Ok(())
}

trait LetRequest {
    fn input(&self) -> &str;
    fn dialect(&self) -> Dialect;
}
impl<'a> LetRequest for ConvertLetToLetStarRequest<'a> {
    fn input(&self) -> &str {
        self.input
    }
    fn dialect(&self) -> Dialect {
        self.dialect
    }
}
impl<'a> LetRequest for ConvertLetStarToLetRequest<'a> {
    fn input(&self) -> &str {
        self.input
    }
    fn dialect(&self) -> Dialect {
        self.dialect
    }
}

fn symbol_eq(dialect: Dialect, left: &str, right: &str) -> bool {
    dialect != Dialect::CommonLisp && left == right
        || dialect == Dialect::CommonLisp && common_lisp_symbol_reference_eq(left, right)
}
fn contains_headed_form(dialect: Dialect, view: &ExpressionView, expected: &str) -> bool {
    (view.kind == ExpressionKind::List
        && view.reader_prefixes.is_empty()
        && view
            .children
            .first()
            .and_then(atom_symbol_text)
            .is_some_and(|head| symbol_eq(dialect, head, expected)))
        || view
            .children
            .iter()
            .any(|child| contains_headed_form(dialect, child, expected))
}
fn replace_head(input: &str, span: ByteSpan, replacement: &str) -> String {
    let mut output = String::with_capacity(input.len() - span.len() + replacement.len());
    output.push_str(&input[..span.start().get()]);
    output.push_str(replacement);
    output.push_str(&input[span.end().get()..]);
    output
}
fn parse_output(output: &str, dialect: Dialect, operation: &'static str) -> EditResult<()> {
    SyntaxTree::parse_with_dialect(output, dialect)
        .map_err(|source| DocumentRefusal::OutputInvalid { operation, source })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_safe_bindings_and_rejects_dependencies() {
        let path: Path = "0".parse().expect("path");
        for dialect in [Dialect::CommonLisp, Dialect::EmacsLisp] {
            let plan = plan_convert_let_to_let_star(ConvertLetToLetStarRequest {
                input: "(let ((x 1) (y 2)) (+ x y))",
                dialect,
                path: path.clone(),
            })
            .expect("plan");
            assert_eq!(plan.rewritten, "(let* ((x 1) (y 2)) (+ x y))");
            assert!(
                plan_convert_let_to_let_star(ConvertLetToLetStarRequest {
                    input: "(let ((x 1) (y (+ x 2))) y)",
                    dialect,
                    path: path.clone()
                })
                .is_err()
            );
        }
        assert!(
            plan_convert_let_star_to_let(ConvertLetStarToLetRequest {
                input: "(let* ((x 1) (y 2)) (+ x y))",
                dialect: Dialect::CommonLisp,
                path,
                allow_partial: false,
            })
            .is_ok()
        );
    }
    #[test]
    fn rejects_shadowing_ambiguity_comments_declarations_and_dialects() {
        let path: Path = "0".parse().expect("path");
        assert!(
            plan_convert_let_to_let_star(ConvertLetToLetStarRequest {
                input: "(let ((x 1) (X 2)) x)",
                dialect: Dialect::CommonLisp,
                path: path.clone()
            })
            .is_err()
        );
        assert!(
            plan_convert_let_star_to_let(ConvertLetStarToLetRequest {
                input: "(let* ((x 1)) ; c\n x)",
                dialect: Dialect::CommonLisp,
                path: path.clone(),
                allow_partial: false,
            })
            .is_err()
        );
        assert!(
            plan_convert_let_to_let_star(ConvertLetToLetStarRequest {
                input: "(let ((x 1)) (declare (special x)) x)",
                dialect: Dialect::CommonLisp,
                path: path.clone()
            })
            .is_err()
        );
        assert!(
            plan_convert_let_star_to_let(ConvertLetStarToLetRequest {
                input: "(let* ((x 1)) x)",
                dialect: Dialect::EmacsLisp,
                path,
                allow_partial: false,
            })
            .is_err()
        );
    }

    #[test]
    fn dialect_support_matrix_is_enforced_before_parsing_and_reparses_output() {
        for (dialect, input) in [
            (Dialect::CommonLisp, "#\\) (let ((x 1) (y 2)) (+ x y))"),
            (Dialect::EmacsLisp, "?\\) (let ((x 1) (y 2)) (+ x y))"),
        ] {
            let plan = plan_convert_let_to_let_star(ConvertLetToLetStarRequest {
                input,
                dialect,
                path: "1".parse().expect("path"),
            })
            .expect("supported dialect");
            SyntaxTree::parse_with_dialect(&plan.rewritten, dialect)
                .expect("dialect-specific let output");
        }

        for dialect in [
            Dialect::Scheme,
            Dialect::Clojure,
            Dialect::Janet,
            Dialect::Fennel,
            Dialect::Unknown,
        ] {
            let error = plan_convert_let_to_let_star(ConvertLetToLetStarRequest {
                input: ")",
                dialect,
                path: "0".parse().expect("path"),
            })
            .expect_err("unsupported dialect");
            assert!(
                error
                    .to_string()
                    .contains("supports only Common Lisp and Emacs Lisp"),
                "{dialect:?}: {error:#}"
            );
        }

        let plan = plan_convert_let_star_to_let(ConvertLetStarToLetRequest {
            input: "#\\) (let* ((x 1) (y 2)) (+ x y))",
            dialect: Dialect::CommonLisp,
            path: "1".parse().expect("path"),
            allow_partial: false,
        })
        .expect("Common Lisp");
        SyntaxTree::parse_with_dialect(&plan.rewritten, Dialect::CommonLisp)
            .expect("Common Lisp let output");

        for dialect in [
            Dialect::EmacsLisp,
            Dialect::Scheme,
            Dialect::Clojure,
            Dialect::Janet,
            Dialect::Fennel,
            Dialect::Unknown,
        ] {
            let error = plan_convert_let_star_to_let(ConvertLetStarToLetRequest {
                input: ")",
                dialect,
                path: "0".parse().expect("path"),
                allow_partial: false,
            })
            .expect_err("unsupported dialect");
            assert!(
                error
                    .to_string()
                    .contains("currently supports only Common Lisp"),
                "{dialect:?}: {error:#}"
            );
        }
    }

    #[test]
    fn allow_partial_splits_at_the_first_real_dependency() {
        let plan = plan_convert_let_star_to_let(ConvertLetStarToLetRequest {
            input: "(let* ((a 1) (b (+ a 1)) (c 2)) (+ a b c))",
            dialect: Dialect::CommonLisp,
            path: "0".parse().expect("path"),
            allow_partial: true,
        })
        .expect("partial plan");
        assert!(plan.partial);
        assert_eq!(
            plan.rewritten,
            "(let ((a 1)) (let* ((b (+ a 1)) (c 2)) (+ a b c)))"
        );
        SyntaxTree::parse_with_dialect(&plan.rewritten, Dialect::CommonLisp)
            .expect("rewritten reparses");
    }

    #[test]
    fn allow_partial_matches_full_conversion_when_every_binding_is_independent() {
        let path: Path = "0".parse().expect("path");
        let input = "(let* ((x 1) (y 2)) (+ x y))";
        let default_plan = plan_convert_let_star_to_let(ConvertLetStarToLetRequest {
            input,
            dialect: Dialect::CommonLisp,
            path: path.clone(),
            allow_partial: false,
        })
        .expect("default plan");
        let partial_plan = plan_convert_let_star_to_let(ConvertLetStarToLetRequest {
            input,
            dialect: Dialect::CommonLisp,
            path,
            allow_partial: true,
        })
        .expect("allow-partial plan");
        assert!(!default_plan.partial);
        assert!(!partial_plan.partial);
        assert_eq!(default_plan.rewritten, partial_plan.rewritten);
        assert_eq!(partial_plan.rewritten, "(let ((x 1) (y 2)) (+ x y))");
    }

    // `independent_prefix_len` can never return 0: at index 0, `names[..0]`
    // is empty, so there are no earlier bindings within this same `let*` for
    // binding 0's initializer to reference. A "dependency" can only be found
    // starting at index 1, which produces a prefix length of (at least) 1.
    // This means `--allow-partial` never turns into a full refusal purely
    // because of a dependency: even a first binding whose initializer looks
    // self-referential (it can't actually see itself, since it isn't
    // "earlier" than itself) never blocks the split. Every other guard
    // (dialect, comments, declare, shape) can still refuse the whole
    // operation - only the dependency-refusal branch is affected.
    #[test]
    fn first_binding_can_never_shrink_the_prefix_to_zero() {
        let plan = plan_convert_let_star_to_let(ConvertLetStarToLetRequest {
            input: r#"(let* ((x x) (y 1)) (+ x y))"#,
            dialect: Dialect::CommonLisp,
            path: "0".parse().expect("path"),
            allow_partial: true,
        })
        .expect("plan");
        assert!(!plan.partial);
        assert_eq!(plan.rewritten, "(let ((x x) (y 1)) (+ x y))");

        let single_binding = plan_convert_let_star_to_let(ConvertLetStarToLetRequest {
            input: "(let* ((x 1)) x)",
            dialect: Dialect::CommonLisp,
            path: "0".parse().expect("path"),
            allow_partial: true,
        })
        .expect("plan");
        assert!(!single_binding.partial);
        assert_eq!(single_binding.rewritten, "(let ((x 1)) x)");
    }

    #[test]
    fn allow_partial_preserves_crlf_line_endings_outside_the_binding_list() {
        let input = "(let* ((a 1)\r\n       (b (+ a 1))\r\n       (c 2))\r\n  (+ a b c))";
        let plan = plan_convert_let_star_to_let(ConvertLetStarToLetRequest {
            input,
            dialect: Dialect::CommonLisp,
            path: "0".parse().expect("path"),
            allow_partial: true,
        })
        .expect("partial plan");
        assert!(plan.partial);
        assert_eq!(
            plan.rewritten,
            "(let ((a 1)) (let* ((b (+ a 1)) (c 2))\r\n  (+ a b c)))"
        );
        SyntaxTree::parse_with_dialect(&plan.rewritten, Dialect::CommonLisp)
            .expect("rewritten reparses");
    }

    #[test]
    fn allow_partial_does_not_mistake_a_string_literal_for_a_dependency() {
        // `collect_unshadowed_symbol_references` is the existing primitive
        // that already gets this right; this confirms it, it does not
        // reimplement any detection logic.
        let path: Path = "0".parse().expect("path");
        let input = r#"(let* ((x 1) (y "x")) (list x y))"#;
        let plan = plan_convert_let_star_to_let(ConvertLetStarToLetRequest {
            input,
            dialect: Dialect::CommonLisp,
            path,
            allow_partial: true,
        })
        .expect("plan");
        assert!(!plan.partial);
        assert_eq!(plan.rewritten, r#"(let ((x 1) (y "x")) (list x y))"#);
    }

    #[test]
    fn allow_partial_does_not_mistake_a_quoted_symbol_for_a_dependency() {
        // `collect_unshadowed_symbol_references` is the existing primitive
        // that already gets this right; this confirms it, it does not
        // reimplement any detection logic.
        let path: Path = "0".parse().expect("path");
        let input = "(let* ((a 1) (b '(uses a)) (c 3)) (list a b c))";
        let plan = plan_convert_let_star_to_let(ConvertLetStarToLetRequest {
            input,
            dialect: Dialect::CommonLisp,
            path,
            allow_partial: true,
        })
        .expect("plan");
        assert!(!plan.partial);
        assert_eq!(
            plan.rewritten,
            "(let ((a 1) (b '(uses a)) (c 3)) (list a b c))"
        );
    }

    #[test]
    fn allow_partial_splits_a_multi_binding_independent_prefix() {
        let plan = plan_convert_let_star_to_let(ConvertLetStarToLetRequest {
            input: "(let* ((a 1) (b 2) (c 3) (d (+ a 1))) (list a b c d))",
            dialect: Dialect::CommonLisp,
            path: "0".parse().expect("path"),
            allow_partial: true,
        })
        .expect("partial plan");
        assert!(plan.partial);
        assert_eq!(
            plan.rewritten,
            "(let ((a 1) (b 2) (c 3)) (let* ((d (+ a 1))) (list a b c d)))"
        );
        SyntaxTree::parse_with_dialect(&plan.rewritten, Dialect::CommonLisp)
            .expect("rewritten reparses");
    }

    #[test]
    fn allow_partial_still_rejects_comments_and_declarations() {
        let path: Path = "0".parse().expect("path");
        assert!(
            plan_convert_let_star_to_let(ConvertLetStarToLetRequest {
                input: "(let* ((x 1)) ; c\n x)",
                dialect: Dialect::CommonLisp,
                path: path.clone(),
                allow_partial: true,
            })
            .is_err()
        );
        assert!(
            plan_convert_let_star_to_let(ConvertLetStarToLetRequest {
                input: "(let* ((x 1) (y (+ x 1))) (declare (special x)) (+ x y))",
                dialect: Dialect::CommonLisp,
                path,
                allow_partial: true,
            })
            .is_err()
        );
    }
}
