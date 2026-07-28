//! `method-lambda-list-mismatch`: a method that cannot be called through its
//! generic.
//!
//! A `defmethod` must agree with its `defgeneric` on the number of required
//! parameters — and on which lambda-list keywords appear at all. When it does
//! not, `defmethod` signals at load time, so the file simply does not load.
//! Catching it here catches it before the load.
//!
//! Correlating the two needs both definitions at once, which is why this is a
//! whole-document rule. It is also why it stops at the file boundary: a method
//! whose generic lives in another file is not reported, because nothing here
//! can see whether that generic agrees. Reporting it would be a guess, and this
//! is a rule whose findings are certainties.
//!
//! Report-only: which of the two lambda lists is right is the whole question.
//!
//! Scope: Common Lisp only.

use std::collections::HashMap;

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView};
use paredit_core_syntax::view_query::{atom_text, list_head, symbol_in, unqualified};

pub const META: RuleMeta = RuleMeta::new(
    "method-lambda-list-mismatch",
    RuleCategory::ObjectSystem,
    Severity::Error,
    "a defmethod whose required-parameter count differs from its defgeneric in the same file",
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "A method's lambda list must be congruent with its generic function's: same number of \
         required parameters, same number of optionals, and `&rest`/`&key` present in both or \
         neither. `defmethod` signals when it is not, so the file fails to load.",
    )
    .with_example(
        "(defgeneric area (shape))\n(defmethod area ((s square) scale) …)",
        "(defgeneric area (shape scale))\n(defmethod area ((s square) scale) …)",
    )
    .with_caveat(
        "A method whose generic is defined in another file is not reported: this rule reads one \
         file, and a finding it cannot be sure of is worse than no finding.",
    ),
);

/// The shape of a lambda list, to the precision congruence cares about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LambdaShape {
    pub required: usize,
    pub optional: usize,
    pub has_rest: bool,
    pub has_key: bool,
}

/// One method that cannot be called through its generic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LambdaListMismatch {
    pub span: ByteSpan,
    pub name: String,
    pub generic: LambdaShape,
    pub method: LambdaShape,
}

/// Reads a lambda list into the shape congruence is judged on.
#[must_use]
pub fn lambda_shape(lambda_list: &ExpressionView) -> LambdaShape {
    let mut shape = LambdaShape::default();
    let mut section = Section::Required;
    for parameter in &lambda_list.children {
        // A specialiser `(s square)` or a default `(x 0)` names its parameter
        // first; either way it is one parameter.
        let name = atom_text(parameter).unwrap_or("");
        match name.to_ascii_lowercase().as_str() {
            "&optional" => section = Section::Optional,
            "&rest" | "&body" => {
                shape.has_rest = true;
                section = Section::Rest;
            }
            "&key" => {
                shape.has_key = true;
                section = Section::Key;
            }
            "&aux" => section = Section::Aux,
            "&allow-other-keys" => {}
            _ => match section {
                Section::Required => shape.required += 1,
                Section::Optional => shape.optional += 1,
                Section::Rest | Section::Key | Section::Aux => {}
            },
        }
    }
    shape
}

enum Section {
    Required,
    Optional,
    Rest,
    Key,
    Aux,
}

/// Whether two lambda lists are congruent in the sense `defmethod` requires.
#[must_use]
pub const fn congruent(generic: LambdaShape, method: LambdaShape) -> bool {
    generic.required == method.required
        && generic.optional == method.optional
        && generic.has_rest == method.has_rest
        // A method may accept keywords the generic does not name, but not the
        // other way round: a generic with `&key` requires its methods to have
        // one too.
        && (!generic.has_key || method.has_key)
}

