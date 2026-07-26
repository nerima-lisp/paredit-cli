use std::path::Path as FsPath;
use std::{fmt::Write as _, ops::Range};

const DIFF_CONTEXT_LINES: usize = 3;
const MAX_LCS_CELLS: usize = 4 * 1024 * 1024;
const MAX_DIFF_LINES: usize = 256 * 1024;
const MAX_DIFF_OPS: usize = 512 * 1024;
const MAX_DIFF_OUTPUT_BYTES: usize = 16 * 1024 * 1024;
const MAX_DISPLAY_PATH_BYTES: usize = 4 * 1024;
const MISSING_NEWLINE_MARKER: &str = "\\ No newline at end of file\n";

#[derive(Clone, Copy)]
struct DiffLimits {
    max_lines: usize,
    max_ops: usize,
    max_output_bytes: usize,
}

const DEFAULT_DIFF_LIMITS: DiffLimits = DiffLimits {
    max_lines: MAX_DIFF_LINES,
    max_ops: MAX_DIFF_OPS,
    max_output_bytes: MAX_DIFF_OUTPUT_BYTES,
};

#[derive(Clone, Copy)]
enum DiffOp {
    Equal(usize, usize),
    Delete(usize),
    Insert(usize),
}

pub(crate) fn unified_diff(path: &FsPath, before: &str, after: &str) -> String {
    unified_diff_with_limits(path, before, after, DEFAULT_DIFF_LIMITS)
}

fn unified_diff_with_limits(
    path: &FsPath,
    before: &str,
    after: &str,
    limits: DiffLimits,
) -> String {
    if before == after {
        return String::new();
    }

    let before_count = diff_line_count(before);
    let after_count = diff_line_count(after);
    let operation_bound = before_count.checked_add(after_count);
    if before_count > limits.max_lines
        || after_count > limits.max_lines
        || operation_bound.is_none_or(|count| count > limits.max_ops)
    {
        return omitted_diff(
            path,
            before_count,
            after_count,
            "line budget exceeded",
            limits.max_output_bytes,
        );
    }

    let Some(before_lines) = split_diff_lines(before, before_count) else {
        return omitted_diff(
            path,
            before_count,
            after_count,
            "allocation unavailable",
            limits.max_output_bytes,
        );
    };
    let Some(after_lines) = split_diff_lines(after, after_count) else {
        return omitted_diff(
            path,
            before_count,
            after_count,
            "allocation unavailable",
            limits.max_output_bytes,
        );
    };
    let Some(ops) = diff_line_ops(&before_lines, &after_lines, limits.max_ops) else {
        return omitted_diff(
            path,
            before_count,
            after_count,
            "operation budget exceeded",
            limits.max_output_bytes,
        );
    };
    let Some(hunks) = group_diff_hunks(&ops, DIFF_CONTEXT_LINES) else {
        return omitted_diff(
            path,
            before_count,
            after_count,
            "allocation unavailable",
            limits.max_output_bytes,
        );
    };
    if hunks.is_empty() {
        return String::new();
    }

    let display_path = bounded_display_path(path);
    let Some(output_size) =
        estimate_output_size(&display_path, &hunks, &ops, &before_lines, &after_lines)
    else {
        return omitted_diff(
            path,
            before_count,
            after_count,
            "output size overflow",
            limits.max_output_bytes,
        );
    };
    if output_size > limits.max_output_bytes {
        return omitted_diff(
            path,
            before_count,
            after_count,
            "output budget exceeded",
            limits.max_output_bytes,
        );
    }

    let mut diff = String::new();
    if diff.try_reserve_exact(output_size).is_err() {
        return omitted_diff(
            path,
            before_count,
            after_count,
            "allocation unavailable",
            limits.max_output_bytes,
        );
    }
    diff.push_str("--- ");
    diff.push_str(&display_path);
    diff.push('\n');
    diff.push_str("+++ ");
    diff.push_str(&display_path);
    diff.push('\n');

    for hunk in hunks {
        render_diff_hunk(&mut diff, &ops[hunk.clone()], &before_lines, &after_lines);
    }

    diff
}

