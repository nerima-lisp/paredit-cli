//! Fitting a report into a token budget, and saying so when it did not fit.
//!
//! An agent with a context window cannot use a report that does not fit in it,
//! and truncating one silently is worse than refusing: a list that stops early
//! and does not say so reads as a complete list, and every conclusion drawn
//! from it is wrong.
//!
//! So truncation here is always accompanied by a [`Truncation`] record naming
//! every array that was cut, how many entries went, and how to get the rest.
//! The budget is approximate — token counts depend on a tokenizer this tool
//! does not have — and the estimate says so in its own name.

use serde_json::{Value as Json, json};

/// Bytes per token, as a rough average across the JSON these reports emit.
///
/// Four is the usual rule of thumb for English text and it holds up
/// reasonably for the dense punctuation of JSON. It is an estimate and the
/// whole budget is documented as approximate; nothing here pretends to be a
/// tokenizer.
const BYTES_PER_TOKEN: usize = 4;

/// The approximate token count of a rendered value.
#[must_use]
pub fn approximate_tokens(value: &Json) -> usize {
    let rendered = serde_json::to_string(value).map_or(0, |text| text.len());
    rendered.div_ceil(BYTES_PER_TOKEN)
}

/// What a budget had to remove to fit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Truncation {
    /// Each trimmed array: its key, how many entries it kept, and how many it
    /// started with.
    pub trimmed: Vec<TrimmedArray>,
    pub approximate_tokens: usize,
    pub budget: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrimmedArray {
    pub key: String,
    pub kept: usize,
    pub total: usize,
}

impl Truncation {
    #[must_use]
    pub fn to_json(&self) -> Json {
        json!({
            "truncated": true,
            "budget_tokens": self.budget,
            "approximate_tokens": self.approximate_tokens,
            "note": "Token counts are approximate. Entries were dropped from the \
                     end of each list, so what remains is a prefix in source order.",
            "arrays": self
                .trimmed
                .iter()
                .map(|array| {
                    json!({
                        "key": array.key,
                        "kept": array.kept,
                        "total": array.total,
                        "dropped": array.total - array.kept,
                    })
                })
                .collect::<Vec<_>>(),
        })
    }
}

/// An approximate token ceiling. Zero means no ceiling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Budget(pub usize);

impl Budget {
    #[must_use]
    pub const fn unlimited(self) -> bool {
        self.0 == 0
    }

