#[cfg(unix)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FilesystemIdentity {
    Unix { device: u64, inode: u64 },
}

#[cfg(not(unix))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) struct FilesystemIdentity(());

impl FilesystemIdentity {
    #[cfg(unix)]
    pub(crate) fn from_std(metadata: &std::fs::Metadata) -> Option<Self> {
        use std::os::unix::fs::MetadataExt as _;

        Some(Self::Unix {
            device: metadata.dev(),
            inode: metadata.ino(),
        })
    }

    #[cfg(unix)]
    pub(crate) fn from_cap(metadata: &cap_std::fs::Metadata) -> Option<Self> {
        use cap_std::fs::MetadataExt as _;

        Some(Self::Unix {
            device: metadata.dev(),
            inode: metadata.ino(),
        })
    }

    #[cfg(not(unix))]
    pub(crate) fn from_std(_metadata: &std::fs::Metadata) -> Option<Self> {
        None
    }

    #[cfg(not(unix))]
    pub(crate) fn from_cap(_metadata: &cap_std::fs::Metadata) -> Option<Self> {
        None
    }
}