fn diff_line_ops(before: &[&str], after: &[&str], max_ops: usize) -> Option<Vec<DiffOp>> {
    let prefix_len = before
        .iter()
        .zip(after)
        .take_while(|(before, after)| before == after)
        .count();
    let max_suffix_len = before
        .len()
        .saturating_sub(prefix_len)
        .min(after.len().saturating_sub(prefix_len));
    let suffix_len = before[prefix_len..]
        .iter()
        .rev()
        .zip(after[prefix_len..].iter().rev())
        .take(max_suffix_len)
        .take_while(|(before, after)| before == after)
        .count();
    let before_middle_end = before.len().saturating_sub(suffix_len);
    let after_middle_end = after.len().saturating_sub(suffix_len);
    let before_middle = &before[prefix_len..before_middle_end];
    let after_middle = &after[prefix_len..after_middle_end];

    let operation_bound = before.len().checked_add(after.len())?;
    if operation_bound > max_ops {
        return None;
    }
    let mut ops = Vec::new();
    ops.try_reserve_exact(operation_bound).ok()?;
    ops.extend((0..prefix_len).map(|index| DiffOp::Equal(index, index)));
    if !push_exact_middle_ops(
        &mut ops,
        before_middle,
        after_middle,
        prefix_len,
        prefix_len,
    ) {
        ops.extend((prefix_len..before_middle_end).map(DiffOp::Delete));
        ops.extend((prefix_len..after_middle_end).map(DiffOp::Insert));
    }
    ops.extend((0..suffix_len).map(|offset| {
        DiffOp::Equal(
            before_middle_end.saturating_add(offset),
            after_middle_end.saturating_add(offset),
        )
    }));
    Some(ops)
}

fn push_exact_middle_ops(
    ops: &mut Vec<DiffOp>,
    before: &[&str],
    after: &[&str],
    before_offset: usize,
    after_offset: usize,
) -> bool {
    let Some(rows) = before.len().checked_add(1) else {
        return false;
    };
    let Some(columns) = after.len().checked_add(1) else {
        return false;
    };
    let Some(cell_count) = rows.checked_mul(columns) else {
        return false;
    };
    if cell_count > MAX_LCS_CELLS {
        return false;
    }

    let mut lcs = Vec::new();
    if lcs.try_reserve_exact(cell_count).is_err() {
        return false;
    }
    lcs.resize(cell_count, 0u32);
    for i in (0..before.len()).rev() {
        for j in (0..after.len()).rev() {
            let index = i * columns + j;
            lcs[index] = if before[i] == after[j] {
                lcs[(i + 1) * columns + j + 1].saturating_add(1)
            } else {
                lcs[(i + 1) * columns + j].max(lcs[i * columns + j + 1])
            };
        }
    }

    let n = before.len();
    let m = after.len();
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if before[i] == after[j] {
            ops.push(DiffOp::Equal(
                before_offset.saturating_add(i),
                after_offset.saturating_add(j),
            ));
            i += 1;
            j += 1;
        } else if lcs[(i + 1) * columns + j] >= lcs[i * columns + j + 1] {
            ops.push(DiffOp::Delete(before_offset.saturating_add(i)));
            i += 1;
        } else {
            ops.push(DiffOp::Insert(after_offset.saturating_add(j)));
            j += 1;
        }
    }
    while i < n {
        ops.push(DiffOp::Delete(before_offset.saturating_add(i)));
        i += 1;
    }
    while j < m {
        ops.push(DiffOp::Insert(after_offset.saturating_add(j)));
        j += 1;
    }
    true
}

