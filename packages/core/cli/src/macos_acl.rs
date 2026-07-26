#![allow(unsafe_code)]

use std::fs::File;
use std::io;
use std::os::fd::AsRawFd;

const ACL_TYPE_EXTENDED: libc::c_int = 0x100;
const MAX_ACL_SERIALIZED_BYTES: usize = 1024 * 1024;

type Acl = *mut libc::c_void;

unsafe extern "C" {
    fn acl_copy_ext(buffer: *mut libc::c_void, acl: Acl, size: libc::ssize_t) -> libc::ssize_t;
    fn acl_copy_int(buffer: *const libc::c_void) -> Acl;
    fn acl_free(object: *mut libc::c_void) -> libc::c_int;
    fn acl_get_fd_np(fd: libc::c_int, acl_type: libc::c_int) -> Acl;
    fn acl_init(count: libc::c_int) -> Acl;
    fn acl_set_fd_np(fd: libc::c_int, acl: Acl, acl_type: libc::c_int) -> libc::c_int;
    fn acl_size(acl: Acl) -> libc::ssize_t;
    fn acl_valid(acl: Acl) -> libc::c_int;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SerializedAcl {
    bytes: Vec<u8>,
}

impl SerializedAcl {
    fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
}

struct OwnedAcl(Acl);

impl OwnedAcl {
    fn from_raw(acl: Acl) -> io::Result<Self> {
        if acl.is_null() {
            Err(io::Error::last_os_error())
        } else {
            Ok(Self(acl))
        }
    }
}

impl Drop for OwnedAcl {
    fn drop(&mut self) {
        // SAFETY: the pointer is an owned ACL returned by an ACL allocation function.
        unsafe {
            acl_free(self.0);
        }
    }
}

pub(super) fn read_acl(file: &File) -> io::Result<Option<SerializedAcl>> {
    // SAFETY: the file descriptor remains valid for the duration of the call.
    let acl =
        match OwnedAcl::from_raw(unsafe { acl_get_fd_np(file.as_raw_fd(), ACL_TYPE_EXTENDED) }) {
            Ok(acl) => acl,
            Err(error) if error.raw_os_error() == Some(libc::ENOENT) => return Ok(None),
            Err(error) => return Err(error),
        };
    // SAFETY: acl is a valid owned ACL.
    let size = unsafe { acl_size(acl.0) };
    if size < 0 {
        return Err(io::Error::last_os_error());
    }
    let size = usize::try_from(size)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "ACL is too large"))?;
    if size > MAX_ACL_SERIALIZED_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "ACL exceeds the serialization limit",
        ));
    }
    let mut bytes = vec![0_u8; size];
    // SAFETY: bytes has exactly size writable bytes and acl remains valid.
    let copied = unsafe {
        acl_copy_ext(
            bytes.as_mut_ptr().cast::<libc::c_void>(),
            acl.0,
            size as libc::ssize_t,
        )
    };
    if copied < 0 {
        return Err(io::Error::last_os_error());
    }
    if usize::try_from(copied).ok() != Some(size) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "ACL serialization returned an unexpected size",
        ));
    }
    Ok(Some(SerializedAcl { bytes }))
}

pub(super) fn write_acl(file: &File, serialized: Option<&SerializedAcl>) -> io::Result<()> {
    let Some(serialized) = serialized else {
        if read_acl(file)?.is_none() {
            return Ok(());
        }
        // macOS removes an extended ACL when an empty valid ACL is installed.
        // SAFETY: acl_init allocates an ACL owned by the caller.
        let acl = OwnedAcl::from_raw(unsafe { acl_init(0) })?;
        // SAFETY: acl is a valid owned ACL returned by acl_init.
        if unsafe { acl_valid(acl.0) } != 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: the file descriptor and ACL remain valid for the duration of the call.
        if unsafe { acl_set_fd_np(file.as_raw_fd(), acl.0, ACL_TYPE_EXTENDED) } != 0 {
            return Err(io::Error::last_os_error());
        }
        if read_acl(file)?.is_some() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "ACL read-back still contained an extended ACL after removal",
            ));
        }
        return Ok(());
    };
    let serialized_bytes = serialized.as_bytes();
    if serialized_bytes.is_empty() || serialized_bytes.len() > MAX_ACL_SERIALIZED_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "serialized ACL has an invalid size",
        ));
    }
    // SAFETY: serialized is the complete bounded representation produced by read_acl.
    let acl = OwnedAcl::from_raw(unsafe {
        acl_copy_int(serialized_bytes.as_ptr().cast::<libc::c_void>())
    })?;
    // SAFETY: acl is a complete representation produced by acl_copy_int.
    if unsafe { acl_valid(acl.0) } != 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: the file descriptor and ACL remain valid for the duration of the call.
    if unsafe { acl_set_fd_np(file.as_raw_fd(), acl.0, ACL_TYPE_EXTENDED) } != 0 {
        return Err(io::Error::last_os_error());
    }
    if read_acl(file)?.as_ref() != Some(serialized) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "ACL read-back did not match the requested ACL",
        ));
    }
    Ok(())
}
