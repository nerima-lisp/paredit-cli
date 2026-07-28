use std::cmp::Ordering;

use paredit_core_syntax::sexpr::ByteSpan;

#[derive(Debug, Clone)]
pub struct DependencyReport {
    pub dependencies: Vec<DependencyReportItem>,
}

impl DependencyReport {
    #[must_use]
    pub const fn new(dependencies: Vec<DependencyReportItem>) -> Self {
        Self { dependencies }
    }
}

#[derive(Debug, Clone)]
pub struct DependencyReportItem {
    pub kind: DependencyKind,
    pub target: String,
    pub path: String,
    pub span: ByteSpan,
    pub source: Option<String>,
}

impl DependencyReportItem {
    pub fn new(
        kind: DependencyKind,
        target: impl Into<String>,
        path: impl Into<String>,
        span: ByteSpan,
        source: Option<String>,
    ) -> Self {
        Self {
            kind,
            target: target.into(),
            path: path.into(),
            span,
            source,
        }
    }

    #[must_use]
    pub fn cmp_position(&self, other: &Self) -> Ordering {
        self.span
            .start()
            .cmp(&other.span.start())
            .then_with(|| self.kind.cmp(&other.kind))
            .then_with(|| self.target.cmp(&other.target))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DependencyKind {
    AsdfDependsOn,
    AsdfComponent,
    Require,
    Provide,
    Load,
    LoadFile,
    LoadLibrary,
    UsePackage,
    Import,
    DefpackageUse,
    DefpackageImportFrom,
    QualifiedSymbol,
}

impl DependencyKind {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::AsdfDependsOn => "asdf-depends-on",
            Self::AsdfComponent => "asdf-component",
            Self::Require => "require",
            Self::Provide => "provide",
            Self::Load => "load",
            Self::LoadFile => "load-file",
            Self::LoadLibrary => "load-library",
            Self::UsePackage => "use-package",
            Self::Import => "import",
            Self::DefpackageUse => "defpackage-use",
            Self::DefpackageImportFrom => "defpackage-import-from",
            Self::QualifiedSymbol => "qualified-symbol",
        }
    }
}
