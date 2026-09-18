import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import QRCode from "qrcode";
import { useCallback, useEffect, useMemo, useState } from "react";
import { WhepPreview } from "./components/WhepPreview";
import { StatusPill } from "./components/StatusPill";
import { loadLanguage, saveLanguage, translate, type Language, type TranslationParams } from "./i18n";
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
  const [language, setLanguage] = useState<Language>(loadLanguage);
  const t = useCallback(
    (key: string, params?: TranslationParams) => translate(language, key, params),
    [language],
  );

  useEffect(() => saveLanguage(language), [language]);

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
          setUiError({
            code: payload.event.toUpperCase(),
            messageKey: "errors.message.virtualCamera",
            detail: payload.message,
            actionKey: "errors.action.virtualCamera",
          });
        }
      }
    ).then((fn) => { unlisten = fn; });
    return () => unlisten?.();
  }, [setUiError]);

  useEffect(() => {
    if (!snapshot?.rtmpUrl) {
      setQrCode(undefined);
      return;
    }
    void QRCode.toDataURL(snapshot.rtmpUrl, {
      width: 220,
      margin: 1,
      color: { dark: "#07110fff", light: "#f1f7f3ff" },
    }).then(setQrCode);
  }, [snapshot?.rtmpUrl]);

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
      bitrate: formatBitrate(metadata?.bitrateCalculatedBps, t("common.unavailable")),
      bytes: formatBytes(metadata?.receivedBytes ?? 0),
      uptime: formatDuration(metadata?.uptimeSeconds, t("common.unavailable")),
      fps: metadata?.fps ? `${metadata.fps.toFixed(2)} fps` : t("common.unavailable"),
    };
  }, [snapshot?.metadata, t]);

  if (!initialized || !snapshot) {
    return <main className="boot-screen">{t("workflow.Preparing")}</main>;
  }

  const mediaReady = snapshot.mediaMtx === "Ready";
  const obsVirtualActive = snapshot.obs.virtualCameraActive === true;
  const nativeVirtualActive = snapshot.virtualCamera.feedActive;

  return (
    <main className="app-shell">
      <header className="topbar">
        <div>
          <p className="eyebrow">{t("header.eyebrow")}</p>
          <h1>DJI Live Bridge</h1>
        </div>
        <div className="header-tools">
          <label className="language-picker">
            <span>{t("language.label")}</span>
            <select value={language} onChange={(event) => setLanguage(event.target.value as Language)}>
              <option value="en">{t("language.en")}</option>
              <option value="tr">{t("language.tr")}</option>
            </select>
          </label>
          <div className="header-status">
            <span className={`pulse ${snapshot.publisherPresent ? "on" : ""}`} />
            <div>
              <strong>{t(`workflow.${snapshot.workflow}`)}</strong>
              <small>{snapshot.lanIpv4 ?? t("header.noLan")}</small>
            </div>
          </div>
        </div>
      </header>

      {(uiError || snapshot.lastError) && (
        <section className="error-banner" role="alert">
          <div>
            <strong>{t((uiError ?? snapshot.lastError)!.messageKey)} · {(uiError ?? snapshot.lastError)!.code}</strong>
            <p>{t((uiError ?? snapshot.lastError)!.actionKey)}</p>
            <details className="technical-details">
              <summary>{t("common.technicalDetails")}</summary>
              <code>{(uiError ?? snapshot.lastError)!.detail}</code>
            </details>
          </div>
          {uiError && <button onClick={clearUiError}>{t("common.dismiss")}</button>}
        </section>
      )}

      {snapshot.ipChangeWarning && (
        <section className="warning-banner">
          {t("warning.ipChanged")}
        </section>
      )}

      <section className="hero-grid">
        <article className="panel ingest-panel">
          <div className="section-heading">
            <div>
              <p className="step">{t("ingest.step")}</p>
              <h2>{t("ingest.title")}</h2>
            </div>
            <StatusPill label={mediaReady ? t("ingest.mediaReady") : t(`common.${snapshot.mediaMtx.toLowerCase()}`)} tone={mediaReady ? "good" : "warn"} />
          </div>

          <label className="field-label" htmlFor="interface">{t("ingest.interface")}</label>
          <select
            id="interface"
            value={snapshot.selectedInterface ?? ""}
            onChange={(event) => void act("interface", () => selectInterface(event.target.value))}
          >
            {snapshot.interfaces.map((item) => (
              <option key={item.name} value={item.name}>
                {item.name} · {item.ipv4}{item.recommended ? ` · ${t("ingest.defaultRoute")}` : ""}
              </option>
            ))}
          </select>

          <div className="url-box">
            <span>{t("ingest.ipLabel")}</span>
            <code>{snapshot.rtmpUrl ?? t("ingest.noInterface")}</code>
            <button
              disabled={!snapshot.rtmpUrl}
              onClick={() => snapshot.rtmpUrl && void navigator.clipboard.writeText(snapshot.rtmpUrl)}
            >
              {t("ingest.copyIp")}
            </button>
          </div>
          <div className="qr-row">
            <div className="qr-shell">{qrCode ? <img src={qrCode} alt={t("ingest.qrAlt")} /> : <span>{t("common.noUrl")}</span>}</div>
            <div>
              <strong>{t("ingest.path")}</strong>
              <p>{t("ingest.pathSteps")}</p>
              <small>{t("ingest.qrHelp")}</small>
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
                    filters: [{ name: t("ingest.videoFilter"), extensions: ["mp4", "mov", "mkv", "m4v"] }],
                  });
                  if (typeof selected === "string") await startTestDrone(selected);
                })
              }
            >
              {t("ingest.startTest")}
            </button>
            <button onClick={() => void act("stop-test", stopTestDrone)}>{t("ingest.stopTest")}</button>
          </div>
        </article>

        <article className="panel preview-panel">
          <div className="section-heading">
            <div>
              <p className="step">{t("preview.step")}</p>
              <h2>{t("preview.title")}</h2>
            </div>
            <StatusPill
              label={snapshot.preview.mode === "Transcoded" ? t("preview.transcode") : t("preview.direct")}
              tone={snapshot.publisherPresent ? "good" : "neutral"}
            />
          </div>
          <WhepPreview
            endpoint={previewEndpoint}
            active={snapshot.publisherPresent}
            onConnected={handlePreviewConnected}
            onFailure={handlePreviewFailure}
            t={t}
          />
          {previewError && (
            <div className="preview-error">
              <div>
                <strong>{t("preview.unavailable")}</strong>
                <p>{t("preview.reason.failed")}</p>
                <details className="technical-details">
                  <summary>{t("common.technicalDetails")}</summary>
                  <code>{previewError}</code>
                </details>
              </div>
              <button
                className="primary"
                disabled={busy === "fallback"}
                onClick={() => void startFallback()}
              >
                {t("preview.startFallback")}
              </button>
            </div>
          )}
          {!previewError && snapshot.preview.mode === "Direct" && snapshot.metadata.audioCodec === "AAC" && (
            <div className="preview-error audio-warning">
              <div>
                <strong>{t("preview.audioUnavailable")}</strong>
                <p>{t("preview.aacExplanation")}</p>
              </div>
              <button disabled={busy === "fallback"} onClick={() => void startFallback()}>{t("preview.enableFallback")}</button>
            </div>
          )}
          {snapshot.preview.reasonKey && <p className="inline-note">{t(snapshot.preview.reasonKey)}</p>}
        </article>
      </section>

      <section className="metrics-grid">
        <Metric label={t("metrics.resolution")} value={snapshot.metadata.resolution ?? t("common.detecting")} note={snapshot.metadata.resolution === "1280x720" ? t("metrics.resolutionNative") : t("metrics.reported")} />
        <Metric label={t("metrics.frameRate")} value={formatted.fps} note={t("metrics.noAssumedFps")} />
        <Metric label={t("metrics.codecs")} value={`${snapshot.metadata.videoCodec ?? "—"} / ${snapshot.metadata.audioCodec ?? "—"}`} note={t("metrics.videoAudio")} />
        <Metric label={t("metrics.bitrate")} value={formatted.bitrate} note={t("metrics.calculated")} />
        <Metric label={t("metrics.received")} value={formatted.bytes} note={t("metrics.uptime", { value: formatted.uptime })} />
        <Metric label={t("metrics.latency")} value={t("common.unavailable")} note={t("metrics.latencyUnavailable")} />
      </section>

      <section className="production-grid">
        <article className="panel obs-panel">
          <div className="section-heading">
            <div>
              <p className="step">{t("production.step")}</p>
              <h2>{destinationMode === "TikTokLiveStudio" ? t("production.liveStudioTitle") : t("production.nativeTitle")}</h2>
            </div>
            <StatusPill
              label={destinationMode === "TikTokLiveStudio"
                ? nativeVirtualActive ? t("production.videoReady") : t("production.videoStopped")
                : snapshot.production.active ? t("production.live", { encoder: snapshot.production.encoder ?? "H.264" }) : snapshot.production.prepared ? t("common.ready") : t("production.notPrepared")}
              tone={destinationMode === "TikTokLiveStudio"
                ? nativeVirtualActive ? "good" : "neutral"
                : snapshot.production.active || snapshot.production.prepared ? "good" : "neutral"}
            />
          </div>

          {destinationMode === "TikTokLiveStudio" ? (
            <div className="hard-truth live-studio-audio-notice">
              <strong>{t("audio.title")}</strong>
              <p>{t("audio.liveStudioHelp")}</p>
            </div>
          ) : (
            <>
              <div className="segmented-row">
                <div>
                  <span className="field-label">{t("production.canvas")}</span>
                  <div className="segmented">
                    <button className={layout === "Landscape" ? "active" : ""} onClick={() => setLayout("Landscape")}>1920 × 1080</button>
                    <button className={layout === "Portrait" ? "active" : ""} onClick={() => setLayout("Portrait")}>1080 × 1920</button>
                  </div>
                </div>
                <div>
                  <span className="field-label">{t("production.framing")}</span>
                  <div className="segmented">
                    <button className={fitMode === "Fit" ? "active" : ""} onClick={() => setFitMode("Fit")}>{t("production.fit")}</button>
                    <button className={fitMode === "Fill" ? "active" : ""} onClick={() => setFitMode("Fill")}>{t("production.fill")}</button>
                  </div>
                </div>
              </div>

              <div className="audio-controls">
                <label>{t("audio.commentary")}
                  <select value={microphone} onChange={(event) => setMicrophone(event.target.value)}>
                    <option value="">{t("audio.droneOnly")}</option>
                    {snapshot.audioInputs.map((device) => <option key={device.name} value={device.name}>{device.name}</option>)}
                  </select>
                </label>
                <label>{t("audio.volume")}<input type="number" min={-60} max={12} step={1} value={microphoneVolumeDb} onChange={(event) => setMicrophoneVolumeDb(Number(event.target.value))} /></label>
                <label>{t("audio.delay")}<input type="number" min={0} max={5000} value={microphoneSyncMs} onChange={(event) => setMicrophoneSyncMs(Math.max(0, Number(event.target.value)))} /></label>
              </div>
              <div className="checkbox-row">
                <label><input type="checkbox" checked={microphoneMuted} onChange={(event) => setMicrophoneMuted(event.target.checked)} /> {t("audio.mute")}</label>
                <label><input type="checkbox" checked={noiseSuppression} onChange={(event) => setNoiseSuppression(event.target.checked)} /> {t("audio.noiseSuppression")}</label>
                <label><input type="checkbox" checked={compressor} onChange={(event) => setCompressor(event.target.checked)} /> {t("audio.compressor")}</label>
                <label><input type="checkbox" checked={limiter} onChange={(event) => setLimiter(event.target.checked)} /> {t("audio.limiter")}</label>
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
                {t("production.prepare")}
              </button>
            </>
          )}
          <div className="virtual-cam-row">
            <div>
              <strong>{t("recording.native")}</strong>
              <p>{snapshot.production.recordingPath ?? t("recording.nativeHelp")}</p>
            </div>
            <button
              className={snapshot.production.recordingActive ? "danger" : "primary"}
              disabled={!snapshot.publisherPresent}
              onClick={() => void act("native-recording", () => setNativeRecording(!snapshot.production.recordingActive))}
            >
              {snapshot.production.recordingActive ? t("recording.stop") : t("recording.start")}
            </button>
          </div>

          <div className="destination-box">
            <div className="section-heading compact">
              <div>
                <p className="step">{t("destination.step")}</p>
                <h2>{destinationMode === "TikTokLiveStudio" ? t("destination.videoOnly") : t("destination.mix")}</h2>
              </div>
              <StatusPill
                label={snapshot.production.active ? t("destination.streaming", { state: snapshot.production.forwardState ?? t("common.starting") }) : t("common.stopped")}
                tone={snapshot.production.active ? "good" : "neutral"}
              />
            </div>
            <div className="segmented destination-mode">
              <button className={destinationMode === "TikTokLiveStudio" ? "active" : ""} onClick={() => setDestinationMode("TikTokLiveStudio")}>{t("destination.liveStudio")}</button>
              <button className={destinationMode === "TikTokRtmp" ? "active" : ""} onClick={() => setDestinationMode("TikTokRtmp")}>{t("destination.tiktokRtmp")}</button>
              <button className={destinationMode === "CustomRtmp" ? "active" : ""} onClick={() => setDestinationMode("CustomRtmp")}>{t("destination.customRtmp")}</button>
            </div>
            {destinationMode === "TikTokLiveStudio" ? (
              <>
                <div className="hard-truth destination-notice">
                  <strong>{snapshot.virtualCamera.deviceName}</strong>
                  <p>{t(snapshot.virtualCamera.detailKey)}</p>
                  <small>{t("camera.videoOnly", { width: snapshot.virtualCamera.width, height: snapshot.virtualCamera.height, fps: snapshot.virtualCamera.fps })}</small>
                </div>
                <div className="action-row live-actions">
                  {snapshot.virtualCamera.status !== "Ready" ? (
                    <button
                      disabled={!snapshot.virtualCamera.bundled || !snapshot.virtualCamera.appInstalled || busy === "enable-native-camera"}
                      onClick={() => void act("enable-native-camera", activateVirtualCameraExtension)}
                    >
                      {t("camera.enable")}
                    </button>
                  ) : (
                    <button
                      className={nativeVirtualActive ? "danger" : "primary"}
                      disabled={!snapshot.publisherPresent || busy === "native-camera"}
                      onClick={() => void act("native-camera", () => setNativeVirtualCamera(!nativeVirtualActive))}
                    >
                      {nativeVirtualActive ? t("camera.stop") : t("camera.start")}
                    </button>
                  )}
                  <button className="primary" onClick={() => void act("open-live-studio", openTikTokLiveStudio)}>{t("camera.openStudio")}</button>
                </div>
                <p className="inline-note">{t("camera.liveStudioHelp", { device: snapshot.virtualCamera.deviceName })}</p>
              </>
            ) : (
              <>
                <div className="destination-fields">
                  <label>{t("rtmp.server")}<input value={destinationServer} placeholder="rtmps://…" onChange={(event) => setDestinationServer(event.target.value)} /></label>
                  <label>{t("rtmp.streamKey")}<input type="password" value={streamKey} placeholder={t("rtmp.keychainPlaceholder")} onChange={(event) => setStreamKey(event.target.value)} /></label>
                </div>
                <div className="action-row live-actions">
                  <button onClick={() => void act("save-destination", () => configureDestination(destinationMode, destinationServer, streamKey))}>{t("rtmp.save")}</button>
                  {!snapshot.production.active ? (
                    <button className="primary" disabled={snapshot.workflow !== "Ready" || snapshot.production.engine !== "NativeFfmpeg"} onClick={() => void act("start-live", startLive)}>{t("rtmp.start")}</button>
                  ) : (
                    <button className="danger" onClick={() => void act("stop-live", stopLive)}>{t("rtmp.stop")}</button>
                  )}
                </div>
                <p className="inline-note">{t("rtmp.securityHelp")}</p>
              </>
            )}
          </div>

          <details className="optional-obs">
            <summary>{t("obs.summary")}</summary>
            <p className="inline-note">{t("obs.help")}</p>
            <div className="obs-fields">
              <label>{t("obs.host")}<input value={obsHost} onChange={(event) => setObsHost(event.target.value)} /></label>
              <label>{t("obs.port")}<input type="number" value={obsPort} onChange={(event) => setObsPort(Number(event.target.value))} /></label>
              <label className="password-field">{t("obs.password")}<input type="password" value={obsPassword} placeholder={t("rtmp.keychainPlaceholder")} onChange={(event) => setObsPassword(event.target.value)} /></label>
            </div>
            <div className="action-row">
              <button onClick={() => void act("save-obs", () => saveObsConnection(obsHost, obsPort, obsPassword))}>{t("obs.save")}</button>
              <button onClick={() => void act("open-obs", openObs)}>{snapshot.obs.installed ? t("obs.open") : t("obs.download")}</button>
              <button disabled={!snapshot.publisherPresent || !snapshot.obs.connected} onClick={() => void act("prepare-obs", () => prepareObs(layout, fitMode))}>{t("obs.prepare")}</button>
            </div>
            <div className="virtual-cam-row">
              <div><strong>{t("obs.camera")}</strong><p>{t("obs.cameraHelp")}</p></div>
              <button className={obsVirtualActive ? "danger" : "primary"} disabled={!snapshot.obs.connected || !snapshot.obs.sceneReady} onClick={() => void act("virtual-cam", () => setObsVirtualCamera(!obsVirtualActive))}>{obsVirtualActive ? t("camera.stop") : t("camera.start")}</button>
            </div>
            <div className="virtual-cam-row">
              <div><strong>{t("obs.recording")}</strong><p>{snapshot.obs.lastRecordingPath ?? t("obs.recordingHelp")}</p></div>
              <button className={snapshot.obs.recordingActive ? "danger" : "primary"} disabled={!snapshot.obs.connected || !snapshot.obs.sceneReady} onClick={() => void act("recording", () => setRecording(!snapshot.obs.recordingActive))}>{snapshot.obs.recordingActive ? t("obs.stopRecording") : t("obs.startRecording")}</button>
            </div>
          </details>
        </article>

        <article className="panel diagnostics-panel">
          <div className="section-heading">
            <div>
              <p className="step">{t("diagnostics.step")}</p>
              <h2>{t("diagnostics.title")}</h2>
            </div>
            <button onClick={() => void act("diagnostics", async () => setDiagnostics(await getDiagnostics()))}>{t("diagnostics.run")}</button>
          </div>
          {diagnostics.length === 0 ? (
            <div className="empty-diagnostics">{t("diagnostics.empty")}</div>
          ) : (
            <div className="diagnostic-list">
              {diagnostics.map((item) => (
                <div className="diagnostic-item" key={item.id}>
                  <StatusPill label={t(`common.${item.level.toLowerCase()}`)} tone={item.level === "Pass" ? "good" : item.level === "Warning" ? "warn" : "bad"} />
                  <div>
                    <strong>{t(item.nameKey)}</strong>
                    <p>{t(item.detailKey)}</p>
                    <details className="technical-details diagnostic-technical">
                      <summary>{t("common.technicalDetails")}</summary>
                      <code>{item.technicalDetail}</code>
                    </details>
                    {item.actionKey && <small>{t(item.actionKey)}</small>}
                  </div>
                </div>
              ))}
            </div>
          )}
          <div className="hard-truth">
            <strong>{t("diagnostics.tiktokTitle")}</strong>
            <p>{t("diagnostics.tiktokHelp")}</p>
          </div>
        </article>
      </section>
    </main>
  );
}

function Metric({ label, value, note }: { label: string; value: string; note: string }) {
  return <article className="metric"><span>{label}</span><strong>{value}</strong><small>{note}</small></article>;
}

function formatBitrate(value: number | null | undefined, unavailable: string) {
  if (value == null) return unavailable;
  return value >= 1_000_000 ? `${(value / 1_000_000).toFixed(2)} Mbps` : `${Math.round(value / 1000)} kbps`;
}

function formatBytes(value: number) {
  if (value < 1024) return `${value} B`;
  if (value < 1024 ** 2) return `${(value / 1024).toFixed(1)} KiB`;
  if (value < 1024 ** 3) return `${(value / 1024 ** 2).toFixed(1)} MiB`;
  return `${(value / 1024 ** 3).toFixed(2)} GiB`;
}

function formatDuration(value: number | null | undefined, unavailable: string) {
  if (value == null) return unavailable;
  const hours = Math.floor(value / 3600);
  const minutes = Math.floor((value % 3600) / 60);
  const seconds = value % 60;
  return [hours, minutes, seconds].map((part) => String(part).padStart(2, "0")).join(":");
}

export default App;
