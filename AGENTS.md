# Fresh

A native macOS app and CLI that get a Mac back to fresh: find what's taking space and
clutter, let a person review it, and clean it up reversibly. The Rust core (`core/`) does the
work; the CLI (`cli/`) and the SwiftUI app (`app/`, through UniFFI) are its two frontends.

<!-- BEGIN:verification-rules -->
# Verification before finishing

After making code changes, **always run `bun run c` before ending your turn** and reporting
back. It runs rustfmt, clippy with warnings as errors, and the Rust tests, then builds the
app, which regenerates the Swift bindings and compiles the SwiftUI code against them.

Fix every error and warning it reports. Do not hand off work with failing checks unless the
user explicitly asked you to stop early.
<!-- END:verification-rules -->

<!-- BEGIN:safety-rules -->
# Safety

Fresh deletes things for people, so:
- Nothing changes outside `apply`. Every action moves to the Trash or is journaled with its
  undo; never delete outright.
- Every finding is re-checked when applied, against the disk as it is then.
- "Regenerable" means it comes back on its own. Check new rules against tool installs (home
  dotfolders, editor extensions, app bundles) and caches of running apps.
- Don't scan a real home folder from the app without warning the user: a new app bundle
  triggers macOS privacy prompts. Build a copy elsewhere (`FRESH_BUNDLE=<dir>/Fresh.app
  app/build.sh`) and point it at a scratch folder: `open -n <dir>/Fresh.app --args -root <path>`.
- A rule fix ships with a test that fails without it.
- Releases are public and reach everyone's installed copy within a day. Only run
  `bun run release` when asked.
<!-- END:safety-rules -->

<!-- BEGIN:design-rules -->
# Design

Read `DESIGN.md` before writing any UI code. Key points:
- Modeled on ChatGPT's desktop app: a flat gray sidebar, one centered column on a white
  card, and a floating composer holding the one action.
- Colors come from `Palette` in `Theme.swift`. Color carries meaning (safety, outcomes),
  never decoration. Don't hard-code colors in views.
- Monochrome controls: the primary button and checked checkboxes are black in light mode and
  near-white in dark mode.
- No spinners or other continuously repainting animations; show progress as numbers.
<!-- END:design-rules -->
