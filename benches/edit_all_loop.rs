//! What does one `--all` edit cost per match it applies?
//!
//! `edit_target_with` is the shared body of every structure-editing command
//! that takes a selector: `edit splice`, `edit kill`, `edit slurp-forward`,
//! and a dozen more. Given `--all`, it applies one edit per match, right to
//! left, re-parsing the whole document between them so that a later match
//! cannot be resolved against text an earlier edit has moved.
//!
//! That re-parse is the cost this measures, and it is the only benchmark here
//! whose work grows with the *number of matches* rather than with document
//! size. `parse_scaling` measures one parse of a large file; this measures
//! many parses of a medium one, which is the shape a real rename or splice
//! across a module has. Neither would notice the other's regression: a change
//! that made the loop parse twice per match instead of once leaves
//! `parse_scaling` flat, and cost this benchmark a factor of two.
//!
//! The match count is the swept axis for the same reason. Document size is
//! held roughly proportional to it on purpose -- a file with four hundred call
//! sites is not the same size as one with twenty-five -- so the arms below
//! trace the loop's real quadratic shape, and a change to the *number* of
//! parses per match shows up as a constant factor between two revisions at
//! every arm rather than as a slope change at one.
//!
//! Each match count is measured twice, as `parse-every-pass` and
//! `carried-parse`: the loop before and after the parse `normalize` makes is
//! carried into the next pass. Both arms are in one group so the speedup is a
//! ratio inside a single run, rather than a comparison against a number
//! written down in another session on another machine.
//!
//! Read the results as a ratio between arms and between sizes of one run,
//! never as absolute numbers: see the header of `scripts/bench-compare.sh`.

use std::hint::black_box;
use std::time::{Duration, Instant};

use criterion::{
    BenchmarkId, Criterion, SamplingMode, Throughput, criterion_group, criterion_main,
};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, Edit, SyntaxTree};

/// Matches to edit in one invocation.
///
/// Four points rather than two, because the loop is quadratic in this number
/// and a single pair cannot tell a constant-factor change from a change in
/// that exponent.
const MATCH_COUNTS: [usize; 4] = [25, 50, 100, 200];

/// One top-level form carrying exactly one `(target-call x)` to edit.
fn form(index: usize) -> String {
    format!(
        "(defun helper-{index:07} (alpha beta)\n  \"Combine ALPHA and BETA.\"\n  (let ((sum (+ alpha beta))\n        (product (* alpha beta)))\n    (list sum product (target-call alpha) {index})))\n\n"
    )
}

fn document(matches: usize) -> String {
    (0..matches).map(form).collect()
}

/// Every `(target-call ...)` span, in source order.
///
/// Resolved once, outside the measurement, because the selector machinery
/// that finds them in the real command is `paredit-feature-query`'s cost, not
/// this loop's.
fn target_spans(source: &str, tree: &SyntaxTree, matches: usize) -> Vec<ByteSpan> {
    let mut spans = Vec::new();
    let mut cursor = 0usize;
    while let Some(found) = source[cursor..].find("(target-call ") {
        let start = cursor + found;
        let selection = tree
            .select_at(start)
            .expect("the generated call site is a node");
        assert_eq!(
            selection.span().start().get(),
            start,
            "the generated call site is a node in its own right"
        );
        spans.push(selection.span());
        cursor = start + 1;
    }
    assert_eq!(
        spans.len(),
        matches,
        "the fixture must offer exactly the swept number of matches"
    );
    spans
}

/// `edit_target_with`'s loop, less the parts that read files and print.
///
/// Kept in step with `paredit_core_cli::shared::edit_target_with` by hand.
/// Calling it directly is not possible from here: it takes its document from a
/// file or stdin and writes the result to stdout.
fn edit_every_match(
    source: &str,
    spans: &[ByteSpan],
    dialect: Dialect,
    seed: SyntaxTree,
) -> String {
    let mut current = source.to_owned();
    let mut parsed = Some(seed);
    for span in spans.iter().rev() {
        let tree = match parsed.take() {
            Some(tree) => tree,
            None => SyntaxTree::parse_with_dialect(&current, dialect)
                .expect("the document under edit parses"),
        };
        let selection = tree
            .select_at(span.start().get())
            .expect("the match is still where it was");
        let rewritten = Edit::splice(&current, &tree, selection).expect("splicing a call succeeds");
        let (normalized, reusable) =
            Edit::normalize_changed_line_trivia_reusing_parse(&current, rewritten, dialect)
                .expect("the rewrite parses");
        parsed = if normalized == current {
            Some(tree)
        } else {
            reusable
        };
        current = normalized;
    }
    current
}

