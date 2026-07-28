//! Reference collection for Scheme and Racket binding forms.
//!
//! Before this module existed, a Scheme form reached the reference query only
//! if `CommonLispOperator::from_head` happened to recognise its head. That is
//! true of `let`, `let*` and `lambda` and of nothing else Scheme binds with:
//! `letrec`, `let-values`, `do`, `case-lambda`, `guard` and `parameterize` all
//! fell through to the generic call walk, which descends into every child in
//! the enclosing scope. The effect was silent and one-directional -- a name
//! shadowed by a `letrec` still collected the outer binding's references, so a
//! rename would rewrite an occurrence that belonged to a different variable.
//!
//! Every function here answers the same question the Common Lisp handlers do:
//! *given a symbol, which spans inside this form are references to the binding
//! visible outside it?* Descending stops where the form rebinds the name.

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::scheme::{
    SchemeBindingForm, SchemeDefineTarget, SchemeDefinitionForm, SchemeLetKind, SchemeOperator,
    scheme_define_target, scheme_formal_defaults_in, scheme_formals_in, scheme_identifier_text,
};
use paredit_core_syntax::sexpr::{
    ByteSpan, Delimiter, ExpressionKind, ExpressionView, SymbolName,
};

use super::super::body::collect_body_forms;
use super::super::lambda_lists::collect_lambda_list_references_from;
use super::super::{collect_unshadowed_symbol_references_in_context, symbol_name_matches};

/// One entry of a Scheme binding list: `(name value)` or `[name value]`.
struct SchemeBinding<'a> {
    /// The nodes that bind. A `let` entry has exactly one; a `let-values`
    /// entry has as many as its formals list names.
    names: Vec<String>,
    value: Option<&'a ExpressionView>,
}

/// Dispatches a Scheme form to its handler.
///
/// Returns whether the form was consumed. `false` means "not a Scheme binding
/// or definition form", and the caller falls back to the generic call walk --
/// which is the right treatment for `begin`, `if`, `cond` and every ordinary
/// procedure call.
pub(super) fn collect_scheme_special_form(
    dialect: Dialect,
    view: &ExpressionView,
    head: &str,
    symbol: &SymbolName,
    input: &str,
    output: &mut Vec<ByteSpan>,
) -> bool {
    let Some(operator) = SchemeOperator::from_head(head) else {
        return false;
    };

    if let Some(binding) = operator.binding_form() {
        return collect_binding_form(dialect, view, binding, symbol, input, output);
    }

    if let Some(definition) = operator.definition_form() {
        return collect_definition_form(dialect, view, definition, symbol, input, output);
    }

    false
}

fn collect_binding_form(
    dialect: Dialect,
    view: &ExpressionView,
    binding: SchemeBindingForm,
    symbol: &SymbolName,
    input: &str,
    output: &mut Vec<ByteSpan>,
) -> bool {
    match binding {
        SchemeBindingForm::Let { kind, .. } => {
            // `(let loop ((i 0)) ...)` and `(let ((i 0)) ...)` share a head and
            // are told apart only by whether child 1 is a symbol. R7RS names
            // only `let` this way, but implementations extend it to `let*` and
            // `letrec`, so the test is on shape rather than on the head.
            if is_named_let(view) {
                collect_named_let(dialect, view, kind, symbol, input, output);
            } else {
                collect_let(dialect, view, kind, symbol, input, output);
            }
            true
        }
        SchemeBindingForm::NamedLet => {
            collect_named_let(dialect, view, SchemeLetKind::Parallel, symbol, input, output);
            true
        }
        SchemeBindingForm::LetValues(kind) => {
            collect_let_values(dialect, view, kind, symbol, input, output);
            true
        }
        SchemeBindingForm::Do => {
            collect_do(dialect, view, symbol, input, output);
            true
        }
        SchemeBindingForm::Lambda => collect_lambda(dialect, view, 1, symbol, input, output),
        SchemeBindingForm::CaseLambda => {
            collect_case_lambda(dialect, view, symbol, input, output);
            true
        }
        SchemeBindingForm::Guard => {
            collect_guard(dialect, view, symbol, input, output);
            true
        }
        SchemeBindingForm::DynamicBinding => {
            collect_dynamic_binding(dialect, view, symbol, input, output);
            true
        }
    }
}

/// `let`, `let*`, `letrec`, `letrec*`, `let-syntax` and `letrec-syntax`.
fn collect_let(
    dialect: Dialect,
    view: &ExpressionView,
    kind: SchemeLetKind,
    symbol: &SymbolName,
    input: &str,
    output: &mut Vec<ByteSpan>,
) {
    let Some(binding_form) = view.children.get(1) else {
        return;
    };
    let Some(bindings) = scheme_binding_groups(binding_form) else {
        return;
    };

    let body = view.children.get(2..).unwrap_or_default();
    collect_binding_list(dialect, &bindings, kind, false, body, symbol, input, output);
}

