/// Guards the doc comments that make up the public library surface.
///
/// The import lines name `paredit_core_syntax`, not `paredit_cli`, since these
/// examples now live in that package and a doctest can only import the crate it
/// is compiled into. The alternative - freezing the `paredit_cli::` spelling -
/// would stop the examples compiling at all, trading a genuinely executed
/// example for a string that merely looks right.
///
/// The re-export chain means `paredit_cli::dialect::Dialect` and
/// `paredit_cli::sexpr::SyntaxTree` still resolve for callers; only the crate
/// named inside the example changed.
#[test]
fn public_library_api_keeps_docs_rs_surface_docs() {
    for (path, required) in [
        (
            "packages/core/syntax/src/dialect/mod.rs",
            "/// Selects Lisp-family parsing and refactoring rules for a source file.",
        ),
        ("packages/core/syntax/src/dialect/mod.rs", "/// # Examples"),
        (
            "packages/core/syntax/src/dialect/mod.rs",
            "/// use paredit_core_syntax::dialect::Dialect;",
        ),
        (
            "packages/core/syntax/src/dialect/mod.rs",
            "/// Resolves the effective dialect from an explicit override or file extension.",
        ),
        (
            "packages/core/syntax/src/sexpr/types.rs",
            "/// A byte offset into the original source text.",
        ),
        (
            "packages/core/syntax/src/sexpr/types.rs",
            "/// A zero-based path from the virtual root to a nested expression.",
        ),
        ("packages/core/syntax/src/sexpr/types.rs", "/// # Examples"),
        (
            "packages/core/syntax/src/sexpr/types.rs",
            "/// use paredit_core_syntax::sexpr::ExpressionPath;",
        ),
        (
            "packages/core/syntax/src/sexpr/types.rs",
            "/// A validated Lisp-family symbol name without reader delimiters or whitespace.",
        ),
        (
            "packages/core/syntax/src/sexpr/tree.rs",
            "/// A parsed S-expression document with tree navigation and query helpers.",
        ),
        ("packages/core/syntax/src/sexpr/tree.rs", "/// # Examples"),
        (
            "packages/core/syntax/src/sexpr/tree.rs",
            "/// use paredit_core_syntax::sexpr::{ExpressionPath, SyntaxTree};",
        ),
        (
            "packages/core/syntax/src/sexpr/tree.rs",
            "/// use paredit_core_syntax::sexpr::{SymbolName, SyntaxTree};",
        ),
        (
            "packages/core/syntax/src/sexpr/tree.rs",
            "/// Builds an outline of root-level lists and marks definition-like forms.",
        ),
        (
            "packages/core/syntax/src/sexpr/tree.rs",
            "/// A validated selection of one non-root expression inside a syntax tree.",
        ),
    ] {
        let contents = std::fs::read_to_string(path).expect("read public api file");
        assert!(
            contents.contains(required),
            "public API docs drifted for {path}: missing `{required}`"
        );
    }
}
