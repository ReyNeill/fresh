import AppKit
import FreshCore
import SwiftUI

/// The scanned folder as a treemap: area is size, color is what the data is, hatching is
/// what a finding can free. Click a folder to look inside; the trail above leads back out.
struct SpaceMapView: View {
    @Environment(AppModel.self) private var model
    let review: Review
    @State private var hovered: Tile?

    var body: some View {
        let tiles = model.spaceMap?.tiles ?? []
        VStack(alignment: .leading, spacing: 12) {
            HStack(alignment: .firstTextBaseline, spacing: 16) {
                Trail(root: review.root, trail: model.spaceMap?.trail ?? [])
                Spacer(minLength: 12)
                Legend(kinds: Set(tiles.map(\.kind)))
            }
            GeometryReader { geometry in
                MapCanvas(tiles: tiles, hovered: hovered)
                    .contentShape(Rectangle())
                    .onContinuousHover { phase in
                        if case .active(let point) = phase { hovered = tile(at: point, in: tiles) } else { hovered = nil }
                    }
                    .onTapGesture { point in
                        if let tile = tile(at: point, in: tiles), tile.isDir { model.zoom(to: tile.node) }
                    }
                    .contextMenu { menu }
                    .onChange(of: geometry.size, initial: true) { model.layOutMap(in: geometry.size) }
            }
            HoverLine(tile: hovered)
            VStack(spacing: 8) {
                if model.mapIsStale { StaleChip() }
                if let notice = model.notice { NoticeChip(notice: notice) }
                if !review.findings.isEmpty { Composer() }
            }
            .frame(maxWidth: MainPanel.columnWidth)
            .frame(maxWidth: .infinity)
        }
        .padding(.horizontal, 20)
        .padding(.top, 18)
        .padding(.bottom, 18)
    }

    /// The deepest tile under a point: children come after their parents.
    private func tile(at point: CGPoint, in tiles: [Tile]) -> Tile? {
        tiles.last { $0.rect.contains(point) }
    }

    @ViewBuilder private var menu: some View {
        if let tile = hovered {
            Button("Reveal in Finder") {
                NSWorkspace.shared.activateFileViewerSelecting([URL(filePath: tile.path)])
            }
            if tile.finding, let finding = model.finding(covering: tile.path), finding.canApply {
                let selected = model.selection.contains(finding.id)
                Button(selected ? "Don't Clean Up" : "Select for Clean-Up") {
                    if selected { model.selection.remove(finding.id) } else { model.selection.insert(finding.id) }
                }
            }
        }
    }
}

private struct MapCanvas: View {
    let tiles: [Tile]
    let hovered: Tile?

    var body: some View {
        Canvas { context, _ in
            for tile in tiles {
                let rect = tile.rect.insetBy(dx: 0.5, dy: 0.5)
                guard rect.width > 0, rect.height > 0 else { continue }
                let radius = min(5, rect.width / 4, rect.height / 4)
                context.fill(Path(roundedRect: rect, cornerRadius: radius), with: .color(tile.kind.color.opacity(fill(tile.depth))))
                if rect.width >= 48, rect.height >= 18 { label(tile, in: rect, on: context) }
            }
            // Findings go on top, hatched in their safety color, children included.
            for tile in tiles where tile.finding {
                hatch(tile, on: context)
            }
            if let hovered {
                let rect = hovered.rect.insetBy(dx: 0.75, dy: 0.75)
                context.stroke(Path(roundedRect: rect, cornerRadius: min(5, rect.width / 4, rect.height / 4)), with: .color(Palette.text), lineWidth: 1.5)
            }
        }
    }

    /// Deeper tiles are paler, so nested folders read as boxes inside boxes.
    private func fill(_ depth: UInt32) -> Double {
        [1, 0.72, 0.55, 0.42][min(Int(depth), 4) - 1]
    }

    private func label(_ tile: Tile, in rect: CGRect, on context: GraphicsContext) {
        var layer = context
        layer.clip(to: Path(rect.insetBy(dx: 5, dy: 2)))
        let text = Text(tile.name).font(.system(size: 11, weight: tile.depth == 1 ? .semibold : .medium))
            .foregroundStyle(Palette.text.opacity(0.85))
            + Text("  \(tile.bytes.formattedBytes)").font(.system(size: 11)).foregroundStyle(Palette.text.opacity(0.55))
        layer.draw(text, at: CGPoint(x: rect.minX + 6, y: rect.minY + 3), anchor: .topLeading)
    }

