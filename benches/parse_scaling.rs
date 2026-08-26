//! How does one parse scale with the size of the file it is handed?
//!
//! Every command in this tool begins by reading a whole file into a `String`
//! and handing it to [`SyntaxTree::parse_with_dialect`]. That is the floor
//! under every other measurement in this directory, and it was the only hot
//! path with no benchmark of its own: `lint_report` measures a rule pass over
//! a fixed document, `similarity_report` measures pruning, `cache_dir`
//! measures discovery. None of them would notice a parser that became
//! quadratic in document length.
//!
//! Two arms, because reader conditionals change the answer:
//!
//! * `plain` — ordinary Common Lisp. Every form becomes a full node subtree.
//! * `reader-conditional` — the same forms with `#+sbcl` in front of the body.
//!   Under [`Dialect::CommonLisp`] a reader conditional and the form it
//!   guards fold into a single opaque atom, so nearly the same bytes produce
//!   about a quarter of the nodes. Keeping both arms means a change that
//!   stopped folding — turning that atom back into a subtree — shows up here
//!   as the two arms converging, rather than as a silent 3x memory increase.
//!
//! Wall time only; Criterion cannot measure memory. `tests/parse_memory.rs`
//! is the memory half, measuring peak resident set over the same two shapes
//! at sizes up to 128 MiB. It carries its own copy of the fixture template
//! below, in the same way every other benchmark in this directory builds its
//! fixture inline — the two are independent measurements and neither reads
//! the other's numbers, so a divergence costs comparability in a report, not
//! correctness in a gate.
//!
//! Read the results as a ratio between arms and between sizes of one run,
//! never as absolute numbers: see the header of `scripts/bench-compare.sh`.

use std::hint::black_box;
use std::time::{Duration, Instant};

use criterion::{
    BenchmarkId, Criterion, SamplingMode, Throughput, criterion_group, criterion_main,
};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ExpressionPath, SyntaxTree};

/// Target document sizes, in mebibytes.
///
/// Two points, an order of magnitude apart, which is what it takes to catch a
/// change in the *shape* of the cost curve — the thing this benchmark exists
/// for. `tests/parse_memory.rs` runs the same fixture up to 128 MiB; that
/// size does not belong here, because at the measured throughput a single
/// iteration of it takes about a second and fifty samples of it would take
/// longer than the rest of this directory put together.
const SIZES_MEGABYTES: [usize; 2] = [1, 8];

const BYTES_PER_MEGABYTE: usize = 1024 * 1024;

#[derive(Clone, Copy)]
enum Shape {
    Plain,
    ReaderConditional,
}

impl Shape {
    const ALL: [Self; 2] = [Self::Plain, Self::ReaderConditional];

    const fn label(self) -> &'static str {
        match self {
            Self::Plain => "plain",
            Self::ReaderConditional => "reader-conditional",
        }
    }

    /// One instance of the fixture template.
    ///
    /// Zero-padded to a fixed width so every instance is the same length and
    /// the generator can size its buffer exactly, keeping `String` growth out
    /// of a fixture whose whole point is its size.
    fn form(self, index: usize) -> String {
        let body = "\
(let ((sum (+ alpha beta))
        (product (* alpha beta)))
    (if (> sum product)
        (list sum product)
        (cons sum product)))";
        let guard = match self {
            Self::Plain => "",
            Self::ReaderConditional => "#+sbcl ",
        };
        format!(
            "(defun bench-form-{index:07} (alpha beta)\n  \"Combine ALPHA and BETA.\"\n  {guard}{body})\n\n"
        )
    }

    /// One synthetic document of roughly `target_bytes`, built by repeating
    /// [`Self::form`].
    ///
    /// Deliberately uniform: every form has the same depth, arity and token
    /// lengths. That makes the fixture reproducible and the two arms directly
    /// comparable, and it means the absolute throughput is a property of this
    /// template rather than of real source, which has longer strings and more
    /// varied nesting. The gate compares two revisions over the same
    /// template, so that is the property it needs.
    fn document(self, target_bytes: usize) -> String {
        let per_form = self.form(0).len();
        let forms = target_bytes.div_ceil(per_form);
        let mut source = String::with_capacity(forms * per_form);
        for index in 0..forms {
            source.push_str(&self.form(index));
        }
        source
    }
}