/// The shared body of every `let` flavour, parameterised by visibility.
///
/// `name_shadows` covers the named-`let` loop variable, which is bound over
/// the body but not over the initializers.
#[allow(clippy::too_many_arguments)]
fn collect_binding_list(
    dialect: Dialect,
    bindings: &[SchemeBinding<'_>],
    kind: SchemeLetKind,
    name_shadows: bool,
    body: &[ExpressionView],
    symbol: &SymbolName,
    input: &str,
    output: &mut Vec<ByteSpan>,
) {
    let binds = |binding: &SchemeBinding<'_>| {
        binding
            .names
            .iter()
            .any(|name| symbol_name_matches(dialect, name, symbol.as_str()))
    };

    match kind {
        // Every initializer is evaluated before any name exists.
        SchemeLetKind::Parallel => {
            for binding in bindings {
                collect_optional(dialect, binding.value, symbol, input, output);
            }
            if bindings.iter().any(binds) {
                return;
            }
        }
        // Each initializer sees the names bound before it, so descent stops at
        // the first entry that rebinds the symbol.
        SchemeLetKind::Sequential => {
            for binding in bindings {
                collect_optional(dialect, binding.value, symbol, input, output);
                if binds(binding) {
                    return;
                }
            }
        }
        // Every name is visible in every initializer, including its own and
        // those textually before it -- that is what makes a group of mutually
        // recursive procedures work. So one entry binding the symbol shadows
        // the whole form, initializers included.
        SchemeLetKind::Recursive => {
            if bindings.iter().any(binds) {
                return;
            }
            for binding in bindings {
                collect_optional(dialect, binding.value, symbol, input, output);
            }
        }
    }

    if name_shadows {
        return;
    }

    collect_body_forms(dialect, body, symbol, input, output);
}

/// `(let name ((var init) ...) body ...)`.
///
/// The loop name is bound over the body but not over the initializers, which
/// are evaluated in the enclosing scope exactly as a plain `let`'s are.
fn collect_named_let(
    dialect: Dialect,
    view: &ExpressionView,
    kind: SchemeLetKind,
    symbol: &SymbolName,
    input: &str,
    output: &mut Vec<ByteSpan>,
) {
    let Some(loop_name) = view.children.get(1) else {
        return;
    };
    let Some(binding_form) = view.children.get(2) else {
        return;
    };
    let Some(bindings) = scheme_binding_groups(binding_form) else {
        return;
    };

    let name_shadows = scheme_identifier_text(loop_name)
        .is_some_and(|name| symbol_name_matches(dialect, name, symbol.as_str()));
    let body = view.children.get(3..).unwrap_or_default();

    collect_binding_list(
        dialect,
        &bindings,
        kind,
        name_shadows,
        body,
        symbol,
        input,
        output,
    );
}

/// `(let-values (((a b) producer) ...) body ...)` and its `let*`/`letrec` kin.
///
/// The only structural difference from `collect_let` is that the bound
/// position is a whole formals list rather than one name.
fn collect_let_values(
    dialect: Dialect,
    view: &ExpressionView,
    kind: SchemeLetKind,
    symbol: &SymbolName,
    input: &str,
    output: &mut Vec<ByteSpan>,
) {
    let Some(binding_form) = view.children.get(1) else {
        return;
    };
    if !is_binding_container(binding_form) {
        return;
    }

    let bindings: Vec<SchemeBinding<'_>> = binding_form
        .children
        .iter()
        .filter(|entry| is_binding_container(entry))
        .map(|entry| SchemeBinding {
            names: entry
                .children
                .first()
                .map(formals_names)
                .unwrap_or_default(),
            value: entry.children.get(1),
        })
        .collect();

    let body = view.children.get(2..).unwrap_or_default();
    collect_binding_list(dialect, &bindings, kind, false, body, symbol, input, output);
}