    /// Trims `report`'s named top-level arrays until it fits.
    ///
    /// `trimmable` is in the order things should be given up: the first key is
    /// cut first, so a caller lists its least information-dense array first.
    /// Returns `None` when the report already fitted, so a report that fits is
    /// byte-identical to one produced with no budget at all.
    ///
    /// A report can still exceed the budget after every trimmable array is
    /// empty — the envelope has a floor. That is reported honestly rather than
    /// papered over: the counts stay, and they are what a caller needs to
    /// decide how to narrow the request.
    pub fn apply(self, report: &mut Json, trimmable: &[&str]) -> Option<Truncation> {
        // The one full serialization this function pays: every later step
        // updates this byte count incrementally instead of reserializing the
        // whole document again, which is what made this function cost
        // O(document size) per halving step on a report with several large
        // trimmable arrays.
        let mut total_bytes = serde_json::to_string(report).map_or(0, |text| text.len());
        if self.unlimited() || total_bytes.div_ceil(BYTES_PER_TOKEN) <= self.0 {
            return None;
        }

        let totals: Vec<(String, usize)> = trimmable
            .iter()
            .filter_map(|key| {
                report
                    .get(*key)
                    .and_then(Json::as_array)
                    .map(|array| ((*key).to_owned(), array.len()))
            })
            .collect();

        for (key, _) in &totals {
            // Halve repeatedly rather than dropping the whole array: a
            // truncated list that still has a prefix is far more useful than
            // an empty one, and this converges in a handful of steps.
            loop {
                if total_bytes.div_ceil(BYTES_PER_TOKEN) <= self.0 {
                    break;
                }
                let Some(array) = report.get_mut(key).and_then(Json::as_array_mut) else {
                    break;
                };
                if array.is_empty() {
                    break;
                }
                let new_len = array.len() / 2;
                // What halving removes from the document's serialized byte
                // count: the dropped elements' own JSON (serialized as a
                // slice, not the whole document), minus the two bracket
                // characters that framed it as its own array, plus one
                // boundary comma to reattach the surviving prefix to the
                // array's closing bracket — except when nothing survives, in
                // which case the array becomes `[]` and there is no comma to
                // add back.
                let dropped_slice =
                    serde_json::to_string(&array[new_len..]).map_or(0, |text| text.len());
                let boundary_comma = usize::from(new_len > 0);
                total_bytes -= dropped_slice.saturating_sub(2) + boundary_comma;
                array.truncate(new_len);
            }
            if total_bytes.div_ceil(BYTES_PER_TOKEN) <= self.0 {
                break;
            }
        }

        let trimmed: Vec<TrimmedArray> = totals
            .into_iter()
            .map(|(key, total)| {
                let kept = report
                    .get(&key)
                    .and_then(Json::as_array)
                    .map_or(0, Vec::len);
                TrimmedArray { key, kept, total }
            })
            .filter(|array| array.kept < array.total)
            .collect();

        Some(Truncation {
            approximate_tokens: total_bytes.div_ceil(BYTES_PER_TOKEN),
            budget: self.0,
            trimmed,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(items: usize) -> Json {
        json!({
            "schema_version": 1,
            "atoms": (0..items)
                .map(|index| json!({ "text": format!("symbol-{index}"), "index": index }))
                .collect::<Vec<_>>(),
            "outline": (0..items)
                .map(|index| json!({ "head": format!("defun-{index}") }))
                .collect::<Vec<_>>(),
        })
    }

    #[test]
    fn an_unlimited_budget_changes_nothing() {
        let mut value = report(50);
        let before = value.clone();
        assert!(Budget(0).apply(&mut value, &["atoms"]).is_none());
        assert_eq!(value, before);
    }

    /// A report that fits must be byte-identical to one produced with no
    /// budget, or `--max-tokens` would change output it did not need to.
    #[test]
    fn a_report_that_already_fits_is_untouched() {
        let mut value = report(3);
        let before = value.clone();
        assert!(Budget(100_000).apply(&mut value, &["atoms"]).is_none());
        assert_eq!(value, before);
    }

    /// What survives is a prefix in source order, so entry 0 is still entry 0
    /// and a caller can page through the rest by other means.
    #[test]
    fn trimming_keeps_a_prefix_rather_than_emptying_the_array() {
        let mut value = report(200);
        let truncation = Budget(2_000)
            .apply(&mut value, &["atoms"])
            .expect("does not fit");

        let atoms = value["atoms"].as_array().expect("atoms");
        assert!(!atoms.is_empty(), "the whole array was dropped");
        assert!(atoms.len() < 200);
        assert_eq!(atoms[0]["index"], 0);
        assert_eq!(atoms[1]["index"], 1);
        assert!(truncation.approximate_tokens <= 2_000);
    }

    /// Silence is the failure mode this type exists to prevent.
    #[test]
    fn every_trimmed_array_is_named_with_what_it_lost() {
        let mut value = report(400);
        let truncation = Budget(200)
            .apply(&mut value, &["atoms", "outline"])
            .expect("does not fit");

        assert!(!truncation.trimmed.is_empty());
        for array in &truncation.trimmed {
            assert!(array.kept < array.total, "{array:?}");
            assert_eq!(array.total, 400);
        }
        let json = truncation.to_json();
        assert_eq!(json["truncated"], true);
        assert_eq!(json["budget_tokens"], 200);
        assert!(!json["arrays"].as_array().expect("arrays").is_empty());
    }

    /// The first key listed is given up first, so a caller can order its
    /// arrays by how much it minds losing them.
    #[test]
    fn the_first_listed_array_is_the_first_given_up() {
        let mut value = report(200);
        Budget(900)
            .apply(&mut value, &["atoms", "outline"])
            .expect("does not fit");

        let atoms = value["atoms"].as_array().expect("atoms").len();
        let outline = value["outline"].as_array().expect("outline").len();
        assert!(
            atoms < outline,
            "atoms ({atoms}) should be given up before outline ({outline})"
        );
    }

    /// A budget below the envelope's own size cannot be met. Reporting that
    /// honestly is the only correct answer; claiming to have met it is not.
    #[test]
    fn a_budget_the_envelope_cannot_meet_is_reported_rather_than_faked() {
        let mut value = report(100);
        let truncation = Budget(1)
            .apply(&mut value, &["atoms", "outline"])
            .expect("over");

        assert!(value["atoms"].as_array().expect("atoms").is_empty());
        assert!(value["outline"].as_array().expect("outline").is_empty());
        assert!(truncation.approximate_tokens > 1);
        assert_eq!(truncation.budget, 1);
    }

    #[test]
    fn a_key_that_is_not_an_array_is_ignored_rather_than_mangled() {
        let mut value = json!({ "schema_version": 1, "note": "x".repeat(4_000) });
        let truncation = Budget(10)
            .apply(&mut value, &["note", "missing"])
            .expect("over budget");
        assert_eq!(value["note"].as_str().expect("note").len(), 4_000);
        assert!(truncation.trimmed.is_empty());
    }

    #[test]
    fn the_estimate_is_bytes_over_four_rounded_up() {
        assert_eq!(approximate_tokens(&json!(1)), 1);
        assert_eq!(approximate_tokens(&json!("abcdefgh")), 3); // 10 bytes with quotes
    }
}
