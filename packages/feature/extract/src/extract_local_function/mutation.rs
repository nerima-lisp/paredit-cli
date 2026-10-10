use paredit_core_syntax::common_lisp::{
    CommonLispLetBindingForm, CommonLispOperator, CommonLispValueScopeForm,
    CommonLispVariableBindingForm, common_lisp_operator_head_eq, common_lisp_symbol_reference_eq,
};
use paredit_core_syntax::sexpr::reader::{apply_reader_prefix_context, atom_symbol_text};
use paredit_core_syntax::sexpr::{ExpressionKind, ExpressionView};

use crate::error::{ExtractionResult, ExtractionScopeError};

pub(super) fn reject_free_variable_mutation(view: &ExpressionView) -> ExtractionResult<()> {
    inspect(view, &[], 0)
}

fn inspect(view: &ExpressionView, bound: &[String], depth: usize) -> ExtractionResult<()> {
    let Some(depth) = apply_reader_prefix_context(view, depth) else {
        return Ok(());
    };
    let head = view.children.first().and_then(atom_symbol_text);
    if view.kind == ExpressionKind::List {
        match head {
            Some(head) if depth == 0 && common_lisp_operator_head_eq(head, "quote") => {
                return Ok(());
            }
            Some(head) if common_lisp_operator_head_eq(head, "quasiquote") => {
                return inspect_all(&view.children[1..], bound, depth + 1);
            }
            Some(head)
                if depth > 0
                    && (common_lisp_operator_head_eq(head, "unquote")
                        || common_lisp_operator_head_eq(head, "unquote-splicing")) =>
            {
                return inspect_all(&view.children[1..], bound, depth - 1);
            }
            _ => {}
        }
    }
    if depth != 0 || view.kind != ExpressionKind::List {
        return inspect_all(&view.children, bound, depth);
    }
    let operator = head.and_then(CommonLispOperator::from_head);
    if let Some(head) = head {
        if common_lisp_operator_head_eq(head, "with-input-from-string") {
            if let Some(spec) = view.children.get(1) {
                for option in spec.children.get(2..).unwrap_or_default().chunks_exact(2) {
                    if atom_symbol_text(&option[0])
                        .is_some_and(|name| name.eq_ignore_ascii_case(":index"))
                    {
                        reject_free_place(&option[1], bound)?;
                    }
                }
            }
        }
        let places: Vec<&ExpressionView> = if ["setq", "psetq", "setf", "psetf"]
            .iter()
            .any(|name| common_lisp_operator_head_eq(head, name))
        {
            view.children.iter().skip(1).step_by(2).collect()
        } else if ["incf", "decf", "pop"]
            .iter()
            .any(|name| common_lisp_operator_head_eq(head, name))
        {
            view.children.get(1).into_iter().collect()
        } else if ["push", "pushnew"]
            .iter()
            .any(|name| common_lisp_operator_head_eq(head, name))
        {
            view.children.get(2).into_iter().collect()
        } else if common_lisp_operator_head_eq(head, "rotatef") {
            view.children.iter().skip(1).collect()
        } else if common_lisp_operator_head_eq(head, "shiftf") {
            view.children
                .iter()
                .skip(1)
                .take(view.children.len().saturating_sub(2))
                .collect()
        } else if common_lisp_operator_head_eq(head, "multiple-value-setq") {
            view.children
                .get(1)
                .map(|list| list.children.iter().collect())
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        for place in places {
            reject_free_place(place, bound)?;
        }
    }

    match operator.and_then(CommonLispOperator::value_scope_form) {
        Some(CommonLispValueScopeForm::Let(CommonLispLetBindingForm::SymbolMacro)) => {
            let mut inner = bound.to_vec();
            if let Some(bindings) = view.children.get(1) {
                for binding in &bindings.children {
                    if let Some(name) = binding.children.first().and_then(atom_symbol_text) {
                        inner.retain(|bound| !common_lisp_symbol_reference_eq(bound, name));
                    }
                }
            }
            // A symbol macro may forward assignment to a caller cell, even
            // when its name shadows an internal variable.
            inspect_all(view.children.get(2..).unwrap_or_default(), &inner, 0)
        }
        Some(CommonLispValueScopeForm::Let(form))
            if form != CommonLispLetBindingForm::SymbolMacro =>
        {
            inspect_bindings(
                view,
                bound,
                form == CommonLispLetBindingForm::Sequential,
                false,
            )
        }
        Some(CommonLispValueScopeForm::Variable(form)) => inspect_bindings(
            view,
            bound,
            form == CommonLispVariableBindingForm::Sequential,
            true,
        ),
        Some(CommonLispValueScopeForm::Lambda | CommonLispValueScopeForm::FunctionLiteral) => {
            inspect_callable(view, 1, bound)
        }
        Some(CommonLispValueScopeForm::Definition) => inspect_callable(view, 2, bound),
        Some(CommonLispValueScopeForm::LocalCallable(_)) => {
            if let Some(bindings) = view.children.get(1) {
                for binding in &bindings.children {
                    inspect_callable(binding, 1, bound)?;
                }
            }
            inspect_all(view.children.get(2..).unwrap_or_default(), bound, 0)
        }
        Some(CommonLispValueScopeForm::Value) => {
            if let Some(value) = view.children.get(2) {
                inspect(value, bound, 0)?;
            }
            let mut inner = bound.to_vec();
            if let Some(parameters) = view.children.get(1) {
                inspect_all(&parameters.children, bound, 0)?;
                parameter_names(parameters, &mut inner);
            }
            inspect_all(view.children.get(3..).unwrap_or_default(), &inner, 0)
        }
        Some(CommonLispValueScopeForm::Iteration | CommonLispValueScopeForm::Resource(_)) => {
            let mut inner = bound.to_vec();
            if let Some(spec) = view.children.get(1) {
                if let Some(name) = spec.children.first().and_then(atom_symbol_text) {
                    inner.push(name.to_owned());
                }
                if let Some(initial) = spec.children.get(1) {
                    inspect(initial, bound, 0)?;
                }
                inspect_all(spec.children.get(2..).unwrap_or_default(), &inner, 0)?;
            }
            inspect_all(view.children.get(2..).unwrap_or_default(), &inner, 0)
        }
        // Symbol macros and slot aliases are places, not fresh lexical cells.
        // Unknown binding constructs must not hide a caller-cell assignment.
        _ => inspect_all(&view.children, bound, 0),
    }
}

fn reject_free_place(place: &ExpressionView, bound: &[String]) -> ExtractionResult<()> {
    if let Some(name) = atom_symbol_text(place).filter(|name| {
        !bound
            .iter()
            .any(|binding| common_lisp_symbol_reference_eq(binding, name))
    }) {
        return Err(ExtractionScopeError::MutatesFreeVariable {
            name: name.to_owned(),
        }
        .into());
    }
    if place.kind != ExpressionKind::List {
        return Ok(());
    }
    let Some(head) = place.children.first().and_then(atom_symbol_text) else {
        return Ok(());
    };
    // These generalized places delegate their store to another place. CAR,
    // AREF and other object accessors instead mutate the referenced object.
    if common_lisp_operator_head_eq(head, "values") {
        for nested in place.children.iter().skip(1) {
            reject_free_place(nested, bound)?;
        }
    } else {
        let index = if common_lisp_operator_head_eq(head, "getf") {
            Some(1)
        } else if ["the", "ldb", "mask-field"]
            .iter()
            .any(|name| common_lisp_operator_head_eq(head, name))
        {
            Some(2)
        } else {
            None
        };
        if let Some(nested) = index.and_then(|index| place.children.get(index)) {
            reject_free_place(nested, bound)?;
        }
    }
    Ok(())
}

fn inspect_all(views: &[ExpressionView], bound: &[String], depth: usize) -> ExtractionResult<()> {
    for view in views {
        inspect(view, bound, depth)?;
    }
    Ok(())
}

fn inspect_bindings(
    view: &ExpressionView,
    bound: &[String],
    sequential: bool,
    steps: bool,
) -> ExtractionResult<()> {
    let mut inner = bound.to_vec();
    if let Some(bindings) = view.children.get(1) {
        for binding in &bindings.children {
            if let Some(initial) = binding.children.get(1) {
                inspect(initial, if sequential { &inner } else { bound }, 0)?;
            }
            if let Some(name) = atom_symbol_text(binding)
                .or_else(|| binding.children.first().and_then(atom_symbol_text))
            {
                inner.push(name.to_owned());
            }
        }
        if steps {
            for binding in &bindings.children {
                inspect_all(binding.children.get(2..).unwrap_or_default(), &inner, 0)?;
            }
        }
    }
    inspect_all(view.children.get(2..).unwrap_or_default(), &inner, 0)
}

fn inspect_callable(
    view: &ExpressionView,
    parameter_index: usize,
    bound: &[String],
) -> ExtractionResult<()> {
    let mut inner = bound.to_vec();
    if let Some(parameters) = view.children.get(parameter_index) {
        // Defaults can execute before all parameters have been bound.
        inspect_all(&parameters.children, bound, 0)?;
        parameter_names(parameters, &mut inner);
    }
    inspect_all(
        view.children.get(parameter_index + 1..).unwrap_or_default(),
        &inner,
        0,
    )
}

fn parameter_names(parameters: &ExpressionView, names: &mut Vec<String>) {
    let mut mode = "required";
    for spec in &parameters.children {
        if let Some(name) = atom_symbol_text(spec) {
            if name.starts_with('&') {
                mode = name;
            } else {
                names.push(name.to_owned());
            }
            continue;
        }
        if mode == "required"
            || mode.eq_ignore_ascii_case("&rest")
            || mode.eq_ignore_ascii_case("&body")
        {
            parameter_names(spec, names);
        } else if let Some(first) = spec.children.first() {
            if let Some(name) = atom_symbol_text(first) {
                names.push(name.to_owned());
            } else if let Some(name) = first
                .children
                .get(1)
                .and_then(atom_symbol_text)
                .filter(|_| mode.eq_ignore_ascii_case("&key"))
            {
                names.push(name.to_owned());
            }
            if let Some(name) = spec
                .children
                .get(2)
                .and_then(atom_symbol_text)
                .filter(|_| !mode.eq_ignore_ascii_case("&aux"))
            {
                names.push(name.to_owned());
            }
        }
    }
}
