//! Filesystem adapters for the editor application service.

use std::fs;
use std::io;
use std::path::Path;

use super::usecase::DocumentStore;

/// The production document store used by the terminal editor.
#[derive(Debug, Clone, Copy, Default)]
pub struct FileDocumentStore;

impl DocumentStore for FileDocumentStore {
    fn load(&self, path: &Path) -> io::Result<String> {
        match fs::read_to_string(path) {
            Ok(text) => Ok(text),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(String::new()),
            Err(error) => Err(error),
        }
    }

    fn save(&self, path: &Path, text: &str) -> io::Result<()> {
        fs::write(path, text)
    }
}
