//! What the macro-authoring rules share: whether a matched form is *code*, and
//! how to read a macro lambda list.
//!
//! Every rule here declares [`HeadFilter::Heads`], so the engine hands it one
//! matched form and no context whatsoever. Two things follow, and this module
//! is both:
//!
//! - **[`is_unevaluated_at`]** answers what the matched form cannot answer
//!   about itself. `RuleContext` carries no parent pointer and no depth, so a
//!   rule keyed on `defmacro` is also handed the `(defmacro …)` inside
//!   `` `(progn (defmacro …)) `` — which is a *template*, not a definition, and
//!   whose lambda list is very often deliberately not a lambda list at all.
//!   This is the single largest false-positive source for this package, because
//!   macro-authoring code is by construction the code that contains templates.
//! - **[`MacroLambdaList`]** reads the one shape three rules here need, once.
//!
//! # Cost
//!
//! Nothing in this module runs unless the engine's head index already matched
//! one of this package's anchor heads. Within a file that does, **the ordering
//! is load-bearing: every rule performs its cheap, local, allocation-free
//! domain check before calling [`is_unevaluated_at`]**, which descends from the
//! root and therefore materializes [`SyntaxTree::root_view`]. Sibling packages
//! have measured 450843 and 8589447 ns/invocation from exactly the opposite
//! ordering. `crate::cost_tests` is where that claim is checked rather than
//! asserted.

use paredit_core_syntax::sexpr::reader::atom_symbol_text;
use paredit_core_syntax::sexpr::{
    ByteSpan, ExpressionView, Path as SexprPath, ReaderPrefix, Selection, SyntaxTree,
};
use paredit_core_syntax::view_query::{
    atom_text, is_paren_list, list_head, symbol_in, symbol_is, unqualified,
};

/// How much of the surrounding reader syntax says "this is data".
///
/// Two independent counters, because `'` and `` ` `` are **not** the same thing
/// and a single `i32` depth counter cannot tell them apart. A comma is an
/// escape back to code only when a backquote opened one: inside `` `(a ,x) ``
/// the `,x` is evaluated, but inside `'(a ,x)` there is no backquote for the
/// comma to escape and what it reads as is either literal list structure or a
/// reader error ("comma not inside a backquote"). In neither case is it code,
/// so `hard` never clears and only `quasi` counts down.
///
/// The same model — deliberately, down to the field names — as
/// `paredit-feature-lint-condition-system`'s, `-lint-object-system`'s and
/// `-lint-data-structure`'s. This package needs it more than any of them: a
/// macro body is mostly template, so a rule that gets this wrong does not
/// misfire occasionally, it misfires on every macro in the file. A single
/// depth counter cannot represent both states; treating every reader prefix as
/// unevaluated also misses `,@` escapes inside templates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct QuoteState {
    hard: bool,
    quasi: u32,
}

impl QuoteState {
    const EVALUATED: Self = Self {
        hard: false,
        quasi: 0,
    };

    const fn is_data(self) -> bool {
        self.hard || self.quasi > 0
    }

    /// The state inside a node, given the state outside it and the node's own
    /// reader prefixes.
    ///
    /// `#'`, `#.`, metadata and the rest are deliberately neutral: none of them
    /// turns code into data. Common Lisp reader conditionals are not prefixes
    /// in this tree at all — `#+sbcl (form)` folds into a *single atom* whose
    /// text includes the `#+sbcl` — so there is nothing here to misread.
    fn after_prefixes(mut self, prefixes: &[ReaderPrefix]) -> Self {
        for prefix in prefixes {
            match prefix {
                ReaderPrefix::Quote => self.hard = true,
                ReaderPrefix::Quasiquote => self.quasi += 1,
                ReaderPrefix::Unquote | ReaderPrefix::UnquoteSplicing => {
                    self.quasi = self.quasi.saturating_sub(1);
                }
                _ => {}
            }
        }
        self
    }

    const fn quoted(mut self) -> Self {
        self.hard = true;
        self
    }
}

/// Whether stepping into operand `index` of the form at `path` steps into a
/// spelled-out `(quote …)`, which is `'…` with letters instead of punctuation.
fn spelled_quote_step(
    tree: &SyntaxTree,
    parent: &SexprPath,
    index: usize,
    state: QuoteState,
) -> QuoteState {
    if index == 0 {
        return state;
    }
    let head = tree
        .select_path(parent)
        .ok()
        .and_then(Selection::head)
        .unwrap_or_default();
    let mut next = state;
    if symbol_is(head, "quote") {
        next.hard = true;
    } else if symbol_is(head, "quasiquote") || symbol_is(head, "backquote") {
        next.quasi += 1;
    }
    next
}

/// The child of `parent` (or of the virtual root) that contains `span`, with
/// the path that reaches it.
///
/// Siblings are in source order, so a child starting past the target ends the
/// search: the remaining ones cannot contain it either.
///
/// The root case reads [`SyntaxTree::root_child_span`], which is a slice index
/// and a field read; the `select_path` spelling would allocate an
/// [`SexprPath`]'s `Vec` per sibling scanned.
fn child_containing<'tree>(
    tree: &'tree SyntaxTree,
    parent: Option<&SexprPath>,
    span: ByteSpan,
) -> Option<(usize, SexprPath, ByteSpan, &'tree [ReaderPrefix])> {
    let mut index = 0usize;
    loop {
        let (child_span, prefixes) = match parent {
            None => (
                tree.root_child_span(index)?,
                tree.root_child_reader_prefixes(index)?,
            ),
            Some(path) => {
                let selection = tree.select_path(&path.child(index)).ok()?;
                (selection.span(), selection.reader_prefixes())
            }
        };
        if child_span.start().get() > span.start().get() {
            return None;
        }
        if span.end().get() <= child_span.end().get() {
            let child =
                parent.map_or_else(|| SexprPath::root_child(index), |path| path.child(index));
            return Some((index, child, child_span, prefixes));
        }
        index += 1;
    }
}

