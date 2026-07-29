use paredit_core_syntax::common_lisp::common_lisp_symbol_reference_eq;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionKind, ExpressionView, SyntaxTree};

/// The Common Lisp operators that turn text into a symbol.
///
/// These are the boundary between "a name the reader saw" and "a name the
/// program computes". Everything a syntactic rename can follow is on the
/// reader's side of it; everything on the other side is what this module
/// reports.
///
/// `read-from-string` and `eval` are here for the same reason and with less
/// precision: they can produce any symbol at all, so a call carrying the
/// target's name in a literal is worth pointing at even though the module
/// cannot say what will come out.
pub const SYMBOL_CONSTRUCTORS: [&str; 7] = [
    "intern",
    "find-symbol",
    "make-symbol",
    "read-from-string",
    "gentemp",
    "eval",
    // Emacs Lisp reads with `read`; `intern` and `make-symbol` it spells the
    // same way Common Lisp does.
    "read",
];

/// Why a site is reported.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConstructionKind {
    /// A string literal naming the target, handed to a symbol constructor.
    /// The rename will certainly miss it.
    NamesTargetLiterally,
    /// A symbol constructor whose argument is computed. The rename may miss
    /// it, and only running the code could say.
    ComputedName,
}

impl ConstructionKind {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::NamesTargetLiterally => "names-target-literally",
            Self::ComputedName => "computed-name",
        }
    }

    /// What the caller should do about it.
    #[must_use]
    pub const fn guidance(self) -> &'static str {
        match self {
            Self::NamesTargetLiterally => {
                "a syntactic rename cannot rewrite a name inside a string; edit this site by hand"
            }
            Self::ComputedName => {
                "this symbol's name is assembled at run time; check whether it can name the target"
            }
        }
    }
}

/// One place a rename cannot follow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacroConstructionSite {
    pub kind: ConstructionKind,
    /// The constructor that was called, as written.
    pub operator: String,
    pub span: ByteSpan,
    pub line: usize,
    /// The call, bounded so a large form does not fill the report.
    pub text: String,
}

/// How much of a call site the report quotes.
const PREVIEW_BYTES: usize = 120;

/// Finds every symbol-construction site that a rename of `symbol` might miss.
///
/// A `None` symbol reports *every* construction site, which is what a caller
/// auditing a file for macro-generated names wants; a `Some` symbol reports
/// only the sites that mention it plus the computed ones, since a computed
/// name could be anything.
#[must_use]
pub fn find_macro_construction_sites(
    tree: &SyntaxTree,
    source: &str,
    symbol: Option<&str>,
) -> Vec<MacroConstructionSite> {
    let mut sites = Vec::new();
    let mut stack = vec![tree.root_view()];

    while let Some(view) = stack.pop() {
        if let Some(site) = classify_call(&view, source, symbol) {
            sites.push(site);
        }
        stack.extend(view.children.iter().cloned());
    }

    // Source order. The walk is a stack, so the natural order is neither
    // document order nor stable across shapes.
    sites.sort_by_key(|site| (site.span.start().get(), site.span.end().get()));
    sites
}

/// Classifies one form, if it is a symbol-construction call.
fn classify_call(
    view: &ExpressionView,
    source: &str,
    symbol: Option<&str>,
) -> Option<MacroConstructionSite> {
    if view.kind != ExpressionKind::List {
        return None;
    }
    let head = view.children.first()?;
    let operator = head.text.as_deref()?;
    if !SYMBOL_CONSTRUCTORS
        .iter()
        .any(|candidate| common_lisp_symbol_reference_eq(operator, candidate))
    {
        return None;
    }

    let argument = view.children.get(1)?;
    let kind = match string_literal(argument) {
        Some(literal) => {
            // A literal that does not name the target says nothing about a
            // rename of the target, so it is not reported at all — otherwise
            // every `(intern "SOMETHING-ELSE")` in the file would be noise.
            let names_target = symbol.is_none_or(|symbol| symbol_name_matches(literal, symbol));
            if !names_target {
                return None;
            }
            ConstructionKind::NamesTargetLiterally
        }
        // A quoted symbol argument — `(intern 'foo)` is not legal, but
        // `(find-symbol (symbol-name 'foo))` reaches here as a computed name,
        // and the rename *does* rewrite the `'foo` inside it. Reporting it
        // anyway is the conservative choice: the report says "check", and the
        // check is quick.
        None => ConstructionKind::ComputedName,
    };

    Some(MacroConstructionSite {
        kind,
        operator: operator.to_owned(),
        span: view.span,
        line: line_of(source, view.span),
        text: bounded(source.get(view.span.as_range()).unwrap_or_default()),
    })
}

/// The contents of a string literal argument, if that is what this is.
fn string_literal(view: &ExpressionView) -> Option<&str> {
    if view.kind != ExpressionKind::Atom {
        return None;
    }
    let text = view.text.as_deref()?;
    let inner = text.strip_prefix('"')?.strip_suffix('"')?;
    Some(inner)
}

