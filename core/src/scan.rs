//! Parallel disk scan into a compact preorder tree.
//!
//! Every directory gets a node; files get one only when they are big enough to matter
//! (`ScanOptions::big_file`). Smaller files are folded into their directory's totals, which
//! keeps a multi-million-file home folder to a few hundred thousand nodes.

use std::collections::HashSet;
use std::io;
use std::ops::BitOr;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use rayon::prelude::*;

use crate::sys::{self, EntryKind};

/// Index into `Scan::nodes`.
pub type NodeId = u32;

pub const NO_PARENT: NodeId = NodeId::MAX;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeKind {
    Dir,
    File,
}

/// Notable entries found directly inside a directory; rules use them to recognize projects.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Markers(u16);

impl Markers {
    pub const GIT_DIR: Self = Self(1 << 0);
    pub const GIT_FILE: Self = Self(1 << 1);
    pub const CARGO: Self = Self(1 << 2);
    pub const NODE: Self = Self(1 << 3);
    pub const PYTHON: Self = Self(1 << 4);
    pub const SWIFTPM: Self = Self(1 << 5);
    pub const GRADLE: Self = Self(1 << 6);
    pub const CACHEDIR_TAG: Self = Self(1 << 7);
    /// Any file that makes a folder a project.
    pub const PROJECT: Self = Self(Self::CARGO.0 | Self::NODE.0 | Self::PYTHON.0 | Self::SWIFTPM.0 | Self::GRADLE.0);

    pub const fn intersects(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }

    fn of(name: &str, kind: EntryKind) -> Self {
        match (name, kind) {
            (".git", EntryKind::Dir) => Self::GIT_DIR,
            (".git", EntryKind::File) => Self::GIT_FILE,
            ("Cargo.toml", _) => Self::CARGO,
            ("package.json", _) => Self::NODE,
            ("pyproject.toml" | "requirements.txt" | "setup.py", _) => Self::PYTHON,
            ("Package.swift", _) => Self::SWIFTPM,
            ("build.gradle" | "build.gradle.kts" | "settings.gradle" | "settings.gradle.kts", _) => Self::GRADLE,
            ("CACHEDIR.TAG", _) => Self::CACHEDIR_TAG,
            _ => Self::default(),
        }
    }
}

impl BitOr for Markers {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

#[derive(Debug)]
pub struct Node {
    pub name: Box<str>,
    pub parent: NodeId,
    /// Exclusive end of this node's subtree: descendants are `id + 1 .. end`.
    pub end: NodeId,
    pub kind: NodeKind,
    /// Allocated bytes; for directories, the whole subtree.
    pub bytes: u64,
    /// Files in the subtree (1 for a file node).
    pub files: u32,
    /// Newest file modification in the subtree, unix seconds. Folder timestamps don't count:
    /// they change whenever anything inside is added or removed, even by moving it to the
    /// Trash and back. A subtree without files falls back to the folder's own time.
    pub newest: i64,
    /// Directories: newest of the files directly inside it, 0 when there are none.
    pub own_newest: i64,
    pub markers: Markers,
    /// Directory contents were not read: no permission, another volume, or an iCloud placeholder.
    pub unread: bool,
}

#[derive(Debug, Default)]
pub struct ScanStats {
    pub files: u64,
    pub dirs: u64,
    pub unread_dirs: u64,
    /// Folders left out because reading them needs Full Disk Access.
    pub needs_full_disk_access: Vec<PathBuf>,
    pub elapsed: Duration,
}

pub struct ScanOptions {
    /// Files at least this large get their own node.
    pub big_file: u64,
    /// Incremented with every directory entry listed, for a live progress readout.
    pub progress: Option<Arc<AtomicU64>>,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self { big_file: 1 << 20, progress: None }
    }
}

/// A finished scan. `nodes[0]` is the root, and nodes are in preorder with each
/// directory's children sorted largest first.
pub struct Scan {
    pub root: PathBuf,
    pub nodes: Vec<Node>,
    pub stats: ScanStats,
}

