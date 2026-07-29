//! Section R item R3: a CI regression gate on how much of the semantic layer
//! resolves, over a fixed corpus.
//!
//! `tests/fixtures/corpus` (shared with the [I10 corpus
//! test](tests/corpus.rs)) is deliberately adversarial — its own doc comment
//! says it holds "the constructs that have historically been awkward, not a
//! sample of ordinary code" — so its resolution rate is near zero and cannot
//! regress in a way this test could catch. `tests/fixtures/semantic_coverage_corpus`
//! is the R5 companion: a small, fixed sample of *ordinary* Common Lisp (plain
//! `defconstant`/`defun`/`let`), vendored so the baseline below is
//! reproducible without network access, the same way the I10 corpus is.
//!
//! Both directories follow the same `PAREDIT_CORPUS_DIR` convention from
//! `tests/corpus.rs`: set it to a real checkout for a run at scale.
//! `scripts/fetch-corpus.sh` fetches one.
//!
//! ```sh
//! PAREDIT_CORPUS_DIR=~/quicklisp/dists cargo test --test semantic_coverage_baseline -- --nocapture
//! ```

use std::path::{Path, PathBuf};

use paredit_cli::application::usecase::semantic_coverage::{
    DiscoveredSemanticCoverageFile, SemanticCoverageInventory, SemanticCoverageRequest,
    SemanticCoverageSourcePort, build_semantic_coverage_report,
};
use paredit_cli::dialect::Dialect;

const EXTENSIONS: [&str; 3] = ["lisp", "lsp", "cl"];

fn is_lisp_source(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| EXTENSIONS.contains(&extension))
}

#[derive(Default)]
struct FilesystemSource;

impl SemanticCoverageSourcePort for FilesystemSource {
    fn discover(
        &mut self,
        request: &SemanticCoverageRequest,
    ) -> anyhow::Result<SemanticCoverageInventory> {
        let mut files = Vec::new();
        for root in &request.paths {
            collect(root, &mut files);
        }
        files.sort();
        files.dedup();
        Ok(SemanticCoverageInventory {
            files: files
                .into_iter()
                .map(|path| DiscoveredSemanticCoverageFile { path })
                .collect(),
        })
    }

    fn load(&self, file: &DiscoveredSemanticCoverageFile) -> Result<Vec<u8>, String> {
        std::fs::read(&file.path).map_err(|error| error.to_string())
    }
}

/// Walks a root for Lisp sources. Symlinks are not followed, matching
/// `tests/corpus.rs`'s walker: a checkout can link to its own parent.
fn collect(root: &Path, files: &mut Vec<PathBuf>) {
    let Ok(metadata) = std::fs::symlink_metadata(root) else {
        return;
    };
    if metadata.is_dir() {
        let Ok(entries) = std::fs::read_dir(root) else {
            return;
        };
        for entry in entries.flatten() {
            collect(&entry.path(), files);
        }
    } else if metadata.is_file() && is_lisp_source(root) {
        files.push(root.to_path_buf());
    }
}

/// The vendored, always-present corpus, plus whatever `PAREDIT_CORPUS_DIR`
/// adds for an opt-in run at scale.
fn corpus_roots() -> Vec<PathBuf> {
    let mut roots = vec![PathBuf::from("tests/fixtures/semantic_coverage_corpus")];
    if let Ok(extra) = std::env::var("PAREDIT_CORPUS_DIR") {
        roots.extend(
            extra
                .split(':')
                .filter(|entry| !entry.trim().is_empty())
                .map(PathBuf::from),
        );
    }
    roots
}

