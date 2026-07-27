use anyhow::Result;

use crate::application::usecase::form_report::types::{FormReport, FormReportRequest};

pub fn build_form_report(request: FormReportRequest<'_>) -> Result<FormReport> {
    Ok(crate::domain::form_report::build_form_report(request))
}
