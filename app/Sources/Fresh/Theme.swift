import AppKit
import SwiftUI

/// Color tokens from DESIGN.md. Surfaces are near-monochrome, like ChatGPT's desktop app;
/// color is reserved for meaning (safety, outcomes). Every token has a light and dark value.
enum Palette {
    static let window = dynamic(0xF9F9F9, 0x181818)
    static let surface = dynamic(0xFFFFFF, 0x212121)
    static let text = dynamic(0x0D0D0D, 0xECECEC)
    static let secondaryText = dynamic(0x5D5D5D, 0xB4B4B4)
    static let tertiaryText = dynamic(0x8F8F8F, 0x8A8A8A)
    static let border = dynamic(0x000000, 0xFFFFFF, lightAlpha: 0.08, darkAlpha: 0.10)
    static let hover = dynamic(0x000000, 0xFFFFFF, lightAlpha: 0.04, darkAlpha: 0.05)
    static let selected = dynamic(0x000000, 0xFFFFFF, lightAlpha: 0.07, darkAlpha: 0.09)
    /// Primary button and checked checkbox fill; `onPrimary` draws on top of it.
    static let primary = dynamic(0x0D0D0D, 0xECECEC)
    static let onPrimary = dynamic(0xFFFFFF, 0x0D0D0D)
    static let green = dynamic(0x10A37F, 0x19C37D)
    static let blue = dynamic(0x2F6FEB, 0x6E9CF2)
    static let amber = dynamic(0xC27A00, 0xE8A33D)
    static let red = dynamic(0xD93A3A, 0xEF6B6B)

    private static func dynamic(_ light: UInt32, _ dark: UInt32, lightAlpha: Double = 1, darkAlpha: Double = 1) -> Color {
        Color(
            nsColor: NSColor(name: nil) { appearance in
                let isDark = appearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua
                let hex = isDark ? dark : light
                return NSColor(
                    srgbRed: CGFloat((hex >> 16) & 0xFF) / 255,
                    green: CGFloat((hex >> 8) & 0xFF) / 255,
                    blue: CGFloat(hex & 0xFF) / 255,
                    alpha: isDark ? darkAlpha : lightAlpha
                )
            })
    }
}

enum Radius {
    static let row: CGFloat = 8
    static let card: CGFloat = 12
    static let composer: CGFloat = 20
}

/// Monochrome checkbox: outlined when off, filled with the primary color when on.
struct CheckboxStyle: ToggleStyle {
    func makeBody(configuration: Configuration) -> some View {
        Checkbox(configuration: configuration)
    }

    private struct Checkbox: View {
        let configuration: Configuration
        @Environment(\.isEnabled) private var isEnabled

        var body: some View {
            Button { configuration.isOn.toggle() } label: {
                HStack(spacing: 8) {
                    RoundedRectangle(cornerRadius: 4)
                        .fill(configuration.isOn ? Palette.primary : .clear)
                        .strokeBorder(configuration.isOn ? .clear : Palette.tertiaryText, lineWidth: 1.2)
                        .overlay {
                            if configuration.isOn {
                                Image(systemName: "checkmark")
                                    .font(.system(size: 9, weight: .bold))
                                    .foregroundStyle(Palette.onPrimary)
                            }
                        }
                        .frame(width: 15, height: 15)
                    configuration.label
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .opacity(isEnabled ? 1 : 0.35)
        }
    }
}

extension ToggleStyle where Self == CheckboxStyle {
    static var monochrome: CheckboxStyle { CheckboxStyle() }
}

/// The one strong button on screen: a filled pill, like ChatGPT's send button.
struct PrimaryButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        Pill(configuration: configuration)
    }

    private struct Pill: View {
        let configuration: Configuration
        @Environment(\.isEnabled) private var isEnabled

        var body: some View {
            configuration.label
                .font(.system(size: 13, weight: .medium))
                .foregroundStyle(Palette.onPrimary)
                .padding(.horizontal, 14)
                .frame(height: 30)
                .background(Palette.primary.opacity(isEnabled ? (configuration.isPressed ? 0.75 : 1) : 0.2), in: Capsule())
        }
    }
}

/// Quiet secondary controls: gray text that gains a soft background on hover.
struct ChipButtonStyle: ButtonStyle {
    var active = false

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: 12))
            .foregroundStyle(active ? Palette.text : Palette.secondaryText)
            .padding(.horizontal, 8)
            .frame(height: 26)
            .rowBackground(selected: active || configuration.isPressed, radius: 13)
    }
}

/// Soft rounded highlight for list and sidebar rows: stronger when selected, faint on hover.
struct RowBackground: ViewModifier {
    var selected: Bool
    var radius: CGFloat
    @State private var hovering = false

    func body(content: Content) -> some View {
        content
            .background(
                selected ? Palette.selected : hovering ? Palette.hover : .clear,
                in: RoundedRectangle(cornerRadius: radius)
            )
            .onHover { hovering = $0 }
    }
}

extension View {
    func rowBackground(selected: Bool = false, radius: CGFloat = Radius.row) -> some View {
        modifier(RowBackground(selected: selected, radius: radius))
    }

    /// A floating card: surface fill, hairline border, soft shadow.
    func card(radius: CGFloat = Radius.card) -> some View {
        background(Palette.surface, in: RoundedRectangle(cornerRadius: radius))
            .overlay(RoundedRectangle(cornerRadius: radius).strokeBorder(Palette.border))
            .shadow(color: .black.opacity(0.05), radius: 10, y: 3)
    }
}