/// The floor this test pins. Measured from the vendored corpus alone: 16
/// variable bindings, 2 resolved, 48 list expressions, 1 folded to a known
/// value.
///
/// A `PAREDIT_CORPUS_DIR` run only ever *adds* files, so `variable_bindings`
/// and `list_expressions` (per-file sums) can only rise. `resolved_bindings`
/// and `known_list_expressions` are not quite as guaranteed: they route
/// through the per-dialect project table, which credits a `defconstant` only
/// when it is defined exactly once project-wide, so an added file that
/// happens to redefine a name the vendored fixtures resolve through a
/// cross-file lookup could in principle lower one of those two counts. That
/// would surface as a loud, correct test failure rather than a silent wrong
/// pass, which is an acceptable edge for a floor this narrow.
///
/// A rule addition that quietly narrows the transparency table would lower
/// `resolved_bindings` or `known_list_expressions` below this floor without
/// touching `variable_bindings`/`list_expressions` at all, which is exactly
/// the regression this test exists to catch. Raising the floor after a real
/// improvement is a one-line, deliberate edit here — the same shape as the
/// pinned counts in `tests/cli/dialect_contract.rs`.
const MIN_VARIABLE_BINDINGS: usize = 16;
const MIN_RESOLVED_BINDINGS: usize = 2;
const MIN_LIST_EXPRESSIONS: usize = 48;
const MIN_KNOWN_LIST_EXPRESSIONS: usize = 1;
const MIN_RESOLVED_PERCENT: f64 = 12.0;

#[test]
fn common_lisp_resolution_over_the_bundled_corpus_does_not_regress() {
    let mut source = FilesystemSource;
    let report = build_semantic_coverage_report(
        &mut source,
        SemanticCoverageRequest {
            paths: corpus_roots(),
            dialect: None,
        },
    )
    .expect("the workflow succeeds even if a PAREDIT_CORPUS_DIR entry is missing");

    assert!(
        report.errors().is_empty(),
        "every vendored fixture is expected to read and parse: {:?}",
        report.errors()
    );

    let variable_bindings = report.total_variable_bindings();
    let resolved_bindings = report.total_resolved_bindings();
    let list_expressions = report.total_list_expressions();
    let known_list_expressions = report.total_known_list_expressions();
    let resolved_percent = if variable_bindings == 0 {
        100.0
    } else {
        (resolved_bindings as f64 / variable_bindings as f64) * 100.0
    };

    println!(
        "variable_bindings={variable_bindings} resolved_bindings={resolved_bindings} \
         list_expressions={list_expressions} known_list_expressions={known_list_expressions} \
         resolved_percent={resolved_percent:.1}%"
    );

    assert!(
        variable_bindings >= MIN_VARIABLE_BINDINGS,
        "the corpus itself shrank: {variable_bindings} < {MIN_VARIABLE_BINDINGS} variable bindings; \
         a vendored fixture was removed rather than the layer regressing"
    );
    assert!(
        list_expressions >= MIN_LIST_EXPRESSIONS,
        "the corpus itself shrank: {list_expressions} < {MIN_LIST_EXPRESSIONS} list expressions"
    );
    assert!(
        resolved_bindings >= MIN_RESOLVED_BINDINGS,
        "variable-binding resolution regressed: {resolved_bindings} < {MIN_RESOLVED_BINDINGS} \
         over a corpus of {variable_bindings}; see the histograms `inspect semantic-coverage` \
         prints for which binder or opacity cause is now blocking one that used to resolve"
    );
    assert!(
        known_list_expressions >= MIN_KNOWN_LIST_EXPRESSIONS,
        "constant folding regressed: {known_list_expressions} < {MIN_KNOWN_LIST_EXPRESSIONS} \
         known list expressions over {list_expressions}"
    );
    assert!(
        resolved_percent >= MIN_RESOLVED_PERCENT,
        "resolved {resolved_percent:.1}% of variable bindings, below the {MIN_RESOLVED_PERCENT:.1}% floor"
    );

    // R2: this corpus is Common-Lisp-only, so the layer's own by-dialect
    // asymmetry should not appear here at all.
    let dialects = report
        .by_dialect()
        .into_iter()
        .map(|(dialect, _)| dialect)
        .collect::<Vec<_>>();
    assert_eq!(dialects, vec![Dialect::CommonLisp]);
}
