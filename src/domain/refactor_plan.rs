#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefactorOperation {
    Rename,
    Remove,
    Move,
    Signature,
}

impl RefactorOperation {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Rename => "rename",
            Self::Remove => "remove",
            Self::Move => "move",
            Self::Signature => "signature",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefactorPlanTargetKind {
    Callable,
    Macro,
    CompilerMacro,
    SetfExpander,
    SymbolMacro,
    Unknown,
}

impl RefactorPlanTargetKind {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Callable => "callable",
            Self::Macro => "macro",
            Self::CompilerMacro => "compiler_macro",
            Self::SetfExpander => "setf_expander",
            Self::SymbolMacro => "symbol_macro",
            Self::Unknown => "unknown",
        }
    }

    #[must_use]
    pub const fn is_macro_like(self) -> bool {
        matches!(
            self,
            Self::Macro | Self::CompilerMacro | Self::SetfExpander | Self::SymbolMacro
        )
    }

    #[must_use]
    pub const fn skips_signature_compatibility(self) -> bool {
        self.is_macro_like()
    }

    #[must_use]
    pub const fn requires_call_coverage(self, operation: RefactorOperation) -> bool {
        match operation {
            RefactorOperation::Rename | RefactorOperation::Move => {
                !matches!(self, Self::SymbolMacro)
            }
            RefactorOperation::Signature => !self.skips_signature_compatibility(),
            RefactorOperation::Remove => true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerificationPhase {
    Pre,
    Post,
}

impl VerificationPhase {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Pre => "pre",
            Self::Post => "post",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RefactorRiskLevel {
    Info,
    Warning,
    Error,
}

impl RefactorRiskLevel {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RefactorPlanSummary {
    pub file_count: usize,
    pub definition_count: usize,
    pub reference_count: usize,
    pub call_count: usize,
    pub inbound_edge_count: usize,
    pub outbound_edge_count: usize,
    pub non_call_reference_count: usize,
    pub signature_mismatch_count: usize,
    pub safe_to_automate: bool,
}

#[derive(Debug, Clone)]
pub struct RefactorPlanGate {
    pub level: RefactorRiskLevel,
    pub code: &'static str,
    pub message: String,
    pub count: usize,
    pub blocks_automation: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RefactorPlanRiskSummary {
    pub highest_level: Option<RefactorRiskLevel>,
    pub info_count: usize,
    pub warning_count: usize,
    pub error_count: usize,
    pub blocking_count: usize,
    pub advisory_count: usize,
}

impl RefactorPlanRiskSummary {
    #[must_use]
    pub fn from_gates(gates: &[RefactorPlanGate]) -> Self {
        let mut summary = Self {
            highest_level: None,
            info_count: 0,
            warning_count: 0,
            error_count: 0,
            blocking_count: 0,
            advisory_count: 0,
        };

        for gate in gates {
            summary.highest_level = Some(match summary.highest_level {
                Some(level) => level.max(gate.level),
                None => gate.level,
            });
            match gate.level {
                RefactorRiskLevel::Info => summary.info_count += gate.count,
                RefactorRiskLevel::Warning => summary.warning_count += gate.count,
                RefactorRiskLevel::Error => summary.error_count += gate.count,
            }
            if gate.blocks_automation {
                summary.blocking_count += gate.count;
            } else {
                summary.advisory_count += gate.count;
            }
        }
        summary
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RefactorPlanPolicyOptions {
    fail_on_blocking_gate: bool,
    require_definitions: Option<usize>,
    require_references: Option<usize>,
}

impl RefactorPlanPolicyOptions {
    pub fn new(
        fail_on_blocking_gate: bool,
        require_definitions: Option<usize>,
        require_references: Option<usize>,
    ) -> Result<Self, String> {
        for (name, value) in [
            ("--require-definitions", require_definitions),
            ("--require-references", require_references),
        ] {
            if value == Some(0) {
                return Err(format!("{name} must be greater than zero when specified"));
            }
        }
        Ok(Self {
            fail_on_blocking_gate,
            require_definitions,
            require_references,
        })
    }

    #[must_use]
    pub const fn fail_on_blocking_gate(self) -> bool {
        self.fail_on_blocking_gate
    }
    #[must_use]
    pub const fn require_definitions(self) -> Option<usize> {
        self.require_definitions
    }
    #[must_use]
    pub const fn require_references(self) -> Option<usize> {
        self.require_references
    }
}

#[derive(Debug, Clone)]
pub struct RefactorPlanPolicy {
    pub fail_on_blocking_gate: bool,
    pub require_definitions: Option<usize>,
    pub require_references: Option<usize>,
    pub blocking_gate_count: usize,
    pub definition_count: usize,
    pub reference_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct RefactorVerificationCheck {
    pub code: &'static str,
    pub level: RefactorRiskLevel,
    pub passed: bool,
    pub message: String,
    pub count: usize,
}

#[derive(Debug, Clone)]
pub struct RefactorVerificationRequest<'a> {
    pub operation: RefactorOperation,
    pub phase: VerificationPhase,
    pub symbol: &'a str,
    pub new_symbol: Option<&'a str>,
    pub target_kind: RefactorPlanTargetKind,
    pub before: RefactorPlanSummary,
    pub after: Option<RefactorPlanSummary>,
}

#[derive(Debug, Clone)]
pub struct RawRefactorRisk {
    pub level: RefactorRiskLevel,
    pub code: &'static str,
    pub message: String,
    pub count: usize,
}

#[must_use]
pub fn refactor_plan_gates(
    operation: RefactorOperation,
    target_kind: RefactorPlanTargetKind,
    summary: &RefactorPlanSummary,
    risks: Vec<RawRefactorRisk>,
) -> Vec<RefactorPlanGate> {
    let mut gates = risks
        .into_iter()
        .map(|risk| {
            let blocks_automation = risk.level == RefactorRiskLevel::Error
                || match operation {
                    RefactorOperation::Rename => match risk.code {
                        "ambiguous-definition" => true,
                        "signature-mismatch" => !target_kind.skips_signature_compatibility(),
                        _ => false,
                    },
                    RefactorOperation::Remove | RefactorOperation::Move => {
                        matches!(risk.code, "inbound-callers" | "ambiguous-definition")
                    }
                    RefactorOperation::Signature => {
                        matches!(
                            risk.code,
                            "inbound-callers"
                                | "non-call-references"
                                | "signature-mismatch"
                                | "ambiguous-definition"
                        ) && !(risk.code == "signature-mismatch"
                            && target_kind.skips_signature_compatibility())
                    }
                };
            RefactorPlanGate {
                level: risk.level,
                code: risk.code,
                message: risk.message,
                count: risk.count,
                blocks_automation,
            }
        })
        .collect::<Vec<_>>();

    if operation == RefactorOperation::Remove && summary.reference_count > summary.definition_count
    {
        gates.push(RefactorPlanGate {
            level: RefactorRiskLevel::Warning,
            code: "external-references",
            message: "The symbol has references outside its own definition; removal needs caller and reference cleanup.".to_owned(),
            count: summary.reference_count.saturating_sub(summary.definition_count),
            blocks_automation: true,
        });
    }
    gates
}

#[must_use]
pub fn evaluate_refactor_plan_policy(
    options: RefactorPlanPolicyOptions,
    summary: &RefactorPlanSummary,
    gates: &[RefactorPlanGate],
) -> RefactorPlanPolicy {
    let mut violations = Vec::new();
    let blocking_gate_count = gates.iter().filter(|gate| gate.blocks_automation).count();
    if options.fail_on_blocking_gate() && blocking_gate_count > 0 {
        violations.push(format!(
            "--fail-on-blocking-gate found {blocking_gate_count} blocking gate(s)"
        ));
    }
    if let Some(required) = options.require_definitions() {
        if summary.definition_count < required {
            violations.push(format!(
                "--require-definitions expected at least {required}, found {}",
                summary.definition_count
            ));
        }
    }
    if let Some(required) = options.require_references() {
        if summary.reference_count < required {
            violations.push(format!(
                "--require-references expected at least {required}, found {}",
                summary.reference_count
            ));
        }
    }
    RefactorPlanPolicy {
        fail_on_blocking_gate: options.fail_on_blocking_gate(),
        require_definitions: options.require_definitions(),
        require_references: options.require_references(),
        blocking_gate_count,
        definition_count: summary.definition_count,
        reference_count: summary.reference_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[must_use]
pub fn refactor_verification_checks(
    request: RefactorVerificationRequest<'_>,
    gates: &[RefactorPlanGate],
) -> Vec<RefactorVerificationCheck> {
    let mut checks = Vec::new();
    match request.phase {
        VerificationPhase::Pre => {
            checks.push(RefactorVerificationCheck {
                code: "preflight-gates",
                level: RefactorRiskLevel::Error,
                passed: request.before.safe_to_automate,
                message: if request.before.safe_to_automate {
                    "No blocking refactor gates were found.".to_owned()
                } else {
                    "Blocking refactor gates were found; inspect gates before automated editing."
                        .to_owned()
                },
                count: gates.iter().filter(|gate| gate.blocks_automation).count(),
            });
            for gate in gates {
                checks.push(RefactorVerificationCheck {
                    code: gate.code,
                    level: gate.level,
                    passed: !gate.blocks_automation,
                    message: gate.message.clone(),
                    count: gate.count,
                });
            }
        }
        VerificationPhase::Post => {
            if matches!(
                request.operation,
                RefactorOperation::Rename | RefactorOperation::Remove
            ) {
                checks.push(RefactorVerificationCheck {
                    code: "old-symbol-removed", level: RefactorRiskLevel::Error,
                    passed: request.before.reference_count == 0 && request.before.definition_count == 0,
                    message: format!("Old symbol `{}` must have no remaining definitions or references after the refactor.", request.symbol),
                    count: request.before.reference_count + request.before.definition_count,
                });
            }
            if request.operation == RefactorOperation::Move {
                checks.push(RefactorVerificationCheck {
                    code: "moved-symbol-present",
                    level: RefactorRiskLevel::Error,
                    passed: request
                        .after
                        .map(|after| after.definition_count > 0)
                        .unwrap_or(false),
                    message: format!(
                        "Moved symbol `{}` must still have a discovered definition after the move.",
                        request.symbol
                    ),
                    count: request
                        .after
                        .map(|after| after.definition_count)
                        .unwrap_or(0),
                });
            }
            if request.operation == RefactorOperation::Rename {
                match (request.new_symbol, request.after) {
                    (Some(new_symbol), Some(after)) => {
                        checks.push(RefactorVerificationCheck {
                            code: "new-symbol-present", level: RefactorRiskLevel::Error,
                            passed: after.definition_count > 0 && after.reference_count > 0,
                            message: format!("New symbol `{new_symbol}` must have at least one definition and one reference."),
                            count: after.reference_count + after.definition_count,
                        });
                        if !request.target_kind.skips_signature_compatibility() {
                            checks.push(RefactorVerificationCheck { code: "new-symbol-signature-compatible", level: RefactorRiskLevel::Error, passed: after.signature_mismatch_count == 0, message: "New symbol call sites must match discovered callable definitions.".to_owned(), count: after.signature_mismatch_count });
                        }
                    }
                    _ => checks.push(RefactorVerificationCheck { code: "new-symbol-required", level: RefactorRiskLevel::Error, passed: false, message: "Post-rename verification requires --new-symbol to inspect replacement impact.".to_owned(), count: 1 }),
                }
            }
            if request.operation == RefactorOperation::Signature
                && !request.target_kind.skips_signature_compatibility()
            {
                checks.push(RefactorVerificationCheck {
                    code: "signature-compatible",
                    level: RefactorRiskLevel::Error,
                    passed: request.before.signature_mismatch_count == 0,
                    message:
                        "Signature refactor must leave every discovered call arity-compatible."
                            .to_owned(),
                    count: request.before.signature_mismatch_count,
                });
            }
        }
    }
    checks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_options_reject_zero_thresholds() {
        assert!(RefactorPlanPolicyOptions::new(false, Some(0), None).is_err());
        assert!(RefactorPlanPolicyOptions::new(false, None, Some(0)).is_err());
        assert!(RefactorPlanPolicyOptions::new(true, Some(1), Some(1)).is_ok());
    }
}
