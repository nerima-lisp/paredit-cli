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
//!
//! Content addressing answers "is this the same question", not "did this
//! answer come from here". A cache directory is an ordinary directory, and one
//! restored from a shared CI artifact — or writable by anything else on the
//! machine — is attacker-controlled input. An entry is therefore authenticated
//! as well as addressed: each is stored with a keyed BLAKE3 tag over its own
//! bytes and its key, under a 32-byte secret minted per cache directory and
//! never shared. An entry whose tag does not verify reads as a miss, exactly as
//! a truncated one does, so tampering costs a recompute and can never change
//! what the tool reports.

use std::fs;
use std::io;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::Value;

/// Distinguishes concurrent writes of the same key within one process.
static STAGED_WRITES: AtomicU64 = AtomicU64::new(0);

/// The on-disk layout version.
///
/// Part of every key, so a build that changes what an entry means cannot read
/// entries written by a build that meant something else. `2` is the first
/// version whose entries carry an authentication tag; a `1` entry is not a
/// differently-shaped answer to the same question but an unauthenticated one,
/// and is unreachable rather than rejected.
const CACHE_FORMAT_VERSION: &str = "2";

/// The per-directory signing key, at the cache root.
///
/// A dotted name, so it cannot be confused with a shard: a shard is always two
/// hex characters.
const SECRET_FILE: &str = ".secret";

/// A content-addressed, authenticated store of per-file analysis results.
#[derive(Clone)]
pub struct AnalysisCache {
    root: PathBuf,
    tool_version: String,
    secret: [u8; 32],
}

/// Without this, any `{cache:?}` anywhere upstream prints the signing key.
impl std::fmt::Debug for AnalysisCache {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AnalysisCache")
            .field("root", &self.root)
            .field("tool_version", &self.tool_version)
            .finish_non_exhaustive()
    }
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
    ///
    /// Mints the directory's signing key on first use. A directory written by a
    /// build from before entries were authenticated has no key and no tags, so
    /// it gets a fresh key and every entry in it reads as a miss — one extra
    /// recompute, which is the whole cost of not attempting a migration that
    /// would have to trust the very bytes it is there to stop trusting.
    pub fn open(root: impl Into<PathBuf>, tool_version: impl Into<String>) -> io::Result<Self> {
        let root = root.into();
        fs::create_dir_all(&root)?;
        let secret = load_or_mint_secret(&root);
        Ok(Self {
            root,
            tool_version: tool_version.into(),
            secret,
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

    /// The stored answer, if this exact question has been answered *here*.
    ///
    /// Every failure — a missing file, an unreadable one, a corrupt entry, an
    /// entry this cache did not write — reads as a miss. A cache that returns
    /// errors makes every caller handle a case in which the correct behaviour
    /// is always "compute it again", and a cache that reports tampering as a
    /// failure hands anyone with write access to the directory a way to stop
    /// the run rather than merely to slow it down.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<Value> {
        let stored = fs::read_to_string(self.entry_path(key)).ok()?;
        let (tag, payload) = stored.split_once('\n')?;
        if !tags_match(&self.tag(key, payload.as_bytes()), tag) {
            return None;
        }
        serde_json::from_str(payload).ok()
    }

    /// Records an answer. Returns whether it was written.
    ///
    /// Written through a uniquely-named temporary in the same directory and
    /// renamed into place, so a concurrent reader sees either the old entry or
    /// the new one and never a half-written file.
    ///
    /// The stored file is the entry's tag, a newline, and the entry. Hashing
    /// the bytes as written rather than a re-serialization of the parsed value
    /// is what makes verification exact: there is no second encoding that could
    /// disagree with the first.
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
        let serialized = format!("{}\n{serialized}", self.tag(key, serialized.as_bytes()));
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

    /// The authenticator for one entry, as hex.
    ///
    /// Over the key as well as the payload, so a genuine entry cannot be copied
    /// onto another key's path and returned as the answer to a question it was
    /// never the answer to — the filename is not evidence of anything, and
    /// binding the key into the tag is what makes it so.
    fn tag(&self, key: &str, payload: &[u8]) -> String {
        let mut hasher = blake3::Hasher::new_keyed(&self.secret);
        // Length-prefixed for the same reason the cache key is.
        hasher.update(&(key.len() as u64).to_le_bytes());
        hasher.update(key.as_bytes());
        hasher.update(payload);
        hasher.finalize().to_hex().to_string()
    }
}

/// Whether two authenticators are equal, in time independent of where they
/// first differ.
///
/// Both are hex of a fixed-width hash, so the length comparison leaks nothing
/// a reader could not get by looking at the file.
fn tags_match(computed: &str, stored: &str) -> bool {
    computed.len() == stored.len()
        && computed
            .bytes()
            .zip(stored.bytes())
            .fold(0_u8, |difference, (left, right)| {
                difference | (left ^ right)
            })
            == 0
}

/// This cache directory's signing key, minting and storing one when it has
/// none.
///
/// A directory that cannot hold a key still gets one, held only in memory: an
/// unwritable cache is a slow run and not a wrong one, and the worst case is
/// that every entry this run writes reads as a miss to the next.
fn load_or_mint_secret(root: &Path) -> [u8; 32] {
    let path = root.join(SECRET_FILE);
    if let Ok(bytes) = fs::read(&path) {
        if let Ok(secret) = <[u8; 32]>::try_from(bytes.as_slice()) {
            return secret;
        }
        // A key of the wrong length is not a key. Left in place it would fail
        // to parse on every future run, so the cache would never hit again;
        // removing it costs at most one directory's entries.
        let _ = fs::remove_file(&path);
    }

    let minted = mint_secret();
    let mut options = fs::OpenOptions::new();
    // `create_new`, so two processes opening the same fresh directory settle on
    // one key rather than each overwriting the other's and signing entries with
    // a key that is about to be replaced.
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        // Forging an entry needs this key, so nobody but its owner may read it.
        // Set at creation rather than after, so it is never briefly world-
        // readable with the key already in it.
        options.mode(0o600);
    }
    match options.open(&path) {
        Ok(mut file) => {
            if file.write_all(&minted).is_ok() {
                return minted;
            }
        }
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            if let Ok(bytes) = fs::read(&path) {
                if let Ok(secret) = <[u8; 32]>::try_from(bytes.as_slice()) {
                    return secret;
                }
            }
        }
        Err(_) => {}
    }
    minted
}

