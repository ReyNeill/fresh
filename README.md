# fresh

Get your Mac back to fresh. `fresh` finds what's taking space and clutter (build output, caches,
Xcode leftovers, old installers, finished git worktrees and branches), lets you review it, and
cleans it up reversibly.

## Install

Download the DMG from the [latest release](https://github.com/ReyNeill/fresh/releases/latest),
open it, and drag Fresh to Applications. Fresh isn't notarized, so macOS blocks the first launch
once: click Done, then System Settings ▸ Privacy & Security ▸ Open Anyway. After that it updates
itself (Fresh ▸ Check for Updates…). Needs macOS 15 or later.

## The command line

```bash
cargo build --release
./target/release/fresh scan                 # where the space goes
./target/release/fresh review               # what can go, grouped and sized
./target/release/fresh review --fetch -o plan.json
./target/release/fresh apply plan.json      # after deleting the findings you want to keep
./target/release/fresh undo                 # put the last apply back
```

## Safety model

- Nothing changes outside `apply`, and `apply` only does what's in the plan you reviewed.
- Every finding is re-checked when applied: paths must still exist, branches must still point at
  the reviewed commit, worktrees must still be clean. Anything that changed is skipped.
- Files go to the Trash; nothing is deleted outright. Removed worktrees carry git's record of
  them into the Trash, so undo restores a working checkout. Deleted branches are recreated from
  the journal.
- `~/Library/Application Support/fresh/journal.jsonl` records every action and how to undo it.
- Remote branch deletions need `--remote` and push with `--force-with-lease` against the
  reviewed commit. They're only suggested after `--fetch`, since stale tracking refs linger.
- Sizes are honest about APFS clones and hard links. A finding's size is what it takes up;
  what it frees is less when copies elsewhere share its data (Bun and uv clone their caches
  into projects). Shared data only counts as freed when every copy is among what you clean
  up together, and the totals say exactly that.
- Worktrees a running process works in are never suggested, and detached checkouts (how
  agents and bisects work) are only offered for review, never pre-selected.
- Scans never download iCloud placeholders.
- Without Full Disk Access, other apps' containers are skipped: each protected folder there
  stalls for seconds in the privacy daemon before being denied.

## The app

```bash
./app/build.sh            # Rust static library → Swift bindings → app/build/Fresh.app
open app/build/Fresh.app
```

The app reviews your Mac: everything in your home folder, plus simulator runtimes you haven't
used in a month and extra copies of Xcode. It pre-selects what's regenerable or reversible,
and cleans up after a confirmation. Its space map shows the whole folder as a
treemap, colored by kind of data, with what can go hatched. Right-click an app's cache to
exclude it; Settings (⌘,) lists everything excluded, and the CLI honors the same list. Undo sits in the status chip after a
clean-up and in the Fresh menu. The app and the CLI share one journal, but macOS lets only the
process that moved something to the Trash, or one with Full Disk Access, move it back out.
Give the app Full Disk Access to include other apps' data too.

`build.sh` signs with the most durable identity in your keychain: a Developer ID if there is
one, else an Apple Development identity, which keeps privacy grants like Full Disk Access
across rebuilds; CI falls back to ad-hoc signing. To hand the app to other Macs, add a
Developer ID Application certificate, store notary credentials once with
`xcrun notarytool store-credentials fresh-notary`, then run `bun run notarize` after
building.

## Releasing

Bump `CFBundleShortVersionString` in `app/Info.plist`, merge to `main`, then:

```bash
bun run release   # builds the DMG, signs the update, publishes GitHub release v<version>
```

The release carries a universal (Apple Silicon and Intel) DMG and `appcast.xml`, the feed the
app checks daily through `releases/latest`. Updates are signed with an EdDSA key that lives in
the release machine's Keychain (made once with Sparkle's `generate_keys`), and the app only
installs updates signed with it and with the same certificate, so the Full Disk Access people
grant carries over. Back the key up with `generate_keys -x <file>` somewhere safe: without it,
installed copies can't update. `bun run package` builds the DMG alone.

The icon is drawn from `app/Icon.svg`, else `app/Icon.png`: square art edge to edge, with no
rounded corners, margins or shadow. The build applies the macOS shape, grid and sizes.

## Layout

- `core/`: scanner (`getattrlistbulk`), rules, git checks, apply and undo journal; the `ffi`
  feature adds the UniFFI API the app uses
- `cli/`: the `fresh` command
- `ffi/`: links the core into the static library the app builds against
- `uniffi-bindgen/`: generates the Swift bindings
- `app/`: the SwiftUI app

## Development

`cargo test`. The apply test moves a temporary worktree through the real Trash and back.