/// The long-hand `(quote …)`, which the reader also produces for `'…` but which
/// hand-written code and macro output both spell out.
fn is_quote_form(view: &ExpressionView) -> bool {
    list_head(view).is_some_and(|head| symbol_is(head, "quote"))
}

/// Whether the node at `target` is unevaluated data rather than code.
///
/// Descends from the root through the one child at each level whose span
/// contains `target`, so the cost is the node's depth, not the file's size.
///
/// The verdict is read *at* the target and nowhere shallower. An ancestor being
/// data does not settle it: `` `(a ,(defmacro …)) `` has a quasiquoted ancestor
/// and an evaluated target. Being inside a hard `'` does settle it, and that is
/// already modelled by `hard` never clearing.
///
/// The root's own span is never consulted. A file with one top-level form has a
/// root whose span equals that form's, and comparing them would call every such
/// form evaluated before looking at its prefixes at all.
#[must_use]
pub fn is_unevaluated_at(tree: &SyntaxTree, target: ByteSpan) -> bool {
    descend_to(tree, target, |_, _, _| {}).is_none_or(|state| state.is_data())
}

/// Descends from the root to `target`, accumulating quote state, and calls
/// `visit` for each strict ancestor with `(path, head-index, child-index)`.
///
/// **No subtree is materialized.** The descent reads spans through
/// [`SyntaxTree::root_child_span`] and [`SyntaxTree::select_path`], and reader
/// prefixes off the source text, so its cost is the node's depth times its
/// siblings rather than the size of the document.
///
/// That distinction is not a micro-optimization. The first version of this
/// module descended over `tree.root_view()`, which materializes a `Vec` per
/// node and a `String` per atom for the *whole file*, once per call. On a
/// 250-macro file where every macro reported, `crate::cost_tests` measured
///
/// ```text
/// destruct n=250   macro-body-destroys-argument-form   6748181ns/inv  (1.69s total)
/// capture  n=250   macrolet-expander-captures-…       11780384ns/inv  (2.95s total)
/// ```
///
/// — per-invocation costs that *grew with the file*, which is a quadratic rule.
/// Sibling packages have measured 450843 and 8589447 ns/call from the same
/// mistake. `None` when no node has exactly `target`'s span.
fn descend_to(
    tree: &SyntaxTree,
    target: ByteSpan,
    mut visit: impl FnMut(&SexprPath, usize, ByteSpan),
) -> Option<QuoteState> {
    let mut parent: Option<SexprPath> = None;
    let mut state = QuoteState::EVALUATED;

    loop {
        let (index, child_path, child_span, prefixes) =
            child_containing(tree, parent.as_ref(), target)?;
        if let Some(parent_path) = &parent {
            state = spelled_quote_step(tree, parent_path, index, state);
            visit(parent_path, index, child_span);
        }
        state = state.after_prefixes(prefixes);
        if child_span == target {
            return Some(state);
        }
        parent = Some(child_path);
    }
}

/// Calls `visit` on every node of `root` reachable as **evaluated code**, in
/// pre-order, starting from `root` itself being evaluated.
///
/// Quoted subtrees are still *descended* — `` `(a ,(f)) `` has code inside data
/// — but their data nodes are never visited. This is the whole discriminator
/// for `macrolet-expander-captures-lexical-variable`: a name written plainly in
/// a template is part of the *expansion* and is bound wherever the expansion
/// lands, while the same name under a comma is read by the expander itself, at
/// expansion time, out of the enclosing lexical environment. One is the
/// commonest macrolet idiom there is; the other is CLHS's undefined
/// consequences. A single depth counter cannot tell them apart.
pub fn for_each_evaluated_subview(root: &ExpressionView, mut visit: impl FnMut(&ExpressionView)) {
    // The third component says "this node is a clause whose first element is a
    // key list or a type specifier, and so is data".
    let mut stack = vec![(root, QuoteState::EVALUATED, false)];
    while let Some((view, outer, clause)) = stack.pop() {
        let state = outer.after_prefixes(&view.reader_prefixes);
        if !state.is_data() {
            visit(view);
        }
        let inside = if is_quote_form(view) {
            state.quoted()
        } else {
            state
        };
        let clauses_from = selector_form_clauses_from(view);
        for (index, child) in view.children.iter().enumerate().rev() {
            // A `(case x ((index start) 'start))` key list is unevaluated, and
            // nothing marks it as such: there is no quote character anywhere.
            // Reading `start` there as a variable reference is what produced
            // two of the four findings in this package's SBCL audit.
            let child_state = if clause && index == 0 {
                inside.quoted()
            } else {
                inside
            };
            let child_is_clause = clauses_from.is_some_and(|from| index >= from);
            stack.push((child, child_state, child_is_clause));
        }
    }
}