/// 32 bytes from the operating system's random device.
///
/// Guessing the key forges entries, so the alternatives are not good enough: a
/// key derived from the clock and the process id is a search of about a billion
/// for anyone who knows roughly when the cache was created. That derivation
/// survives only as the fallback for a platform with no random device, where it
/// is still better than a constant.
fn mint_secret() -> [u8; 32] {
    // `read_exact`, never `fs::read`. `/dev/urandom` is a character device with
    // no end, so a read-to-EOF never returns — it fills memory until the
    // process dies.
    let mut bytes = [0_u8; 32];
    let read = fs::File::open("/dev/urandom")
        .and_then(|mut device| io::Read::read_exact(&mut device, &mut bytes));
    if read.is_ok() {
        return bytes;
    }

    let mut hasher = blake3::Hasher::new();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    hasher.update(&nanos.to_le_bytes());
    hasher.update(&std::process::id().to_le_bytes());
    hasher.update(&STAGED_WRITES.fetch_add(1, Ordering::Relaxed).to_le_bytes());
    *hasher.finalize().as_bytes()
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

    /// The attack this cache is authenticated against.
    ///
    /// A cache directory is an ordinary directory — restored from a CI artifact,
    /// shared between jobs, writable by anything on the machine — so its
    /// contents are attacker-controlled input. Editing a stored answer into a
    /// different one must not make the tool report the different one: the entry
    /// no longer verifies, so it reads as a miss and the answer is recomputed.
    #[test]
    fn an_edited_entry_reads_as_a_miss() {
        let cache = cache("tampered");
        let key = cache.key("lint", "rules=all", "(defun f ())");
        assert!(cache.put(&key, &json!({"findings": ["a real finding"]})));

        let path = cache.root().join(&key[..2]).join(&key);
        let stored = std::fs::read_to_string(&path).expect("read the entry");
        let (tag, _) = stored
            .split_once('\n')
            .expect("an entry is tag then payload");
        // The tag is left exactly as written; only the answer is rewritten.
        // This is the forgery a plain-JSON entry would have accepted.
        std::fs::write(&path, format!("{tag}\n{{\"findings\":[]}}")).expect("forge the entry");

        assert_eq!(cache.get(&key), None);
    }

    /// Rewriting the tag to match the forged payload does not help either,
    /// because the tag is keyed by a secret the forger does not have.
    #[test]
    fn an_entry_retagged_without_the_secret_reads_as_a_miss() {
        let cache = cache("retagged");
        let key = cache.key("lint", "rules=all", "(defun f ())");
        assert!(cache.put(&key, &json!({"findings": ["a real finding"]})));

        let path = cache.root().join(&key[..2]).join(&key);
        let forged = r#"{"findings":[]}"#;
        // The unkeyed hash: what someone who can read the code but not the
        // secret file would compute.
        let tag = blake3::hash(forged.as_bytes()).to_hex();
        std::fs::write(&path, format!("{tag}\n{forged}")).expect("forge the entry");

        assert_eq!(cache.get(&key), None);
    }

    /// A genuine entry is still only an answer to its own question: moving one
    /// onto another key's path must not answer that key.
    #[test]
    fn an_entry_moved_onto_another_key_reads_as_a_miss() {
        let cache = cache("relocated");
        let source = cache.key("lint", "rules=all", "(defun f ())");
        let destination = cache.key("lint", "rules=all", "(defun g ())");
        assert!(cache.put(&source, &json!({"findings": ["a real finding"]})));

        let stored = std::fs::read_to_string(cache.root().join(&source[..2]).join(&source))
            .expect("read the entry");
        let moved = cache.root().join(&destination[..2]).join(&destination);
        std::fs::create_dir_all(moved.parent().expect("a shard")).expect("create the shard");
        std::fs::write(&moved, stored).expect("relocate the entry");

        assert_eq!(cache.get(&destination), None);
    }

    /// Two cache directories are two secrets, so an entry carried from one to
    /// the other is not evidence of anything in the other.
    #[test]
    fn a_secret_does_not_validate_another_directory_s_entries() {
        let theirs = cache("cross-theirs");
        let ours = cache("cross-ours");
        let key = theirs.key("lint", "rules=all", "(defun f ())");
        assert_eq!(key, ours.key("lint", "rules=all", "(defun f ())"));
        assert!(theirs.put(&key, &json!({"findings": []})));

        let stored = std::fs::read_to_string(theirs.root().join(&key[..2]).join(&key))
            .expect("read the entry");
        let planted = ours.root().join(&key[..2]).join(&key);
        std::fs::create_dir_all(planted.parent().expect("a shard")).expect("create the shard");
        std::fs::write(&planted, stored).expect("plant the entry");

        assert_eq!(theirs.get(&key), Some(json!({"findings": []})));
        assert_eq!(ours.get(&key), None);
    }

    /// A directory written before entries were authenticated has no secret and
    /// no tags. It is treated as entirely stale — one extra recompute — rather
    /// than migrated, since a migration would have to trust the bytes the tag
    /// exists to stop trusting.
    #[test]
    fn a_directory_from_before_authentication_is_stale_but_usable() {
        let legacy = cache("legacy");
        let root = legacy.root().to_path_buf();
        let key = legacy.key("lint", "rules=all", "(defun f ())");

        // What the previous format wrote: the bare JSON, and no secret file.
        let path = root.join(&key[..2]).join(&key);
        std::fs::create_dir_all(path.parent().expect("a shard")).expect("create the shard");
        std::fs::write(&path, r#"{"findings":[]}"#).expect("write a legacy entry");
        std::fs::remove_file(root.join(super::SECRET_FILE)).expect("remove the secret");

        let upgraded = AnalysisCache::open(&root, "1.2.3").expect("open cache");
        assert_eq!(upgraded.get(&key), None, "a legacy entry must be a miss");
        assert!(
            root.join(super::SECRET_FILE).is_file(),
            "the upgrade must mint a secret"
        );

        // And the cache works normally from there.
        assert!(upgraded.put(&key, &json!({"findings": ["fresh"]})));
        assert_eq!(upgraded.get(&key), Some(json!({"findings": ["fresh"]})));
    }

    /// Reopening the same directory must keep hitting: the secret is stored,
    /// not minted per process.
    #[test]
    fn a_reopened_directory_still_serves_its_entries() {
        let first = cache("reopen");
        let root = first.root().to_path_buf();
        let key = first.key("lint", "rules=all", "(defun f ())");
        assert!(first.put(&key, &json!({"findings": []})));

        let second = AnalysisCache::open(&root, "1.2.3").expect("open cache");
        assert_eq!(second.get(&key), Some(json!({"findings": []})));
    }

    /// Anyone who can read the secret can forge every entry in the directory.
    #[cfg(unix)]
    #[test]
    fn the_secret_is_readable_only_by_its_owner() {
        use std::os::unix::fs::PermissionsExt as _;

        let cache = cache("permissions");
        let metadata = std::fs::metadata(cache.root().join(super::SECRET_FILE))
            .expect("the secret was minted");

        assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
    }

    /// The key is what the forger sees; it must not reveal the secret, and two
    /// directories must agree on it so that content addressing still works.
    #[test]
    fn the_secret_does_not_change_the_key() {
        assert_eq!(
            cache("key-a").key("lint", "rules=all", "(defun f ())"),
            cache("key-b").key("lint", "rules=all", "(defun f ())")
        );
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
