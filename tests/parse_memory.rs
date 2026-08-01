//! How much memory does parsing one huge file actually cost?
//!
//! `benches/parse_scaling.rs` measures the wall time of the same parses.
//! Criterion cannot measure memory, and the question this file answers is a
//! memory question: `SyntaxTree` keeps its own copy of the source *and* a
//! flat `Vec<Node>` whose entries are far larger than the tokens they
//! describe, so the peak resident set of a parse is some multiple of the file
//! size — and nobody had ever measured the multiple. A streaming or
//! incremental parser is only worth its complexity if that multiple is large.
//!
//! Ignored by default. It allocates gigabytes at the larger sizes and reports
//! numbers rather than asserting on them; a threshold would only measure the
//! machine it last ran on. Run it deliberately:
//!
//! ```console
//! $ cargo test --profile bench --test parse_memory -- --ignored --nocapture
//! ```
//!
//! `--profile bench` is not optional in practice. The peak-memory figures are
//! the same either way, but an unoptimised parse of the 128 MiB case takes
//! long enough to look hung.
//!
//! ## Why each size runs in its own process
//!
//! Both platforms report a *high-water* mark that never falls: `VmHWM` on
//! Linux, `ru_maxrss` on macOS. Measuring several sizes in one process would
//! report every case at the largest case's peak, and freeing a tree between
//! cases would not help — the mark does not come back down. So the test
//! re-executes its own binary once per case and reads a single measurement
//! out of each child.
//!
//! ## What the numbers do and do not represent
//!
//! The fixture is one template form repeated to reach the target size, which
//! is the simplest generator that produces valid, parseable, structurally
//! typical Lisp. It is *not* a realistic file: every form has the same depth,
//! the same arity and the same token lengths, so the node-count-per-byte
//! ratio it produces is a property of the template. Real source with longer
//! docstrings, long string literals or deeply nested macros will land at a
//! different ratio. The ratio's *shape* — whether cost grows linearly with
//! file size — is what this measures reliably; the constant is indicative.

use std::hint::black_box;
use std::process::Command;
use std::time::Instant;

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ExpressionView, SyntaxTree};

/// Set by the parent on each child it spawns; its presence is what puts a
/// process into single-case mode.
const CASE_MEGABYTES: &str = "PAREDIT_PARSE_MEMORY_MEGABYTES";
/// `plain` or `reader-conditional`, see [`Shape`].
const CASE_SHAPE: &str = "PAREDIT_PARSE_MEMORY_SHAPE";

/// Target file sizes, in mebibytes.
///
/// 128 MiB is the top of the range because it is roughly where a single
/// source file stops being conceivable and starts being a bug report — and
/// because at the measured ratio it is already the largest allocation a
/// developer machine can be asked for without swapping. The intermediate
/// points exist to answer the linearity question: three points on a line
/// prove more than two.
const SIZES_MEGABYTES: [usize; 4] = [1, 8, 32, 128];

const BYTES_PER_MEGABYTE: usize = 1024 * 1024;

/// The two fixture shapes, which differ only in whether the bulk of each
/// form is behind a reader conditional.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Shape {
    /// Ordinary Common Lisp: every form parses into the full node tree.
    Plain,
    /// The same forms with `#+sbcl` in front of the body. Under
    /// [`Dialect::CommonLisp`] the reader conditional and the whole form it
    /// guards fold into one opaque atom, so almost the same bytes produce
    /// far fewer nodes.
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

    fn from_label(label: &str) -> Self {
        match label {
            "plain" => Self::Plain,
            "reader-conditional" => Self::ReaderConditional,
            other => panic!("unknown fixture shape {other}"),
        }
    }

    /// One instance of the fixture template.
    ///
    /// Zero-padded to a fixed width so every instance is byte-identical in
    /// length: the generator can then size its buffer exactly and avoid a
    /// `String` growth spike that would show up in the peak it is measuring.
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
}

/// A generated fixture, plus the facts about it the report needs.
struct Fixture {
    source: String,
    forms: usize,
    /// Nodes the parse will build, derived by parsing one template instance
    /// and multiplying. Counting the real tree would mean materialising an
    /// `ExpressionView` per node — a second, larger tree — inside the
    /// measurement it is supposed to describe.
    nodes: usize,
}

impl Fixture {
    fn generate(shape: Shape, target_bytes: usize) -> Self {
        let per_form = shape.form(0).len();
        let forms = target_bytes.div_ceil(per_form);

        let mut source = String::with_capacity(forms * per_form);
        for index in 0..forms {
            source.push_str(&shape.form(index));
        }
        assert_eq!(
            source.len(),
            forms * per_form,
            "every template instance must be the same length, or the \
             pre-sized buffer above is wrong"
        );

        let nodes = nodes_in_one_form(shape) * forms;
        Self {
            source,
            forms,
            nodes,
        }
    }
}