/// The child index at which a selector form's clauses start, for the forms
/// whose clauses lead with something the reader never evaluates.
///
/// `case`/`ecase`/`ccase` lead with a **key list**, the `typecase` family with a
/// **type specifier**, and `handler-case`/`restart-case` with a condition type
/// or restart name. None of them is quoted, and all of them are data.
fn selector_form_clauses_from(view: &ExpressionView) -> Option<usize> {
    let head = list_head(view)?;
    match unqualified(head).to_ascii_lowercase().as_str() {
        "case" | "ecase" | "ccase" | "typecase" | "etypecase" | "ctypecase" | "handler-case"
        | "restart-case" => Some(2),
        _ => None,
    }
}

/// The names a `loop` binds, appended to `out`.
///
/// `loop`'s binding positions are keyword-directed rather than positional,
/// which is why it is absent from [`binder_shape`] — a rule must not *report*
/// on a name whose binder it cannot read exactly. Shadowing is the opposite
/// direction: a `loop` variable this misses becomes a false positive, and one
/// it over-collects merely costs a finding. So it is read here, conservatively,
/// and used only to suppress.
///
/// This was not hypothetical. SBCL `src/code/type.lisp:2810` writes
/// `(loop for (class format coerce simple-coerce) in specs …)` inside a
/// `macrolet` expander that sits in a `defun` with a parameter also called
/// `format`; without this, that reads as a capture of the parameter.
fn loop_names(view: &ExpressionView, out: &mut Vec<String>) {
    let mut expecting = false;
    for child in &view.children {
        if expecting {
            expecting = false;
            if is_paren_list(child) {
                // A destructuring template: every symbol in it is bound.
                let mut stack = vec![child];
                while let Some(node) = stack.pop() {
                    for element in &node.children {
                        if is_paren_list(element) {
                            stack.push(element);
                        } else if let Some(name) = variable_name(element) {
                            out.push(name);
                        }
                    }
                }
                continue;
            }
            if let Some(name) = variable_name(child) {
                out.push(name);
            }
            continue;
        }
        expecting = atom_text(child).is_some_and(|text| {
            matches!(
                unqualified(text).to_ascii_lowercase().as_str(),
                "for" | "as" | "with" | "into"
            )
        });
    }
}

/// An atom's symbol text, past any reader prefix, lowercased and stripped of
/// its package qualifier — the spelling every comparison here is written in.
#[must_use]
pub fn normalized_symbol(text: &str) -> String {
    unqualified(text).to_ascii_lowercase()
}

/// The symbol an atom names, in the normalized spelling.
#[must_use]
pub fn symbol_name(view: &ExpressionView) -> Option<String> {
    atom_symbol_text(view)
        .filter(|text| !text.is_empty())
        .map(normalized_symbol)
}

/// Whether an atom is a lambda-list marker: a symbol starting `&` with at least
/// one character after it.
///
/// A folded reader conditional is rejected here — `#+sbcl &body` is a *single*
/// atom whose text starts with `#`, not with `&` — which is what keeps a
/// conditional marker from being read at the wrong index.
///
/// [`atom_text`] and [`atom_symbol_text`] agree on every input this sees, since
/// `#+` is not a reader prefix in this tree and a lambda-list marker never
/// carries one. Mutation testing confirmed the two spellings are
/// interchangeable here, so the choice is **not** load-bearing and this comment
/// no longer claims it is.
#[must_use]
pub fn marker_text(view: &ExpressionView) -> Option<String> {
    let text = atom_text(view)?;
    if !text.starts_with('&') || text.len() < 2 {
        return None;
    }
    Some(text.to_ascii_lowercase())
}

/// The binding forms this package models, and where each keeps the names it
/// binds.
///
/// Deliberately short, and short in the *safe* direction. Every rule here
/// reports a name only when it appears in this table, so a binder left out
/// costs a missed finding while a binder read wrongly costs a false positive.
/// `loop`'s `with`/`for` clauses, `with-slots`, `with-accessors` and the
/// `symbol-macrolet` family are all omitted for that reason: their binding
/// positions are keyword-directed rather than positional, and half-reading them
/// would report the *iterated sequence* as a bound name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BinderShape {
    /// `(let ((name value) …) body…)` — child 1 is a list of bindings, each a
    /// bare name or a `(name …)` list. The body starts at child 2.
    BindingList,
    /// `(do ((name init step) …) …)` — same binding list shape, different body
    /// offset, which does not matter here.
    DoBindingList,
    /// `(multiple-value-bind (name …) values body…)` — child 1 is a flat list
    /// of bare names.
    NameList,
    /// `(lambda (lambda-list) body…)` — child 1 is a lambda list.
    LambdaListAt1,
    /// `(defun name (lambda-list) body…)` — child 2 is a lambda list.
    LambdaListAt2,
    /// `(dolist (name sequence) body…)` — child 1 is a list whose *first*
    /// element is the name.
    SingleBinding,
}

impl BinderShape {
    /// The child index the binder keeps its names at.
    const fn names_at(self) -> usize {
        match self {
            Self::BindingList
            | Self::DoBindingList
            | Self::NameList
            | Self::LambdaListAt1
            | Self::SingleBinding => 1,
            Self::LambdaListAt2 => 2,
        }
    }

    /// The first child index that is *body*, and so is in the scope of the
    /// names. Everything before it is not: a `let` init form is evaluated
    /// before the binding exists.
    const fn body_from(self) -> usize {
        self.names_at() + 1
    }
}

