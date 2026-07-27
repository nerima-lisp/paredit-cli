use std::path::PathBuf;

use crate::application::usecase::call_report::CallReportItem;
use crate::domain::dialect::Dialect;

#[derive(Debug)]
pub(in crate::presentation::cli::call_report) struct CallReportFile {
    pub(in crate::presentation::cli::call_report) path: PathBuf,
    pub(in crate::presentation::cli::call_report) dialect: Dialect,
    pub(in crate::presentation::cli::call_report) calls: Vec<CallReportItem>,
}
