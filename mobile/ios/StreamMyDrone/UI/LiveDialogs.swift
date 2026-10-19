import SwiftUI

/// The platforms a title dialog is for.
struct TitleRequest: Identifiable, Equatable {
    let id = UUID()
    let profileIds: [String]
}

/**
 * The last step before going live: where the broadcast goes, its title and, for Instagram, who
 * sees it. Dark like the camera screen it opens over, with the one big orange button.
 */
struct BroadcastTitleDialog: View {
    @Environment(AppModel.self) private var model
    let request: TitleRequest
    @State private var draft: String
    @State private var audience: InstagramAudience
    @State private var touched = false
    @FocusState private var focused: Bool

    init(request: TitleRequest, initialTitle: String, initialAudience: InstagramAudience) {
        self.request = request
        _draft = State(initialValue: initialTitle)
        _audience = State(initialValue: initialAudience)
    }

    private var profiles: [DestinationProfile] {
        model.destinations.profiles.filter { request.profileIds.contains($0.id) }
    }

    var body: some View {
        let title = try? validateBroadcastTitle(draft)
        let kinds = profiles.map(\.kind).reduce(into: [DestinationKind]()) { if !$0.contains($1) { $0.append($1) } }
        let hasInstagram = profiles.contains { $0.kind == .instagram && !model.instagramManualProfiles.contains($0.id) }
        let hasManual = profiles.contains {
            ($0.kind != .instagram && $0.youtube == nil) || model.instagramManualProfiles.contains($0.id)
        }
        LiveDialog(onDismiss: { model.titleRequest = nil }) {
                VStack(alignment: .leading, spacing: 18) {
                    VStack(alignment: .leading, spacing: 12) {
                        Text(tr("go_live")).font(.title2.weight(.semibold)).accessibilityAddTraits(.isHeader)
                        if !kinds.isEmpty { PlatformChips(kinds: kinds) }
                    }
                    VStack(alignment: .leading, spacing: 6) {
                        Text(tr("broadcast_title")).font(.caption.weight(.semibold))
                            .foregroundStyle(touched && title == nil ? DroneColors.danger : focused ? DroneColors.goLiveStart : .white.opacity(0.7))
                        HStack {
                            TextField(tr("broadcast_title"), text: $draft)
                                .focused($focused)
                                .submitLabel(.go)
                                .onSubmit(submit)
                                .onChange(of: draft) { touched = true }
                            if !draft.isEmpty {
                                Button { draft = ""; focused = true } label: {
                                    Image(systemName: "xmark.circle.fill").foregroundStyle(.white.opacity(0.5))
                                }
                                .accessibilityLabel(tr("broadcast_title_clear"))
                            }
                        }
                        .padding(.horizontal, 14)
                        .frame(minHeight: 52)
                        .background(.white.opacity(0.05), in: RoundedRectangle(cornerRadius: 14, style: .continuous))
                        .overlay(RoundedRectangle(cornerRadius: 14, style: .continuous).strokeBorder(
                            touched && title == nil ? DroneColors.danger : focused ? DroneColors.goLiveStart : .white.opacity(0.2),
                            lineWidth: focused ? 2 : 1))
                        Text(touched && title == nil ? tr("broadcast_title_error") : "\(draft.unicodeScalars.count)/100")
                            .font(.caption)
                            .foregroundStyle(touched && title == nil ? DroneColors.danger : .white.opacity(0.6))
                            .padding(.horizontal, 4)
                    }
                    if hasInstagram {
                        VStack(alignment: .leading, spacing: 8) {
                            Text(tr("instagram_audience")).font(.subheadline.weight(.semibold)).foregroundStyle(.white.opacity(0.7))
                            ForEach(InstagramAudience.allCases) { choice in
                                LiveChoice(label: tr(choice.labelKey), hint: tr(choice.hintKey), selected: choice == audience) {
                                    audience = choice
                                }
                            }
                        }
                    }
                    if hasManual {
                        HStack(alignment: .top, spacing: 8) {
                            Image(systemName: "info.circle").font(.footnote).foregroundStyle(.white.opacity(0.5))
                            Text(tr("broadcast_title_manual")).font(.footnote).foregroundStyle(.white.opacity(0.6))
                        }
                    }
                    VStack(spacing: 4) {
                        LiveButton(title: tr("go_live"), systemImage: "dot.radiowaves.left.and.right", enabled: title != nil, action: submit)
                        Button(tr("cancel")) { model.titleRequest = nil }
                            .font(.subheadline.weight(.semibold))
                            .foregroundStyle(.white.opacity(0.8))
                            .frame(maxWidth: .infinity, minHeight: 48)
                    }
                }
                .padding(.horizontal, 20)
                .padding(.vertical, 22)
        }
        .onAppear { focused = true }
    }

