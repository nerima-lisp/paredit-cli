use anyhow::Result;

use crate::form_report::usecase::types::{FormReport, FormReportRequest};

pub fn build_form_report(request: FormReportRequest<'_>) -> Result<FormReport> {
    Ok(crate::form_report::domain::build_form_report(request))
}
