# Design

Fresh reviews what can go on a Mac and cleans it up. The window is modeled on ChatGPT's
desktop app: a flat gray sidebar, one white card holding a single centered column, and a
floating composer at the bottom for the one action. It is quiet and near-monochrome. Color
only carries meaning: how safe a finding is, and how a clean-up went.

## Color

Tokens live in `Palette` (`app/Sources/Fresh/Theme.swift`). Views never hard-code colors.
Every token has a light and a dark value, and the app follows the system appearance.

| token           | light              | dark               | usage                          |
| --------------- | ------------------ | ------------------ | ------------------------------ |
| `window`        | `#F9F9F9`          | `#181818`          | window, title bar, sidebar     |
| `surface`       | `#FFFFFF`          | `#212121`          | main card, composer, chips     |
| `text`          | `#0D0D0D`          | `#ECECEC`          | primary text                   |
| `secondaryText` | `#5D5D5D`          | `#B4B4B4`          | reasons, sizes, labels         |
| `tertiaryText`  | `#8F8F8F`          | `#8A8A8A`          | paths, section labels, counts  |
| `border`        | black 8%           | white 10%          | hairlines around cards         |
| `hover`         | black 4%           | white 5%           | row hover                      |
| `selected`      | black 7%           | white 9%           | selected sidebar row, chips on |
| `primary`       | `#0D0D0D`          | `#ECECEC`          | primary button, checked boxes  |
| `onPrimary`     | `#FFFFFF`          | `#0D0D0D`          | text and marks on `primary`    |

Space map fills, muted so labels stay readable and hatching shows through. Deeper tiles are
drawn paler (100%, 72%, 55%, 42% opacity by depth) so nested folders read as boxes in boxes.

| token          | light     | dark      | kind        |
| -------------- | --------- | --------- | ----------- |
| `mapCode`      | `#8FA9D6` | `#4A6A9E` | projects and repositories |
| `mapGit`       | `#D99A9A` | `#8C4A4A` | `.git` folders |
| `mapCache`     | `#E2C58A` | `#8C7340` | caches      |
| `mapToolchain` | `#96C7A4` | `#4F805C` | toolchains and SDKs |
| `mapSynced`    | `#8FCACF` | `#46807F` | iCloud and cloud drives |
| `mapMedia`     | `#B9A0DA` | `#6E579A` | photos, video, music |
| `mapDocuments` | `#CBC6BC` | `#6F6A60` | Documents, Desktop, Downloads |
| `mapApps`      | `#AAB6C4` | `#5A6675` | app data    |
| `mapOther`     | `#D7D7D7` | `#4A4A4A` | everything else |

Meaning colors, used only as small dots, icons and map outlines:

| token   | light     | dark      | means                                |
| ------- | --------- | --------- | ------------------------------------ |
| `green` | `#10A37F` | `#19C37D` | regenerable; a clean-up that went fine |
| `blue`  | `#2F6FEB` | `#6E9CF2` | reversible                           |
| `amber` | `#C27A00` | `#E8A33D` | to review; a clean-up with problems  |
| `red`   | `#D93A3A` | `#EF6B6B` | changes a remote                     |

## Typography

The system font throughout; weight does the work, not size.

- 22 semibold: the folder name at the top of the column.
- 15 semibold: the "Fresh" menu in the sidebar, empty-state titles.
- 13 regular or medium: rows, sidebar items, the composer sentence, the title bar.
- 12 regular: reasons, section labels, chips, counts.

Sizes and counts use monospaced digits so columns don't jitter as numbers change.

## Shape

- Rows and sidebar items: 8pt radius highlights, no dividers between rows.
- Cards (main panel corner, chips, the Full Disk Access note): 12pt radius with a hairline
  border and a soft shadow (`.card()`).
- Composer: 20pt radius. Buttons and chips are capsules.

## Layout

- **Title bar**: hidden. A slim row shares the window controls' line and shows the shown
  group above the main column.
- **Sidebar** (244pt, on `window`): the "Fresh ▾" menu, "Scan again", the space map, then the
  finding groups with their totals. There is no folder to pick: Fresh reviews the Mac. The selected group gets the `selected` highlight.
- **Main card** (`surface`, rounded top-left corner): one column, at most 700pt wide and
  centered. It opens with "Your Mac" at a glance (the home folder's size and files, and
  what's reclaimable by safety),
  then each group's rows. Rows fade out under the composer.
- **Space map** (the sidebar's "Map"): the treemap takes the whole card. A trail of folders
  sits top left (each step zooms back out), the kind legend top right, a line under the map
  describes what's under the pointer, and the composer sits below. Clicking a folder zooms
  into it. Findings are hatched and outlined in their safety color, over their children.
  After a clean-up, a chip says the map is stale until the next scan.
- **Composer**: floats at the bottom of the column. Its first line says in plain words what
  Clean up will do; below are the "Check remotes" chip, the selection count, and the one
  primary button. The status of the last clean-up or undo sits above it as a chip, with Undo.

- **Settings** (⌘,, or "Settings…" in the Fresh menu): one pane on `surface`. A 15 semibold
  title, one line of explanation, the list in a card (folder icon, name, location, a Remove
  chip), and a footnote on when changes apply.

## Components

- **Finding row**: checkbox, then the name (13 medium) with its location (tertiary, truncated
  in the middle), and a second line with the reason, data shared with copies elsewhere, idle
  time and a safety dot. The right-aligned size is what it takes up; totals (group headers,
  the overview, the composer) are what cleaning up actually frees. The whole row toggles; right-click reveals it in Finder.
- **Checkbox**: `.monochrome`: outlined when off, filled `primary` with a check when on.
- **Primary button**: `PrimaryButtonStyle`, a filled capsule. One per screen.
- **Chips**: `ChipButtonStyle`, gray text that picks up a soft background on hover or when on.

## App icon

`app/Icon.svg` is square, full-bleed art: no rounded corners, margins or shadow of its own.
`app/icon.swift` clips it to Apple's continuous-corner rounded square on the 1024 grid (824
points, 100 points of margin) and adds the system's soft shadow, so it sits with other Mac
icons and macOS 26 doesn't put it in a gray container.

## Motion and progress

No spinners, shimmers, pulses or any other animation that repaints continuously; they peg
the GPU on high-refresh displays. Progress is a number that changes a few times a second
("1,234,567 items so far"). Hover highlights switch instantly.

## Copy

Plain, specific sentences in sentence case: "Move 3 items (147 MB) to the Trash and delete 2
branches." Say what happens and how to get it back. Never promise space is free before the
Trash is emptied.
