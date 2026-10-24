import CoreBluetooth
import Foundation
import Observation

/**
 * Reads only CoreBluetooth's actual authorization and radio state after the user's action.
 * It never scans, opens a peripheral connection, or advertises a microphone service.
 */
@MainActor
@Observable
final class VirtualMicrophoneBluetoothProbe: NSObject, CBCentralManagerDelegate {
    private(set) var availability = VirtualMicrophoneAvailability()
    @ObservationIgnored private var central: CBCentralManager?
    @ObservationIgnored private var deadline: Task<Void, Never>?

    func checkStatus() {
        stop()
        availability.beginCheck()
        central = CBCentralManager(delegate: self, queue: .main,
                                   options: [CBCentralManagerOptionShowPowerAlertKey: false])
        deadline = Task { [weak self] in
            try? await Task.sleep(for: .seconds(5))
            guard !Task.isCancelled else { return }
            self?.availability.timeOut()
        }
    }

    func stop() {
        deadline?.cancel()
        deadline = nil
        central?.delegate = nil
        central = nil
        availability.timeOut()
    }

    nonisolated func centralManagerDidUpdateState(_ central: CBCentralManager) {
        // The manager uses the main queue; the task also protects actor isolation and lets
        // a dismissed sheet or a newer explicit check discard a stale callback.
        Task { @MainActor [weak self, weak central] in
            guard let self, let central, self.central === central else { return }
            self.availability.receive(radio: Self.radio(central.state), authorization: Self.authorization(CBManager.authorization))
            if self.availability.phase != .checking {
                self.deadline?.cancel()
                self.deadline = nil
            }
        }
    }

    private static func radio(_ state: CBManagerState) -> VirtualMicrophoneRadioState {
        switch state {
        case .unknown: .unknown
        case .resetting: .resetting
        case .unsupported: .unsupported
        case .unauthorized: .unauthorized
        case .poweredOff: .poweredOff
        case .poweredOn: .poweredOn
        @unknown default: .unknown
        }
    }

    private static func authorization(_ value: CBManagerAuthorization) -> VirtualMicrophoneBluetoothAuthorization {
        switch value {
        case .notDetermined: .notDetermined
        case .restricted: .restricted
        case .denied: .denied
        case .allowedAlways: .allowedAlways
        @unknown default: .unknown
        }
    }
}
