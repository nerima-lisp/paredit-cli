use std::path::PathBuf;

use paredit_core_syntax::dialect::Dialect;
use paredit_feature_package::dependency_report::usecase::DependencyReportItem;

#[derive(Debug)]
pub struct DependencyReportFile {
    pub path: PathBuf,
    pub dialect: Dialect,
    pub package: Option<String>,
    pub dependencies: Vec<DependencyReportItem>,
}