fn binder_shape(head: &str) -> Option<BinderShape> {
    match unqualified(head).to_ascii_lowercase().as_str() {
        "let" | "let*" | "prog" | "prog*" => Some(BinderShape::BindingList),
        "do" | "do*" => Some(BinderShape::DoBindingList),
        "multiple-value-bind" => Some(BinderShape::NameList),
        "lambda" => Some(BinderShape::LambdaListAt1),
        "defun" | "defmacro" | "define-compiler-macro" => Some(BinderShape::LambdaListAt2),
        "destructuring-bind" => Some(BinderShape::LambdaListAt1),
        "dolist" | "dotimes" => Some(BinderShape::SingleBinding),
        _ => None,
    }
}

/// The name an atom introduces as a *variable*, or `None` when it cannot be
/// one.
///
/// A lambda list and a `let` binding list both carry atoms that are not
/// variable names — the `1` of `(b 1)`, the `:key` of a keyword entry, the
/// `t`/`nil` constants, string and number literals. Collecting those as bound
/// names is how a scope set acquires entries that then match a reference and
/// produce a finding about a binding that does not exist.
/// Read past the reader prefixes, deliberately.
///
/// `,n` inside a template **is** a reference to `n`; it is in fact the only
/// kind of reference `macrolet-expander-captures-lexical-variable` reports, so
/// reading the name off [`atom_text`] — which returns `",n"` — rejected every
/// finding the rule exists for. [`marker_text`] keeps the opposite convention
/// for the opposite reason.
///
/// A folded reader conditional still reads as `"#+sbcl foo"` here, because
/// `#+` is not a reader prefix in this tree at all, and is rejected by the
/// leading `#`.
#[must_use]
pub fn variable_name(view: &ExpressionView) -> Option<String> {
    let text = atom_symbol_text(view)?;
    let first = text.chars().next()?;
    // A number, a string, a character or dispatch literal, a keyword, or a
    // lambda-list marker.
    if first.is_ascii_digit() || matches!(first, '"' | '#' | ':' | '&' | '\'' | '`' | ',') {
        return None;
    }
    if matches!(first, '-' | '+' | '.')
        && text
            .chars()
            .nth(1)
            .is_some_and(|next| next.is_ascii_digit())
    {
        // `-1`, `+2.0` are numbers; `-`, `+foo+` and `.` are names.
        return None;
    }
    let name = normalized_symbol(text);
    if name.is_empty() || name == "t" || name == "nil" {
        return None;
    }
    Some(name)
}

/// The variable names a lambda list binds, appended to `out`.
///
/// A parenthesized entry contributes **only its first element**. That is exact
/// for the `(name default)` and `(name default supplied-p)` entries of
/// `&optional` and `&key`, and it deliberately under-reads a `defmacro`
/// destructuring sublist, whose remaining names are simply not collected. Under
/// -reading costs a missed finding; over-reading — which is what recursing here
/// does, since it also collects the `1` of `(b 1)` — costs a false positive.
fn lambda_list_names(list: &ExpressionView, out: &mut Vec<String>) {
    for child in &list.children {
        let named = if is_paren_list(child) {
            child.children.first()
        } else {
            Some(child)
        };
        if let Some(name) = named.and_then(variable_name) {
            out.push(name);
        }
    }
}

/// The lexical variable names `form` binds over its body, appended to `out`.
///
/// `flet` and `labels` are deliberately absent: they bind *function* names
/// through a `((name (lambda-list) body…) …)` list, which is not any of these
/// shapes, and reading them as one would collect the local functions' own
/// parameters as if the outer body bound them.
fn binder_names(holder: &ExpressionView, shape: BinderShape, out: &mut Vec<String>) {
    match shape {
        BinderShape::BindingList | BinderShape::DoBindingList => {
            let bindings = holder;
            for binding in &bindings.children {
                let named = if is_paren_list(binding) {
                    binding.children.first()
                } else {
                    Some(binding)
                };
                if let Some(name) = named.and_then(variable_name) {
                    out.push(name);
                }
            }
        }
        BinderShape::NameList => {
            out.extend(holder.children.iter().filter_map(variable_name));
        }
        BinderShape::LambdaListAt1 | BinderShape::LambdaListAt2 => {
            lambda_list_names(holder, out);
        }
        BinderShape::SingleBinding => {
            if let Some(name) = holder.children.first().and_then(variable_name) {
                out.push(name);
            }
        }
    }
}

/// The lexical variable names bound by the forms **enclosing** `target`, or
/// `None` when `target` is unevaluated data.
///
/// Descends from the root through the one child at each level whose span
/// contains `target`, so this costs the node's depth rather than the file's
/// size — and it answers the quote question in the same descent, because both
/// callers need both answers and neither wants two walks.
///
/// A binder's names are collected only when `target` sits in its **body**. The
/// `(let ((x (macrolet …))) …)` case is the one this excludes: an init form is
/// evaluated *before* the binding exists, so `x` is not visible there and
/// reporting it would be a false positive. `let*` is treated the same way,
/// which under-reports the later init forms of a `let*` and never over-reports.
///
/// # Cost
///
/// One depth-bounded descent that materializes **only** each enclosing binder's
/// own binding list — a `let`'s `((a 1) (b 2))`, a `defun`'s lambda list — and
/// never the document. **Call it only after a rule's local domain check has
/// already found a candidate**: it still costs a descent, and calling it to
/// decide whether to look is the ordering mistake that turns a 30ns rule into a
/// 400µs one.
///
/// The verdict on data is read at the target and nowhere shallower. An ancestor
/// being data does not settle it: `` `(list ,(macrolet …)) `` has a quasiquoted
/// ancestor and an evaluated target, and bailing at the ancestor would make
/// every escape inside a template unreachable. A hard `'` does settle it,
/// because `hard` never clears.
#[must_use]
pub fn enclosing_lexical_names(tree: &SyntaxTree, target: ByteSpan) -> Option<Vec<String>> {
    let mut names = Vec::new();
    let state = descend_to(tree, target, |path, index, _| {
        let Some(head) = tree.select_path(path).ok().and_then(Selection::head) else {
            return;
        };
        let Some(shape) = binder_shape(head) else {
            return;
        };
        // The ancestor's bindings are in scope over its body only. Reaching the
        // target through a child before the body means the binding does not
        // exist there yet: a `let` init form runs before its own variable is
        // bound.
        if index < shape.body_from() {
            return;
        }
        // Only the binding list itself is materialized, not the ancestor's body.
        let Ok(selection) = tree.select_path(&path.child(shape.names_at())) else {
            return;
        };
        let holder = selection.view();
        if is_paren_list(&holder) {
            binder_names(&holder, shape, &mut names);
        }
    })?;
    (!state.is_data()).then_some(names)
}

