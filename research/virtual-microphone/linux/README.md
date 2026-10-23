# Linux HFP-HF protocol experiment

This is an executable **lab harness**, not a completed RC 2 microphone product. It delegates actual HFP/RFCOMM/SCO/codecs to BlueZ and PipeWire. Hardware success requires the tests below. No Linux host/controller or RC 2 was exercised during development on the available macOS host. The [recorded host preflight](results/host-preflight-2026-10-24.json) reports `UNSUPPORTED_HOST` and `NOT_TESTED`.

## Requirements and local checks

- Linux with a BR/EDR Bluetooth controller supporting usable SCO/eSCO; a lab adapter is allowed for research only.
- BlueZ (`bluetoothctl`, optional `sdptool` and `btmon`), PipeWire built with the BlueZ/native-HFP backend and speech codec support, WirePlumber **0.5+**, `wpctl`, `pw-dump`, `pw-loopback`.
- An active desktop user session with access to BlueZ/PipeWire, an actual ALSA microphone source and Python 3.9+.
- RC 2 with exact firmware and DJI Fly versions recorded. Use a safe stationary drone test and an authorized private RTMP destination.

Install the appropriate distribution packages using its supported package manager. The harness does not install packages, request root, change SELinux/daemon policy or modify controller firmware. Linux monitor access can require administrator privileges; denial means missing evidence.

From the repository root:

```sh
python3 -m unittest discover -s research/virtual-microphone/linux -p 'test_*.py' -v
python3 research/virtual-microphone/linux/hfp_lab.py --help
python3 research/virtual-microphone/linux/hfp_lab.py preflight \
  --output research/virtual-microphone/linux/evidence/01-preflight.json
```

Exit 0 on preflight means prerequisites were observed, **not** profile registration/SCO/RTMP success. Missing dependencies/adapters, an older WirePlumber, unavailable session services or macOS return exit 2. Reports never overwrite existing evidence files; use new names for each attempt. JSON uses mode 0600; a new evidence directory uses 0700.

## Enable the actual local HF role

Review [90-stream-my-drone-hfp.conf](90-stream-my-drone-hfp.conf). It temporarily replaces the local Bluetooth role list with `hfp_hf`, affecting other Bluetooth audio roles until rollback. Restarting the session manager can interrupt desktop audio. Do this deliberately on the Linux lab host:

```sh
(
  set -eu
  install -d -m 700 "$HOME/.config/wireplumber/wireplumber.conf.d"
  test ! -e "$HOME/.config/wireplumber/wireplumber.conf.d/90-stream-my-drone-hfp.conf"
  install -m 600 research/virtual-microphone/linux/90-stream-my-drone-hfp.conf \
    "$HOME/.config/wireplumber/wireplumber.conf.d/90-stream-my-drone-hfp.conf"
  systemctl --user restart wireplumber
)
```

