#!/usr/bin/env python3
"""Linux BlueZ/PipeWire HFP lab helper, with no synthetic Bluetooth success.

Python 3.9+, no third-party modules. Does not pair, change the adapter, install
configuration, elevate privileges or publish a stream. Snapshot output is
sanitized diagnostic evidence, never a hardware acceptance verdict.
"""

import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import signal
import subprocess
import sys
import time


MAC = re.compile(r"(?i)(?<![0-9a-f])(?:[0-9a-f]{2}[:-]){5}[0-9a-f]{2}(?![0-9a-f])")
MAC_PATH = re.compile(r"(?i)(?<![0-9a-f])(?:[0-9a-f]{2}_){5}[0-9a-f]{2}(?![0-9a-f])")
HFP_AG = "0000111f-0000-1000-8000-00805f9b34fb"
ANSI = re.compile(r"\x1b\[[0-?]*[ -/]*[@-~]")
REQUIRED = ("bluetoothctl", "pipewire", "wireplumber", "wpctl", "pw-dump", "pw-loopback")


def now():
    return datetime.datetime.now(datetime.timezone.utc).isoformat()


def sanitize(value):
    """Best-effort sanitization; raw HCI/media must remain local and be reviewed."""
    text = ANSI.sub("", value)
    text = MAC.sub("<BT_ADDRESS>", text)
    text = MAC_PATH.sub("<BT_ADDRESS>", text)
    text = re.sub(r"(?i)\brtmps?://[^\s\"'<>]+", "<RTMP_URL>", text)
    text = re.sub(r"(?i)(bearer\s+)\S+", r"\1<REDACTED>", text)
    text = re.sub(r"(?im)^.*(?:link\s*key|long\s*term\s*key|identity\s*resolving\s*key|passkey|pin\s*code|password|stream.?key|token|secret).*$", "<SENSITIVE_LINE_REMOVED>", text)
    text = re.sub(r"(?im)^\s*(?:[0-9a-f]{2} ){7,}[0-9a-f]{2}(?:\s+.*)?$", "<RAW_HEX_REMOVED>", text)
    text = re.sub(r"(?im)^\s*(?:Data|Key):\s*.*$", "<PAYLOAD_REMOVED>", text)
    text = re.sub(r"(?im)^(\s*(?:Name|Alias|Name \(complete\)|Name \(short\)):\s*).*$", r"\1<DEVICE_NAME>", text)
    text = re.sub(r"(?im)^(\s*(?:Controller|Device) <BT_ADDRESS>)(?:\s+.*)?$", r"\1 <DEVICE_NAME>", text)
    text = re.sub(r"(?:/home|/Users)/[^/\s\"]+", "/<USER>", text)
    return text


def command(argv, timeout=15):
    started = now()
    if not shutil.which(argv[0]):
        return {"command": sanitize(" ".join(argv)), "started_utc": started,
                "status": "unavailable", "exit_code": None, "stdout": "", "stderr": "executable not found"}
    try:
        result = subprocess.run(argv, capture_output=True, text=True, errors="replace",
                                timeout=timeout, check=False, env={**os.environ, "NO_COLOR": "1", "LC_ALL": "C"})
        return {"command": sanitize(" ".join(argv)), "started_utc": started,
                "status": "ok" if result.returncode == 0 else "failed", "exit_code": result.returncode,
                "stdout": result.stdout, "stderr": result.stderr}
    except subprocess.TimeoutExpired as exc:
        def decoded(value):
            return value.decode(errors="replace") if isinstance(value, bytes) else value or ""
        return {"command": sanitize(" ".join(argv)), "started_utc": started,
                "status": "timeout", "exit_code": None,
                "stdout": decoded(exc.stdout), "stderr": decoded(exc.stderr)}


def linux_only():
    if platform.system() != "Linux":
        raise RuntimeError("UNSUPPORTED_HOST: BlueZ/PipeWire HFP experiment requires Linux; no Bluetooth operation was attempted.")