/// Node count of a single template instance, measured rather than asserted:
/// the folding behaviour under test is exactly what would make a hand-counted
/// constant wrong.
fn nodes_in_one_form(shape: Shape) -> usize {
    let tree = SyntaxTree::parse_with_dialect(&shape.form(0), Dialect::CommonLisp)
        .expect("the fixture template is valid Common Lisp");
    count(&tree.root_view())
}

fn count(view: &ExpressionView) -> usize {
    1 + view.children.iter().map(count).sum::<usize>()
}

/// The process's peak resident set size, in bytes.
///
/// High-water marks, not current usage: they only ever rise, which is why
/// each case gets its own process.
#[cfg(target_os = "linux")]
fn peak_resident_bytes() -> u64 {
    // `/proc/self/status` rather than `getrusage`, whose `ru_maxrss` is in
    // kilobytes on Linux and bytes on macOS — a unit difference that is
    // silent and off by 1024.
    let status = std::fs::read_to_string("/proc/self/status").expect("read /proc/self/status");
    for line in status.lines() {
        if let Some(value) = line.strip_prefix("VmHWM:") {
            let kilobytes = value
                .trim()
                .trim_end_matches("kB")
                .trim()
                .parse::<u64>()
                .expect("VmHWM is a decimal number of kilobytes");
            return kilobytes * 1024;
        }
    }
    panic!("/proc/self/status has no VmHWM line");
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
fn peak_resident_bytes() -> u64 {
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::uninit();
    // SAFETY: `getrusage` fully initialises the `rusage` it is handed when it
    // returns 0, and the pointer is to a live, correctly typed local.
    let outcome = unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) };
    assert_eq!(outcome, 0, "getrusage(RUSAGE_SELF) failed");
    // SAFETY: `getrusage` returned 0, so the value is initialised.
    let usage = unsafe { usage.assume_init() };
    // Bytes on macOS. Linux reports the same field in kilobytes, which is why
    // that platform reads `/proc/self/status` instead.
    u64::try_from(usage.ru_maxrss).expect("a peak resident size is not negative")
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn peak_resident_bytes() -> u64 {
    panic!("no peak-resident-set source is implemented for this platform");
}

/// One measurement, as the child produces it and the parent parses it back.
struct Measurement {
    source_bytes: usize,
    forms: usize,
    nodes: usize,
    /// Peak before the fixture exists: the process's own footprint, which is
    /// a few megabytes and therefore not negligible at the 1 MiB case.
    baseline_bytes: u64,
    /// Peak after generating the source but before parsing: the floor any
    /// parse strategy has to pay, including a streaming one that never held
    /// the whole file.
    before_parse_bytes: u64,
    /// Peak with the tree alive.
    after_parse_bytes: u64,
    parse_nanos: u128,
}

/// A line the child prints and the parent scrapes.
///
/// Deliberately not JSON: the payload is six integers, and the parent has to
/// find it inside libtest's own output either way.
const RESULT_PREFIX: &str = "paredit-parse-memory";

impl Measurement {
    fn emit(&self) {
        println!(
            "{RESULT_PREFIX} source_bytes={} forms={} nodes={} baseline_bytes={} \
             before_parse_bytes={} after_parse_bytes={} parse_nanos={}",
            self.source_bytes,
            self.forms,
            self.nodes,
            self.baseline_bytes,
            self.before_parse_bytes,
            self.after_parse_bytes,
            self.parse_nanos
        );
    }

    fn parse_from(output: &str) -> Self {
        // Not `starts_with`: libtest writes `test <name> ... ` without a
        // newline before releasing the child's own stdout, so the result
        // lands in the middle of that line.
        let line = output
            .lines()
            .find_map(|line| line.split_once(RESULT_PREFIX).map(|(_, rest)| rest))
            .unwrap_or_else(|| panic!("the child printed no result line:\n{output}"));

        let field = |name: &str| -> u128 {
            line.split_whitespace()
                .find_map(|pair| pair.strip_prefix(&format!("{name}=")))
                .unwrap_or_else(|| panic!("the result line has no {name} field: {line}"))
                .parse()
                .expect("every result field is a decimal integer")
        };

        Self {
            source_bytes: field("source_bytes") as usize,
            forms: field("forms") as usize,
            nodes: field("nodes") as usize,
            baseline_bytes: field("baseline_bytes") as u64,
            before_parse_bytes: field("before_parse_bytes") as u64,
            after_parse_bytes: field("after_parse_bytes") as u64,
            parse_nanos: field("parse_nanos"),
        }
    }

