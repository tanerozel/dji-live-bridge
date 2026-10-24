import Foundation

/// The radio state is deliberately separate from HFP availability and DJI Fly recognition.
enum VirtualMicrophoneRadioState: String, CaseIterable {
    case unknown, resetting, unsupported, unauthorized, poweredOff = "powered_off", poweredOn = "powered_on"
}

enum VirtualMicrophoneBluetoothAuthorization: String, CaseIterable {
    case notChecked = "not_checked", notDetermined = "not_determined", restricted, denied
    case allowedAlways = "allowed_always", unknown
}

enum VirtualMicrophoneBluetoothStatus: String {
    case notChecked = "not_checked", checking, poweredOn = "powered_on", poweredOff = "powered_off"
    case unauthorized, unsupported, resetting, unknown

    var textKey: String {
        switch self {
        case .notChecked: "ios_vmic_not_checked"
        case .checking: "ios_vmic_checking"
        case .poweredOn: "ios_vmic_bluetooth_on"
        case .poweredOff: "ios_vmic_bluetooth_off"
        case .unauthorized: "ios_vmic_bluetooth_unauthorized"
        case .unsupported: "ios_vmic_bluetooth_unsupported"
        case .resetting: "ios_vmic_bluetooth_resetting"
        case .unknown: "ios_vmic_bluetooth_unknown"
        }
    }
}

/**
 * The app has no public iOS HFP control transport. A Bluetooth permission or powered-on
 * radio never changes that fact, and never establishes microphone recognition on the RC 2.
 * The pure model also keeps a check bounded while allowing a later real delegate update.
 */
struct VirtualMicrophoneAvailability: Equatable {
    enum CheckPhase: String { case notChecked = "not_checked", checking, received, timedOut = "timed_out" }

    private(set) var phase: CheckPhase = .notChecked
    private(set) var radio: VirtualMicrophoneRadioState = .unknown
    private(set) var authorization: VirtualMicrophoneBluetoothAuthorization = .notChecked

    var hfpControlAvailable: Bool { false }
    var flyRecognitionVerified: Bool { false }

    var bluetoothStatus: VirtualMicrophoneBluetoothStatus {
        guard phase != .notChecked else { return .notChecked }
        if authorization == .denied || authorization == .restricted || radio == .unauthorized { return .unauthorized }
        switch radio {
        case .poweredOn: return authorization == .allowedAlways ? .poweredOn : .unknown
        case .poweredOff: return .poweredOff
        case .unsupported: return .unsupported
        case .resetting: return .resetting
        case .unauthorized: return .unauthorized
        case .unknown: return phase == .checking ? .checking : .unknown
        }
    }

    var canOpenAppSettings: Bool { authorization == .denied || authorization == .restricted }

    mutating func beginCheck() {
        phase = .checking
        radio = .unknown
        authorization = .notChecked
    }

    mutating func receive(radio: VirtualMicrophoneRadioState, authorization: VirtualMicrophoneBluetoothAuthorization) {
        guard phase != .notChecked else { return }
        self.radio = radio
        self.authorization = authorization
        // Initial .unknown is transient. Keep waiting for a real result, up to the deadline.
        if radio != .unknown || authorization == .denied || authorization == .restricted {
            phase = .received
        }
    }

    mutating func timeOut() {
        guard phase == .checking else { return }
        phase = .timedOut
    }

    /// Only fixed status codes and numeric OS version fields enter the report.
    func sanitizedReport(osVersion: OperatingSystemVersion) -> String {
        """
        platform=iOS
        os_version=\(osVersion.majorVersion).\(osVersion.minorVersion).\(osVersion.patchVersion)
        bluetooth_check=\(phase.rawValue)
        bluetooth_status=\(bluetoothStatus.rawValue)
        bluetooth_radio=\(radio.rawValue)
        bluetooth_authorization=\(authorization.rawValue)
        hfp_control_transport=unavailable_in_this_app
        dji_fly_microphone_recognition=unverified
        audio_transmission=not_started
        """
    }
}
