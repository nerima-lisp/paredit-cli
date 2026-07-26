use anyhow::Result;

use crate::domain::dialect::Dialect;
use crate::domain::sexpr::reader::apply_reader_prefix_context;
use crate::domain::sexpr::{ExpressionView, Path};

use super::super::LetFormReport;
use super::report::analyze_let_form;

pub(in crate::domain::let_report) fn collect_let_reports_from_view(
    dialect: Dialect,
    input: &str,
    view: &ExpressionView,
    path: Path,
    reports: &mut Vec<LetFormReport>,
) -> Result<()> {
    collect_let_reports_in_context(dialect, input, view, path, reports, 0)
}

/// A `let`-shaped list found inside a quasiquote template (and not itself
/// inside a nested unquote) is not a real binding to analyze: it is a code
/// fragment a macro assembles for its expansion, e.g. `` `(let ((,x ,val))
/// ...) `` in a with-gensyms-style macro helper. Its "binding name" is
/// frequently a literal `,x` (an unquoted gensym variable, not a real
/// symbol), and judging such a fragment's bindings "unused" and removing
/// them would corrupt the macro's generated code rather than clean up dead
/// code. `quasiquote_depth` mirrors the same tracking used by scope-aware
/// reference collection: it increments inside `` ` ``, decrements inside
/// `,`/`,@`, and forms are only analyzed at depth 0.
fn collect_let_reports_in_context(
    dialect: Dialect,
    input: &str,
    view: &ExpressionView,
    path: Path,
    reports: &mut Vec<LetFormReport>,
    quasiquote_depth: usize,
) -> Result<()> {
    let Some(quasiquote_depth) = apply_reader_prefix_context(view, quasiquote_depth) else {
        return Ok(());
    };

    if quasiquote_depth == 0 {
        if let Some(report) = analyze_let_form(dialect, input, view, &path)? {
            reports.push(report);
        }
    }

    for (index, child) in view.children.iter().enumerate() {
        collect_let_reports_in_context(
            dialect,
            input,
            child,
            path.child(index),
            reports,
            quasiquote_depth,
        )?;
    }

    Ok(())
}
