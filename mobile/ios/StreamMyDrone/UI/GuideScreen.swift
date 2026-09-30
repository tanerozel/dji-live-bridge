import SwiftUI

private let guidePageCount = 3

/// First-run walkthrough; "How to use" in the settings menu opens it again at any time.
struct GuideScreen: View {
    let onFinish: () -> Void
    @State private var page = 0
    @Environment(\.palette) private var palette

    var body: some View {
        let lastPage = page == guidePageCount - 1
        VStack(spacing: 0) {
            HStack {
                Spacer()
                if !lastPage { Button(tr("guide_skip"), action: onFinish).font(.body.weight(.semibold)).foregroundStyle(palette.link) }
            }
            .frame(minHeight: 56)
            .padding(.horizontal, 16)
            TabView(selection: $page) {
                ForEach(0..<guidePageCount, id: \.self) { index in
                    GuidePage(page: index).tag(index)
                }
            }
            .tabViewStyle(.page(indexDisplayMode: .never))
            VStack(spacing: 24) {
                PageIndicator(current: page)
                PrimaryButton(title: tr(lastPage ? "guide_start" : "guide_next")) {
                    if lastPage { onFinish() } else { withAnimation { page += 1 } }
                }
            }
            .frame(maxWidth: 520)
            .padding(.horizontal, 24)
            .padding(.vertical, 16)
        }
        .foregroundStyle(palette.text)
        .background(palette.background.ignoresSafeArea())
    }
}

private struct GuidePage: View {
    let page: Int
    @Environment(\.palette) private var palette

    var body: some View {
        GeometryReader { geometry in
            // Centered when it fits, scrollable when the text is large or the screen is short.
            ScrollView {
                VStack(spacing: 28) {
                    switch page {
                    case 0:
                        FlowIllustration()
                        PageText(title: tr("guide_flow_title"), message: tr("guide_flow_body"))
                    case 1:
                        AddressIllustration()
                        PageText(title: tr("guide_connect_title"), message: tr("guide_connect_body"))
                    default:
                        PlatformsIllustration()
                        PageText(title: tr("guide_live_title"), message: tr("guide_live_body"))
                    }
                }
                .frame(maxWidth: 520)
                .padding(.horizontal, 28)
                .padding(.vertical, 12)
                .frame(maxWidth: .infinity, minHeight: geometry.size.height)
            }
        }
    }
}

private struct PageText: View {
    let title: String
    let message: String
    @Environment(\.palette) private var palette

    var body: some View {
        VStack(spacing: 10) {
            Text(title).font(.title2.weight(.semibold)).multilineTextAlignment(.center).accessibilityAddTraits(.isHeader)
            Text(message).font(.body).foregroundStyle(palette.muted).multilineTextAlignment(.center)
        }
    }
}

private struct IllustrationCircle<Icon: View>: View {
    @ViewBuilder let icon: Icon
    @Environment(\.palette) private var palette

    var body: some View {
        Circle()
            .fill(palette.accentSoft)
            .frame(width: 72, height: 72)
            .overlay(icon.frame(width: 32, height: 32).foregroundStyle(palette.link))
    }
}

private struct FlowIllustration: View {
    @Environment(\.palette) private var palette

    var body: some View {
        HStack(spacing: 6) {
            IllustrationCircle { Image("drone").resizable().renderingMode(.template).scaledToFit() }
            arrow
            IllustrationCircle { Image(systemName: "iphone").resizable().scaledToFit() }
            arrow
            HStack(spacing: -10) {
                ForEach([DestinationKind.instagram, .tiktok, .youtube]) { kind in
                    PlatformTile(kind: kind, size: 44)
                        .overlay(RoundedRectangle(cornerRadius: 12, style: .continuous).strokeBorder(palette.background, lineWidth: 2))
                }
            }
        }
        .accessibilityHidden(true)
    }

    private var arrow: some View {
        Image(systemName: "chevron.forward").font(.body.weight(.semibold)).foregroundStyle(palette.faint).flipsForRightToLeftLayoutDirection(true)
    }
}

private struct PlatformsIllustration: View {
    var body: some View {
        VStack(spacing: 12) {
            HStack(spacing: 12) { ForEach(DestinationKind.allCases.prefix(4)) { PlatformTile(kind: $0, size: 56) } }
            HStack(spacing: 12) { ForEach(DestinationKind.allCases.dropFirst(4)) { PlatformTile(kind: $0, size: 56) } }
        }
        .accessibilityHidden(true)
    }
}

private struct AddressIllustration: View {
    @Environment(\.palette) private var palette

    var body: some View {
        VStack(spacing: 14) {
            IllustrationCircle { Image("drone").resizable().renderingMode(.template).scaledToFit() }
            Text(verbatim: "rtmp://192.168.1.20:1935/drone")
                .font(.system(.body, design: .monospaced).weight(.semibold))
                .padding(.horizontal, 16)
                .padding(.vertical, 12)
                .background(palette.card, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
                .environment(\.layoutDirection, .leftToRight)
        }
        .accessibilityHidden(true)
    }
}

private struct PageIndicator: View {
    let current: Int
    @Environment(\.palette) private var palette

    var body: some View {
        HStack(spacing: 8) {
            ForEach(0..<guidePageCount, id: \.self) { index in
                Capsule()
                    .fill(index == current ? palette.accent : palette.fieldStrong)
                    .frame(width: index == current ? 22 : 8, height: 8)
            }
        }
        .animation(.easeInOut, value: current)
        .accessibilityElement()
        .accessibilityLabel(tr("guide_page", current + 1, guidePageCount))
    }
}