fn group_diff_hunks(ops: &[DiffOp], context: usize) -> Option<Vec<Range<usize>>> {
    let mut hunks: Vec<Range<usize>> = Vec::new();
    for change in ops
        .iter()
        .enumerate()
        .filter(|(_, op)| !matches!(op, DiffOp::Equal(_, _)))
        .map(|(index, _)| index)
    {
        let start = change.saturating_sub(context);
        let end = change
            .saturating_add(context)
            .saturating_add(1)
            .min(ops.len());
        match hunks.last_mut() {
            Some(last) if start <= last.end => last.end = last.end.max(end),
            _ => {
                hunks.try_reserve(1).ok()?;
                hunks.push(start..end);
            }
        }
    }
    Some(hunks)
}

fn render_diff_hunk(diff: &mut String, ops: &[DiffOp], before: &[&str], after: &[&str]) {
    let mut before_start = None;
    let mut after_start = None;
    let mut before_len: usize = 0;
    let mut after_len: usize = 0;
    for op in ops {
        match *op {
            DiffOp::Equal(bi, ai) => {
                before_start.get_or_insert(bi);
                after_start.get_or_insert(ai);
                before_len = before_len.saturating_add(1);
                after_len = after_len.saturating_add(1);
            }
            DiffOp::Delete(bi) => {
                before_start.get_or_insert(bi);
                before_len = before_len.saturating_add(1);
            }
            DiffOp::Insert(ai) => {
                after_start.get_or_insert(ai);
                after_len = after_len.saturating_add(1);
            }
        }
    }

    let before_header = hunk_header_start(before_start, before_len);
    let after_header = hunk_header_start(after_start, after_len);
    writeln!(
        diff,
        "@@ -{before_header},{before_len} +{after_header},{after_len} @@"
    )
    .expect("writing to a String is infallible after capacity validation");

    for op in ops {
        let (marker, line) = match *op {
            DiffOp::Equal(bi, _) => (' ', before[bi]),
            DiffOp::Delete(bi) => ('-', before[bi]),
            DiffOp::Insert(ai) => ('+', after[ai]),
        };
        diff.push(marker);
        push_escaped_diff_line(diff, line);
        if !line.ends_with('\n') {
            diff.push('\n');
            diff.push_str(MISSING_NEWLINE_MARKER);
        }
    }
}

fn push_escaped_diff_line(output: &mut String, line: &str) {
    for character in line.chars() {
        match character {
            '\n' => output.push('\n'),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            '\0'..='\u{8}' | '\u{b}'..='\u{c}' | '\u{e}'..='\u{1f}' | '\u{7f}'..='\u{9f}' => {
                write!(output, "\\x{:02x}", character as u32)
                    .expect("writing to a String cannot fail");
            }
            character if unicode_display_control_escape_len(character).is_some() => {
                push_unicode_display_control_escape(output, character);
            }
            _ => output.push(character),
        }
    }
}

fn escaped_diff_line_len(line: &str) -> Option<usize> {
    line.chars().try_fold(0usize, |length, character| {
        let character_len = match character {
            '\n' => 1,
            '\r' | '\t' => 2,
            '\0'..='\u{8}' | '\u{b}'..='\u{c}' | '\u{e}'..='\u{1f}' | '\u{7f}'..='\u{9f}' => 4,
            character => unicode_display_control_escape_len(character)
                .unwrap_or_else(|| character.len_utf8()),
        };
        length.checked_add(character_len)
    })
}

fn unicode_display_control_escape_len(character: char) -> Option<usize> {
    matches!(
        character,
        '\u{61c}' | '\u{200e}'..='\u{200f}' | '\u{2028}'..='\u{202e}' | '\u{2066}'..='\u{2069}'
    )
    .then(|| 5 + (character as u32).ilog(16) as usize)
}

fn push_unicode_display_control_escape(output: &mut String, character: char) {
    debug_assert!(unicode_display_control_escape_len(character).is_some());
    write!(output, "\\u{{{:x}}}", character as u32).expect("writing to a String cannot fail");
}

