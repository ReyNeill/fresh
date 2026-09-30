import FreshCore
import SwiftUI

/// Preferences (⌘,): what reviews never suggest, and a way to bring each one back.
struct SettingsView: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            VStack(alignment: .leading, spacing: 4) {
                Text("Excluded").font(.system(size: 15, weight: .semibold))
                Text("Fresh never suggests these, or anything inside them. To exclude an app's cache, right-click it under Caches.")
                    .font(.system(size: 12))
                    .foregroundStyle(Palette.secondaryText)
                    .fixedSize(horizontal: false, vertical: true)
            }
            VStack(spacing: 0) {
                if model.excluded.isEmpty {
                    Text("Nothing excluded yet.")
                        .font(.system(size: 13))
                        .foregroundStyle(Palette.tertiaryText)
                        .frame(maxWidth: .infinity, minHeight: 64)
                } else {
                    ForEach(model.excluded, id: \.self) { ExcludedRow(path: $0) }
                }
            }
            .padding(6)
            .card()
            if let error = model.settingsError {
                Text(error).font(.system(size: 12)).foregroundStyle(Palette.red)
            }
            Text("Removing one brings it back from the next scan.")
                .font(.system(size: 12))
                .foregroundStyle(Palette.tertiaryText)
        }
        .padding(20)
        .frame(width: 520)
        .frame(minHeight: 260, alignment: .top)
        .background(Palette.surface)
        .foregroundStyle(Palette.text)
        .onAppear { model.loadExcluded() }
    }
}

private struct ExcludedRow: View {
    @Environment(AppModel.self) private var model
    let path: String

    var body: some View {
        HStack(spacing: 10) {
            Image(systemName: "folder")
                .font(.system(size: 12))
                .foregroundStyle(Palette.secondaryText)
                .frame(width: 16)
            VStack(alignment: .leading, spacing: 2) {
                Text(URL(filePath: path).lastPathComponent).font(.system(size: 13, weight: .medium))
                Text(tilde(URL(filePath: path).deletingLastPathComponent().path))
                    .font(.system(size: 12))
                    .foregroundStyle(Palette.tertiaryText)
                    .lineLimit(1)
                    .truncationMode(.middle)
            }
            Spacer(minLength: 12)
            Button("Remove") { model.stopExcluding(path) }
                .buttonStyle(ChipButtonStyle())
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 8)
        .rowBackground()
    }
}
