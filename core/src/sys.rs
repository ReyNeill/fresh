//! macOS filesystem primitives: bulk directory listing and iCloud safety.

use std::cell::RefCell;
use std::ffi::{CString, c_int, c_void};
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

// Values from <sys/attr.h>, <sys/vnode.h>, <sys/stat.h> and <sys/resource.h>.
const ATTR_BIT_MAP_COUNT: u16 = 5;
const ATTR_CMN_NAME: u32 = 0x0000_0001;
const ATTR_CMN_OBJTYPE: u32 = 0x0000_0008;
const ATTR_CMN_MODTIME: u32 = 0x0000_0400;
const ATTR_CMN_FLAGS: u32 = 0x0004_0000;
const ATTR_CMN_FILEID: u32 = 0x0200_0000;
const ATTR_CMN_ERROR: u32 = 0x2000_0000;
const ATTR_CMN_RETURNED_ATTRS: u32 = 0x8000_0000;
const ATTR_FILE_LINKCOUNT: u32 = 0x0000_0001;
const ATTR_FILE_ALLOCSIZE: u32 = 0x0000_0004;
const VREG: u32 = 1;
const VDIR: u32 = 2;
const VLNK: u32 = 5;
const SF_DATALESS: u32 = 0x4000_0000;
const IOPOL_TYPE_VFS_MATERIALIZE_DATALESS_FILES: c_int = 3;
const IOPOL_SCOPE_THREAD: c_int = 1;
const IOPOL_MATERIALIZE_DATALESS_FILES_OFF: c_int = 1;

#[repr(C)]
struct AttrList {
    bitmapcount: u16,
    reserved: u16,
    commonattr: u32,
    volattr: u32,
    dirattr: u32,
    fileattr: u32,
    forkattr: u32,
}