def write_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    # Fail on existing files and on symlinks instead of overwriting evidence.
    with path.open("x", encoding="utf-8") as output:
        os.chmod(path, 0o600)
        json.dump(value, output, indent=2, ensure_ascii=False)
        output.write("\n")


def peer_address(value):
    if not re.fullmatch(r"(?i)(?:[0-9a-f]{2}:){5}[0-9a-f]{2}", value):
        raise argparse.ArgumentTypeError("peer must be a Bluetooth address AA:BB:CC:DD:EE:FF")
    return value.upper()


def graph_nodes(graph):
    return [item for item in graph if item.get("type") == "PipeWire:Interface:Node"]


def node_props(item):
    return item.get("info", {}).get("props", {})


def find_node(graph, name):
    matches = [item for item in graph_nodes(graph) if node_props(item).get("node.name") == name]
    if len(matches) != 1:
        raise RuntimeError("Requested node.name is absent or ambiguous; inspect live `pw-dump` / `wpctl status`.")
    return matches[0]


def route_plan(graph, source_name, sink_name, peer):
    source, sink = find_node(graph, source_name), find_node(graph, sink_name)
    sp, tp = node_props(source), node_props(sink)
    if sp.get("media.class") != "Audio/Source" or not (
            sp.get("device.api") == "alsa" or sp.get("api.alsa.path")):
        raise RuntimeError("Capture target must be a live ALSA Audio/Source microphone, not a Bluetooth downlink or sink monitor.")
    if tp.get("media.class") != "Audio/Sink":
        raise RuntimeError("Outbound HF microphone audio must play into the Bluetooth Audio/Sink; Audio/Source is the AG downlink.")
    if tp.get("api.bluez5.profile") != "headset-audio-gateway":
        raise RuntimeError("Sink does not identify a remote headset-audio-gateway transport. Refusing A2DP/wrong-role routing.")
    if str(tp.get("api.bluez5.address", "")).upper() != peer:
        raise RuntimeError("Sink Bluetooth peer does not match the requested RC 2 address.")
    if str(tp.get("api.bluez5.codec", "")).lower() not in ("cvsd", "msbc"):
        raise RuntimeError("Transport does not expose CVSD or mSBC. Capture evidence and inspect the negotiated codec first.")
    # Stable serials prevent numeric node-id reuse and name reuse after disconnect.
    if not sp.get("object.serial") or not tp.get("object.serial"):
        raise RuntimeError("PipeWire object.serial missing; refusing an unstable target selection.")
    args = ["pw-loopback", "--name=smd-hfp-microphone-lab", "--channels=1", "--channel-map=MONO",
            "--capture=" + str(sp["object.serial"]), "--playback=" + str(tp["object.serial"]),
            '--capture-props={"node.dont-reconnect":true,"node.dont-fallback":true}',
            '--playback-props={"node.dont-reconnect":true,"node.dont-fallback":true}']
    return {"source_id": source["id"], "sink_id": sink["id"],
            "source_serial": sp["object.serial"], "sink_serial": tp["object.serial"],
            "codec": tp["api.bluez5.codec"], "command": args,
            "meaning": "Graph can route local capture to a remote AG transport; this is not proof of SCO delivery or RTMP audibility."}


def live_graph():
    result = command(["pw-dump"])
    if result["status"] != "ok":
        raise RuntimeError("pw-dump failed: " + sanitize(result["stderr"]))
    graph = json.loads(result["stdout"])
    if not isinstance(graph, list):
        raise RuntimeError("Unexpected pw-dump shape")
    return graph


def summarized_graph(graph):
    # An allowlist avoids collecting application names, device serial numbers,
    # arbitrary metadata, media titles, process arguments and paths.
    keys = ("media.class", "device.api", "api.bluez5.profile", "api.bluez5.codec",
            "api.bluez5.address", "object.serial", "device.id", "audio.channels", "audio.rate")
    nodes = []
    for item in graph_nodes(graph):
        props = node_props(item)
        nodes.append({"id": item["id"], "state": item.get("info", {}).get("state", "unknown"),
                      "props": {key: sanitize(str(props[key])) for key in keys if key in props}})
    links = []
    for item in graph:
        if item.get("type") == "PipeWire:Interface:Link":
            info = item.get("info", {})
            links.append({key: sanitize(info[key]) if isinstance(info[key], str) else info[key]
                          for key in ("output-node-id", "input-node-id", "state", "error") if key in info})
    return {"nodes": nodes, "links": links}


