//! Pure screen rendering for the editor adapter.

use std::fmt::Write as _;

use paredit_feature_editor::EditorSession;

const GUTTER_WIDTH: usize = 7;

pub(super) fn frame(session: &EditorSession, width: usize, height: usize) -> String {
    let width = width.max(1);
    let height = height.max(3);
    let code_height = height - 1;
    let cursor = session.cursor();
    let first_line = cursor.line().saturating_sub(code_height.saturating_sub(1));
    let mut output = String::new();

    for row in 0..code_height {
        let line_index = first_line + row;
        let line = session.buffer().line(line_index).unwrap_or("");
        let source_budget = width.saturating_sub(GUTTER_WIDTH);
        let source = clip(line, source_budget);
        writeln!(output, "{:>4} │ {source}", line_index + 1).expect("String writes cannot fail");
    }

    let modified = if session.is_dirty() { " [+]" } else { "" };
    let status = format!(
        " {}{} | line {}, col {} | C-s save  C-z undo  C-y redo  C-q quit",
        session.path().display(),
        modified,
        cursor.line() + 1,
        cursor.column() + 1,
    );
    writeln!(output, "{}", clip(&status, width)).expect("String writes cannot fail");

    let screen_row = cursor.line().saturating_sub(first_line) + 1;
    let screen_column = (GUTTER_WIDTH + cursor.column() + 1).min(width);
    write!(output, "\x1b[{screen_row};{screen_column}H").expect("String writes cannot fail");
    output
}

fn clip(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_owned();
    }
    text.chars().take(width).collect()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn frame_renders_line_numbers_dirty_state_and_cursor() {
        let mut session = EditorSession::new(PathBuf::from("fixture.lisp"), "(a)\n(b)".to_owned());
        let store = paredit_feature_editor::FileDocumentStore;
        session
            .apply(paredit_feature_editor::EditorAction::End, &store)
            .expect("move succeeds");
        session
            .apply(paredit_feature_editor::EditorAction::Insert('!'), &store)
            .expect("insert succeeds");

        let rendered = frame(&session, 80, 8);

        assert!(rendered.contains("   1 │ (a)!"));
        assert!(rendered.contains("   2 │ (b)"));
        assert!(rendered.contains("fixture.lisp [+]"));
        assert!(rendered.contains("\x1b[1;12H"));
    }

    #[test]
    fn frame_clips_source_to_terminal_width() {
        let session = EditorSession::new(PathBuf::from("fixture"), "abcdefghijk".to_owned());

        let rendered = frame(&session, 10, 4);
        let source_line = rendered.lines().next().expect("source line exists");

        assert!(source_line.chars().count() <= 10);
    }
}
