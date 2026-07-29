//! Not re-analyzing a file whose bytes have not changed.
//!
//! A lint pass runs 170 rules over every file, every time. On a large tree
//! that is most of the wall clock, and on a *re-run* almost all of it is spent
//! rediscovering that nothing changed: the ninth commit of the day touches
//! four files and pays for four thousand.
//!
//! The cache is content-addressed, which is what makes it safe to trust. The
//! key is a hash of everything that can change the answer — the tool version,
//! the analysis, the dialect, the caller's rule selection and settings, and
//! the file's own bytes — so a hit means "this exact question was asked and
//! answered", not "a file with this name was seen before". A file that is
//! renamed, or reverted to an earlier state, or copied from elsewhere, hits
//! correctly. A file whose content changed by one byte cannot hit at all.
//!
//! Three consequences follow from that and are worth stating, because each is
//! a class of cache bug this design does not have:
//!
//! - **No invalidation logic.** There is nothing to invalidate. A stale entry
//!   is unreachable rather than wrong, because its key describes a question
//!   nobody is asking any more.
//! - **No mtime, no size, no inode.** All three are proxies for content and
//!   all three lie — a checkout, a `touch`, a filesystem without sub-second
//!   timestamps.
//! - **Upgrading the tool empties it.** The version is in the key, so a build
//!   with a new rule cannot serve an answer computed without it.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::Value;

/// Distinguishes concurrent writes of the same key within one process.
static STAGED_WRITES: AtomicU64 = AtomicU64::new(0);

/// The on-disk layout version.
///
/// Part of every key, so a build that changes what an entry means cannot read
/// entries written by a build that meant something else.
const CACHE_FORMAT_VERSION: &str = "1";

/// A content-addressed store of per-file analysis results.
#[derive(Debug, Clone)]
pub struct AnalysisCache {
    root: PathBuf,
    tool_version: String,
}

/// What a cache run did, for a report that wants to say so.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CacheStatistics {
    pub hits: usize,
    pub misses: usize,
    /// Entries that could not be written. Never fatal: a cache that cannot be
    /// written is a slow run, not a wrong one.
    pub write_failures: usize,
}

impl CacheStatistics {
    #[must_use]
    pub const fn total(&self) -> usize {
        self.hits + self.misses
    }

    /// Hits as a percentage, or `None` when nothing was looked up.
    #[must_use]
    pub fn hit_rate(&self) -> Option<f64> {
        (self.total() > 0).then(|| {
            #[expect(
                clippy::cast_precision_loss,
                reason = "a percentage of a file count; precision beyond 2^53 files is meaningless"
            )]
            {
                self.hits as f64 * 100.0 / self.total() as f64
            }
        })
    }
}

impl AnalysisCache {
    /// Opens (creating if needed) a cache under `root`.
    pub fn open(root: impl Into<PathBuf>, tool_version: impl Into<String>) -> io::Result<Self> {
        let root = root.into();
        fs::create_dir_all(&root)?;
        Ok(Self {
            root,
            tool_version: tool_version.into(),
        })
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The key for one question.
    ///
    /// `analysis` names the report, and `discriminator` carries everything
    /// else about the request that changes the answer — the active rule set,
    /// the rule settings, the dialect. Getting that argument wrong is the one
    /// way to make this cache return a wrong result, which is why it is a
    /// required parameter rather than something with a default.
    #[must_use]
    pub fn key(&self, analysis: &str, discriminator: &str, content: &str) -> String {
        let mut hasher = blake3::Hasher::new();
        // Length-prefixed so two different field splits cannot hash the same:
        // ("ab", "c") and ("a", "bc") must be different questions.
        for field in [
            CACHE_FORMAT_VERSION,
            self.tool_version.as_str(),
            analysis,
            discriminator,
            content,
        ] {
            hasher.update(&(field.len() as u64).to_le_bytes());
            hasher.update(field.as_bytes());
        }
        hasher.finalize().to_hex().to_string()
    }

    /// The stored answer, if this exact question has been answered.
    ///
    /// Every failure — a missing file, an unreadable one, a corrupt entry —
    /// reads as a miss. A cache that returns errors makes every caller handle
    /// a case in which the correct behaviour is always "compute it again".
    #[must_use]
    pub fn get(&self, key: &str) -> Option<Value> {
        let text = fs::read_to_string(self.entry_path(key)).ok()?;
        serde_json::from_str(&text).ok()
    }

    /// Records an answer. Returns whether it was written.
    ///
    /// Written through a uniquely-named temporary in the same directory and
    /// renamed into place, so a concurrent reader sees either the old entry or
    /// the new one and never a half-written file.
    pub fn put(&self, key: &str, value: &Value) -> bool {
        let path = self.entry_path(key);
        let Some(parent) = path.parent() else {
            return false;
        };
        if fs::create_dir_all(parent).is_err() {
            return false;
        }
        let Ok(serialized) = serde_json::to_string(value) else {
            return false;
        };
        // Unique per *attempt*, not per process. Two workers analysing two
        // identical files compute the same key at the same time, and a name
        // built from the pid alone makes them race for one temporary — which
        // showed up immediately on a 1065-file corpus as one unwritable entry.
        let attempt = STAGED_WRITES.fetch_add(1, Ordering::Relaxed);
        let staged = parent.join(format!(".{key}.{}.{attempt}.tmp", std::process::id()));
        if fs::write(&staged, serialized).is_err() {
            let _ = fs::remove_file(&staged);
            return false;
        }
        if fs::rename(&staged, &path).is_err() {
            let _ = fs::remove_file(&staged);
            return false;
        }
        true
    }

    /// Where one entry lives.
    ///
    /// Sharded by the first two hex characters. A single directory with a
    /// hundred thousand entries is slow to open on several filesystems and
    /// unpleasant to look at on all of them.
    fn entry_path(&self, key: &str) -> PathBuf {
        let shard = key.get(..2).unwrap_or("00");
        self.root.join(shard).join(key)
    }
}

#[cfg(test)]
mod tests {
    use super::{AnalysisCache, CacheStatistics};
    use serde_json::json;

