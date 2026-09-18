import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import QRCode from "qrcode";
import { useCallback, useEffect, useMemo, useState } from "react";
import { WhepPreview } from "./components/WhepPreview";
import { StatusPill } from "./components/StatusPill";
import {
  activatePreviewFallback,
  activateVirtualCameraExtension,
  configureDestination,
  getDiagnostics,
  openObs,
  openTikTokLiveStudio,
  prepareNativeProduction,
  prepareObs,
  reportPreviewStatus,
  saveObsConnection,
  selectInterface,
  setNativeVirtualCamera,
  setObsVirtualCamera,
  setRecording,
  setNativeRecording,
  startLive,
  startTestDrone,
  stopTestDrone,
  stopLive,
} from "./lib/backend";
import { useBridgeStore } from "./store";
import type { DiagnosticItem } from "./types";

const workflowLabels: Record<string, string> = {
  Idle: "Idle",
  Preparing: "Preparing local bridge",
  WaitingForDrone: "Waiting for drone",
  DroneConnected: "Drone connected",
  PreparingObs: "Preparing production",
  Ready: "Production ready",
  GoingLive: "Going live",
  Live: "Live",
  Stopping: "Stopping",
};

function App() {
  const { snapshot, uiError, initialized, initialize, setUiError, clearUiError } = useBridgeStore();
  const [qrCode, setQrCode] = useState<string>();
  const [previewError, setPreviewError] = useState<string>();
  const [previewEndpoint, setPreviewEndpoint] = useState("http://127.0.0.1:8889/drone/whep");
  const [busy, setBusy] = useState<string>();
  const [obsHost, setObsHost] = useState("127.0.0.1");
  const [obsPort, setObsPort] = useState(4455);
  const [obsPassword, setObsPassword] = useState("");
  const [layout, setLayout] = useState<"Landscape" | "Portrait">("Landscape");
  const [fitMode, setFitMode] = useState<"Fit" | "Fill">("Fit");
  const [diagnostics, setDiagnostics] = useState<DiagnosticItem[]>([]);
  const [destinationMode, setDestinationMode] = useState<"TikTokLiveStudio" | "TikTokRtmp" | "CustomRtmp">("TikTokLiveStudio");
  const [destinationServer, setDestinationServer] = useState("");
  const [streamKey, setStreamKey] = useState("");
  const [microphone, setMicrophone] = useState("");
  const [microphoneMuted, setMicrophoneMuted] = useState(false);
  const [microphoneVolumeDb, setMicrophoneVolumeDb] = useState(0);
  const [microphoneSyncMs, setMicrophoneSyncMs] = useState(0);
  const [noiseSuppression, setNoiseSuppression] = useState(true);
  const [compressor, setCompressor] = useState(true);
  const [limiter, setLimiter] = useState(true);

  useEffect(() => {
    let cleanup: (() => void) | undefined;
    void initialize().then((unlisten) => {
      cleanup = unlisten;
    });
    return () => cleanup?.();
  }, [initialize]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void listen<{ event: string; message: string }>(
      "virtual-camera-extension-event",
      ({ payload }) => {
        if (payload.event === "failed" || payload.event === "debug") {
          setUiError({ code: payload.event.toUpperCase(), message: payload.message, action: "" });
        }
      }
    ).then((fn) => { unlisten = fn; });
    return () => unlisten?.();
  }, [setUiError]);

  useEffect(() => {
    const preferredUrl = snapshot?.rtmpDomainUrl ?? snapshot?.rtmpUrl;
    if (!preferredUrl) {
      setQrCode(undefined);
      return;
    }
    void QRCode.toDataURL(preferredUrl, {
      width: 220,
      margin: 1,
      color: { dark: "#07110fff", light: "#f1f7f3ff" },
    }).then(setQrCode);
  }, [snapshot?.rtmpDomainUrl, snapshot?.rtmpUrl]);

  useEffect(() => {
    if (!snapshot?.publisherPresent) {
      setPreviewEndpoint(snapshot?.preview.directWhepUrl ?? "http://127.0.0.1:8889/drone/whep");
      setPreviewError(undefined);
    }
  }, [snapshot?.preview.directWhepUrl, snapshot?.publisherPresent]);

  const act = useCallback(
    async (name: string, operation: () => Promise<unknown>) => {
      setBusy(name);
      clearUiError();
      try {
        await operation();
      } catch (error) {
        setUiError(error);
      } finally {
        setBusy(undefined);
      }
    },
    [clearUiError, setUiError],
  );

  const handlePreviewConnected = useCallback(() => {
    setPreviewError(undefined);
    void reportPreviewStatus(true);
  }, []);
  const handlePreviewFailure = useCallback((reason: string) => {
    setPreviewError(reason);
    void reportPreviewStatus(false, reason);
  }, []);

  const startFallback = useCallback(
    () =>
      act("fallback", async () => {
        const endpoint = await activatePreviewFallback();
        setPreviewEndpoint(endpoint);
        setPreviewError(undefined);
      }),
    [act],
  );

  const formatted = useMemo(() => {
    const metadata = snapshot?.metadata;
    return {
      bitrate: formatBitrate(metadata?.bitrateCalculatedBps),
      bytes: formatBytes(metadata?.receivedBytes ?? 0),
      uptime: formatDuration(metadata?.uptimeSeconds),
      fps: metadata?.fps ? `${metadata.fps.toFixed(2)} fps` : "Unavailable",
    };
  }, [snapshot?.metadata]);

  if (!initialized || !snapshot) {
    return <main className="boot-screen">Starting the local production bridge…</main>;
  }

  const mediaReady = snapshot.mediaMtx === "Ready";
  const obsVirtualActive = snapshot.obs.virtualCameraActive === true;
  const nativeVirtualActive = snapshot.virtualCamera.feedActive;

  return (
    <main className="app-shell">
      <header className="topbar">
        <div>
          <p className="eyebrow">LOCAL PRODUCTION ROUTER</p>
          <h1>DJI Live Bridge</h1>
        </div>
        <div className="header-status">
          <span className={`pulse ${snapshot.publisherPresent ? "on" : ""}`} />
          <div>
            <strong>{workflowLabels[snapshot.workflow]}</strong>
            <small>{snapshot.lanIpv4 ?? "No LAN IPv4"}</small>
          </div>
        </div>
      </header>

      {(uiError || snapshot.lastError) && (
        <section className="error-banner" role="alert">
          <div>
            <strong>{(uiError ?? snapshot.lastError)?.code}</strong>
            <p>{(uiError ?? snapshot.lastError)?.message}</p>
            <small>{(uiError ?? snapshot.lastError)?.action}</small>
          </div>
          {uiError && <button onClick={clearUiError}>Dismiss</button>}
        </section>
      )}

      {snapshot.ipChangeWarning && (
        <section className="warning-banner">
          LAN IP changed. live.local was updated automatically; update DJI Fly only if you use the IP fallback URL.
        </section>
      )}

      <section className="hero-grid">
        <article className="panel ingest-panel">
          <div className="section-heading">
            <div>
              <p className="step">01 / INGEST</p>
              <h2>RC 2 connection</h2>
            </div>
            <StatusPill label={mediaReady ? "MediaMTX ready" : snapshot.mediaMtx} tone={mediaReady ? "good" : "warn"} />
          </div>

          <label className="field-label" htmlFor="interface">LAN interface</label>
          <select
            id="interface"
            value={snapshot.selectedInterface ?? ""}
            onChange={(event) => void act("interface", () => selectInterface(event.target.value))}
          >
            {snapshot.interfaces.map((item) => (
              <option key={item.name} value={item.name}>
                {item.name} · {item.ipv4}{item.recommended ? " · default route" : ""}
              </option>
            ))}
          </select>

          <div className="url-box">
            <span>DJI Fly RTMP URL · Local domain</span>
            <code>{snapshot.rtmpDomainUrl ?? "Bonjour name unavailable"}</code>
            <button
              disabled={!snapshot.rtmpDomainUrl}
              onClick={() => snapshot.rtmpDomainUrl && void navigator.clipboard.writeText(snapshot.rtmpDomainUrl)}
            >
              Copy URL
            </button>
          </div>

          <div className="url-box secondary-url">
            <span>IP fallback URL</span>
            <code>{snapshot.rtmpUrl ?? "No eligible LAN interface"}</code>
            <button
              disabled={!snapshot.rtmpUrl}
              onClick={() => snapshot.rtmpUrl && void navigator.clipboard.writeText(snapshot.rtmpUrl)}
            >
              Copy IP
            </button>
          </div>
          {snapshot.bonjourDetail && <p className="inline-note">{snapshot.bonjourDetail}</p>}

          <div className="qr-row">
            <div className="qr-shell">{qrCode ? <img src={qrCode} alt="RTMP URL QR code" /> : <span>No URL</span>}</div>
            <div>
              <strong>DJI Fly path</strong>
              <p>GO FLY → Transmission → Live Streaming Platforms → RTMP</p>
              <small>The QR code uses live.local when Bonjour is ready. If RC 2 cannot resolve it, use the IP fallback shown above.</small>
            </div>
          </div>

          <div className="action-row">
            <button
              className="primary"
              disabled={!mediaReady || busy === "test-drone"}
              onClick={() =>
                void act("test-drone", async () => {
                  const selected = await open({
                    multiple: false,
                    directory: false,
                    filters: [{ name: "Video", extensions: ["mp4", "mov", "mkv", "m4v"] }],
                  });
                  if (typeof selected === "string") await startTestDrone(selected);
                })
              }
            >
              Start Test Drone
            </button>
            <button onClick={() => void act("stop-test", stopTestDrone)}>Stop Test Drone</button>
          </div>
        </article>

        <article className="panel preview-panel">
          <div className="section-heading">
            <div>
              <p className="step">02 / PREVIEW</p>
              <h2>Local low-latency monitor</h2>
            </div>
            <StatusPill
              label={snapshot.preview.mode === "Transcoded" ? "Preview transcode" : "Direct WHEP"}
              tone={snapshot.publisherPresent ? "good" : "neutral"}
            />
          </div>
          <WhepPreview
            endpoint={previewEndpoint}
            active={snapshot.publisherPresent}
            onConnected={handlePreviewConnected}
            onFailure={handlePreviewFailure}
          />
          {previewError && (
            <div className="preview-error">
              <div>
                <strong>Direct preview unavailable</strong>
                <p>{previewError}</p>
              </div>
              <button
                className="primary"
                disabled={busy === "fallback"}
                onClick={() => void startFallback()}
              >
                Start preview-only fallback
              </button>
            </div>
          )}
          {!previewError && snapshot.preview.mode === "Direct" && snapshot.metadata.audioCodec === "AAC" && (
            <div className="preview-error audio-warning">
              <div>
                <strong>Preview audio is unavailable</strong>
                <p>MediaMTX is carrying direct H.264 video, but WebRTC does not accept the DJI AAC track.</p>
              </div>
              <button disabled={busy === "fallback"} onClick={() => void startFallback()}>Enable H.264 + Opus fallback</button>
            </div>
          )}
          {snapshot.preview.reason && <p className="inline-note">{snapshot.preview.reason}</p>}
        </article>
      </section>

      <section className="metrics-grid">
        <Metric label="Input resolution" value={snapshot.metadata.resolution ?? "Detecting / unavailable"} note={snapshot.metadata.resolution === "1280x720" ? "Native RC 2 limit; production output may upscale" : "Reported by ffprobe"} />
        <Metric label="Frame rate" value={formatted.fps} note="No assumed FPS" />
        <Metric label="Codecs" value={`${snapshot.metadata.videoCodec ?? "—"} / ${snapshot.metadata.audioCodec ?? "—"}`} note="Video / audio" />
        <Metric label="Input bitrate" value={formatted.bitrate} note="Calculated from byte delta" />
        <Metric label="Received" value={formatted.bytes} note={`Uptime ${formatted.uptime}`} />
        <Metric label="Latency / jitter" value="Unavailable" note="Not measured for RTMP; never estimated" />
      </section>

      <section className="production-grid">
        <article className="panel obs-panel">
          <div className="section-heading">
            <div>
              <p className="step">03 / PRODUCTION</p>
              <h2>{destinationMode === "TikTokLiveStudio" ? "TikTok LIVE Studio camera" : "Built-in FFmpeg production"}</h2>
            </div>
            <StatusPill
              label={destinationMode === "TikTokLiveStudio"
                ? nativeVirtualActive ? "Video ready" : "Video stopped"
                : snapshot.production.active ? `Live · ${snapshot.production.encoder ?? "H.264"}` : snapshot.production.prepared ? "Ready" : "Not prepared"}
              tone={destinationMode === "TikTokLiveStudio"
                ? nativeVirtualActive ? "good" : "neutral"
                : snapshot.production.active || snapshot.production.prepared ? "good" : "neutral"}
            />
          </div>

          {destinationMode === "TikTokLiveStudio" ? (
            <div className="hard-truth live-studio-audio-notice">
              <strong>Audio is handled only by TikTok LIVE Studio</strong>
              <p>DJI Live Bridge sends video only and never captures or forwards a microphone in this mode. In LIVE Studio, set the camera source's Audio capture to None, then select exactly one microphone from TikTok's main microphone control. Use headphones when monitoring to prevent speaker feedback.</p>
            </div>
          ) : (
            <>
              <div className="segmented-row">
                <div>
                  <span className="field-label">Canvas</span>
                  <div className="segmented">
                    <button className={layout === "Landscape" ? "active" : ""} onClick={() => setLayout("Landscape")}>1920 × 1080</button>
                    <button className={layout === "Portrait" ? "active" : ""} onClick={() => setLayout("Portrait")}>1080 × 1920</button>
                  </div>
                </div>
                <div>
                  <span className="field-label">Framing</span>
                  <div className="segmented">
                    <button className={fitMode === "Fit" ? "active" : ""} onClick={() => setFitMode("Fit")}>Fit · no crop</button>
                    <button className={fitMode === "Fill" ? "active" : ""} onClick={() => setFitMode("Fill")}>Fill</button>
                  </div>
                </div>
              </div>

              <div className="audio-controls">
                <label>Commentary microphone
                  <select value={microphone} onChange={(event) => setMicrophone(event.target.value)}>
                    <option value="">Drone audio only</option>
                    {snapshot.audioInputs.map((device) => <option key={device.name} value={device.name}>{device.name}</option>)}
                  </select>
                </label>
                <label>Mic volume (dB)<input type="number" min={-60} max={12} step={1} value={microphoneVolumeDb} onChange={(event) => setMicrophoneVolumeDb(Number(event.target.value))} /></label>
                <label>Sync delay (ms)<input type="number" min={0} max={5000} value={microphoneSyncMs} onChange={(event) => setMicrophoneSyncMs(Math.max(0, Number(event.target.value)))} /></label>
              </div>
              <div className="checkbox-row">
                <label><input type="checkbox" checked={microphoneMuted} onChange={(event) => setMicrophoneMuted(event.target.checked)} /> Mute mic</label>
                <label><input type="checkbox" checked={noiseSuppression} onChange={(event) => setNoiseSuppression(event.target.checked)} /> Noise suppression</label>
                <label><input type="checkbox" checked={compressor} onChange={(event) => setCompressor(event.target.checked)} /> Compressor</label>
                <label><input type="checkbox" checked={limiter} onChange={(event) => setLimiter(event.target.checked)} /> Limiter</label>
              </div>
              <button
                className="primary wide"
                disabled={!snapshot.publisherPresent || snapshot.production.active || busy === "prepare-native"}
                onClick={() => void act("prepare-native", () => prepareNativeProduction({
                  layout,
                  fitMode,
                  microphone: microphone || null,
                  microphoneMuted,
                  microphoneVolumeDb,
                  microphoneSyncMs,
                  noiseSuppression,
                  compressor,
                  limiter,
                }))}
              >
                Prepare built-in production
              </button>
            </>
          )}
          <div className="virtual-cam-row">
            <div>
              <strong>Native recording</strong>
              <p>{snapshot.production.recordingPath ?? "Records the real /production mix when live, otherwise the direct /drone stream."}</p>
            </div>
            <button
              className={snapshot.production.recordingActive ? "danger" : "primary"}
              disabled={!snapshot.publisherPresent}
              onClick={() => void act("native-recording", () => setNativeRecording(!snapshot.production.recordingActive))}
            >
              {snapshot.production.recordingActive ? "Stop Recording" : "Start Recording"}
            </button>
          </div>

          <div className="destination-box">
            <div className="section-heading compact">
              <div>
                <p className="step">DESTINATION</p>
                <h2>{destinationMode === "TikTokLiveStudio" ? "Video-only camera" : "Built-in video + audio mix"}</h2>
              </div>
              <StatusPill
                label={snapshot.production.active ? `Streaming · ${snapshot.production.forwardState ?? "starting"}` : "Stopped"}
                tone={snapshot.production.active ? "good" : "neutral"}
              />
            </div>
            <div className="segmented destination-mode">
              <button className={destinationMode === "TikTokLiveStudio" ? "active" : ""} onClick={() => setDestinationMode("TikTokLiveStudio")}>LIVE Studio</button>
              <button className={destinationMode === "TikTokRtmp" ? "active" : ""} onClick={() => setDestinationMode("TikTokRtmp")}>TikTok RTMP</button>
              <button className={destinationMode === "CustomRtmp" ? "active" : ""} onClick={() => setDestinationMode("CustomRtmp")}>Custom RTMP</button>
            </div>
            {destinationMode === "TikTokLiveStudio" ? (
              <>
                <div className="hard-truth destination-notice">
                  <strong>{snapshot.virtualCamera.deviceName}</strong>
                  <p>{snapshot.virtualCamera.detail ?? "Video-only Core Media I/O camera for TikTok LIVE Studio."}</p>
                  <small>{snapshot.virtualCamera.width} × {snapshot.virtualCamera.height} · {snapshot.virtualCamera.fps} fps · video only. DJI Live Bridge audio is disabled; select one microphone only in LIVE Studio.</small>
                </div>
                <div className="action-row live-actions">
                  {snapshot.virtualCamera.status !== "Ready" ? (
                    <button
                      disabled={!snapshot.virtualCamera.bundled || !snapshot.virtualCamera.appInstalled || busy === "enable-native-camera"}
                      onClick={() => void act("enable-native-camera", activateVirtualCameraExtension)}
                    >
                      Enable virtual camera
                    </button>
                  ) : (
                    <button
                      className={nativeVirtualActive ? "danger" : "primary"}
                      disabled={!snapshot.publisherPresent || busy === "native-camera"}
                      onClick={() => void act("native-camera", () => setNativeVirtualCamera(!nativeVirtualActive))}
                    >
                      {nativeVirtualActive ? "Stop Virtual Camera" : "Start Virtual Camera"}
                    </button>
                  )}
                  <button className="primary" onClick={() => void act("open-live-studio", openTikTokLiveStudio)}>Open / download LIVE Studio</button>
                </div>
                <p className="inline-note">Choose “{snapshot.virtualCamera.deviceName}” as the Camera source, set that source's Audio capture to None, and choose one microphone from LIVE Studio's main audio control. Go Live remains manual.</p>
              </>
            ) : (
              <>
                <div className="destination-fields">
                  <label>Server URL<input value={destinationServer} placeholder="rtmps://…" onChange={(event) => setDestinationServer(event.target.value)} /></label>
                  <label>Stream key<input type="password" value={streamKey} placeholder="Stored only in Keychain" onChange={(event) => setStreamKey(event.target.value)} /></label>
                </div>
                <div className="action-row live-actions">
                  <button onClick={() => void act("save-destination", () => configureDestination(destinationMode, destinationServer, streamKey))}>Save destination</button>
                  {!snapshot.production.active ? (
                    <button className="primary" disabled={snapshot.workflow !== "Ready" || snapshot.production.engine !== "NativeFfmpeg"} onClick={() => void act("start-live", startLive)}>START LIVE</button>
                  ) : (
                    <button className="danger" onClick={() => void act("stop-live", stopLive)}>STOP LIVE</button>
                  )}
                </div>
                <p className="inline-note">The stream key stays in macOS Keychain. START LIVE creates a loopback-only MediaMTX forward; STOP LIVE removes it. The secret is never placed in FFmpeg process arguments.</p>
              </>
            )}
          </div>

          <details className="optional-obs">
            <summary>Optional OBS integration</summary>
            <p className="inline-note">Use OBS only when you need its scene system, filters, recording or Virtual Camera. It is not required for Direct RTMP.</p>
            <div className="obs-fields">
              <label>Host<input value={obsHost} onChange={(event) => setObsHost(event.target.value)} /></label>
              <label>Port<input type="number" value={obsPort} onChange={(event) => setObsPort(Number(event.target.value))} /></label>
              <label className="password-field">Password<input type="password" value={obsPassword} placeholder="Stored in macOS Keychain" onChange={(event) => setObsPassword(event.target.value)} /></label>
            </div>
            <div className="action-row">
              <button onClick={() => void act("save-obs", () => saveObsConnection(obsHost, obsPort, obsPassword))}>Save connection</button>
              <button onClick={() => void act("open-obs", openObs)}>{snapshot.obs.installed ? "Open OBS" : "Open official download"}</button>
              <button disabled={!snapshot.publisherPresent || !snapshot.obs.connected} onClick={() => void act("prepare-obs", () => prepareObs(layout, fitMode))}>Prepare OBS scene</button>
            </div>
            <div className="virtual-cam-row">
              <div><strong>OBS Virtual Camera</strong><p>Video only; destination audio must be selected separately.</p></div>
              <button className={obsVirtualActive ? "danger" : "primary"} disabled={!snapshot.obs.connected || !snapshot.obs.sceneReady} onClick={() => void act("virtual-cam", () => setObsVirtualCamera(!obsVirtualActive))}>{obsVirtualActive ? "Stop Virtual Camera" : "Start Virtual Camera"}</button>
            </div>
            <div className="virtual-cam-row">
              <div><strong>OBS Recording</strong><p>{snapshot.obs.lastRecordingPath ?? "Uses the active OBS profile output path."}</p></div>
              <button className={snapshot.obs.recordingActive ? "danger" : "primary"} disabled={!snapshot.obs.connected || !snapshot.obs.sceneReady} onClick={() => void act("recording", () => setRecording(!snapshot.obs.recordingActive))}>{snapshot.obs.recordingActive ? "Stop OBS Recording" : "Start OBS Recording"}</button>
            </div>
          </details>
        </article>

        <article className="panel diagnostics-panel">
          <div className="section-heading">
            <div>
              <p className="step">SYSTEM</p>
              <h2>Diagnostics</h2>
            </div>
            <button onClick={() => void act("diagnostics", async () => setDiagnostics(await getDiagnostics()))}>Run checks</button>
          </div>
          {diagnostics.length === 0 ? (
            <div className="empty-diagnostics">Checks use current process, API and capability state—no simulated PASS results.</div>
          ) : (
            <div className="diagnostic-list">
              {diagnostics.map((item) => (
                <div className="diagnostic-item" key={item.name}>
                  <StatusPill label={item.level.toUpperCase()} tone={item.level === "Pass" ? "good" : item.level === "Warning" ? "warn" : "bad"} />
                  <div><strong>{item.name}</strong><p>{item.detail}</p>{item.action && <small>{item.action}</small>}</div>
                </div>
              ))}
            </div>
          )}
          <div className="hard-truth">
            <strong>TikTok LIVE Studio on macOS</strong>
            <p>The current official page offers a macOS 12+ build. App discovery/opening is supported; login and the Go Live click remain manual. No private API or bypass is used.</p>
          </div>
        </article>
      </section>
    </main>
  );
}

function Metric({ label, value, note }: { label: string; value: string; note: string }) {
  return <article className="metric"><span>{label}</span><strong>{value}</strong><small>{note}</small></article>;
}

function formatBitrate(value?: number | null) {
  if (value == null) return "Unavailable";
  return value >= 1_000_000 ? `${(value / 1_000_000).toFixed(2)} Mbps` : `${Math.round(value / 1000)} kbps`;
}

function formatBytes(value: number) {
  if (value < 1024) return `${value} B`;
  if (value < 1024 ** 2) return `${(value / 1024).toFixed(1)} KiB`;
  if (value < 1024 ** 3) return `${(value / 1024 ** 2).toFixed(1)} MiB`;
  return `${(value / 1024 ** 3).toFixed(2)} GiB`;
}

function formatDuration(value?: number | null) {
  if (value == null) return "Unavailable";
  const hours = Math.floor(value / 3600);
  const minutes = Math.floor((value % 3600) / 60);
  const seconds = value % 60;
  return [hours, minutes, seconds].map((part) => String(part).padStart(2, "0")).join(":");
}

export default App;