const fn hunk_header_start(start: Option<usize>, len: usize) -> usize {
    match start {
        Some(index) => index.saturating_add(1),
        None if len == 0 => 0,
        None => 1,
    }
}

fn diff_line_count(text: &str) -> usize {
    if text.is_empty() {
        return 0;
    }
    text.as_bytes()
        .iter()
        .filter(|&&byte| byte == b'\n')
        .count()
        .saturating_add(usize::from(!text.ends_with('\n')))
}

fn split_diff_lines(text: &str, line_count: usize) -> Option<Vec<&str>> {
    let mut lines = Vec::new();
    lines.try_reserve_exact(line_count).ok()?;
    lines.extend(text.split_inclusive('\n'));
    Some(lines)
}

#[cfg(unix)]
fn bounded_display_path(path: &FsPath) -> String {
    use std::os::unix::ffi::OsStrExt as _;

    let bytes = path.as_os_str().as_bytes();
    let decode_limit = MAX_DISPLAY_PATH_BYTES.min(bytes.len());
    bound_decoded_path(
        String::from_utf8_lossy(&bytes[..decode_limit]).into_owned(),
        bytes.len() > decode_limit,
    )
}

#[cfg(windows)]
fn bounded_display_path(path: &FsPath) -> String {
    use std::os::windows::ffi::OsStrExt as _;

    let mut units = path.as_os_str().encode_wide();
    let prefix = units
        .by_ref()
        .take(MAX_DISPLAY_PATH_BYTES)
        .collect::<Vec<_>>();
    bound_decoded_path(String::from_utf16_lossy(&prefix), units.next().is_some())
}

fn bound_decoded_path(mut displayed: String, was_truncated: bool) -> String {
    displayed = escape_display_path_controls(&displayed);
    if displayed.len() <= MAX_DISPLAY_PATH_BYTES && !was_truncated {
        return displayed;
    }

    let mut end = MAX_DISPLAY_PATH_BYTES
        .saturating_sub(3)
        .min(displayed.len());
    while !displayed.is_char_boundary(end) {
        end = end.saturating_sub(1);
    }
    displayed.truncate(end);
    displayed.push_str("...");
    displayed
}

fn escape_display_path_controls(path: &str) -> String {
    let mut escaped = String::with_capacity(path.len());
    for character in path.chars() {
        match character {
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            '\0'..='\u{8}' | '\u{b}'..='\u{c}' | '\u{e}'..='\u{1f}' | '\u{7f}'..='\u{9f}' => {
                write!(escaped, "\\x{:02x}", character as u32)
                    .expect("writing to a String cannot fail");
            }
            character if unicode_display_control_escape_len(character).is_some() => {
                push_unicode_display_control_escape(&mut escaped, character);
            }
            _ => escaped.push(character),
        }
    }
    escaped
}

fn estimate_output_size(
    display_path: &str,
    hunks: &[Range<usize>],
    ops: &[DiffOp],
    before: &[&str],
    after: &[&str],
) -> Option<usize> {
    let mut size = display_path.len().checked_mul(2)?.checked_add(10)?;
    for hunk in hunks {
        // Four usize values and fixed punctuation cannot exceed this bound.
        let usize_digits = usize::MAX.ilog10() as usize + 1;
        size = size.checked_add(4 * usize_digits + 16)?;
        for op in &ops[hunk.clone()] {
            let line = match *op {
                DiffOp::Equal(before_index, _) | DiffOp::Delete(before_index) => {
                    before[before_index]
                }
                DiffOp::Insert(after_index) => after[after_index],
            };
            size = size
                .checked_add(1)?
                .checked_add(escaped_diff_line_len(line)?)?;
            if !line.ends_with('\n') {
                size = size
                    .checked_add(1)?
                    .checked_add(MISSING_NEWLINE_MARKER.len())?;
            }
        }
    }
    Some(size)
}

