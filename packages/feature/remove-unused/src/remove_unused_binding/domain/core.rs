use crate::error::{BindingListError, RemoveSelectionError, RemoveUnusedResult};

use paredit_core_syntax::common_lisp::{
    CommonLispBindingListShape, CommonLispBindingRefactorForm, CommonLispLocalCallableForm,
    CommonLispSlotBindingForm, CommonLispVariableSpecForm,
};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, Delimiter, ExpressionKind, ExpressionView};

#[derive(Debug)]
pub struct LetBindingRemovalCandidate {
    pub index: usize,
    pub name: String,
    pub value_span: ByteSpan,
    pub removal_span: ByteSpan,
}

pub fn binding_removal_candidates(
    dialect: Dialect,
    refactor_form: CommonLispBindingRefactorForm,
    binding_form: &ExpressionView,
) -> RemoveUnusedResult<Vec<LetBindingRemovalCandidate>> {
    if matches!(dialect, Dialect::Clojure | Dialect::Janet | Dialect::Fennel) {
        return vector_let_binding_removal_candidates(binding_form);
    }

    let Some(shape) = refactor_form.binding_list_shape() else {
        return Err(RemoveSelectionError::UnsupportedBindingForm.into());
    };
    match shape {
        CommonLispBindingListShape::NameValuePairs => {
            list_pair_let_binding_removal_candidates(binding_form)
        }
        CommonLispBindingListShape::LocalCallableDefinitions(form) => {
            list_pair_local_callable_binding_removal_candidates(binding_form, form)
        }
        CommonLispBindingListShape::VariableSpecs(form) => {
            list_pair_iteration_binding_removal_candidates(binding_form, form)
        }
        CommonLispBindingListShape::SlotBindings(form) => {
            list_pair_slot_binding_removal_candidates(binding_form, form)
        }
    }
}

fn vector_let_binding_removal_candidates(
    binding_form: &ExpressionView,
) -> RemoveUnusedResult<Vec<LetBindingRemovalCandidate>> {
    if binding_form.kind != ExpressionKind::List
        || binding_form.delimiter != Some(Delimiter::Bracket)
    {
        return Err(BindingListError::ExpectedVectorLet.into());
    }
    if binding_form.children.len() % 2 != 0 {
        return Err(BindingListError::VectorNotPaired.into());
    }

    binding_form
        .children
        .chunks_exact(2)
        .enumerate()
        .map(|(index, pair)| {
            let name = atom_text(&pair[0])
                .ok_or(BindingListError::LetBindingNameNotAnAtom)?
                .to_owned();
            Ok(LetBindingRemovalCandidate {
                index,
                name,
                value_span: pair[1].span,
                removal_span: ByteSpan::new(pair[0].span.start(), pair[1].span.end()),
            })
        })
        .collect()
}

fn list_pair_iteration_binding_removal_candidates(
    binding_form: &ExpressionView,
    form: CommonLispVariableSpecForm,
) -> RemoveUnusedResult<Vec<LetBindingRemovalCandidate>> {
    if binding_form.kind != ExpressionKind::List || binding_form.delimiter != Some(Delimiter::Paren)
    {
        let form_name = form.form_name();
        return Err(BindingListError::ExpectedVariableSpecs {
            form: form_name.to_owned(),
        }
        .into());
    }

    binding_form
        .children
        .iter()
        .enumerate()
        .map(|(index, spec)| {
            let (name, value_span) = iteration_variable_spec_name_and_value_span(spec, form)?;
            Ok(LetBindingRemovalCandidate {
                index,
                name: name.to_owned(),
                value_span,
                removal_span: spec.span,
            })
        })
        .collect()
}

fn iteration_variable_spec_name_and_value_span(
    spec: &ExpressionView,
    form: CommonLispVariableSpecForm,
) -> RemoveUnusedResult<(&str, ByteSpan)> {
    if spec.kind == ExpressionKind::Atom {
        let name = atom_text(spec).ok_or(BindingListError::IterationNameNotAnAtom)?;
        return Ok((name, spec.span));
    }

    if spec.kind != ExpressionKind::List || spec.delimiter != Some(Delimiter::Paren) {
        let form_name = form.form_name();
        return Err(BindingListError::NotASymbolOrVariableSpec {
            form: form_name.to_owned(),
        }
        .into());
    }
    if spec.children.is_empty() || spec.children.len() > form.max_children() {
        let form_name = form.form_name();
        return Err(BindingListError::VariableSpecWrongArity {
            form: form_name.to_owned(),
        }
        .into());
    }

    let name = atom_text(&spec.children[0]).ok_or(BindingListError::IterationNameNotAnAtom)?;
    let value_span = spec
        .children
        .get(1)
        .map_or(spec.children[0].span, |child| child.span);
    Ok((name, value_span))
}

