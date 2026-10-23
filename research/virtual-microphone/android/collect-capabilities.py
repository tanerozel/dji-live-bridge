#!/usr/bin/env python3
"""Read-only ADB evidence; raw dumps, Bluetooth addresses and serials are never saved."""

import argparse
import datetime
import json
import re
import subprocess
from pathlib import Path


PROPERTIES = (
    "ro.product.manufacturer",
    "ro.product.model",
    "ro.build.version.release",
    "ro.build.version.sdk",
    "bluetooth.profile.hfp.hf.enabled",
    "bluetooth.profile.hfp.ag.enabled",
    "bluetooth.hfp.software_datapath.enabled",
)


def property_value(raw):
    """An absent property is unknown, never false."""
    value = raw.strip()
    return {"value": value or None, "observed": bool(value)}


def summarize_bluetooth(raw):
    # Only identifier-shaped profile headings. No device/event/history lines survive.
    profiles = sorted(set(re.findall(r"^\s*Profile:\s*([A-Za-z][A-Za-z0-9_]+)\s*$", raw, re.M)))
    enabled = re.search(r"^\s*enabled:\s*(true|false)\s*$", raw, re.M)
    return {
        "enabled": enabled.group(1) == "true" if enabled else None,
        "profile_dump_headings": profiles,
        "headset_client_service_mentioned": "HeadsetClientService" in raw,
        "interpretation": (
            "A service mention or dump heading does not prove app access, SDP advertising, "
            "SLC, SCO, microphone routing, or remote acceptance. Absence is not proof of lack of hardware support."
        ),
    }


def summarize_package(raw):
    # Absence from this dump is not an API permission check. The app performs checkSelfPermission.
    return {
        "bluetooth_privileged_mentioned_in_package_dump": "android.permission.BLUETOOTH_PRIVILEGED" in raw,
        "interpretation": "A package dump mention is not proof of a granted signature permission.",
    }


def collect(adb, serial):
    def read(*args):
        try:
            result = subprocess.run(
                [adb, "-s", serial, "shell", *args],
                capture_output=True, text=True, timeout=30, check=False,
            )
        except subprocess.TimeoutExpired:
            return {"result": "timeout", "stdout": ""}
        except OSError:
            return {"result": "adb_unavailable", "stdout": ""}
        return {"result": "ok" if result.returncode == 0 else "adb_nonzero_exit", "stdout": result.stdout}

    properties = {}
    for name in PROPERTIES:
        result = read("getprop", name)
        properties[name] = {"command_result": result["result"], **property_value(result["stdout"] if result["result"] == "ok" else "")}
    bluetooth = read("dumpsys", "bluetooth_manager")
    package = read("dumpsys", "package", "com.streammydrone.app")
    selinux = read("getenforce")
    enforcing = selinux["stdout"].strip()
    return {
        "schema_version": 1,
        "collected_at_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "method": "Explicitly selected USB/ADB device; read-only shell. Raw dumps discarded in memory.",
        "properties": properties,
        "selinux": enforcing if selinux["result"] == "ok" and enforcing in ("Enforcing", "Permissive", "Disabled") else None,
        "bluetooth_manager": {"command_result": bluetooth["result"], **summarize_bluetooth(bluetooth["stdout"] if bluetooth["result"] == "ok" else "")},
        "app_package": {"command_result": package["result"], **summarize_package(package["stdout"] if package["result"] == "ok" else "")},
        "not_tested": ["RC 2 SDP", "HFP service-level connection", "SCO/eSCO", "phone microphone routing", "DJI Fly microphone detection", "RTMP audible speech", "latency/CPU/power"],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--adb", default="adb")
    parser.add_argument("--serial", required=True, help="Explicit target; never recorded in the report")
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    report = collect(args.adb, args.serial)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(f"Sanitized evidence written to {args.output}")
    # Keep a partial evidence report but signal an unavailable target/tool to automation.
    return 0 if all(p["command_result"] == "ok" for p in report["properties"].values()) else 1


if __name__ == "__main__":
    raise SystemExit(main())