def preflight(output):
    report = {"created_utc": now(), "host_os": platform.system(),
              "host_release": platform.release(), "machine": platform.machine(),
              "hardware_verdict": "NOT_TESTED", "tools": {name: bool(shutil.which(name)) for name in REQUIRED},
              "notes": ["No pairing, discovery, SCO audio or livestream is started by preflight."]}
    if platform.system() != "Linux":
        report["status"] = "UNSUPPORTED_HOST"
        write_json(output, report)
        print("UNSUPPORTED_HOST: Linux lab not run; report written.")
        return 2
    checks = []
    for args in (["bluetoothctl", "--version"], ["pipewire", "--version"], ["wireplumber", "--version"],
                 ["bluetoothctl", "show"], ["systemctl", "--user", "is-active", "pipewire", "wireplumber"]):
        item = command(args)
        checks.append({**item, "stdout": sanitize(item["stdout"]), "stderr": sanitize(item["stderr"])})
    report["checks"] = checks
    adapters = list(Path("/sys/class/bluetooth").glob("hci*"))
    report["controllers"] = [item.name for item in adapters]
    wp_version = checks[2]["stdout"]
    version = re.search(r"wireplumber\s+(\d+)\.(\d+)\.(\d+)", wp_version, re.I)
    report["wireplumber_05_or_newer"] = bool(version and tuple(map(int, version.groups())) >= (0, 5, 0))
    ready = bool(adapters) and all(report["tools"].values()) and report["wireplumber_05_or_newer"] and all(
        check["status"] == "ok" for check in checks)
    report["status"] = "PREREQUISITES_PRESENT" if ready else "PREREQUISITES_MISSING"
    write_json(output, report)
    print(report["status"] + ": hardware acceptance remains NOT_TESTED.")
    return 0 if ready else 2


def snapshot(peer, output):
    linux_only()
    checks = []
    for args in (["bluetoothctl", "show"], ["bluetoothctl", "info", peer],
                 ["sdptool", "browse", peer]):
        item = command(args, timeout=25)
        checks.append({**item, "stdout": sanitize(item["stdout"]), "stderr": sanitize(item["stderr"])})
    graph_result = command(["pw-dump"])
    try:
        graph = summarized_graph(json.loads(graph_result["stdout"])) if graph_result["status"] == "ok" else None
    except (ValueError, TypeError, KeyError) as exc:
        graph = {"error": type(exc).__name__}
    report = {"created_utc": now(), "host_os": platform.system(), "peer": "<BT_ADDRESS>",
              "hardware_verdict": "NOT_TESTED", "checks": checks, "pipewire_graph": graph,
              "pw_dump_status": graph_result["status"], "pw_dump_stderr": sanitize(graph_result["stderr"]),
              "notes": ["Command success is not HFP SLC/SCO success. Review SDP/HCI, graph, Fly UI and remote recording independently.",
                        "No raw HCI binary or audio collected; sanitized text still needs human review before sharing."]}
    write_json(output, report)
    print("Snapshot written; hardware acceptance remains NOT_TESTED.")
    return 0


