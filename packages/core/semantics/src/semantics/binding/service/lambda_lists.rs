//! Lambda-list scopes.
//!
//! A lambda list binds *sequentially*: `(lambda (&optional (b b)) ...)` reads
//! an outer `b` in the default form, because the parameter is not bound until
//! its own spec is complete. That is why this cannot simply declare every name
//! from [`lambda_list_bound_names`] up front and then walk the defaults.
//!
//! The per-spec split of names is a mirror of
//! `lexical_scope::patterns::collect_lambda_list_parameter_spec_names`. Only
//! the mode-to-subform mapping is restated; the extraction itself delegates to
//! the shared [`binding_pattern_bound_names`], and
//! `per_spec_names_match_the_shared_extractor` pins the mirror to the original.

use crate::lexical_scope::{BoundName, binding_pattern_bound_names, lambda_list_bound_names};
use paredit_core_syntax::sexpr::{ExpressionKind, ExpressionView};

use super::super::model::{BindingKind, ScopeId};
use super::builder::{Walk, head_text};

#[derive(Clone, Copy, Eq, PartialEq)]
enum LambdaListMode {
    Required,
    Optional,
    Key,
    Aux,
}

/// Every name a lambda list binds, for the binders that need them all at once
/// and have no default forms to sequence around.
pub(super) fn lambda_list_names(parameter_form: &ExpressionView) -> Vec<BoundName> {
    lambda_list_bound_names(parameter_form)
}

impl Walk<'_> {
    /// Opens the scope of a lambda list and walks its body in it.
    ///
    /// Returns whether the form had a lambda list at all; `false` lets the
    /// caller fall back to an ordinary descent, matching
    /// `collect_lambda_list_references`.
    pub(super) fn lambda_scope(
        &mut self,
        parameter_form: &ExpressionView,
        body_forms: &[ExpressionView],
        scope: ScopeId,
        target: &ExpressionView,
        head: &str,
    ) -> bool {
        if parameter_form.kind != ExpressionKind::List {
            return false;
        }

        let depth = self.stack.depth();
        let inner = self.builder.open_scope(scope, target.span);
        self.declare_lambda_list_from(parameter_form, inner, target, head, 0);
        self.body(body_forms, inner);
        self.stack.rewind(depth);
        true
    }

    /// Declares every parameter of `parameter_form` from `start_index`
    /// onward, into `scope`.
    ///
    /// Used both for a whole ordinary lambda list (`start_index == 0`, from
    /// [`Self::lambda_scope`]) and for the `&optional`/`&rest`/`&key`/`&aux`
    /// tail of a `defmethod` lambda list, whose required section a caller
    /// binds separately because its parameters may carry a specializer an
    /// ordinary lambda list does not expect. `start_index` landing exactly on
    /// a `&optional`/`&rest`/`&key` marker (as it does for that caller) makes
    /// the mode switch on the very first iteration, so the tail is read
    /// exactly as it would be as a standalone lambda list.
    pub(super) fn declare_lambda_list_from(
        &mut self,
        parameter_form: &ExpressionView,
        scope: ScopeId,
        target: &ExpressionView,
        head: &str,
        start_index: usize,
    ) {
        let mut mode = LambdaListMode::Required;
        let mut index = start_index;
        while index < parameter_form.children.len() {
            let child = &parameter_form.children[index];

            if let Some(marker) = head_text(child) {
                match marker {
                    "&optional" => {
                        mode = LambdaListMode::Optional;
                        index += 1;
                        continue;
                    }
                    "&key" => {
                        mode = LambdaListMode::Key;
                        index += 1;
                        continue;
                    }
                    "&aux" => {
                        mode = LambdaListMode::Aux;
                        index += 1;
                        continue;
                    }
                    "&rest" | "&body" | "&whole" | "&environment" => {
                        if let Some(next) = parameter_form.children.get(index + 1) {
                            for bound in binding_pattern_bound_names(next) {
                                self.declare(
                                    target,
                                    scope,
                                    head,
                                    &bound,
                                    BindingKind::Variable,
                                    None,
                                );
                            }
                        }
                        index += 2;
                        continue;
                    }
                    "&allow-other-keys" => {
                        index += 1;
                        continue;
                    }
                    _ if marker.starts_with('&') => {
                        index += 1;
                        continue;
                    }
                    _ => {}
                }
            }

            // Walked before the parameter is declared: a default form sees the
            // parameters to its left and nothing further right.
            let default_form = (mode != LambdaListMode::Required)
                .then(|| child.children.get(1))
                .flatten();
            if let Some(default_form) = default_form {
                self.form(default_form, scope, 0);
            }

            for bound in spec_bound_names(child, mode) {
                self.declare(
                    target,
                    scope,
                    head,
                    &bound,
                    BindingKind::Variable,
                    default_form.map(|form| form.span),
                );
            }

            index += 1;
        }
    }
}

