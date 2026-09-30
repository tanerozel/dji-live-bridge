import SwiftUI

/**
 * Where the stream goes, in the style of the screen: every platform with its switch, then the saved
 * ones with their stream keys. Instagram and TikTok need a new key for every broadcast.
 */
struct PlatformsSheet: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        let saved = Set(model.destinations.profiles.map(\.kind))
        let on = switchedOn(model)
        let live = model.relay.state.isLive
        let snapshot = model.relay.state.snapshot
        VStack(alignment: .leading, spacing: 2) {
            Text(tr("where_to_stream")).font(.headline)
            Text(tr("pick_platforms_hint")).font(.footnote).foregroundStyle(.white.opacity(0.6))
        }
        LazyVGrid(columns: Array(repeating: GridItem(.flexible(), spacing: Metrics.tileSpacing), count: Int(Metrics.visibleTiles)),
                  spacing: Metrics.tileSpacing) {
            ForEach(DestinationKind.allCases) { kind in
                let liveProfile = model.liveDestinations.first { $0.kind == kind }
                PlatformSwitch(
                    kind: kind,
                    checked: on.contains(kind),
                    saved: saved.contains(kind),
                    state: liveProfile.map { outputTone(snapshot.output($0.id)?.status, palette: .dark) }
                )
            }
        }
        if !model.destinations.profiles.isEmpty {
            Divider().overlay(.white.opacity(0.1))
            ForEach(model.destinations.profiles) { profile in
                SavedPlatform(profile: profile) { model.editProfile(profile) }
            }
        }
        let selected = model.destinations.selectedProfiles.count
        if !live && selected > 1 {
            Text(snapshot.bitrateKbps > 0
                ? trPlural("upload_note_total", selected, formatBitrate(snapshot.bitrateKbps * Double(selected)))
                : trPlural("upload_note", selected))
                .font(.footnote)
                .foregroundStyle(.white.opacity(0.6))
        }
    }
}

/// A saved platform with the state of its stream key and the way to change it.
private struct SavedPlatform: View {
    let profile: DestinationProfile
    let onEdit: () -> Void

    var body: some View {
        let renewKey = profile.kind.keyChangesEachStream
        HStack(spacing: 10) {
            PlatformTile(kind: profile.kind, size: 28)
            VStack(alignment: .leading, spacing: 1) {
                Text(profile.kind.displayName).font(.subheadline.weight(.semibold)).lineLimit(1)
                Text(tr(renewKey ? "key_changes_each_stream" : "key_saved"))
                    .font(.caption2)
                    .foregroundStyle(renewKey ? DroneColors.warning : .white.opacity(0.6))
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            Button(action: onEdit) {
                Label(tr(renewKey ? "update_key" : "edit"), systemImage: "key")
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(DroneColors.goLiveStart)
            }
            .buttonStyle(.plain)
        }
    }
}

/**
 * Everything about the stream that does not fit over the picture: the numbers, each platform
 * with its own "End", the tips and the technical details.
 */
struct LiveDetails: View {
    @Environment(AppModel.self) private var model
    @Environment(\.palette) private var palette
    let onEndPlatform: (DestinationProfile) -> Void

    var body: some View {
        let phase = model.phase
        let snapshot = model.relay.state.snapshot
        let destinations = model.liveDestinations
        if phase.isStreaming { StatsCard(snapshot: snapshot) }
        if destinations.count > 1 {
            BridgeCard(spacing: 10) {
                SectionHeader(title: tr("platforms"))
                ForEach(destinations) { profile in
                    let status = snapshot.output(profile.id)?.status
                    HStack(spacing: 12) {
                        PlatformTile(kind: profile.kind, size: 32)
                        VStack(alignment: .leading, spacing: 2) {
                            Text(profile.kind.displayName).font(.subheadline).lineLimit(1)
                            HStack(spacing: 6) {
                                Circle().fill(outputTone(status, palette: palette)).frame(width: 8, height: 8)
                                Text(tr(outputLabelKey(status))).font(.caption).foregroundStyle(palette.muted)
                            }
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)
                        Button(tr("end")) { onEndPlatform(profile) }
                            .foregroundStyle(palette.dangerText)
                            .accessibilityLabel(tr("end_platform", profile.kind.displayName))
                    }
                }
            }
        }
        ForEach(liveTips(phase: phase, snapshot: snapshot, destinations: destinations), id: \.self) { TipBox(text: $0) }
        BridgeCard(spacing: 8) {
            SectionHeader(title: tr("technical_details"))
            TechnicalDetailRows(snapshot: snapshot, destinations: destinations, lan: model.lan)
        }
    }
}

private struct StatsCard: View {
    let snapshot: RelaySnapshot
    @Environment(\.palette) private var palette

