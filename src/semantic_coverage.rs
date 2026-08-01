//! Measuring how much of the semantic layer resolves on real source.
//!
//! `paredit-core-semantics` is conservative by design — it says `Known` only
//! when it can prove something, and stays silent otherwise. That discipline
//! is only worth its cost if it actually resolves a useful fraction of real
//! code, and *why* it does not resolve the rest is what decides where the
//! next round of work should go. This use case answers both questions: it
//! builds the binding and value tables for each file and counts, rather than
//! asserts, what they found.
//!
//! Discovery is a source-port responsibility, mirroring
//! [`paredit_feature_similarity::similarity_report::usecase`]: this module only knows
//! how to turn bytes into a report, not how paths become files.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use paredit_core_semantics::semantics::binding::{
    Binding, BindingKind, BindingTable, OpacityCauseKind, build_binding_table,
};
use paredit_core_semantics::semantics::project::GlobalTable;
use paredit_core_semantics::semantics::project::service::{
    FilePackages, ProjectFile, build_global_table, resolve_file_packages,
};
use paredit_core_semantics::semantics::value::{
    ProjectConstants, ValueTable, build_value_table, build_value_table_in_project,
    evaluate_constant,
};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionKind, SyntaxTree};
use paredit_core_syntax::view_query::for_each_subview;

/// The files to measure. Discovery (walking directories, filtering
/// extensions) happens behind [`SemanticCoverageSourcePort`]; this only names
/// the roots the caller asked about.
#[derive(Debug, Clone, Default)]
pub struct SemanticCoverageRequest {
    pub paths: Vec<PathBuf>,
    /// Forces every file to the same dialect, the way a CLI `--dialect` flag
    /// would. `None` detects each file from its own extension (and, for a
    /// `.scm`/extensionless file, its `#lang` line), so a corpus of mixed
    /// dialects measures each file under its own grammar rather than all
    /// under one guessed dialect.
    pub dialect: Option<Dialect>,
}

/// One file the source discovered, ready to be loaded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredSemanticCoverageFile {
    pub path: PathBuf,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SemanticCoverageInventory {
    pub files: Vec<DiscoveredSemanticCoverageFile>,
}

/// Turns a request into loadable files, and files into bytes.
///
/// A source may discover files of any dialect: the workflow measures each
/// file under its own detected (or overridden) dialect, and
/// [`SemanticCoverageReport::by_dialect`] is exactly what makes the resulting
/// zeroes for a dialect the semantic layer does not model yet legible, rather
/// than a reason to filter that dialect out before measuring.
pub trait SemanticCoverageSourcePort {
    /// What this adapter's own failures look like.
    ///
    /// An associated type rather than `anyhow::Result`, for the reason given
    /// on every other port in this workspace: the use case must not know what
    /// an adapter can fail with, and `anyhow::Error` does not say that — it
    /// says "no error type at all", and takes the classification with it.
    type Error: Into<paredit_core_cli::CliError>;

    fn discover(
        &mut self,
        request: &SemanticCoverageRequest,
    ) -> Result<SemanticCoverageInventory, Self::Error>;

    fn load(&self, file: &DiscoveredSemanticCoverageFile) -> Result<Vec<u8>, String>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticCoverageProcessingStage {
    Read,
    Decode,
    Parse,
}

impl SemanticCoverageProcessingStage {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Decode => "decode",
            Self::Parse => "parse",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticCoverageFileError {
    pub path: PathBuf,
    pub stage: SemanticCoverageProcessingStage,
    pub message: String,
}

#[derive(Debug)]
pub enum SemanticCoverageWorkflowError {
    /// Whatever the source port's adapter failed with.
    ///
    /// The port still does not enumerate its adapters' failures — see
    /// [`SemanticCoverageSourcePort::Error`] — but a `CliError` still carries
    /// a classification, which an `anyhow::Error` did not.
    Source(Box<paredit_core_cli::CliError>),
}

impl std::fmt::Display for SemanticCoverageWorkflowError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Source(_) => formatter.write_str("semantic coverage source failed"),
        }
    }
}

impl std::error::Error for SemanticCoverageWorkflowError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Source(error) => Some(error.as_ref()),
        }
    }
}

/// Why a `Variable` binding's constant value was not in the value table.
///
/// The variants mirror the checks
/// [`Binding::is_propagatable`](paredit_core_semantics::semantics::binding::Binding::is_propagatable)
/// makes, in the same order, so a binding falls into exactly the bucket that
/// explains the first disqualifying fact about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BindingNonResolutionReason {
    /// `setq`/`incf`/`push`/… reassigns it somewhere in its scope.
    Reassigned,
    /// Its scope contains an unknown macro call, quoted data, or a
    /// reader-conditional region.
    OpaqueScope,
    /// It is declared special, so a lexical read cannot be trusted.
    Special,
    /// The binder gave it no initial form to propagate.
    NoInitialForm,
    /// Every other condition held, but the initial form did not fold at all —
    /// an unregistered operator or an unresolved sub-reference.
    InitialFormNotConstant,
    /// The initial form folded to a `Known` value that cannot be substituted
    /// for a reference: a string, whose contents are mutable through
    /// `(setf (char s 0) …)`, or a float, whose printed form is not its value.
    ///
    /// Split out from [`Self::InitialFormNotConstant`] because it is a
    /// deliberate refusal rather than a gap in the folder. Widening the folder
    /// would not move a single binding out of this bucket, so counting the two
    /// together would misdirect the next round of work.
    InitialFormNotPropagatable,
}

/// One opaque region, as a corpus histogram counts them.
///
/// The split is the whole point of the histogram: only an unregistered *name*
/// is something a transparency-table entry could ever remove. Quoted data and
/// reader dispatch are structural — no table entry makes them readable — so
/// they are counted apart rather than crowding the ranking of heads worth
/// looking at.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum OpacityCauseLabel {
    /// The head that no table registered, case-folded the way Common Lisp
    /// reads symbols. The package prefix is kept: `app:helper` and `helper`
    /// are different names.
    UnknownHead(String),
    /// A region no name could describe.
    Structural(OpacityCauseKind),
}

