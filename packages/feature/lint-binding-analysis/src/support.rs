//! Shared helpers for the binding-analysis rules.
//!
//! Everything here exists to keep a rule from concluding "this binding is
//! never read" when the language, the author, or the analysis itself has
//! already said otherwise. The probe in `crate::probe` established what
//! `binding_table()` does and does not know; these are the four gaps it left,
//! turned into guards.

use paredit_core_semantics::semantics::NodeKey;
use paredit_core_semantics::semantics::binding::{Binding, BindingTable};
use paredit_core_syntax::sexpr::reader::{atom_symbol_span, atom_symbol_text};
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionKind, ExpressionView, ReaderPrefix};
use paredit_core_syntax::view_query::{atom_text, list_head};

/// The scope a binding form opened, found by its opener span.
///
/// `build_binding_table` opens every `let`/`flet`/… scope with the *whole
/// form's* span as its opener, so the form a rule was handed identifies its
/// own scope exactly. Matching on the opener rather than on span containment
/// is what keeps a nested binder's names out of the answer:
/// `(let ((a (let ((b 1)) b))) …)` puts `b`'s definition inside the outer
/// binding list, but `b` lives in the inner scope.
/// A `ScopeId` cannot be constructed from outside `core/semantics`, so the
/// scopes are reached through the bindings that live in them rather than by
/// indexing the scope arena.
pub fn bindings_opened_by(table: &BindingTable, form: ByteSpan) -> impl Iterator<Item = &Binding> {
    table
        .bindings()
        .map(|(_, binding)| binding)
        .filter(move |binding| table.scope(binding.scope()).opener() == Some(form))
}

/// Whether `view` is a `(declare …)` form.
fn is_declare(view: &ExpressionView) -> bool {
    list_head(view).is_some_and(|head| head.eq_ignore_ascii_case("declare"))
}

/// The names a body's leading declarations mark `ignore` **or** `ignorable`.
///
/// Both, unlike `lint-convention`'s `ignore-declaration-conflict`, which reads
/// `ignore` only. That rule contradicts the author — it reports a name declared
/// ignored and then used — so `ignorable` ("this may or may not be used") is
/// exactly the statement it cannot contradict. This rule is the mirror image:
/// it reports a name that is *never* used, and `ignorable` is a direct
/// statement that not using it is intended. Reading only `ignore` here would
/// make every `ignorable` binding a false positive.
///
/// A prefixed atom is skipped: `(declare (ignore ,@dummies))` in a macro
/// template names no variable in this text.
#[must_use]
pub fn declared_ignorable(body: &[ExpressionView]) -> Vec<&str> {
    let mut names = Vec::new();
    for form in body {
        if !is_declare(form) {
            // Declarations only lead a body; anything else ends the section.
            break;
        }
        for specifier in form.children.iter().skip(1) {
            let is_ignore = list_head(specifier).is_some_and(|head| {
                head.eq_ignore_ascii_case("ignore") || head.eq_ignore_ascii_case("ignorable")
            });
            if !is_ignore {
                continue;
            }
            for name in specifier.children.iter().skip(1) {
                if !name.reader_prefixes.is_empty() {
                    continue;
                }
                // `(declare (ignore (function f)))` names the local function
                // `f`, which is how a `flet` binding is declared ignored.
                if let Some(inner) = function_designator(name) {
                    names.push(inner);
                    continue;
                }
                if let Some(text) = atom_text(name) {
                    names.push(text);
                }
            }
        }
    }
    names
}

/// The `f` of `(function f)`, the only shape in which a declaration can name a
/// local *function* rather than a variable.
fn function_designator(view: &ExpressionView) -> Option<&str> {
    if !list_head(view).is_some_and(|head| head.eq_ignore_ascii_case("function")) {
        return None;
    }
    view.children.get(1).and_then(atom_text)
}

