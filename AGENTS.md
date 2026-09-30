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
  triggers macOS privacy prompts. Point it at a scratch folder instead with
  `defaults write io.silixon.fresh root -string <path>`.
- A rule fix ships with a test that fails without it.
<!-- END:safety-rules -->