impl OpacityCauseLabel {
    #[must_use]
    pub fn display(&self) -> String {
        match self {
            Self::UnknownHead(head) => head.clone(),
            Self::Structural(kind) => format!("<{}>", kind.label()),
        }
    }
}

/// A count of unresolved `Variable` bindings, broken down by the first
/// disqualifying fact about each one.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BindingNonResolutionBreakdown {
    reassigned: usize,
    opaque_scope: usize,
    special: usize,
    no_initial_form: usize,
    initial_form_not_constant: usize,
    initial_form_not_propagatable: usize,
    opacity_causes: BTreeMap<OpacityCauseLabel, usize>,
    uninitialized_binders: BTreeMap<Option<String>, usize>,
}

impl BindingNonResolutionBreakdown {
    #[must_use]
    pub const fn reassigned(&self) -> usize {
        self.reassigned
    }

    #[must_use]
    pub const fn opaque_scope(&self) -> usize {
        self.opaque_scope
    }

    #[must_use]
    pub const fn special(&self) -> usize {
        self.special
    }

    #[must_use]
    pub const fn no_initial_form(&self) -> usize {
        self.no_initial_form
    }

    #[must_use]
    pub const fn initial_form_not_constant(&self) -> usize {
        self.initial_form_not_constant
    }

    #[must_use]
    pub const fn initial_form_not_propagatable(&self) -> usize {
        self.initial_form_not_propagatable
    }

    /// What made each opaque scope opaque, counted once per binding.
    ///
    /// The counts sum to [`Self::opaque_scope`]: every binding contributes the
    /// first cause recorded for it and no more, so a head's count reads as
    /// "bindings this form alone is blocking", not "times the form appears".
    #[must_use]
    pub const fn opacity_causes(&self) -> &BTreeMap<OpacityCauseLabel, usize> {
        &self.opacity_causes
    }

    /// Which binder left each uninitialized binding without a value.
    ///
    /// Separates the structural ceiling from a real gap: a `defun` parameter
    /// has no initial form because a caller supplies it, and no amount of
    /// analysis changes that, whereas a `let` with no value form is ordinary
    /// code the layer simply declines to follow.
    ///
    /// `None` keys a binding with no binding operator at all — a definition's
    /// own name — rather than a made-up head, so "recognized with no binder"
    /// stays distinguishable from "bound by a form literally named that".
    #[must_use]
    pub const fn uninitialized_binders(&self) -> &BTreeMap<Option<String>, usize> {
        &self.uninitialized_binders
    }

    /// [`Self::opacity_causes`] with the largest counts first.
    ///
    /// Ties break on the label so two runs over the same corpus print the same
    /// ranking: a measurement whose output reorders between runs cannot be
    /// diffed, which is most of what this harness is for.
    #[must_use]
    pub fn ranked_opacity_causes(&self) -> Vec<(&OpacityCauseLabel, usize)> {
        rank(&self.opacity_causes)
    }

    /// [`Self::uninitialized_binders`] with the largest counts first.
    #[must_use]
    pub fn ranked_uninitialized_binders(&self) -> Vec<(&Option<String>, usize)> {
        rank(&self.uninitialized_binders)
    }

    #[must_use]
    pub const fn total(&self) -> usize {
        self.reassigned
            + self.opaque_scope
            + self.special
            + self.no_initial_form
            + self.initial_form_not_constant
            + self.initial_form_not_propagatable
    }

    const fn record(&mut self, reason: BindingNonResolutionReason) {
        match reason {
            BindingNonResolutionReason::Reassigned => self.reassigned += 1,
            BindingNonResolutionReason::OpaqueScope => self.opaque_scope += 1,
            BindingNonResolutionReason::Special => self.special += 1,
            BindingNonResolutionReason::NoInitialForm => self.no_initial_form += 1,
            BindingNonResolutionReason::InitialFormNotConstant => {
                self.initial_form_not_constant += 1;
            }
            BindingNonResolutionReason::InitialFormNotPropagatable => {
                self.initial_form_not_propagatable += 1;
            }
        }
    }

    fn record_opacity_cause(&mut self, label: OpacityCauseLabel) {
        *self.opacity_causes.entry(label).or_default() += 1;
    }

    fn record_uninitialized_binder(&mut self, head: Option<&str>) {
        *self
            .uninitialized_binders
            .entry(head.map(str::to_ascii_lowercase))
            .or_default() += 1;
    }

    fn merge(&mut self, other: &Self) {
        self.reassigned += other.reassigned;
        self.opaque_scope += other.opaque_scope;
        self.special += other.special;
        self.no_initial_form += other.no_initial_form;
        self.initial_form_not_constant += other.initial_form_not_constant;
        self.initial_form_not_propagatable += other.initial_form_not_propagatable;
        for (label, count) in &other.opacity_causes {
            *self.opacity_causes.entry(label.clone()).or_default() += count;
        }
        for (head, count) in &other.uninitialized_binders {
            *self.uninitialized_binders.entry(head.clone()).or_default() += count;
        }
    }
}

/// Orders a histogram by descending count, breaking ties on the key.
fn rank<K: Ord>(counts: &BTreeMap<K, usize>) -> Vec<(&K, usize)> {
    let mut ranked: Vec<(&K, usize)> = counts.iter().map(|(key, count)| (key, *count)).collect();
    ranked.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(right.0)));
    ranked
}

/// What the semantic layer resolved in one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticCoverageFileReport {
    path: PathBuf,
    dialect: Dialect,
    variable_binding_count: usize,
    resolved_binding_count: usize,
    list_expression_count: usize,
    known_list_expression_count: usize,
    non_resolution: BindingNonResolutionBreakdown,
}

impl Default for SemanticCoverageFileReport {
    fn default() -> Self {
        Self {
            path: PathBuf::new(),
            dialect: Dialect::Unknown,
            variable_binding_count: 0,
            resolved_binding_count: 0,
            list_expression_count: 0,
            known_list_expression_count: 0,
            non_resolution: BindingNonResolutionBreakdown::default(),
        }
    }
}

impl SemanticCoverageFileReport {
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The dialect this file was measured under — detected from its
    /// extension (or `#lang` line), or the request's override.
    #[must_use]
    pub const fn dialect(&self) -> Dialect {
        self.dialect
    }