    private func submit() {
        guard let title = try? validateBroadcastTitle(draft) else { touched = true; return }
        model.startBroadcast(request, title: title, audience: audience)
    }
}

/// While the app gets Instagram ready by itself; its window opens only if it needs the user.
struct InstagramPreparingCard: View {
    let onCancel: () -> Void

    var body: some View {
        LiveDialog(onDismiss: onCancel) {
            VStack(spacing: 12) {
                ZStack {
                    Circle().stroke(.white.opacity(0.08), lineWidth: 3).frame(width: 84, height: 84)
                    SpinningArc().frame(width: 84, height: 84)
                    PlatformTile(kind: .instagram, size: 56)
                }
                .padding(.bottom, 4)
                Text(tr("instagram_preparing")).font(.title3.weight(.semibold)).multilineTextAlignment(.center)
                Text(tr("instagram_preparing_hint")).font(.subheadline).foregroundStyle(.white.opacity(0.7)).multilineTextAlignment(.center)
                Button(tr("cancel"), action: onCancel)
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(.white.opacity(0.8))
                    .frame(maxWidth: .infinity, minHeight: 48)
            }
            .padding(.horizontal, 24)
            .padding(.top, 28)
            .padding(.bottom, 12)
            .accessibilityElement(children: .contain)
        }
    }
}

/// An orange arc going round, like Android's progress ring around the Instagram tile.
private struct SpinningArc: View {
    @State private var turning = false

    var body: some View {
        Circle()
            .trim(from: 0, to: 0.3)
            .stroke(DroneColors.goLiveStart, style: StrokeStyle(lineWidth: 3, lineCap: .round))
            .rotationEffect(.degrees(turning ? 360 : 0))
            .animation(.linear(duration: 1).repeatForever(autoreverses: false), value: turning)
            .onAppear { turning = true }
            .accessibilityHidden(true)
    }
}

/**
 * A dialog over the camera screen: always dark, whatever theme the rest of the app uses. It is
 * as tall as its content, and scrolls when the keyboard or a large text size leaves less room.
 */
struct LiveDialog<Content: View>: View {
    let onDismiss: () -> Void
    @ViewBuilder let content: Content
    @State private var contentHeight: CGFloat = 0

    var body: some View {
        GeometryReader { space in
            ZStack {
                Color.black.opacity(0.5).ignoresSafeArea().onTapGesture(perform: onDismiss)
                    .accessibilityAddTraits(.isButton)
                    .accessibilityLabel(tr("close"))
                ScrollView {
                    content.background(GeometryReader { measured in
                        Color.clear.preference(key: DialogHeight.self, value: measured.size.height)
                    })
                }
                .scrollBounceBehavior(.basedOnSize)
                .frame(height: max(min(contentHeight, space.size.height - 48), 0))
                .foregroundStyle(.white)
                .frame(maxWidth: 440)
                .background(DroneColors.sheet, in: RoundedRectangle(cornerRadius: 28, style: .continuous))
                .overlay(RoundedRectangle(cornerRadius: 28, style: .continuous).strokeBorder(.white.opacity(0.08), lineWidth: 1))
                .padding(.horizontal, 24)
            }
            .frame(width: space.size.width, height: space.size.height)
        }
        .onPreferenceChange(DialogHeight.self) { contentHeight = $0 }
        .environment(\.colorScheme, .dark)
    }
}

private struct DialogHeight: PreferenceKey {
    static let defaultValue: CGFloat = 0
    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) { value = max(value, nextValue()) }
}

/// One choice in a list of them, with a line under its name saying what it means.
struct LiveChoice: View {
    let label: String
    let hint: String?
    let selected: Bool
    let onSelect: () -> Void

