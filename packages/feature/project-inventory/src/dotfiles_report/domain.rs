use serde_json::{Value, json};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    File,
    Directory,
    Symlink,
    Other,
}

impl EntryKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Directory => "directory",
            Self::Symlink => "symlink",
            Self::Other => "other",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Rule,
    Migration,
    KillRing,
    Other,
}

impl Category {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Rule => "rule",
            Self::Migration => "migration",
            Self::KillRing => "kill_ring",
            Self::Other => "other",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryStatus {
    Inventoried,
    NotFollowed,
    Unsupported,
    FileSizeLimit,
    DepthLimit,
}

impl EntryStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Inventoried => "inventoried",
            Self::NotFollowed => "not_followed",
            Self::Unsupported => "unsupported",
            Self::FileSizeLimit => "file_size_limit",
            Self::DepthLimit => "depth_limit",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub path: String,
    pub kind: EntryKind,
    pub category: Category,
    pub bytes: Option<u64>,
    pub status: EntryStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub path: String,
    pub code: &'static str,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub present: bool,
    pub complete: bool,
    pub entries: Vec<Entry>,
    pub diagnostics: Vec<Diagnostic>,
}

impl Report {
    #[must_use]
    pub fn to_json(&self) -> Value {
        json!({
            "schema_version": 1,
            "present": self.present,
            "complete": self.complete,
            "entries": self.entries.iter().map(|entry| json!({
                "path": entry.path,
                "kind": entry.kind.as_str(),
                "category": entry.category.as_str(),
                "bytes": entry.bytes,
                "status": entry.status.as_str(),
            })).collect::<Vec<_>>(),
            "diagnostics": self.diagnostics.iter().map(|diagnostic| json!({
                "path": diagnostic.path,
                "code": diagnostic.code,
                "message": diagnostic.message,
            })).collect::<Vec<_>>(),
        })
    }
}