    fn cache(label: &str) -> AnalysisCache {
        let root = std::env::temp_dir().join(format!(
            "paredit-cache-{label}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        AnalysisCache::open(root, "1.2.3").expect("open cache")
    }

    #[test]
    fn a_stored_answer_comes_back() {
        let cache = cache("round-trip");
        let key = cache.key("lint", "rules=all", "(defun f ())");

        assert!(cache.get(&key).is_none());
        assert!(cache.put(&key, &json!({"findings": []})));
        assert_eq!(cache.get(&key), Some(json!({"findings": []})));
    }

    /// The property the whole design rests on: one changed byte is a different
    /// question, so a stale answer is unreachable rather than wrong.
    #[test]
    fn one_changed_byte_is_a_different_key() {
        let cache = cache("content");
        assert_ne!(
            cache.key("lint", "rules=all", "(defun f ())"),
            cache.key("lint", "rules=all", "(defun g ())")
        );
    }

    #[test]
    fn the_analysis_and_the_request_are_both_in_the_key() {
        let cache = cache("discriminator");
        let content = "(defun f ())";

        assert_ne!(
            cache.key("lint", "rules=all", content),
            cache.key("complexity", "rules=all", content)
        );
        assert_ne!(
            cache.key("lint", "rules=all", content),
            cache.key("lint", "rules=recommended", content)
        );
    }

    /// A build with a new rule must not serve an answer computed without it.
    #[test]
    fn a_different_tool_version_is_a_different_key() {
        let root =
            std::env::temp_dir().join(format!("paredit-cache-version-{}", std::process::id()));
        let older = AnalysisCache::open(&root, "1.2.3").expect("open cache");
        let newer = AnalysisCache::open(&root, "1.3.0").expect("open cache");

        assert_ne!(
            older.key("lint", "rules=all", "(defun f ())"),
            newer.key("lint", "rules=all", "(defun f ())")
        );
    }

    /// Field boundaries must be part of the hash, or two different requests
    /// can collide by splitting the same bytes differently.
    #[test]
    fn adjacent_fields_cannot_be_confused_for_one_another() {
        let cache = cache("boundaries");
        assert_ne!(cache.key("lint", "ab", "c"), cache.key("lint", "a", "bc"));
    }

    /// The correct response to a corrupt entry is to compute the answer again,
    /// so it reads as a miss rather than an error.
    #[test]
    fn a_corrupt_entry_reads_as_a_miss() {
        let cache = cache("corrupt");
        let key = cache.key("lint", "rules=all", "(defun f ())");
        assert!(cache.put(&key, &json!({"findings": []})));

        let path = cache.root().join(&key[..2]).join(&key);
        std::fs::write(&path, "{ not json").expect("corrupt the entry");

        assert_eq!(cache.get(&key), None);
    }

    #[test]
    fn statistics_report_a_rate_only_when_something_was_looked_up() {
        assert_eq!(CacheStatistics::default().hit_rate(), None);
        let statistics = CacheStatistics {
            hits: 3,
            misses: 1,
            write_failures: 0,
        };
        assert_eq!(statistics.total(), 4);
        assert_eq!(statistics.hit_rate(), Some(75.0));
    }
}