/// The names one parameter spec binds, given the section it appears in.
fn spec_bound_names(spec: &ExpressionView, mode: LambdaListMode) -> Vec<BoundName> {
    if head_text(spec).is_some() || mode == LambdaListMode::Required {
        return binding_pattern_bound_names(spec);
    }
    if spec.kind != ExpressionKind::List || spec.children.is_empty() {
        return Vec::new();
    }

    let mut names = match mode {
        LambdaListMode::Required => binding_pattern_bound_names(spec),
        LambdaListMode::Optional | LambdaListMode::Aux => {
            binding_pattern_bound_names(&spec.children[0])
        }
        LambdaListMode::Key => key_parameter_names(&spec.children[0]),
    };

    // `(x default x-supplied-p)` binds the supplied-p flag too.
    if matches!(mode, LambdaListMode::Optional | LambdaListMode::Key) {
        if let Some(supplied_p) = spec.children.get(2) {
            names.extend(binding_pattern_bound_names(supplied_p));
        }
    }

    names
}

/// `((:keyword name) default)` binds `name`, not the keyword.
fn key_parameter_names(spec_name: &ExpressionView) -> Vec<BoundName> {
    if spec_name.kind == ExpressionKind::List && spec_name.children.len() >= 2 {
        if let Some(designator) = head_text(&spec_name.children[0]) {
            if designator.starts_with(':') {
                return binding_pattern_bound_names(&spec_name.children[1]);
            }
        }
    }
    binding_pattern_bound_names(spec_name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::{Path, SyntaxTree};

    fn lambda_list(input: &str) -> ExpressionView {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        tree.select_path(&"0".parse::<Path>().expect("path"))
            .expect("select")
            .view()
            .children
            .swap_remove(1)
    }

    /// The one thing this module restates from `patterns.rs` is which subform
    /// of a spec holds the name in each section. If the restatement drifts,
    /// the union of the per-spec names stops matching the shared extractor —
    /// which is exactly what this asserts, for every section and shape.
    #[test]
    fn per_spec_names_match_the_shared_extractor() {
        let cases = [
            "(lambda (a b) 1)",
            "(lambda (a &optional b) 1)",
            "(lambda (a &optional (b 1)) 1)",
            "(lambda (a &optional (b 1 b-p)) 1)",
            "(lambda (&key c) 1)",
            "(lambda (&key (c 1)) 1)",
            "(lambda (&key (c 1 c-p)) 1)",
            "(lambda (&key ((:see c) 1 c-p)) 1)",
            "(lambda (&aux (d 1)) 1)",
            "(lambda (&rest args) 1)",
            "(lambda (a &rest args &key b) 1)",
            "(lambda (&whole w a &environment e) 1)",
            "(lambda ((a b) c) 1)",
            "(lambda (a &optional b &rest r &key k &aux x) 1)",
        ];

        for input in cases {
            let parameter_form = lambda_list(input);
            let expected = lambda_list_bound_names(&parameter_form);
            let actual = per_spec_union(&parameter_form);
            assert_eq!(actual, expected, "{input}");
        }
    }

    /// Replays the walk's own mode machine, collecting names only.
    fn per_spec_union(parameter_form: &ExpressionView) -> Vec<BoundName> {
        let mut names = Vec::new();
        let mut mode = LambdaListMode::Required;
        let mut index = 0usize;

        while index < parameter_form.children.len() {
            let child = &parameter_form.children[index];
            if let Some(marker) = head_text(child) {
                match marker {
                    "&optional" => {
                        mode = LambdaListMode::Optional;
                        index += 1;
                        continue;
                    }
                    "&key" => {
                        mode = LambdaListMode::Key;
                        index += 1;
                        continue;
                    }
                    "&aux" => {
                        mode = LambdaListMode::Aux;
                        index += 1;
                        continue;
                    }
                    "&rest" | "&body" | "&whole" | "&environment" => {
                        if let Some(next) = parameter_form.children.get(index + 1) {
                            names.extend(binding_pattern_bound_names(next));
                        }
                        index += 2;
                        continue;
                    }
                    "&allow-other-keys" => {
                        index += 1;
                        continue;
                    }
                    _ if marker.starts_with('&') => {
                        index += 1;
                        continue;
                    }
                    _ => {}
                }
            }
            names.extend(spec_bound_names(child, mode));
            index += 1;
        }

        names
    }
}
