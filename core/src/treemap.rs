//! The space map: a squarified treemap of a scan, colored by what kind of data each folder
//! holds and marked where findings can free space.

use std::cmp::Reverse;
use std::path::{Path, PathBuf};

use crate::finding::{Action, Finding, Safety};
use crate::scan::{Markers, NO_PARENT, Node, NodeId, NodeKind, Scan};

/// What a folder or file mostly is, for coloring the map.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "ffi", derive(uniffi::Enum))]
pub enum Kind {
    Code,
    Git,
    Cache,
    Toolchain,
    Synced,
    Media,
    Documents,
    Apps,
    Other,
}

/// One rectangle of the map, in the coordinates of the box it was laid out in.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct Tile {
    /// The scan node, or for a `rest` tile, the folder whose small files it stands for.
    pub node: NodeId,
    /// Stands for a folder's files too small to get their own tile.
    pub rest: bool,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    /// 1 for the shown folder's children, 2 for theirs, and so on.
    pub depth: u32,
    pub name: String,
    pub path: PathBuf,
    pub bytes: u64,
    pub kind: Kind,
    pub is_dir: bool,
    /// Safety of the finding this tile is, or is inside of.
    pub reclaimable: Option<Safety>,
    /// This tile is exactly a finding's path, not just inside one.
    pub finding: bool,
}

/// A folder on the way from the scan's root to the shown one.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct Crumb {
    pub node: NodeId,
    pub name: String,
}

#[derive(Clone, Debug)]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct SpaceMap {
    /// The folder shown.
    pub node: NodeId,
    /// From the scan's root down to the shown folder.
    pub trail: Vec<Crumb>,
    /// Parents before their children, so later tiles draw on top.
    pub tiles: Vec<Tile>,
}

/// Per-node facts the map needs besides the scan: each node's kind and the findings that
/// cover it. Built once per review.
pub struct Atlas {
    kinds: Vec<Kind>,
    reclaimable: Vec<Option<Safety>>,
    findings: Vec<bool>,
}

/// Folder name header height, and the gap between a folder's edge and its children.
const HEADER: f64 = 18.0;
const PAD: f64 = 2.0;
/// Tiles thinner than this aren't worth drawing.
const MIN_SIDE: f64 = 3.0;

impl Atlas {
    pub fn new(scan: &Scan, findings: &[Finding]) -> Self {
        let n = scan.nodes.len();
        let mut reclaimable = vec![None; n];
        let mut roots = vec![false; n];
        for f in findings {
            if !matches!(f.action, Action::Trash { .. } | Action::RemoveWorktree { .. }) {
                continue;
            }
            if let Some(id) = scan.find(&f.path) {
                reclaimable[id as usize] = Some(f.safety);
                roots[id as usize] = true;
            }
        }
        // Preorder puts every parent before its children, so one pass can inherit.
        let mut kinds = vec![Kind::Other; n];
        for (id, node) in scan.nodes.iter().enumerate() {
            let (kind, mark) = match node.parent {
                NO_PARENT => (Kind::Other, None),
                parent => (kinds[parent as usize], reclaimable[parent as usize]),
            };
            kinds[id] = own_kind(node).unwrap_or(kind);
            reclaimable[id] = reclaimable[id].or(mark);
        }
        Self { kinds, reclaimable, findings: roots }
    }

    /// Lays out `node` (the scan's root when `None` or not a folder) in a `width` × `height`
    /// box, `depth` levels deep.
    pub fn map(&self, scan: &Scan, node: Option<NodeId>, width: f64, height: f64, depth: u32) -> SpaceMap {
        let node = node.filter(|&n| (n as usize) < scan.nodes.len() && scan.node(n).kind == NodeKind::Dir).unwrap_or(0);
        let mut tiles = Vec::new();
        let bounds = Rect { x: 0.0, y: 0.0, w: width.max(0.0), h: height.max(0.0) };
        self.place_children(scan, node, bounds, 1, depth, &mut tiles);
        SpaceMap { node, trail: trail(scan, node), tiles }
    }

