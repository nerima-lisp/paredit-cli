#[test]
fn top_level_public_modules_keep_docs_rs_responsibility_docs() {
    for (path, required) in [
        (
            "src/lint/mod.rs",
            "//! The lint context: what a rule is, how rules are registered, and the single",
        ),
        (
            "src/semantic_coverage.rs",
            "//! Measuring how much of the semantic layer resolves on real source.",
        ),
        (
            "packages/feature/refactor-workflow/src/refactor/usecase/mod.rs",
            "//! Refactor planning, preview, and guarded apply services.",
        ),
        (
            "src/presentation/mod.rs",
            "//! Presentation adapters that map delivery mechanisms — the command line, and",
        ),
        (
            "packages/core/syntax/src/sexpr.rs",
            "//! Typed S-expression parsing, tree navigation, spans, and balanced edit",
        ),
        (
            "packages/core/syntax/src/dialect/mod.rs",
            "//! Dialect detection and capability helpers for Lisp-family files, including",
        ),
    ] {
        let contents = std::fs::read_to_string(path).expect("read module file");
        assert!(
            contents.contains(required),
            "top-level public module docs drifted for {path}: missing `{required}`"
        );
    }
}
