import SwiftUI

// Sized for a 360-point wide phone at a larger text size: four platform tiles fit in a row.
enum Metrics {
    static let edge: CGFloat = 12
    static let gap: CGFloat = 8
    static let icon: CGFloat = 22
    static let cardHeight: CGFloat = 52
    static let cardRadius: CGFloat = 14
    static let panelRadius: CGFloat = 18
    static let sideButton: CGFloat = 40
    static let tileRadius: CGFloat = 12
    static let tileIcon: CGFloat = 30
    static let visibleTiles: CGFloat = 4
    static let tileSpacing: CGFloat = 6
    static let minTileWidth: CGFloat = 62
    static let squareAction: CGFloat = 52
    static let squareActionMaxWidth: CGFloat = 76
    static let landscapeActionsWidth: CGFloat = 320
}

extension View {
    /// The see-through dark surface with a faint edge that everything over the picture uses.
    func glass<S: InsettableShape>(_ shape: S) -> some View {
        background(DroneColors.glass, in: shape)
            .overlay(shape.strokeBorder(.white.opacity(0.15), lineWidth: 1))
            .clipShape(shape)
    }

    func glass(radius: CGFloat = Metrics.cardRadius) -> some View {
        glass(RoundedRectangle(cornerRadius: radius, style: .continuous))
    }
}

/// One line that shrinks rather than cutting off a name at a large text size.
struct FittedText: View {
    let text: String
    var font: Font = .caption
    var weight: Font.Weight = .regular
    var color: Color = .white
    var minimumScale: CGFloat = 0.55

    var body: some View {
        Text(text)
            .font(font.weight(weight))
            .foregroundStyle(color)
            .lineLimit(1)
            .minimumScaleFactor(minimumScale)
    }
}

/// A small switch that fits a platform tile; the whole tile is the control.
struct MiniSwitch: View {
    let checked: Bool

    var body: some View {
        Capsule()
            .fill(checked ? DroneColors.goLiveStart : .white.opacity(0.18))
            .overlay(Capsule().strokeBorder(checked ? DroneColors.goLiveStart : .white.opacity(0.3), lineWidth: 1))
            .frame(width: 32, height: 18)
            .overlay(alignment: checked ? .trailing : .leading) {
                Circle()
                    .fill(.white.opacity(checked ? 1 : 0.85))
                    .frame(width: 12, height: 12)
                    .padding(3)
            }
            .animation(.easeInOut(duration: 0.18), value: checked)
    }
}

struct PulsingDot: View {
    let color: Color
    var size: CGFloat = 8
    @State private var dim = false

    var body: some View {
        Circle()
            .fill(color)
            .frame(width: size, height: size)
            .opacity(dim ? 0.3 : 1)
            .onAppear {
                withAnimation(.easeInOut(duration: 0.9).repeatForever(autoreverses: true)) { dim = true }
            }
    }
}

/// How long the broadcast has been live, ticking every second.
struct LiveTimer: View {
    let since: TimeInterval
    var font: Font = .caption2
    var color: Color = .white

    var body: some View {
        TimelineView(.periodic(from: .now, by: 1)) { _ in
            let elapsed = formatDuration(millis: Int64((ProcessInfo.processInfo.systemUptime - since) * 1_000))
            Text(elapsed)
                .font(font.monospacedDigit())
                .foregroundStyle(color)
                .accessibilityLabel(tr("stream_time", elapsed))
        }
    }
}

// MARK: - Themed surfaces (guide, key screen, details)

struct BridgeCard<Content: View>: View {
    var padding: EdgeInsets = EdgeInsets(top: 20, leading: 20, bottom: 20, trailing: 20)
    var spacing: CGFloat = 16
    @ViewBuilder let content: Content
    @Environment(\.palette) private var palette

    var body: some View {
        VStack(alignment: .leading, spacing: spacing) { content }
            .padding(padding)
            .frame(maxWidth: .infinity, alignment: .leading)
            .foregroundStyle(palette.text)
            .background(palette.card, in: RoundedRectangle(cornerRadius: 20, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: 20, style: .continuous).strokeBorder(palette.border, lineWidth: 1))
    }
}

struct SectionHeader: View {
    let title: String

    var body: some View {
        Text(title)
            .font(.subheadline.weight(.semibold))
            .frame(maxWidth: .infinity, alignment: .leading)
            .accessibilityAddTraits(.isHeader)
    }
}

struct DetailRow: View {
    let label: String
    let value: String
    @Environment(\.palette) private var palette

    var body: some View {
        HStack(alignment: .top, spacing: 12) {
            Text(label)
                .foregroundStyle(palette.muted)
                .frame(maxWidth: .infinity, alignment: .leading)
            Text(value)
                .foregroundStyle(palette.text)
                .multilineTextAlignment(.trailing)
                .frame(maxWidth: .infinity, alignment: .trailing)
        }
        .font(.subheadline)
        .accessibilityElement(children: .combine)
    }
}

/// Soft accent box for short explanations (desktop .callout).
struct TipBox: View {
    let text: String
    var systemImage = "lightbulb"
    @Environment(\.palette) private var palette

    var body: some View {
        HStack(alignment: .top, spacing: 10) {
            Image(systemName: systemImage).font(.subheadline)
            Text(text).font(.subheadline)
        }
        .foregroundStyle(palette.onAccentSoft)
        .padding(12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(palette.accentSoft, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
    }
}

struct AlertBanner: View {
    let title: String
    var message: String?
    @Environment(\.palette) private var palette

    var body: some View {
        HStack(alignment: .top, spacing: 10) {
            Image(systemName: "exclamationmark.circle").foregroundStyle(palette.dangerText)
            VStack(alignment: .leading, spacing: 2) {
                Text(title).font(.subheadline.weight(.semibold)).foregroundStyle(palette.dangerText)
                if let message { Text(message).font(.subheadline).foregroundStyle(palette.text) }
            }
        }
        .padding(12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(palette.dangerSoft, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
        .accessibilityElement(children: .combine)
    }
}

struct PrimaryButton: View {
    let title: String
    var systemImage: String?
    var enabled = true
    let action: () -> Void
    @Environment(\.palette) private var palette

    var body: some View {
        Button(action: action) {
            HStack(spacing: 8) {
                if let systemImage { Image(systemName: systemImage) }
                Text(title).font(.headline)
            }
            .frame(maxWidth: .infinity, minHeight: 54)
            .foregroundStyle(enabled ? .white : palette.muted)
            .background(enabled ? palette.action : palette.fieldStrong, in: RoundedRectangle(cornerRadius: 16, style: .continuous))
        }
        .buttonStyle(.plain)
        .disabled(!enabled)
    }
}
