//! The content hash that manifests, journals, and write guards all quote.
//!
//! It lives here rather than beside the writer because three unrelated callers
//! now compare against it — the refactor manifest's `input_hash`, the undo
//! journal's two endpoints, and the report cache's key — and a second spelling
//! of the same digest would silently fail to match the first.

/// A stable, self-describing digest of a document's bytes.
///
/// The `fnv1a64:` prefix is part of the value. It makes a hash in a manifest
/// or a journal readable as "which function produced this", so a future change
/// of algorithm is a visible mismatch rather than a silent one.
#[must_use]
pub fn stable_text_hash(text: &str) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("fnv1a64:{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::stable_text_hash;

    #[test]
    fn hash_is_prefixed_and_fixed_width() {
        let hash = stable_text_hash("(defun f ())\n");
        assert!(hash.starts_with("fnv1a64:"), "unexpected hash: {hash}");
        assert_eq!(hash.len(), "fnv1a64:".len() + 16);
    }

    #[test]
    fn empty_input_hashes_to_the_offset_basis() {
        assert_eq!(stable_text_hash(""), "fnv1a64:cbf29ce484222325");
    }

    #[test]
    fn distinct_documents_hash_distinctly() {
        assert_ne!(stable_text_hash("(a)"), stable_text_hash("(b)"));
    }
}
