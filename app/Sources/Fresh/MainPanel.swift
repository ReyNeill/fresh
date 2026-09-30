import AppKit
import FreshCore
import SwiftUI

/// One centered column, like a ChatGPT conversation: the folder at a glance, then each
/// group's findings, with the composer floating over the bottom.
struct MainPanel: View {
    static let columnWidth: CGFloat = 700

    @Environment(AppModel.self) private var model

    var body: some View {
        switch model.phase {
        case .scanning(let entries):
            VStack(spacing: 4) {
                Text("Scanning \(tilde(model.root.path))")
                    .font(.system(size: 13, weight: .medium))
                    .foregroundStyle(Palette.secondaryText)
                Text("\(entries.formatted()) items so far")
                    .font(.system(size: 12))
                    .monospacedDigit()
                    .foregroundStyle(Palette.tertiaryText)
            }
        case .failed(let message):
            EmptyState(symbol: "exclamationmark.triangle", title: "Couldn't scan", message: message)
        case .reviewed(let review) where model.showingMap:
            SpaceMapView(review: review)
        case .reviewed(let review):
            ZStack(alignment: .bottom) {
                if review.findings.isEmpty {
                    EmptyState(
                        symbol: "sparkles",
                        title: "All fresh",
                        message: "Nothing in \(tilde(review.root)) is worth cleaning up."
                    )
                } else {
                    ScrollView {
                        VStack(alignment: .leading, spacing: 0) {
                            Overview(review: review)
                            ForEach(model.visibleGroups) { GroupSection(group: $0) }
                        }
                        .frame(maxWidth: Self.columnWidth)
                        .padding(.horizontal, 24)
                        .padding(.top, 30)
                        .padding(.bottom, 180)
                        .frame(maxWidth: .infinity)
                    }
                    // Rows fade out under the composer instead of being cut off.
                    LinearGradient(colors: [Palette.surface.opacity(0), Palette.surface], startPoint: .top, endPoint: .bottom)
                        .frame(height: 130)
                        .allowsHitTesting(false)
                }
                VStack(spacing: 8) {
                    if let notice = model.notice { NoticeChip(notice: notice) }
                    if !review.findings.isEmpty { Composer() }
                }
                .frame(maxWidth: Self.columnWidth)
                .padding(.horizontal, 24)
                .padding(.bottom, 18)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
    }
}

/// A quiet centered message for when there's nothing to list.
private struct EmptyState: View {
    let symbol: String
    let title: String
    let message: String

    var body: some View {
        VStack(spacing: 8) {
            Image(systemName: symbol).font(.system(size: 22)).foregroundStyle(Palette.tertiaryText)
            Text(title).font(.system(size: 15, weight: .semibold))
            Text(message)
                .font(.system(size: 13))
                .foregroundStyle(Palette.secondaryText)
                .multilineTextAlignment(.center)
                .frame(maxWidth: 420)
        }
        .padding(24)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

/// The folder at a glance: its size, and what could go by how safe it is to let go.
private struct Overview: View {
    let review: Review

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(folderName(review.root)).font(.system(size: 22, weight: .semibold))
            Text(
                "\(review.bytes.formattedBytes) in \(review.files.formatted()) files, scanned in \(review.seconds.formatted(.number.precision(.fractionLength(1)))) s"
            )
            .font(.system(size: 13))
            .foregroundStyle(Palette.secondaryText)
            HStack(spacing: 16) {
                ForEach(totals, id: \.safety) { total in
                    HStack(spacing: 5) {
                        Circle().fill(total.safety.color).frame(width: 6, height: 6)
                        Text("\(total.bytes.formattedBytes) \(total.safety.phrase)")
                    }
                }
            }
            .font(.system(size: 12))
            .foregroundStyle(Palette.secondaryText)
            .padding(.top, 2)
        }
        .padding(.horizontal, 10)
        .padding(.bottom, 4)
    }

    private var totals: [(safety: Safety, bytes: UInt64)] {
        let freeing = outermost(review.findings)
        return [Safety.regenerable, .reversible, .review, .remote].compactMap { safety in
            let bytes = freeing.filter { $0.safety == safety }.map(\.bytes).reduce(0, +)
            return bytes > 0 ? (safety, bytes) : nil
        }
    }
}

private struct GroupSection: View {
    @Environment(AppModel.self) private var model
    let group: FindingGroup

    var body: some View {
        let ids = group.findings.filter(\.canApply).map(\.id)
        let all = Binding(
            get: { !ids.isEmpty && ids.allSatisfy(model.selection.contains) },
            set: { on in
                if on { model.selection.formUnion(ids) } else { model.selection.subtract(ids) }
            }
        )
        VStack(alignment: .leading, spacing: 2) {
            HStack(spacing: 8) {
                Toggle(isOn: all) {
                    Text(group.rule.title)
                        .font(.system(size: 12, weight: .medium))
                        .foregroundStyle(Palette.secondaryText)
                }
                .toggleStyle(.monochrome)
                .disabled(ids.isEmpty)
                Spacer()
                Text(summary)
                    .font(.system(size: 12))
                    .foregroundStyle(Palette.tertiaryText)
                    .monospacedDigit()
            }
            .padding(.horizontal, 10)
            .padding(.top, 26)
            .padding(.bottom, 6)
            ForEach(group.findings) { FindingRow(finding: $0) }
        }
    }

    private var summary: String {
        let count = plural(group.findings.count, "item", "items")
        return group.bytes > 0 ? "\(group.bytes.formattedBytes) · \(count)" : count
    }
}

private struct FindingRow: View {
    @Environment(AppModel.self) private var model
    let finding: Finding

    var body: some View {
        let selected = Binding(
            get: { model.selection.contains(finding.id) },
            set: { on in
                if on { model.selection.insert(finding.id) } else { model.selection.remove(finding.id) }
            }
        )
        HStack(alignment: .top, spacing: 12) {
            Toggle(isOn: selected) { EmptyView() }
                .toggleStyle(.monochrome)
                .labelsHidden()
                .disabled(!finding.canApply)
                .padding(.top, 1)
            VStack(alignment: .leading, spacing: 3) {
                HStack(spacing: 6) {
                    Text(finding.title).font(.system(size: 13, weight: .medium)).lineLimit(1).layoutPriority(1)
                    Text(finding.place)
                        .font(.system(size: 13))
                        .foregroundStyle(Palette.tertiaryText)
                        .lineLimit(1)
                        .truncationMode(.middle)
                }
                HStack(spacing: 8) {
                    Text(finding.reason).foregroundStyle(Palette.secondaryText).lineLimit(1)
                    HStack(spacing: 4) {
                        Circle().fill(finding.safety.color).frame(width: 6, height: 6)
                        Text(finding.safety.label).foregroundStyle(Palette.tertiaryText)
                    }
                    .fixedSize()
                }
                .font(.system(size: 12))
            }
            Spacer(minLength: 16)
            Text(finding.bytes > 0 ? finding.bytes.formattedBytes : "")
                .font(.system(size: 13))
                .foregroundStyle(Palette.secondaryText)
                .monospacedDigit()
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 9)
        .contentShape(Rectangle())
        .onTapGesture { if finding.canApply { selected.wrappedValue.toggle() } }
        .rowBackground()
        .help(finding.detail)
        .contextMenu {
            Button("Reveal in Finder") {
                NSWorkspace.shared.activateFileViewerSelecting([URL(filePath: finding.path)])
            }
        }
    }
}
