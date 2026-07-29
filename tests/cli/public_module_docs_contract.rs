#[test]
fn top_level_public_modules_keep_docs_rs_responsibility_docs() {
    for (path, required) in [
        (
            "src/application/mod.rs",
            "//! Application services that orchestrate typed domain operations into",
        ),
        (
            "src/application/usecase/mod.rs",
            "//! Application use cases for Lisp-aware analysis, reporting, and refactor planning.",
        ),
        (
            "packages/feature/refactor-workflow/src/refactor/usecase/mod.rs",
            "//! Refactor planning, preview, and guarded apply services.",
        ),
        (
            "src/domain/mod.rs",
            "//! Core Lisp parsing, dialect, and semantic refactoring rules that stay",
        ),
        (
            "src/infrastructure/mod.rs",
            "//! Infrastructure adapters that turn filesystems and workspace discovery into",
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