    var body: some View {
        let shape = RoundedRectangle(cornerRadius: 14, style: .continuous)
        Button(action: onSelect) {
            HStack(spacing: 12) {
                Image(systemName: selected ? "largecircle.fill.circle" : "circle")
                    .font(.title3)
                    .foregroundStyle(selected ? DroneColors.goLiveStart : .white.opacity(0.5))
                VStack(alignment: .leading, spacing: 2) {
                    Text(label).font(.subheadline.weight(.semibold)).foregroundStyle(.white)
                    if let hint { Text(hint).font(.footnote).foregroundStyle(.white.opacity(0.65)) }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .multilineTextAlignment(.leading)
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
            .background(selected ? DroneColors.goLiveStart.opacity(0.12) : .white.opacity(0.04), in: shape)
            .overlay(shape.strokeBorder(selected ? DroneColors.goLiveStart.opacity(0.9) : .white.opacity(0.12), lineWidth: 1))
            .contentShape(shape)
        }
        .buttonStyle(.plain)
        .accessibilityAddTraits(selected ? [.isSelected] : [])
    }
}

/// The orange button of the camera screen, full width; its text shrinks to fit any language.
struct LiveButton: View {
    let title: String
    var systemImage: String?
    var enabled = true
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: 8) {
                if let systemImage { Image(systemName: systemImage).font(.system(size: 18, weight: .semibold)) }
                Text(title).font(.system(size: 16, weight: .semibold)).lineLimit(1).minimumScaleFactor(0.65)
            }
            .foregroundStyle(.white)
            .padding(.horizontal, 16)
            .frame(maxWidth: .infinity, minHeight: 52)
            .background(LinearGradient(colors: [DroneColors.goLiveStart, DroneColors.goLiveEnd], startPoint: .leading, endPoint: .trailing), in: Capsule())
            .opacity(enabled ? 1 : 0.45)
        }
        .buttonStyle(.plain)
        .disabled(!enabled)
    }
}

/// The platforms a broadcast goes to, as small labelled tiles.
struct PlatformChips: View {
    let kinds: [DestinationKind]

    var body: some View {
        FlowLayout(spacing: 6) {
            ForEach(kinds) { kind in
                HStack(spacing: 6) {
                    PlatformTile(kind: kind, size: 20)
                    Text(kind.displayName).font(.caption.weight(.medium)).foregroundStyle(.white)
                }
                .padding(.leading, 4)
                .padding(.trailing, 10)
                .padding(.vertical, 4)
                .background(.white.opacity(0.07), in: Capsule())
                .overlay(Capsule().strokeBorder(.white.opacity(0.1), lineWidth: 1))
            }
        }
    }
}

/// Lays its children out in rows, starting a new row when one is full.
struct FlowLayout: Layout {
    var spacing: CGFloat = 8

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let rows = arrange(proposal.width ?? .infinity, subviews)
        let width = rows.map(\.width).max() ?? 0
        let height = rows.map(\.height).reduce(0, +) + spacing * CGFloat(max(rows.count - 1, 0))
        return CGSize(width: width, height: height)
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        var y = bounds.minY
        for row in arrange(bounds.width, subviews) {
            var x = bounds.minX
            for index in row.indices {
                let size = subviews[index].sizeThatFits(.unspecified)
                subviews[index].place(at: CGPoint(x: x, y: y), proposal: ProposedViewSize(size))
                x += size.width + spacing
            }
            y += row.height + spacing
        }
    }

    private struct Row { var indices: [Int] = []; var width: CGFloat = 0; var height: CGFloat = 0 }

    private func arrange(_ maxWidth: CGFloat, _ subviews: Subviews) -> [Row] {
        var rows = [Row()]
        for index in subviews.indices {
            let size = subviews[index].sizeThatFits(.unspecified)
            if !rows[rows.count - 1].indices.isEmpty && rows[rows.count - 1].width + spacing + size.width > maxWidth {
                rows.append(Row())
            }
            var row = rows[rows.count - 1]
            row.width += (row.indices.isEmpty ? 0 : spacing) + size.width
            row.height = max(row.height, size.height)
            row.indices.append(index)
            rows[rows.count - 1] = row
        }
        return rows
    }
}
