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

Meaning colors, used only as small dots and icons:

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
  group and folder above the main column.
- **Sidebar** (244pt, on `window`): the "Fresh ▾" menu, "Scan again", the folder, then the
  finding groups with their totals. The selected group gets the `selected` highlight.
- **Main card** (`surface`, rounded top-left corner): one column, at most 700pt wide and
  centered. It opens with the folder at a glance (size, files, what's reclaimable by safety),
  then each group's rows. Rows fade out under the composer.
- **Composer**: floats at the bottom of the column. Its first line says in plain words what
  Clean up will do; below are the "Check remotes" chip, the selection count, and the one
  primary button. The status of the last clean-up or undo sits above it as a chip, with Undo.

## Components

- **Finding row**: checkbox, then the name (13 medium) with its location (tertiary, truncated
  in the middle), and a second line with the reason, idle time and a safety dot. The size is
  right-aligned. The whole row toggles; right-click reveals it in Finder.
- **Checkbox**: `.monochrome`: outlined when off, filled `primary` with a check when on.
- **Primary button**: `PrimaryButtonStyle`, a filled capsule. One per screen.
- **Chips**: `ChipButtonStyle`, gray text that picks up a soft background on hover or when on.

## Motion and progress

No spinners, shimmers, pulses or any other animation that repaints continuously; they peg
the GPU on high-refresh displays. Progress is a number that changes a few times a second
("1,234,567 items so far"). Hover highlights switch instantly.

## Copy

Plain, specific sentences in sentence case: "Move 3 items (147 MB) to the Trash and delete 2
branches." Say what happens and how to get it back. Never promise space is free before the
Trash is emptied.
