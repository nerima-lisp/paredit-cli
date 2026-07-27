use std::collections::BTreeMap;
use std::path::PathBuf;
use std::str::FromStr;

use anyhow::Result;

use crate::domain::call_graph_report::{
    CallGraphNode, CallGraphNodeIndex, build_call_graph_edge, call_graph_edge_matches,
    insert_call_graph_node,
};
use crate::domain::call_report::{CallReportItem, build_call_report};
use crate::domain::common_lisp::common_lisp_symbol_reference_needle;
use crate::domain::dialect::Dialect;
use crate::domain::sexpr::SymbolName;
use crate::domain::signature_report::{SignatureCallItem, classify_signature_call};

mod definitions;
mod identity;
mod references;
mod summary;
mod syntax;
mod types;

use definitions::{collect_impact_definitions, impact_definition_matches_signature};
use identity::SymbolIdentity;
use references::{count_non_call_references, matching_symbol_occurrences};

pub use summary::{
    impact_risks, impact_status_counts, raw_refactor_risks, summarize_impact_reports,
};
pub use types::{
    ImpactDefinitionItem, ImpactReportFile, ImpactReportSource, ImpactSymbolOccurrence,
    ImpactSymbolOccurrenceContext,
};

use crate::domain::refactor_plan::{RefactorPlanSummary, RefactorRiskLevel};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ImpactRiskLevel {
    Info,
    Warning,
    Error,
}

// Lives here, not in `refactor_plan`, so the dependency runs one way only.
// `refactor_plan` is core and `impact_report` is a feature-level report, so
// core must not name it. The orphan rule permits the impl here because
// `ImpactRiskLevel` is local to this module's crate.
impl From<ImpactRiskLevel> for RefactorRiskLevel {
    fn from(value: ImpactRiskLevel) -> Self {
        match value {
            ImpactRiskLevel::Info => Self::Info,
            ImpactRiskLevel::Warning => Self::Warning,
            ImpactRiskLevel::Error => Self::Error,
        }
    }
}

impl ImpactRiskLevel {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }
}

impl FromStr for ImpactRiskLevel {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "info" => Ok(Self::Info),
            "warning" => Ok(Self::Warning),
            "error" => Ok(Self::Error),
            _ => Err(format!("unknown impact risk level: {value}")),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ImpactReportPolicyOptions {
    fail_on_risk_level: Option<ImpactRiskLevel>,
    require_definitions: Option<usize>,
    require_references: Option<usize>,
    require_calls: Option<usize>,
}

impl ImpactReportPolicyOptions {
    pub fn new(
        fail_on_risk_level: Option<ImpactRiskLevel>,
        require_definitions: Option<usize>,
        require_references: Option<usize>,
        require_calls: Option<usize>,
    ) -> Result<Self, String> {
        Self::validate_threshold("require-definitions", require_definitions)?;
        Self::validate_threshold("require-references", require_references)?;
        Self::validate_threshold("require-calls", require_calls)?;

        Ok(Self {
            fail_on_risk_level,
            require_definitions,
            require_references,
            require_calls,
        })
    }

    fn validate_threshold(label: &str, value: Option<usize>) -> Result<(), String> {
        if matches!(value, Some(0)) {
            return Err(format!("{label} must be greater than zero"));
        }
        Ok(())
    }

    #[must_use]
    pub const fn fail_on_risk_level(self) -> Option<ImpactRiskLevel> {
        self.fail_on_risk_level
    }

    #[must_use]
    pub const fn require_definitions(self) -> Option<usize> {
        self.require_definitions
    }

    #[must_use]
    pub const fn require_references(self) -> Option<usize> {
        self.require_references
    }

