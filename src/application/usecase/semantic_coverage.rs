//! Measuring how much of the semantic layer resolves on real source.
//!
//! `src/domain/semantics` is conservative by design — it says `Known` only
//! when it can prove something, and stays silent otherwise. That discipline
//! is only worth its cost if it actually resolves a useful fraction of real
//! code, and *why* it does not resolve the rest is what decides where the
//! next round of work should go. This use case answers both questions: it
//! builds the binding and value tables for each file and counts, rather than
//! asserts, what they found.
//!
//! Discovery is a source-port responsibility, mirroring
//! [`crate::application::usecase::similarity_report`]: this module only knows
//! how to turn bytes into a report, not how paths become files.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::domain::dialect::Dialect;
use crate::domain::semantics::binding::{
    Binding, BindingKind, BindingTable, OpacityCauseKind, build_binding_table,
};
use crate::domain::semantics::project::GlobalTable;
use crate::domain::semantics::project::service::{
    FilePackages, ProjectFile, build_global_table, resolve_file_packages,
};
use crate::domain::semantics::value::{
    ProjectConstants, ValueTable, build_value_table, build_value_table_in_project,
    evaluate_constant,
};
use crate::domain::sexpr::{ByteSpan, ExpressionKind, SyntaxTree};
use crate::domain::view_query::for_each_subview;

/// The files to measure. Discovery (walking directories, filtering
/// extensions) happens behind [`SemanticCoverageSourcePort`]; this only names
/// the roots the caller asked about.
#[derive(Debug, Clone)]
pub struct SemanticCoverageRequest {
    pub paths: Vec<PathBuf>,
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
/// Only Common Lisp is worth measuring here: `build_binding_table` and
/// `build_value_table` return an empty table for every other dialect, so a
/// source that discovered other dialects would only measure zeroes.
pub trait SemanticCoverageSourcePort {
    fn discover(
        &mut self,
        request: &SemanticCoverageRequest,
    ) -> anyhow::Result<SemanticCoverageInventory>;

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
    Source(anyhow::Error),
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
/// [`Binding::is_propagatable`](crate::domain::semantics::binding::Binding::is_propagatable)
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
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SemanticCoverageFileReport {
    path: PathBuf,
    variable_binding_count: usize,
    resolved_binding_count: usize,
    list_expression_count: usize,
    known_list_expression_count: usize,
    non_resolution: BindingNonResolutionBreakdown,
}

impl SemanticCoverageFileReport {
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
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
}

pub fn build_semantic_coverage_report(
    source: &mut impl SemanticCoverageSourcePort,
    request: SemanticCoverageRequest,
) -> Result<SemanticCoverageReport, SemanticCoverageWorkflowError> {
    let inventory = source
        .discover(&request)
        .map_err(SemanticCoverageWorkflowError::Source)?;

    let mut loaded = Vec::with_capacity(inventory.files.len());
    let mut errors = Vec::new();
    for file in &inventory.files {
        match load_file(source, file) {
            Ok(file) => loaded.push(file),
            Err(error) => errors.push(error),
        }
    }

    // The project table is what makes a `defconstant` visible to a file that
    // does not define it, and building it needs every file analysed first —
    // which is the whole reason discovery and measurement are separate passes
    // here. Analysis *order* does not matter: the table carries a value only
    // for a constant defined exactly once project-wide, and "exactly once" is
    // the same however the files are visited.
    let project_files: Vec<ProjectFile<'_>> = loaded
        .iter()
        .map(|file| ProjectFile::new(&file.tree, &file.packages, &file.values))
        .collect();
    let globals = build_global_table(Dialect::CommonLisp, &project_files);

    let files = loaded
        .iter()
        .map(|file| measure_file(file, &globals))
        .collect();

    Ok(SemanticCoverageReport { files, errors })
}

/// One file, parsed and analysed on its own, before any project context
/// exists to widen it.
struct LoadedFile {
    path: PathBuf,
    text: String,
    tree: SyntaxTree,
    bindings: BindingTable,
    packages: FilePackages,
    values: ValueTable,
}

fn load_file(
    source: &impl SemanticCoverageSourcePort,
    file: &DiscoveredSemanticCoverageFile,
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
    let tree = SyntaxTree::parse_with_dialect(&text, Dialect::CommonLisp).map_err(|error| {
        file_error(
            &file.path,
            SemanticCoverageProcessingStage::Parse,
            error.to_string(),
        )
    })?;

    let bindings = build_binding_table(Dialect::CommonLisp, &tree, &text);
    let values = build_value_table(Dialect::CommonLisp, &tree, &bindings);
    let packages = resolve_file_packages(Dialect::CommonLisp, &tree);

    Ok(LoadedFile {
        path: file.path.clone(),
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
        ..
    } = file;
    let project = ProjectConstants::new(globals, packages);
    let values = build_value_table_in_project(Dialect::CommonLisp, tree, bindings, Some(&project));

    let mut report = SemanticCoverageFileReport {
        path: file.path.clone(),
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
            && evaluate_constant(Dialect::CommonLisp, view, bindings, &values).is_known()
        {
            folded_initial_forms.insert(view.span);
        }

        if view.kind != ExpressionKind::List {
            return;
        }
        report.list_expression_count += 1;
        if evaluate_constant(Dialect::CommonLisp, view, bindings, &values).is_known() {
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
        fn discover(
            &mut self,
            _request: &SemanticCoverageRequest,
        ) -> anyhow::Result<SemanticCoverageInventory> {
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
}
