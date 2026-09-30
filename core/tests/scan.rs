use std::fs;
use std::os::unix::fs::MetadataExt;

use fresh_core::{ScanOptions, scan};

#[test]
fn scan_counts_real_disk_usage_once_and_stays_on_the_tree() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    fs::create_dir_all(root.join("a/b")).unwrap();
    fs::write(root.join("a/big.bin"), vec![7u8; 2 << 20]).unwrap();
    fs::write(root.join("a/b/small.txt"), "hi").unwrap();
    fs::hard_link(root.join("a/big.bin"), root.join("a/b/big-link.bin")).unwrap();
    std::os::unix::fs::symlink(root.join("a"), root.join("loop")).unwrap();

    let scan = scan(&root, &ScanOptions::default()).unwrap();

    // Independent oracle: allocated blocks from lstat, the hard-linked inode counted once.
    let blocks = |rel: &str| fs::symlink_metadata(root.join(rel)).unwrap().blocks() * 512;
    assert_eq!(scan.node(0).bytes, blocks("a/big.bin") + blocks("a/b/small.txt") + blocks("loop"));
    assert_eq!(scan.stats.files, 4);

    // Exactly one of the two names carries the big file's bytes.
    let big: Vec<_> = ["a/big.bin", "a/b/big-link.bin"].iter().filter_map(|rel| scan.find(&root.join(rel))).collect();
    assert_eq!(big.len(), 1);
    assert_eq!(scan.node(big[0]).bytes, blocks("a/big.bin"));
    assert!(scan.path(big[0]).starts_with(root.join("a")));

    assert!(scan.find(&root.join("loop/big.bin")).is_none(), "symlinks must not be followed");
}
