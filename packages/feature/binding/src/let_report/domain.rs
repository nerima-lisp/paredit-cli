use anyhow::Result;

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, Path, SyntaxTree};

mod collect;
mod syntax;

pub fn build_let_report(
    dialect: Dialect,
    input: &str,
    tree: &SyntaxTree,
) -> Result<Vec<LetFormReport>> {
    let mut reports = Vec::new();
    for index in 0..tree.root_children().len() {
        let path = Path::root_child(index);
        let view = tree.select_path(&path)?.view();
        collect::collect_let_reports_from_view(dialect, input, &view, path, &mut reports)?;
    }
    Ok(reports)
}

#[derive(Debug, Clone, Copy)]
pub struct LetReportPolicyOptions {
    fail_on_duplicate_evaluation: bool,
    fail_on_unused_binding: bool,
    require_inlineable_bindings: Option<usize>,
}

impl LetReportPolicyOptions {
    pub fn new(
        fail_on_duplicate_evaluation: bool,
        fail_on_unused_binding: bool,
        require_inlineable_bindings: Option<usize>,
    ) -> Result<Self, String> {
        if matches!(require_inlineable_bindings, Some(0)) {
            return Err("require-inlineable-bindings must be greater than zero".to_string());
        }

        Ok(Self {
            fail_on_duplicate_evaluation,
            fail_on_unused_binding,
            require_inlineable_bindings,
        })
    }

    #[must_use]
    pub const fn fail_on_duplicate_evaluation(self) -> bool {
        self.fail_on_duplicate_evaluation
    }

    #[must_use]
    pub const fn fail_on_unused_binding(self) -> bool {
        self.fail_on_unused_binding
    }

    #[must_use]
    pub const fn require_inlineable_bindings(self) -> Option<usize> {
        self.require_inlineable_bindings
    }
}

#[derive(Debug, Clone)]
pub struct LetFormReport {
    pub path: Path,
    pub form: String,
    pub span: ByteSpan,
    pub binding_style: &'static str,
    pub body_count: usize,
    pub inline_supported_by_inline_let: bool,
    pub bindings: Vec<LetBindingReport>,
}

#[derive(Debug, Clone)]
pub struct LetBindingReport {
    pub name: String,
    pub value: String,
    pub value_span: ByteSpan,
    pub reference_count: usize,
    pub can_inline_without_duplication: bool,
    pub risks: Vec<&'static str>,
}

#[derive(Debug, Clone)]
pub struct LetReportPolicy {
    pub fail_on_duplicate_evaluation: bool,
    pub fail_on_unused_binding: bool,
    pub require_inlineable_bindings: Option<usize>,
    pub binding_count: usize,
    pub inlineable_binding_count: usize,
    pub unused_binding_count: usize,
    pub duplicate_evaluation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

#[must_use]
pub fn evaluate_let_report_policy(
    reports: &[LetFormReport],
    options: &LetReportPolicyOptions,
) -> LetReportPolicy {
    let mut binding_count = 0;
    let mut inlineable_binding_count = 0;
    let mut unused_binding_count = 0;
    let mut duplicate_evaluation_count = 0;

    for binding in reports.iter().flat_map(|report| &report.bindings) {
        binding_count += 1;
        if binding.can_inline_without_duplication {
            inlineable_binding_count += 1;
        }
        if binding.risks.contains(&"unused-binding") {
            unused_binding_count += 1;
        }
        if binding.risks.contains(&"duplicate-evaluation") {
            duplicate_evaluation_count += 1;
        }
    }

    let mut violations = Vec::new();
    if options.fail_on_duplicate_evaluation() && duplicate_evaluation_count > 0 {
        violations.push(format!(
            "duplicate_evaluation_count {duplicate_evaluation_count} exceeds 0"
        ));
    }
    if options.fail_on_unused_binding() && unused_binding_count > 0 {
        violations.push(format!(
            "unused_binding_count {unused_binding_count} exceeds 0"
        ));
    }
    if let Some(required) = options.require_inlineable_bindings() {
        if inlineable_binding_count < required {
            violations.push(format!(
                "inlineable_binding_count {inlineable_binding_count} is below required {required}"
            ));
        }
    }

    LetReportPolicy {
        fail_on_duplicate_evaluation: options.fail_on_duplicate_evaluation(),
        fail_on_unused_binding: options.fail_on_unused_binding(),
        require_inlineable_bindings: options.require_inlineable_bindings(),
        binding_count,
        inlineable_binding_count,
        unused_binding_count,
        duplicate_evaluation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_inlineable_binding_threshold() {
        assert!(LetReportPolicyOptions::new(true, false, Some(1)).is_ok());
        assert_eq!(
            LetReportPolicyOptions::new(false, true, Some(0)).unwrap_err(),
            "require-inlineable-bindings must be greater than zero"
        );
    }
}
