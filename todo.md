# TODO

- [ ] Treemap of the scan, with the review list beside it.
- [ ] Clone-aware sizes: APFS clones share blocks (Bun and uv caches clone into projects),
      so "reclaimable" overstates them. Use the private-size attribute.
- [ ] Duplicates: group by size, then partial and full BLAKE3 hashes; replace copies with
      APFS clones instead of deleting them.
- [ ] Incremental rescans from FSEvents event ids instead of full scans.
- [ ] Developer ID signing and notarization, so Full Disk Access survives rebuilds; Homebrew
      cask and Sparkle updates.
- [ ] Match caches to running apps by bundle id through NSWorkspace instead of the process
      name heuristic.
- [ ] `simctl delete unavailable` can't be undone: at least journal the removed devices.