def route(args):
    linux_only()
    info = command(["bluetoothctl", "info", args.peer])
    if info["status"] != "ok" or HFP_AG not in info["stdout"].lower():
        raise RuntimeError("No remote HFP AG UUID observed through bluetoothctl info. Collect SDP before routing.")
    plan = route_plan(live_graph(), args.source, args.sink, args.peer)
    if not args.execute:
        print(json.dumps(plan, indent=2))
        print("DRY_RUN: no audio capture started. --execute captures the chosen microphone until Ctrl+C/disconnect.")
        return 0
    if not shutil.which("pw-loopback"):
        raise RuntimeError("pw-loopback not installed")
    print("Starting the selected microphone -> RC 2 AG sink loopback. Verify SCO + remote audibility separately. Ctrl+C stops it.")
    process = subprocess.Popen(plan["command"], start_new_session=True)
    started = time.monotonic()
    try:
        while process.poll() is None:
            time.sleep(1)
            current = route_plan(live_graph(), args.source, args.sink, args.peer)
            if (current["source_serial"], current["sink_serial"]) != (plan["source_serial"], plan["sink_serial"]):
                raise RuntimeError("Source/transport replaced: stopped capture; select and verify the new transport explicitly.")
            if args.max_seconds and time.monotonic() - started >= args.max_seconds:
                print("Experiment time limit reached; stopping microphone.")
                break
        if process.poll() not in (None, 0):
            raise RuntimeError("pw-loopback exited with code " + str(process.returncode))
    finally:
        if process.poll() is None:
            os.killpg(process.pid, signal.SIGTERM)
            try:
                process.wait(timeout=3)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait()
    return 0


def redact(args):
    # Accept decoded text only: binary HCI data can contain keys and audio.
    data = args.input.read_bytes()
    if b"\x00" in data:
        raise RuntimeError("Binary input rejected; decode locally with btmon --read first. Raw trace must remain private.")
    value = sanitize(data.decode("utf-8", errors="strict"))
    args.output.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    with args.output.open("x", encoding="utf-8") as output:
        os.chmod(args.output, 0o600)
        output.write(value)
    print("Best-effort redaction written; manually remove raw payloads, names and secrets before sharing.")
    return 0


def manifest(directory):
    if not directory.is_dir():
        raise RuntimeError("Evidence directory does not exist")
    entries = []
    for path in sorted(directory.iterdir()):
        if path.is_symlink() or path.suffix != ".json" or path.name == "manifest.json" or not path.is_file():
            continue
        entries.append({"file": path.name, "sha256": hashlib.sha256(path.read_bytes()).hexdigest()})
    write_json(directory / "manifest.json", {"created_utc": now(), "files": entries,
               "acceptance_verdict": "NOT_TESTED", "note": "Hashes preserve evidence integrity; they do not validate its interpretation."})
    print("Manifest written for JSON reports only; no raw traces/media included.")
    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="action", required=True)
    for action in ("preflight", "snapshot"):
        child = commands.add_parser(action)
        child.add_argument("--output", required=True, type=Path)
        if action == "snapshot":
            child.add_argument("--peer", required=True, type=peer_address)
    child = commands.add_parser("route", help="Inspect then optionally route a live ALSA mic to a verified remote-AG sink")
    child.add_argument("--peer", required=True, type=peer_address)
    child.add_argument("--source", required=True, help="Exact live ALSA microphone node.name")
    child.add_argument("--sink", required=True, help="Exact live remote AG sink node.name")
    child.add_argument("--execute", action="store_true")
    child.add_argument("--max-seconds", default=300, type=int, help="Capture duration limit (default 300, 0 means until stopped)")
    child = commands.add_parser("redact-text")
    child.add_argument("--input", required=True, type=Path)
    child.add_argument("--output", required=True, type=Path)
    child = commands.add_parser("manifest")
    child.add_argument("directory", type=Path)
    args = parser.parse_args()
    def stopped(_signum, _frame):
        raise KeyboardInterrupt
    signal.signal(signal.SIGTERM, stopped)
    try:
        if args.action == "preflight":
            return preflight(args.output)
        if args.action == "snapshot":
            return snapshot(args.peer, args.output)
        if args.action == "route":
            if args.max_seconds < 0:
                parser.error("--max-seconds must be >= 0")
            return route(args)
        if args.action == "redact-text":
            return redact(args)
        return manifest(args.directory)
    except KeyboardInterrupt:
        print("Stopped by operator; no acceptance verdict inferred.", file=sys.stderr)
        return 130
    except (RuntimeError, OSError, ValueError) as exc:
        print(sanitize(str(exc)), file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