    /// Peak resident bytes per source byte, with the tree alive, net of the
    /// process's own footprint.
    fn total_ratio(&self) -> f64 {
        (self.after_parse_bytes.saturating_sub(self.baseline_bytes)) as f64
            / self.source_bytes as f64
    }

    /// The tree's own share, i.e. everything the parse added on top of
    /// already holding the source in memory.
    fn tree_ratio(&self) -> f64 {
        (self
            .after_parse_bytes
            .saturating_sub(self.before_parse_bytes)) as f64
            / self.source_bytes as f64
    }

    fn bytes_per_node(&self) -> f64 {
        (self
            .after_parse_bytes
            .saturating_sub(self.before_parse_bytes)) as f64
            / self.nodes as f64
    }

    fn megabytes_per_second(&self) -> f64 {
        let seconds = self.parse_nanos as f64 / 1e9;
        (self.source_bytes as f64 / BYTES_PER_MEGABYTE as f64) / seconds
    }
}

/// The single-case body, run in a child process.
fn measure(shape: Shape, target_bytes: usize) {
    // Read before the fixture exists: at the 1 MiB case the process's own
    // few megabytes would otherwise dominate the ratio being reported.
    let baseline_bytes = peak_resident_bytes();

    let fixture = Fixture::generate(shape, target_bytes);
    let before_parse_bytes = peak_resident_bytes();

    let started = Instant::now();
    let tree = SyntaxTree::parse_with_dialect(&fixture.source, Dialect::CommonLisp)
        .expect("the generated fixture is valid Common Lisp");
    let parse_nanos = started.elapsed().as_nanos();

    let after_parse_bytes = peak_resident_bytes();

    // The peak has to be read while the tree is still alive, but the tree
    // also has to stay alive across the read, or the optimiser is free to
    // drop it first and the measurement becomes the source string's.
    assert_eq!(
        black_box(&tree).root_children().len(),
        fixture.forms,
        "the parse produced a different top-level form count than the \
         generator wrote"
    );

    Measurement {
        source_bytes: fixture.source.len(),
        forms: fixture.forms,
        nodes: fixture.nodes,
        baseline_bytes,
        before_parse_bytes,
        after_parse_bytes,
        parse_nanos,
    }
    .emit();
}

/// Runs one case in a fresh process and returns what it measured.
fn spawn_case(shape: Shape, megabytes: usize) -> Measurement {
    let binary = std::env::current_exe().expect("the test binary knows its own path");
    let output = Command::new(binary)
        .args([
            "parse_memory_scaling",
            "--exact",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CASE_MEGABYTES, megabytes.to_string())
        .env(CASE_SHAPE, shape.label())
        .output()
        .expect("re-execute the test binary");

    assert!(
        output.status.success(),
        "the {} {megabytes} MiB case failed:\n{}\n{}",
        shape.label(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    Measurement::parse_from(&String::from_utf8_lossy(&output.stdout))
}

#[test]
#[ignore = "measurement, not an assertion: allocates gigabytes and re-executes itself once per case"]
fn parse_memory_scaling() {
    if let Ok(megabytes) = std::env::var(CASE_MEGABYTES) {
        let shape = Shape::from_label(
            &std::env::var(CASE_SHAPE).expect("the parent sets both case variables together"),
        );
        let megabytes: usize = megabytes
            .parse()
            .expect("the case size is a decimal integer");
        measure(shape, megabytes * BYTES_PER_MEGABYTE);
        return;
    }

    println!(
        "platform: {} {}",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    for shape in Shape::ALL {
        println!("\nshape: {}", shape.label());
        println!(
            "{:>7}  {:>12}  {:>10}  {:>12}  {:>12}  {:>10}  {:>7}  {:>7}  {:>8}  {:>9}  {:>9}",
            "MiB",
            "source B",
            "nodes",
            "peak base B",
            "peak post B",
            "tree B",
            "total x",
            "tree x",
            "B/node",
            "parse ms",
            "MiB/s"
        );
        for megabytes in SIZES_MEGABYTES {
            let measurement = spawn_case(shape, megabytes);
            println!(
                "{megabytes:>7}  {:>12}  {:>10}  {:>12}  {:>12}  {:>10}  {:>7.2}  {:>7.2}  {:>8.1}  {:>9.1}  {:>9.1}",
                measurement.source_bytes,
                measurement.nodes,
                measurement.baseline_bytes,
                measurement.after_parse_bytes,
                measurement
                    .after_parse_bytes
                    .saturating_sub(measurement.before_parse_bytes),
                measurement.total_ratio(),
                measurement.tree_ratio(),
                measurement.bytes_per_node(),
                measurement.parse_nanos as f64 / 1e6,
                measurement.megabytes_per_second(),
            );
        }
    }
}