The subshell stops if the existence check fails; preserve existing configuration. WirePlumber 0.4 has a different config system and is outside this harness. Verify local Handsfree `111E` registration/SDP; a config file alone does not prove it loaded. Keep the normal active logind session. [WirePlumber configuration](https://pipewire.pages.freedesktop.org/wireplumber/daemon/configuration/bluetooth.html).

Use interactive `bluetoothctl` for user-confirmed discovery/pairing:

```text
show
system-alias Stream My Drone Microphone
power on
agent KeyboardDisplay
default-agent
pairable on
discoverable on
scan on
```

Keep this session open so its agent remains registered. Record prior alias/discoverable/pairable/power settings for rollback. Confirm the real peer identity/address on RC 2 and matching pairing prompts. A name is only an identity label. Replace the example address with the **RC 2** address:

```text
scan off
pair AA:BB:CC:DD:EE:FF
info AA:BB:CC:DD:EE:FF
```

If RC 2 already initiated successful pairing, skip redundant `pair`. Do not auto-trust unrelated devices. Inspect `Paired`, `ServicesResolved` and UUIDs. Obtain fresh classic SDP where available:

```sh
sdptool browse AA:BB:CC:DD:EE:FF
```

Require remote AG `0000111f-0000-1000-8000-00805f9b34fb`. A failed query is **unknown**, not proof of absent support. To request the remote AG profile on BlueZ versions supporting `connect <device> [uuid]` (check `bluetoothctl help`):

```text
connect AA:BB:CC:DD:EE:FF 0000111f-0000-1000-8000-00805f9b34fb
```

Otherwise initiate from RC 2 and observe the actual negotiation. Generic `Connected: yes` does not prove SLC. [BlueZ Device API](https://bluez.readthedocs.io/en/latest/device-api/).

## Capture actual control/audio evidence

Before pairing, start a monitor in a second terminal with access authorized on that Linux host:

```sh
umask 077
SMD_PRIVATE_DIR="$(mktemp -d)"
btmon --index hci0 --write "$SMD_PRIVATE_DIR/rc2-private.btsnoop"
```

Stop with Ctrl+C. On permission denial, have the lab administrator provide normal approved monitor access. The harness never elevates or modifies policy. Raw snoop may contain keys, names, phone-control data and audio; keep it private, outside the repository. Decode after stopping:

```sh
btmon --read "$SMD_PRIVATE_DIR/rc2-private.btsnoop" --no-pager \
  > "$SMD_PRIVATE_DIR/rc2-private.txt"
python3 research/virtual-microphone/linux/hfp_lab.py redact-text \
  --input "$SMD_PRIVATE_DIR/rc2-private.txt" \
  --output "$SMD_PRIVATE_DIR/rc2-review-before-sharing.txt"
```

Manually reduce to relevant SDP/HFP/synchronous events before sharing. Best-effort redaction removes common addresses/keys/URLs/raw hex but cannot guarantee anonymity. Binary input is rejected. Avoid raw SCO dumps unless a separate audio investigation requires them. Save AT progression, codec selection and synchronous-completion **status/peer/handle**, not only outgoing setup commands. [BlueZ btmon manual](https://github.com/bluez/bluez/blob/master/doc/btmon.rst).

Snapshot before/after SLC, while trying Fly's microphone/start action, during audio and after disconnect, with distinct names:

```sh
python3 research/virtual-microphone/linux/hfp_lab.py snapshot \
  --peer AA:BB:CC:DD:EE:FF \
  --output research/virtual-microphone/linux/evidence/02-after-slc.json
```

Includes sanitized BlueZ/SDP, command failures/timeouts, and selected graph node/link properties. Excludes raw HCI/audio, application process metadata and arbitrary device serials. BlueZ UUIDs may be cached; correlate with fresh SDP. Exported graph snapshots intentionally omit readable node names: select locally using `wpctl status`, `wpctl inspect <id>` or `pw-dump`.

## Route the lab microphone when the graph exists

With local HF -> remote AG, Bluetooth **Audio/Sink is outbound microphone transport**; Bluetooth Audio/Source is incoming RC 2/downlink. Local `hfp_hf` and remote `headset-audio-gateway` are consistent. See [protocol analysis](../HFP_PROTOCOL_ANALYSIS.md).

PipeWire may create the AG sink only after the remote opens audio. If absent after SLC, try the real RC 2/Fly audio action while tracing. Do not invent a sink or claim ready. The helper does not inject ad hoc AT commands or manufacture a phone call. Failure to activate audio remains a prototype blocker.

Inspect live names and run a dry plan:

```sh
wpctl status
wpctl inspect 42
python3 research/virtual-microphone/linux/hfp_lab.py route \
  --peer AA:BB:CC:DD:EE:FF \
  --source alsa_input.REPLACE_WITH_REAL_LOCAL_MICROPHONE \
  --sink bluez_output.REPLACE_WITH_REAL_RC2_AG_SINK
```

Replace names with exact live `node.name`, including actual suffixes. Requires an ALSA Audio/Source and matching peer's AG-family Audio/Sink with CVSD/mSBC, plus remote HFP AG UUID in BlueZ. AG-family metadata alone includes HSP; establish HFP using HF-only configuration and the trace. Dry run prints serial-targeted arguments, not success. Explicit capture:

```sh
python3 research/virtual-microphone/linux/hfp_lab.py route \
  --peer AA:BB:CC:DD:EE:FF \
  --source alsa_input.REPLACE_WITH_REAL_LOCAL_MICROPHONE \
  --sink bluez_output.REPLACE_WITH_REAL_RC2_AG_SINK \
  --execute --max-seconds 300
```

Ctrl+C stops capture. The child loopback terminates on normal exit, TERM, failure, time limit, transport loss or changed serial. Automatic fallback/reconnect is disabled; defaults stay unchanged. Graph query timeout can delay loss cleanup up to that timeout. BlueZ/PipeWire still handle SLC/SCO, requiring separate evidence. No echo cancellation, gain/latency optimization or battery measurements are claimed.

Speak unique random words near the microphone, mute its source and speak again, then unmute/repeat. Keep RC 2 and playback speakers away to exclude acoustic feedback. Save a recording at a separate RTMP receiver. Local meters/running loopback do not prove arrival at DJI Fly.

## Hardware acceptance record

Record firmware/Fly/drone versions, Linux/kernel/BlueZ/PipeWire/WirePlumber versions, controller model, codec and timestamps, without stream keys. For each gate use `NOT_TESTED`, `PASS`, `FAIL` or `BLOCKED` with actual evidence filename/time:

| Gate | Required evidence |
|---|---|
| HF discovery | Local `111E` registration/SDP and RC 2 discovery UI |
| Pairing | Matching prompts and authenticated bond on both ends |
| SLC | Correct HF/AG AT initialization completion |
| SCO/eSCO | Successful synchronous completion with matching peer/handle |
| Microphone PCM | ALSA capture format and active graph links to AG sink |
| Fly detection | RC 2 UI before/after each connection stage |
| RTMP start | Fly started state and received stream |
| Remote audibility | Receiver recording with unique words and mute/unmute control |
| Disconnect/reconnect | Capture stops; observed Fly behavior; deliberate new session result |
| 10-minute stability | Actual duration/dropouts/CPU/xruns/reconnect measurements |

If audio never opens or Fly still demands a microphone, record `FAIL/BLOCKED` with evidence and leave the Linux proof incomplete. Linux success does not prove stock S23, Android screen lock/phone-call behavior, no-root operation or Play distribution. Hash collected JSON reports:

```sh
python3 research/virtual-microphone/linux/hfp_lab.py manifest \
  research/virtual-microphone/linux/evidence
```

Only JSON reports are hashed; raw HCI/media are excluded. Integrity is not a PASS verdict.

## Rollback

Stop helper/monitor. Disconnect only the selected peer, restore prior alias/discoverability/pairability/power settings, and remove only the experiment-created bond if desired. Preserve prior bonds. Remove only this experiment's installed config:

```sh
rm "$HOME/.config/wireplumber/wireplumber.conf.d/90-stream-my-drone-hfp.conf"
systemctl --user restart wireplumber
```

## Development verification

Offline tests cover direction/role/codec/peer rejection, missing/ambiguous identity, redaction/allowlist, command timeouts, non-Linux rejection and no-overwrite evidence. They do not emulate HFP or exercise SCO. AT/codec implementations remain upstream rather than being replaced speculatively.