/// Worker pool for scans and repo checks: big stacks for deep trees, and every thread
/// refuses to download iCloud placeholders. Nested scans reuse it instead of spawning more.
pub(crate) fn pool() -> &'static rayon::ThreadPool {
    static POOL: OnceLock<rayon::ThreadPool> = OnceLock::new();
    POOL.get_or_init(|| {
        rayon::ThreadPoolBuilder::new()
            .stack_size(16 << 20)
            .start_handler(|_| sys::forbid_dataless_downloads())
            .build()
            .expect("failed to start the scan thread pool")
    })
}

pub fn scan(root: &Path, opts: &ScanOptions) -> io::Result<Scan> {
    let started = Instant::now();
    let root = root.canonicalize()?;
    let meta = std::fs::symlink_metadata(&root)?;
    // Without Full Disk Access, each protected folder in other apps' containers stalls for
    // seconds in the privacy daemon before being denied, so leave them out entirely.
    let home = crate::home();
    let skip: Vec<PathBuf> = if sys::has_full_disk_access(&home) {
        Vec::new()
    } else {
        ["Library/Containers", "Library/Group Containers"]
            .iter()
            .map(|rel| home.join(rel))
            .filter(|p| p.starts_with(&root) && p.exists())
            .collect()
    };
    let ctx = Ctx {
        opts,
        root_dev: meta.dev() as u64,
        skip: &skip,
        hard_links: std::array::from_fn(|_| Mutex::default()),
        unread: AtomicU64::new(0),
    };
    let tree = pool().install(|| scan_dir(&ctx, &root, root.to_string_lossy().into(), meta.mtime()));

    let mut nodes = Vec::new();
    flatten(tree, NO_PARENT, &mut nodes);
    let stats = ScanStats {
        files: u64::from(nodes[0].files),
        dirs: nodes.iter().filter(|n| n.kind == NodeKind::Dir).count() as u64,
        unread_dirs: ctx.unread.load(Ordering::Relaxed),
        needs_full_disk_access: skip,
        elapsed: started.elapsed(),
    };
    Ok(Scan { root, nodes, stats })
}

impl Scan {
    pub fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id as usize]
    }

    pub fn children(&self, id: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        let end = self.node(id).end;
        let mut next = id + 1;
        std::iter::from_fn(move || {
            (next < end).then(|| {
                let child = next;
                next = self.node(child).end;
                child
            })
        })
    }

    pub fn child(&self, id: NodeId, name: &str) -> Option<NodeId> {
        self.children(id).find(|&c| &*self.node(c).name == name)
    }

    /// Absolute path of a node.
    pub fn path(&self, id: NodeId) -> PathBuf {
        let mut names = Vec::new();
        let mut at = id;
        while at != 0 {
            names.push(&*self.node(at).name);
            at = self.node(at).parent;
        }
        let mut path = self.root.clone();
        path.extend(names.iter().rev());
        path
    }

    /// Node at an absolute path, if the scan covered it.
    pub fn find(&self, path: &Path) -> Option<NodeId> {
        let rest = path.strip_prefix(&self.root).ok()?;
        rest.iter().try_fold(0, |at, name| self.child(at, name.to_str()?))
    }

    /// Whether `id` is `ancestor` or inside it.
    pub fn contains(&self, ancestor: NodeId, id: NodeId) -> bool {
        ancestor <= id && id < self.node(ancestor).end
    }
}

struct Ctx<'a> {
    opts: &'a ScanOptions,
    root_dev: u64,
    skip: &'a [PathBuf],
    /// File ids of hard-linked files already counted, sharded to keep lock contention low.
    hard_links: [Mutex<HashSet<u64>>; 16],
    unread: AtomicU64,
}

impl Ctx<'_> {
    /// True the first time a hard-linked file is seen, so its bytes count once.
    fn first_sight(&self, file_id: u64) -> bool {
        self.hard_links[(file_id % 16) as usize].lock().unwrap().insert(file_id)
    }
}

struct DirTree {
    name: Box<str>,
    bytes: u64,
    files: u32,
    newest: i64,
    own_newest: i64,
    markers: Markers,
    unread: bool,
    big_files: Vec<FileLeaf>,
    subdirs: Vec<DirTree>,
}

struct FileLeaf {
    name: Box<str>,
    bytes: u64,
    mtime: i64,
}

