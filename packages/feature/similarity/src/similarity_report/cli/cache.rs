//! Not recomputing a similarity report over a corpus that has not changed.
//!
//! Every pair of candidate forms is compared by tree edit distance, so the
//! analysis is quadratic in the candidate count where discovery is linear in
//! the file count. On a repeated run — an editor, an agent, a pre-commit hook
//! invoked twice — all of that work re-derives an answer the previous run
//! already had.
//!
//! The design is the one [`paredit_core_safety::cache`] already argues for and
//! is reused here rather than reinvented: the key is a hash of everything that
//! can change the answer, so a hit means "this exact question was asked and
//! answered". There is no invalidation logic because there is nothing to
//! invalidate — a superseded entry describes a question nobody is asking any
//! more, so it is unreachable rather than wrong.
//!
//! What goes into the key:
//!
//! * the tool version, so a build with new similarity logic cannot serve an
//!   answer computed by an older one;
//! * every analysis option — threshold, the two scopes, the overlap policy,
//!   the node and line minimums, the three budgets, the forced dialect, the
//!   error policy;
//! * the *resolved file set*: each selected file's path, dialect and full
//!   content hash, in discovery order. Keying on the file set rather than on
//!   the discovery flags that produced it is strictly stronger — two flag
//!   combinations that select the same bytes are genuinely the same question,
//!   and one that selects different bytes cannot alias however it was spelled.
//!
//! What deliberately stays out, and why each is safe:
//!
//! * **`--output`.** What is stored is the report, not the rendering, so a
//!   `--output text` run and a `--output json` run share one entry.
//! * **`--fail-on-duplicates`.** The gate is a pure function of the policy,
//!   the report and the errors, and a hit rebuilds the plan through the same
//!   [`SimilarityReportPlan::new`] the fresh path uses, with the *current*
//!   run's policy. Keying on it would force a recompute to reach a decision
//!   that is recomputed anyway.
//! * **the roots, and `--jobs`.** The roots reach the answer only through
//!   which files they select, which the file set already covers; the worker
//!   count cannot change a result that is reassembled in inventory order.
//!
//! No mtime, no size, no inode: all three are proxies for content and all
//! three lie across a checkout, a `touch`, or a filesystem without sub-second
//! timestamps. A file that cannot be read at fingerprint time takes the run
//! off the cache entirely — see [`fingerprint_corpus`].

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use paredit_core_cli::{CliError, CliResult};
use paredit_core_safety::cache::AnalysisCache;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteOffset, ByteSpan, ExpressionPath};

use crate::similarity_report::usecase::{
    DiscoveredSimilarityFile, FormHead, PairProcessingCounts, PairResultCounts, ReportLimit,
    SimilarityDuplicatePolicy, SimilarityErrorPolicy, SimilarityFileError, SimilarityFormReport,
    SimilarityInventory, SimilarityPairReport, SimilarityProcessingStage, SimilarityRatio,
    SimilarityReport, SimilarityReportPlan, SimilarityReportRequest, SimilarityReportSummary,
    SimilarityScore,
};

/// Names the question, so no other analysis can read these entries.
const ANALYSIS: &str = "similarity-report";

/// The subdirectory of `--cache-dir` these entries live in.
///
/// Separate from the discovery cache's entries, which sit at the root of the
/// same directory. Two reasons: `DiscoveryCache::clear` walks that root and
/// deletes `*.json`, which would either miss these or eat them depending on
/// how they were named; and a directory of its own is what makes
/// `--clear-cache` able to remove exactly these and nothing else.
const SUBDIRECTORY: &str = "similarity";

/// Whether the report was reused, for a report that wants to say so.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResultCacheOutcome {
    /// The stored report was reused.
    Hit,
    /// Nothing was stored for this question, or what was stored could not be
    /// understood. Both mean the same thing to a caller — compute it again —
    /// and distinguishing them would invite handling a case whose only correct
    /// response is the one already taken.
    Missing,
}

impl ResultCacheOutcome {
    /// A stable identifier for JSON and text output.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Hit => "hit",
            Self::Missing => "missing",
        }
    }
}

/// A store of previously computed similarity reports.
#[derive(Debug)]
pub struct SimilarityResultCache {
    inner: AnalysisCache,
}

