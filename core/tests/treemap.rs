use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime};

use fresh_core::treemap::{Atlas, Kind};
use fresh_core::{ReviewOptions, Safety, ScanOptions, review, scan};

fn write(path: &Path, bytes: usize) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, vec![1u8; bytes]).unwrap();
}

#[test]
fn space_map_colors_by_kind_and_marks_what_can_go() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    write(&root.join("code/app/package.json"), 10);
    write(&root.join("code/app/node_modules/dep/index.js"), 2 << 20);
    write(&root.join("code/app/.git/objects/pack"), 1 << 20);
    write(&root.join("Pictures/trip.mov"), 2 << 20);
    let old = SystemTime::now() - Duration::from_secs(40 * 86_400);
    fs::File::open(root.join("code/app/package.json")).unwrap().set_modified(old).unwrap();

    let scan = scan(&root, &ScanOptions::default()).unwrap();
    let findings = review(&scan, &ReviewOptions { home: root.clone(), min_bytes: 0, ..ReviewOptions::default() });
    let map = Atlas::new(&scan, &findings).map(&scan, None, 800.0, 600.0, 4);

    let tile = |rel: &str| {
        map.tiles.iter().find(|t| !t.rest && t.path == root.join(rel)).unwrap_or_else(|| panic!("no tile for {rel}"))
    };
    assert_eq!(tile("code/app").kind, Kind::Code);
    assert_eq!(tile("code/app/.git").kind, Kind::Git);
    assert_eq!(tile("Pictures").kind, Kind::Media);
    let deps = tile("code/app/node_modules");
    assert!(deps.finding);
    assert_eq!(deps.reclaimable, Some(Safety::Regenerable));
    let inside = tile("code/app/node_modules/dep");
    assert!(!inside.finding, "only the finding itself is the finding");
    assert_eq!(inside.reclaimable, Some(Safety::Regenerable));
    assert!(
        map.tiles
            .iter()
            .all(|t| t.x >= 0.0 && t.y >= 0.0 && t.x + t.width <= 800.0 + 1e-6 && t.y + t.height <= 600.0 + 1e-6)
    );
}