    private func hatch(_ tile: Tile, on context: GraphicsContext) {
        guard let safety = tile.reclaimable else { return }
        let rect = tile.rect.insetBy(dx: 0.75, dy: 0.75)
        let outline = Path(roundedRect: rect, cornerRadius: min(5, rect.width / 4, rect.height / 4))
        var layer = context
        layer.clip(to: outline)
        var lines = Path()
        var x = rect.minX - rect.height
        while x < rect.maxX {
            lines.move(to: CGPoint(x: x, y: rect.maxY))
            lines.addLine(to: CGPoint(x: x + rect.height, y: rect.minY))
            x += 7
        }
        layer.stroke(lines, with: .color(safety.color.opacity(0.4)), lineWidth: 1)
        context.stroke(outline, with: .color(safety.color), lineWidth: 1.5)
    }
}

/// Where on the map you are, from the scanned folder down; each step zooms back out.
private struct Trail: View {
    @Environment(AppModel.self) private var model
    let root: String
    let trail: [Crumb]

    var body: some View {
        HStack(spacing: 5) {
            ForEach(Array(trail.enumerated()), id: \.element.node) { index, crumb in
                if index > 0 { Text("/").foregroundStyle(Palette.tertiaryText) }
                let current = index == trail.count - 1
                Button(index == 0 ? folderName(root) : crumb.name) { model.zoom(to: index == 0 ? nil : crumb.node) }
                    .buttonStyle(.plain)
                    .font(.system(size: 13, weight: current ? .semibold : .regular))
                    .foregroundStyle(current ? Palette.text : Palette.secondaryText)
                    .disabled(current)
            }
        }
        .lineLimit(1)
    }
}

private struct Legend: View {
    let kinds: Set<Kind>

    var body: some View {
        HStack(spacing: 12) {
            ForEach(Kind.all.filter(kinds.contains), id: \.self) { kind in
                HStack(spacing: 5) {
                    RoundedRectangle(cornerRadius: 2).fill(kind.color).frame(width: 9, height: 9)
                    Text(kind.title)
                }
            }
            HStack(spacing: 5) {
                RoundedRectangle(cornerRadius: 2).strokeBorder(Palette.green, lineWidth: 1.5).frame(width: 9, height: 9)
                Text("Can go")
            }
        }
        .font(.system(size: 12))
        .foregroundStyle(Palette.tertiaryText)
    }
}

/// What's under the pointer: name, size, where, and the finding it belongs to.
private struct HoverLine: View {
    @Environment(AppModel.self) private var model
    let tile: Tile?

    var body: some View {
        HStack(spacing: 8) {
            if let tile {
                RoundedRectangle(cornerRadius: 2).fill(tile.kind.color).frame(width: 9, height: 9)
                Text(tile.name).fontWeight(.medium)
                Text(tile.bytes.formattedBytes).foregroundStyle(Palette.secondaryText).monospacedDigit()
                Text(tilde(tile.rest ? tile.path : URL(filePath: tile.path).deletingLastPathComponent().path))
                    .foregroundStyle(Palette.tertiaryText)
                    .lineLimit(1)
                    .truncationMode(.middle)
                Spacer(minLength: 12)
                if let safety = tile.reclaimable, let finding = model.finding(covering: tile.path) {
                    Circle().fill(safety.color).frame(width: 6, height: 6)
                    Text(tile.finding ? finding.reason : "Inside \(finding.title), which can go")
                        .foregroundStyle(Palette.secondaryText)
                        .lineLimit(1)
                }
            } else {
                Text("Click a folder to look inside. Outlined, hatched areas can go.")
                    .foregroundStyle(Palette.tertiaryText)
            }
        }
        .font(.system(size: 12))
        .frame(height: 18)
    }
}

private struct StaleChip: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: "clock.arrow.circlepath").foregroundStyle(Palette.tertiaryText)
            Text("The map is from before your clean-up.").foregroundStyle(Palette.secondaryText)
            Spacer()
            Button("Scan again") { Task { await model.scan() } }
                .buttonStyle(ChipButtonStyle(active: true))
        }
        .font(.system(size: 12))
        .padding(.leading, 12)
        .padding(.trailing, 6)
        .padding(.vertical, 6)
        .card()
    }
}