fn omitted_diff(
    path: &FsPath,
    before_lines: usize,
    after_lines: usize,
    reason: &str,
    max_output_bytes: usize,
) -> String {
    let display_path = bounded_display_path(path);
    let mut diff = String::new();
    let Some(output_size) = display_path
        .len()
        .checked_mul(2)
        .and_then(|size| size.checked_add(10))
        .and_then(|size| size.checked_add("@@ diff omitted: ".len()))
        .and_then(|size| size.checked_add(reason.len()))
        .and_then(|size| size.checked_add("; ".len()))
        .and_then(|size| size.checked_add(decimal_digits(before_lines)))
        .and_then(|size| size.checked_add(" old lines, ".len()))
        .and_then(|size| size.checked_add(decimal_digits(after_lines)))
        .and_then(|size| size.checked_add(" new lines @@\n".len()))
    else {
        return minimal_omission(max_output_bytes);
    };
    if output_size > max_output_bytes {
        return minimal_omission(max_output_bytes);
    }
    if diff.try_reserve_exact(output_size).is_err() {
        return minimal_omission(max_output_bytes);
    }
    writeln!(diff, "--- {display_path}").expect("the omission output capacity was reserved");
    writeln!(diff, "+++ {display_path}").expect("the omission output capacity was reserved");
    writeln!(
        diff,
        "@@ diff omitted: {reason}; {before_lines} old lines, {after_lines} new lines @@"
    )
    .expect("the omission output capacity was reserved");
    diff
}

fn decimal_digits(value: usize) -> usize {
    value.checked_ilog10().unwrap_or(0) as usize + 1
}

