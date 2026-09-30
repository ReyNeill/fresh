import AppKit
import FreshCore
import SwiftUI

/// Findings of one rule, in the order the review sorted them.
struct FindingGroup: Identifiable {
    let rule: Rule
    var findings: [Finding]
    var id: Rule { rule }
}

struct ReviewList: View {
    let findings: [Finding]

    var body: some View {
        List {
            ForEach(groups) { group in
                Section {
                    ForEach(group.findings) { FindingRow(finding: $0) }
                } header: {
                    GroupHeader(group: group)
                }
            }
        }
        .listStyle(.inset)
    }

    private var groups: [FindingGroup] {
        var groups: [FindingGroup] = []
        for finding in findings {
            if groups.last?.rule == finding.rule {
                groups[groups.count - 1].findings.append(finding)
            } else {
                groups.append(FindingGroup(rule: finding.rule, findings: [finding]))
            }
        }
        return groups
    }
}

struct GroupHeader: View {
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
        HStack {
            Toggle(isOn: all) { Label(group.rule.title, systemImage: group.rule.symbol) }
                .toggleStyle(.checkbox)
                .disabled(ids.isEmpty)
            Spacer()
            Text(total).monospacedDigit()
        }
    }

    private var total: String {
        let bytes = outermost(group.findings).map(\.bytes).reduce(0, +)
        let count = plural(group.findings.count, "item", "items")
        return bytes > 0 ? "\(bytes.formattedBytes) · \(count)" : count
    }
}

struct FindingRow: View {
    @Environment(AppModel.self) private var model
    let finding: Finding

    var body: some View {
        let selected = Binding(
            get: { model.selection.contains(finding.id) },
            set: { on in
                if on { model.selection.insert(finding.id) } else { model.selection.remove(finding.id) }
            }
        )
        HStack(spacing: 10) {
            Toggle(isOn: selected) { EmptyView() }
                .toggleStyle(.checkbox)
                .labelsHidden()
                .disabled(!finding.canApply)
            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 6) {
                    Text(finding.title).lineLimit(1).layoutPriority(1)
                    Text(finding.place).foregroundStyle(.tertiary).lineLimit(1).truncationMode(.middle)
                }
                Text(finding.reason).font(.callout).foregroundStyle(.secondary).lineLimit(1)
            }
            Spacer(minLength: 12)
            SafetyTag(safety: finding.safety)
            Text(finding.bytes > 0 ? finding.bytes.formattedBytes : "")
                .monospacedDigit()
                .foregroundStyle(.secondary)
                .frame(width: 76, alignment: .trailing)
        }
        .padding(.vertical, 2)
        .help(finding.detail)
        .contextMenu {
            Button("Reveal in Finder") {
                NSWorkspace.shared.activateFileViewerSelecting([URL(filePath: finding.path)])
            }
        }
    }
}

struct SafetyTag: View {
    let safety: Safety

    var body: some View {
        Text(safety.label)
            .font(.caption)
            .foregroundStyle(safety.color)
            .padding(.horizontal, 6)
            .padding(.vertical, 1)
            .background(safety.color.opacity(0.12), in: Capsule())
    }
}
