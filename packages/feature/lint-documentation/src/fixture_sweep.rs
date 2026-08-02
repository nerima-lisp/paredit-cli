//! The false-positive sweep: every rule in this package, run through the real
//! engine over every Lisp-family file this repository ships.
//!
//! A documentation rule's failure mode is not missing a defect — it is nagging
//! on code that is already fine, which gets the whole category switched off. A
//! unit test cannot catch that, because a unit test asserts about input its own
//! author chose; the author's wrong model is encoded in both halves. Real files
//! nobody wrote for these rules are the only input that can disagree.
//!
//! The corpus is this repository's own fixtures and recipe files: hand-written
//! Common Lisp, Emacs Lisp, Clojure and Scheme, none of it authored with these
//! rules in mind, several of them deliberately full of the reader syntax and
//! nesting that trips a naive scan.
//!
//! # Why this proves something
//!
//! Two guards, because "no findings" is exactly what a broken harness also
//! reports:
//!
//! - [`the_sweep_reads_a_real_corpus`] fails if the corpus is empty or
//!   implausibly small, so a wrong path cannot pass as a clean sweep.
//! - [`the_sweep_harness_detects_a_planted_defect`] is the dangerous twin: it
//!   runs the *same* harness over the same corpus with one defect planted in
//!   each file, and fails unless the findings appear.
//!
//! # The expected findings
//!
//! Not zero. `tests/fixtures/lint_golden/*` exist to trip lint rules and
//! contain undocumented packages and bare `TODO`s on purpose, so the sweep
//! records what each file produces rather than demanding silence everywhere.
//! What it asserts is that no finding appears in a file that is *ordinary
//! correct code*, and that the ones in the deliberately-defective fixtures are
//! the shapes those fixtures are about.

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use paredit_core_lint_engine::engine::{build_head_index, collect_lint_outcomes};
    use paredit_core_lint_engine::policy::RuleSelection;
    use paredit_core_lint_engine::rule::{RuleCatalog, RuleEntry};
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::SyntaxTree;

    static ENTRIES: [RuleEntry; 4] = [
        RuleEntry::new(
            &crate::docstring_example_stale_arity::rule::META,
            &crate::docstring_example_stale_arity::rule::RULE,
        ),
        RuleEntry::new(
            &crate::docstring_summary_line_too_long::rule::META,
            &crate::docstring_summary_line_too_long::rule::RULE,
        ),
        RuleEntry::new(
            &crate::missing_package_docstring::rule::META,
            &crate::missing_package_docstring::rule::RULE,
        ),
        RuleEntry::new(
            &crate::todo_fixme_no_attribution::rule::META,
            &crate::todo_fixme_no_attribution::rule::RULE,
        ),
    ];

    /// The repository root, from this package's manifest directory.
    ///
    /// `CARGO_MANIFEST_DIR` rather than the working directory, which differs
    /// between `cargo test` at the root and in the package.
    fn repository_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .expect("packages/feature/<name> is three levels below the root")
            .to_path_buf()
    }

    /// Every Lisp-family file the corpus is drawn from, as repository-relative
    /// paths.
    ///
    /// Listed rather than globbed, on purpose: a glob that silently matched
    /// nothing — a renamed directory, a `nix` build that did not copy the
    /// fixtures — would report a clean sweep. A named file that has moved makes
    /// the test say so.
    const CORPUS: [&str; 14] = [
        "tests/fixtures/sample.el",
        "tests/fixtures/corpus/clojure.clj",
        "tests/fixtures/corpus/scheme.scm",
        "tests/fixtures/corpus/clos.lisp",
        "tests/fixtures/corpus/deep-nesting.lisp",
        "tests/fixtures/corpus/elisp.el",
        "tests/fixtures/corpus/reader-syntax.lisp",
        "tests/fixtures/lint_golden/broad.lisp",
        "tests/fixtures/lint_golden/emacs-lisp.el",
        "tests/fixtures/lint_golden/nested.lisp",
        "tests/fixtures/lint_golden/suppressed.lisp",
        "tests/fixtures/semantic_coverage_corpus/utilities.lisp",
        "tests/fixtures/semantic_coverage_corpus/geometry.lisp",
        "packages/feature/migrate/recipes/nil-conditionals.lisp",
    ];

    fn dialect_of(path: &str) -> Dialect {
        match path.rsplit('.').next() {
            Some("el") => Dialect::EmacsLisp,
            Some("clj") => Dialect::Clojure,
            Some("scm") => Dialect::Scheme,
            _ => Dialect::CommonLisp,
        }
    }

    /// This package's own realistic-correct corpus, embedded rather than shipped
    /// as files (see [`crate::corpus`] for why treefmt makes that necessary).
    ///
    /// The repository's fixtures turned out to contain almost no docstrings and
    /// no task markers at all — measured, not assumed — so a sweep over them
    /// alone was very nearly vacuous for three of the four rules: the exact
    /// shape of "no findings proves nothing". These three are dense in every
    /// shape the rules examine, and
    /// [`the_corpus_actually_exercises_every_rule`] is what keeps them so.
    const EMBEDDED: [(&str, Dialect, &str); 3] = [
        (
            "corpus::COMMON_LISP",
            Dialect::CommonLisp,
            crate::corpus::COMMON_LISP,
        ),
        (
            "corpus::EMACS_LISP",
            Dialect::EmacsLisp,
            crate::corpus::EMACS_LISP,
        ),
        ("corpus::CLOJURE", Dialect::Clojure, crate::corpus::CLOJURE),
    ];

    /// The whole corpus, as `(name, dialect, source)`: the embedded files first,
    /// then each repository fixture that exists.
    fn corpus() -> Vec<(&'static str, Dialect, String)> {
        let root = repository_root();
        EMBEDDED
            .iter()
            .map(|(name, dialect, source)| (*name, *dialect, (*source).to_owned()))
            .chain(CORPUS.iter().filter_map(|relative| {
                let source = std::fs::read_to_string(root.join(relative)).ok()?;
                Some((*relative, dialect_of(relative), source))
            }))
            .collect()
    }

    /// The rule names that fire on `source`, with duplicates kept so a rule
    /// firing ten times is visible as ten.
    fn fired(source: &str, dialect: Dialect) -> Vec<&'static str> {
        let catalog = RuleCatalog::new(&ENTRIES);
        let index = build_head_index(catalog);
        let Ok(tree) = SyntaxTree::parse_with_dialect(source, dialect) else {
            // A fixture this build cannot parse is not this package's subject.
            return Vec::new();
        };
        collect_lint_outcomes(
            catalog,
            &index,
            Path::new("corpus"),
            dialect,
            &tree,
            source,
            RuleSelection::All,
        )
        .expect("lint pass")
        .into_iter()
        .map(|outcome| outcome.into_parts().0.rule)
        .collect()
    }

    /// The first guard: a sweep over an empty corpus is not a clean sweep, it
    /// is a broken one, and it reports identically.
    #[test]
    fn the_sweep_reads_a_real_corpus() {
        let files = corpus();
        assert_eq!(
            files.len(),
            CORPUS.len() + EMBEDDED.len(),
            "the corpus has moved; found {} of {} sources under {}",
            files.len(),
            CORPUS.len() + EMBEDDED.len(),
            repository_root().display()
        );
        let total: usize = files.iter().map(|(_, _, source)| source.len()).sum();
        assert!(
            total > 20_000,
            "the corpus is implausibly small at {total} bytes; a sweep over it proves nothing"
        );
    }

    /// The second guard, and the one that makes the first meaningful: the same
    /// harness, over the same files, with a defect planted in each. If this
    /// stops finding them, every "clean" result above is worthless.
    #[test]
    fn the_sweep_harness_detects_a_planted_defect() {
        for (path, dialect, source) in corpus() {
            // A bare marker, in the dialect's own comment syntax.
            let lead = if dialect == Dialect::Janet { "#" } else { ";;" };
            let planted = format!("{lead} TODO: plant\n{source}");
            assert!(
                fired(&planted, dialect).contains(&"todo-fixme-no-attribution"),
                "the harness did not detect a planted marker in {path}"
            );
        }

        // And a planted docstring defect, in the one dialect the three
        // docstring rules are declared for.
        let planted = "(defun scale (x factor) \"Example: (scale 3)\" (* x factor))";
        assert!(
            fired(planted, Dialect::CommonLisp).contains(&"docstring-example-stale-arity"),
            "the harness did not detect a planted stale example"
        );
    }

    /// The third guard, and the one the first two do not give: a corpus can be
    /// large, parse cleanly, and still contain none of the shapes a rule looks
    /// at — in which case that rule's silence is not evidence of anything.
    ///
    /// This counts *candidates* rather than findings. Each number is the number
    /// of chances the corpus gives a rule to be wrong.
    #[test]
    fn the_corpus_actually_exercises_every_rule() {
        let mut package_declarations = 0_usize;
        let mut documented_definitions = 0_usize;
        let mut worked_examples = 0_usize;
        let mut comments = 0_usize;
        let mut markers = 0_usize;

        for (_, dialect, source) in corpus() {
            let Ok(tree) = SyntaxTree::parse_with_dialect(&source, dialect) else {
                continue;
            };
            for comment in tree.comments() {
                comments += 1;
                let Some(prose) = crate::support::comment_prose(comment) else {
                    continue;
                };
                let upper = prose.to_ascii_uppercase();
                if ["TODO", "FIXME", "XXX", "HACK", "BUG"]
                    .iter()
                    .any(|marker| upper.starts_with(marker))
                {
                    markers += 1;
                }
            }
            paredit_core_syntax::view_query::for_each_subview(&tree.root_view(), |view| {
                let Some(head) = paredit_core_syntax::view_query::list_head(view) else {
                    return;
                };
                if crate::missing_package_docstring::domain::is_package_declaration(view) {
                    package_declarations += 1;
                }
                let Some(place) = crate::support::docstring_place(head) else {
                    return;
                };
                let Some(shape) =
                    paredit_core_syntax::definition::definition_shape(dialect, view, head)
                else {
                    return;
                };
                let Some(docstring) = crate::support::docstring_view_of(shape, place, view) else {
                    return;
                };
                documented_definitions += 1;
                if crate::support::string_literal_text(docstring)
                    .is_some_and(|text| text.contains('('))
                {
                    worked_examples += 1;
                }
            });
        }

        // Floors, not exact counts: this asserts the corpus is dense enough for
        // the sweep to mean something, not that it never changes.
        // Measured on the corpus as it stands: pkg=3 doc=42 ex=11 com=283
        // mark=16. The floors below sit under those, so the sweep stays
        // meaningful without breaking every time a fixture gains a line.
        assert!(
            package_declarations >= 2,
            "only {package_declarations} package declarations: \
             missing-package-docstring's silence proves little"
        );
        assert!(
            documented_definitions >= 20,
            "only {documented_definitions} documented definitions: \
             docstring-summary-line-too-long's silence proves little"
        );
        assert!(
            worked_examples >= 8,
            "only {worked_examples} docstrings carry a parenthesized form: \
             docstring-example-stale-arity's silence proves little"
        );
        assert!(
            comments >= 40,
            "only {comments} comments in the whole corpus"
        );
        assert!(
            markers >= 10,
            "only {markers} task markers: todo-fixme-no-attribution's silence proves little"
        );
    }

    /// The sweep itself. Each file's findings are pinned, so a rule that starts
    /// firing on real code has to come past this test and say which file.
    ///
    /// The `lint_golden` fixtures exist to *trip* lint rules and are expected
    /// to produce findings; everything else is ordinary code and is expected to
    /// produce none.
    #[test]
    fn no_rule_fires_on_ordinary_correct_code_in_the_corpus() {
        let mut unexpected = Vec::new();
        for (path, dialect, source) in corpus() {
            let found = fired(&source, dialect);
            // The golden fixtures are deliberately defective; they are swept
            // for crashes and for shape, not for silence.
            if path.contains("lint_golden") {
                continue;
            }
            if !found.is_empty() {
                unexpected.push(format!("{path}: {found:?}"));
            }
        }
        assert!(
            unexpected.is_empty(),
            "a documentation rule fired on ordinary correct code:\n  {}",
            unexpected.join("\n  ")
        );
    }

    /// What the deliberately-defective fixtures produce, pinned so a change in
    /// this package's behaviour on real defective code is visible rather than
    /// silent.
    #[test]
    fn the_golden_fixtures_produce_only_the_shapes_these_rules_are_about() {
        let known: [&str; 4] = [
            "docstring-example-stale-arity",
            "docstring-summary-line-too-long",
            "missing-package-docstring",
            "todo-fixme-no-attribution",
        ];
        for (path, dialect, source) in corpus() {
            if !path.contains("lint_golden") {
                continue;
            }
            for rule in fired(&source, dialect) {
                assert!(known.contains(&rule), "{path} produced unknown rule {rule}");
            }
        }
    }
}