    fn place_children(&self, scan: &Scan, parent: NodeId, rect: Rect, depth: u32, max_depth: u32, out: &mut Vec<Tile>) {
        let mut items: Vec<(Option<NodeId>, u64)> =
            scan.children(parent).map(|c| (Some(c), scan.node(c).bytes)).filter(|&(_, b)| b > 0).collect();
        let listed: u64 = items.iter().map(|&(_, b)| b).sum();
        let rest = scan.node(parent).bytes.saturating_sub(listed);
        if rest > 0 {
            items.push((None, rest));
            items.sort_by_key(|&(_, b)| Reverse(b));
        }
        let sizes: Vec<f64> = items.iter().map(|&(_, b)| b as f64).collect();
        for (&(item, bytes), r) in items.iter().zip(squarify(&sizes, rect)) {
            if r.w < MIN_SIDE || r.h < MIN_SIDE {
                continue;
            }
            let Some(id) = item else {
                out.push(self.tile(scan, parent, true, r, depth, bytes));
                continue;
            };
            out.push(self.tile(scan, id, false, r, depth, bytes));
            let node = scan.node(id);
            if node.kind == NodeKind::Dir
                && !node.unread
                && depth < max_depth
                && let Some(inner) = r.inner()
            {
                self.place_children(scan, id, inner, depth + 1, max_depth, out);
            }
        }
    }

    fn tile(&self, scan: &Scan, id: NodeId, rest: bool, r: Rect, depth: u32, bytes: u64) -> Tile {
        let node = scan.node(id);
        Tile {
            node: id,
            rest,
            x: r.x,
            y: r.y,
            width: r.w,
            height: r.h,
            depth,
            name: if rest { "Smaller files".into() } else { node.name.to_string() },
            path: scan.path(id),
            bytes,
            kind: self.kinds[id as usize],
            is_dir: !rest && node.kind == NodeKind::Dir,
            reclaimable: self.reclaimable[id as usize],
            finding: !rest && self.findings[id as usize],
        }
    }
}

fn trail(scan: &Scan, node: NodeId) -> Vec<Crumb> {
    let mut trail = Vec::new();
    let mut at = node;
    while at != NO_PARENT {
        trail.push(Crumb { node: at, name: scan.node(at).name.to_string() });
        at = scan.node(at).parent;
    }
    trail.reverse();
    trail
}

const MEDIA_EXTENSIONS: &[&str] = &[
    "mov", "mp4", "m4v", "mkv", "avi", "webm", "jpg", "jpeg", "png", "heic", "gif", "tiff", "raw", "dng", "cr2", "arw",
    "mp3", "m4a", "aac", "wav", "flac", "aiff",
];