/// Every incongruent method in one document.
#[must_use]
pub fn collect(root: &ExpressionView) -> Vec<LambdaListMismatch> {
    let mut generics: HashMap<String, LambdaShape> = HashMap::new();
    for form in &root.children {
        let Some(head) = list_head(form) else {
            continue;
        };
        if !symbol_in(head, &["defgeneric"]) {
            continue;
        }
        let (Some(name), Some(lambda_list)) = (
            form.children.get(1).and_then(atom_text),
            form.children.get(2),
        ) else {
            continue;
        };
        generics.insert(
            unqualified(name).to_ascii_lowercase(),
            lambda_shape(lambda_list),
        );
    }
    if generics.is_empty() {
        return Vec::new();
    }

    let mut found = Vec::new();
    for form in &root.children {
        let Some(head) = list_head(form) else {
            continue;
        };
        if !symbol_in(head, &["defmethod"]) {
            continue;
        }
        let Some(name) = form.children.get(1).and_then(atom_text) else {
            continue;
        };
        let Some(&generic) = generics.get(&unqualified(name).to_ascii_lowercase()) else {
            continue;
        };
        // `(defmethod name :before (args) …)` puts a qualifier before the
        // lambda list; the list is the first *list* child after the name.
        let Some(lambda_list) = form
            .children
            .iter()
            .skip(2)
            .find(|child| atom_text(child).is_none())
        else {
            continue;
        };
        let method = lambda_shape(lambda_list);
        if !congruent(generic, method) {
            found.push(LambdaListMismatch {
                span: form.span,
                name: name.to_owned(),
                generic,
                method,
            });
        }
    }
    found
}

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        // Correlating two top-level definitions is exactly what this variant is
        // for: no per-node predicate can see the other definition.
        HeadFilter::WholeTree
    }

    fn check(
        &self,
        _context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult {
        for mismatch in collect(view) {
            sink.report(
                mismatch.span,
                format!(
                    "this method takes {} required parameter(s) but the defgeneric for {} \
                     declares {}; defmethod signals when the two are not congruent",
                    mismatch.method.required, mismatch.name, mismatch.generic.required
                ),
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::SyntaxTree;

    fn names(input: &str) -> Vec<String> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        collect(&tree.root_view())
            .into_iter()
            .map(|item| item.name)
            .collect()
    }

    fn shape(input: &str) -> LambdaShape {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let view = tree
            .select_path(&paredit_core_syntax::sexpr::Path::root_child(0))
            .expect("root form")
            .view();
        lambda_shape(&view)
    }

    #[test]
    fn reads_each_lambda_list_section() {
        assert_eq!(
            shape("(a b &optional c &rest d)"),
            LambdaShape {
                required: 2,
                optional: 1,
                has_rest: true,
                has_key: false,
            }
        );
        assert_eq!(
            shape("((s square) n &key scale)"),
            LambdaShape {
                required: 2,
                optional: 0,
                has_rest: false,
                has_key: true,
            }
        );
    }

    #[test]
    fn flags_a_method_with_too_many_required_parameters() {
        assert_eq!(
            names("(defgeneric area (shape))\n(defmethod area ((s square) scale) 1)"),
            vec!["area"]
        );
    }

    #[test]
    fn flags_a_method_with_too_few() {
        assert_eq!(
            names("(defgeneric area (shape scale))\n(defmethod area ((s square)) 1)"),
            vec!["area"]
        );
    }

    #[test]
    fn accepts_a_congruent_method() {
        assert!(names("(defgeneric area (shape))\n(defmethod area ((s square)) 1)").is_empty());
    }

    #[test]
    fn reads_a_qualified_method() {
        assert!(
            names("(defgeneric area (shape))\n(defmethod area :before ((s square)) 1)").is_empty()
        );
        assert_eq!(
            names("(defgeneric area (shape))\n(defmethod area :before ((s square) n) 1)"),
            vec!["area"]
        );
    }

    #[test]
    fn a_method_whose_generic_is_elsewhere_is_not_reported() {
        assert!(names("(defmethod area ((s square) scale) 1)").is_empty());
    }

    #[test]
    fn a_generic_with_key_requires_its_methods_to_have_one() {
        assert_eq!(
            names("(defgeneric draw (s &key) )\n(defmethod draw ((s square)) 1)"),
            vec!["draw"]
        );
        assert!(
            names("(defgeneric draw (s &key))\n(defmethod draw ((s square) &key color) 1)")
                .is_empty()
        );
    }

    #[test]
    fn a_method_may_add_keywords_the_generic_does_not_name() {
        assert!(
            names("(defgeneric draw (s &key))\n(defmethod draw ((s square) &key color size) 1)")
                .is_empty()
        );
    }

    #[test]
    fn an_optional_mismatch_is_incongruent_too() {
        assert_eq!(
            names("(defgeneric f (a &optional b))\n(defmethod f ((a t)) 1)"),
            vec!["f"]
        );
    }

    #[test]
    fn a_document_with_no_generics_reports_nothing() {
        assert!(names("(defmethod area ((s square)) 1)").is_empty());
    }
}