/// Whether a string literal names the symbol.
///
/// Compared case-insensitively because the Common Lisp reader upcases, so the
/// source's `handle-click` is the string literal's `HANDLE-CLICK`. Substring
/// matching is deliberate: `(intern (concatenate 'string "HANDLE-" "CLICK"))`
/// splits the name across two literals, and reporting the fragment is more
/// use than reporting nothing.
fn symbol_name_matches(literal: &str, symbol: &str) -> bool {
    let literal = literal.to_ascii_uppercase();
    let symbol = symbol.to_ascii_uppercase();
    // A short symbol would match half the strings in a file, so an exact match
    // is required below the length at which a substring means anything.
    const SUBSTRING_MINIMUM: usize = 4;
    if symbol.len() < SUBSTRING_MINIMUM {
        return literal == symbol;
    }
    literal.contains(&symbol)
}

fn bounded(text: &str) -> String {
    if text.len() <= PREVIEW_BYTES {
        return text.to_owned();
    }
    let mut end = PREVIEW_BYTES.min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &text[..end])
}

fn line_of(source: &str, span: ByteSpan) -> usize {
    let start = span.start().get().min(source.len());
    source[..start]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
        + 1
}

#[cfg(test)]
mod tests {
    use super::{ConstructionKind, find_macro_construction_sites};
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::SyntaxTree;

    fn sites(source: &str, symbol: Option<&str>) -> Vec<(ConstructionKind, String, usize)> {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        find_macro_construction_sites(&tree, source, symbol)
            .into_iter()
            .map(|site| (site.kind, site.operator, site.line))
            .collect()
    }

    /// The case that motivates the whole module: a call site whose symbol is a
    /// string literal, which a syntactic rename rewrites nothing of.
    #[test]
    fn a_literal_naming_the_target_is_reported() {
        let source = "(defun call-it (x)\n  (funcall (intern \"HANDLE-CLICK\") x))\n";
        assert_eq!(
            sites(source, Some("handle-click")),
            [(
                ConstructionKind::NamesTargetLiterally,
                "intern".to_owned(),
                2
            )]
        );
    }

    /// The reader upcases, so the source spells the symbol in lower case and
    /// the literal in upper. Matching case-sensitively would find nothing.
    #[test]
    fn matching_ignores_case() {
        let source = "(find-symbol \"handle-click\")\n";
        assert_eq!(
            sites(source, Some("HANDLE-CLICK")),
            [(
                ConstructionKind::NamesTargetLiterally,
                "find-symbol".to_owned(),
                1
            )]
        );
    }

    /// A literal about something else says nothing about this rename, and
    /// reporting it would bury the sites that matter.
    #[test]
    fn a_literal_naming_something_else_is_not_reported() {
        assert!(sites("(intern \"SOMETHING-ELSE\")\n", Some("handle-click")).is_empty());
    }

    /// A name split across concatenated literals still mentions the target in
    /// a fragment; a partial report beats none.
    #[test]
    fn a_name_split_across_literals_is_still_reported() {
        let source = "(intern (concatenate 'string \"HANDLE-CLICK\" \"-INNER\"))\n";
        let found = sites(source, Some("handle-click"));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0, ConstructionKind::ComputedName);
    }

    /// A short symbol would match half the strings in a file as a substring,
    /// so it has to match exactly.
    #[test]
    fn a_short_symbol_requires_an_exact_match() {
        assert!(sites("(intern \"FOOBAR\")\n", Some("foo")).is_empty());
        assert_eq!(sites("(intern \"FOO\")\n", Some("foo")).len(), 1);
    }

    #[test]
    fn a_computed_name_is_reported_as_needing_a_look() {
        let source = "(defmacro define-handler (name)\n  `(defun ,(intern (format nil \"HANDLE-~a\" name)) (event) event))\n";
        let found = sites(source, Some("handle-click"));

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0, ConstructionKind::ComputedName);
        assert_eq!(found[0].1, "intern");
    }

    /// A computed name is reported whatever the target is: nothing short of
    /// running the code can rule it out.
    #[test]
    fn a_computed_name_is_reported_for_any_target() {
        let source = "(intern (build-name x))\n";
        assert_eq!(sites(source, Some("anything-at-all")).len(), 1);
        assert_eq!(sites(source, Some("something-else")).len(), 1);
    }

    /// Every constructor, so adding one to the table is enough.
    #[test]
    fn each_symbol_constructor_is_recognised() {
        for operator in [
            "intern",
            "find-symbol",
            "make-symbol",
            "read-from-string",
            "gentemp",
        ] {
            let source = format!("({operator} \"HANDLE-CLICK\")\n");
            assert_eq!(
                sites(&source, Some("handle-click")).len(),
                1,
                "{operator} was not recognised"
            );
        }
    }

    #[test]
    fn ordinary_code_reports_nothing() {
        let source = "(defun handle-click (event)\n  (process event))\n\n(handle-click e)\n";
        assert!(sites(source, Some("handle-click")).is_empty());
    }

    /// With no target, the module answers "where does this file construct
    /// symbols at all", which is the auditing question.
    #[test]
    fn an_absent_target_reports_every_construction_site() {
        let source = "(intern \"A\")\n(find-symbol \"B\")\n(intern (compute))\n";
        assert_eq!(sites(source, None).len(), 3);
    }

    /// Nested sites must come back in source order, or two runs over the same
    /// file could report them differently.
    #[test]
    fn sites_are_reported_in_source_order() {
        let source = "(progn\n  (intern (a))\n  (list (intern (b)))\n  (intern (c)))\n";
        let lines = sites(source, None)
            .into_iter()
            .map(|(_, _, line)| line)
            .collect::<Vec<_>>();

        assert_eq!(lines, [2, 3, 4]);
    }
}