fn list_pair_slot_binding_removal_candidates(
    binding_form: &ExpressionView,
    form: CommonLispSlotBindingForm,
) -> RemoveUnusedResult<Vec<LetBindingRemovalCandidate>> {
    if binding_form.kind != ExpressionKind::List || binding_form.delimiter != Some(Delimiter::Paren)
    {
        match form {
            CommonLispSlotBindingForm::WithSlots => {
                return Err(BindingListError::ExpectedWithSlots.into());
            }
            CommonLispSlotBindingForm::WithAccessors => {
                return Err(BindingListError::ExpectedWithAccessors.into());
            }
        }
    }

    binding_form
        .children
        .iter()
        .enumerate()
        .map(|(index, spec)| match form {
            CommonLispSlotBindingForm::WithSlots => slot_binding_removal_candidate(index, spec),
            CommonLispSlotBindingForm::WithAccessors => {
                accessor_binding_removal_candidate(index, spec)
            }
        })
        .collect()
}

fn slot_binding_removal_candidate(
    index: usize,
    spec: &ExpressionView,
) -> RemoveUnusedResult<LetBindingRemovalCandidate> {
    if spec.kind == ExpressionKind::Atom {
        let name = atom_text(spec)
            .ok_or(BindingListError::WithSlotsBareNameNotAnAtom)?
            .to_owned();
        return Ok(LetBindingRemovalCandidate {
            index,
            name,
            value_span: spec.span,
            removal_span: spec.span,
        });
    }
    if spec.kind != ExpressionKind::List || spec.delimiter != Some(Delimiter::Paren) {
        return Err(BindingListError::WithSlotsNotANameOrPair.into());
    }
    if spec.children.len() != 2 {
        return Err(BindingListError::WithSlotsPairIncomplete.into());
    }
    let name = atom_text(&spec.children[0])
        .ok_or(BindingListError::WithSlotsNameNotAnAtom)?
        .to_owned();
    Ok(LetBindingRemovalCandidate {
        index,
        name,
        value_span: spec.children[1].span,
        removal_span: spec.span,
    })
}

fn accessor_binding_removal_candidate(
    index: usize,
    spec: &ExpressionView,
) -> RemoveUnusedResult<LetBindingRemovalCandidate> {
    if spec.kind != ExpressionKind::List || spec.delimiter != Some(Delimiter::Paren) {
        return Err(BindingListError::WithAccessorsNotAPair.into());
    }
    if spec.children.len() != 2 {
        return Err(BindingListError::WithAccessorsPairIncomplete.into());
    }
    let name = atom_text(&spec.children[0])
        .ok_or(BindingListError::WithAccessorsNameNotAnAtom)?
        .to_owned();
    Ok(LetBindingRemovalCandidate {
        index,
        name,
        value_span: spec.children[1].span,
        removal_span: spec.span,
    })
}

fn list_pair_let_binding_removal_candidates(
    binding_form: &ExpressionView,
) -> RemoveUnusedResult<Vec<LetBindingRemovalCandidate>> {
    if binding_form.kind != ExpressionKind::List || binding_form.delimiter != Some(Delimiter::Paren)
    {
        return Err(BindingListError::ExpectedListPairLet.into());
    }

    binding_form
        .children
        .iter()
        .enumerate()
        .map(|(index, pair)| {
            if pair.kind != ExpressionKind::List || pair.delimiter != Some(Delimiter::Paren) {
                if pair.kind != ExpressionKind::Atom {
                    return Err(BindingListError::LetBindingNotANameOrPair.into());
                }
                let name = atom_text(pair)
                    .ok_or(BindingListError::LetBindingNameNotAnAtom)?
                    .to_owned();
                return Ok(LetBindingRemovalCandidate {
                    index,
                    name,
                    value_span: pair.span,
                    removal_span: pair.span,
                });
            }
            if pair.children.is_empty() || pair.children.len() > 2 {
                return Err(BindingListError::LetBindingPairWrongArity.into());
            }
            let name = atom_text(&pair.children[0])
                .ok_or(BindingListError::LetBindingNameNotAnAtom)?
                .to_owned();
            let value_span = pair
                .children
                .get(1)
                .map_or(pair.children[0].span, |value| value.span);
            Ok(LetBindingRemovalCandidate {
                index,
                name,
                value_span,
                removal_span: pair.span,
            })
        })
        .collect()
}

