//! Use case for extracting an expression into an enclosing local function.

use anyhow::{Context, Result, bail};

use crate::domain::common_lisp::{
    CommonLispLocalCallableForm, CommonLispOperator, common_lisp_local_callable_form,
    common_lisp_operator_head_eq, common_lisp_symbol_identity_eq, common_lisp_symbol_reference_eq,
    local_callable_names,
};
use crate::domain::dialect::Dialect;
use crate::domain::extract_function::{infer_extract_function_params, rewrite::extracted_call};
use crate::domain::extract_shared::replace_span;
use crate::domain::mutation_safety::reject_common_lisp_reader_conditionals;
use crate::domain::sexpr::reader::{apply_reader_prefix_context, atom_symbol_text};
use crate::domain::sexpr::{
    ByteSpan, ExpressionKind, ExpressionView, Path, ReaderPrefix, Selection, SymbolName, SyntaxTree,
};

#[derive(Debug, Clone)]
pub struct ExtractLocalFunctionRequest<'a> {
    pub input: &'a str,
    pub selection: Selection<'a>,
    pub path: Option<Path>,
    pub enclosing: Selection<'a>,
    pub enclosing_path: Path,
    pub dialect: Dialect,
    pub name: SymbolName,
    pub explicit_params: Vec<String>,
    pub infer_params: bool,
    pub recursive: bool,
}

#[derive(Debug, Clone)]
pub struct ExtractLocalFunctionPlan {
    pub path: Option<Path>,
    pub enclosing_path: Path,
    pub selected_span: ByteSpan,
    pub enclosing_span: ByteSpan,
    pub name: SymbolName,
    pub params: Vec<String>,
    pub inferred_params: Vec<String>,
    pub recursive: bool,
    pub call: String,
    pub replacement: String,
    pub rewritten: String,
    pub changed: bool,
}

fn ensure_common_lisp_dialect(dialect: Dialect) -> Result<()> {
    if dialect != Dialect::CommonLisp {
        bail!("extract-local-function currently supports only Common Lisp");
    }
    Ok(())
}

pub fn plan_extract_local_function(
    request: ExtractLocalFunctionRequest<'_>,
) -> Result<ExtractLocalFunctionPlan> {
    request.selection.validate_source(request.input)?;
    request.enclosing.validate_source(request.input)?;
    ensure_common_lisp_dialect(request.dialect)?;
    let tree = SyntaxTree::parse_with_dialect(request.input, request.dialect)
        .context("extract-local-function input is not a valid S-expression document")?;
    reject_common_lisp_reader_conditionals(&tree, request.dialect)?;
    let path = request
        .path
        .as_ref()
        .context("extract-local-function requires a path selection")?;
    if tree.select_path(path)?.span() != request.selection.span()
        || tree.select_path(&request.enclosing_path)?.span() != request.enclosing.span()
    {
        bail!("extract-local-function paths and selections must refer to the input tree");
    }
    reject_structural_position(&tree, path)?;

    let selected_span = request.selection.span();
    let enclosing_span = request.enclosing.span();
    let enclosing_view = request.enclosing.view();
    if enclosing_view.kind != ExpressionKind::List {
        bail!("extract-local-function enclosing selection must be a list");
    }
    if selected_span == enclosing_span
        || selected_span.start() < enclosing_span.start()
        || selected_span.end() > enclosing_span.end()
    {
        bail!("extract-local-function target must be a proper descendant of the enclosing list");
    }
    reject_existing_call_capture(
        &enclosing_view,
        request.name.as_str(),
        request.selection.span(),
    )?;
    reject_non_local_control_transfer(&request.selection.view())?;

    let mut params = request.explicit_params;
    let inferred_params = if request.infer_params {
        infer_extract_function_params(request.dialect, &request.selection.view(), &params)
    } else {
        Vec::new()
    };
    for param in &inferred_params {
        if !params
            .iter()
            .any(|existing| common_lisp_symbol_reference_eq(existing, param))
        {
            params.push(param.clone());
        }
    }

    let call = extracted_call(&request.name, &params);
    let selected = selected_span.slice(request.input);
    let enclosed = replace_within(request.input, enclosing_span, selected_span, &call);
    let operator = if request.recursive { "labels" } else { "flet" };
    let replacement = format!(
        "({operator} (({} ({}) {})) {})",
        request.name.as_str(),
        params.join(" "),
        selected,
        enclosed
    );
    let rewritten = replace_span(request.input, enclosing_span, &replacement);
    SyntaxTree::parse_with_dialect(&rewritten, request.dialect)
        .context("extracted local function output is not a valid S-expression document")?;

    Ok(ExtractLocalFunctionPlan {
        path: request.path,
        enclosing_path: request.enclosing_path,
        selected_span,
        enclosing_span,
        name: request.name,
        params,
        inferred_params,
        recursive: request.recursive,
        call,
        replacement,
        changed: rewritten != request.input,
        rewritten,
    })
}

fn replace_within(input: &str, container: ByteSpan, target: ByteSpan, replacement: &str) -> String {
    let start = target.start().get() - container.start().get();
    let end = target.end().get() - container.start().get();
    let mut output = container.slice(input).to_owned();
    output.replace_range(start..end, replacement);
    output
}

fn common_lisp_definition_body_start(
    operator: CommonLispOperator,
    view: &ExpressionView,
) -> Option<usize> {
    match operator {
        CommonLispOperator::Defun | CommonLispOperator::Defmacro => Some(3),
        CommonLispOperator::Defmethod | CommonLispOperator::ClDefmethod => view
            .children
            .iter()
            .enumerate()
            .skip(2)
            .find(|(_, child)| child.kind == ExpressionKind::List)
            .map(|(index, _)| index + 1),
        _ => None,
    }
}

fn common_lisp_lambda_list_init_at_path(
    lambda_list: &ExpressionView,
    indexes: &[usize],
    spec_depth: usize,
) -> bool {
    let (Some(&spec_index), Some(&slot_index)) =
        (indexes.get(spec_depth), indexes.get(spec_depth + 1))
    else {
        return false;
    };
    if slot_index != 1
        || lambda_list
            .children
            .get(spec_index)
            .is_none_or(|spec| spec.kind != ExpressionKind::List)
    {
        return false;
    }

    let mut has_runtime_init = false;
    for parameter in lambda_list.children.iter().take(spec_index) {
        let Some(keyword) = atom_symbol_text(parameter) else {
            continue;
        };
        if common_lisp_operator_head_eq(keyword, "&optional")
            || common_lisp_operator_head_eq(keyword, "&key")
            || common_lisp_operator_head_eq(keyword, "&aux")
        {
            has_runtime_init = true;
        } else if keyword.starts_with('&') {
            has_runtime_init = false;
        }
    }
    has_runtime_init
}

fn common_lisp_restart_bind_option_bit(keyword: &str) -> Option<u8> {
    if common_lisp_operator_head_eq(keyword, ":interactive-function") {
        Some(1)
    } else if common_lisp_operator_head_eq(keyword, ":report-function") {
        Some(2)
    } else if common_lisp_operator_head_eq(keyword, ":test-function") {
        Some(4)
    } else {
        None
    }
}

fn common_lisp_restart_bind_entry_is_valid(entry: &ExpressionView) -> bool {
    if entry.children.len() < 2 {
        return false;
    }

    let mut seen_options = 0;
    let mut index = 2;
    while index < entry.children.len() {
        let Some(option) = entry.children.get(index).and_then(atom_symbol_text) else {
            return false;
        };
        let Some(option_bit) = common_lisp_restart_bind_option_bit(option) else {
            return false;
        };
        if seen_options & option_bit != 0 || entry.children.get(index + 1).is_none() {
            return false;
        }

        seen_options |= option_bit;
        index += 2;
    }

    true
}

fn common_lisp_binding_entry_runtime_slot(
    operator: CommonLispOperator,
    entry: &ExpressionView,
    slot: usize,
) -> bool {
    match operator {
        CommonLispOperator::Let
        | CommonLispOperator::LetStar
        | CommonLispOperator::Prog
        | CommonLispOperator::ProgStar => slot == 1,
        CommonLispOperator::Do | CommonLispOperator::DoStar => slot == 1 || slot == 2,
        CommonLispOperator::HandlerBind => slot == 1,
        CommonLispOperator::RestartBind => {
            common_lisp_restart_bind_entry_is_valid(entry)
                && (slot == 1 || (slot >= 3 && slot % 2 == 1))
        }
        _ => false,
    }
}

fn descends_into_runtime_binding_entry(
    operator: CommonLispOperator,
    parent: &ExpressionView,
    indexes: &[usize],
    depth: usize,
) -> bool {
    let Some((entry_index, slot)) = indexes.get(depth + 1).zip(indexes.get(depth + 2)) else {
        return false;
    };
    parent
        .children
        .get(1)
        .and_then(|bindings| bindings.children.get(*entry_index))
        .is_some_and(|entry| common_lisp_binding_entry_runtime_slot(operator, entry, *slot))
}

fn descends_into_runtime_local_definition(
    parent: &ExpressionView,
    indexes: &[usize],
    depth: usize,
) -> bool {
    let (Some(&definition_index), Some(&definition_slot)) =
        (indexes.get(depth + 1), indexes.get(depth + 2))
    else {
        return false;
    };
    if definition_slot >= 2 {
        return true;
    }
    if definition_slot != 1 {
        return false;
    }
    parent
        .children
        .get(1)
        .and_then(|definitions| definitions.children.get(definition_index))
        .and_then(|definition| definition.children.get(1))
        .is_some_and(|lambda_list| {
            common_lisp_lambda_list_init_at_path(lambda_list, indexes, depth + 3)
        })
}

fn common_lisp_restart_case_option_bit(keyword: &str) -> Option<u8> {
    if common_lisp_operator_head_eq(keyword, ":interactive") {
        Some(1)
    } else if common_lisp_operator_head_eq(keyword, ":report") {
        Some(2)
    } else if common_lisp_operator_head_eq(keyword, ":test") {
        Some(4)
    } else {
        None
    }
}

