//! Taking a preview manifest apart into individually approvable steps.
//!
//! `refactor apply` is all-or-nothing: the manifest describes a rewrite and
//! applying it writes every edit in it. That is the right default — a rename
//! half-applied is a broken program — but it is the wrong shape for review. A
//! reader who disagrees with one of forty edits has to discard the manifest and
//! re-derive a narrower one, and in practice discards the review instead.
//!
//! So this makes the manifest steppable: each edit becomes a numbered step with
//! its own context, and a caller says which ones to take. What it does *not* do
//! is pretend that is always safe — see [`selection_is_coherent`].

use std::collections::BTreeSet;

use paredit_core_syntax::sexpr::ByteSpan;

/// One edit, numbered and described.
#[derive(Debug, Clone)]
pub struct Step {
    /// The step's 1-based number, stable for one manifest: numbering is by
    /// file order and then by byte offset, both of which the manifest fixes.
    pub number: usize,
    pub file_index: usize,
    pub path: String,
    pub span: ByteSpan,
    pub line: usize,
    /// The source this edit replaces.
    pub before: String,
    pub after: String,
    /// The line the edit sits on, for a reader deciding without the file open.
    pub context: String,
}

/// Which steps a caller chose.
#[derive(Debug, Clone, Default)]
pub struct Selection {
    accepted: BTreeSet<usize>,
}

impl Selection {
    /// Every step.
    #[must_use]
    pub fn all(count: usize) -> Self {
        Self {
            accepted: (1..=count).collect(),
        }
    }

    #[must_use]
    pub fn none() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn contains(&self, number: usize) -> bool {
        self.accepted.contains(&number)
    }

    pub fn accept(&mut self, number: usize) {
        self.accepted.insert(number);
    }

    pub fn skip(&mut self, number: usize) {
        self.accepted.remove(&number);
    }

    #[must_use]
    pub fn count(&self) -> usize {
        self.accepted.len()
    }
}

/// Parses a step selector: `all`, `none`, `3`, `1,4`, `2-5`, or a mix.
///
/// Returns the numbers named, so the caller decides whether they are being
/// accepted or skipped. A number outside the manifest is an error rather than
/// a silent no-op: it almost always means the caller is holding a stale plan,
/// and stepping the wrong plan is exactly what this command exists to prevent.
pub fn parse_selector(spec: &str, count: usize) -> Result<BTreeSet<usize>, String> {
    let mut numbers = BTreeSet::new();
    match spec.trim() {
        "all" => return Ok((1..=count).collect()),
        "none" => return Ok(numbers),
        _ => {}
    }

    for part in spec
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
    {
        let range = match part.split_once('-') {
            Some((start, end)) => {
                let start = number(start, count)?;
                let end = number(end, count)?;
                if start > end {
                    return Err(format!("{part:?} counts backwards"));
                }
                start..=end
            }
            None => {
                let single = number(part, count)?;
                single..=single
            }
        };
        numbers.extend(range);
    }

    if numbers.is_empty() {
        return Err(format!("{spec:?} names no steps"));
    }
    Ok(numbers)
}

fn number(text: &str, count: usize) -> Result<usize, String> {
    let value: usize = text
        .trim()
        .parse()
        .map_err(|_| format!("{text:?} is not a step number"))?;
    if value == 0 || value > count {
        return Err(format!(
            "step {value} is outside this manifest, which has {count} step(s); \
             the plan may be stale"
        ));
    }
    Ok(value)
}

/// Whether taking only these steps leaves a coherent rewrite.
///
/// The honest answer is usually no, and saying so is the point. A rename's
/// edits are one change: applying the edit at the definition and skipping one
/// at a call site produces a program that does not compile. This does not
/// refuse — a reviewer may be deliberately splitting a change across commits —
/// but it is what `--fail-on-partial` gates on, so a *script* cannot take a
/// subset by accident.
#[must_use]
pub fn selection_is_coherent(steps: &[Step], selection: &Selection) -> bool {
    steps.iter().all(|step| selection.contains(step.number))
}