unsafe extern "C" {
    fn getattrlistbulk(
        dirfd: c_int,
        attr_list: *mut c_void,
        attr_buf: *mut c_void,
        attr_buf_size: usize,
        options: u64,
    ) -> c_int;
    fn setiopolicy_np(iotype: c_int, scope: c_int, policy: c_int) -> c_int;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryKind {
    File,
    Dir,
    Symlink,
    Other,
}

/// One directory entry as reported by `getattrlistbulk`.
#[derive(Debug)]
pub struct Entry {
    pub name: Box<str>,
    pub kind: EntryKind,
    /// Allocated bytes on disk (what deleting it can free), not the apparent length.
    pub bytes: u64,
    pub mtime: i64,
    /// An iCloud/File Provider placeholder whose contents live in the cloud.
    pub dataless: bool,
    pub file_id: u64,
    pub links: u32,
}

/// A listed directory: its device (to detect mount points) and its entries.
pub struct Listing {
    pub dev: u64,
    pub entries: Vec<Entry>,
}

thread_local! {
    static BUF: RefCell<Vec<u8>> = RefCell::new(vec![0; 256 * 1024]);
}

/// Makes the calling thread fail on iCloud placeholders instead of downloading them.
/// Scan threads call this so walking a synced folder never pulls data from the network.
pub fn forbid_dataless_downloads() {
    unsafe {
        setiopolicy_np(
            IOPOL_TYPE_VFS_MATERIALIZE_DATALESS_FILES,
            IOPOL_SCOPE_THREAD,
            IOPOL_MATERIALIZE_DATALESS_FILES_OFF,
        );
    }
}

/// Whether this process has Full Disk Access. The TCC database is only readable with it,
/// and the check fails instantly instead of waiting on the privacy daemon.
pub fn has_full_disk_access(home: &Path) -> bool {
    std::fs::File::open(home.join("Library/Application Support/com.apple.TCC/TCC.db")).is_ok()
}

/// Lists a directory with one syscall per batch of entries instead of one `lstat` per file.
pub fn list_dir(path: &Path) -> io::Result<Listing> {
    let cpath = CString::new(path.as_os_str().as_bytes())?;
    let raw = unsafe { libc::open(cpath.as_ptr(), libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC) };
    if raw < 0 {
        return Err(io::Error::last_os_error());
    }
    let fd = unsafe { OwnedFd::from_raw_fd(raw) };
    let mut st: libc::stat = unsafe { std::mem::zeroed() };
    if unsafe { libc::fstat(fd.as_raw_fd(), &mut st) } != 0 {
        return Err(io::Error::last_os_error());
    }

    let mut attrs = AttrList {
        bitmapcount: ATTR_BIT_MAP_COUNT,
        reserved: 0,
        commonattr: ATTR_CMN_RETURNED_ATTRS
            | ATTR_CMN_NAME
            | ATTR_CMN_ERROR
            | ATTR_CMN_OBJTYPE
            | ATTR_CMN_MODTIME
            | ATTR_CMN_FLAGS
            | ATTR_CMN_FILEID,
        volattr: 0,
        dirattr: 0,
        fileattr: ATTR_FILE_LINKCOUNT | ATTR_FILE_ALLOCSIZE,
        forkattr: 0,
    };

    BUF.with_borrow_mut(|buf| {
        let mut entries = Vec::new();
        loop {
            let count = unsafe {
                getattrlistbulk(fd.as_raw_fd(), (&raw mut attrs).cast(), buf.as_mut_ptr().cast(), buf.len(), 0)
            };
            if count < 0 {
                return Err(io::Error::last_os_error());
            }
            if count == 0 {
                break;
            }
            let mut offset = 0;
            for _ in 0..count {
                let len = read_u32(buf, offset) as usize;
                if let Some(entry) = parse_entry(&buf[offset..offset + len]) {
                    entries.push(entry);
                }
                offset += len;
            }
        }
        Ok(Listing { dev: st.st_dev as u64, entries })
    })
}

/// Decodes one packed entry. Attributes follow the returned-attribute set in bit order,
/// except `ATTR_CMN_ERROR`, which comes first (see getattrlistbulk(2)).
fn parse_entry(e: &[u8]) -> Option<Entry> {
    let common = read_u32(e, 4);
    let file = read_u32(e, 4 + 12);
    let mut p = 4 + 20;

    if common & ATTR_CMN_ERROR != 0 {
        let error = read_u32(e, p);
        p += 4;
        if error != 0 {
            return None;
        }
    }

    // attrreference_t: offset is relative to the reference itself; length includes the NUL.
    let name_start = p.checked_add_signed(read_u32(e, p) as i32 as isize)?;
    let name_len = read_u32(e, p + 4) as usize;
    let name_bytes = e.get(name_start..name_start + name_len.saturating_sub(1))?;
    let name: Box<str> = String::from_utf8_lossy(name_bytes).into();
    p += 8;

    let mut kind = EntryKind::Other;
    if common & ATTR_CMN_OBJTYPE != 0 {
        kind = match read_u32(e, p) {
            VREG => EntryKind::File,
            VDIR => EntryKind::Dir,
            VLNK => EntryKind::Symlink,
            _ => EntryKind::Other,
        };
        p += 4;
    }
    let mut mtime = 0;
    if common & ATTR_CMN_MODTIME != 0 {
        mtime = read_u64(e, p) as i64;
        p += 16;
    }
    let mut flags = 0;
    if common & ATTR_CMN_FLAGS != 0 {
        flags = read_u32(e, p);
        p += 4;
    }
    let mut file_id = 0;
    if common & ATTR_CMN_FILEID != 0 {
        file_id = read_u64(e, p);
        p += 8;
    }

    let mut links = 1;
    if file & ATTR_FILE_LINKCOUNT != 0 {
        links = read_u32(e, p);
        p += 4;
    }
    let mut bytes = 0;
    if file & ATTR_FILE_ALLOCSIZE != 0 {
        bytes = read_u64(e, p);
    }

    Some(Entry { name, kind, bytes, mtime, dataless: flags & SF_DATALESS != 0, file_id, links })
}

fn read_u32(buf: &[u8], at: usize) -> u32 {
    u32::from_ne_bytes(buf[at..at + 4].try_into().unwrap())
}

fn read_u64(buf: &[u8], at: usize) -> u64 {
    u64::from_ne_bytes(buf[at..at + 8].try_into().unwrap())
}
