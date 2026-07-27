use std::path::PathBuf;

use crate::call_report::usecase::CallReportItem;
use paredit_core_syntax::dialect::Dialect;

#[derive(Debug)]
pub struct CallReportFile {
    pub path: PathBuf,
    pub dialect: Dialect,
    pub calls: Vec<CallReportItem>,
}