/// Numbers the manifest's edits and describes each.
///
/// `sources` is each file's current text, in manifest file order.
#[must_use]
pub fn steps(files: &[(String, Vec<(ByteSpan, String)>)], sources: &[String]) -> Vec<Step> {
    let mut steps = Vec::new();
    let mut number = 0;
    for (file_index, (path, edits)) in files.iter().enumerate() {
        let source = sources.get(file_index).map(String::as_str).unwrap_or("");
        let mut ordered: Vec<&(ByteSpan, String)> = edits.iter().collect();
        ordered.sort_by_key(|(span, _)| (span.start().get(), span.end().get()));
        for (span, replacement) in ordered {
            number += 1;
            let start = span.start().get();
            let end = span.end().get();
            steps.push(Step {
                number,
                file_index,
                path: path.clone(),
                span: *span,
                line: line_of(source, start),
                before: source.get(start..end).unwrap_or_default().to_owned(),
                after: replacement.clone(),
                context: line_text(source, start),
            });
        }
    }
    steps
}

fn line_of(text: &str, offset: usize) -> usize {
    text.as_bytes()
        .iter()
        .take(offset.min(text.len()))
        .filter(|byte| **byte == b'\n')
        .count()
        + 1
}

/// The whole source line an offset sits on, trimmed.
fn line_text(text: &str, offset: usize) -> String {
    let clamped = offset.min(text.len());
    let start = text[..clamped].rfind('\n').map_or(0, |index| index + 1);
    let end = text[clamped..]
        .find('\n')
        .map_or(text.len(), |index| clamped + index);
    text.get(start..end).unwrap_or_default().trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::sexpr::ByteOffset;

    fn span(start: usize, end: usize) -> ByteSpan {
        ByteSpan::new(ByteOffset::new(start), ByteOffset::new(end))
    }

    const SOURCE: &str = "(defun helper (x) (* x 2))\n(defun main (y)\n  (helper y))\n";

    fn sample() -> Vec<Step> {
        steps(
            &[(
                "core.lisp".to_owned(),
                vec![
                    (span(45, 51), "helper2".to_owned()),
                    (span(7, 13), "helper2".to_owned()),
                ],
            )],
            &[SOURCE.to_owned()],
        )
    }

    /// Numbering must not depend on the order the manifest happened to list
    /// edits in: a caller who re-reads the same manifest must see the same
    /// numbers, or `--accept 3` means something different every run.
    #[test]
    fn steps_are_numbered_in_source_order_whatever_order_the_manifest_used() {
        let steps = sample();
        assert_eq!(steps[0].number, 1);
        assert_eq!(steps[0].span.start().get(), 7);
        assert_eq!(steps[1].span.start().get(), 45);
    }

    #[test]
    fn a_step_carries_the_text_it_replaces_and_the_line_it_is_on() {
        let steps = sample();
        assert_eq!(steps[0].before, "helper");
        assert_eq!(steps[0].after, "helper2");
        assert_eq!(steps[0].line, 1);
        assert_eq!(steps[1].line, 3);
        assert_eq!(steps[1].context, "(helper y))");
    }

    #[test]
    fn a_selector_accepts_singles_ranges_and_the_two_keywords() {
        assert_eq!(parse_selector("all", 3).expect("all"), [1, 2, 3].into());
        assert!(parse_selector("none", 3).expect("none").is_empty());
        assert_eq!(parse_selector("2", 3).expect("single"), [2].into());
        assert_eq!(parse_selector("1,3", 3).expect("list"), [1, 3].into());
        assert_eq!(parse_selector("1-3", 3).expect("range"), [1, 2, 3].into());
        assert_eq!(parse_selector("1,3-4", 4).expect("mixed"), [1, 3, 4].into());
    }

    /// A number outside the manifest almost always means a stale plan, and
    /// stepping the wrong plan is what this command exists to prevent.
    #[test]
    fn a_selector_naming_a_step_that_does_not_exist_is_refused() {
        let error = parse_selector("9", 3).expect_err("out of range");
        assert!(error.contains("stale"), "{error}");
        assert!(parse_selector("0", 3).is_err());
        assert!(parse_selector("3-1", 3).is_err());
        assert!(parse_selector("x", 3).is_err());
    }

    /// A rename's edits are one change. Taking a subset is a thing a reviewer
    /// may mean and a script never should.
    #[test]
    fn a_partial_selection_is_reported_as_incoherent() {
        let steps = sample();
        assert!(selection_is_coherent(&steps, &Selection::all(steps.len())));

        let mut partial = Selection::all(steps.len());
        partial.skip(1);
        assert!(!selection_is_coherent(&steps, &partial));
        assert_eq!(partial.count(), 1);
    }
}