/// Fails the benchmark on a fixture that would measure nothing.
///
/// A document the parser rejects still benchmarks happily — it just measures
/// how fast the parser reaches the error, which for an unbalanced template is
/// the first few bytes. And the two arms are only comparable while they are
/// the same size, which the `#+sbcl` prefix would break if the form count
/// were not recomputed per shape.
fn assert_measurable(shape: Shape, source: &str, target_bytes: usize) {
    let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp)
        .expect("the generated fixture is valid Common Lisp");
    assert!(
        !tree.root_children().is_empty(),
        "the {} fixture parsed into no top-level forms",
        shape.label()
    );
    assert!(
        source.len() >= target_bytes,
        "the {} fixture is smaller than the size it is labelled with",
        shape.label()
    );
}

fn benchmark_parse_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse-scaling");
    // Every other benchmark here leaves this on `Auto`, which scales the
    // iteration count linearly across samples. That costs 1275 iterations for
    // fifty samples — fine for a routine measured in microseconds, and about
    // a hundred seconds for the 8 MiB arm. `Flat` runs each sample for the
    // measurement time instead, so this target's cost is bounded by the
    // config below rather than by how slow the largest fixture is.
    group.sampling_mode(SamplingMode::Flat);

    for shape in Shape::ALL {
        for megabytes in SIZES_MEGABYTES {
            let source = shape.document(megabytes * BYTES_PER_MEGABYTE);
            assert_measurable(shape, &source, megabytes * BYTES_PER_MEGABYTE);

            group.throughput(Throughput::Bytes(source.len() as u64));
            group.bench_with_input(
                BenchmarkId::new(shape.label(), format!("{megabytes}MiB")),
                source.as_str(),
                |bencher, source| {
                    // `iter_custom` rather than `iter`, to keep the tree's
                    // destructor out of the measurement without keeping the
                    // trees. Dropping an 8 MiB parse means freeing about
                    // 1.4 million nodes and their child vectors, which is
                    // real work and is not parsing; Criterion's `iter` times
                    // it, and its `iter_with_large_drop` excludes it only by
                    // retaining every tree in the batch — over a hundred
                    // megabytes each.
                    bencher.iter_custom(|iterations| {
                        let mut elapsed = Duration::ZERO;
                        for _ in 0..iterations {
                            let started = Instant::now();
                            let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp)
                                .expect("the fixture is valid Common Lisp");
                            elapsed += started.elapsed();
                            drop(black_box(tree));
                        }
                        elapsed
                    });
                },
            );
        }
    }

    group.finish();
}

fn benchmark_wide_sibling_path(c: &mut Criterion) {
    let mut group = c.benchmark_group("wide-sibling-path");

    for width in [1_024, 65_536] {
        let mut source = String::with_capacity(width * 2 + 2);
        source.push('(');
        for _ in 0..width {
            source.push_str("x ");
        }
        source.push(')');

        let tree = SyntaxTree::parse(&source).expect("the generated fixture is valid");
        let path = ExpressionPath::from_indexes(vec![0, width - 1]);
        let selection = tree
            .select_path(&path)
            .expect("the final sibling exists in the generated fixture");
        assert_eq!(selection.path(), path);

        group.throughput(Throughput::Elements(width as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(width),
            &selection,
            |bencher, selection| {
                bencher.iter(|| black_box(selection.path()));
            },
        );
    }

    group.finish();
}

/// Enough samples for the confidence interval to be narrower than the gate's
/// threshold — see the same function in `benches/similarity_report.rs`.
///
/// Under the flat sampling selected above, `measurement_time` is very nearly
/// this target's whole cost: four arms of about five seconds each. Lowering
/// the sample count instead of the measurement time would be the cheaper
/// knob, and the wrong one — a narrow confidence interval is what lets the
/// gate distinguish a regression from a noisy runner.
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
    targets = benchmark_parse_scaling, benchmark_wide_sibling_path
}
criterion_main!(benches);
