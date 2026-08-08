#![doc = include_str!("../README.md")]

pub mod editor;

pub use editor::domain::{Cursor, TextBuffer};
pub use editor::infrastructure::FileDocumentStore;
pub use editor::usecase::{ActionResult, DocumentStore, EditorAction, EditorError, EditorSession};
