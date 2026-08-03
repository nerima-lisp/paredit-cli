//! Where a Hy parameter default lives, and which defaults are shared.

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{Delimiter, ExpressionView};

use crate::shared::{atom_text, is_hash_list, is_plain_list, list_head};

/// Hy only. The defect is Python's, and Hy is the only dialect here that
/// compiles to a Python `FunctionDef` — the very same rule was correctly
/// *refuted* for Common Lisp, where a default is an expression the callee
/// evaluates on entry to each call.
pub const DIALECTS: [Dialect; 1] = [Dialect::Hy];

/// The heads that introduce a Hy parameter list.
///
/// `defn/a` and `fn/a` were removed in Hy 1.0 in favour of `(defn :async …)`,
/// and Hy 1.3.1 rejects them outright — but 47 files of the 2825-file audit
/// corpus still spell `defn/a` and 9 still spell `fn/a`, and the defect in
/// those files is the same one. Matching a head the current compiler has
/// dropped costs one hash-map entry and reads real code.
pub const HEAD_NAMES: [&str; 4] = ["defn", "fn", "defn/a", "fn/a"];

/// Whether `head` names a form whose parameter list follows a *name*.
///
/// `(defn name [params] …)` has one; `(fn [params] …)` does not, and reading
/// the first bracket list of a `defn` without accounting for the name would
/// mistake a decorator list for the parameters.
const fn head_is_named(head: &str) -> bool {
    matches!(head.as_bytes(), b"defn" | b"defn/a")
}

/// The parameter list of a `defn`/`fn` form, or `None` when the form does not
/// have the shape this rule can read.
///
/// The accepted `defn` shape, verified against Hy 1.3.1 by running each
/// spelling, is `(defn <:async>? <[decorators]>? name [params] body…)`:
///
/// ```text
/// (defn :async [staticmethod] f [] 1)   accepted
/// (defn [staticmethod] :async f [] 1)   rejected: ":async, expected Symbol"
/// (defn [staticmethod] f [x] x)         accepted
/// ```
///
/// So the name is the first atom that is not `:async`, and a bracket list
/// *before* it is the decorator list, not the parameters. Taking the first
/// bracket list unconditionally would read `[staticmethod]` as a parameter
/// list — which happens to contain no defaults, so the rule would silently
/// report nothing on every decorated function rather than fail loudly.
/// Returns the parameter list and its child index, so the caller can take the
/// body as everything after it.
#[must_use]
pub fn parameter_list<'a>(
    view: &'a ExpressionView,
    head: &str,
) -> Option<(usize, &'a ExpressionView)> {
    let mut seen_name = !head_is_named(head);
    for (index, child) in view.children.iter().enumerate().skip(1) {
        if let Some(text) = atom_text(child) {
            if text == ":async" {
                continue;
            }
            seen_name = true;
            continue;
        }
        if is_plain_list(child, Delimiter::Bracket) {
            if seen_name {
                return Some((index, child));
            }
            // A decorator list, which precedes the name.
            continue;
        }
        return None;
    }
    None
}

/// The constructor calls whose result is a fresh mutable container.
///
/// Deliberately short. A default that is *any* call is evaluated once all the
/// same, but flagging every call would report `(defn f [[timeout (default-timeout)]] …)`
/// — where sharing an immutable return value is harmless and the author
/// plainly meant it — so this names only the builtins whose whole purpose is
/// to return something mutable.
const MUTABLE_CONSTRUCTORS: [&str; 8] = [
    "list",
    "dict",
    "set",
    "bytearray",
    "defaultdict",
    "Counter",
    "OrderedDict",
    "deque",
];

/// Whether `value`, used as a parameter default, is a fresh mutable object
/// that every call will then share.
///
/// Measured against Hy 1.3.1 — `(defn f [[acc []]] (.append acc 1) acc)`
/// called three times returns `[1, 1, 1]` from all three, and `(dict)` behaves
/// identically, so a constructor *call* is no safer than a literal.
/// `#()` (tuple), a number, a string and `None` are all immutable or shared
/// harmlessly, and `None` is the idiom this rule recommends.
#[must_use]
pub fn is_mutable_default(value: &ExpressionView) -> bool {
    // `[…]` list literal and `{…}` dict literal.
    if is_plain_list(value, Delimiter::Bracket) || is_plain_list(value, Delimiter::Brace) {
        return true;
    }
    // `#{…}` set literal. `#(…)` is a *tuple* and is immutable, so the
    // delimiter is load-bearing here and not merely descriptive.
    if is_hash_list(value, Delimiter::Brace) {
        return true;
    }
    list_head(value).is_some_and(|head| MUTABLE_CONSTRUCTORS.contains(&head))
}

