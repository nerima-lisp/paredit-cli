use std::path::PathBuf;

use crate::application::usecase::dependency_report::DependencyReportItem;
use crate::domain::dialect::Dialect;

#[derive(Debug)]
pub struct DependencyReportFile {
    pub path: PathBuf,
    pub dialect: Dialect,
    pub package: Option<String>,
    pub dependencies: Vec<DependencyReportItem>,
}
