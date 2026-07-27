use paredit_core_syntax::sexpr::ByteSpan;

#[derive(Debug, Clone)]
pub struct PackageReport {
    pub defpackages: Vec<PackageDefinitionReport>,
    pub in_packages: Vec<InPackageReport>,
}

impl PackageReport {
    #[must_use]
    pub const fn new(
        defpackages: Vec<PackageDefinitionReport>,
        in_packages: Vec<InPackageReport>,
    ) -> Self {
        Self {
            defpackages,
            in_packages,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PackageDefinitionReport {
    pub path: String,
    pub span: ByteSpan,
    pub name: String,
    pub nicknames: Vec<String>,
    pub uses: Vec<String>,
    pub exports: Vec<String>,
    pub imports: Vec<PackageImportReport>,
    pub option_count: usize,
}

impl PackageDefinitionReport {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        path: impl Into<String>,
        span: ByteSpan,
        name: impl Into<String>,
        nicknames: Vec<String>,
        uses: Vec<String>,
        exports: Vec<String>,
        imports: Vec<PackageImportReport>,
        option_count: usize,
    ) -> Self {
        Self {
            path: path.into(),
            span,
            name: name.into(),
            nicknames,
            uses,
            exports,
            imports,
            option_count,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PackageImportReport {
    pub package: String,
    pub symbols: Vec<String>,
}

impl PackageImportReport {
    pub fn new(package: impl Into<String>, symbols: Vec<String>) -> Self {
        Self {
            package: package.into(),
            symbols,
        }
    }
}

#[derive(Debug, Clone)]
pub struct InPackageReport {
    pub path: String,
    pub span: ByteSpan,
    pub name: String,
}

impl InPackageReport {
    pub fn new(path: impl Into<String>, span: ByteSpan, name: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            span,
            name: name.into(),
        }
    }
}