    /// Every `Variable`-kind binding the binding table recorded. Function,
    /// macro, symbol-macro, and slot bindings are out of scope: only a
    /// variable's *value* is something the value table resolves.
    #[must_use]
    pub const fn variable_binding_count(&self) -> usize {
        self.variable_binding_count
    }

    /// How many of those the value table gave a constant value.
    #[must_use]
    pub const fn resolved_binding_count(&self) -> usize {
        self.resolved_binding_count
    }

    /// Every `(...)` list expression in the file, at any nesting depth.
    #[must_use]
    pub const fn list_expression_count(&self) -> usize {
        self.list_expression_count
    }

    /// How many of those `evaluate_constant` folded to `Known`.
    #[must_use]
    pub const fn known_list_expression_count(&self) -> usize {
        self.known_list_expression_count
    }

    #[must_use]
    pub const fn non_resolution(&self) -> &BindingNonResolutionBreakdown {
        &self.non_resolution
    }
}

/// The measurement across every file the source discovered.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SemanticCoverageReport {
    files: Vec<SemanticCoverageFileReport>,
    errors: Vec<SemanticCoverageFileError>,
}

impl SemanticCoverageReport {
    #[must_use]
    pub fn files(&self) -> &[SemanticCoverageFileReport] {
        &self.files
    }

    /// Files the source discovered but could not be read, decoded, or
    /// parsed. Measurement continues over the rest: a corpus with a handful
    /// of unreadable files should not lose every other file's numbers.
    #[must_use]
    pub fn errors(&self) -> &[SemanticCoverageFileError] {
        &self.errors
    }

    pub fn total_variable_bindings(&self) -> usize {
        self.files
            .iter()
            .map(SemanticCoverageFileReport::variable_binding_count)
            .sum()
    }

    pub fn total_resolved_bindings(&self) -> usize {
        self.files
            .iter()
            .map(SemanticCoverageFileReport::resolved_binding_count)
            .sum()
    }

    pub fn total_list_expressions(&self) -> usize {
        self.files
            .iter()
            .map(SemanticCoverageFileReport::list_expression_count)
            .sum()
    }

    pub fn total_known_list_expressions(&self) -> usize {
        self.files
            .iter()
            .map(SemanticCoverageFileReport::known_list_expression_count)
            .sum()
    }

    #[must_use]
    pub fn total_non_resolution(&self) -> BindingNonResolutionBreakdown {
        let mut total = BindingNonResolutionBreakdown::default();
        for file in &self.files {
            total.merge(&file.non_resolution);
        }
        total
    }

    /// Coverage totals scoped to each dialect the corpus actually contained,
    /// in [`Dialect::ALL`] order.
    ///
    /// This is `R2`'s whole point: `domain::semantics` resolves only Common
    /// Lisp today, and a per-dialect breakdown makes that a number next to
    /// every other dialect's zero rather than a claim buried in a doc
    /// comment. A dialect with no discovered files is omitted rather than
    /// printed as an all-zero row, since "0/0" is not evidence of anything.
    #[must_use]
    pub fn by_dialect(&self) -> Vec<(Dialect, DialectCoverageTotals)> {
        Dialect::ALL
            .into_iter()
            .filter_map(|dialect| {
                let mut totals = DialectCoverageTotals::default();
                for file in self.files.iter().filter(|file| file.dialect == dialect) {
                    totals.file_count += 1;
                    totals.variable_bindings += file.variable_binding_count;
                    totals.resolved_bindings += file.resolved_binding_count;
                    totals.list_expressions += file.list_expression_count;
                    totals.known_list_expressions += file.known_list_expression_count;
                }
                (totals.file_count > 0).then_some((dialect, totals))
            })
            .collect()
    }
}

/// One dialect's slice of [`SemanticCoverageReport::by_dialect`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DialectCoverageTotals {
    pub file_count: usize,
    pub variable_bindings: usize,
    pub resolved_bindings: usize,
    pub list_expressions: usize,
    pub known_list_expressions: usize,
}

/// The outcome of a `--fail-under`-style gate on corpus-wide resolution.
///
/// Scoped to the total variable-binding resolution rate rather than the list-
/// expression rate: a binding either resolves or it does not, so its rate is
/// a stable target to pin a threshold to, while the list rate moves with how
/// much of a corpus is fold-eligible code versus data literals and is not a
/// meaningful regression signal on its own.
#[derive(Debug, Clone, PartialEq)]
pub struct SemanticCoveragePolicy {
    pub threshold: Option<f64>,
    pub resolved_percent: f64,
    pub passed: bool,
    pub message: Option<String>,
}

impl SemanticCoveragePolicy {
    #[must_use]
    pub fn evaluate(threshold: Option<f64>, report: &SemanticCoverageReport) -> Self {
        let total = report.total_variable_bindings();
        let resolved = report.total_resolved_bindings();
        let resolved_percent = if total == 0 {
            100.0
        } else {
            (resolved as f64 / total as f64) * 100.0
        };
        let Some(threshold) = threshold else {
            return Self {
                threshold: None,
                resolved_percent,
                passed: true,
                message: None,
            };
        };
        // An armed threshold over zero measured bindings is almost always a
        // misconfiguration — an empty corpus, a typo'd path, an accidental
        // glob that matched nothing — not evidence the corpus is fully
        // resolved. Reporting `100%` and passing silently would hide exactly
        // that mistake from CI, so this is the one case where the gate fires
        // regardless of the threshold.
        let (passed, message) = if total == 0 {
            (
                false,
                Some(
                    "no variable bindings were measured; --fail-under cannot evaluate \
                     an empty corpus"
                        .to_owned(),
                ),
            )
        } else {
            let passed = resolved_percent >= threshold;
            let message = (!passed).then(|| {
                format!(
                    "resolved {resolved}/{total} variable bindings ({resolved_percent:.1}%), \
                     below the --fail-under threshold of {threshold:.1}%"
                )
            });
            (passed, message)
        };
        Self {
            threshold: Some(threshold),
            resolved_percent,
            passed,
            message,
        }
    }
}