fn minimal_omission(max_output_bytes: usize) -> String {
    const MESSAGE: &str = "[diff omitted]\n";
    let length = MESSAGE.len().min(max_output_bytes);
    let mut result = String::new();
    if result.try_reserve_exact(length).is_ok() {
        result.push_str(&MESSAGE[..length]);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::{
        DiffLimits, MAX_DISPLAY_PATH_BYTES, bounded_display_path, escaped_diff_line_len,
        push_escaped_diff_line, unified_diff, unified_diff_with_limits,
    };
    use std::path::Path;

    #[test]
    fn unified_diff_only_shows_changed_lines_with_context() {
        let before = "a\nb\nc\nd\ne\nf\ng\n";
        let after = "a\nb\nc\nD\ne\nf\ng\n";
        let diff = unified_diff(Path::new("x.lisp"), before, after);

        assert!(diff.contains("--- x.lisp\n"));
        assert!(diff.contains("+++ x.lisp\n"));
        assert!(diff.contains("@@ -1,7 +1,7 @@\n"));
        assert!(diff.contains("-d\n"));
        assert!(diff.contains("+D\n"));
        assert!(!diff.contains("-a\n"));
        assert!(!diff.contains("+a\n"));
        assert_eq!(
            diff.lines()
                .filter(|l| l.starts_with('+') && !l.starts_with("+++"))
                .count(),
            1
        );
        assert_eq!(
            diff.lines()
                .filter(|l| l.starts_with('-') && !l.starts_with("---"))
                .count(),
            1
        );
    }

    #[test]
    fn unified_diff_emits_separate_hunks_for_distant_changes() {
        let before = "1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n11\n12\n";
        let after = "1x\n2\n3\n4\n5\n6\n7\n8\n9\n10\n11\n12x\n";
        let diff = unified_diff(Path::new("f"), before, after);
        assert_eq!(diff.matches("@@ -").count(), 2);
    }

    #[test]
    fn unified_diff_empty_when_unchanged() {
        let text = "(defun a () 1)\n";
        assert!(unified_diff(Path::new("f"), text, text).is_empty());
    }

    #[test]
    fn unified_diff_handles_missing_trailing_newline() {
        let before = "a\nb";
        let after = "a\nc";
        let diff = unified_diff(Path::new("f"), before, after);
        assert!(diff.contains("-b\n"));
        assert!(diff.contains("+c\n"));
        assert_eq!(diff.matches("\\ No newline at end of file\n").count(), 2);
    }

    #[test]
    fn unified_diff_exposes_a_trailing_newline_only_change() {
        let diff = unified_diff(Path::new("f"), "a", "a\n");

        assert!(diff.contains("-a\n\\ No newline at end of file\n"));
        assert!(diff.contains("+a\n"));
        assert_eq!(diff.matches("\\ No newline at end of file\n").count(), 1);
    }

    #[test]
    fn unified_diff_escapes_terminal_controls_in_content() {
        let after = "\x1b]52;c;payload\x07\n\t\u{0085}\u{009b}\n";
        let diff = unified_diff(Path::new("f"), "safe\n", after);

        assert!(diff.contains("+\\x1b]52;c;payload\\x07\n"));
        assert!(diff.contains("+\\t\\x85\\x9b\n"));
        assert!(!diff.contains('\x1b'));
        assert!(!diff.contains('\x07'));
        assert!(!diff.contains('\u{0085}'));
        assert!(!diff.contains('\u{009b}'));
    }

    #[test]
    fn unified_diff_escapes_unicode_display_controls_in_content() {
        let controls = [
            '\u{61c}', '\u{200e}', '\u{200f}', '\u{2028}', '\u{2029}', '\u{202a}', '\u{202b}',
            '\u{202c}', '\u{202d}', '\u{202e}', '\u{2066}', '\u{2067}', '\u{2068}', '\u{2069}',
        ];
        let raw = controls.iter().copied().collect::<String>();
        let after = format!("prefix{raw}suffix\n");
        let diff = unified_diff(Path::new("f"), "safe\n", &after);

        for control in controls {
            assert!(!diff.contains(control));
            assert!(diff.contains(&format!("\\u{{{:x}}}", control as u32)));
        }
    }

    #[test]
    fn escaped_diff_line_length_matches_unicode_display_control_output() {
        let line = "\t\u{61c}\u{200e}\u{2028}\u{202e}\u{2069}\x1b\n";
        let mut escaped = String::new();

        push_escaped_diff_line(&mut escaped, line);

        assert_eq!(escaped_diff_line_len(line), Some(escaped.len()));
        assert_eq!(
            escaped,
            "\\t\\u{61c}\\u{200e}\\u{2028}\\u{202e}\\u{2069}\\x1b\n"
        );
    }

    #[test]
    fn unified_diff_output_budget_counts_escaped_content() {
        let limits = DiffLimits {
            max_lines: 10,
            max_ops: 20,
            max_output_bytes: 48,
        };
        let before = "\x01\x01\x01\x01\x01\x01\n";
        let after = "\x02\x02\x02\x02\x02\x02\n";
        let diff = unified_diff_with_limits(Path::new("f"), before, after, limits);

        assert_eq!(diff, "[diff omitted]\n");
        assert!(diff.len() <= limits.max_output_bytes);
    }

    #[test]
    fn unified_diff_output_budget_counts_escaped_unicode_display_controls() {
        let limits = DiffLimits {
            max_lines: 10,
            max_ops: 20,
            max_output_bytes: 160,
        };
        let before = format!("{}\n", "\u{202e}".repeat(4));
        let after = format!("{}\n", "\u{2066}".repeat(4));
        let diff = unified_diff_with_limits(Path::new("f"), &before, &after, limits);

        assert_eq!(
            diff,
            "--- f\n+++ f\n@@ diff omitted: output budget exceeded; 1 old lines, 1 new lines @@\n"
        );
        assert!(diff.len() <= limits.max_output_bytes);
    }

    #[test]
    fn unified_diff_omits_content_when_the_line_budget_is_exceeded() {
        let limits = DiffLimits {
            max_lines: 3,
            max_ops: 6,
            max_output_bytes: 1024,
        };
        let diff = unified_diff_with_limits(Path::new("f"), "a\nb\nc\nd\n", "x\ny\nz\nw\n", limits);

        assert_eq!(
            diff,
            "--- f\n+++ f\n@@ diff omitted: line budget exceeded; 4 old lines, 4 new lines @@\n"
        );
        assert!(!diff.contains("-a\n"));
    }

    #[test]
    fn unified_diff_omits_content_when_the_output_budget_is_exceeded() {
        let limits = DiffLimits {
            max_lines: 10,
            max_ops: 20,
            max_output_bytes: 32,
        };
        let diff = unified_diff_with_limits(Path::new("f"), "old\n", "new\n", limits);

        assert_eq!(diff, "[diff omitted]\n");
        assert!(diff.len() <= limits.max_output_bytes);
        assert!(!diff.contains("-old\n"));
    }

    #[test]
    fn unified_diff_never_exceeds_a_tiny_output_budget() {
        for max_output_bytes in 0..16 {
            let limits = DiffLimits {
                max_lines: 0,
                max_ops: 0,
                max_output_bytes,
            };
            let diff = unified_diff_with_limits(Path::new("f"), "old\n", "new\n", limits);

            assert!(diff.len() <= max_output_bytes);
            assert!("[diff omitted]\n".starts_with(&diff));
        }
    }

    #[cfg(unix)]
    #[test]
    fn bounded_display_path_limits_lossy_expansion() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt as _;

        let bytes = vec![0xff; MAX_DISPLAY_PATH_BYTES * 2];
        let displayed = bounded_display_path(Path::new(OsStr::from_bytes(&bytes)));

        assert!(displayed.len() <= MAX_DISPLAY_PATH_BYTES);
        assert!(displayed.ends_with("..."));
    }

    #[test]
    fn bounded_display_path_escapes_control_characters() {
        let displayed = bounded_display_path(Path::new("a\nb\rc\td\x1be\x01f\u{7f}\u{9b}"));

        assert_eq!(displayed, "a\\nb\\rc\\td\\x1be\\x01f\\x7f\\x9b");
        assert!(!displayed.contains('\u{9b}'));
        assert!(!displayed.chars().any(char::is_control));
    }

    #[test]
    fn bounded_display_path_escapes_unicode_display_controls() {
        let controls = [
            '\u{61c}', '\u{200e}', '\u{200f}', '\u{2028}', '\u{2029}', '\u{202a}', '\u{202b}',
            '\u{202c}', '\u{202d}', '\u{202e}', '\u{2066}', '\u{2067}', '\u{2068}', '\u{2069}',
        ];
        let raw = controls.iter().copied().collect::<String>();
        let displayed = bounded_display_path(Path::new(&format!("a{raw}z")));

        for control in controls {
            assert!(!displayed.contains(control));
            assert!(displayed.contains(&format!("\\u{{{:x}}}", control as u32)));
        }
    }

    #[test]
    fn bounded_display_path_caps_expanded_control_characters() {
        let path = "\x01".repeat(MAX_DISPLAY_PATH_BYTES);
        let displayed = bounded_display_path(Path::new(&path));

        assert!(displayed.len() <= MAX_DISPLAY_PATH_BYTES);
        assert!(displayed.ends_with("..."));
        assert!(!displayed.chars().any(char::is_control));
    }

    #[test]
    fn unified_diff_falls_back_without_allocating_an_oversized_lcs_matrix() {
        let line_count = 20_000;
        let before = "old\n".repeat(line_count);
        let after = "new\n".repeat(line_count);
        let diff = unified_diff(Path::new("large.lisp"), &before, &after);

        assert!(diff.contains("@@ -1,20000 +1,20000 @@\n"));
        assert_eq!(diff.matches("-old\n").count(), line_count);
        assert_eq!(diff.matches("+new\n").count(), line_count);
        assert!(diff.find("-old\n").unwrap() < diff.find("+new\n").unwrap());
    }
}