/// Whether a name is spelled the way the community spells "I know this is
/// unused": a leading underscore, or exactly `_`.
///
/// Not a Common Lisp rule — the standard gives `_` no meaning — but a
/// convention widespread enough that reporting one is noise rather than a
/// finding.
#[must_use]
pub fn is_conventionally_unused(name: &str) -> bool {
    name.starts_with('_')
}

/// What a cheap syntactic scan of one binder form can say about a name,
/// before the binding table is consulted.
#[derive(Debug, Clone, Copy, Default)]
pub struct NameUse {
    /// How many times the symbol occurs anywhere under the form.
    pub occurrences: usize,
    /// Whether some *nested* binder rebinds it, which is the only way a name
    /// with several occurrences can still have an unread binding here.
    pub rebound_below: bool,
}

impl NameUse {
    /// Whether the binding table has to be asked about this name at all.
    ///
    /// One occurrence means the binding site itself and nothing else, so
    /// nothing under this form reads the name and the table can only confirm
    /// it. More than one occurrence means something does read it — *unless* a
    /// nested binder rebinds the name, in which case the extra occurrences may
    /// all belong to the inner binding and the outer one is still unread.
    ///
    /// This is the whole performance story. Asking the table requires a scan
    /// of every binding in the file, so doing it for every `let` in the file
    /// is quadratic: 125 microseconds per call on a 400 KB file. This test is
    /// linear in the size of the one form and rejects roughly 19 candidates in
    /// 20 on real code.
    #[must_use]
    pub const fn needs_the_table(self) -> bool {
        self.occurrences <= 1 || self.rebound_below
    }
}

/// Counts occurrences of each of `names` under `view`, and notes which of them
/// a nested binder rebinds.
///
/// One walk for all the names, because the walk is the cost.
///
/// `functions` selects the namespace to count in, and getting this wrong makes
/// the pre-filter unsound rather than merely slow. Common Lisp is a Lisp-2:
/// in `(flet ((x () 1)) (list x))` the local function `x` is unread and the
/// `x` in `(list x)` is a *variable*. Counting both would hide the finding.
/// So a `let`/`let*` name counts only value-position occurrences, and an
/// `flet`/`labels` name only operator-position and `#'` ones.
#[must_use]
pub fn scan_name_uses(view: &ExpressionView, names: &[&str], functions: bool) -> Vec<NameUse> {
    let mut uses = vec![NameUse::default(); names.len()];
    scan(view, names, functions, &mut uses, false, false);
    uses
}

fn scan(
    view: &ExpressionView,
    names: &[&str],
    functions: bool,
    uses: &mut [NameUse],
    nested_binder: bool,
    is_operator: bool,
) {
    if view.kind == ExpressionKind::Atom {
        let Some(text) = atom_symbol_text(view) else {
            return;
        };
        // `#'x` designates the function namespace wherever it appears.
        let function_quoted = view.reader_prefixes.contains(&ReaderPrefix::Function);
        let in_function_namespace = is_operator || function_quoted;
        if in_function_namespace != functions {
            return;
        }
        for (index, name) in names.iter().enumerate() {
            if text.eq_ignore_ascii_case(name) {
                uses[index].occurrences += 1;
            }
        }
        return;
    }

    if is_declare(view) {
        return;
    }

    // A declaration is not evaluated, so a name in one is not a read of it —
    // which is exactly why `(declare (ignore x))` leaves `x` looking unused in
    // the first place. Counting it as a use would skip the candidate before
    // the `DeclaredIgnorable` guard ever got to say so.

    // A nested binding form's binding list: its clause heads rebind.
    let head = list_head(view);
    let opens_scope = head.is_some_and(|head| {
        [
            "let",
            "let*",
            "flet",
            "labels",
            "macrolet",
            "symbol-macrolet",
        ]
        .iter()
        .any(|binder| head.eq_ignore_ascii_case(binder))
    });
    if opens_scope && nested_binder {
        if let Some(clauses) = view.children.get(1) {
            for clause in &clauses.children {
                let bound = if clause.kind == ExpressionKind::Atom {
                    Some(clause)
                } else {
                    clause.children.first()
                };
                let Some(text) = bound.and_then(atom_symbol_text) else {
                    continue;
                };
                for (index, name) in names.iter().enumerate() {
                    if text.eq_ignore_ascii_case(name) {
                        uses[index].rebound_below = true;
                    }
                }
            }
        }
    }

    for (index, child) in view.children.iter().enumerate() {
        scan(child, names, functions, uses, true, index == 0);
    }
}