/// One `--fail-under-dialect DIALECT=PERCENT` request, already validated.
///
/// Private fields behind a fallible constructor rather than public settable
/// ones: `percent` alone can be "inconsistent" (negative, `NaN`, above 100),
/// and rejecting that once here means every caller — the CLI parser and any
/// future one — gets the same rejection instead of re-deriving it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DialectCoverageThreshold {
    dialect: Dialect,
    percent: f64,
}

impl DialectCoverageThreshold {
    /// Rejects a percentage that could never describe a resolution rate.
    pub fn new(dialect: Dialect, percent: f64) -> Result<Self, String> {
        if !percent.is_finite() || !(0.0..=100.0).contains(&percent) {
            return Err(format!(
                "--fail-under-dialect percentage must be a number between 0 and 100, got {percent}"
            ));
        }
        Ok(Self { dialect, percent })
    }

    #[must_use]
    pub const fn dialect(&self) -> Dialect {
        self.dialect
    }

    #[must_use]
    pub const fn percent(&self) -> f64 {
        self.percent
    }
}

/// The outcome of gating one dialect's resolution rate against a
/// [`DialectCoverageThreshold`].
///
/// Mirrors [`SemanticCoveragePolicy`], scoped to one dialect via
/// [`SemanticCoverageReport::by_dialect`] instead of the corpus-wide totals.
#[derive(Debug, Clone, PartialEq)]
pub struct DialectCoveragePolicyResult {
    dialect: Dialect,
    threshold: f64,
    resolved_percent: f64,
    passed: bool,
    message: Option<String>,
}

impl DialectCoveragePolicyResult {
    #[must_use]
    pub const fn dialect(&self) -> Dialect {
        self.dialect
    }

    #[must_use]
    pub const fn threshold(&self) -> f64 {
        self.threshold
    }

    #[must_use]
    pub const fn resolved_percent(&self) -> f64 {
        self.resolved_percent
    }

    #[must_use]
    pub const fn passed(&self) -> bool {
        self.passed
    }

    #[must_use]
    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }

    /// Evaluates one dialect threshold against the report's per-dialect
    /// totals.
    ///
    /// Mirrors [`SemanticCoveragePolicy::evaluate`]'s empty-corpus handling
    /// exactly, just scoped to one dialect: a dialect
    /// [`SemanticCoverageReport::by_dialect`] omits entirely (zero
    /// discovered files) and a dialect with files but zero measured variable
    /// bindings both fail loudly rather than trivially passing at a
    /// fabricated 100%. An armed per-dialect threshold over nothing measured
    /// is the same misconfiguration signal the corpus-wide gate already
    /// treats as a loud failure, not a silent skip.
    #[must_use]
    pub fn evaluate(threshold: DialectCoverageThreshold, report: &SemanticCoverageReport) -> Self {
        let dialect = threshold.dialect();
        let percent_threshold = threshold.percent();
        let totals = report
            .by_dialect()
            .into_iter()
            .find_map(|(found, totals)| (found == dialect).then_some(totals))
            .unwrap_or_default();

        let resolved_percent = if totals.variable_bindings == 0 {
            100.0
        } else {
            (totals.resolved_bindings as f64 / totals.variable_bindings as f64) * 100.0
        };

        let (passed, message) = if totals.variable_bindings == 0 {
            (
                false,
                Some(format!(
                    "no {} variable bindings were measured; --fail-under-dialect cannot \
                     evaluate an empty corpus for this dialect",
                    dialect.label()
                )),
            )
        } else {
            let passed = resolved_percent >= percent_threshold;
            let message = (!passed).then(|| {
                format!(
                    "resolved {}/{} variable bindings ({resolved_percent:.1}%) for {}, below \
                     the --fail-under-dialect threshold of {percent_threshold:.1}%",
                    totals.resolved_bindings,
                    totals.variable_bindings,
                    dialect.label(),
                )
            });
            (passed, message)
        };

        Self {
            dialect,
            threshold: percent_threshold,
            resolved_percent,
            passed,
            message,
        }
    }
}

/// Every `--fail-under-dialect` result for one run, in the order the caller
/// supplied the thresholds.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DialectCoveragePolicyReport {
    results: Vec<DialectCoveragePolicyResult>,
}

impl DialectCoveragePolicyReport {
    #[must_use]
    pub fn evaluate(
        thresholds: &[DialectCoverageThreshold],
        report: &SemanticCoverageReport,
    ) -> Self {
        Self {
            results: thresholds
                .iter()
                .map(|threshold| DialectCoveragePolicyResult::evaluate(*threshold, report))
                .collect(),
        }
    }

    #[must_use]
    pub fn results(&self) -> &[DialectCoveragePolicyResult] {
        &self.results
    }

    /// Whether every supplied dialect threshold passed. Vacuously true when
    /// no per-dialect thresholds were requested.
    #[must_use]
    pub fn passed(&self) -> bool {
        self.results.iter().all(DialectCoveragePolicyResult::passed)
    }
}

pub fn build_semantic_coverage_report(
    source: &mut impl SemanticCoverageSourcePort,
    request: SemanticCoverageRequest,
) -> Result<SemanticCoverageReport, SemanticCoverageWorkflowError> {
    let inventory = source
        .discover(&request)
        .map_err(|error| SemanticCoverageWorkflowError::Source(Box::new(error.into())))?;

    let mut loaded = Vec::with_capacity(inventory.files.len());
    let mut errors = Vec::new();
    for file in &inventory.files {
        match load_file(source, file, request.dialect) {
            Ok(file) => loaded.push(file),
            Err(error) => errors.push(error),
        }
    }

    // One project table per dialect present, not one for the whole corpus:
    // `build_global_table` reads every file's top level with a single
    // dialect's definition forms, so handing it a `defun` from one dialect
    // and a `cl-defun` from another under the same call would misclassify
    // whichever dialect it was not called with. A dialect table stays empty
    // when the layer does not model that dialect's definitions yet — the
    // same conservatism `build_global_table` already applies for Common Lisp
    // alone, just run once per dialect instead of assumed for all of them.
    //
    // Building it needs every file of that dialect analysed first, which is
    // the whole reason discovery and measurement are separate passes here.
    // Analysis *order* does not matter within a dialect: the table carries a
    // value only for a constant defined exactly once project-wide, and
    // "exactly once" is the same however the files are visited.
    let globals: Vec<(Dialect, GlobalTable)> = Dialect::ALL
        .into_iter()
        .filter_map(|dialect| {
            let group: Vec<ProjectFile<'_>> = loaded
                .iter()
                .filter(|file| file.dialect == dialect)
                .map(|file| ProjectFile::new(&file.tree, &file.packages, &file.values))
                .collect();
            (!group.is_empty()).then(|| (dialect, build_global_table(dialect, &group)))
        })
        .collect();

    let files = loaded
        .iter()
        .map(|file| {
            let table = globals
                .iter()
                .find(|(dialect, _)| *dialect == file.dialect)
                .map(|(_, table)| table)
                .expect("every loaded file's dialect built a (possibly empty) global table above");
            measure_file(file, table)
        })
        .collect();

    Ok(SemanticCoverageReport { files, errors })
}