/// `(do ((var init step) ...) (test result ...) body ...)`.
///
/// Initializers are evaluated in the enclosing scope; the step forms, the
/// termination test and the result forms all run with every loop variable
/// bound, which puts them on the same side of the shadowing boundary as the
/// body.
fn collect_do(
    dialect: Dialect,
    view: &ExpressionView,
    symbol: &SymbolName,
    input: &str,
    output: &mut Vec<ByteSpan>,
) {
    let Some(binding_form) = view.children.get(1) else {
        return;
    };
    if !is_binding_container(binding_form) {
        return;
    }

    for spec in &binding_form.children {
        collect_optional(dialect, spec.children.get(1), symbol, input, output);
    }

    let shadowed = binding_form.children.iter().any(|spec| {
        specification_name(spec).is_some_and(|name| symbol_name_matches(dialect, name, symbol.as_str()))
    });
    if shadowed {
        return;
    }

    for spec in &binding_form.children {
        collect_optional(dialect, spec.children.get(2), symbol, input, output);
    }

    // Children 2.. are the test/result clause followed by the body; all are
    // evaluated inside the loop's scope, so one walk covers them.
    collect_body_forms(
        dialect,
        view.children.get(2..).unwrap_or_default(),
        symbol,
        input,
        output,
    );
}

/// `(lambda formals body ...)`.
fn collect_lambda(
    dialect: Dialect,
    view: &ExpressionView,
    formals_index: usize,
    symbol: &SymbolName,
    input: &str,
    output: &mut Vec<ByteSpan>,
) -> bool {
    let Some(formals) = view.children.get(formals_index) else {
        return false;
    };
    let body = view.children.get(formals_index + 1..).unwrap_or_default();

    // `(lambda args body)` collects every argument into one binding.
    if formals.kind == ExpressionKind::Atom {
        let binds = scheme_identifier_text(formals)
            .is_some_and(|name| symbol_name_matches(dialect, name, symbol.as_str()));
        if !binds {
            collect_body_forms(dialect, body, symbol, input, output);
        }
        return true;
    }

    collect_lambda_list_references_from(dialect, formals, 0, body, symbol, input, output)
}

/// `(case-lambda (formals body ...) ...)`: each clause is its own scope.
fn collect_case_lambda(
    dialect: Dialect,
    view: &ExpressionView,
    symbol: &SymbolName,
    input: &str,
    output: &mut Vec<ByteSpan>,
) {
    for clause in view.children.iter().skip(1) {
        if !is_binding_container(clause) {
            continue;
        }
        collect_lambda(dialect, clause, 0, symbol, input, output);
    }
}

/// `(guard (var clause ...) body ...)`.
///
/// The guarded body is evaluated *before* the handler exists, so it sits in
/// the enclosing scope. Only the clauses see `var`.
fn collect_guard(
    dialect: Dialect,
    view: &ExpressionView,
    symbol: &SymbolName,
    input: &str,
    output: &mut Vec<ByteSpan>,
) {
    let Some(handler) = view.children.get(1) else {
        return;
    };

    collect_body_forms(
        dialect,
        view.children.get(2..).unwrap_or_default(),
        symbol,
        input,
        output,
    );

    if !is_binding_container(handler) {
        return;
    }

    let binds = handler
        .children
        .first()
        .and_then(scheme_identifier_text)
        .is_some_and(|name| symbol_name_matches(dialect, name, symbol.as_str()));
    if binds {
        return;
    }

    collect_body_forms(
        dialect,
        handler.children.get(1..).unwrap_or_default(),
        symbol,
        input,
        output,
    );
}

/// `(parameterize ((param value) ...) body ...)` and `fluid-let`.
///
/// Neither binds a lexical name. The left-hand side of each pair is a
/// *reference* to an existing parameter object or variable, so both halves are
/// ordinary expressions in the enclosing scope -- and so is the body. Reading
/// these pairs as a binding list would wrongly stop the walk at the body.
fn collect_dynamic_binding(
    dialect: Dialect,
    view: &ExpressionView,
    symbol: &SymbolName,
    input: &str,
    output: &mut Vec<ByteSpan>,
) {
    if let Some(binding_form) = view.children.get(1) {
        if is_binding_container(binding_form) {
            for entry in &binding_form.children {
                collect_unshadowed_symbol_references_in_context(
                    dialect, entry, symbol, input, output, 0,
                );
            }
        }
    }

    collect_body_forms(
        dialect,
        view.children.get(2..).unwrap_or_default(),
        symbol,
        input,
        output,
    );
}