/// The symbol without its package prefix: `sb-xc:*features*` -> `*features*`.
///
/// `Binding::name()` keeps the qualifier, and every name test that encodes a
/// spelling convention has to strip it first. Eight of the twenty findings in
/// the first corpus run were package-qualified dynamic variables
/// (`sb-debug:*stack-top-hint*`, `log4cl:*logger-truename*`) that the earmuff
/// test missed because the name starts with `s` and `l`.
#[must_use]
pub fn unqualified(name: &str) -> &str {
    match name.rfind(':') {
        Some(index) => &name[index + 1..],
        None => name,
    }
}

/// Whether every occurrence of `name` under `view` is one the binding table
/// accounts for — either a defining occurrence, or a reference it resolved.
///
/// The completeness check, and the guard that matters most. `references()`
/// being empty means "the table found no reference", which is only the same as
/// "there is no reference" when the table saw everything. It does not always:
/// a variable spliced into operator position of a macro template
/// (`` `(,bignum-hash key) ``) is looked up in the *function* namespace and
/// missed, and `#'name` is dropped outright. Both appear in SBCL's own
/// sources, and both made this rule report a binding that is plainly used.
///
/// So rather than enumerate the ways the table can lose a reference — a list
/// that would go stale the moment `core/semantics` changes — this asks the
/// table to account for every occurrence of the name it can see, and refuses
/// to conclude anything when it cannot. Unsound only in the safe direction:
/// an unexplained occurrence suppresses a finding, never creates one.
#[must_use]
pub fn every_occurrence_is_explained(
    table: &BindingTable,
    view: &ExpressionView,
    name: &str,
) -> bool {
    // Every definition of this name, gathered once. The obvious spelling —
    // asking `table.bindings().any(…)` per occurrence — is a scan of the whole
    // file inside a walk of the form, and made this function quadratic in the
    // number of bindings.
    let definitions: Vec<ByteSpan> = table
        .bindings()
        .filter(|(_, binding)| binding.name().as_str().eq_ignore_ascii_case(name))
        .map(|(_, binding)| binding.definition())
        .collect();

    let mut explained = true;
    walk_atoms(view, &mut |atom| {
        if !explained {
            return;
        }
        let (Some(text), Some(span)) = (atom_symbol_text(atom), atom_symbol_span(atom)) else {
            return;
        };
        if !text.eq_ignore_ascii_case(name) {
            // Not this symbol — but it may be a *blob* with this symbol buried
            // inside it. The dialect-aware parse folds `#+64-bit (…)`,
            // `#.(…)` and `#n=(…)` into one atom whose text is the entire
            // guarded form, so a reference inside one is invisible to the
            // binding table and to a symbol-level scan alike. SBCL's
            // `target-sxhash.lisp` reads `bignum-hash` only from inside
            // `#+64-bit (t (,bignum-hash key))`, and the rule called it unused.
            if buries_symbol(text, name) {
                explained = false;
            }
            return;
        }
        // A reference the table resolved, anywhere.
        if table.resolve(NodeKey::atom(span)).is_some() {
            return;
        }
        // Or the defining occurrence of some binding — its own, or a nested
        // binder's rebinding of the same name.
        if definitions.contains(&span) {
            return;
        }
        explained = false;
    });
    explained
}