/// One file, parsed and analysed on its own, before any project context
/// exists to widen it.
struct LoadedFile {
    path: PathBuf,
    dialect: Dialect,
    text: String,
    tree: SyntaxTree,
    bindings: BindingTable,
    packages: FilePackages,
    values: ValueTable,
}

fn load_file(
    source: &impl SemanticCoverageSourcePort,
    file: &DiscoveredSemanticCoverageFile,
    explicit_dialect: Option<Dialect>,
) -> Result<LoadedFile, SemanticCoverageFileError> {
    let bytes = source.load(file).map_err(|message| {
        file_error(&file.path, SemanticCoverageProcessingStage::Read, message)
    })?;
    let text = String::from_utf8(bytes).map_err(|error| {
        file_error(
            &file.path,
            SemanticCoverageProcessingStage::Decode,
            error.to_string(),
        )
    })?;
    let dialect = Dialect::detect_in_source(Some(&file.path), explicit_dialect, &text);
    let tree = SyntaxTree::parse_with_dialect(&text, dialect).map_err(|error| {
        file_error(
            &file.path,
            SemanticCoverageProcessingStage::Parse,
            error.to_string(),
        )
    })?;

    let bindings = build_binding_table(dialect, &tree, &text);
    let values = build_value_table(dialect, &tree, &bindings);
    let packages = resolve_file_packages(dialect, &tree);

    Ok(LoadedFile {
        path: file.path.clone(),
        dialect,
        text,
        tree,
        bindings,
        packages,
        values,
    })
}

/// Measures one already-loaded file, this time with the project's constants
/// available to it.
///
/// The value table is rebuilt rather than reused: the one computed during
/// loading exists only to tell the project table what this file *defines*, and
/// measuring what the file can *see* is a different question. A development
/// harness can afford the second pass; nothing on the lint path does this.
fn measure_file(file: &LoadedFile, globals: &GlobalTable) -> SemanticCoverageFileReport {
    let LoadedFile {
        text,
        tree,
        bindings,
        packages,
        dialect,
        ..
    } = file;
    let dialect = *dialect;
    let project = ProjectConstants::new(globals, packages);
    let values = build_value_table_in_project(dialect, tree, bindings, Some(&project));

    let mut report = SemanticCoverageFileReport {
        path: file.path.clone(),
        dialect,
        ..SemanticCoverageFileReport::default()
    };

    // Initial forms whose fold outcome decides between "did not fold" and
    // "folded to something unpropagatable". Answering that needs the *view*
    // at the span, which only the traversal below has, so the question is
    // parked here and settled there.
    let mut pending_initial_forms: HashMap<ByteSpan, usize> = HashMap::new();

    for (id, binding) in bindings.bindings() {
        if binding.kind() != BindingKind::Variable {
            continue;
        }
        report.variable_binding_count += 1;
        if values.binding_value(id).is_some() {
            report.resolved_binding_count += 1;
            continue;
        }

        match classify_non_resolution(binding) {
            BindingNonResolutionReason::OpaqueScope => {
                report
                    .non_resolution
                    .record(BindingNonResolutionReason::OpaqueScope);
                report
                    .non_resolution
                    .record_opacity_cause(opacity_cause_label(binding, text));
            }
            BindingNonResolutionReason::NoInitialForm => {
                report
                    .non_resolution
                    .record(BindingNonResolutionReason::NoInitialForm);
                report
                    .non_resolution
                    .record_uninitialized_binder(binding.binder_head());
            }
            BindingNonResolutionReason::InitialFormNotConstant => {
                // Deferred: counted once the traversal has folded the form.
                let span = binding
                    .init_form()
                    .expect("the classifier reached this arm only with an initial form");
                *pending_initial_forms.entry(span).or_default() += 1;
            }
            reason => report.non_resolution.record(reason),
        }
    }

    let document = tree.root_view();
    let mut folded_initial_forms: HashSet<ByteSpan> = HashSet::new();
    for_each_subview(&document, |view| {
        if pending_initial_forms.contains_key(&view.span)
            && evaluate_constant(dialect, view, bindings, &values).is_known()
        {
            folded_initial_forms.insert(view.span);
        }

        if view.kind != ExpressionKind::List {
            return;
        }
        report.list_expression_count += 1;
        if evaluate_constant(dialect, view, bindings, &values).is_known() {
            report.known_list_expression_count += 1;
        }
    });

    for (span, count) in pending_initial_forms {
        // A `Known` fold that still did not reach the binding means the value
        // exists but refuses to travel — a string or a float.
        let reason = if folded_initial_forms.contains(&span) {
            BindingNonResolutionReason::InitialFormNotPropagatable
        } else {
            BindingNonResolutionReason::InitialFormNotConstant
        };
        for _ in 0..count {
            report.non_resolution.record(reason);
        }
    }

    report
}

