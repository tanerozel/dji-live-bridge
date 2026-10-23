import importlib.util
import unittest
from pathlib import Path

spec = importlib.util.spec_from_file_location("collector", Path(__file__).with_name("collect-capabilities.py"))
collector = importlib.util.module_from_spec(spec)
spec.loader.exec_module(collector)


class SanitizedEvidenceTest(unittest.TestCase):
    def test_missing_property_is_unknown(self):
        self.assertEqual({"value": None, "observed": False}, collector.property_value("\n"))
        self.assertEqual({"value": "false", "observed": True}, collector.property_value("false\n"))

    def test_device_names_addresses_and_event_history_are_not_retained(self):
        raw = """enabled: true
Profile: HeadsetService
Profile: A2dpService
device: AA:BB:CC:DD:EE:FF Alice's headset
last error: Contact Alice at private@example.com
"""
        summary = collector.summarize_bluetooth(raw)
        self.assertEqual(["A2dpService", "HeadsetService"], summary["profile_dump_headings"])
        self.assertTrue(summary["enabled"])
        self.assertNotIn("Alice", str(summary))
        self.assertNotIn("AA:BB", str(summary))
        self.assertNotIn("private@", str(summary))

    def test_service_mention_does_not_report_running_or_connected(self):
        summary = collector.summarize_bluetooth("Disabled profiles: HeadsetClientService")
        self.assertTrue(summary["headset_client_service_mentioned"])
        self.assertEqual([], summary["profile_dump_headings"])
        self.assertIsNone(summary["enabled"])


if __name__ == "__main__":
    unittest.main()
