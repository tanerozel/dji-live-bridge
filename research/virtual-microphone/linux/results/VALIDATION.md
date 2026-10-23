# Validation recorded on 2026-10-24 (Europe/Istanbul)

- Python 3.9 offline tests: **12 passed** (`python3 -m unittest discover -s research/virtual-microphone/linux -p 'test_*.py' -v`). Includes direction/role/peer/codec/identity checks, redaction, timeout/error behavior, evidence file protection, and stopping the owned loopback child on disconnect.
- Python syntax compilation: **passed** using a writable temporary bytecode cache (`PYTHONPYCACHEPREFIX=/private/tmp/smd-hfp-pycache`). The initial default-cache attempt was denied by the macOS filesystem sandbox; no permission change was made.
- CLI `--help`: **passed**.
- Real host preflight: **UNSUPPORTED_HOST**, expected exit 2. Host is Darwin/arm64; Linux tools were absent. [Machine-readable report](host-preflight-2026-10-24.json). Its UTC timestamp is on the prior UTC calendar date; local test date was 2026-10-24.
- BlueZ/WirePlumber config load: **NOT TESTED**; no Linux session was available.
- RC 2 SDP/HFP SLC/SCO/eSCO/codec negotiation: **NOT TESTED**.
- Lab microphone to RC 2 / DJI Fly detection / RTMP audibility: **NOT TESTED**.

No HFP connection log, audio-performance measurement or hardware PASS is generated from offline fixtures. The Linux experiment must still run on real hardware before Phase 2 can be considered proven.