/// Classifies an unresolved `Variable` binding by the first disqualifying
/// fact, in the same order `Binding::is_propagatable` checks them.
///
/// [`BindingNonResolutionReason::InitialFormNotPropagatable`] is never
/// returned here: telling it from `InitialFormNotConstant` needs the initial
/// form folded, which the caller does.
fn classify_non_resolution(binding: &Binding) -> BindingNonResolutionReason {
    if !binding.assignments().is_empty() {
        BindingNonResolutionReason::Reassigned
    } else if !binding.opacity().is_transparent() {
        BindingNonResolutionReason::OpaqueScope
    } else if !binding.special().is_lexical() {
        BindingNonResolutionReason::Special
    } else if binding.init_form().is_none() {
        BindingNonResolutionReason::NoInitialForm
    } else {
        BindingNonResolutionReason::InitialFormNotConstant
    }
}

/// Names the region that cost an opaque binding its transparency.
///
/// The binding table records the site as a span rather than a name, so the
/// name is sliced back out here — the harness is the one caller that wants
/// text, and it is holding the source those spans index into.
fn opacity_cause_label(binding: &Binding, text: &str) -> OpacityCauseLabel {
    let Some(cause) = binding.opacity_cause() else {
        // The table marks a scope opaque and records why in the same call, so
        // this is unreachable; classifying it as unknown-head would invent a
        // name, and inventing evidence is the one thing this measurement must
        // not do.
        return OpacityCauseLabel::Structural(OpacityCauseKind::UnreadableHead);
    };
    match cause.kind() {
        OpacityCauseKind::UnknownHead => text
            .get(cause.site().start().get()..cause.site().end().get())
            .map_or_else(
                || OpacityCauseLabel::Structural(OpacityCauseKind::UnreadableHead),
                |head| OpacityCauseLabel::UnknownHead(head.to_ascii_lowercase()),
            ),
        kind => OpacityCauseLabel::Structural(kind),
    }
}

