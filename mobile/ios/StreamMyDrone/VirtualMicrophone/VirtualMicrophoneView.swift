import SwiftUI
import UIKit

/// Shows this iOS app's availability boundary and an optional, real Bluetooth status check.
struct VirtualMicrophoneView: View {
    @Environment(\.palette) private var palette
    @Environment(\.openURL) private var openURL
    @State private var probe = VirtualMicrophoneBluetoothProbe()
    @State private var copied = false

    var body: some View {
        let availability = probe.availability
        VStack(alignment: .leading, spacing: 12) {
            Text(tr("vmic_title")).font(.headline)
            Text(tr("ios_vmic_explanation")).font(.subheadline).foregroundStyle(palette.muted)
        }

        BridgeCard(spacing: 10) {
            Label(tr("ios_vmic_transport_unavailable"), systemImage: "mic.slash")
                .font(.subheadline.weight(.semibold))
            Text(tr("vmic_fly_unverified")).font(.footnote).foregroundStyle(palette.muted)
            Text(tr("ios_vmic_scope")).font(.footnote).foregroundStyle(palette.muted)
        }

        BridgeCard(spacing: 12) {
            HStack(spacing: 8) {
                if availability.bluetoothStatus == .checking { ProgressView().tint(palette.accent) }
                Text(tr(availability.bluetoothStatus.textKey)).font(.subheadline)
            }
            Button { copied = false; probe.checkStatus() } label: {
                Label(tr("ios_vmic_check_bluetooth"), systemImage: "arrow.clockwise")
                    .font(.subheadline.weight(.semibold))
            }
            .disabled(availability.bluetoothStatus == .checking)
            .foregroundStyle(palette.accent)
            if availability.canOpenAppSettings {
                Button(tr("ios_vmic_app_settings")) {
                    if let url = URL(string: UIApplication.openSettingsURLString) { openURL(url) }
                }
                .font(.subheadline.weight(.semibold))
                .foregroundStyle(palette.accent)
            }
        }

        BridgeCard(spacing: 12) {
            DisclosureGroup(tr("ios_vmic_diagnostics")) {
                VStack(alignment: .leading, spacing: 10) {
                    Text(tr("ios_vmic_report_title")).font(.subheadline.weight(.semibold))
                    Text(report)
                        .font(.caption.monospaced())
                        .textSelection(.enabled)
                        .frame(maxWidth: .infinity, alignment: .leading)
                    Text(tr("ios_vmic_report_privacy")).font(.footnote).foregroundStyle(palette.muted)
                    Button {
                        UIPasteboard.general.string = report
                        copied = true
                    } label: {
                        Label(tr(copied ? "vmic_copied" : "vmic_copy_report"), systemImage: copied ? "checkmark" : "doc.on.doc")
                            .font(.subheadline.weight(.semibold))
                    }
                    .foregroundStyle(palette.accent)
                }
                .padding(.top, 8)
            }
            .tint(palette.muted)
        }
        .onChange(of: probe.availability) { copied = false }
        .onDisappear { probe.stop() }
    }

    private var report: String {
        probe.availability.sanitizedReport(osVersion: ProcessInfo.processInfo.operatingSystemVersion)
    }
}