/// The in-place mutators of Python's built-in mutable containers.
///
/// Restricted to methods that mutate the receiver. `.get`, `.copy`, `.index`
/// and the rest read it, and a default nothing mutates is shared harmlessly —
/// which is what an audit over 2355 third-party files established: reporting
/// every mutable default produced 88 findings, and the overwhelming majority
/// were read-only lookup tables like `[notes [0 5 7]]`.
const MUTATING_METHODS: [&str; 14] = [
    "append",
    "add",
    "extend",
    "insert",
    "update",
    "pop",
    "popitem",
    "remove",
    "discard",
    "clear",
    "sort",
    "reverse",
    "setdefault",
    "appendleft",
];

/// Whether `head` is an augmented assignment, which mutates its target in
/// place for a list (`+=` is `list.__iadd__`, i.e. `extend`).
fn is_augmented_assignment(head: &str) -> bool {
    head.len() >= 2
        && head.ends_with('=')
        && !matches!(head, "==" | "!=" | "<=" | ">=")
        && head.trim_end_matches('=').chars().all(|c| {
            matches!(
                c,
                '+' | '-' | '*' | '/' | '%' | '|' | '&' | '^' | '@' | '<' | '>'
            )
        })
}

/// Whether `view` names the parameter `name`, either directly or through a
/// `(get name …)` subscript.
fn targets(view: &ExpressionView, name: &str) -> bool {
    if atom_text(view) == Some(name) {
        return true;
    }
    list_head(view) == Some("get") && view.children.get(1).and_then(atom_text) == Some(name)
}

/// Whether the parameter `name` is mutated in place anywhere under `body`.
///
/// This is what separates the defect from the shape. A default that is only
/// *read* is shared too, but sharing an unchanging list is invisible; the bug
/// people actually hit is the accumulator that keeps its contents between
/// calls. Three mutations are recognized, which between them cover every
/// finding the audit adjudicated as genuine:
///
/// - a mutating method call — `(.append acc x)`, `(.update d …)`;
/// - an augmented assignment — `(+= acc [x])`, `(|= s other)`;
/// - assignment or deletion through a subscript — `(setv (get d k) v)`,
///   `(del (get d k))`.
///
/// Aliasing defeats it: `(setv alias acc) (.append alias x)` mutates the
/// default and is not reported. That is a false *negative*, which is the side
/// to be wrong on.
#[must_use]
pub fn parameter_is_mutated(body: &[ExpressionView], name: &str) -> bool {
    body.iter().any(|form| form_mutates(form, name))
}

fn form_mutates(view: &ExpressionView, name: &str) -> bool {
    if let Some(head) = list_head(view) {
        // `(.append acc x)` — a method call on the parameter.
        if let Some(method) = head.strip_prefix('.') {
            if MUTATING_METHODS.contains(&method)
                && view
                    .children
                    .get(1)
                    .is_some_and(|receiver| targets(receiver, name))
            {
                return true;
            }
        }
        // `(+= acc [x])`.
        if is_augmented_assignment(head)
            && view
                .children
                .get(1)
                .is_some_and(|target| targets(target, name))
        {
            return true;
        }
        // `(setv (get d k) v)` and `(del (get d k))`.
        if matches!(head, "setv" | "setx" | "del")
            && view
                .children
                .iter()
                .skip(1)
                .any(|child| list_head(child) == Some("get") && targets(child, name))
        {
            return true;
        }
        // `(assoc d k v)` — how Hy spelled in-place dict assignment before
        // `(setv (get d k) v)` replaced it. Old spellings matter here: the
        // audit corpus is full of pre-1.0 Hy, and a rule that only knew the
        // current one would under-report on exactly the files most likely to
        // carry the defect.
        if head == "assoc"
            && view
                .children
                .get(1)
                .is_some_and(|target| targets(target, name))
        {
            return true;
        }
    }
    view.children.iter().any(|child| form_mutates(child, name))
}

/// The `[name default]` pairs of a parameter list, skipping the annotation
/// syntax that sits between them.
///
/// `#^` introduces an annotation and the *next* form is the type, not a
/// parameter: `(defn f [#^ int a #^ (get List int) [b []]] …)` has five
/// children, two of which belong to annotations. Hy 1.3.1 requires the space —
/// `#^int` is rejected as an undefined reader macro — so the type is always a
/// separate node.
#[must_use]
pub fn default_pairs(parameters: &ExpressionView) -> Vec<&ExpressionView> {
    let mut pairs = Vec::new();
    let mut children = parameters.children.iter();
    while let Some(child) = children.next() {
        if atom_text(child) == Some("#^") {
            // Skip the annotation's type form.
            let _ = children.next();
            continue;
        }
        if is_plain_list(child, Delimiter::Bracket) && child.children.len() == 2 {
            pairs.push(child);
        }
    }
    pairs
}
