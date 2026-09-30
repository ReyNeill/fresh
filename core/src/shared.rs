//! Copies can share data: APFS clones share blocks, and hard links share a whole file.
//! Deleting one copy frees none of that; it's freed once every copy goes. This measures what
//! each finding frees on its own, and what findings only free together.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

use rayon::prelude::*;

use crate::finding::{Finding, Joint};
use crate::scan::pool;
use crate::sys::{self, Entry, EntryKind};

/// Keeps hard-linked file ids apart from clone ids.
const HARD_LINK: u64 = 1 << 63;

/// What deleting a folder or file frees, and what it shares.
#[derive(Default)]
struct Sharing {
    /// Freed by deleting it, whatever else stays.
    private: u64,
    /// Data it shares with other files, as (key, copies, bytes).
    shared: Vec<(u64, u32, u64)>,
}

impl Sharing {
    fn add(&mut self, entry: &Entry) {
        if entry.links > 1 {
            self.shared.push((entry.file_id | HARD_LINK, entry.links, entry.bytes));
        } else if entry.clones > 1 {
            self.shared.push((entry.clone_id, entry.clones, entry.bytes));
        } else {
            // Short of `bytes` only when a snapshot or an edited clone holds the rest, which
            // deleting this can't free.
            self.private += entry.private;
        }
    }

    fn merge(mut self, other: Sharing) -> Sharing {
        self.private += other.private;
        self.shared.extend(other.shared);
        self
    }
}

/// Sets each space-freeing finding's `bytes` to what applying it alone frees, and returns
/// the space only several findings free together. Copies are counted toward the outermost
/// finding holding them.
pub(crate) fn resolve(findings: &mut [Finding]) -> Vec<Joint> {
    let freeing: Vec<usize> = (0..findings.len()).filter(|&i| findings[i].action.removes_files()).collect();
    let nested: Vec<bool> = freeing
        .iter()
        .map(|&i| {
            let path = &findings[i].path;
            freeing.iter().any(|&j| findings[j].path != *path && path.starts_with(&findings[j].path))
        })
        .collect();
    let measured: Vec<Sharing> = pool().install(|| freeing.par_iter().map(|&i| measure(&findings[i].path)).collect());

    struct Family {
        copies: u32,
        seen: u32,
        bytes: u64,
        owners: Vec<usize>,
    }
    let mut families: HashMap<u64, Family> = HashMap::new();
    for ((&i, sharing), nested) in freeing.iter().zip(measured).zip(nested) {
        findings[i].bytes = sharing.private;
        if nested {
            continue;
        }
        for (key, copies, bytes) in sharing.shared {
            let family = families.entry(key).or_insert(Family { copies, seen: 0, bytes: 0, owners: Vec::new() });
            family.seen += 1;
            family.bytes = family.bytes.max(bytes);
            if !family.owners.contains(&i) {
                family.owners.push(i);
            }
        }
    }

    let mut joint: BTreeMap<Vec<usize>, u64> = BTreeMap::new();
    for mut family in families.into_values() {
        // A copy outside every finding keeps the data.
        if family.seen < family.copies {
            continue;
        }
        family.owners.sort_unstable();
        match family.owners[..] {
            [only] => findings[only].bytes += family.bytes,
            _ => *joint.entry(family.owners).or_default() += family.bytes,
        }
    }
    joint
        .into_iter()
        .map(|(owners, bytes)| Joint { findings: owners.iter().map(|&i| findings[i].id.clone()).collect(), bytes })
        .collect()
}

/// Measures a finding's path: a folder's subtree on its own volume, or a single file.
fn measure(path: &Path) -> Sharing {
    let Ok(meta) = fs::symlink_metadata(path) else { return Sharing::default() };
    if meta.is_dir() {
        return walk(path, meta.dev());
    }
    let mut sharing = Sharing::default();
    if let (Some(dir), Some(name)) = (path.parent(), path.file_name().and_then(|n| n.to_str()))
        && let Ok(listing) = sys::list_dir(dir, true)
        && let Some(entry) = listing.entries.iter().find(|e| &*e.name == name)
    {
        sharing.add(entry);
    }
    sharing
}

fn walk(dir: &Path, dev: u64) -> Sharing {
    let Ok(listing) = sys::list_dir(dir, true) else { return Sharing::default() };
    if listing.dev != dev {
        return Sharing::default();
    }
    let mut sharing = Sharing::default();
    let mut dirs = Vec::new();
    for entry in listing.entries {
        match entry.kind {
            EntryKind::Dir if !entry.dataless => dirs.push(entry.name),
            EntryKind::File | EntryKind::Symlink => sharing.add(&entry),
            _ => {}
        }
    }
    let below = dirs.into_par_iter().map(|name| walk(&dir.join(&*name), dev)).reduce(Sharing::default, Sharing::merge);
    sharing.merge(below)
}