impl SimilarityResultCache {
    /// Opens the cache under `cache_dir`, emptying it first when `clear`.
    ///
    /// A directory that cannot be created is an error rather than a silent
    /// fallback: the caller asked for a cache, and a run that quietly does not
    /// use one looks identical to one that does, only slower.
    pub fn open(cache_dir: &Path, clear: bool) -> CliResult<Self> {
        let root = cache_dir.join(SUBDIRECTORY);
        if clear {
            match std::fs::remove_dir_all(&root) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(CliError::io(format!("clearing {}", root.display()))(error));
                }
            }
        }
        let inner = AnalysisCache::open(&root, env!("CARGO_PKG_VERSION"))
            .map_err(CliError::io(format!("opening {}", root.display())))?;
        Ok(Self { inner })
    }

    /// The key for one question: the request's options over one corpus.
    #[must_use]
    pub fn key(&self, request: &SimilarityReportRequest, fingerprint: &str) -> String {
        self.inner
            .key(ANALYSIS, &discriminator(request), fingerprint)
    }

    /// The stored report, rebuilt against `inventory`, if one is usable.
    ///
    /// `inventory` comes from the current run's discovery rather than from the
    /// entry. The file set is part of the key, so the two agree by
    /// construction; taking the fresh one means a form's dialect and the skip
    /// counters are read from the tree as it is now, and it removes any need
    /// to map a stored dialect label back onto the enum — a mapping that would
    /// silently degrade to `unknown` the day a dialect is added.
    #[must_use]
    pub fn get(
        &self,
        key: &str,
        inventory: &SimilarityInventory,
        duplicate_policy: SimilarityDuplicatePolicy,
    ) -> Option<SimilarityReportPlan> {
        decode_plan(&self.inner.get(key)?, inventory, duplicate_policy)
    }

    /// Records a report. A write that fails is a slow run, not a wrong one.
    pub fn put(&self, key: &str, plan: &SimilarityReportPlan) {
        self.inner.put(key, &encode_plan(plan));
    }
}

/// A content hash of the corpus, or `None` when it cannot be taken.
///
/// Reads every selected file through the same loader the analysis uses, so the
/// bytes hashed are the bytes that would be analysed rather than whatever a
/// second, independent read might find.
///
/// A file that fails to load returns `None`, which takes the whole run off the
/// cache. It would be tempting to fold a marker into the hash instead and keep
/// caching, but the report's text for an unreadable file is the I/O error's
/// own message: two different failures on the same path would then share a key
/// and one would be reported under the other's message. Not caching a run that
/// is already reporting an error costs nothing worth having.
pub fn fingerprint_corpus(
    inventory: &SimilarityInventory,
    load: impl Fn(&DiscoveredSimilarityFile) -> Result<Vec<u8>, String>,
) -> Option<String> {
    let mut hasher = blake3::Hasher::new();
    for file in &inventory.files {
        let bytes = load(file).ok()?;
        // Length-prefixed, so no two different field splits can hash the same:
        // a path ending in the first byte of its own contents must not collide
        // with the path one byte shorter.
        for field in [
            file.path.as_os_str().as_encoded_bytes(),
            file.dialect.label().as_bytes(),
            bytes.as_slice(),
        ] {
            hasher.update(&(field.len() as u64).to_le_bytes());
            hasher.update(field);
        }
    }
    Some(hasher.finalize().to_hex().to_string())
}

/// Everything other than the corpus that changes what the analysis reports.
///
/// Getting this wrong is the one way to make this cache serve a wrong answer,
/// so every field of [`SimilarityReportOptions`] appears, and the float is
/// hashed by its bits rather than by a rendering that could round two distinct
/// thresholds onto one string.
///
/// [`SimilarityReportOptions`]: crate::similarity_report::domain::SimilarityReportOptions
fn discriminator(request: &SimilarityReportRequest) -> String {
    let options = &request.options;
    let optional = |value: Option<usize>| value.map_or_else(|| "-".to_owned(), |v| v.to_string());
    [
        format!("threshold={:016x}", options.threshold().to_bits()),
        format!("min_node_count={}", options.min_node_count()),
        format!("min_line_span={}", options.min_line_span()),
        format!("comparison_scope={}", options.comparison_scope().label()),
        format!("form_scope={}", options.form_scope().label()),
        format!("overlap_policy={}", options.overlap_policy().label()),
        format!("max_candidates={}", optional(options.max_candidates())),
        format!("max_comparisons={}", optional(options.max_comparisons())),
        format!("max_results={}", optional(options.max_results())),
        format!(
            "dialect={}",
            request.forced_dialect.map_or("-", Dialect::label)
        ),
        format!(
            "error_policy={}",
            match request.error_policy {
                SimilarityErrorPolicy::Fail => "fail",
                SimilarityErrorPolicy::Skip => "skip",
            }
        ),
    ]
    .join("|")
}

