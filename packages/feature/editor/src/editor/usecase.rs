//! Application services for an editor session.

use std::fmt::{Display, Formatter};
use std::io;
use std::path::{Path, PathBuf};

use super::domain::{Cursor, TextBuffer};

/// Persistence boundary used by the editor application service.
pub trait DocumentStore {
    /// Reads a document from its path.
    fn load(&self, path: &Path) -> io::Result<String>;

    /// Writes a document to its path.
    fn save(&self, path: &Path, text: &str) -> io::Result<()>;
}

/// The actions a delivery adapter can send to an editor session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorAction {
    Insert(char),
    Newline,
    Backspace,
    Delete,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    Save,
    Undo,
    Redo,
}

/// The result of applying one action.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ActionResult {
    /// Whether the buffer or cursor history changed.
    pub changed: bool,
    /// Whether the action completed a save.
    pub saved: bool,
}

/// Errors crossing the storage boundary.
#[derive(Debug)]
pub enum EditorError {
    Load { path: PathBuf, source: io::Error },
    Save { path: PathBuf, source: io::Error },
}

impl Display for EditorError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Load { path, source } => {
                write!(formatter, "could not read {}: {source}", path.display())
            }
            Self::Save { path, source } => {
                write!(formatter, "could not write {}: {source}", path.display())
            }
        }
    }
}

impl std::error::Error for EditorError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Load { source, .. } | Self::Save { source, .. } => Some(source),
        }
    }
}

#[derive(Debug, Clone)]
struct Snapshot {
    buffer: TextBuffer,
    cursor: Cursor,
}

/// The aggregate representing one open document and its editing history.
#[derive(Debug)]
pub struct EditorSession {
    path: PathBuf,
    buffer: TextBuffer,
    cursor: Cursor,
    saved_text: String,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
}

impl EditorSession {
    /// Opens a document through the supplied persistence port.
    pub fn open<S: DocumentStore>(path: &Path, store: &S) -> Result<Self, EditorError> {
        let text = store.load(path).map_err(|source| EditorError::Load {
            path: path.to_owned(),
            source,
        })?;
        Ok(Self::new(path.to_owned(), text))
    }

    /// Creates a session from already-loaded text.
    #[must_use]
    pub fn new(path: PathBuf, text: String) -> Self {
        let buffer = TextBuffer::from_text(&text);
        Self {
            path,
            buffer,
            cursor: Cursor::default(),
            saved_text: text,
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }

    /// Applies an action and returns its observable outcome.
    pub fn apply<S: DocumentStore>(
        &mut self,
        action: EditorAction,
        store: &S,
    ) -> Result<ActionResult, EditorError> {
        match action {
            EditorAction::Insert(character) => {
                Ok(self.edit(|buffer, cursor| buffer.insert_char(cursor, character)))
            }
            EditorAction::Newline => Ok(self.edit(TextBuffer::newline)),
            EditorAction::Backspace => Ok(self.edit(TextBuffer::backspace)),
            EditorAction::Delete => Ok(self.edit(TextBuffer::delete)),
            EditorAction::Left => {
                self.buffer.move_left(&mut self.cursor);
                Ok(ActionResult::default())
            }
            EditorAction::Right => {
                self.buffer.move_right(&mut self.cursor);
                Ok(ActionResult::default())
            }
            EditorAction::Up => {
                self.buffer.move_up(&mut self.cursor);
                Ok(ActionResult::default())
            }
            EditorAction::Down => {
                self.buffer.move_down(&mut self.cursor);
                Ok(ActionResult::default())
            }
            EditorAction::Home => {
                self.buffer.move_home(&mut self.cursor);
                Ok(ActionResult::default())
            }
            EditorAction::End => {
                self.buffer.move_end(&mut self.cursor);
                Ok(ActionResult::default())
            }
            EditorAction::Save => {
                self.save(store)?;
                Ok(ActionResult {
                    saved: true,
                    ..ActionResult::default()
                })
            }
            EditorAction::Undo => Ok(self.restore_undo()),
            EditorAction::Redo => Ok(self.restore_redo()),
        }
    }

    /// Saves the current text and clears the dirty state.
    pub fn save<S: DocumentStore>(&mut self, store: &S) -> Result<(), EditorError> {
        let text = self.buffer.to_text();
        store
            .save(&self.path, &text)
            .map_err(|source| EditorError::Save {
                path: self.path.clone(),
                source,
            })?;
        self.saved_text = text;
        Ok(())
    }

    /// Returns the open document's path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns the current buffer.
    #[must_use]
    pub const fn buffer(&self) -> &TextBuffer {
        &self.buffer
    }

    /// Returns the current cursor.
    #[must_use]
    pub const fn cursor(&self) -> Cursor {
        self.cursor
    }

    /// Returns whether the current text differs from the last successful save.
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.buffer.to_text() != self.saved_text
    }

