use crate::error::ProjectAnalysisResult;

use crate::form_report::usecase::types::{FormReport, FormReportRequest};

pub fn build_form_report(request: FormReportRequest<'_>) -> ProjectAnalysisResult<FormReport> {
    Ok(crate::form_report::domain::build_form_report(request))
}