/// The same loop as it was before the parse was carried between passes.
///
/// The A in the A/B: it parses the document itself on every pass, and calls
/// the `normalize` that discards the parse it makes internally, so the pair
/// below differs in exactly one variable. Deliberately *not* the differential
/// test's `edit_loop_parsing_every_pass`, which lives behind `#[cfg(test)]` in
/// `paredit-core-syntax` and so does not exist in this crate's build at all;
/// copying it would also have changed two more variables, since it kills
/// rather than splices and clones the document into a trace on every pass.
fn edit_every_match_parsing_every_pass(
    source: &str,
    spans: &[ByteSpan],
    dialect: Dialect,
) -> String {
    let mut current = source.to_owned();
    for span in spans.iter().rev() {
        let tree = SyntaxTree::parse_with_dialect(&current, dialect)
            .expect("the document under edit parses");
        let selection = tree
            .select_at(span.start().get())
            .expect("the match is still where it was");
        let rewritten = Edit::splice(&current, &tree, selection).expect("splicing a call succeeds");
        current = Edit::normalize_changed_line_trivia(&current, rewritten, dialect)
            .expect("the rewrite parses");
    }
    current
}

/// Fails the benchmark on a fixture that would measure nothing.
///
/// A loop that refuses on its first match still benchmarks happily -- it just
/// measures how fast it reaches the refusal.
fn assert_measurable(source: &str, spans: &[ByteSpan], dialect: Dialect) {
    let edited = edit_every_match(
        source,
        spans,
        dialect,
        SyntaxTree::parse_with_dialect(source, dialect).expect("the fixture is valid Common Lisp"),
    );
    assert!(
        !edited.contains("(target-call "),
        "every match should have been spliced away"
    );
    SyntaxTree::parse_with_dialect(&edited, dialect).expect("the edited document still parses");
    // A ratio between the two arms only means anything if they did the same
    // work. This is what stops the old-loop arm drifting into a cheaper or
    // costlier job than the new one and reporting the drift as a speedup.
    assert_eq!(
        edit_every_match_parsing_every_pass(source, spans, dialect),
        edited,
        "the two loop shapes must agree on the edited document"
    );
}

fn benchmark_edit_all_loop(criterion: &mut Criterion) {
    let dialect = Dialect::CommonLisp;
    let mut group = criterion.benchmark_group("edit-all-loop");
    // Flat, for the reason `parse_scaling` gives: the 200-match arm is
    // hundreds of parses of a 20 KiB document, and linearly scaled iteration
    // counts would make this target's cost depend on its slowest arm.
    group.sampling_mode(SamplingMode::Flat);

    for matches in MATCH_COUNTS {
        let source = document(matches);
        let tree = SyntaxTree::parse_with_dialect(&source, dialect).expect("the fixture parses");
        let spans = target_spans(&source, &tree, matches);
        assert_measurable(&source, &spans, dialect);

        // Bytes rewritten, not bytes in the file: the loop walks the whole
        // document once per match, so this is what its throughput is per.
        group.throughput(Throughput::Bytes((source.len() * matches) as u64));
        group.bench_with_input(
            BenchmarkId::new("parse-every-pass", matches),
            &spans,
            |bencher, spans| {
                // No `iter_custom`: this arm builds no seed, so there is
                // nothing to keep out of the measurement. Its first pass pays
                // for the parse the other arm is handed, which is exactly the
                // difference being measured.
                bencher.iter(|| {
                    black_box(edit_every_match_parsing_every_pass(
                        &source,
                        black_box(spans),
                        dialect,
                    ))
                });
            },
        );
        group.bench_with_input(
            BenchmarkId::new("carried-parse", matches),
            &spans,
            |bencher, spans| {
                // `iter_custom` to keep the seed parse each iteration needs out
                // of the measurement. The loop consumes the tree it is handed,
                // so it cannot be built once and reused across iterations, and
                // timing it would measure `parse_scaling`'s subject instead.
                bencher.iter_custom(|iterations| {
                    let mut elapsed = Duration::ZERO;
                    for _ in 0..iterations {
                        let seed = SyntaxTree::parse_with_dialect(&source, dialect)
                            .expect("the fixture parses");
                        let started = Instant::now();
                        let edited = edit_every_match(&source, spans, dialect, seed);
                        elapsed += started.elapsed();
                        drop(black_box(edited));
                    }
                    elapsed
                });
            },
        );
    }

    group.finish();
}

/// Enough samples for the confidence interval to be narrower than the gate's
/// threshold -- see the same function in `benches/similarity_report.rs`.
fn criterion_config() -> Criterion {
    Criterion::default()
        .sample_size(50)
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(5))
        .without_plots()
}

criterion_group! {
    name = benches;
    config = criterion_config();
    targets = benchmark_edit_all_loop
}
criterion_main!(benches);