    fn edit<F>(&mut self, operation: F) -> ActionResult
    where
        F: FnOnce(&mut TextBuffer, &mut Cursor) -> bool,
    {
        let snapshot = Snapshot {
            buffer: self.buffer.clone(),
            cursor: self.cursor,
        };
        if !operation(&mut self.buffer, &mut self.cursor) {
            return ActionResult::default();
        }
        self.undo.push(snapshot);
        self.redo.clear();
        ActionResult {
            changed: true,
            ..ActionResult::default()
        }
    }

    fn restore_undo(&mut self) -> ActionResult {
        let Some(snapshot) = self.undo.pop() else {
            return ActionResult::default();
        };
        self.redo.push(Snapshot {
            buffer: self.buffer.clone(),
            cursor: self.cursor,
        });
        self.buffer = snapshot.buffer;
        self.cursor = snapshot.cursor;
        ActionResult {
            changed: true,
            ..ActionResult::default()
        }
    }

    fn restore_redo(&mut self) -> ActionResult {
        let Some(snapshot) = self.redo.pop() else {
            return ActionResult::default();
        };
        self.undo.push(Snapshot {
            buffer: self.buffer.clone(),
            cursor: self.cursor,
        });
        self.buffer = snapshot.buffer;
        self.cursor = snapshot.cursor;
        ActionResult {
            changed: true,
            ..ActionResult::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::HashMap;

    use super::*;

    #[derive(Default)]
    struct MemoryStore {
        files: RefCell<HashMap<PathBuf, String>>,
    }

    impl DocumentStore for MemoryStore {
        fn load(&self, path: &Path) -> io::Result<String> {
            Ok(self.files.borrow().get(path).cloned().unwrap_or_default())
        }

        fn save(&self, path: &Path, text: &str) -> io::Result<()> {
            self.files
                .borrow_mut()
                .insert(path.to_owned(), text.to_owned());
            Ok(())
        }
    }

    #[test]
    fn edit_undo_redo_and_save_follow_the_document_lifecycle() {
        let store = MemoryStore::default();
        let path = PathBuf::from("fixture.lisp");
        store
            .files
            .borrow_mut()
            .insert(path.clone(), "(a)".to_owned());
        let mut session = EditorSession::open(&path, &store).expect("document opens");

        session
            .apply(EditorAction::End, &store)
            .expect("move succeeds");
        session
            .apply(EditorAction::Insert('!'), &store)
            .expect("insert succeeds");
        assert!(session.is_dirty());
        session
            .apply(EditorAction::Undo, &store)
            .expect("undo succeeds");
        assert_eq!(session.buffer().to_text(), "(a)");
        session
            .apply(EditorAction::Redo, &store)
            .expect("redo succeeds");
        assert_eq!(session.buffer().to_text(), "(a)!");
        session
            .apply(EditorAction::Save, &store)
            .expect("save succeeds");
        assert!(!session.is_dirty());
        assert_eq!(store.files.borrow().get(&path), Some(&"(a)!".to_owned()));
    }

    #[test]
    fn a_new_edit_invalidates_redo_history() {
        let store = MemoryStore::default();
        let mut session = EditorSession::new(PathBuf::from("fixture"), "a".to_owned());

        session
            .apply(EditorAction::End, &store)
            .expect("move succeeds");
        session
            .apply(EditorAction::Insert('b'), &store)
            .expect("insert succeeds");
        session
            .apply(EditorAction::Undo, &store)
            .expect("undo succeeds");
        session
            .apply(EditorAction::Insert('c'), &store)
            .expect("insert succeeds");
        let result = session
            .apply(EditorAction::Redo, &store)
            .expect("redo is a no-op");

        assert!(!result.changed);
        assert_eq!(session.buffer().to_text(), "ac");
    }
}