fn common_lisp_restart_clause_body_start(clause: &ExpressionView) -> Option<usize> {
    let mut seen_options = 0;
    let mut index = 2;
    while let Some(child) = clause.children.get(index) {
        let Some(keyword) = atom_symbol_text(child) else {
            break;
        };
        if !keyword.starts_with(':') {
            break;
        }

        let option_bit = common_lisp_restart_case_option_bit(keyword)?;
        if seen_options & option_bit != 0 || clause.children.get(index + 1).is_none() {
            return None;
        }

        seen_options |= option_bit;
        index += 2;
    }

    while let Some(child) = clause.children.get(index) {
        let declaration = child
            .children
            .first()
            .and_then(atom_symbol_text)
            .is_some_and(|head| common_lisp_operator_head_eq(head, "declare"));
        if declaration {
            index += 1;
            continue;
        }
        break;
    }
    Some(index)
}

fn descends_into_runtime_clause_body(
    operator: Option<CommonLispOperator>,
    head: &str,
    parent: &ExpressionView,
    indexes: &[usize],
    depth: usize,
    child_index: usize,
) -> bool {
    let Some(&clause_slot) = indexes.get(depth + 1) else {
        return false;
    };
    match operator {
        Some(CommonLispOperator::HandlerCase) => clause_slot >= 2,
        Some(CommonLispOperator::RestartCase) => {
            parent.children.get(child_index).is_some_and(|clause| {
                common_lisp_restart_clause_body_start(clause).is_some_and(|body_start| {
                    clause_slot >= body_start
                        || (clause_slot >= 3
                            && clause
                                .children
                                .get(clause_slot - 1)
                                .and_then(atom_symbol_text)
                                .and_then(common_lisp_restart_case_option_bit)
                                .is_some())
                })
            })
        }
        _ if common_lisp_operator_head_eq(head, "typecase")
            || common_lisp_operator_head_eq(head, "etypecase")
            || common_lisp_operator_head_eq(head, "ctypecase")
            || common_lisp_operator_head_eq(head, "case")
            || common_lisp_operator_head_eq(head, "ccase")
            || common_lisp_operator_head_eq(head, "ecase") =>
        {
            clause_slot >= 1
        }
        _ => false,
    }
}

fn selected_form_is_structural(view: &ExpressionView) -> bool {
    let Some(head) = view.children.first().and_then(atom_symbol_text) else {
        return false;
    };
    common_lisp_operator_head_eq(head, "declare") || common_lisp_operator_head_eq(head, "declaim")
}

fn reject_structural_position(tree: &SyntaxTree, path: &Path) -> Result<()> {
    let indexes = path.to_raw_indexes();
    if selected_form_is_structural(&tree.select_path(path)?.view()) {
        bail!("extract-local-function target cannot be inside a structural binding position");
    }
    for depth in 1..indexes.len() {
        let child_index = indexes[depth];
        if child_index == 0 && !is_executable_structural_container(tree, &indexes, depth)? {
            bail!("extract-local-function target cannot be in a list head position");
        }

        let parent_path = Path::from_indexes(indexes[..depth].to_vec());
        let parent = tree.select_path(&parent_path)?.view();
        let Some(head) = parent
            .children
            .first()
            .and_then(|child| child.text.as_deref())
        else {
            continue;
        };

        let target_descends_from_parent = indexes.len() > depth + 1;
        let operator = CommonLispOperator::from_head(head);
        let structural_child = operator.is_some_and(|operator| {
            let descends_into_runtime_local_body = child_index == 1
                && matches!(
                    operator.local_callable_form(),
                    Some(CommonLispLocalCallableForm::Flet | CommonLispLocalCallableForm::Labels)
                )
                && descends_into_runtime_local_definition(&parent, &indexes, depth);
            let binding_entry_runtime_form = child_index == 1
                && descends_into_runtime_binding_entry(operator, &parent, &indexes, depth);
            let lambda_list_runtime_init = child_index == 1
                && operator == CommonLispOperator::Lambda
                && parent.children.get(1).is_some_and(|lambda_list| {
                    common_lisp_lambda_list_init_at_path(lambda_list, &indexes, depth + 1)
                });
            let definition_body_start = common_lisp_definition_body_start(operator, &parent);
            let definition_signature_child = matches!(
                operator,
                CommonLispOperator::Defun
                    | CommonLispOperator::Defmacro
                    | CommonLispOperator::Defmethod
                    | CommonLispOperator::ClDefmethod
            ) && child_index >= 1
                && definition_body_start.is_none_or(|body_start| child_index < body_start)
                && !(definition_body_start.is_some_and(|body_start| child_index + 1 == body_start)
                    && parent.children.get(child_index).is_some_and(|lambda_list| {
                        common_lisp_lambda_list_init_at_path(lambda_list, &indexes, depth + 1)
                    }));
            (child_index == 1
                && (operator == CommonLispOperator::Lambda
                    || (operator.is_let_binding() && !binding_entry_runtime_form)
                    || operator.is_value_binding()
                    || (operator.is_do_binding() && !binding_entry_runtime_form)
                    || (operator.is_prog_binding() && !binding_entry_runtime_form)
                    || (operator.is_handler_bind_binding() && !binding_entry_runtime_form)
                    || (operator.local_callable_form().is_some()
                        && !descends_into_runtime_local_body)))
                && !lambda_list_runtime_init
                || definition_signature_child
                || (operator.is_clause_binding()
                    && child_index >= 2
                    && !descends_into_runtime_clause_body(
                        Some(operator),
                        head,
                        &parent,
                        &indexes,
                        depth,
                        child_index,
                    ))
                || (child_index == 2 && operator.is_do_binding() && !target_descends_from_parent)
        });
        let structural_type_or_clause_child = (common_lisp_operator_head_eq(head, "the")
            || common_lisp_operator_head_eq(head, "function"))
            && child_index == 1
            || ((common_lisp_operator_head_eq(head, "declare")
                || common_lisp_operator_head_eq(head, "declaim"))
                && child_index >= 1)
            || ((common_lisp_operator_head_eq(head, "typecase")
                || common_lisp_operator_head_eq(head, "etypecase")
                || common_lisp_operator_head_eq(head, "ctypecase")
                || common_lisp_operator_head_eq(head, "case")
                || common_lisp_operator_head_eq(head, "ccase")
                || common_lisp_operator_head_eq(head, "ecase"))
                && child_index >= 2
                && !descends_into_runtime_clause_body(
                    operator,
                    head,
                    &parent,
                    &indexes,
                    depth,
                    child_index,
                ))
            || (common_lisp_operator_head_eq(head, "eval-when") && child_index == 1)
            || (common_lisp_operator_head_eq(head, "load-time-value") && child_index == 2);
        let structural_assignment_place = if common_lisp_operator_head_eq(head, "setq")
            || common_lisp_operator_head_eq(head, "psetq")
            || common_lisp_operator_head_eq(head, "setf")
            || common_lisp_operator_head_eq(head, "psetf")
        {
            child_index % 2 == 1
        } else if common_lisp_operator_head_eq(head, "multiple-value-setq") {
            child_index == 1
        } else if common_lisp_operator_head_eq(head, "rotatef") {
            child_index >= 1
        } else if common_lisp_operator_head_eq(head, "shiftf") {
            child_index >= 1 && child_index + 1 < parent.children.len()
        } else {
            false
        };
        let structural_control_child = child_index == 1
            && (common_lisp_operator_head_eq(head, "block")
                || common_lisp_operator_head_eq(head, "return-from")
                || common_lisp_operator_head_eq(head, "go"));
        let tagbody_label = common_lisp_operator_head_eq(head, "tagbody")
            && parent.children.get(child_index).is_some_and(|child| {
                child.kind == ExpressionKind::Atom
                    && child.reader_prefixes.is_empty()
                    && atom_symbol_text(child).is_some()
            });
        if structural_child
            || structural_type_or_clause_child
            || structural_assignment_place
            || structural_control_child
            || tagbody_label
        {
            bail!("extract-local-function target cannot be inside a structural binding position");
        }
    }
    Ok(())
}

fn is_executable_structural_container(
    tree: &SyntaxTree,
    indexes: &[usize],
    depth: usize,
) -> Result<bool> {
    if depth < 2 {
        return Ok(false);
    }
    let ancestor_path = Path::from_indexes(indexes[..depth - 1].to_vec());
    let ancestor = tree.select_path(&ancestor_path)?.view();
    let Some(operator) = ancestor
        .children
        .first()
        .and_then(atom_symbol_text)
        .and_then(CommonLispOperator::from_head)
    else {
        return Ok(false);
    };
    let container_index = indexes[depth - 1];
    if operator.is_do_binding() && container_index == 2 {
        return Ok(true);
    }
    if container_index != 1 {
        return Ok(false);
    }
    if let Some((entry_index, slot)) = indexes.get(depth).zip(indexes.get(depth + 1)) {
        if ancestor
            .children
            .get(1)
            .and_then(|bindings| bindings.children.get(*entry_index))
            .is_some_and(|entry| common_lisp_binding_entry_runtime_slot(operator, entry, *slot))
        {
            return Ok(true);
        }
    }
    Ok(matches!(
        operator.local_callable_form(),
        Some(CommonLispLocalCallableForm::Flet | CommonLispLocalCallableForm::Labels)
    ) && descends_into_runtime_local_definition(&ancestor, indexes, depth - 1))
}