impl DirTree {
    fn unread(name: Box<str>, mtime: i64) -> Self {
        Self {
            name,
            bytes: 0,
            files: 0,
            newest: mtime,
            own_newest: mtime,
            markers: Markers::default(),
            unread: true,
            big_files: Vec::new(),
            subdirs: Vec::new(),
        }
    }
}

fn scan_dir(ctx: &Ctx, path: &Path, name: Box<str>, mtime: i64) -> DirTree {
    if ctx.skip.iter().any(|s| s == path) {
        return DirTree::unread(name, mtime);
    }
    let listing = match sys::list_dir(path) {
        Ok(listing) if listing.dev == ctx.root_dev => listing,
        _ => {
            ctx.unread.fetch_add(1, Ordering::Relaxed);
            return DirTree::unread(name, mtime);
        }
    };

    if let Some(progress) = &ctx.opts.progress {
        progress.fetch_add(listing.entries.len() as u64, Ordering::Relaxed);
    }
    let mut tree = DirTree { unread: false, own_newest: 0, ..DirTree::unread(name, mtime) };
    let mut dirs = Vec::new();
    for entry in listing.entries {
        tree.markers = tree.markers | Markers::of(&entry.name, entry.kind);
        match entry.kind {
            EntryKind::Dir if entry.dataless => {
                ctx.unread.fetch_add(1, Ordering::Relaxed);
                tree.subdirs.push(DirTree::unread(entry.name, entry.mtime));
            }
            EntryKind::Dir => dirs.push(entry),
            EntryKind::File | EntryKind::Symlink => {
                let bytes = if entry.links > 1 && !ctx.first_sight(entry.file_id) { 0 } else { entry.bytes };
                tree.bytes += bytes;
                tree.files += 1;
                tree.own_newest = tree.own_newest.max(entry.mtime);
                if bytes >= ctx.opts.big_file {
                    tree.big_files.push(FileLeaf { name: entry.name, bytes, mtime: entry.mtime });
                }
            }
            EntryKind::Other => {}
        }
    }

    let scanned: Vec<DirTree> =
        dirs.into_par_iter().map(|entry| scan_dir(ctx, &path.join(&*entry.name), entry.name, entry.mtime)).collect();
    tree.subdirs.extend(scanned);

    tree.newest = tree.own_newest;
    for sub in &tree.subdirs {
        tree.bytes += sub.bytes;
        tree.files += sub.files;
        if sub.files > 0 {
            tree.newest = tree.newest.max(sub.newest);
        }
    }
    if tree.files == 0 {
        tree.newest = mtime;
    }
    tree.subdirs.sort_unstable_by_key(|d| std::cmp::Reverse(d.bytes));
    tree.big_files.sort_unstable_by_key(|f| std::cmp::Reverse(f.bytes));
    tree
}

/// Appends a directory and its subtree in preorder, merging files and subdirectories
/// largest first.
fn flatten(tree: DirTree, parent: NodeId, nodes: &mut Vec<Node>) {
    let id = nodes.len();
    nodes.push(Node {
        name: tree.name,
        parent,
        end: 0,
        kind: NodeKind::Dir,
        bytes: tree.bytes,
        files: tree.files,
        newest: tree.newest,
        own_newest: tree.own_newest,
        markers: tree.markers,
        unread: tree.unread,
    });
    let mut files = tree.big_files.into_iter().peekable();
    let mut dirs = tree.subdirs.into_iter().peekable();
    loop {
        let take_file = match (files.peek(), dirs.peek()) {
            (Some(f), Some(d)) => f.bytes >= d.bytes,
            (Some(_), None) => true,
            (None, Some(_)) => false,
            (None, None) => break,
        };
        if take_file {
            let file = files.next().unwrap();
            let file_id = nodes.len() as NodeId;
            nodes.push(Node {
                name: file.name,
                parent: id as NodeId,
                end: file_id + 1,
                kind: NodeKind::File,
                bytes: file.bytes,
                files: 1,
                newest: file.mtime,
                own_newest: file.mtime,
                markers: Markers::default(),
                unread: false,
            });
        } else {
            flatten(dirs.next().unwrap(), id as NodeId, nodes);
        }
    }
    nodes[id].end = nodes.len() as NodeId;
}
