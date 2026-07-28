//! Whether an occurrence names the symbol the report is about.

use paredit_core_semantics::semantics::project::QualifiedSymbol;
use paredit_core_semantics::semantics::project::service::{
    FilePackages, resolve_file_packages, resolve_symbol,
};
use paredit_core_syntax::common_lisp::common_lisp_symbol_reference_eq;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{SymbolName, SyntaxTree};

/// The queried symbol, plus the package regions of one file.
///
/// Every match in an impact report used to be a bare-name comparison, which
/// makes `app:run` and `test:run` one symbol and reports a change to one as
/// affecting callers of the other. This narrows that comparison with the
/// package each side resolves to, without ever widening it.
#[derive(Debug)]
pub struct SymbolIdentity {
    symbol: SymbolName,
    target: Option<QualifiedSymbol>,
    packages: FilePackages,
}

impl SymbolIdentity {
    pub fn new(dialect: Dialect, tree: &SyntaxTree, symbol: &SymbolName) -> Self {
        Self {
            symbol: symbol.clone(),
            target: resolve_target(dialect, symbol),
            packages: resolve_file_packages(dialect, tree),
        }
    }

    pub const fn symbol(&self) -> &SymbolName {
        &self.symbol
    }

    /// Whether the occurrence `text`, starting at `offset`, names the symbol.
    ///
    /// The name comparison stays the gate, so this can only ever drop a match
    /// the report used to make -- never add one.
    pub fn matches(&self, text: &str, offset: usize) -> bool {
        common_lisp_symbol_reference_eq(text, self.symbol.as_str())
            && self.packages_agree(text, offset)
    }

    /// The three-valued package decision, kept out of boolean form so that
    /// "unknown" cannot collapse into "different".
    fn packages_agree(&self, text: &str, offset: usize) -> bool {
        match (
            self.target.as_ref(),
            resolve_symbol(&self.packages, text, offset),
        ) {
            (Some(target), Some(occurrence)) => target.package() == occurrence.package(),
            // An unqualified name in a file with no `in-package` is read in
            // whatever package the build set up, which no static analysis can
            // know. Treating that as a mismatch would silently drop real
            // references from the single-package files that make up most of
            // this tool's input, so the honest answer stays the bare-name one.
            _ => true,
        }
    }
}

/// The package the queried symbol belongs to, if it names one at all.
///
/// The symbol arrives from a command line rather than from a file, so there is
/// no `in-package` region to read it in: only an explicit qualifier gives it a
/// package. A bare `run` therefore asks about the name in whichever package
/// each occurrence is read in, and must keep matching all of them -- excluding
/// on a package the caller never named would turn this fix into a false
/// negative.
///
/// Outside Common Lisp a colon is not a package marker at all, so no dialect
/// but that one may resolve a target.
fn resolve_target(dialect: Dialect, symbol: &SymbolName) -> Option<QualifiedSymbol> {
    (dialect == Dialect::CommonLisp)
        .then(|| resolve_symbol(&FilePackages::default(), symbol.as_str(), 0))
        .flatten()
}
