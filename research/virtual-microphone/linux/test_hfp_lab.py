"""Offline safety/interpretation tests. These do not exercise Bluetooth hardware."""

import copy
import argparse
from pathlib import Path
import tempfile
import unittest
from unittest import mock

import hfp_lab as lab


PEER = "12:34:56:78:9A:BC"


def fixture():
    return [
        {"id": 42, "type": "PipeWire:Interface:Node", "info": {"props": {
            "node.name": "alsa_input.builtin", "media.class": "Audio/Source",
            "device.api": "alsa", "object.serial": 142}}},
        {"id": 43, "type": "PipeWire:Interface:Node", "info": {"props": {
            "node.name": "bluez_output.peer", "media.class": "Audio/Sink",
            "api.bluez5.profile": "headset-audio-gateway", "api.bluez5.address": PEER,
            "api.bluez5.codec": "mSBC", "object.serial": 143}}},
    ]


class RoutingTests(unittest.TestCase):
    def test_outbound_microphone_goes_to_remote_ag_sink(self):
        plan = lab.route_plan(fixture(), "alsa_input.builtin", "bluez_output.peer", PEER)
        self.assertIn("--capture=142", plan["command"])
        self.assertIn("--playback=143", plan["command"])
        self.assertIn("not proof", plan["meaning"])

    def test_remote_downlink_is_not_a_microphone_uplink(self):
        graph = fixture()
        graph[1]["info"]["props"]["media.class"] = "Audio/Source"
        with self.assertRaisesRegex(RuntimeError, "Audio/Sink"):
            lab.route_plan(graph, "alsa_input.builtin", "bluez_output.peer", PEER)

    def test_a2dp_and_local_ag_role_are_rejected(self):
        for profile in ("a2dp-sink", "headset-head-unit"):
            graph = fixture()
            graph[1]["info"]["props"]["api.bluez5.profile"] = profile
            with self.subTest(profile=profile), self.assertRaisesRegex(RuntimeError, "wrong-role"):
                lab.route_plan(graph, "alsa_input.builtin", "bluez_output.peer", PEER)

    def test_wrong_peer_missing_node_and_unsupported_codec_stop_capture(self):
        for key, value in (("api.bluez5.address", "AA:BB:CC:DD:EE:FF"),
                           ("api.bluez5.codec", "sbc"), ("object.serial", None)):
            graph = fixture()
            graph[1]["info"]["props"][key] = value
            with self.subTest(key=key), self.assertRaises(RuntimeError):
                lab.route_plan(graph, "alsa_input.builtin", "bluez_output.peer", PEER)
        with self.assertRaisesRegex(RuntimeError, "absent"):
            lab.route_plan(fixture()[:1], "alsa_input.builtin", "bluez_output.peer", PEER)

    def test_sink_monitor_and_bluetooth_downlink_cannot_be_the_local_mic(self):
        graph = fixture()
        graph[0]["info"]["props"].pop("device.api")
        with self.assertRaisesRegex(RuntimeError, "ALSA"):
            lab.route_plan(graph, "alsa_input.builtin", "bluez_output.peer", PEER)

    def test_duplicate_names_and_id_reuse_do_not_silently_retarget(self):
        graph = fixture()
        graph.append(copy.deepcopy(graph[1]))
        with self.assertRaisesRegex(RuntimeError, "ambiguous"):
            lab.route_plan(graph, "alsa_input.builtin", "bluez_output.peer", PEER)

    def test_disconnect_terminates_the_owned_loopback_child(self):
        args = argparse.Namespace(peer=PEER, source="alsa_input.builtin", sink="bluez_output.peer", execute=True, max_seconds=300)
        process = mock.Mock(pid=12345)
        process.poll.return_value = None
        with mock.patch.object(lab, "linux_only"), mock.patch.object(lab, "command", return_value={"status": "ok", "stdout": lab.HFP_AG}), mock.patch.object(lab, "live_graph", side_effect=[fixture(), fixture()[:1]]), mock.patch.object(lab.shutil, "which", return_value="pw-loopback"), mock.patch.object(lab.subprocess, "Popen", return_value=process), mock.patch.object(lab.time, "sleep"), mock.patch.object(lab.os, "killpg") as kill:
            with self.assertRaisesRegex(RuntimeError, "absent"):
                lab.route(args)
            kill.assert_called_once_with(process.pid, lab.signal.SIGTERM)
            process.wait.assert_called_once_with(timeout=3)


class EvidenceTests(unittest.TestCase):
    def test_redacts_addresses_keys_urls_payloads_names_and_home(self):
        data = (f"Device {PEER} Personal phone\nAlias: Personal phone\n"
                "/org/bluez/hci0/dev_12_34_56_78_9A_BC\n"
                "Link key: 0011223344556677\nKey: 001122\n"
                "  00 11 22 33 44 55 66 77 88 99 aa bb\n"
                "rtmps://example.test/app/secret-value\n/home/person/private\nAT+BRSF=128\n")
        value = lab.sanitize(data)
        for personal in (PEER, "12_34_56_78_9A_BC", "Personal phone", "001122", "secret-value", "person/private", "aa bb"):
            self.assertNotIn(personal, value)
        self.assertIn("AT+BRSF=128", value)

    def test_graph_allowlist_excludes_titles_names_serials_and_argv(self):
        graph = fixture()
        graph[0]["info"]["props"].update({"media.title": "private", "device.serial": "private", "application.process.args": "private"})
        value = str(lab.summarized_graph(graph))
        self.assertNotIn("private", value)
        self.assertNotIn(PEER, value)
        self.assertNotIn("alsa_input.builtin", value)

    def test_reports_failures_and_timeouts_without_success(self):
        with mock.patch.object(lab.shutil, "which", return_value=None):
            self.assertEqual(lab.command(["missing"])["status"], "unavailable")
        with mock.patch.object(lab.shutil, "which", return_value="tool"), mock.patch.object(
                lab.subprocess, "run", side_effect=lab.subprocess.TimeoutExpired("tool", 1, output=b"partial")):
            result = lab.command(["tool"], timeout=1)
            self.assertEqual(result["status"], "timeout")
            self.assertIsNone(result["exit_code"])

    def test_non_linux_host_is_recorded_without_attempting_bluetooth(self):
        with tempfile.TemporaryDirectory() as directory, mock.patch.object(lab.platform, "system", return_value="Darwin"), mock.patch.object(lab, "command") as command:
            path = Path(directory) / "report.json"
            self.assertEqual(lab.preflight(path), 2)
            command.assert_not_called()
            self.assertIn('"hardware_verdict": "NOT_TESTED"', path.read_text())
            self.assertEqual(path.stat().st_mode & 0o777, 0o600)

    def test_never_overwrites_or_follows_an_existing_evidence_symlink(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "evidence.json"
            lab.write_json(path, {"first": True})
            with self.assertRaises(FileExistsError):
                lab.write_json(path, {"first": False})
            link = Path(directory) / "link.json"
            link.symlink_to(path)
            with self.assertRaises(FileExistsError):
                lab.write_json(link, {"first": False})


if __name__ == "__main__":
    unittest.main()