/// Every name that is rebound or reassigned anywhere in the evaluated part of
/// `root`.
///
/// This is the shadowing guard both reporting rules need, and it is why they
/// have the false-positive rate they do. A parameter that the expander rebinds
/// —
///
/// ```lisp
/// (defmacro m (&body body)
///   (let ((body (copy-list body)))     ; a fresh list from here down
///     `(progn ,@(nreverse body))))     ; destroying it harms nobody
/// ```
///
/// — or reassigns with `setf`/`setq` is no longer the caller's structure, so a
/// destructive call on it is correct code. Neither rule can tell *where* in the
/// body the rebinding happened relative to the destructive call without flow
/// analysis, so a name that is rebound **anywhere** is dropped from
/// consideration entirely. That under-reports a macro which destroys its
/// argument first and copies it later, and never over-reports.
///
/// Only the *evaluated* part is walked: a `let` inside a quasiquote template is
/// part of the expansion, and binds a name in the caller's code rather than in
/// the expander's.
#[must_use]
pub fn names_bound_within(root: &ExpressionView) -> Vec<String> {
    let mut names = Vec::new();
    for_each_evaluated_subview(root, |view| {
        if let Some(shape) = list_head(view).and_then(binder_shape) {
            if let Some(holder) = view
                .children
                .get(shape.names_at())
                .filter(|holder| is_paren_list(holder))
            {
                binder_names(holder, shape, &mut names);
            }
        }
        if list_head(view).is_some_and(|head| symbol_in(head, &["loop", "loop-finish"])) {
            loop_names(view, &mut names);
        }
        // `(setf place value …)` and `(setq name value …)` re-point a name
        // without rebinding it, which has the same effect on what the name
        // designates from that point on.
        if list_head(view).is_some_and(|head| symbol_in(head, &["setf", "setq", "psetf", "psetq"]))
        {
            for place in view.children.iter().skip(1).step_by(2) {
                if let Some(name) = variable_name(place) {
                    names.push(name);
                }
            }
        }
    });
    names.sort_unstable();
    names.dedup();
    names
}

/// One macro lambda list, read once.
///
/// "Read" is deliberately shallow: this records where each **top-level** marker
/// sits and nothing about the destructuring sublists, because every position
/// rule CLHS 3.4.4 states about `&whole`, `&environment` and `&body` is stated
/// about one level of the list at a time. A sublist is its own macro lambda
/// list and is read as one, separately.
#[derive(Debug, Clone)]
pub struct MacroLambdaList {
    /// Each top-level `&`-marker: its lowercased text, its position among the
    /// list's children, and its own span.
    pub markers: Vec<(String, usize, ByteSpan)>,
    /// How many children the list has, so "last" can be decided.
    pub length: usize,
}

impl MacroLambdaList {
    /// Reads the top-level markers of `list`, which must already be a `(…)`.
    #[must_use]
    pub fn read(list: &ExpressionView) -> Self {
        let markers = list
            .children
            .iter()
            .enumerate()
            .filter_map(|(index, child)| marker_text(child).map(|text| (text, index, child.span)))
            .collect();
        Self {
            markers,
            length: list.children.len(),
        }
    }

    /// The position of the named marker, if the list has one.
    #[must_use]
    pub fn position_of(&self, marker: &str) -> Option<usize> {
        self.markers
            .iter()
            .find(|(text, _, _)| text == marker)
            .map(|(_, index, _)| *index)
    }

    /// The span of the named marker, if the list has one.
    #[must_use]
    pub fn span_of(&self, marker: &str) -> Option<ByteSpan> {
        self.markers
            .iter()
            .find(|(text, _, _)| text == marker)
            .map(|(_, _, span)| *span)
    }
}