/// Whether `text` contains `name` as a whole symbol token.
///
/// Only ever asked of an atom whose text is not itself `name`, which in a
/// dialect-aware parse means a folded reader form. Bounded on both sides so
/// `bignum-hash` does not match inside `sxhash-bignum-hash-p`.
fn buries_symbol(text: &str, name: &str) -> bool {
    if name.is_empty() || text.len() <= name.len() {
        return false;
    }
    let haystack = text.to_ascii_lowercase();
    let needle = name.to_ascii_lowercase();
    let mut from = 0;
    while let Some(offset) = haystack[from..].find(&needle) {
        let start = from + offset;
        let end = start + needle.len();
        let before = haystack[..start].chars().next_back();
        let after = haystack[end..].chars().next();
        if !before.is_some_and(is_symbol_char) && !after.is_some_and(is_symbol_char) {
            return true;
        }
        from = start + 1;
    }
    false
}

/// A conservative view of what can be part of a symbol name. Erring towards
/// "yes" only widens the boundary test, which costs a suppression rather than
/// creating a finding.
fn is_symbol_char(character: char) -> bool {
    !character.is_whitespace() && !matches!(character, '(' | ')' | '\'' | '"' | '`' | ',' | ';')
}

fn walk_atoms(view: &ExpressionView, visit: &mut impl FnMut(&ExpressionView)) {
    if view.kind == ExpressionKind::Atom {
        visit(view);
        return;
    }
    for child in &view.children {
        walk_atoms(child, visit);
    }
}

/// Whether a name is spelled like a dynamic variable (`*earmuffed*`).
///
/// A *conservative suppression*, and deliberately not a detection: the probe
/// showed that `build_binding_table` marks a binding special only when this
/// file declares it so, which means `(let ((*standard-output* s)) …)` — a
/// rebinding whose entire purpose is to be read with no textual reference
/// anywhere — reads as an ordinary unused lexical binding.
///
/// `lint-safety`'s `global-mutation-in-function` uses the same convention to
/// *identify* a global, which is unsound in the other direction. Here the
/// convention can only ever cost a true positive, never create a false one:
/// a lexical variable that happens to be earmuffed is simply not reported.
#[must_use]
pub fn looks_dynamically_bound(name: &str) -> bool {
    let bytes = unqualified(name).as_bytes();
    bytes.len() >= 3 && bytes[0] == b'*' && bytes[bytes.len() - 1] == b'*'
}

/// Whether the text at a point is code or data.
///
/// Two counters, not one depth: `'` is absorbing (nothing inside a hard quote
/// is ever evaluated, and a `,` inside it does not undo it) while `` ` `` is
/// a counter that `,` decrements. A single `i32` conflates them and reads
/// ``  `(a ,b) `` and `'(a ,b)` the same way, which is wrong for the second.
///
/// Mirrors `paredit-feature-lint-condition-system`'s `QuoteState`; restated
/// rather than shared because that one is private to its package and making it
/// `pub` would create a feature-to-feature dependency.
#[derive(Debug, Clone, Copy)]
pub struct QuoteState {
    hard: bool,
    quasi: u32,
}

impl QuoteState {
    pub const EVALUATED: Self = Self {
        hard: false,
        quasi: 0,
    };

    #[must_use]
    pub const fn is_data(self) -> bool {
        self.hard || self.quasi > 0
    }

