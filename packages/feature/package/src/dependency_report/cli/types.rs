use std::path::PathBuf;

use crate::application::usecase::dependency_report::DependencyReportItem;
use crate::domain::dialect::Dialect;

#[derive(Debug)]
pub(in crate::presentation::cli::dependency_report) struct DependencyReportFile {
    pub(in crate::presentation::cli::dependency_report) path: PathBuf,
    pub(in crate::presentation::cli::dependency_report) dialect: Dialect,
    pub(in crate::presentation::cli::dependency_report) package: Option<String>,
    pub(in crate::presentation::cli::dependency_report) dependencies: Vec<DependencyReportItem>,
}