/// The parameters of a macro lambda list that are **always** bound to structure
/// the caller wrote: the required parameters, the `&rest`/`&body` tail, and the
/// `&whole` form.
///
/// `&optional` and `&key` parameters are deliberately excluded. When the caller
/// omits one, its value comes from the default form — freshly built by the
/// expander — and destroying that harms nobody, so a rule that reported on it
/// would be reporting on a call it cannot see. `&environment` is excluded
/// because it is bound to an environment object rather than to a form.
///
/// Nested destructuring names are not collected: only the first element of a
/// sublist is, matching [`lambda_list_names`], which under-reads rather than
/// inventing names.
#[must_use]
pub fn caller_supplied_parameters(list: &ExpressionView) -> Vec<String> {
    /// What the next non-marker child of the lambda list is.
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Section {
        /// A required parameter: the caller's structure, and admitted.
        Required,
        /// The single variable a `&rest`/`&body`/`&whole` marker names.
        MarkerVariable {
            /// `&environment`'s variable is an environment object rather than a
            /// form, so it is read and discarded.
            admitted: bool,
            /// `&whole` and `&environment` are followed by required parameters;
            /// `&rest`/`&body` are followed only by more markers.
            back_to_required: bool,
        },
        /// `&optional`, `&key`, `&aux`: a parameter that may come from a
        /// default form rather than from the caller.
        Defaultable,
    }

    let mut names = Vec::new();
    let mut section = Section::Required;

    for child in &list.children {
        if let Some(marker) = marker_text(child) {
            section = match marker.as_str() {
                "&whole" => Section::MarkerVariable {
                    admitted: true,
                    back_to_required: true,
                },
                "&environment" => Section::MarkerVariable {
                    admitted: false,
                    back_to_required: true,
                },
                "&rest" | "&body" => Section::MarkerVariable {
                    admitted: true,
                    back_to_required: false,
                },
                _ => Section::Defaultable,
            };
            continue;
        }
        let admitted = match section {
            Section::Required => true,
            Section::MarkerVariable { admitted, .. } => admitted,
            Section::Defaultable => false,
        };
        if let Section::MarkerVariable {
            back_to_required, ..
        } = section
        {
            section = if back_to_required {
                Section::Required
            } else {
                Section::Defaultable
            };
        }
        if !admitted {
            continue;
        }
        let named = if is_paren_list(child) {
            child.children.first()
        } else {
            Some(child)
        };
        if let Some(name) = named.and_then(variable_name) {
            names.push(name);
        }
    }
    names.sort_unstable();
    names.dedup();
    names
}

/// The lambda list of a `(defmacro name (…) …)` or `(define-compiler-macro name
/// (…) …)`, when child 2 is a `(…)` list.
///
/// `None` when it is not — a `defmacro` whose lambda list is missing or is an
/// atom is malformed in a way that is not this package's subject, and guessing
/// at its shape would make every rule depend on the guess.
#[must_use]
pub fn definition_lambda_list(form: &ExpressionView) -> Option<&ExpressionView> {
    let list = form.children.get(2)?;
    is_paren_list(list).then_some(list)
}