fn reject_existing_call_capture(
    view: &ExpressionView,
    name: &str,
    selected: ByteSpan,
) -> Result<()> {
    // Calls inside the extracted expression move into the definition body. In
    // `labels` they intentionally become recursive; in `flet` they retain the
    // surrounding function binding. Only calls left in the wrapped body can be
    // captured by the newly introduced binding.
    let mut stack = vec![(view, false, false, 0)];
    while let Some((view, call_shadowed, function_shadowed, quasiquote_depth)) = stack.pop() {
        if view.span == selected {
            continue;
        }
        let Some(quasiquote_depth) = apply_reader_prefix_context(view, quasiquote_depth) else {
            continue;
        };
        let head = view.children.first().and_then(atom_symbol_text);
        if view.kind == ExpressionKind::List {
            match head {
                Some(head) if common_lisp_operator_head_eq(head, "quote") => continue,
                Some(head) if common_lisp_operator_head_eq(head, "quasiquote") => {
                    stack.extend(view.children.iter().skip(1).rev().map(|child| {
                        (
                            child,
                            call_shadowed,
                            function_shadowed,
                            quasiquote_depth + 1,
                        )
                    }));
                    continue;
                }
                Some(head)
                    if quasiquote_depth > 0
                        && (common_lisp_operator_head_eq(head, "unquote")
                            || common_lisp_operator_head_eq(head, "unquote-splicing")) =>
                {
                    stack.extend(view.children.iter().skip(1).rev().map(|child| {
                        (
                            child,
                            call_shadowed,
                            function_shadowed,
                            quasiquote_depth - 1,
                        )
                    }));
                    continue;
                }
                _ => {}
            }
        }
        if quasiquote_depth > 0 {
            stack.extend(
                view.children
                    .iter()
                    .rev()
                    .map(|child| (child, call_shadowed, function_shadowed, quasiquote_depth)),
            );
            continue;
        }

        if let Some(form) =
            head.and_then(|head| common_lisp_local_callable_form(Dialect::CommonLisp, head))
        {
            let binds_target = local_callable_names(view)
                .iter()
                .any(|bound| common_lisp_symbol_reference_eq(bound, name));
            let binds_function = binds_target
                && matches!(
                    form,
                    CommonLispLocalCallableForm::Flet | CommonLispLocalCallableForm::Labels
                );
            let body_call_shadowed = call_shadowed || binds_target;
            let body_function_shadowed = function_shadowed || binds_function;
            let definitions_bind_target = form == CommonLispLocalCallableForm::Labels;
            let binding_call_shadowed = call_shadowed || (definitions_bind_target && binds_target);
            let binding_function_shadowed =
                function_shadowed || (definitions_bind_target && binds_function);
            if let Some(bindings) = view.children.get(1) {
                for binding in bindings.children.iter().rev() {
                    stack.extend(binding.children.iter().skip(2).rev().map(|child| {
                        (
                            child,
                            binding_call_shadowed,
                            binding_function_shadowed,
                            quasiquote_depth,
                        )
                    }));
                }
            }
            stack.extend(view.children.iter().skip(2).rev().map(|child| {
                (
                    child,
                    body_call_shadowed,
                    body_function_shadowed,
                    quasiquote_depth,
                )
            }));
            continue;
        }

        if view.kind == ExpressionKind::List
            && head.is_some_and(|head| common_lisp_operator_head_eq(head, "lambda"))
        {
            stack.extend(
                view.children
                    .iter()
                    .skip(2)
                    .rev()
                    .map(|child| (child, call_shadowed, function_shadowed, quasiquote_depth)),
            );
            continue;
        }

        let direct_call = view.kind == ExpressionKind::List
            && head.is_some_and(|head| common_lisp_symbol_reference_eq(head, name));
        let reader_function_designator = view.kind == ExpressionKind::Atom
            && view.reader_prefixes.contains(&ReaderPrefix::Function)
            && atom_symbol_text(view)
                .is_some_and(|symbol| common_lisp_symbol_reference_eq(symbol, name));
        let function_form_designator = view.kind == ExpressionKind::List
            && view.children.len() == 2
            && head.is_some_and(|head| common_lisp_operator_head_eq(head, "function"))
            && view.children.get(1).is_some_and(|designator| {
                designator.kind == ExpressionKind::Atom
                    && designator.reader_prefixes.is_empty()
                    && designator
                        .text
                        .as_deref()
                        .is_some_and(|symbol| common_lisp_symbol_reference_eq(symbol, name))
            });
        if (!call_shadowed && direct_call)
            || (!function_shadowed && (reader_function_designator || function_form_designator))
        {
            bail!(
                "local function name '{name}' would capture an existing call or function designator in the enclosing list"
            );
        }
        stack.extend(
            view.children
                .iter()
                .rev()
                .map(|child| (child, call_shadowed, function_shadowed, quasiquote_depth)),
        );
    }
    Ok(())
}

fn common_lisp_function_block_name(view: &ExpressionView) -> Option<&str> {
    if let Some(name) = atom_symbol_text(view) {
        return Some(name);
    }
    if view.kind != ExpressionKind::List {
        return None;
    }

    let head = view.children.first().and_then(atom_symbol_text)?;
    if !common_lisp_operator_head_eq(head, "setf") {
        return None;
    }
    view.children.get(1).and_then(atom_symbol_text)
}

fn is_common_lisp_standard_nil(symbol: &str) -> bool {
    [
        "nil",
        "cl:nil",
        "cl::nil",
        "common-lisp:nil",
        "common-lisp::nil",
    ]
    .iter()
    .any(|alias| common_lisp_symbol_identity_eq(symbol, alias))
}

fn common_lisp_block_name_eq(candidate: &str, expected: &str) -> bool {
    common_lisp_symbol_identity_eq(candidate, expected)
        || (is_common_lisp_standard_nil(candidate) && is_common_lisp_standard_nil(expected))
}

fn common_lisp_loop_block_name(view: &ExpressionView) -> Option<&str> {
    match view.children.get(1).and_then(atom_symbol_text) {
        Some(clause) if common_lisp_operator_head_eq(clause, "named") => {
            view.children.get(2).and_then(atom_symbol_text)
        }
        _ => Some("nil"),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CommonLispIntegerTag {
    negative: bool,
    limbs: Vec<u32>,
}

// Big-integer conversion is quadratic in the token length, so keep it well below the
// 64 MiB CLI input limit while still supporting practical arbitrary-precision tags.
const MAX_COMMON_LISP_INTEGER_TAG_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone)]
enum CommonLispControlTag {
    Symbol(String),
    Integer(CommonLispIntegerTag),
}

fn parse_common_lisp_integer_digits(
    digits: &str,
    radix: u32,
    negative: bool,
) -> Option<CommonLispIntegerTag> {
    if digits.is_empty() {
        return None;
    }

    let mut limbs = vec![0_u32];
    for character in digits.chars() {
        let digit = character.to_digit(radix)? as u64;
        let mut carry = digit;
        for limb in &mut limbs {
            let value = (*limb as u64) * (radix as u64) + carry;
            *limb = value as u32;
            carry = value >> u32::BITS;
        }
        if carry != 0 {
            limbs.push(carry as u32);
        }
    }
    while limbs.len() > 1 && limbs.last() == Some(&0) {
        limbs.pop();
    }
    let is_zero = limbs.iter().all(|limb| *limb == 0);
    Some(CommonLispIntegerTag {
        negative: negative && !is_zero,
        limbs,
    })
}

fn strip_common_lisp_integer_sign(text: &str) -> (bool, &str) {
    match text.as_bytes().first() {
        Some(b'+') => (false, &text[1..]),
        Some(b'-') => (true, &text[1..]),
        _ => (false, text),
    }
}

fn parse_common_lisp_integer_tag(text: &str) -> Option<CommonLispIntegerTag> {
    if text.len() > MAX_COMMON_LISP_INTEGER_TAG_BYTES {
        return None;
    }

    let lower = text.to_ascii_lowercase();
    let (radix, digits_with_sign, allows_trailing_decimal_point) =
        if let Some(rest) = lower.strip_prefix("#b") {
            (2, rest, false)
        } else if let Some(rest) = lower.strip_prefix("#o") {
            (8, rest, false)
        } else if let Some(rest) = lower.strip_prefix("#d") {
            (10, rest, false)
        } else if let Some(rest) = lower.strip_prefix("#x") {
            (16, rest, false)
        } else if let Some(rest) = lower.strip_prefix('#') {
            let radix_end = rest.find('r')?;
            let radix = rest[..radix_end].parse::<u32>().ok()?;
            if !(2..=36).contains(&radix) {
                return None;
            }
            (radix, &rest[radix_end + 1..], false)
        } else {
            (10, lower.as_str(), true)
        };
    let (negative, digits) = strip_common_lisp_integer_sign(digits_with_sign);
    let digits = if allows_trailing_decimal_point {
        digits.strip_suffix('.').unwrap_or(digits)
    } else {
        digits
    };
    parse_common_lisp_integer_digits(digits, radix, negative)
}

fn common_lisp_control_tag(view: &ExpressionView) -> Option<CommonLispControlTag> {
    if view.kind != ExpressionKind::Atom || !view.reader_prefixes.is_empty() {
        return None;
    }
    let text = atom_symbol_text(view)?;
    if text.contains(['\\', '|']) {
        return Some(CommonLispControlTag::Symbol(text.to_owned()));
    }

    let first_after_sign = text.strip_prefix(['+', '-']).unwrap_or(text).chars().next();
    let numeric_syntax = first_after_sign.is_some_and(|character| character.is_ascii_digit())
        || text
            .strip_prefix('#')
            .and_then(|rest| rest.chars().next())
            .is_some_and(|character| {
                character.is_ascii_digit()
                    || matches!(character.to_ascii_lowercase(), 'b' | 'o' | 'd' | 'x')
            });
    if numeric_syntax {
        return parse_common_lisp_integer_tag(text).map(CommonLispControlTag::Integer);
    }
    Some(CommonLispControlTag::Symbol(text.to_owned()))
}

fn common_lisp_control_tag_eq(
    candidate: &CommonLispControlTag,
    expected: &CommonLispControlTag,
) -> bool {
    match (candidate, expected) {
        (CommonLispControlTag::Symbol(candidate), CommonLispControlTag::Symbol(expected)) => {
            common_lisp_symbol_identity_eq(candidate, expected)
        }
        (CommonLispControlTag::Integer(candidate), CommonLispControlTag::Integer(expected)) => {
            candidate == expected
        }
        _ => false,
    }
}