    #[must_use]
    pub const fn require_calls(self) -> Option<usize> {
        self.require_calls
    }
}

#[derive(Debug)]
pub struct ImpactRisk {
    pub level: ImpactRiskLevel,
    pub code: &'static str,
    pub message: String,
    pub count: usize,
}

#[derive(Debug)]
pub struct ImpactReportPolicy {
    pub fail_on_risk_level: Option<ImpactRiskLevel>,
    pub require_definitions: Option<usize>,
    pub require_references: Option<usize>,
    pub require_calls: Option<usize>,
    pub definition_count: usize,
    pub reference_count: usize,
    pub call_count: usize,
    pub inbound_edge_count: usize,
    pub non_call_reference_count: usize,
    pub signature_mismatch_count: usize,
    pub risk_level: ImpactRiskLevel,
    pub passed: bool,
    pub violations: Vec<String>,
}

#[must_use]
pub fn evaluate_impact_report_policy(
    options: ImpactReportPolicyOptions,
    summary: &RefactorPlanSummary,
    risk_level: ImpactRiskLevel,
) -> ImpactReportPolicy {
    let mut violations = Vec::new();

    if let Some(threshold) = options.fail_on_risk_level() {
        if risk_level >= threshold {
            violations.push(format!(
                "--fail-on-risk-level {} failed with {} risk",
                threshold.label(),
                risk_level.label()
            ));
        }
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
    if let Some(required) = options.require_calls() {
        if summary.call_count < required {
            violations.push(format!(
                "--require-calls expected at least {required}, found {}",
                summary.call_count
            ));
        }
    }

    ImpactReportPolicy {
        fail_on_risk_level: options.fail_on_risk_level(),
        require_definitions: options.require_definitions(),
        require_references: options.require_references(),
        require_calls: options.require_calls(),
        definition_count: summary.definition_count,
        reference_count: summary.reference_count,
        call_count: summary.call_count,
        inbound_edge_count: summary.inbound_edge_count,
        non_call_reference_count: summary.non_call_reference_count,
        signature_mismatch_count: summary.signature_mismatch_count,
        risk_level,
        passed: violations.is_empty(),
        violations,
    }
}

/// One parsed source, held until every file's definitions are known.
///
/// A call edge can only be classified once `nodes_by_name` has seen every
/// file, so the per-file work is split across two passes.
struct ParsedSource {
    path: PathBuf,
    dialect: Dialect,
    package: Option<String>,
    definitions: Vec<ImpactDefinitionItem>,
    references: Vec<ImpactSymbolOccurrence>,
    calls: Vec<CallReportItem>,
    all_calls: Vec<CallReportItem>,
    identity: SymbolIdentity,
}

pub fn build_impact_reports(
    sources: Vec<ImpactReportSource>,
    symbol: &SymbolName,
) -> Result<Vec<ImpactReportFile>> {
    let mut parsed = Vec::with_capacity(sources.len());
    let mut nodes_by_name = BTreeMap::<String, CallGraphNode>::new();
    let mut node_index = CallGraphNodeIndex::new();
    let mut definitions_by_name = BTreeMap::<String, Vec<(usize, Option<usize>)>>::new();

    for source in sources {
        let identity = SymbolIdentity::new(source.dialect, &source.tree, symbol);
        let outline = source
            .tree
            .outline(|head| source.dialect.is_definition_head(head));
        let (package, all_definitions) = collect_impact_definitions(&source.tree, source.dialect)?;
        let references = matching_symbol_occurrences(source.dialect, &source.tree, &identity)
            .into_iter()
            .map(|occurrence| ImpactSymbolOccurrence {
                path: occurrence.path.to_string(),
                span: occurrence.span,
                context: outline
                    .iter()
                    .filter(|entry| entry.span.contains_span(occurrence.span))
                    .min_by_key(|entry| entry.span.end().get() - entry.span.start().get())
                    .map(|entry| ImpactSymbolOccurrenceContext {
                        path: entry.path.to_string(),
                        span: entry.span,
                        head: entry.head.clone(),
                        definition_like: entry.definition_like,
                    }),
            })
            .collect::<Vec<_>>();
        let definitions = all_definitions
            .iter()
            .filter(|definition| {
                definition
                    .name
                    .as_deref()
                    .is_some_and(|name| identity.matches(name, definition.span.start().get()))
            })
            .cloned()
            .collect::<Vec<_>>();
        // `build_call_report` matches heads by bare name, so a call the symbol's
        // package rules out still has to be dropped here.
        let calls = build_call_report(&source.tree, source.dialect, Some(symbol), false)?
            .into_iter()
            .filter(|call| identity.matches(&call.head, call.span.start().get()))
            .collect::<Vec<_>>();
        let all_calls = build_call_report(&source.tree, source.dialect, None, false)?;

        for definition in &all_definitions {
            insert_call_graph_node(
                &mut nodes_by_name,
                &mut node_index,
                definition.name.as_deref(),
                definition.category,
            );

            if impact_definition_matches_signature(definition, None) {
                if let (Some(name), Some(arity)) = (&definition.name, definition.parameter_arity) {
                    definitions_by_name
                        .entry(common_lisp_symbol_reference_needle(name))
                        .or_default()
                        .push(arity);
                }
            }
        }

        parsed.push(ParsedSource {
            path: source.path,
            dialect: source.dialect,
            package,
            definitions,
            references,
            calls,
            all_calls,
            identity,
        });
    }

    Ok(parsed
        .into_iter()
        .map(|source| {
            let ParsedSource {
                path,
                dialect,
                package,
                definitions,
                references,
                calls,
                all_calls,
                identity,
            } = source;
            let calls = calls
                .into_iter()
                .map(|call| {
                    let (expected_parameter_arity, status) =
                        classify_signature_call(&definitions_by_name, &call);
                    SignatureCallItem {
                        call,
                        expected_parameter_arity,
                        status,
                    }
                })
                .collect::<Vec<_>>();
            let edges = all_calls
                .into_iter()
                .map(|call| build_call_graph_edge(call, &nodes_by_name, &node_index))
                .filter(|edge| call_graph_edge_matches(edge, Some(symbol)))
                .collect::<Vec<_>>();
            // An edge's span sits inside its caller, so one offset resolves
            // both ends against the same `in-package` region.
            let inbound_edges = edges
                .iter()
                .filter(|edge| identity.matches(&edge.callee, edge.span.start().get()))
                .cloned()
                .collect::<Vec<_>>();
            let outbound_edges = edges
                .iter()
                .filter(|edge| {
                    edge.caller
                        .as_deref()
                        .is_some_and(|caller| identity.matches(caller, edge.span.start().get()))
                })
                .cloned()
                .collect::<Vec<_>>();
            let non_call_reference_count =
                count_non_call_references(&path, &references, &definitions, &calls);

            ImpactReportFile::new(
                path,
                dialect,
                package,
                definitions,
                references,
                calls,
                inbound_edges,
                outbound_edges,
                non_call_reference_count,
            )
        })
        .collect::<Vec<_>>())
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::domain::sexpr::SyntaxTree;

    const APP: &str = "(in-package :app)\n(defun run () 1)\n(defun app-caller () (run))";
    const TEST: &str = "(in-package :test)\n(defun run () 2)\n(defun test-caller () (run))";

    fn source(path: &str, input: &str) -> ImpactReportSource {
        ImpactReportSource::new(
            PathBuf::from(path),
            Dialect::CommonLisp,
            SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse"),
        )
    }

    fn reports(sources: Vec<ImpactReportSource>, symbol: &str) -> Vec<ImpactReportFile> {
        build_impact_reports(sources, &SymbolName::new(symbol).expect("symbol")).expect("report")
    }

    #[test]
    fn a_qualified_symbol_leaves_the_same_name_in_another_package_alone() {
        let files = reports(
            vec![source("app.lisp", APP), source("test.lisp", TEST)],
            "app:run",
        );

        assert_eq!(files[0].definitions.len(), 1);
        assert_eq!(files[0].calls.len(), 1);
        assert_eq!(files[0].references.len(), 1);
        assert_eq!(files[0].inbound_edges.len(), 1);
        assert!(files[1].definitions.is_empty());
        assert!(files[1].calls.is_empty());
        assert!(files[1].references.is_empty());
        assert!(files[1].inbound_edges.is_empty());
        assert_eq!(summarize_impact_reports(&files).definition_count, 1);
    }

    #[test]
    fn an_unqualified_symbol_still_matches_every_package() {
        // The queried symbol comes from a command line with no `in-package` to
        // read it in, so narrowing it to one package would invent a package the
        // caller never named and drop real callers.
        let files = reports(
            vec![source("app.lisp", APP), source("test.lisp", TEST)],
            "run",
        );

        assert_eq!(summarize_impact_reports(&files).definition_count, 2);
        assert_eq!(files[0].calls.len(), 1);
        assert_eq!(files[1].calls.len(), 1);
    }

    #[test]
    fn a_qualified_reference_from_a_third_package_reaches_the_definition() {
        let files = reports(
            vec![
                source("app.lisp", APP),
                source(
                    "other.lisp",
                    "(in-package :other)\n(defun other-caller () (app:run 1))",
                ),
            ],
            "app:run",
        );

        assert_eq!(files[1].calls.len(), 1);
        assert_eq!(files[1].calls[0].call.head, "app:run");
        assert_eq!(files[1].inbound_edges.len(), 1);
    }

    #[test]
    fn a_file_without_in_package_matches_by_name_as_before() {
        // The reader's current package here depends on the build, so the report
        // has to keep giving its bare-name answer.
        let unpackaged = "(defun run () 1)\n(defun caller () (run))";
        let qualified = reports(vec![source("plain.lisp", unpackaged)], "app:run");
        let bare = reports(vec![source("plain.lisp", unpackaged)], "run");

        assert_eq!(qualified[0].definitions.len(), 1);
        assert_eq!(qualified[0].calls.len(), 1);
        assert_eq!(qualified[0].references.len(), bare[0].references.len());
        assert_eq!(qualified[0].calls.len(), bare[0].calls.len());
        assert_eq!(
            qualified[0].inbound_edges.len(),
            bare[0].inbound_edges.len()
        );
    }

    #[test]
    fn labels_are_stable() {
        assert_eq!(ImpactRiskLevel::Info.label(), "info");
        assert_eq!(ImpactRiskLevel::Warning.label(), "warning");
        assert_eq!(ImpactRiskLevel::Error.label(), "error");
    }

    #[test]
    fn validates_thresholds() {
        assert!(ImpactReportPolicyOptions::new(None, Some(1), Some(2), Some(3)).is_ok());
        assert_eq!(
            ImpactReportPolicyOptions::new(None, Some(0), None, None).unwrap_err(),
            "require-definitions must be greater than zero"
        );
        assert_eq!(
            ImpactReportPolicyOptions::new(None, None, Some(0), None).unwrap_err(),
            "require-references must be greater than zero"
        );
        assert_eq!(
            ImpactReportPolicyOptions::new(None, None, None, Some(0)).unwrap_err(),
            "require-calls must be greater than zero"
        );
    }

    #[test]
    fn evaluates_policy_failures() {
        let summary = RefactorPlanSummary {
            file_count: 1,
            definition_count: 0,
            reference_count: 1,
            call_count: 0,
            inbound_edge_count: 0,
            outbound_edge_count: 0,
            non_call_reference_count: 1,
            signature_mismatch_count: 0,
            safe_to_automate: false,
        };

        let policy = evaluate_impact_report_policy(
            ImpactReportPolicyOptions::new(
                Some(ImpactRiskLevel::Warning),
                Some(1),
                Some(2),
                Some(1),
            )
            .unwrap(),
            &summary,
            ImpactRiskLevel::Error,
        );

        assert!(!policy.passed);
        assert_eq!(policy.violations.len(), 4);
    }
}