    var body: some View {
        let codecs = [snapshot.videoCodec, snapshot.audioCodec].compactMap { $0 }.map(friendlyCodecName).joined(separator: " · ")
        BridgeCard(padding: EdgeInsets(top: 16, leading: 8, bottom: 16, trailing: 8)) {
            HStack(spacing: 0) {
                stat(tr("stat_bitrate"), formatBitrate(snapshot.bitrateKbps))
                Divider().overlay(palette.border)
                stat(tr("stat_sent"), formatBytes(snapshot.outboundBytes))
                Divider().overlay(palette.border)
                stat(tr("stat_codec"), codecs.isEmpty ? "—" : codecs)
            }
            .fixedSize(horizontal: false, vertical: true)
        }
    }

    private func stat(_ label: String, _ value: String) -> some View {
        VStack(spacing: 2) {
            Text(value).font(.headline.monospacedDigit()).lineLimit(1).minimumScaleFactor(0.7)
            Text(label).font(.caption2).foregroundStyle(palette.muted)
        }
        .frame(maxWidth: .infinity)
        .accessibilityElement(children: .combine)
    }
}

private struct TechnicalDetailRows: View {
    let snapshot: RelaySnapshot
    let destinations: [DestinationProfile]
    let lan: LanAddress?
    @Environment(\.palette) private var palette

    var body: some View {
        DetailRow(label: tr("detail_source"), value: snapshot.remoteAddress ?? tr("detail_source_waiting"))
        DetailRow(label: tr("detail_received"), value: formatBytes(snapshot.receivedBytes))
        DetailRow(label: tr("detail_packets"), value: "\(snapshot.videoFrames) / \(snapshot.audioFrames)")
        // Where short drops come from: pauses in what arrives, DJI Fly starting over, the network.
        DetailRow(label: tr("detail_stalls"), value: snapshot.stalls > 0
            ? tr("detail_stalls_value", Int(snapshot.stalls), formatSeconds(millis: snapshot.longestStallMs))
            : "0")
        DetailRow(label: tr("detail_source_reconnects"), value: "\(snapshot.sourceReconnects)")
        // iOS does not tell apps the Wi-Fi signal; it can only say the phone is the hotspot.
        if lan?.kind == .hotspot {
            DetailRow(label: tr("detail_phone_wifi"), value: tr("detail_phone_wifi_hotspot"))
        }
        if snapshot.rejectedPublishAttempts > 0 {
            DetailRow(label: tr("detail_rejected"), value: "\(snapshot.rejectedPublishAttempts)")
        }
        ForEach(destinations) { profile in
            if let output = snapshot.output(profile.id) {
                Divider().overlay(palette.border)
                Text(profile.kind.displayName).font(.subheadline.weight(.semibold)).padding(.top, 8)
                DetailRow(label: tr("detail_forwarded"), value: formatBytes(output.outboundBytes))
                DetailRow(label: tr("detail_connection"), value: tr(
                    output.secure && (liveOutputStatuses.contains(output.status) || output.status == "ready")
                        ? "connection_rtmps_verified"
                        : output.secure ? "connection_rtmps_pending" : "connection_rtmp"
                ))
                DetailRow(label: tr("detail_status"), value: outputStateText(output))
                if let failure = outputFailureText(output) { DetailRow(label: tr("detail_last_error"), value: failure) }
                if output.reconnectAttempts > 0 { DetailRow(label: tr("detail_reconnects"), value: "\(output.reconnectAttempts)") }
                if output.droppedFrames > 0 { DetailRow(label: tr("detail_dropped"), value: "\(output.droppedFrames)") }
            }
        }
        Divider().overlay(palette.border)
        Text(tr("background_note")).font(.footnote).foregroundStyle(palette.muted)
    }
}

/// What one platform's connection is doing, for the technical details.
private func outputStateText(_ output: OutputSnapshot) -> String {
    switch output.status {
    case "armed": tr("output_armed")
    case "connecting": tr("output_connecting", output.secure ? "RTMPS" : "RTMP")
    case "ready": tr(output.secure ? "output_ready_secure" : "output_ready")
    case "forwarding": tr(output.secure ? "output_forwarding_secure" : "output_forwarding")
    case "congested": tr("output_congested")
    case "holding": tr("output_holding")
    case "reconnecting": output.retryInSeconds.map { trPlural("output_retry", Int($0)) } ?? tr("status_reconnecting")
    case "stopped": tr("output_stopped")
    default: output.status
    }
}

/// Why the last attempt failed; the technical cause from the relay stays in English, in brackets.
private func outputFailureText(_ output: OutputSnapshot) -> String? {
    guard let reason = output.reason else { return nil }
    let key: String? = switch reason {
    case "tls_or_network": "output_failure_tls"
    case "connect_failed": "output_failure_connect"
    case "publish_rejected": "output_failure_rejected"
    case "send_failed": "output_failure_send"
    case "network": "output_failure_network"
    case "stalled": "output_failure_stalled"
    default: nil
    }
    let text = key.map { tr($0) } ?? reason
    return output.reasonDetail.map { tr("error_with_detail", text, $0) } ?? text
}