/// The name a `(defmacro name …)` or `(define-compiler-macro name …)` defines.
///
/// A `(setf foo)` function name reads as `None`: it is a legal name that this
/// package's correlation cannot key on, and inventing a spelling for it would
/// make two unrelated definitions look like the same one.
#[must_use]
pub fn definition_name(form: &ExpressionView) -> Option<String> {
    form.children.get(1).and_then(symbol_name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;

    fn tree(source: &str) -> SyntaxTree {
        SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse")
    }

    /// Finds the first node whose head is `head`, *including* data nodes, and
    /// asks whether it is unevaluated.
    fn unevaluated_at_first_head(source: &str, head: &str) -> bool {
        let parsed = tree(source);
        let mut span = None;
        paredit_core_syntax::view_query::for_each_subview(&parsed.root_view(), |view| {
            if span.is_none() && list_head(view).is_some_and(|found| found == head) {
                span = Some(view.span);
            }
        });
        is_unevaluated_at(&parsed, span.expect("the head must occur in the source"))
    }

    // ---- the two directions the two-counter model exists to separate -------

    /// A hard-quoted mention must **not** count as evaluated. This is the
    /// direction that produces false positives.
    #[test]
    fn a_hard_quoted_defmacro_is_data() {
        assert!(unevaluated_at_first_head("'(defmacro m (a) a)", "defmacro"));
        assert!(unevaluated_at_first_head(
            "(quote (defmacro m (a) a))",
            "defmacro"
        ));
    }

    /// An unquote at depth 0 must count as evaluated. This is the direction a
    /// single depth counter gets wrong, and the direction that produces false
    /// *negatives* — a rule that never sees inside `,@` misses every finding in
    /// a code-generating macro.
    #[test]
    fn an_unquote_inside_a_quasiquote_is_code_again() {
        assert!(!unevaluated_at_first_head(
            "`(progn ,(defmacro m (a) a))",
            "defmacro"
        ));
        assert!(!unevaluated_at_first_head(
            "`(progn ,@(defmacro m (a) a))",
            "defmacro"
        ));
    }

    /// A plain quasiquote with no escape is data, and a comma under a *hard*
    /// quote does not escape it.
    #[test]
    fn a_quasiquote_without_an_escape_is_data() {
        assert!(unevaluated_at_first_head("`(defmacro m (a) a)", "defmacro"));
    }

    #[test]
    fn a_comma_inside_a_hard_quote_stays_data() {
        assert!(unevaluated_at_first_head(
            "'(progn ,(defmacro m (a) a))",
            "defmacro"
        ));
    }

    /// Nested quasiquotes need the *counter*, not a flag: one comma inside two
    /// backquotes is still data.
    #[test]
    fn one_unquote_does_not_escape_two_quasiquotes() {
        assert!(unevaluated_at_first_head(
            "``(progn ,(defmacro m (a) a))",
            "defmacro"
        ));
    }

    #[test]
    fn two_unquotes_escape_two_quasiquotes() {
        assert!(!unevaluated_at_first_head(
            "``(progn ,,(defmacro m (a) a))",
            "defmacro"
        ));
    }

    #[test]
    fn plain_code_is_evaluated() {
        assert!(!unevaluated_at_first_head("(defmacro m (a) a)", "defmacro"));
        assert!(!unevaluated_at_first_head(
            "(progn (defmacro m (a) a))",
            "defmacro"
        ));
    }

    /// `#'` is a function designator, not a quote: what is under it is code.
    #[test]
    fn a_sharp_quote_is_not_a_quote() {
        assert!(!unevaluated_at_first_head(
            "(mapcar #'(lambda (x) (defmacro m (a) a)) xs)",
            "defmacro"
        ));
    }

    // ---- lambda-list reading ----------------------------------------------

    fn lambda_list_of(source: &str) -> MacroLambdaList {
        let parsed = tree(source);
        let form = &parsed.root_view().children[0];
        MacroLambdaList::read(definition_lambda_list(form).expect("a lambda list"))
    }

    #[test]
    fn markers_are_recorded_with_their_positions() {
        let read = lambda_list_of("(defmacro m (a &optional b &body c) a)");
        assert_eq!(read.position_of("&optional"), Some(1));
        assert_eq!(read.position_of("&body"), Some(3));
        assert_eq!(read.length, 5);
    }

    #[test]
    fn markers_fold_case_the_way_the_reader_does() {
        let read = lambda_list_of("(defmacro m (&WHOLE w a) a)");
        assert_eq!(read.position_of("&whole"), Some(0));
    }

    /// A sublist's markers are not the outer list's. Every position rule is
    /// stated one level at a time, so reading them together would report a
    /// `&whole` that is legally first *in its own sublist*.
    #[test]
    fn a_sublist_marker_is_not_a_top_level_marker() {
        let read = lambda_list_of("(defmacro m ((&whole w a b) c) c)");
        assert_eq!(read.position_of("&whole"), None);
        assert_eq!(read.markers.len(), 0);
    }

    #[test]
    fn a_bare_ampersand_is_not_a_marker() {
        let read = lambda_list_of("(defmacro m (a & b) a)");
        assert_eq!(read.markers.len(), 0);
    }

    #[test]
    fn a_missing_or_atom_lambda_list_reads_as_none() {
        let parsed = tree("(defmacro m)");
        assert!(definition_lambda_list(&parsed.root_view().children[0]).is_none());
        let parsed = tree("(defmacro m nil a)");
        assert!(definition_lambda_list(&parsed.root_view().children[0]).is_none());
    }

    /// A Common Lisp reader conditional folds into a **single atom** whose text
    /// includes the `#+sbcl`, so `#+sbcl &body` is one child spelled
    /// `"#+sbcl &body"` rather than two children. Reading markers off
    /// [`atom_text`] rather than [`atom_symbol_text`] is what makes that a
    /// declined candidate instead of a marker at the wrong index — the
    /// conservative direction, and the one that cannot invent a finding.
    ///
    /// This has made several guards unreachable elsewhere in this repository,
    /// so it is pinned rather than assumed.
    #[test]
    fn a_reader_conditional_folds_into_one_atom_and_is_not_read_as_a_marker() {
        let parsed = tree("(defmacro m (a #+sbcl &body b) a)");
        let form = &parsed.root_view().children[0];
        let list = definition_lambda_list(form).expect("a lambda list");
        assert_eq!(list.children.len(), 3, "the conditional folded with &body");
        assert_eq!(atom_text(&list.children[1]), Some("#+sbcl &body"));

        let read = MacroLambdaList::read(list);
        assert!(
            read.markers.is_empty(),
            "a folded conditional must not read as a marker"
        );
    }

    // ---- enclosing lexical scope -------------------------------------------

    /// The names in scope at the first `macrolet` in `source`.
    fn scope_at_macrolet(source: &str) -> Option<Vec<String>> {
        let parsed = tree(source);
        let mut span = None;
        paredit_core_syntax::view_query::for_each_subview(&parsed.root_view(), |view| {
            if span.is_none() && list_head(view).is_some_and(|head| head == "macrolet") {
                span = Some(view.span);
            }
        });
        let mut names = enclosing_lexical_names(&parsed, span.expect("a macrolet"))?;
        names.sort();
        Some(names)
    }

    #[test]
    fn a_top_level_form_has_no_enclosing_lexical_names() {
        assert_eq!(scope_at_macrolet("(macrolet ((m () 1)) (m))"), Some(vec![]));
    }

    #[test]
    fn a_let_body_sees_the_lets_own_names() {
        assert_eq!(
            scope_at_macrolet("(let ((limit 10) (step 2)) (macrolet ((m () 1)) (m)))"),
            Some(vec!["limit".to_owned(), "step".to_owned()])
        );
    }

    #[test]
    fn a_defuns_body_sees_its_parameters() {
        assert_eq!(
            scope_at_macrolet("(defun f (a &optional (b 1) &key c) (macrolet ((m () 1)) (m)))"),
            Some(vec!["a".to_owned(), "b".to_owned(), "c".to_owned()])
        );
    }

    #[test]
    fn nested_binders_accumulate() {
        assert_eq!(
            scope_at_macrolet(
                "(defun f (a) (let ((b 1)) (dolist (c b) (macrolet ((m () 1)) (m)))))"
            ),
            Some(vec!["a".to_owned(), "b".to_owned(), "c".to_owned()])
        );
    }

    /// An init form is evaluated *before* the binding exists, so `x` is not in
    /// scope there. Collecting it would be a false positive.
    #[test]
    fn a_let_init_form_does_not_see_the_name_being_bound() {
        assert_eq!(
            scope_at_macrolet("(let ((x (macrolet ((m () 1)) (m)))) x)"),
            Some(vec![])
        );
    }

    /// A lambda list is not in the scope of its own names either.
    #[test]
    fn a_lambda_list_is_not_in_the_scope_it_introduces() {
        assert_eq!(
            scope_at_macrolet("(defun f (a (b (macrolet ((m () 1)) (m)))) a)"),
            Some(vec![])
        );
    }

    /// `flet` binds *functions* through a shape none of these binders match;
    /// reading it as a lambda list would collect the local function's own
    /// parameters as if the outer body bound them.
    #[test]
    fn flet_contributes_no_variable_names() {
        assert_eq!(
            scope_at_macrolet("(flet ((g (q) q)) (macrolet ((m () 1)) (m)))"),
            Some(vec![])
        );
    }

    /// Data is `None`, so a caller cannot mistake "no names in scope" for
    /// "this is not code".
    #[test]
    fn a_quoted_target_has_no_scope_at_all() {
        assert_eq!(
            scope_at_macrolet("'(let ((x 1)) (macrolet ((m () 1)) (m)))"),
            None
        );
        assert_eq!(
            scope_at_macrolet("`(let ((x 1)) (macrolet ((m () 1)) (m)))"),
            None
        );
    }

    #[test]
    fn an_unquoted_target_inside_a_template_is_code_with_a_scope() {
        assert_eq!(
            scope_at_macrolet("(defun f (a) `(list ,(macrolet ((m () 1)) (m))))"),
            Some(vec!["a".to_owned()])
        );
    }

    // ---- caller-supplied parameters ---------------------------------------

    fn caller_params(source: &str) -> Vec<String> {
        let parsed = tree(source);
        let form = &parsed.root_view().children[0];
        caller_supplied_parameters(definition_lambda_list(form).expect("a lambda list"))
    }

    #[test]
    fn required_and_body_parameters_are_caller_structure() {
        assert_eq!(
            caller_params("(defmacro m (a b &body forms) a)"),
            vec!["a".to_owned(), "b".to_owned(), "forms".to_owned()]
        );
    }

    #[test]
    fn a_whole_variable_is_caller_structure_and_required_params_follow_it() {
        assert_eq!(
            caller_params("(defmacro m (&whole w a b) a)"),
            vec!["a".to_owned(), "b".to_owned(), "w".to_owned()]
        );
    }

    /// An `&optional`/`&key` parameter may hold a default the expander built
    /// itself, so it is not the caller's structure and is not admitted.
    #[test]
    fn defaultable_parameters_are_not_caller_structure() {
        assert_eq!(
            caller_params("(defmacro m (a &optional b &key c &aux d) a)"),
            vec!["a".to_owned()]
        );
    }

    /// `&environment` names an environment object, not a form.
    #[test]
    fn an_environment_variable_is_not_caller_structure() {
        assert_eq!(
            caller_params("(defmacro m (a &environment e) a)"),
            vec!["a".to_owned()]
        );
        assert_eq!(
            caller_params("(defmacro m (&environment e a) a)"),
            vec!["a".to_owned()]
        );
    }

    #[test]
    fn a_rest_variable_is_caller_structure() {
        assert_eq!(
            caller_params("(defmacro m (a &rest more) a)"),
            vec!["a".to_owned(), "more".to_owned()]
        );
    }

    // ---- shadowing ---------------------------------------------------------

    fn bound_within(source: &str) -> Vec<String> {
        let parsed = tree(source);
        names_bound_within(&parsed.root_view())
    }

    #[test]
    fn a_let_inside_the_expander_rebinds_the_name() {
        assert_eq!(
            bound_within("(let ((body (copy-list body))) body)"),
            vec!["body".to_owned()]
        );
    }

    #[test]
    fn a_setf_of_a_bare_name_counts_as_reassignment() {
        assert_eq!(
            bound_within("(setf body (copy-list body))"),
            vec!["body".to_owned()]
        );
        assert_eq!(bound_within("(setq body nil)"), vec!["body".to_owned()]);
    }

    /// A `let` written inside a **template** binds a name in the caller's
    /// expansion, not in the expander, so it must not count as shadowing.
    #[test]
    fn a_let_inside_a_template_does_not_rebind_the_expanders_name() {
        assert!(bound_within("`(let ((body 1)) ,body)").is_empty());
    }

    /// …but a `let` under an unquote is expander code again.
    #[test]
    fn a_let_under_an_unquote_does_rebind() {
        assert_eq!(
            bound_within("`(progn ,(let ((body 1)) body))"),
            vec!["body".to_owned()]
        );
    }

    /// A `setf` of a *place* rather than a bare name does not re-point the
    /// name; it mutates the structure the name designates, which is the very
    /// thing `macro-body-destroys-argument-form` reports.
    #[test]
    fn a_setf_of_a_place_is_not_a_reassignment_of_the_name() {
        assert!(bound_within("(setf (car body) 1)").is_empty());
    }

    #[test]
    fn a_definitions_name_is_normalized() {
        let parsed = tree("(defmacro cl-user::my-macro (a) a)");
        assert_eq!(
            definition_name(&parsed.root_view().children[0]).as_deref(),
            Some("my-macro")
        );
    }
}
