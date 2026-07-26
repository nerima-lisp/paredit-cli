use crate::domain::{
    dialect::Dialect,
    sexpr::{ByteSpan, SymbolName},
};

use super::PackageOptionSortOrder;

#[derive(Debug, Clone)]
pub struct AddExportRequest<'a> {
    pub input: &'a str,
    pub dialect: Dialect,
    pub package: Option<&'a SymbolName>,
    pub symbol: &'a SymbolName,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddExportPlan {
    pub package: String,
    pub symbol: SymbolName,
    pub defpackage_path: String,
    pub defpackage_span: ByteSpan,
    pub export_span: Option<ByteSpan>,
    pub insertion_span: ByteSpan,
    pub already_exported: bool,
    pub changed: bool,
    pub rewritten: String,
}

#[derive(Debug, Clone)]
pub struct RenamePackageRequest<'a> {
    pub input: &'a str,
    pub dialect: Dialect,
    pub from: &'a SymbolName,
    pub to: &'a SymbolName,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenamePackagePlan {
    pub occurrences: Vec<PackageRenameOccurrence>,
    pub changed: bool,
    pub rewritten: String,
}

#[derive(Debug, Clone)]
pub struct SortPackageExportsRequest<'a> {
    pub input: &'a str,
    pub dialect: Dialect,
    pub package: Option<&'a SymbolName>,
}

#[derive(Debug, Clone)]
pub struct SortPackageOptionsRequest<'a> {
    pub input: &'a str,
    pub dialect: Dialect,
    pub package: Option<&'a SymbolName>,
    pub order: PackageOptionSortOrder,
}

#[derive(Debug, Clone)]
pub struct MergePackageOptionsRequest<'a> {
    pub input: &'a str,
    pub dialect: Dialect,
    pub package: Option<&'a SymbolName>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SortPackageExportsPlan {
    pub exports: Vec<PackageExportSort>,
    pub changed: bool,
    pub rewritten: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SortPackageOptionsPlan {
    pub packages: Vec<PackageOptionSort>,
    pub changed: bool,
    pub rewritten: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergePackageOptionsPlan {
    pub merges: Vec<PackageOptionMerge>,
    pub changed: bool,
    pub rewritten: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageExportSort {
    pub package: String,
    pub defpackage_path: String,
    pub defpackage_span: ByteSpan,
    pub export_path: String,
    pub export_span: ByteSpan,
    pub old_symbols: Vec<String>,
    pub new_symbols: Vec<String>,
    pub changed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageOptionSort {
    pub package: String,
    pub defpackage_path: String,
    pub defpackage_span: ByteSpan,
    pub old_options: Vec<String>,
    pub new_options: Vec<String>,
    pub changed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageOptionMerge {
    pub package: String,
    pub defpackage_path: String,
    pub defpackage_span: ByteSpan,
    pub head: String,
    pub key: Option<String>,
    pub kept_path: String,
    pub kept_span: ByteSpan,
    pub removed_paths: Vec<String>,
    pub removed_spans: Vec<ByteSpan>,
    pub old_atoms: Vec<String>,
    pub new_atoms: Vec<String>,
    pub changed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageRenameOccurrence {
    pub kind: PackageRenameKind,
    pub path: String,
    pub span: ByteSpan,
    pub text: String,
    pub replacement: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageRenameKind {
    DefpackageName,
    InPackageName,
    PackageOption,
    QualifiedPrefix,
}

impl PackageRenameKind {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::DefpackageName => "defpackage-name",
            Self::InPackageName => "in-package-name",
            Self::PackageOption => "package-option",
            Self::QualifiedPrefix => "qualified-prefix",
        }
    }
}