fn reject_non_local_control_transfer(view: &ExpressionView) -> Result<()> {
    let mut stack = vec![(
        view,
        0,
        Vec::<String>::new(),
        Vec::<CommonLispControlTag>::new(),
    )];
    while let Some((view, quasiquote_depth, block_names, tag_names)) = stack.pop() {
        let Some(quasiquote_depth) = apply_reader_prefix_context(view, quasiquote_depth) else {
            continue;
        };
        let head = view.children.first().and_then(atom_symbol_text);

        if view.kind == ExpressionKind::List {
            match head {
                Some(head) if common_lisp_operator_head_eq(head, "quote") => continue,
                Some(head) if common_lisp_operator_head_eq(head, "quasiquote") => {
                    stack.extend(view.children.iter().skip(1).rev().map(|child| {
                        (
                            child,
                            quasiquote_depth + 1,
                            block_names.clone(),
                            tag_names.clone(),
                        )
                    }));
                    continue;
                }
                Some(head)
                    if quasiquote_depth > 0
                        && (common_lisp_operator_head_eq(head, "unquote")
                            || common_lisp_operator_head_eq(head, "unquote-splicing")) =>
                {
                    stack.extend(view.children.iter().skip(1).rev().map(|child| {
                        (
                            child,
                            quasiquote_depth - 1,
                            block_names.clone(),
                            tag_names.clone(),
                        )
                    }));
                    continue;
                }
                _ => {}
            }
        }

        if quasiquote_depth == 0 && view.kind == ExpressionKind::List {
            if let Some(head) = head {
                let operator = CommonLispOperator::from_head(head);

                if common_lisp_operator_head_eq(head, "return")
                    && !block_names
                        .iter()
                        .any(|bound| common_lisp_block_name_eq(bound, "nil"))
                {
                    bail!("extract-local-function cannot move {head} across a function boundary");
                }
                if common_lisp_operator_head_eq(head, "return-from") {
                    let local =
                        view.children
                            .get(1)
                            .and_then(atom_symbol_text)
                            .is_some_and(|name| {
                                block_names
                                    .iter()
                                    .any(|bound| common_lisp_block_name_eq(name, bound))
                            });
                    if !local {
                        bail!(
                            "extract-local-function cannot move {head} across a function boundary"
                        );
                    }
                }
                if common_lisp_operator_head_eq(head, "go") {
                    let local = view
                        .children
                        .get(1)
                        .and_then(common_lisp_control_tag)
                        .is_some_and(|name| {
                            tag_names
                                .iter()
                                .any(|bound| common_lisp_control_tag_eq(&name, bound))
                        });
                    if !local {
                        bail!(
                            "extract-local-function cannot move {head} across a function boundary"
                        );
                    }
                }

                if operator.is_some_and(|operator| {
                    matches!(
                        operator.local_callable_form(),
                        Some(
                            CommonLispLocalCallableForm::Flet | CommonLispLocalCallableForm::Labels
                        )
                    )
                }) {
                    stack.extend(view.children.iter().skip(2).rev().map(|child| {
                        (
                            child,
                            quasiquote_depth,
                            block_names.clone(),
                            tag_names.clone(),
                        )
                    }));
                    if let Some(definitions) = view.children.get(1) {
                        for definition in definitions.children.iter().rev() {
                            let Some(name) = definition
                                .children
                                .first()
                                .and_then(common_lisp_function_block_name)
                            else {
                                continue;
                            };
                            let mut function_blocks = block_names.clone();
                            function_blocks.push(name.to_owned());
                            stack.extend(definition.children.iter().skip(2).rev().map(|child| {
                                (
                                    child,
                                    quasiquote_depth,
                                    function_blocks.clone(),
                                    tag_names.clone(),
                                )
                            }));
                        }
                    }
                    continue;
                }

                if operator.is_some_and(|operator| {
                    matches!(
                        operator,
                        CommonLispOperator::Defun
                            | CommonLispOperator::Defmacro
                            | CommonLispOperator::Defmethod
                            | CommonLispOperator::ClDefmethod
                    )
                }) {
                    let body_start = operator
                        .and_then(|operator| common_lisp_definition_body_start(operator, view));
                    if let (Some(name), Some(body_start)) = (
                        view.children
                            .get(1)
                            .and_then(common_lisp_function_block_name),
                        body_start,
                    ) {
                        let mut function_blocks = block_names.clone();
                        function_blocks.push(name.to_owned());
                        stack.extend(view.children.iter().skip(body_start).rev().map(|child| {
                            (
                                child,
                                quasiquote_depth,
                                function_blocks.clone(),
                                tag_names.clone(),
                            )
                        }));
                    }
                    continue;
                }

                if common_lisp_operator_head_eq(head, "block") {
                    let mut nested_blocks = block_names.clone();
                    if let Some(name) = view.children.get(1).and_then(atom_symbol_text) {
                        nested_blocks.push(name.to_owned());
                    }
                    stack.extend(view.children.iter().skip(2).rev().map(|child| {
                        (
                            child,
                            quasiquote_depth,
                            nested_blocks.clone(),
                            tag_names.clone(),
                        )
                    }));
                    continue;
                }
                if operator == Some(CommonLispOperator::Loop) {
                    let mut nested_blocks = block_names.clone();
                    if let Some(name) = common_lisp_loop_block_name(view) {
                        nested_blocks.push(name.to_owned());
                    }
                    stack.extend(view.children.iter().skip(1).rev().map(|child| {
                        (
                            child,
                            quasiquote_depth,
                            nested_blocks.clone(),
                            tag_names.clone(),
                        )
                    }));
                    continue;
                }
                if operator
                    .is_some_and(|operator| operator.is_do_binding() || operator.is_prog_binding())
                {
                    let mut nested_blocks = block_names.clone();
                    nested_blocks.push("nil".to_owned());
                    stack.extend(view.children.iter().skip(1).rev().map(|child| {
                        (
                            child,
                            quasiquote_depth,
                            nested_blocks.clone(),
                            tag_names.clone(),
                        )
                    }));
                    continue;
                }
                if common_lisp_operator_head_eq(head, "tagbody") {
                    let mut nested_tags = tag_names.clone();
                    nested_tags.extend(
                        view.children
                            .iter()
                            .skip(1)
                            .filter_map(common_lisp_control_tag),
                    );
                    stack.extend(view.children.iter().skip(1).rev().map(|child| {
                        (
                            child,
                            quasiquote_depth,
                            block_names.clone(),
                            nested_tags.clone(),
                        )
                    }));
                    continue;
                }
            }
        }

        stack.extend(view.children.iter().rev().map(|child| {
            (
                child,
                quasiquote_depth,
                block_names.clone(),
                tag_names.clone(),
            )
        }));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(
        input: &str,
        target: &Path,
        enclosing: &Path,
        recursive: bool,
    ) -> Result<ExtractLocalFunctionPlan> {
        let tree = SyntaxTree::parse(input)?;
        plan_extract_local_function(ExtractLocalFunctionRequest {
            input,
            selection: tree.select_path(target)?,
            path: Some(target.clone()),
            enclosing: tree.select_path(enclosing)?,
            enclosing_path: enclosing.clone(),
            dialect: Dialect::CommonLisp,
            name: SymbolName::new("compute")?,
            explicit_params: Vec::new(),
            infer_params: true,
            recursive,
        })
    }

    #[test]
    fn dialect_matrix_gates_unsupported_dialects_before_parsing() {
        for (dialect, supported) in [
            (Dialect::CommonLisp, true),
            (Dialect::EmacsLisp, false),
            (Dialect::Scheme, false),
            (Dialect::Clojure, false),
            (Dialect::Janet, false),
            (Dialect::Fennel, false),
            (Dialect::Unknown, false),
        ] {
            let parse_attempted = std::cell::Cell::new(false);
            match ensure_common_lisp_dialect(dialect) {
                Ok(()) => {
                    parse_attempted.set(true);
                    assert!(SyntaxTree::parse_with_dialect(")", dialect).is_err());
                }
                Err(error) => {
                    assert!(!supported);
                    assert_eq!(
                        error.to_string(),
                        "extract-local-function currently supports only Common Lisp"
                    );
                }
            }

            assert_eq!(parse_attempted.get(), supported);
        }
    }

    #[test]
    fn preserves_common_lisp_reader_character_literal() -> Result<()> {
        let input = r"(progn (print #\)) (finish))";
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp)?;
        let path: Path = "0.1".parse()?;
        let enclosing_path: Path = "0".parse()?;
        let selection = tree.select_path(&path)?;
        let enclosing = tree.select_path(&enclosing_path)?;

        let result = plan_extract_local_function(ExtractLocalFunctionRequest {
            input,
            selection,
            path: Some(path),
            enclosing,
            enclosing_path,
            dialect: Dialect::CommonLisp,
            name: SymbolName::new("compute")?,
            explicit_params: Vec::new(),
            infer_params: true,
            recursive: false,
        })?;

        assert!(result.rewritten.contains(r"#\)"));
        SyntaxTree::parse_with_dialect(&result.rewritten, Dialect::CommonLisp)?;
        Ok(())
    }

    #[test]
    fn extracts_into_flet_and_infers_free_values() {
        let result = plan(
            "(defun render (x) (print (+ x 1)))",
            &Path::from_indexes(vec![0, 3, 1]),
            &Path::from_indexes(vec![0, 3]),
            false,
        )
        .expect("plan");
        assert_eq!(result.params, vec!["x"]);
        assert_eq!(
            result.rewritten,
            "(defun render (x) (flet ((compute (x) (+ x 1))) (print (compute x))))"
        );
    }

    #[test]
    fn recursive_uses_labels() {
        let result = plan(
            "(defun render () (print (+ 1 2)))",
            &Path::from_indexes(vec![0, 3, 1]),
            &Path::from_indexes(vec![0, 3]),
            true,
        )
        .expect("plan");
        assert!(result.rewritten.contains("(labels ((compute () (+ 1 2)))"));
    }

    #[test]
    fn recursive_allows_self_calls_inside_the_extracted_body() {
        let result = plan(
            "(defun render (x) (print (if (zerop x) 0 (compute (- x 1)))))",
            &Path::from_indexes(vec![0, 3, 1]),
            &Path::from_indexes(vec![0, 3]),
            true,
        )
        .expect("plan");
        assert!(result.rewritten.contains("(labels ((compute"));
        assert!(result.rewritten.contains("(compute (- x 1))"));
    }

    #[test]
    fn nested_local_binding_shadows_the_extracted_function() {
        let result = plan(
            "(progn (flet ((compute () 1)) (mapcar #'compute xs)) (+ x 1))",
            &Path::from_indexes(vec![0, 2]),
            &Path::from_indexes(vec![0]),
            false,
        )
        .expect("nested local binding owns its references");
        assert!(result.rewritten.contains("(flet ((compute (x) (+ x 1)))"));
    }

    #[test]
    fn macrolet_shadows_calls_but_not_function_designators() {
        plan(
            "(progn (macrolet ((compute () 1)) (compute)) (+ x 1))",
            &Path::from_indexes(vec![0, 2]),
            &Path::from_indexes(vec![0]),
            false,
        )
        .expect("macro binding owns the direct call");

        let error = plan(
            "(progn (macrolet ((compute () 1)) (mapcar #'compute xs)) (+ x 1))",
            &Path::from_indexes(vec![0, 2]),
            &Path::from_indexes(vec![0]),
            false,
        )
        .expect_err("macro bindings do not bind the function namespace");
        assert!(error.to_string().contains("function designator"));
    }

    #[test]
    fn compiler_macrolet_shadows_calls_but_not_function_designators() {
        plan(
            "(progn (compiler-macrolet ((compute () 1)) (compute)) (+ x 1))",
            &Path::from_indexes(vec![0, 2]),
            &Path::from_indexes(vec![0]),
            false,
        )
        .expect("compiler macro binding owns the direct call");

        let error = plan(
            "(progn (compiler-macrolet ((compute () 1)) (mapcar (function compute) xs)) (+ x 1))",
            &Path::from_indexes(vec![0, 2]),
            &Path::from_indexes(vec![0]),
            false,
        )
        .expect_err("compiler macro bindings do not bind the function namespace");
        assert!(error.to_string().contains("function designator"));
    }

    #[test]
    fn flet_shadows_calls_and_function_designators() {
        plan(
            "(progn (flet ((compute () 1)) (compute) #'compute (function compute)) (+ x 1))",
            &Path::from_indexes(vec![0, 2]),
            &Path::from_indexes(vec![0]),
            false,
        )
        .expect("function binding owns calls and function designators");
    }

    #[test]
    fn rejects_capture_of_existing_same_name_call() {
        let error = plan(
            "(defun render (x) (progn (compute 1) (+ x 1)))",
            &Path::from_indexes(vec![0, 3, 2]),
            &Path::from_indexes(vec![0, 3]),
            false,
        )
        .expect_err("captured call");
        assert!(error.to_string().contains("capture an existing call"));
    }

    #[test]
    fn rejects_capture_of_reader_function_designator() {
        let error = plan(
            "(progn (mapcar #'compute xs) (+ x 1))",
            &Path::from_indexes(vec![0, 2]),
            &Path::from_indexes(vec![0]),
            false,
        )
        .expect_err("captured function designator");
        assert!(error.to_string().contains("function designator"));
    }

    #[test]
    fn rejects_capture_of_function_form_designator() {
        let error = plan(
            "(progn (mapcar (function compute) xs) (+ x 1))",
            &Path::from_indexes(vec![0, 2]),
            &Path::from_indexes(vec![0]),
            false,
        )
        .expect_err("captured function designator");
        assert!(error.to_string().contains("function designator"));
    }

    #[test]
    fn ignores_quoted_calls_bindings_and_function_designators() {
        let result = plan(
            "(progn '(flet ((compute () 1)) (compute) (function compute) #'compute) (+ x 1))",
            &Path::from_indexes(vec![0, 2]),
            &Path::from_indexes(vec![0]),
            false,
        )
        .expect("quoted data must not be treated as executable code");
        assert!(result.rewritten.contains("(flet ((compute (x) (+ x 1)))"));
    }

    #[test]
    fn ignores_explicitly_quoted_and_quasiquoted_data() {
        for input in [
            "(progn (quote ((compute) (function compute))) (+ x 1))",
            "(progn (quasiquote ((compute) (function compute))) (+ x 1))",
        ] {
            plan(
                input,
                &Path::from_indexes(vec![0, 2]),
                &Path::from_indexes(vec![0]),
                false,
            )
            .expect("quoted data must not be treated as executable code");
        }
    }

    #[test]
    fn rejects_active_unquote_inside_explicit_quasiquote() {
        let error = plan(
            "(progn (quasiquote ((unquote (compute)))) (+ x 1))",
            &Path::from_indexes(vec![0, 2]),
            &Path::from_indexes(vec![0]),
            false,
        )
        .expect_err("unquoted call is executable");
        assert!(error.to_string().contains("capture an existing call"));
    }

    #[test]
    fn package_qualified_lookalikes_are_not_builtin_syntax() {
        for input in [
            "(progn (my:quote (compute)) (+ x 1))",
            "(progn (my:quasiquote (compute)) (+ x 1))",
        ] {
            let error = plan(
                input,
                &Path::from_indexes(vec![0, 2]),
                &Path::from_indexes(vec![0]),
                false,
            )
            .expect_err("package-qualified call must remain executable");
            assert!(error.to_string().contains("capture an existing call"));
        }

        plan(
            "(progn (my:function compute) (+ x 1))",
            &Path::from_indexes(vec![0, 2]),
            &Path::from_indexes(vec![0]),
            false,
        )
        .expect("package-qualified function lookalike is not a function designator");
        plan(
            "(progn (quasiquote ((my:unquote (compute)))) (+ x 1))",
            &Path::from_indexes(vec![0, 2]),
            &Path::from_indexes(vec![0]),
            false,
        )
        .expect("package-qualified unquote lookalike remains quoted data");
    }

    #[test]
    fn flet_definition_body_is_not_shadowed() {
        let error = plan(
            "(progn (flet ((compute () (compute))) nil) (+ x 1))",
            &Path::from_indexes(vec![0, 2]),
            &Path::from_indexes(vec![0]),
            false,
        )
        .expect_err("flet names are not visible in definition bodies");
        assert!(error.to_string().contains("capture an existing call"));
    }

    #[test]
    fn labels_definition_body_is_shadowed() {
        plan(
            "(progn (labels ((compute () (compute))) nil) (+ x 1))",
            &Path::from_indexes(vec![0, 2]),
            &Path::from_indexes(vec![0]),
            false,
        )
        .expect("labels names are visible in definition bodies");
    }

    #[test]
    fn lambda_list_is_not_scanned_as_executable_code() {
        plan(
            "(progn (lambda ((compute)) nil) (+ x 1))",
            &Path::from_indexes(vec![0, 2]),
            &Path::from_indexes(vec![0]),
            false,
        )
        .expect("lambda-list syntax is not a call");
    }

    #[test]
    fn deep_enclosing_form_does_not_overflow_the_stack() {
        const DEPTH: usize = 30_000;
        let mut input = String::from("(progn (+ 1 2) ");
        input.push_str(&"(".repeat(DEPTH));
        input.push('x');
        input.push_str(&")".repeat(DEPTH));
        input.push(')');

        let result = plan(
            &input,
            &Path::from_indexes(vec![0, 1]),
            &Path::from_indexes(vec![0]),
            false,
        )
        .expect("deep extraction plan");
        SyntaxTree::parse(&result.rewritten).expect("rewritten tree");
    }

    #[test]
    fn deep_function_designator_is_rejected_without_overflowing_the_stack() {
        const DEPTH: usize = 30_000;
        let mut input = String::from("(progn (+ 1 2) ");
        input.push_str(&"(".repeat(DEPTH));
        input.push_str("#'compute");
        input.push_str(&")".repeat(DEPTH));
        input.push(')');

        let error = plan(
            &input,
            &Path::from_indexes(vec![0, 1]),
            &Path::from_indexes(vec![0]),
            false,
        )
        .expect_err("deep function designator must be rejected");
        assert!(error.to_string().contains("function designator"));
    }

    #[test]
    fn rejects_non_local_control_transfer() {
        let error = plan(
            "(defun render () (block done (return-from done 1)))",
            &Path::from_indexes(vec![0, 3, 2]),
            &Path::from_indexes(vec![0, 3]),
            false,
        )
        .expect_err("return-from");
        assert!(error.to_string().contains("function boundary"));
    }

    #[test]
    fn rejects_return_in_executable_context() {
        for input in ["(progn (return 1) (+ x 1))", "(loop (return 1))"] {
            let error = plan(
                input,
                &Path::from_indexes(vec![0, 1]),
                &Path::from_indexes(vec![0]),
                false,
            )
            .expect_err("return crosses the new function boundary");
            assert!(error.to_string().contains("function boundary"));
        }
    }

    #[test]
    fn return_respects_quote_quasiquote_and_active_unquote() {
        for input in [
            "(progn (quote (return 1)) (+ x 1))",
            "(progn (quasiquote ((return 1))) (+ x 1))",
        ] {
            plan(
                input,
                &Path::from_indexes(vec![0, 2]),
                &Path::from_indexes(vec![0]),
                false,
            )
            .expect("quoted return is data");
        }

        let error = plan(
            "(progn (quasiquote ((unquote (return 1)))) (+ x 1))",
            &Path::from_indexes(vec![0, 1]),
            &Path::from_indexes(vec![0]),
            false,
        )
        .expect_err("active unquote executes return");
        assert!(error.to_string().contains("function boundary"));
    }

    #[test]
    fn allows_control_transfer_bound_inside_the_selection() {
        for input in [
            "(progn (block done (return-from done 1)) (+ x 1))",
            "(progn (block nil (return 1)) (+ x 1))",
            "(progn (loop (return 1)) (+ x 1))",
            "(progn (loop (return-from nil 1)) (+ x 1))",
            "(progn (do () (nil) (return 1)) (+ x 1))",
            "(progn (do* () (nil) (return 1)) (+ x 1))",
            "(progn (prog () (return 1)) (+ x 1))",
            "(progn (prog* () (return 1)) (+ x 1))",
        ] {
            plan(
                input,
                &Path::from_indexes(vec![0, 1]),
                &Path::from_indexes(vec![0]),
                false,
            )
            .expect("block and return move together");
        }
        plan(
            "(progn (tagbody start (go start)) (+ x 1))",
            &Path::from_indexes(vec![0, 1]),
            &Path::from_indexes(vec![0]),
            false,
        )
        .expect("tagbody and go move together");
    }

    #[test]
    fn recognizes_standard_nil_aliases_as_the_same_block_name() {
        for input in [
            "(progn (block cl:nil (return 1)) (+ x 1))",
            "(progn (block common-lisp::nil (return-from nil 1)) (+ x 1))",
            "(progn (loop (return-from cl:nil 1)) (+ x 1))",
            "(progn (do () (nil) (return-from common-lisp:nil 1)) (+ x 1))",
            "(progn (prog () (return-from cl::nil 1)) (+ x 1))",
        ] {
            plan(
                input,
                &Path::from_indexes(vec![0, 1]),
                &Path::from_indexes(vec![0]),
                false,
            )
            .expect("standard NIL aliases denote the same control block");
        }
    }

    #[test]
    fn named_loop_establishes_only_its_named_block() {
        plan(
            "(progn (loop named done do (return-from done 1)) (+ x 1))",
            &Path::from_indexes(vec![0, 1]),
            &Path::from_indexes(vec![0]),
            false,
        )
        .expect("named LOOP binds its declared block name");

        let error = plan(
            "(block nil (progn (loop named done do (return 1)) (+ x 1)))",
            &Path::from_indexes(vec![0, 2, 1]),
            &Path::from_indexes(vec![0, 2]),
            false,
        )
        .expect_err("named LOOP does not establish an implicit NIL block");
        assert!(error.to_string().contains("function boundary"));
    }

    #[test]
    fn unrelated_package_nil_is_not_the_standard_nil_block() {
        let error = plan(
            "(progn (block app:nil (return 1)) (+ x 1))",
            &Path::from_indexes(vec![0, 1]),
            &Path::from_indexes(vec![0]),
            false,
        )
        .expect_err("an unrelated package's NIL symbol is a distinct block name");
        assert!(error.to_string().contains("function boundary"));
    }

    #[test]
    fn allows_self_return_from_in_named_function_bodies() {
        for input in [
            "(progn (flet ((foo () (return-from foo 1))) (foo)) 2)",
            "(progn (labels ((foo () (return-from foo 1))) (foo)) 2)",
            "(progn (cl:flet ((foo () (return-from foo 1))) (foo)) 2)",
            "(progn (defun foo () (return-from foo 1)) 2)",
            "(progn (defun (setf widget) (value object) (return-from widget value)) 2)",
            "(progn (defun app:foo () (return-from app::FOO 1)) 2)",
            "(progn (defun (setf app:widget) (value object) (return-from app::WIDGET value)) 2)",
            "(progn (defmacro foo () (return-from foo 1)) 2)",
            "(progn (defmethod foo :around ((x t)) (return-from foo 1)) 2)",
        ] {
            plan(
                input,
                &Path::from_indexes(vec![0, 1]),
                &Path::from_indexes(vec![0]),
                false,
            )
            .expect("named function establishes an implicit block");
        }
    }

    #[test]
    fn rejects_return_from_a_different_named_function() {
        for input in [
            "(progn (flet ((foo () (return-from outer 1))) (foo)) 2)",
            "(progn (labels ((foo () (return-from outer 1))) (foo)) 2)",
            "(progn (labels ((foo () 1) (bar () (return-from foo 2))) (bar)) 3)",
        ] {
            let error = plan(
                input,
                &Path::from_indexes(vec![0, 1]),
                &Path::from_indexes(vec![0]),
                false,
            )
            .expect_err("different function block");
            assert!(error.to_string().contains("function boundary"));
        }
    }

    #[test]
    fn allows_package_qualified_control_targets() {
        for input in [
            "(progn (block app:done (return-from app::DONE 1)) (+ x 1))",
            "(progn (tagbody app:start (go app::START)) (+ x 1))",
            "(progn (block cl:done (return-from common-lisp::DONE 1)) (+ x 1))",
            "(progn (tagbody common-lisp:start (go cl::START)) (+ x 1))",
        ] {
            plan(
                input,
                &Path::from_indexes(vec![0, 1]),
                &Path::from_indexes(vec![0]),
                false,
            )
            .expect("package-qualified control target moves with its binding");
        }
    }

    #[test]
    fn rejects_control_targets_from_different_explicit_packages() {
        for input in [
            "(progn (block pkg-a:done (return-from pkg-b:done 1)) (+ x 1))",
            "(progn (defun pkg-a:foo () (return-from pkg-b:foo 1)) 2)",
            "(progn (tagbody pkg-a:start (go pkg-b:start)) (+ x 1))",
            "(progn (block cl:done (return-from done 1)) (+ x 1))",
            "(progn (block #:done (return-from cl:done 1)) (+ x 1))",
            "(progn (block |cl|:done (return-from common-lisp:done 1)) (+ x 1))",
        ] {
            let error = plan(
                input,
                &Path::from_indexes(vec![0, 1]),
                &Path::from_indexes(vec![0]),
                false,
            )
            .expect_err("different explicit packages name different control targets");
            assert!(error.to_string().contains("function boundary"));
        }
    }

    #[test]
    fn compares_integer_tagbody_tags_by_common_lisp_integer_value() {
        for input in [
            "(progn (tagbody 10 (go #xA)) (+ x 1))",
            "(progn (tagbody 10. (go 10)) (+ x 1))",
            "(progn (tagbody +10. (go #d10)) (+ x 1))",
            "(progn (tagbody -0. (go 0)) (+ x 1))",
            "(progn (tagbody -10 (go #x-A)) (+ x 1))",
            "(progn (tagbody #b1010 (go #o12)) (+ x 1))",
            "(progn (tagbody #36rz (go 35)) (+ x 1))",
            "(progn (tagbody 340282366920938463463374607431768211456 (go #x100000000000000000000000000000000)) (+ x 1))",
        ] {
            plan(
                input,
                &Path::from_indexes(vec![0, 1]),
                &Path::from_indexes(vec![0]),
                false,
            )
            .expect("equal integer tags move with their TAGBODY");
        }
    }

    #[test]
    fn bounds_arbitrary_precision_integer_tag_conversion() {
        let at_limit = format!("1{}", "0".repeat(MAX_COMMON_LISP_INTEGER_TAG_BYTES - 1));
        assert!(parse_common_lisp_integer_tag(&at_limit).is_some());

        let over_limit = format!("1{}", "0".repeat(MAX_COMMON_LISP_INTEGER_TAG_BYTES));
        assert!(parse_common_lisp_integer_tag(&over_limit).is_none());

        let tree = SyntaxTree::parse(&over_limit).expect("oversized integer is valid syntax");
        let view = tree
            .select_path(&Path::from_indexes(vec![0]))
            .expect("integer atom")
            .view();
        assert!(common_lisp_control_tag(&view).is_none());
    }

    #[test]
    fn rejects_non_integer_numeric_tagbody_targets() {
        for input in [
            "(progn (tagbody 1.0 (go 1.0)) (+ x 1))",
            "(progn (tagbody 1/2 (go 1/2)) (+ x 1))",
            "(progn (tagbody #37r10 (go #37r10)) (+ x 1))",
            "(progn (tagbody #xg (go #xg)) (+ x 1))",
            "(progn (tagbody #d10. (go 10)) (+ x 1))",
            "(progn (tagbody #16ra. (go 10)) (+ x 1))",
        ] {
            let error = plan(
                input,
                &Path::from_indexes(vec![0, 1]),
                &Path::from_indexes(vec![0]),
                false,
            )
            .expect_err("unsupported numeric syntax is not an integer GO tag");
            assert!(error.to_string().contains("function boundary"));
        }
    }

    #[test]
    fn rejects_numeric_tagbody_tag_as_structural_target() {
        let error = plan(
            "(tagbody 10 (go 10))",
            &Path::from_indexes(vec![0, 1]),
            &Path::from_indexes(vec![0]),
            false,
        )
        .expect_err("a TAGBODY tag is structural syntax");
        assert!(error.to_string().contains("structural binding position"));
    }

    #[test]
    fn package_qualified_control_lookalikes_are_not_special_forms() {
        plan(
            "(progn (my:return 1) (+ x 1))",
            &Path::from_indexes(vec![0, 1]),
            &Path::from_indexes(vec![0]),
            false,
        )
        .expect("package-qualified return lookalike is an ordinary call");
    }

    #[test]
    fn rejects_structural_binding_positions() {
        for (input, target) in [
            ("(lambda (x) (print x))", vec![0, 1]),
            ("(defun render (x) (print x))", vec![0, 1]),
            ("(defun render (x) (print x))", vec![0, 2, 0]),
            ("(defmacro render (x) (print x))", vec![0, 1]),
            ("(defmacro render (x) (print x))", vec![0, 2]),
            (
                "(defmethod render :around :logging ((x t)) (print x))",
                vec![0, 1],
            ),
            (
                "(defmethod render :around :logging ((x t)) (print x))",
                vec![0, 3],
            ),
            (
                "(defmethod render :around :logging ((x t)) (print x))",
                vec![0, 4],
            ),
            ("(flet ((render (x) (print x))) (render 1))", vec![0, 1]),
            (
                "(flet ((render (x) (print x))) (render 1))",
                vec![0, 1, 0, 0],
            ),
            (
                "(flet ((render (x) (print x))) (render 1))",
                vec![0, 1, 0, 1],
            ),
            ("(destructuring-bind (x y) pair (+ x y))", vec![0, 1]),
            ("(multiple-value-bind (x y) values (+ x y))", vec![0, 1]),
            ("(do ((x 0 (1+ x))) ((> x 3) x) (print x))", vec![0, 1]),
            ("(do ((x 0 (1+ x))) ((> x 3) x) (print x))", vec![0, 2]),
            ("(do* ((x 0 (1+ x))) ((> x 3) x) (print x))", vec![0, 1]),
            ("(prog ((x 0)) (print x) (return x))", vec![0, 1]),
            ("(prog* ((x 0)) (print x) (return x))", vec![0, 1]),
        ] {
            let error = plan(
                input,
                &Path::from_indexes(target),
                &Path::from_indexes(vec![0]),
                false,
            )
            .expect_err("structural binding position");
            assert!(error.to_string().contains("structural binding position"));
        }
    }

    #[test]
    fn allows_named_definition_body_positions() {
        for (input, target) in [
            ("(defun render (x) (print x))", vec![0, 3]),
            ("(defmacro render (x) (print x))", vec![0, 3]),
            ("(defmethod render :around ((x t)) (print x))", vec![0, 4]),
            (
                "(defmethod render :around :logging ((x t)) (print x))",
                vec![0, 5],
            ),
        ] {
            plan(
                input,
                &Path::from_indexes(target),
                &Path::from_indexes(vec![0]),
                false,
            )
            .expect("definition body position is executable");
        }
    }

    #[test]
    fn rejects_assignment_places() {
        for (input, target) in [
            ("(setq x (+ y 1))", vec![0, 1]),
            ("(psetq x (+ y 1) z (+ y 2))", vec![0, 3]),
            ("(multiple-value-setq (x y) (values 1 2))", vec![0, 1, 0]),
            ("(setf (car items) (+ y 1))", vec![0, 1, 1]),
            ("(psetf (car items) 1 (cdr items) 2)", vec![0, 3]),
            ("(rotatef (car items) (cdr items))", vec![0, 2]),
            ("(shiftf (car items) (cdr items) (+ y 1))", vec![0, 2]),
            ("(cl:setq x (+ y 1))", vec![0, 1]),
            ("(common-lisp:setf (car items) (+ y 1))", vec![0, 1]),
        ] {
            let error = plan(
                input,
                &Path::from_indexes(target),
                &Path::from_indexes(vec![0]),
                false,
            )
            .expect_err("assignment place");
            assert!(error.to_string().contains("structural binding position"));
        }
    }

    #[test]
    fn allows_assignment_value_positions() {
        for (input, target) in [
            ("(setq x (+ y 1))", vec![0, 2]),
            ("(psetq x (+ y 1) z (+ y 2))", vec![0, 4]),
            ("(multiple-value-setq (x y) (values y 2))", vec![0, 2]),
            ("(setf (car items) (+ y 1))", vec![0, 2]),
            (
                "(psetf (car items) (+ y 1) (cdr items) (+ y 2))",
                vec![0, 4],
            ),
            ("(shiftf (car items) (cdr items) (+ y 1))", vec![0, 3]),
            ("(cl:setq x (+ y 1))", vec![0, 2]),
            ("(common-lisp:psetf (car items) (+ y 1))", vec![0, 2]),
        ] {
            plan(
                input,
                &Path::from_indexes(target),
                &Path::from_indexes(vec![0]),
                false,
            )
            .expect("assignment value position");
        }
    }

    #[test]
    fn rejects_non_executable_control_positions() {
        for (input, target) in [
            ("(block done (print 1))", vec![0, 1]),
            ("(return-from done 1)", vec![0, 1]),
            ("(go done)", vec![0, 1]),
            ("(tagbody start (print 1))", vec![0, 1]),
            ("(tagbody 10 (print 1))", vec![0, 1]),
        ] {
            let error = plan(
                input,
                &Path::from_indexes(target),
                &Path::from_indexes(vec![0]),
                false,
            )
            .expect_err("non-executable control position");
            assert!(error.to_string().contains("structural"));
        }
    }

    #[test]
    fn allows_executable_control_operands() {
        for (input, target) in [
            ("(catch (next-tag) (work))", vec![0, 1]),
            ("(catch (next-tag) (work))", vec![0, 2]),
            ("(throw (next-tag) (result))", vec![0, 1]),
            ("(throw (next-tag) (result))", vec![0, 2]),
        ] {
            plan(
                input,
                &Path::from_indexes(target),
                &Path::from_indexes(vec![0]),
                false,
            )
            .expect("catch and throw operands are evaluated");
        }
    }

    #[test]
    fn allows_do_end_clause_expressions() {
        for operator in ["do", "do*"] {
            let input = format!("({operator} ((x 0 (1+ x))) ((> x 3) (finish x)) (print x))");
            for target in [vec![0, 2, 0], vec![0, 2, 1]] {
                plan(
                    &input,
                    &Path::from_indexes(target),
                    &Path::from_indexes(vec![0]),
                    false,
                )
                .expect("do end-test and result forms are evaluated");
            }
        }
    }

    #[test]
    fn allows_runtime_local_callable_definition_bodies() {
        for input in [
            "(flet ((render (x) (+ x 1))) (render 1))",
            "(labels ((render (x) (+ x 1))) (render 1))",
        ] {
            plan(
                input,
                &Path::from_indexes(vec![0, 1, 0, 2]),
                &Path::from_indexes(vec![0]),
                false,
            )
            .expect("runtime local function body is executable");
        }
    }

    #[test]
    fn allows_executable_forms_adjacent_to_structural_bindings() {
        for (input, target_index) in [
            ("(destructuring-bind (x y) pair (+ x y))", 2),
            ("(multiple-value-bind (x y) values (+ x y))", 2),
            ("(do ((x 0 (1+ x))) ((> x 3) x) (print x))", 3),
            ("(do* ((x 0 (1+ x))) ((> x 3) x) (print x))", 3),
            ("(prog ((x 0)) (print x) (return x))", 2),
            ("(prog* ((x 0)) (print x) (return x))", 2),
        ] {
            plan(
                input,
                &Path::from_indexes(vec![0, target_index]),
                &Path::from_indexes(vec![0]),
                false,
            )
            .expect("adjacent executable form");
        }
    }

    #[test]
    fn allows_runtime_binding_initializer_and_step_forms() {
        for (input, targets) in [
            ("(let ((x (+ a b))) x)", vec![vec![0, 1, 0, 1]]),
            ("(let* ((x (+ a b))) x)", vec![vec![0, 1, 0, 1]]),
            ("(prog ((x (+ a b))) (return x))", vec![vec![0, 1, 0, 1]]),
            (
                "(do ((x (+ a b) (+ x 1))) ((done-p x) x))",
                vec![vec![0, 1, 0, 1], vec![0, 1, 0, 2]],
            ),
            (
                "(do* ((x (+ a b) (+ x 1))) ((done-p x) x))",
                vec![vec![0, 1, 0, 1], vec![0, 1, 0, 2]],
            ),
        ] {
            for target in targets {
                plan(
                    input,
                    &Path::from_indexes(target),
                    &Path::from_indexes(vec![0]),
                    false,
                )
                .expect("binding initializer and step forms are evaluated");
            }
        }
    }

    #[test]
    fn rejects_structural_binding_list_entry_and_name() {
        for (input, target) in [
            ("(let ((x (+ a b))) x)", vec![0, 1]),
            ("(let ((x (+ a b))) x)", vec![0, 1, 0]),
            ("(let ((x (+ a b))) x)", vec![0, 1, 0, 0]),
            ("(do ((x (+ a b) (+ x 1))) ((done-p x) x))", vec![0, 1, 0]),
            (
                "(do ((x (+ a b) (+ x 1))) ((done-p x) x))",
                vec![0, 1, 0, 0],
            ),
        ] {
            let error = plan(
                input,
                &Path::from_indexes(target),
                &Path::from_indexes(vec![0]),
                false,
            )
            .expect_err("binding syntax is structural");
            assert!(error.to_string().contains("structural"));
        }
    }

    #[test]
    fn allows_lambda_list_runtime_init_forms() {
        for (input, target) in [
            (
                "(lambda (&optional (x (+ a b) supplied-p)) x)",
                vec![0, 1, 1, 1],
            ),
            ("(lambda (&key (x (+ a b) supplied-p)) x)", vec![0, 1, 1, 1]),
            ("(lambda (&aux (x (+ a b))) x)", vec![0, 1, 1, 1]),
            ("(defun render (&optional (x (+ a b))) x)", vec![0, 2, 1, 1]),
            (
                "(flet ((render (&optional (x (+ a b))) x)) (render))",
                vec![0, 1, 0, 1, 1, 1],
            ),
        ] {
            plan(
                input,
                &Path::from_indexes(target),
                &Path::from_indexes(vec![0]),
                false,
            )
            .expect("lambda-list init form is evaluated");
        }
    }

    #[test]
    fn rejects_structural_lambda_list_slots() {
        let input = "(lambda (&optional (x (+ a b) supplied-p)) x)";
        for target in [
            vec![0, 1],
            vec![0, 1, 1],
            vec![0, 1, 1, 0],
            vec![0, 1, 1, 2],
        ] {
            let error = plan(
                input,
                &Path::from_indexes(target),
                &Path::from_indexes(vec![0]),
                false,
            )
            .expect_err("lambda-list syntax is structural");
            assert!(error.to_string().contains("structural"));
        }
    }

    #[test]
    fn distinguishes_runtime_and_structural_condition_clause_slots() {
        for operator in ["handler-case", "restart-case"] {
            let input = format!("({operator} (work) (error (condition) (+ a b)))");
            plan(
                &input,
                &Path::from_indexes(vec![0, 2, 2]),
                &Path::from_indexes(vec![0]),
                false,
            )
            .expect("condition clause body is evaluated");
            for target in [vec![0, 2], vec![0, 2, 0], vec![0, 2, 1]] {
                let error = plan(
                    &input,
                    &Path::from_indexes(target),
                    &Path::from_indexes(vec![0]),
                    false,
                )
                .expect_err("condition clause signature is structural");
                assert!(error.to_string().contains("structural"));
            }
        }
    }

    #[test]
    fn validates_restart_case_option_and_body_boundaries() {
        let input = "(restart-case (work) (retry () :interactive (+ a b) :report (+ c d) :test (+ e f) (declare (ignorable marker)) (+ g h)))";
        for target in [vec![0, 2, 3], vec![0, 2, 5], vec![0, 2, 7], vec![0, 2, 9]] {
            plan(
                input,
                &Path::from_indexes(target),
                &Path::from_indexes(vec![0]),
                false,
            )
            .expect("restart-case option values and body forms are evaluated");
        }

        for target in [vec![0, 2, 2], vec![0, 2, 4], vec![0, 2, 6], vec![0, 2, 8]] {
            let error = plan(
                input,
                &Path::from_indexes(target),
                &Path::from_indexes(vec![0]),
                false,
            )
            .expect_err("restart-case option keywords and declarations are structural");
            assert!(error.to_string().contains("structural"));
        }
    }

    #[test]
    fn rejects_malformed_restart_case_options_fail_closed() {
        for (input, targets) in [
            (
                "(restart-case (work) (retry () :unknown (+ a b) (+ c d)))",
                vec![vec![0, 2, 3], vec![0, 2, 4]],
            ),
            (
                "(restart-case (work) (retry () :interactive (+ a b) :test))",
                vec![vec![0, 2, 3]],
            ),
            (
                "(restart-case (work) (retry () :interactive (+ a b) :interactive (+ c d) (+ e f)))",
                vec![vec![0, 2, 3], vec![0, 2, 5], vec![0, 2, 6]],
            ),
        ] {
            for target in targets {
                let error = plan(
                    input,
                    &Path::from_indexes(target),
                    &Path::from_indexes(vec![0]),
                    false,
                )
                .expect_err("malformed restart-case options must fail closed");
                assert!(error.to_string().contains("structural"));
            }
        }
    }

    #[test]
    fn distinguishes_runtime_and_structural_handler_bind_slots() {
        for operator in ["handler-bind", "restart-bind"] {
            let input = format!(
                "({operator} ((error (function handle-error)) (warning (function handle-warning))) (+ a b))"
            );
            for target in [vec![0, 1, 0, 1], vec![0, 1, 1, 1]] {
                plan(
                    &input,
                    &Path::from_indexes(target),
                    &Path::from_indexes(vec![0]),
                    false,
                )
                .expect("handler function form is evaluated");
            }
            for target in [vec![0, 1], vec![0, 1, 0], vec![0, 1, 0, 0]] {
                let error = plan(
                    &input,
                    &Path::from_indexes(target),
                    &Path::from_indexes(vec![0]),
                    false,
                )
                .expect_err("handler binding syntax is structural");
                assert!(error.to_string().contains("structural"));
            }
        }
    }

    #[test]
    fn validates_restart_bind_option_value_slots() {
        let input = "(restart-bind ((retry (+ a b) :interactive-function (+ c d) :report-function (+ e f) :test-function (+ g h))) (+ i j))";
        for target in [
            vec![0, 1, 0, 1],
            vec![0, 1, 0, 3],
            vec![0, 1, 0, 5],
            vec![0, 1, 0, 7],
        ] {
            plan(
                input,
                &Path::from_indexes(target),
                &Path::from_indexes(vec![0]),
                false,
            )
            .expect("restart-bind function and option value forms are evaluated");
        }

        for target in [vec![0, 1, 0, 2], vec![0, 1, 0, 4], vec![0, 1, 0, 6]] {
            let error = plan(
                input,
                &Path::from_indexes(target),
                &Path::from_indexes(vec![0]),
                false,
            )
            .expect_err("restart-bind option keywords are structural");
            assert!(error.to_string().contains("structural"));
        }
    }

    #[test]
    fn rejects_malformed_restart_bind_options_without_broadening_handler_bind() {
        for (input, targets) in [
            (
                "(restart-bind ((retry (+ a b) :unknown (+ c d))) (+ e f))",
                vec![vec![0, 1, 0, 1], vec![0, 1, 0, 3]],
            ),
            (
                "(restart-bind ((retry (+ a b) :interactive-function (+ c d) :test-function)) (+ e f))",
                vec![vec![0, 1, 0, 1], vec![0, 1, 0, 3]],
            ),
            (
                "(restart-bind ((retry (+ a b) :test-function (+ c d) :test-function (+ e f))) (+ g h))",
                vec![vec![0, 1, 0, 1], vec![0, 1, 0, 3], vec![0, 1, 0, 5]],
            ),
            (
                "(handler-bind ((error (+ a b) :test-function (+ c d))) (+ e f))",
                vec![vec![0, 1, 0, 3]],
            ),
        ] {
            for target in targets {
                let error = plan(
                    input,
                    &Path::from_indexes(target),
                    &Path::from_indexes(vec![0]),
                    false,
                )
                .expect_err("malformed or structural option positions must be rejected");
                assert!(error.to_string().contains("structural"));
            }
        }
    }

    #[test]
    fn distinguishes_runtime_and_structural_typecase_slots() {
        for operator in ["typecase", "etypecase", "ctypecase"] {
            let input = format!("({operator} value (integer (+ a b)) (otherwise 0))");
            plan(
                &input,
                &Path::from_indexes(vec![0, 2, 1]),
                &Path::from_indexes(vec![0]),
                false,
            )
            .expect("typecase clause body is evaluated");
            for target in [vec![0, 2], vec![0, 2, 0]] {
                let error = plan(
                    &input,
                    &Path::from_indexes(target),
                    &Path::from_indexes(vec![0]),
                    false,
                )
                .expect_err("typecase clause key is structural");
                assert!(error.to_string().contains("structural"));
            }
        }
    }

    #[test]
    fn distinguishes_runtime_and_structural_case_slots() {
        for operator in ["case", "ccase", "ecase"] {
            let input = format!("({operator} value ((1 2) (+ a b)) (otherwise 0))");
            plan(
                &input,
                &Path::from_indexes(vec![0, 2, 1]),
                &Path::from_indexes(vec![0]),
                false,
            )
            .expect("case clause body is evaluated");
            for target in [vec![0, 2], vec![0, 2, 0], vec![0, 2, 0, 0]] {
                let error = plan(
                    &input,
                    &Path::from_indexes(target),
                    &Path::from_indexes(vec![0]),
                    false,
                )
                .expect_err("case clause key is structural");
                assert!(error.to_string().contains("structural"));
            }
        }
    }

    #[test]
    fn distinguishes_eval_when_and_load_time_value_slots() {
        for (input, target) in [
            ("(eval-when (:execute) (+ a b))", vec![0, 2]),
            ("(load-time-value (+ a b) t)", vec![0, 1]),
        ] {
            plan(
                input,
                &Path::from_indexes(target),
                &Path::from_indexes(vec![0]),
                false,
            )
            .expect("runtime form is evaluated");
        }

        for (input, target) in [
            (
                "(eval-when (:compile-toplevel :execute) (+ a b))",
                vec![0, 1],
            ),
            (
                "(eval-when (:compile-toplevel :execute) (+ a b))",
                vec![0, 1, 0],
            ),
            ("(load-time-value (+ a b) t)", vec![0, 2]),
        ] {
            let error = plan(
                input,
                &Path::from_indexes(target),
                &Path::from_indexes(vec![0]),
                false,
            )
            .expect_err("non-evaluated control slot is structural");
            assert!(error.to_string().contains("structural"));
        }
    }

    #[test]
    fn distinguishes_runtime_and_structural_declaration_and_type_slots() {
        for (input, target) in [
            ("(the integer (+ a b))", vec![0, 2]),
            ("(proclaim (next-declaration))", vec![0, 1]),
        ] {
            plan(
                input,
                &Path::from_indexes(target),
                &Path::from_indexes(vec![0]),
                false,
            )
            .expect("runtime value form is evaluated");
        }

        for (input, target) in [
            ("(the (integer 0 10) (+ a b))", vec![0, 1]),
            ("(function handle-error)", vec![0, 1]),
            ("(locally (declare (optimize speed)) (+ a b))", vec![0, 1]),
            ("(declaim (optimize speed))", vec![0, 1]),
        ] {
            let error = plan(
                input,
                &Path::from_indexes(target),
                &Path::from_indexes(vec![0]),
                false,
            )
            .expect_err("declaration and type syntax is structural");
            assert!(error.to_string().contains("structural"));
        }
    }

    #[test]
    fn allows_whole_executable_clause_forms_and_rejects_declarations() {
        for selected in [
            "(handler-case (work) (error (condition) (recover condition)))",
            "(restart-case (work) (retry () (work)))",
            "(handler-bind ((error (function handle-error))) (work))",
            "(restart-bind ((retry (function retry-work))) (work))",
            "(typecase value (integer (work)))",
            "(etypecase value (integer (work)))",
            "(ctypecase value (integer (work)))",
            "(case value (1 (work)))",
            "(ccase value (1 (work)))",
            "(ecase value (1 (work)))",
        ] {
            let input = format!("(progn {selected} (finish))");
            plan(
                &input,
                &Path::from_indexes(vec![0, 1]),
                &Path::from_indexes(vec![0]),
                false,
            )
            .expect("whole executable form can be extracted");
        }

        for selected in ["(declare (optimize speed))", "(declaim (optimize speed))"] {
            let input = format!("(progn {selected} (finish))");
            let error = plan(
                &input,
                &Path::from_indexes(vec![0, 1]),
                &Path::from_indexes(vec![0]),
                false,
            )
            .expect_err("whole declaration form cannot be extracted");
            assert!(error.to_string().contains("structural"));
        }
    }

    #[test]
    fn generated_binding_is_not_swallowed_by_a_line_comment() {
        let result = plan(
            "(defun render () (progn ; retain comment\n  (+ 1 2)))",
            &Path::from_indexes(vec![0, 3, 1]),
            &Path::from_indexes(vec![0, 3]),
            false,
        )
        .expect("plan");
        SyntaxTree::parse(&result.rewritten).expect("rewritten parse");
        assert!(result.rewritten.starts_with("(defun render () (flet"));
    }

    #[test]
    fn rejects_selection_from_a_different_source() {
        let input = "(defun render (x) (print (+ x 1)))";
        let input_tree = SyntaxTree::parse(input).expect("input tree");
        let foreign_tree =
            SyntaxTree::parse("(defun render (y) (print (- y 2)))").expect("foreign tree");
        let target = Path::from_indexes(vec![0, 3, 1]);
        let enclosing = Path::from_indexes(vec![0, 3]);

        let error = plan_extract_local_function(ExtractLocalFunctionRequest {
            input,
            selection: foreign_tree
                .select_path(&target)
                .expect("foreign selection"),
            path: Some(target),
            enclosing: input_tree.select_path(&enclosing).expect("input enclosing"),
            enclosing_path: enclosing,
            dialect: Dialect::CommonLisp,
            name: SymbolName::new("compute").expect("name"),
            explicit_params: Vec::new(),
            infer_params: true,
            recursive: false,
        })
        .expect_err("foreign selection");

        assert!(error.to_string().contains("does not match the source"));
    }

    #[test]
    fn rejects_enclosing_selection_from_a_different_source() {
        let input = "(defun render (x) (print (+ x 1)))";
        let input_tree = SyntaxTree::parse(input).expect("input tree");
        let foreign_tree =
            SyntaxTree::parse("(defun render (y) (print (- y 2)))").expect("foreign tree");
        let target = Path::from_indexes(vec![0, 3, 1]);
        let enclosing = Path::from_indexes(vec![0, 3]);

        let error = plan_extract_local_function(ExtractLocalFunctionRequest {
            input,
            selection: input_tree.select_path(&target).expect("input selection"),
            path: Some(target),
            enclosing: foreign_tree
                .select_path(&enclosing)
                .expect("foreign enclosing"),
            enclosing_path: enclosing,
            dialect: Dialect::CommonLisp,
            name: SymbolName::new("compute").expect("name"),
            explicit_params: Vec::new(),
            infer_params: true,
            recursive: false,
        })
        .expect_err("foreign enclosing selection");

        assert!(error.to_string().contains("does not match the source"));
    }
}
