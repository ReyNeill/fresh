# TODO

- [x] Treemap of the scan (the space map).
- [ ] Space map: keyboard navigation (arrows to move, Return to zoom, Escape to go up).
- [x] Clone-aware sizes: findings report what they free alone, and joint savings count data
      shared by clones and hard links once every copy is included.
- [ ] Clone-aware sizes miss blocks a partly edited clone still shares with its original:
      deleting every full clone of that original can then overstate what's freed.
- [ ] Duplicates: group by size, then partial and full BLAKE3 hashes; replace copies with
      APFS clones instead of deleting them.
- [ ] Incremental rescans from FSEvents event ids instead of full scans.
- [x] Stable signing (Apple Development), so Full Disk Access survives rebuilds.
- [ ] Developer ID Application certificate (account holder creates it), then notarize with
      `bun run notarize`; after that, a Homebrew cask and Sparkle updates.
- [ ] Match caches to running apps by bundle id through NSWorkspace instead of the process
      name heuristic.
- [ ] `simctl delete unavailable` can't be undone: at least journal the removed devices.
