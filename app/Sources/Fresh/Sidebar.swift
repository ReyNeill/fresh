import AppKit
import FreshCore
import SwiftUI

/// App menu, the scan action, the folder, and the finding groups to browse.
struct Sidebar: View {
    @Environment(AppModel.self) private var model
    let chooseFolder: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 1) {
            Menu {
                Button("Choose Folder…", action: chooseFolder)
                Button("Review Home Folder") { Task { await model.choose(URL(filePath: homeFolder())) } }
                Divider()
                Button("Undo Last Clean-Up") { Task { await model.undo() } }
                    .disabled(model.busy)
            } label: {
                HStack(spacing: 5) {
                    Text("Fresh").font(.system(size: 15, weight: .semibold))
                    Image(systemName: "chevron.down")
                        .font(.system(size: 9, weight: .semibold))
                        .foregroundStyle(Palette.tertiaryText)
                }
            }
            .menuStyle(.button)
            .buttonStyle(.plain)
            .menuIndicator(.hidden)
            .fixedSize()
            .padding(.horizontal, 10)
            .padding(.top, 10)
            .padding(.bottom, 12)

            SidebarRow(symbol: "arrow.clockwise", title: "Scan again") { Task { await model.scan() } }
                .disabled(model.isScanning || model.busy)

            SectionLabel("Folder")
            SidebarRow(symbol: "folder", title: model.root.lastPathComponent, action: chooseFolder)
                .help(tilde(model.root.path))

            if model.review != nil {
                SectionLabel("Findings")
                SidebarRow(
                    symbol: "tray.full",
                    title: "Everything",
                    trailing: total,
                    selected: model.group == nil
                ) { model.group = nil }
                ForEach(model.groups) { group in
                    SidebarRow(
                        symbol: group.rule.symbol,
                        title: group.rule.title,
                        trailing: group.total,
                        selected: model.group == group.rule
                    ) { model.group = group.rule }
                }
            }

            Spacer(minLength: 12)
            if let review = model.review, !review.needsFullDiskAccess.isEmpty {
                FullDiskAccessNote()
            }
        }
        .padding(.horizontal, 8)
        .padding(.bottom, 12)
    }

    private var total: String {
        let bytes = outermost(model.review?.findings ?? []).map(\.bytes).reduce(0, +)
        return bytes > 0 ? bytes.formattedBytes : ""
    }
}

private struct SidebarRow: View {
    let symbol: String
    let title: String
    var trailing: String?
    var selected = false
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: 9) {
                Image(systemName: symbol)
                    .font(.system(size: 12))
                    .foregroundStyle(Palette.secondaryText)
                    .frame(width: 16)
                Text(title).lineLimit(1)
                Spacer(minLength: 6)
                if let trailing {
                    Text(trailing).foregroundStyle(Palette.tertiaryText).monospacedDigit()
                }
            }
            .font(.system(size: 13))
            .padding(.horizontal, 10)
            .frame(height: 30)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .rowBackground(selected: selected)
    }
}

private struct SectionLabel: View {
    let title: String

    init(_ title: String) {
        self.title = title
    }

    var body: some View {
        Text(title)
            .font(.system(size: 12))
            .foregroundStyle(Palette.tertiaryText)
            .padding(.horizontal, 10)
            .padding(.top, 16)
            .padding(.bottom, 5)
    }
}

/// Shown when other apps' containers were left out of the scan.
private struct FullDiskAccessNote: View {
    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Label("Other apps' data isn't included", systemImage: "lock")
                .font(.system(size: 12))
                .foregroundStyle(Palette.secondaryText)
            Button("Allow Full Disk Access") {
                NSWorkspace.shared.open(
                    URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles")!
                )
            }
            .buttonStyle(ChipButtonStyle(active: true))
        }
        .padding(12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .card()
    }
}