fn encode_plan(plan: &SimilarityReportPlan) -> Value {
    let report = plan.report();
    let summary = &report.summary;
    json!({
        "summary": {
            // `ReportLimit` is `Complete` or `Limited(NonZeroUsize)`, so the
            // omitted count carries the reached flag too: it is non-zero if
            // and only if the limit was hit. One number, no redundant pair to
            // disagree with itself.
            "omitted_candidates": summary.omitted_candidates(),
            "unprocessed_pairs": summary.unprocessed_pairs(),
            "possible_pairs": summary.possible_pairs(),
            "evaluated_pairs": summary.evaluated_pairs(),
            "pruned_by_size": summary.pruned_by_size(),
            "resource_skipped_pairs": summary.resource_skipped_pairs(),
            "matched_pairs": summary.matched_pairs(),
            "suppressed_pairs": summary.suppressed_pairs(),
            "reported_pairs": summary.reported_pairs(),
        },
        "pairs": report.pairs.iter().map(|pair| json!({
            "similarity": encode_f64(pair.similarity().as_f64()),
            "score": encode_f64(pair.score().as_f64()),
            "left": encode_form(pair.left()),
            "right": encode_form(pair.right()),
        })).collect::<Vec<_>>(),
        "errors": plan.errors().iter().map(|error| json!({
            "path": error.path.to_string_lossy(),
            "stage": error.stage.label(),
            "message": error.message,
        })).collect::<Vec<_>>(),
    })
}

/// A float as its bit pattern, in hex.
///
/// **Not as a JSON number.** `serde_json` writes a float exactly but does not
/// read one back exactly: without its `float_roundtrip` feature the parser is
/// the fast approximate one, and a similarity of `0.9333333333333333` returns
/// as `0.9333333333333332`. Storing the number is the obvious thing to write
/// and it silently makes a reused report differ from a fresh one in the last
/// digit — caught on a 227-pair corpus, invisible on a small one.
///
/// Turning on `float_roundtrip` would fix the symptom for this call site and
/// slow every other JSON parse in the tool down to do it. The bit pattern is
/// exact by construction and cannot regress if that feature moves.
fn encode_f64(value: f64) -> String {
    format!("{:016x}", value.to_bits())
}

fn decode_f64(stored: &Value) -> Option<f64> {
    Some(f64::from_bits(
        u64::from_str_radix(stored.as_str()?, 16).ok()?,
    ))
}

/// One form, without its dialect.
///
/// The dialect is context rather than result — discovery decides it from the
/// path and `--dialect` — so it is re-attached on decode from the run's own
/// inventory instead of being stored and mapped back.
fn encode_form(form: &SimilarityFormReport) -> Value {
    json!({
        "path": form.path().to_string_lossy(),
        "form_path": form.form_path().to_raw_indexes(),
        "start": form.span().start().get(),
        "end": form.span().end().get(),
        "node_count": form.node_count(),
        "head": form.head().map(FormHead::as_str),
        "text": form.text().as_ref(),
    })
}

/// Rebuilds a plan from a stored entry, or `None` if anything is off.
///
/// Every step goes through the same constructor the fresh path uses, so an
/// entry that would violate a report invariant cannot become a plan: it fails
/// the same validation and reads as a miss. That is what makes a corrupt or
/// hand-edited entry a slow run rather than a wrong report.
fn decode_plan(
    entry: &Value,
    inventory: &SimilarityInventory,
    duplicate_policy: SimilarityDuplicatePolicy,
) -> Option<SimilarityReportPlan> {
    let dialects = inventory
        .files
        .iter()
        .map(|file| (file.path.as_path(), file.dialect))
        .collect::<HashMap<_, _>>();

    let stored = &entry["summary"];
    let count = |name: &str| usize::try_from(stored[name].as_u64()?).ok();
    let summary = SimilarityReportSummary::new(
        ReportLimit::from_omitted(count("omitted_candidates")?),
        PairProcessingCounts::new(
            count("possible_pairs")?,
            count("evaluated_pairs")?,
            count("pruned_by_size")?,
            count("resource_skipped_pairs")?,
        )
        .ok()?,
        ReportLimit::from_omitted(count("unprocessed_pairs")?),
        PairResultCounts::new(
            count("matched_pairs")?,
            count("suppressed_pairs")?,
            count("reported_pairs")?,
        )
        .ok()?,
    )
    .ok()?;

    let pairs = entry["pairs"]
        .as_array()?
        .iter()
        .map(|pair| {
            Some(SimilarityPairReport::new(
                // Validated rather than trusted: a hand-edited entry holding a
                // NaN or a ratio above one must be a miss, not a report.
                SimilarityRatio::new(decode_f64(&pair["similarity"])?)?,
                SimilarityScore::new(decode_f64(&pair["score"])?)?,
                decode_form(&pair["left"], &dialects)?,
                decode_form(&pair["right"], &dialects)?,
            ))
        })
        .collect::<Option<Vec<_>>>()?;

    let errors = entry["errors"]
        .as_array()?
        .iter()
        .map(|error| {
            Some(SimilarityFileError {
                path: PathBuf::from(error["path"].as_str()?),
                stage: decode_stage(error["stage"].as_str()?)?,
                message: error["message"].as_str()?.to_owned(),
            })
        })
        .collect::<Option<Vec<_>>>()?;

    SimilarityReportPlan::new(
        SimilarityReport::new(summary, pairs).ok()?,
        inventory.clone(),
        errors,
        duplicate_policy,
    )
    .ok()
}