fn collect_definition_form(
    dialect: Dialect,
    view: &ExpressionView,
    definition: SchemeDefinitionForm,
    symbol: &SymbolName,
    input: &str,
    output: &mut Vec<ByteSpan>,
) -> bool {
    match definition {
        SchemeDefinitionForm::Define | SchemeDefinitionForm::DefineContract => {
            collect_define(dialect, view, symbol, input, output)
        }
        SchemeDefinitionForm::DefineValues => {
            // `(define-values formals producer)`: the producer runs in the
            // enclosing scope, and the formals are definition sites.
            collect_optional(dialect, view.children.get(2), symbol, input, output);
            true
        }
        // A record, struct or library declaration is made entirely of names --
        // constructor, predicate, fields, accessors, import sets. None of them
        // is a variable reference, so descending would invent one.
        SchemeDefinitionForm::DefineRecordType
        | SchemeDefinitionForm::Struct
        | SchemeDefinitionForm::DefineStruct => true,
        // A macro transformer is code the expander runs, not code this file
        // evaluates. Common Lisp's `define-compiler-macro` is skipped for the
        // same reason.
        SchemeDefinitionForm::DefineSyntax | SchemeDefinitionForm::DefineSyntaxRule => true,
        // A library's body does evaluate, but only inside its `begin`
        // declarations; the generic walk reaches those correctly.
        SchemeDefinitionForm::DefineLibrary => false,
    }
}

/// `(define name value)` and `(define (name . formals) body ...)`.
fn collect_define(
    dialect: Dialect,
    view: &ExpressionView,
    symbol: &SymbolName,
    input: &str,
    output: &mut Vec<ByteSpan>,
) -> bool {
    let Some(target) = view.children.get(1).and_then(scheme_define_target) else {
        return false;
    };

    let body = view.children.get(2..).unwrap_or_default();

    match target {
        // The value is an ordinary expression, and the name is a definition
        // site rather than a reference to anything outside.
        SchemeDefineTarget::Variable { .. } => {
            collect_body_forms(dialect, body, symbol, input, output);
        }
        SchemeDefineTarget::Procedure { formals, .. } => {
            // A curried `(define ((f a) b) ...)` binds every level's
            // parameters over the one body.
            let parameters: Vec<ExpressionView> = formals
                .iter()
                .flat_map(|level| level.children.iter().skip(1))
                .cloned()
                .collect();

            for default_form in scheme_formal_defaults_in(&parameters) {
                collect_unshadowed_symbol_references_in_context(
                    dialect,
                    default_form,
                    symbol,
                    input,
                    output,
                    0,
                );
            }

            let shadowed = scheme_formals_in(&parameters)
                .iter()
                .any(|formal| symbol_name_matches(dialect, &formal.name, symbol.as_str()));
            if !shadowed {
                collect_body_forms(dialect, body, symbol, input, output);
            }
        }
    }

    true
}

fn collect_optional(
    dialect: Dialect,
    form: Option<&ExpressionView>,
    symbol: &SymbolName,
    input: &str,
    output: &mut Vec<ByteSpan>,
) {
    if let Some(form) = form {
        collect_unshadowed_symbol_references_in_context(dialect, form, symbol, input, output, 0);
    }
}

/// Whether `(let ...)` is the named variant.
fn is_named_let(view: &ExpressionView) -> bool {
    view.children
        .get(1)
        .is_some_and(|child| scheme_identifier_text(child).is_some())
}

/// Reads a `((name value) ...)` binding list.
///
/// Accepts bracketed entries as well as parenthesised ones: `(let ([x 1]) x)`
/// is the dominant spelling in Racket and common everywhere else.
fn scheme_binding_groups(binding_form: &ExpressionView) -> Option<Vec<SchemeBinding<'_>>> {
    if !is_binding_container(binding_form) {
        return None;
    }

    binding_form
        .children
        .iter()
        .map(|entry| {
            // `(let (x) ...)` -- a name with no initializer.
            if let Some(name) = scheme_identifier_text(entry) {
                return Some(SchemeBinding {
                    names: vec![name.to_owned()],
                    value: None,
                });
            }
            if !is_binding_container(entry) {
                return None;
            }
            let name = entry.children.first().and_then(scheme_identifier_text)?;
            Some(SchemeBinding {
                names: vec![name.to_owned()],
                value: entry.children.get(1),
            })
        })
        .collect()
}

fn formals_names(formals: &ExpressionView) -> Vec<String> {
    if let Some(name) = scheme_identifier_text(formals) {
        return vec![name.to_owned()];
    }
    scheme_formals_in(&formals.children)
        .into_iter()
        .map(|formal| formal.name)
        .collect()
}

/// The name a `do` variable specification binds: `x` or `(x init step)`.
fn specification_name(spec: &ExpressionView) -> Option<&str> {
    scheme_identifier_text(spec)
        .or_else(|| spec.children.first().and_then(scheme_identifier_text))
}

/// Whether a node can hold binding entries.
///
/// Both delimiters are accepted throughout: R6RS makes them interchangeable
/// and Racket code uses brackets by convention.
fn is_binding_container(view: &ExpressionView) -> bool {
    view.kind == ExpressionKind::List
        && matches!(view.delimiter, Some(Delimiter::Paren | Delimiter::Bracket))
        && view.reader_prefixes.is_empty()
}