/// A node's own kind, when its name or contents say; otherwise it takes its parent's.
fn own_kind(node: &Node) -> Option<Kind> {
    let name = &*node.name;
    if node.kind == NodeKind::File {
        let ext = Path::new(name).extension()?.to_str()?.to_ascii_lowercase();
        return MEDIA_EXTENSIONS.contains(&ext.as_str()).then_some(Kind::Media);
    }
    let kind = match name {
        ".git" => Kind::Git,
        _ if node.markers.intersects(Markers::GIT_DIR | Markers::GIT_FILE | Markers::PROJECT) => Kind::Code,
        "Caches" | ".cache" | "Cache" | "caches" | "CachedData" | "Code Cache" | "GPUCache" => Kind::Cache,
        _ if node.markers.intersects(Markers::CACHEDIR_TAG) => Kind::Cache,
        ".rustup" | ".cargo" | ".bun" | ".npm" | ".nvm" | ".volta" | ".pyenv" | ".local" | ".gradle" | ".m2"
        | ".deno" | "Developer" => Kind::Toolchain,
        "Mobile Documents" | "CloudStorage" | "Dropbox" | "Google Drive" | "OneDrive" => Kind::Synced,
        "Pictures" | "Movies" | "Music" => Kind::Media,
        _ if name.ends_with(".photoslibrary") || name.ends_with(".musiclibrary") => Kind::Media,
        "Documents" | "Desktop" | "Downloads" => Kind::Documents,
        "Application Support" | "Containers" | "Group Containers" | "Applications" => Kind::Apps,
        _ => return None,
    };
    Some(kind)
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Rect {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

impl Rect {
    /// Room for a folder's children: inside a small margin, below its name header when the
    /// folder is big enough to show one. `None` when too small to be worth subdividing.
    fn inner(self) -> Option<Rect> {
        if self.w < 30.0 || self.h < 30.0 {
            return None;
        }
        let header = if self.w >= 56.0 && self.h >= 44.0 { HEADER } else { 0.0 };
        Some(Rect { x: self.x + PAD, y: self.y + header + PAD, w: self.w - 2.0 * PAD, h: self.h - header - 2.0 * PAD })
    }
}

/// Splits `rect` into rectangles with areas proportional to `sizes` (largest first), keeping
/// them as close to square as possible: the squarified layout of Bruls, Huizing and van Wijk.
fn squarify(sizes: &[f64], rect: Rect) -> Vec<Rect> {
    let total: f64 = sizes.iter().sum();
    if total <= 0.0 || rect.w <= 0.0 || rect.h <= 0.0 {
        return vec![Rect { x: rect.x, y: rect.y, w: 0.0, h: 0.0 }; sizes.len()];
    }
    let scale = rect.w * rect.h / total;
    let areas: Vec<f64> = sizes.iter().map(|s| s * scale).collect();
    let mut out = Vec::with_capacity(areas.len());
    let mut free = rect;
    let mut start = 0;
    while start < areas.len() {
        let side = free.w.min(free.h);
        // Grow the row along the short side while that keeps its tiles squarer.
        let mut end = start + 1;
        let mut sum = areas[start];
        while end < areas.len()
            && worst(&areas[start..=end], sum + areas[end], side) <= worst(&areas[start..end], sum, side)
        {
            sum += areas[end];
            end += 1;
        }
        let thickness = sum / side;
        let mut offset = 0.0;
        for &area in &areas[start..end] {
            let length = area / thickness;
            out.push(if free.w >= free.h {
                Rect { x: free.x, y: free.y + offset, w: thickness, h: length }
            } else {
                Rect { x: free.x + offset, y: free.y, w: length, h: thickness }
            });
            offset += length;
        }
        if free.w >= free.h {
            free.x += thickness;
            free.w = (free.w - thickness).max(0.0);
        } else {
            free.y += thickness;
            free.h = (free.h - thickness).max(0.0);
        }
        start = end;
    }
    out
}

/// The worst aspect ratio in a row of `areas` totalling `sum`, laid along `side`.
fn worst(areas: &[f64], sum: f64, side: f64) -> f64 {
    let (min, max) = areas.iter().fold((f64::MAX, 0.0_f64), |(lo, hi), &a| (lo.min(a), hi.max(a)));
    let (s2, w2) = (sum * sum, side * side);
    (w2 * max / s2).max(s2 / (w2 * min))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn squarify_fills_the_box_with_proportional_non_overlapping_tiles() {
        // The worked example from the paper: a 6 × 4 box and areas that sum to 24.
        let sizes = [6.0, 6.0, 4.0, 3.0, 2.0, 2.0, 1.0];
        let bounds = Rect { x: 0.0, y: 0.0, w: 6.0, h: 4.0 };
        let tiles = squarify(&sizes, bounds);

        for (tile, size) in tiles.iter().zip(sizes) {
            assert!((tile.w * tile.h - size).abs() < 1e-9, "{tile:?} should have area {size}");
            assert!(tile.x >= -1e-9 && tile.y >= -1e-9, "{tile:?}");
            assert!(tile.x + tile.w <= bounds.w + 1e-9 && tile.y + tile.h <= bounds.h + 1e-9, "{tile:?}");
        }
        for (i, a) in tiles.iter().enumerate() {
            for b in &tiles[i + 1..] {
                let overlap_w = (a.x + a.w).min(b.x + b.w) - a.x.max(b.x);
                let overlap_h = (a.y + a.h).min(b.y + b.h) - a.y.max(b.y);
                assert!(overlap_w <= 1e-9 || overlap_h <= 1e-9, "{a:?} overlaps {b:?}");
            }
        }
        // Squarified: no tile worse than 3:1, where slice-and-dice would give 6:1.
        let worst_ratio = tiles.iter().map(|t| (t.w / t.h).max(t.h / t.w)).fold(0.0, f64::max);
        assert!(worst_ratio < 3.0, "{worst_ratio}");
    }
}
