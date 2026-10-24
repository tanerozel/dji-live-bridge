import XCTest
@testable import StreamMyDrone

final class VirtualMicrophoneTests: XCTestCase {
    func testBluetoothIsNotAssumedBeforeAnExplicitCheck() {
        let availability = VirtualMicrophoneAvailability()
        XCTAssertEqual(availability.bluetoothStatus, .notChecked)
        XCTAssertEqual(availability.authorization, .notChecked)
        XCTAssertFalse(availability.canOpenAppSettings)
    }

    func testAnAllowedPoweredOnRadioDoesNotEstablishHfpOrMicrophoneRecognition() {
        var availability = VirtualMicrophoneAvailability()
        availability.beginCheck()
        availability.receive(radio: .poweredOn, authorization: .allowedAlways)
        XCTAssertEqual(availability.bluetoothStatus, .poweredOn)
        XCTAssertFalse(availability.hfpControlAvailable)
        XCTAssertFalse(availability.flyRecognitionVerified)
    }

    func testDeniedAndRestrictedAuthorizationOverridePoweredOnRadio() {
        for authorization in [VirtualMicrophoneBluetoothAuthorization.denied, .restricted] {
            var availability = VirtualMicrophoneAvailability()
            availability.beginCheck()
            availability.receive(radio: .poweredOn, authorization: authorization)
            XCTAssertEqual(availability.bluetoothStatus, .unauthorized)
            XCTAssertTrue(availability.canOpenAppSettings)
        }
    }

    func testUnknownAuthorizationCannotClaimAnAuthorizedRadio() {
        for authorization in [VirtualMicrophoneBluetoothAuthorization.notDetermined, .unknown] {
            var availability = VirtualMicrophoneAvailability()
            availability.beginCheck()
            availability.receive(radio: .poweredOn, authorization: authorization)
            XCTAssertEqual(availability.bluetoothStatus, .unknown)
            XCTAssertFalse(availability.canOpenAppSettings)
        }
    }

    func testRadioStatesRemainDistinctFromTheTransportBoundary() {
        let states: [(VirtualMicrophoneRadioState, VirtualMicrophoneBluetoothStatus)] = [
            (.poweredOff, .poweredOff), (.unauthorized, .unauthorized), (.unsupported, .unsupported), (.resetting, .resetting)
        ]
        for (radio, status) in states {
            var availability = VirtualMicrophoneAvailability()
            availability.beginCheck()
            availability.receive(radio: radio, authorization: .allowedAlways)
            XCTAssertEqual(availability.bluetoothStatus, status)
            XCTAssertFalse(availability.hfpControlAvailable)
            XCTAssertFalse(availability.flyRecognitionVerified)
        }
    }

    func testInitialUnknownWaitsForTheDeadlineAndLateUpdatesRemainTruthful() {
        var availability = VirtualMicrophoneAvailability()
        availability.beginCheck()
        availability.receive(radio: .unknown, authorization: .notDetermined)
        XCTAssertEqual(availability.bluetoothStatus, .checking)
        availability.timeOut()
        XCTAssertEqual(availability.phase, .timedOut)
        XCTAssertEqual(availability.bluetoothStatus, .unknown)
        availability.receive(radio: .poweredOn, authorization: .allowedAlways)
        XCTAssertEqual(availability.phase, .received)
        XCTAssertEqual(availability.bluetoothStatus, .poweredOn)
        XCTAssertFalse(availability.hfpControlAvailable)
    }

    func testCompletedResultDoesNotTimeOutAndRefreshDiscardsTheOldResult() {
        var availability = VirtualMicrophoneAvailability()
        availability.beginCheck()
        availability.receive(radio: .poweredOff, authorization: .allowedAlways)
        availability.timeOut()
        XCTAssertEqual(availability.bluetoothStatus, .poweredOff)
        availability.beginCheck()
        XCTAssertEqual(availability.bluetoothStatus, .checking)
        XCTAssertEqual(availability.authorization, .notChecked)
        XCTAssertEqual(availability.radio, .unknown)
    }

    func testUnrequestedUpdatesCannotChangeInitialState() {
        var availability = VirtualMicrophoneAvailability()
        availability.receive(radio: .poweredOn, authorization: .allowedAlways)
        availability.timeOut()
        XCTAssertEqual(availability.bluetoothStatus, .notChecked)
        XCTAssertEqual(availability.authorization, .notChecked)
    }

    func testReportContainsOnlyTheAllowlistedFieldsAndSanitizedCodes() {
        var availability = VirtualMicrophoneAvailability()
        availability.beginCheck()
        availability.receive(radio: .poweredOn, authorization: .allowedAlways)
        let report = availability.sanitizedReport(osVersion: OperatingSystemVersion(majorVersion: 27, minorVersion: 0, patchVersion: 1))
        XCTAssertEqual(report, """
        platform=iOS
        os_version=27.0.1
        bluetooth_check=received
        bluetooth_status=powered_on
        bluetooth_radio=powered_on
        bluetooth_authorization=allowed_always
        hfp_control_transport=unavailable_in_this_app
        dji_fly_microphone_recognition=unverified
        audio_transmission=not_started
        """)
        XCTAssertFalse(report.contains("connected"))
        XCTAssertFalse(report.contains("QCOM"))
        XCTAssertFalse(report.contains("UUID"))
        XCTAssertFalse(report.contains("address"))
    }
}