fn file_error(
    path: &Path,
    stage: SemanticCoverageProcessingStage,
    message: String,
) -> SemanticCoverageFileError {
    SemanticCoverageFileError {
        path: path.to_path_buf(),
        stage,
        message,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    #[derive(Default)]
    struct FakeSource {
        files: BTreeMap<PathBuf, Result<Vec<u8>, String>>,
    }

    impl FakeSource {
        fn with_file(mut self, path: &str, text: &str) -> Self {
            self.files
                .insert(PathBuf::from(path), Ok(text.as_bytes().to_vec()));
            self
        }

        fn with_error(mut self, path: &str, message: &str) -> Self {
            self.files
                .insert(PathBuf::from(path), Err(message.to_owned()));
            self
        }
    }

    impl SemanticCoverageSourcePort for FakeSource {
        type Error = paredit_core_cli::CliError;

        fn discover(
            &mut self,
            _request: &SemanticCoverageRequest,
        ) -> Result<SemanticCoverageInventory, Self::Error> {
            Ok(SemanticCoverageInventory {
                files: self
                    .files
                    .keys()
                    .cloned()
                    .map(|path| DiscoveredSemanticCoverageFile { path })
                    .collect(),
            })
        }

        fn load(&self, file: &DiscoveredSemanticCoverageFile) -> Result<Vec<u8>, String> {
            self.files[&file.path].clone()
        }
    }

    fn report(text: &str) -> SemanticCoverageFileReport {
        let mut source = FakeSource::default().with_file("a.lisp", text);
        let report = build_semantic_coverage_report(
            &mut source,
            SemanticCoverageRequest {
                paths: vec![PathBuf::from("a.lisp")],
                ..Default::default()
            },
        )
        .expect("workflow succeeds");
        assert!(report.errors().is_empty());
        report.files().first().cloned().expect("one file measured")
    }

    #[test]
    fn a_plain_let_binding_resolves_and_propagates_into_its_reference() {
        let report = report("(let ((x 1)) (+ x 1))");
        assert_eq!(report.variable_binding_count(), 1);
        assert_eq!(report.resolved_binding_count(), 1);
        assert_eq!(report.non_resolution().total(), 0);
        // `(+ x 1)` folds through the resolved binding; `(let (...) ...)`
        // does not, since a binder is not itself a fold candidate.
        assert_eq!(report.known_list_expression_count(), 1);
    }

    #[test]
    fn a_reassigned_binding_is_attributed_to_reassignment() {
        let report = report("(let ((x 1)) (setq x 2) x)");
        assert_eq!(report.resolved_binding_count(), 0);
        assert_eq!(report.non_resolution().reassigned(), 1);
        assert_eq!(report.non_resolution().total(), 1);
    }

    #[test]
    fn a_binding_with_no_initial_form_is_attributed_accordingly() {
        let report = report("(let (x) x)");
        assert_eq!(report.resolved_binding_count(), 0);
        assert_eq!(report.non_resolution().no_initial_form(), 1);
    }

    #[test]
    fn a_binding_whose_initial_form_is_not_constant_is_attributed_accordingly() {
        let report = report("(let ((x (read))) x)");
        assert_eq!(report.resolved_binding_count(), 0);
        assert_eq!(report.non_resolution().initial_form_not_constant(), 1);
    }

    #[test]
    fn a_declared_special_binding_is_attributed_accordingly() {
        let report = report("(let ((x 1)) (declare (special x)) x)");
        assert_eq!(report.resolved_binding_count(), 0);
        assert_eq!(report.non_resolution().special(), 1);
    }

    #[test]
    fn an_opaque_scope_is_attributed_accordingly() {
        let report = report("(let ((x 1)) (some-unknown-macro x) x)");
        assert_eq!(report.resolved_binding_count(), 0);
        assert_eq!(report.non_resolution().opaque_scope(), 1);
    }

    #[test]
    fn a_processing_error_is_collected_without_failing_the_whole_report() {
        let mut source = FakeSource::default()
            .with_file("good.lisp", "(let ((x 1)) x)")
            .with_error("bad.lisp", "permission denied");
        let report = build_semantic_coverage_report(
            &mut source,
            SemanticCoverageRequest {
                paths: vec![PathBuf::from("good.lisp"), PathBuf::from("bad.lisp")],
                ..Default::default()
            },
        )
        .expect("workflow succeeds despite a per-file error");
        assert_eq!(report.files().len(), 1);
        assert_eq!(report.errors().len(), 1);
        assert_eq!(
            report.errors()[0].stage,
            SemanticCoverageProcessingStage::Read
        );
    }

    fn report_for(path: &str, text: &str) -> SemanticCoverageFileReport {
        let mut source = FakeSource::default().with_file(path, text);
        let report = build_semantic_coverage_report(
            &mut source,
            SemanticCoverageRequest {
                paths: vec![PathBuf::from(path)],
                ..Default::default()
            },
        )
        .expect("workflow succeeds");
        assert!(report.errors().is_empty());
        report.files().first().cloned().expect("one file measured")
    }

    #[test]
    fn a_file_is_measured_under_its_detected_dialect() {
        assert_eq!(
            report_for("a.lisp", "(let ((x 1)) x)").dialect(),
            Dialect::CommonLisp
        );
        assert_eq!(
            report_for("a.el", "(let ((x 1)) x)").dialect(),
            Dialect::EmacsLisp
        );
    }

    #[test]
    fn an_explicit_dialect_override_wins_over_the_extension() {
        let mut source = FakeSource::default().with_file("a.txt", "(let ((x 1)) x)");
        let report = build_semantic_coverage_report(
            &mut source,
            SemanticCoverageRequest {
                paths: vec![PathBuf::from("a.txt")],
                dialect: Some(Dialect::CommonLisp),
            },
        )
        .expect("workflow succeeds");
        assert_eq!(
            report.files().first().expect("one file").dialect(),
            Dialect::CommonLisp
        );
    }

    /// The layer's own asymmetry, made visible per dialect: Emacs Lisp has a
    /// binding table (dialect-depth work in progress elsewhere registers its
    /// `let`) but no value table yet, so it resolves nothing, while Common
    /// Lisp resolves the same shape of binding. This is `R2` end to end.
    #[test]
    fn by_dialect_shows_common_lisp_resolving_while_emacs_lisp_does_not() {
        let mut source = FakeSource::default()
            .with_file("a.lisp", "(let ((x 1)) x)")
            .with_file("a.el", "(let ((x 1)) x)");
        let report = build_semantic_coverage_report(
            &mut source,
            SemanticCoverageRequest {
                paths: vec![PathBuf::from("a.lisp"), PathBuf::from("a.el")],
                ..Default::default()
            },
        )
        .expect("workflow succeeds");

        let by_dialect: std::collections::BTreeMap<&str, DialectCoverageTotals> = report
            .by_dialect()
            .into_iter()
            .map(|(dialect, totals)| (dialect.label(), totals))
            .collect();

        let common_lisp = by_dialect["common-lisp"];
        assert_eq!(common_lisp.variable_bindings, 1);
        assert_eq!(common_lisp.resolved_bindings, 1);

        let emacs_lisp = by_dialect["emacs-lisp"];
        assert_eq!(emacs_lisp.variable_bindings, 1);
        assert_eq!(
            emacs_lisp.resolved_bindings, 0,
            "the value table is Common-Lisp-only today; a dialect whose \
             binding table exists but whose value table does not must show \
             up as bindings found, none resolved — not folded into a single \
             number that hides which half of the layer is missing"
        );

        // A dialect the corpus never contained does not appear at all: an
        // absent row, not an all-zero one that would misreport "measured and
        // found nothing" for "never looked".
        assert!(!by_dialect.contains_key("clojure"));
    }

    #[test]
    fn an_unarmed_threshold_always_passes() {
        let report = build_semantic_coverage_report(
            &mut FakeSource::default().with_file("a.lisp", "(let ((x (read))) x)"),
            SemanticCoverageRequest {
                paths: vec![PathBuf::from("a.lisp")],
                ..Default::default()
            },
        )
        .expect("workflow succeeds");
        let policy = SemanticCoveragePolicy::evaluate(None, &report);
        assert!(policy.passed);
        assert!(policy.message.is_none());
    }

    #[test]
    fn a_threshold_above_the_resolved_rate_fails_with_a_message() {
        let report = report("(let ((x (read))) x)");
        let report = SemanticCoverageReport {
            files: vec![report],
            errors: Vec::new(),
        };
        let policy = SemanticCoveragePolicy::evaluate(Some(50.0), &report);
        assert!(!policy.passed);
        assert!(policy.message.is_some());
    }

    #[test]
    fn a_threshold_at_or_below_the_resolved_rate_passes() {
        let report = report("(let ((x 1)) x)");
        let report = SemanticCoverageReport {
            files: vec![report],
            errors: Vec::new(),
        };
        let policy = SemanticCoveragePolicy::evaluate(Some(100.0), &report);
        assert!(policy.passed);
    }

    #[test]
    fn a_threshold_at_the_fractional_resolved_rate_passes_but_one_just_above_fails() {
        let coverage = SemanticCoverageReport {
            files: vec![report("(let ((x 1)) x)"), report("(let ((x (read))) x)")],
            errors: Vec::new(),
        };

        let exact = SemanticCoveragePolicy::evaluate(Some(50.0), &coverage);
        assert_eq!(exact.resolved_percent, 50.0);
        assert!(exact.passed, "the threshold comparison is inclusive");

        let above = SemanticCoveragePolicy::evaluate(Some(50.000_001), &coverage);
        assert!(!above.passed);
        assert!(above.message.is_some());
    }

    /// An armed threshold over zero measured bindings fails rather than
    /// trivially passing at a fabricated 100% — an empty corpus almost always
    /// means a misconfiguration (a typo'd path, a glob matching nothing), and
    /// CI silently passing on that mistake would be worse than a loud failure.
    #[test]
    fn an_armed_threshold_over_zero_bindings_fails_rather_than_passing_trivially() {
        let report = SemanticCoverageReport::default();
        let policy = SemanticCoveragePolicy::evaluate(Some(50.0), &report);
        assert!(!policy.passed);
        assert!(policy.message.is_some());
    }

    /// An *unarmed* threshold is a different question: `None` means the
    /// caller never asked for a gate at all, so an empty corpus is not this
    /// policy's problem to flag.
    #[test]
    fn an_unarmed_threshold_still_passes_over_zero_bindings() {
        let report = SemanticCoverageReport::default();
        let policy = SemanticCoveragePolicy::evaluate(None, &report);
        assert!(policy.passed);
        assert!(policy.message.is_none());
    }

    #[test]
    fn even_a_zero_threshold_fails_over_zero_measured_bindings() {
        let report = SemanticCoverageReport::default();
        let policy = SemanticCoveragePolicy::evaluate(Some(0.0), &report);
        assert_eq!(policy.resolved_percent, 100.0);
        assert!(!policy.passed);
        assert!(policy.message.is_some());
    }

    #[test]
    fn a_dialect_threshold_rejects_an_out_of_range_percentage() {
        assert!(DialectCoverageThreshold::new(Dialect::CommonLisp, 0.0).is_ok());
        assert!(DialectCoverageThreshold::new(Dialect::CommonLisp, 100.0).is_ok());
        assert!(DialectCoverageThreshold::new(Dialect::CommonLisp, -1.0).is_err());
        assert!(DialectCoverageThreshold::new(Dialect::CommonLisp, 100.1).is_err());
        assert!(DialectCoverageThreshold::new(Dialect::CommonLisp, f64::NAN).is_err());
        assert!(DialectCoverageThreshold::new(Dialect::CommonLisp, f64::INFINITY).is_err());
        assert!(DialectCoverageThreshold::new(Dialect::CommonLisp, f64::NEG_INFINITY).is_err());
        assert!(DialectCoverageThreshold::new(Dialect::CommonLisp, 50.0).is_ok());
    }

    #[test]
    fn non_resolution_reason_uses_the_first_disqualifying_fact() {
        let reassigned_and_opaque = report("(let ((x 1)) (setq x 2) (some-unknown-macro x) x)");
        assert_eq!(reassigned_and_opaque.non_resolution().reassigned(), 1);
        assert_eq!(reassigned_and_opaque.non_resolution().opaque_scope(), 0);

        let opaque_and_special =
            report("(let ((x 1)) (declare (special x)) (some-unknown-macro x) x)");
        assert_eq!(opaque_and_special.non_resolution().opaque_scope(), 1);
        assert_eq!(opaque_and_special.non_resolution().special(), 0);

        let special_without_initializer = report("(let (x) (declare (special x)) x)");
        assert_eq!(special_without_initializer.non_resolution().special(), 1);
        assert_eq!(
            special_without_initializer
                .non_resolution()
                .no_initial_form(),
            0
        );
    }

    /// (a) A per-dialect threshold at or below that dialect's own resolved
    /// rate passes, the same as the corpus-wide gate.
    #[test]
    fn a_dialect_threshold_at_or_below_the_resolved_rate_passes() {
        let file = report("(let ((x 1)) x)");
        let coverage = SemanticCoverageReport {
            files: vec![file],
            errors: Vec::new(),
        };
        let threshold = DialectCoverageThreshold::new(Dialect::CommonLisp, 90.0)
            .expect("90 is a valid percentage");
        let result = DialectCoveragePolicyResult::evaluate(threshold, &coverage);
        assert!(result.passed());
        assert!(result.message().is_none());
    }

    /// (b) A per-dialect threshold above that dialect's own resolved rate
    /// fails, with a message naming the dialect.
    #[test]
    fn a_dialect_threshold_above_the_resolved_rate_fails_with_a_message() {
        let file = report("(let ((x (read))) x)");
        let coverage = SemanticCoverageReport {
            files: vec![file],
            errors: Vec::new(),
        };
        let threshold = DialectCoverageThreshold::new(Dialect::CommonLisp, 50.0)
            .expect("50 is a valid percentage");
        let result = DialectCoveragePolicyResult::evaluate(threshold, &coverage);
        assert!(!result.passed());
        let message = result.message().expect("a failing gate explains itself");
        assert!(message.contains("common-lisp"));
    }

    /// (c) A dialect the corpus never discovered any files for fails loudly
    /// the same way an empty corpus does for `--fail-under`: `by_dialect`
    /// omits the row entirely, and that must not read as "measured and
    /// found nothing to resolve" — the same misconfiguration signal (a
    /// typo'd dialect, a corpus that simply has none of that dialect yet)
    /// the corpus-wide gate already refuses to pass silently.
    #[test]
    fn a_dialect_threshold_over_zero_discovered_files_fails_rather_than_passing_trivially() {
        let file = report("(let ((x 1)) x)");
        let coverage = SemanticCoverageReport {
            files: vec![file],
            errors: Vec::new(),
        };
        let threshold = DialectCoverageThreshold::new(Dialect::EmacsLisp, 50.0)
            .expect("50 is a valid percentage");
        let result = DialectCoveragePolicyResult::evaluate(threshold, &coverage);
        assert!(!result.passed());
        assert!(result.message().is_some());
    }

    /// (d) Global and per-dialect thresholds combine with AND semantics: the
    /// corpus-wide rate can clear `--fail-under` while one dialect's own
    /// rate still misses its `--fail-under-dialect` floor, and the overall
    /// gate — as the CLI workflow combines the two — must still fail.
    #[test]
    fn a_passing_global_threshold_and_a_failing_dialect_threshold_together_fail_overall() {
        let mut source = FakeSource::default()
            .with_file("a.lisp", "(let ((x 1)) x)")
            .with_file("a.el", "(let ((x 1)) x)");
        let coverage = build_semantic_coverage_report(
            &mut source,
            SemanticCoverageRequest {
                paths: vec![PathBuf::from("a.lisp"), PathBuf::from("a.el")],
                ..Default::default()
            },
        )
        .expect("workflow succeeds");

        // 2 variable bindings total (one per file), 1 resolved (common-lisp
        // only) = exactly 50%.
        let global = SemanticCoveragePolicy::evaluate(Some(50.0), &coverage);
        assert!(global.passed, "corpus-wide rate is exactly 50%");

        let threshold = DialectCoverageThreshold::new(Dialect::EmacsLisp, 50.0)
            .expect("50 is a valid percentage");
        let dialect_policy = DialectCoveragePolicyReport::evaluate(&[threshold], &coverage);
        assert!(
            !dialect_policy.passed(),
            "emacs-lisp resolves nothing, so its own rate is 0%"
        );

        let overall_passed = global.passed && dialect_policy.passed();
        assert!(
            !overall_passed,
            "a failing per-dialect threshold must fail the run even when the \
             corpus-wide threshold passes"
        );
    }
}