fn list_pair_local_callable_binding_removal_candidates(
    binding_form: &ExpressionView,
    form: CommonLispLocalCallableForm,
) -> RemoveUnusedResult<Vec<LetBindingRemovalCandidate>> {
    if binding_form.kind != ExpressionKind::List || binding_form.delimiter != Some(Delimiter::Paren)
    {
        return Err(BindingListError::ExpectedListPairCallable {
            form: form.operator_name().to_owned(),
        }
        .into());
    }
    let body_label = if form.is_macro() {
        "macro expander body"
    } else {
        "lambda list, and body"
    };

    binding_form
        .children
        .iter()
        .enumerate()
        .map(|(index, pair)| {
            if pair.kind != ExpressionKind::List || pair.delimiter != Some(Delimiter::Paren) {
                return Err(BindingListError::CallableBindingNotAList {
                    form: form.operator_name().to_owned(),
                }
                .into());
            }
            if pair.children.len() < 2 {
                return Err(BindingListError::CallableBindingIncomplete {
                    form: form.operator_name().to_owned(),
                    body_label: body_label.to_owned(),
                }
                .into());
            }
            let name = atom_text(&pair.children[0])
                .ok_or_else(|| BindingListError::CallableNameNotAnAtom {
                    form: form.operator_name().to_owned(),
                })?
                .to_owned();
            let value_start = pair.children[1].span.start();
            let value_end = pair
                .children
                .last()
                .ok_or_else(|| BindingListError::CallableBindingIncomplete {
                    form: form.operator_name().to_owned(),
                    body_label: body_label.to_owned(),
                })?
                .span
                .end();
            Ok(LetBindingRemovalCandidate {
                index,
                name,
                value_span: ByteSpan::new(value_start, value_end),
                removal_span: pair.span,
            })
        })
        .collect()
}

fn atom_text(view: &ExpressionView) -> Option<&str> {
    (view.kind == ExpressionKind::Atom)
        .then_some(view.text.as_deref())
        .flatten()
}

mod references;

pub use references::binding_reference_spans;

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::sexpr::{SymbolName, SyntaxTree};

    fn target_for(input: &str) -> (SyntaxTree, ExpressionView) {
        let tree = SyntaxTree::parse(input).expect("input should parse");
        let target = tree.root_view().children[0].clone();
        (tree, target)
    }

    #[test]
    fn collects_common_lisp_binding_candidates_and_references() {
        let (_tree, target) = target_for("(let* ((first 1) (second first)) second)");
        let form = Dialect::CommonLisp
            .common_lisp_binding_refactor_form_for_head("let*")
            .expect("let* should be supported");
        let candidates = binding_removal_candidates(Dialect::CommonLisp, form, &target.children[1])
            .expect("bindings should parse");

        assert_eq!(
            candidates
                .iter()
                .map(|candidate| candidate.name.as_str())
                .collect::<Vec<_>>(),
            ["first", "second"]
        );
        let name = SymbolName::new("first".to_owned()).expect("symbol should parse");
        let references = binding_reference_spans(
            Dialect::CommonLisp,
            "(let* ((first 1) (second first)) second)",
            &target,
            form,
            &target.children[1],
            &candidates,
            &candidates[0],
            &name,
        )
        .expect("references should collect");

        assert_eq!(references.len(), 1);
        assert_eq!(references[0].len(), "first".len());
    }

    #[test]
    fn collects_clojure_vector_binding_candidates() {
        let (_, target) = target_for("(let [first 1 second 2] second)");
        let form = Dialect::Clojure
            .common_lisp_binding_refactor_form_for_head("let")
            .expect("let should be supported");
        let candidates = binding_removal_candidates(Dialect::Clojure, form, &target.children[1])
            .expect("vector bindings should parse");

        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].name, "first");
        assert_eq!(candidates[1].name, "second");
    }
}