fn decode_form(stored: &Value, dialects: &HashMap<&Path, Dialect>) -> Option<SimilarityFormReport> {
    let path = PathBuf::from(stored["path"].as_str()?);
    // A stored form whose file is no longer in the corpus cannot be reported
    // under a dialect this run agrees with, so the entry is unusable.
    let dialect = *dialects.get(path.as_path())?;
    let indexes = stored["form_path"]
        .as_array()?
        .iter()
        .map(|index| usize::try_from(index.as_u64()?).ok())
        .collect::<Option<Vec<_>>>()?;
    let span = ByteSpan::new(
        ByteOffset::new(usize::try_from(stored["start"].as_u64()?).ok()?),
        ByteOffset::new(usize::try_from(stored["end"].as_u64()?).ok()?),
    );
    let head = match &stored["head"] {
        Value::Null => None,
        value => Some(FormHead::new(value.as_str()?)),
    };
    Some(SimilarityFormReport::new(
        path,
        dialect,
        ExpressionPath::from_indexes(indexes),
        span,
        usize::try_from(stored["node_count"].as_u64()?).ok()?,
        head,
        stored["text"].as_str()?,
    ))
}

fn decode_stage(label: &str) -> Option<SimilarityProcessingStage> {
    match label {
        "read" => Some(SimilarityProcessingStage::Read),
        "decode" => Some(SimilarityProcessingStage::Decode),
        "parse" => Some(SimilarityProcessingStage::Parse),
        "collect" => Some(SimilarityProcessingStage::Collect),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use serde_json::{Value, json};

    use super::{decode_f64, discriminator, encode_f64, fingerprint_corpus};
    use crate::similarity_report::domain::{SimilarityOverlapPolicy, SimilarityReportOptions};
    use crate::similarity_report::usecase::{
        DiscoveredSimilarityFile, SimilarityDuplicatePolicy, SimilarityErrorPolicy,
        SimilarityInventory, SimilarityReportRequest,
    };
    use paredit_core_syntax::dialect::Dialect;

    fn request(options: SimilarityReportOptions) -> SimilarityReportRequest {
        SimilarityReportRequest {
            roots: vec![PathBuf::from(".")],
            include_unknown: false,
            include_hidden: false,
            include_generated: false,
            max_depth: None,
            exclude: Vec::new(),
            forced_dialect: None,
            options,
            error_policy: SimilarityErrorPolicy::Fail,
            duplicate_policy: SimilarityDuplicatePolicy::Ignore,
        }
    }

    fn inventory(paths: &[&str]) -> SimilarityInventory {
        SimilarityInventory {
            files: paths
                .iter()
                .map(|path| DiscoveredSimilarityFile {
                    path: PathBuf::from(path),
                    dialect: Dialect::CommonLisp,
                })
                .collect(),
            ..SimilarityInventory::default()
        }
    }

    /// A reused similarity must be the *same* float, not a near one.
    ///
    /// The values below are the ones a JSON number does not survive:
    /// `serde_json`'s default parser is approximate, and `0.9333333333333333`
    /// — a real similarity from a 227-pair corpus — comes back one ULP low,
    /// which shows up as a changed last digit in an otherwise identical
    /// report. Encoding the bits sidesteps the parser entirely.
    #[test]
    fn a_similarity_survives_the_entry_bit_for_bit() {
        for value in [
            0.933_333_333_333_333_3_f64,
            0.87,
            1.0,
            0.0,
            f64::MIN_POSITIVE,
            1.0 / 3.0,
        ] {
            let stored = serde_json::to_string(&json!(encode_f64(value)))
                .expect("the encoding is a plain string");
            let read: Value = serde_json::from_str(&stored).expect("it parses back");

            assert_eq!(
                decode_f64(&read).map(f64::to_bits),
                Some(value.to_bits()),
                "{value} did not survive the entry"
            );
        }
    }

    #[test]
    fn a_float_that_is_not_a_bit_pattern_is_a_miss() {
        for bad in [json!(0.87), json!("not hex"), Value::Null] {
            assert_eq!(decode_f64(&bad), None);
        }
    }

    #[test]
    fn one_changed_byte_is_a_different_corpus() {
        let inventory = inventory(&["a.lisp"]);
        let first = fingerprint_corpus(&inventory, |_| Ok(b"(defun f ())".to_vec()));
        let second = fingerprint_corpus(&inventory, |_| Ok(b"(defun g ())".to_vec()));

        assert!(first.is_some());
        assert_ne!(first, second);
    }

    /// The field boundaries have to be in the hash, or moving a byte from a
    /// path into the file it names would leave the corpus looking unchanged.
    #[test]
    fn adjacent_corpus_fields_cannot_be_confused_for_one_another() {
        assert_ne!(
            fingerprint_corpus(&inventory(&["ab"]), |_| Ok(b"c".to_vec())),
            fingerprint_corpus(&inventory(&["a"]), |_| Ok(b"bc".to_vec()))
        );
    }

    #[test]
    fn a_reordered_corpus_is_a_different_corpus() {
        assert_ne!(
            fingerprint_corpus(&inventory(&["a.lisp", "b.lisp"]), |file| Ok(file
                .path
                .to_string_lossy()
                .as_bytes()
                .to_vec())),
            fingerprint_corpus(&inventory(&["b.lisp", "a.lisp"]), |file| Ok(file
                .path
                .to_string_lossy()
                .as_bytes()
                .to_vec()))
        );
    }

    /// An unreadable file takes the run off the cache rather than aliasing two
    /// different I/O failures onto one entry.
    #[test]
    fn an_unreadable_file_has_no_fingerprint() {
        assert_eq!(
            fingerprint_corpus(&inventory(&["a.lisp"]), |_| Err("denied".to_owned())),
            None
        );
    }

    #[test]
    fn every_matching_option_changes_the_discriminator() {
        let base = SimilarityReportOptions::default();
        let baseline = discriminator(&request(base.clone()));

        let variants = [
            SimilarityReportOptions::new(
                0.9,
                base.min_node_count(),
                base.min_line_span(),
                base.comparison_scope(),
                base.form_scope(),
                base.overlap_policy(),
                base.max_candidates(),
                base.max_comparisons(),
                base.max_results(),
            ),
            SimilarityReportOptions::new(
                base.threshold(),
                base.min_node_count(),
                base.min_line_span(),
                base.comparison_scope(),
                base.form_scope(),
                SimilarityOverlapPolicy::All,
                base.max_candidates(),
                base.max_comparisons(),
                base.max_results(),
            ),
            SimilarityReportOptions::new(
                base.threshold(),
                base.min_node_count(),
                base.min_line_span(),
                base.comparison_scope(),
                base.form_scope(),
                base.overlap_policy(),
                base.max_candidates(),
                base.max_comparisons(),
                Some(5),
            ),
        ];
        for variant in variants {
            let variant = variant.expect("the variant options are valid");
            assert_ne!(baseline, discriminator(&request(variant)));
        }
    }

    /// Two thresholds that render identically at the report's six decimal
    /// places are still two different questions.
    #[test]
    fn nearly_equal_thresholds_do_not_share_a_discriminator() {
        let of = |threshold: f64| {
            let base = SimilarityReportOptions::default();
            discriminator(&request(
                SimilarityReportOptions::new(
                    threshold,
                    base.min_node_count(),
                    base.min_line_span(),
                    base.comparison_scope(),
                    base.form_scope(),
                    base.overlap_policy(),
                    None,
                    None,
                    None,
                )
                .expect("valid options"),
            ))
        };

        assert_ne!(of(0.87), of(0.870_000_000_1));
    }

    #[test]
    fn the_error_policy_is_part_of_the_question() {
        let mut skipping = request(SimilarityReportOptions::default());
        skipping.error_policy = SimilarityErrorPolicy::Skip;

        assert_ne!(
            discriminator(&request(SimilarityReportOptions::default())),
            discriminator(&skipping)
        );
    }

    /// The gate is recomputed from the current run's policy, so it must not
    /// split the cache.
    #[test]
    fn the_duplicate_gate_does_not_split_the_cache() {
        let mut failing = request(SimilarityReportOptions::default());
        failing.duplicate_policy = SimilarityDuplicatePolicy::Fail;

        assert_eq!(
            discriminator(&request(SimilarityReportOptions::default())),
            discriminator(&failing)
        );
    }
}
