//! Development harness for `application::usecase::semantic_coverage`.
//!
//! Not part of the released binary or the CLI contract — this exists to
//! measure, on a real corpus, how much of `domain::semantics` actually
//! resolves. Run it against a directory of Common Lisp sources:
//!
//! ```text
//! cargo run --example semantic_coverage -- path/to/corpus
//! ```

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use paredit_cli::application::usecase::semantic_coverage::{
    DiscoveredSemanticCoverageFile, SemanticCoverageInventory, SemanticCoverageReport,
    SemanticCoverageRequest, SemanticCoverageSourcePort, build_semantic_coverage_report,
};

const EXTENSIONS: [&str; 3] = ["lisp", "lsp", "cl"];

/// How many entries of each histogram to print by default. Long enough to see
/// whether a tail exists, short enough that the answer to "what should I
/// register next" is on one screen.
const DEFAULT_TOP_CAUSES: usize = 25;

/// The histogram depth, from `SEMANTIC_COVERAGE_TOP` when it holds a number.
///
/// An environment variable rather than a flag because the positional arguments
/// are paths, and a development harness is not worth an argument parser.
fn top_causes() -> usize {
    std::env::var("SEMANTIC_COVERAGE_TOP")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(DEFAULT_TOP_CAUSES)
}

/// Walks the given paths for `.lisp`/`.lsp`/`.cl` files and reads them
/// straight off disk. A throwaway implementation of the port: the CLI shell
/// has a full workspace discovery module, but this harness has no CLI
/// contract to keep and does not need it.
#[derive(Default)]
struct FilesystemSource;

impl SemanticCoverageSourcePort for FilesystemSource {
    fn discover(
        &mut self,
        request: &SemanticCoverageRequest,
    ) -> anyhow::Result<SemanticCoverageInventory> {
        let mut files = Vec::new();
        for root in &request.paths {
            collect(root, &mut files)?;
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
        fs::read(&file.path).map_err(|error| error.to_string())
    }
}

fn collect(root: &Path, files: &mut Vec<PathBuf>) -> anyhow::Result<()> {
    let metadata = fs::symlink_metadata(root)
        .map_err(|error| anyhow::anyhow!("{}: {error}", root.display()))?;
    if metadata.is_dir() {
        let mut entries: Vec<PathBuf> = fs::read_dir(root)
            .map_err(|error| anyhow::anyhow!("{}: {error}", root.display()))?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .collect();
        entries.sort();
        for entry in entries {
            collect(&entry, files)?;
        }
    } else if is_lisp_source(root) {
        files.push(root.to_path_buf());
    }
    Ok(())
}

fn is_lisp_source(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| EXTENSIONS.contains(&extension))
}

fn main() -> ExitCode {
    let paths: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    if paths.is_empty() {
        eprintln!("usage: semantic_coverage <path>...");
        return ExitCode::FAILURE;
    }

    let mut source = FilesystemSource;
    let report =
        match build_semantic_coverage_report(&mut source, SemanticCoverageRequest { paths }) {
            Ok(report) => report,
            Err(error) => {
                eprintln!("error: {error}");
                return ExitCode::FAILURE;
            }
        };

    print_report(&report);
    ExitCode::SUCCESS
}

fn print_report(report: &SemanticCoverageReport) {
    println!("== per-file ==");
    for file in report.files() {
        println!(
            "{}: bindings {}/{} ({}), lists {}/{} ({})",
            file.path().display(),
            file.resolved_binding_count(),
            file.variable_binding_count(),
            percentage(file.resolved_binding_count(), file.variable_binding_count()),
            file.known_list_expression_count(),
            file.list_expression_count(),
            percentage(
                file.known_list_expression_count(),
                file.list_expression_count()
            ),
        );
    }

    if !report.errors().is_empty() {
        println!("\n== errors ==");
        for error in report.errors() {
            println!(
                "{}: {} failed: {}",
                error.path.display(),
                error.stage.label(),
                error.message
            );
        }
    }

    let total_bindings = report.total_variable_bindings();
    let resolved_bindings = report.total_resolved_bindings();
    let total_lists = report.total_list_expressions();
    let known_lists = report.total_known_list_expressions();

    println!("\n== totals ==");
    println!(
        "variable bindings resolved: {resolved_bindings}/{total_bindings} ({})",
        percentage(resolved_bindings, total_bindings)
    );
    println!(
        "list expressions folded to a known value: {known_lists}/{total_lists} ({})",
        percentage(known_lists, total_lists)
    );

    println!(
        "\n== non-resolution breakdown (of {} unresolved bindings) ==",
        total_bindings - resolved_bindings
    );
    let breakdown = report.total_non_resolution();
    let unresolved = total_bindings - resolved_bindings;
    print_reason("reassigned", breakdown.reassigned(), unresolved);
    print_reason("opaque scope", breakdown.opaque_scope(), unresolved);
    print_reason("special", breakdown.special(), unresolved);
    print_reason("no initial form", breakdown.no_initial_form(), unresolved);
    print_reason(
        "initial form did not fold",
        breakdown.initial_form_not_constant(),
        unresolved,
    );
    print_reason(
        "initial form folded but cannot propagate",
        breakdown.initial_form_not_propagatable(),
        unresolved,
    );

    let top = top_causes();

    println!(
        "\n== what made {} scopes opaque (top {top}) ==",
        breakdown.opaque_scope()
    );
    for (label, count) in breakdown.ranked_opacity_causes().into_iter().take(top) {
        println!(
            "{}: {count} ({})",
            label.display(),
            percentage(count, breakdown.opaque_scope())
        );
    }

    println!(
        "\n== which binder left {} bindings uninitialized (top {top}) ==",
        breakdown.no_initial_form()
    );
    for (head, count) in breakdown
        .ranked_uninitialized_binders()
        .into_iter()
        .take(top)
    {
        println!(
            "{}: {count} ({})",
            head.as_deref().unwrap_or("<no binder>"),
            percentage(count, breakdown.no_initial_form())
        );
    }
}

fn print_reason(label: &str, count: usize, unresolved: usize) {
    println!("{label}: {count} ({})", percentage(count, unresolved));
}

fn percentage(numerator: usize, denominator: usize) -> String {
    if denominator == 0 {
        return "n/a".to_owned();
    }
    format!("{:.1}%", (numerator as f64 / denominator as f64) * 100.0)
}