    /// The state inside a node, given the state outside it and its own reader
    /// prefixes. `#'`, `#.`, `#+` and metadata are neutral: none of them turns
    /// code into data.
    #[must_use]
    pub fn after_prefixes(mut self, view: &ExpressionView) -> Self {
        for prefix in &view.reader_prefixes {
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

    /// The state inside a long-hand `(quote …)`, which macro output spells out.
    #[must_use]
    pub const fn inside(self, view_is_quote_form: bool) -> Self {
        if view_is_quote_form {
            Self {
                hard: true,
                quasi: self.quasi,
            }
        } else {
            self
        }
    }
}

/// The long-hand `(quote …)`.
#[must_use]
pub fn is_quote_form(view: &ExpressionView) -> bool {
    list_head(view).is_some_and(|head| head.eq_ignore_ascii_case("quote"))
}

/// Whether `name` appears anywhere under `view` as a function designator —
/// `#'name` or `(function name)`.
///
/// A guard against a gap in the binding table, not a feature. The reference
/// walk drops a `#'`-prefixed atom outright
/// (`core/semantics/.../binding/service/references.rs`, the
/// `reader_prefixes.contains(&ReaderPrefix::Function)` early return), so
/// `(flet ((g () 1)) (mapcar #'g list))` records **no** reference to `g` and
/// the local function reads as unused. Deleting it on that advice breaks the
/// program.
///
/// A purely textual scan, and sound in the only direction that matters: it can
/// only ever suppress a finding, never create one. Matching a `#'g` that sits
/// inside quoted data costs a true positive and nothing else.
#[must_use]
pub fn mentions_function_designator(view: &ExpressionView, name: &str) -> bool {
    if view.reader_prefixes.contains(&ReaderPrefix::Function)
        && atom_symbol_text(view).is_some_and(|text| text.eq_ignore_ascii_case(name))
    {
        return true;
    }
    // The long-hand `(function g)`, which macro output spells out.
    if list_head(view).is_some_and(|head| head.eq_ignore_ascii_case("function"))
        && view
            .children
            .get(1)
            .and_then(atom_symbol_text)
            .is_some_and(|text| text.eq_ignore_ascii_case(name))
    {
        return true;
    }
    view.children
        .iter()
        .any(|child| mentions_function_designator(child, name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::SyntaxTree;

    fn body_of(source: &str) -> (SyntaxTree, usize) {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        (tree, 2)
    }

    fn ignorable(source: &str) -> Vec<String> {
        let (tree, skip) = body_of(source);
        let view = tree.root_view().children.swap_remove(0);
        declared_ignorable(&view.children[skip..])
            .into_iter()
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn reads_both_ignore_and_ignorable() {
        assert_eq!(
            ignorable("(let ((a 1) (b 2)) (declare (ignore a) (ignorable b)) nil)"),
            vec!["a".to_owned(), "b".to_owned()]
        );
    }

    #[test]
    fn reads_a_function_designator() {
        assert_eq!(
            ignorable("(flet ((f () 1)) (declare (ignore (function f))) nil)"),
            vec!["f".to_owned()]
        );
    }

    #[test]
    fn stops_at_the_first_non_declaration() {
        assert_eq!(
            ignorable("(let ((a 1)) (print a) (declare (ignore a)))"),
            Vec::<String>::new()
        );
    }

    #[test]
    fn skips_a_spliced_name_in_a_macro_template() {
        assert_eq!(
            ignorable("(let ((a 1)) (declare (ignore ,@dummies)) nil)"),
            Vec::<String>::new()
        );
    }

    #[test]
    fn ignores_a_declaration_that_is_not_about_ignoring() {
        assert_eq!(
            ignorable("(let ((a 1)) (declare (type fixnum a)) nil)"),
            Vec::<String>::new()
        );
    }

    #[test]
    fn underscore_names_read_as_deliberately_unused() {
        assert!(is_conventionally_unused("_"));
        assert!(is_conventionally_unused("_ignored"));
        assert!(!is_conventionally_unused("x"));
        assert!(!is_conventionally_unused("a_b"));
    }

    #[test]
    fn earmuffed_names_read_as_dynamic() {
        assert!(looks_dynamically_bound("*x*"));
        assert!(looks_dynamically_bound("*standard-output*"));
        assert!(!looks_dynamically_bound("x"));
        // `*` alone and `**` are the REPL history variables, not earmuffs, and
        // are too short to be a rebinding of a named global.
        assert!(!looks_dynamically_bound("*"));
        assert!(!looks_dynamically_bound("**"));
        assert!(!looks_dynamically_bound("*x"));
        assert!(!looks_dynamically_bound("x*"));
    }
}
