import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import QRCode from "qrcode";
import { useCallback, useEffect, useMemo, useState, type ReactNode } from "react";
import { WhepPreview } from "./components/WhepPreview";
import { StatusPill } from "./components/StatusPill";
import { loadLanguage, saveLanguage, translate, type Language, type TranslationParams } from "./i18n";
import { THEMES, applyTheme, loadTheme, saveTheme, watchSystemTheme, type Theme } from "./theme";
import {
  activatePreviewFallback,
  activateVirtualCameraExtension,
  getDiagnostics,
  openObs,
  openTikTokLiveStudio,
  prepareNativeProduction,
  prepareObs,
  reportPreviewStatus,
  removeRtmpDestination,
  saveObsConnection,
  selectInterface,
  setNativeVirtualCamera,
  setObsMonitoring,
  setObsVirtualCamera,
  setRecording,
  setNativeRecording,
  setRtmpDestinationEnabled,
  startLive,
  startTestDrone,
  stopTestDrone,
  stopLive,
  upsertRtmpDestination,
} from "./lib/backend";
import { useBridgeStore } from "./store";
import type {
  BridgeSnapshot,
  DiagnosticItem,
  NativeProductionSettings,
  RtmpDestinationKind,
  RtmpDestinationState,
} from "./types";

type Translate = (key: string, params?: TranslationParams) => string;
type Act = (name: string, operation: () => Promise<unknown>) => Promise<void>;
type Tab = "live" | "camera" | "advanced";

const PLATFORMS: RtmpDestinationKind[] = ["Instagram", "TikTok", "Custom"];
const SETTINGS_KEY = "dji-live-bridge.stream-settings";
const DEFAULT_SETTINGS: NativeProductionSettings = {
  layout: "Portrait",
  fitMode: "Fit",
  microphone: null,
  microphoneMuted: false,
  microphoneVolumeDb: 0,
  microphoneSyncMs: 0,
  noiseSuppression: true,
  compressor: true,
  limiter: true,
};

function loadSettings(): NativeProductionSettings {
  try {
    const stored = localStorage.getItem(SETTINGS_KEY);
    return stored ? { ...DEFAULT_SETTINGS, ...JSON.parse(stored) } : DEFAULT_SETTINGS;
  } catch {
    return DEFAULT_SETTINGS;
  }
}

function App() {
  const { snapshot, uiError, initialized, initialize, setUiError, clearUiError } = useBridgeStore();
  const [busy, setBusy] = useState<string>();
  const [tab, setTab] = useState<Tab>("live");
  const [settings, setSettings] = useState<NativeProductionSettings>(loadSettings);
  const [language, setLanguage] = useState<Language>(loadLanguage);
  const [theme, setTheme] = useState<Theme>(loadTheme);
  const [liveSince, setLiveSince] = useState<number>();
  const live = snapshot?.production.active ?? false;
  const t = useCallback<Translate>(
    (key, params) => translate(language, key, params),
    [language],
  );

  useEffect(() => saveLanguage(language), [language]);

  useEffect(() => {
    saveTheme(theme);
    applyTheme(theme);
    return watchSystemTheme(theme);
  }, [theme]);

  // Kept here, not in the panel, so switching tabs does not reset the timer.
  useEffect(() => {
    setLiveSince(live ? Date.now() : undefined);
  }, [live]);

  useEffect(() => {
    try {
      localStorage.setItem(SETTINGS_KEY, JSON.stringify(settings));
    } catch {
      // Settings still apply for this session.
    }
  }, [settings]);

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
      },
    ).then((fn) => { unlisten = fn; });
    return () => unlisten?.();
  }, [setUiError]);

  const act = useCallback<Act>(
    async (name, operation) => {
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

  if (!initialized || !snapshot) {
    return <main className="boot-screen"><span className="spinner" />{t("status.starting")}</main>;
  }

  const error = uiError ?? snapshot.lastError;

  return (
    <main className="app-shell">
      <header className="topbar">
        <div className="brand">
          <span className="brand-mark" aria-hidden="true">
            <svg viewBox="0 0 24 24"><path d="M12 12 7.5 8M12 12l4.5-4M12 12l-4.5 4M12 12l4.5 4" /><circle cx="7.5" cy="8" r="2.4" /><circle cx="16.5" cy="8" r="2.4" /><circle cx="7.5" cy="16" r="2.4" /><circle cx="16.5" cy="16" r="2.4" /><rect x="10" y="10" width="4" height="4" rx="1.2" className="fill" /></svg>
          </span>
          <div>
            <h1>DJI Live Bridge</h1>
            <small>{snapshot.lanIpv4 ?? t("header.noLan")}</small>
          </div>
        </div>

        <nav className="tabs" aria-label="Sections">
          {(["live", "camera", "advanced"] as Tab[]).map((item) => (
            <button key={item} className={tab === item ? "active" : ""} onClick={() => setTab(item)}>
              {t(`nav.${item}`)}
            </button>
          ))}
        </nav>

        <div className="header-tools">
          <HeaderStatus snapshot={snapshot} t={t} />
          <select className="compact-select" aria-label={t("theme.label")} title={t("theme.label")} value={theme} onChange={(event) => setTheme(event.target.value as Theme)}>
            {THEMES.map((item) => <option key={item} value={item}>{t(`theme.${item}`)}</option>)}
          </select>
          <select className="compact-select" aria-label={t("language.label")} value={language} onChange={(event) => setLanguage(event.target.value as Language)}>
            <option value="en">EN</option>
            <option value="tr">TR</option>
          </select>
        </div>
      </header>

      {error && (
        <section className="banner banner-error" role="alert">
          <span className="banner-icon" aria-hidden="true">!</span>
          <div>
            <strong>{t(error.messageKey)}</strong>
            {error.detail && <p className="banner-detail">{error.detail}</p>}
            <p>{t(error.actionKey)}</p>
          </div>
          {uiError && <button className="ghost" onClick={clearUiError}>{t("common.close")}</button>}
        </section>
      )}

      {snapshot.ipChangeWarning && (
        <section className="banner banner-warn">
          <span className="banner-icon" aria-hidden="true">!</span>
          <div><p>{t("warning.ipChanged")}</p></div>
        </section>
      )}

      {tab === "live" && (
        <LiveTab snapshot={snapshot} settings={settings} setSettings={setSettings} busy={busy} act={act} t={t} live={live} liveSince={liveSince} />
      )}
      {tab === "camera" && <CameraTab snapshot={snapshot} busy={busy} act={act} t={t} />}
      {tab === "advanced" && <AdvancedTab snapshot={snapshot} settings={settings} act={act} t={t} />}
    </main>
  );
}

function HeaderStatus({ snapshot, t }: { snapshot: BridgeSnapshot; t: Translate }) {
  if (snapshot.production.active) {
    return <span className="header-status live"><span className="dot" />{t("status.live")}</span>;
  }
  if (snapshot.mediaMtx !== "Ready") {
    return <span className="header-status"><span className="dot" />{t("status.starting")}</span>;
  }
  return snapshot.publisherPresent
    ? <span className="header-status good"><span className="dot" />{t("status.droneConnected")}</span>
    : <span className="header-status wait"><span className="dot" />{t("status.waitingDrone")}</span>;
}

/* ---------------------------------------------------------------- Live tab */

function LiveTab({
  snapshot,
  settings,
  setSettings,
  busy,
  act,
  t,
  live,
  liveSince,
}: {
  snapshot: BridgeSnapshot;
  settings: NativeProductionSettings;
  setSettings: (update: (current: NativeProductionSettings) => NativeProductionSettings) => void;
  busy?: string;
  act: Act;
  t: Translate;
  live: boolean;
  liveSince?: number;
}) {
  const destinations = snapshot.production.destinations;
  const hasEnabled = destinations.some((destination) => destination.enabled);

  return (
    <div className="live-layout">
      <div className="steps">
        <StepCard n={1} tone="cyan" done={snapshot.publisherPresent} title={t("s1.title")}>
          <DroneStep snapshot={snapshot} busy={busy} act={act} t={t} />
        </StepCard>
        <StepCard n={2} tone="pink" done={hasEnabled} title={t("s2.title")}>
          <DestinationStep destinations={destinations} live={live} busy={busy} act={act} t={t} />
        </StepCard>
        <StepCard n={3} tone="amber" done title={t("s3.title")}>
          <SettingsStep snapshot={snapshot} settings={settings} setSettings={setSettings} disabled={live} t={t} />
        </StepCard>
      </div>

      <div className="stage">
        <PreviewCard snapshot={snapshot} busy={busy} act={act} t={t} />
        <GoLivePanel snapshot={snapshot} settings={settings} busy={busy} act={act} t={t} liveSince={liveSince} />
      </div>
    </div>
  );
}

function StepCard({ n, tone, done, title, children }: { n: number; tone: string; done: boolean; title: string; children: ReactNode }) {
  return (
    <section className={`card step-card tone-${tone} ${done ? "done" : ""}`}>
      <header className="step-head">
        <span className="step-badge">{done && n !== 3 ? "✓" : n}</span>
        <h2>{title}</h2>
      </header>
      {children}
    </section>
  );
}

function DroneStep({ snapshot, busy, act, t }: { snapshot: BridgeSnapshot; busy?: string; act: Act; t: Translate }) {
  const [qrCode, setQrCode] = useState<string>();
  const [showQr, setShowQr] = useState(false);
  const [copied, setCopied] = useState(false);
  const testRunning = snapshot.processes.some((process) => process.name === "test-drone" && process.status === "Running");

  useEffect(() => {
    if (!snapshot.rtmpUrl) {
      setQrCode(undefined);
      return;
    }
    void QRCode.toDataURL(snapshot.rtmpUrl, { width: 240, margin: 1, color: { dark: "#1d1d1fff", light: "#ffffffff" } }).then(setQrCode);
  }, [snapshot.rtmpUrl]);

  const copy = () => {
    if (!snapshot.rtmpUrl) return;
    void navigator.clipboard.writeText(snapshot.rtmpUrl).then(() => {
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1600);
    });
  };

  return (
    <>
      <div className={`connection-state ${snapshot.publisherPresent ? "on" : ""}`}>
        <span className="dot" />
        {snapshot.publisherPresent ? t("s1.connected") : t("s1.waiting")}
      </div>
      <div className="copy-field">
        <code>{snapshot.rtmpUrl ?? t("ingest.noInterface")}</code>
        <button className="soft" disabled={!snapshot.rtmpUrl} onClick={copy}>{copied ? t("s1.copied") : t("s1.copy")}</button>
        <button className="soft" disabled={!qrCode} onClick={() => setShowQr((value) => !value)}>{t("s1.qr")}</button>
      </div>
      {showQr && qrCode && <img className="qr" src={qrCode} alt={t("ingest.qrAlt")} />}
      <p className="hint">{t("s1.help")}</p>
      <div className="row-between">
        {snapshot.interfaces.length > 1 ? (
          <label className="inline-select">
            {t("s1.network")}
            <select value={snapshot.selectedInterface ?? ""} onChange={(event) => void act("interface", () => selectInterface(event.target.value))}>
              {snapshot.interfaces.map((item) => (
                <option key={item.name} value={item.name}>{item.name} · {item.ipv4}</option>
              ))}
            </select>
          </label>
        ) : <span />}
        <button
          className="link"
          disabled={snapshot.mediaMtx !== "Ready" || busy === "test-drone" || (snapshot.publisherPresent && !testRunning)}
          onClick={() =>
            void act("test-drone", async () => {
              if (testRunning) {
                await stopTestDrone();
                return;
              }
              const selected = await open({
                multiple: false,
                directory: false,
                filters: [{ name: t("ingest.videoFilter"), extensions: ["mp4", "mov", "mkv", "m4v"] }],
              });
              if (typeof selected === "string") await startTestDrone(selected);
            })
          }
        >
          {testRunning ? t("s1.stopTest") : t("s1.test")}
        </button>
      </div>
    </>
  );
}

function PlatformIcon({ kind }: { kind: RtmpDestinationKind }) {
  return (
    <span className={`platform-icon ${kind.toLowerCase()}`} aria-hidden="true">
      {kind === "Instagram" && (
        <svg viewBox="0 0 24 24"><rect x="4" y="4" width="16" height="16" rx="5" /><circle cx="12" cy="12" r="3.6" /><circle cx="16.9" cy="7.1" r="1" className="fill" /></svg>
      )}
      {kind === "TikTok" && (
        <svg viewBox="0 0 24 24"><path d="M14 4v10.5a3.5 3.5 0 1 1-3.5-3.5M14 4c.4 2.6 2 4.2 4.8 4.4" /></svg>
      )}
      {kind === "Custom" && (
        <svg viewBox="0 0 24 24"><circle cx="12" cy="12" r="2" className="fill" /><path d="M7.8 7.8a6 6 0 0 0 0 8.4M16.2 7.8a6 6 0 0 1 0 8.4M5 5a10 10 0 0 0 0 14M19 5a10 10 0 0 1 0 14" /></svg>
      )}
    </span>
  );
}

interface DraftDestination {
  id: string | null;
  kind: RtmpDestinationKind;
  name: string;
  server: string;
  key: string;
  enabled: boolean;
}

function DestinationStep({
  destinations,
  live,
  busy,
  act,
  t,
}: {
  destinations: RtmpDestinationState[];
  live: boolean;
  busy?: string;
  act: Act;
  t: Translate;
}) {
  const [draft, setDraft] = useState<DraftDestination | null>(null);
  const editorOpen = draft !== null || destinations.length === 0;
  const current: DraftDestination = draft ?? { id: null, kind: "Instagram", name: "", server: "", key: "", enabled: true };
  const update = (patch: Partial<DraftDestination>) => setDraft({ ...current, ...patch });

  const save = () =>
    act("save-destination", async () => {
      await upsertRtmpDestination(
        current.id,
        current.name.trim() || t(`destination.kind.${current.kind}`),
        current.kind,
        current.server.trim(),
        current.key.trim() ? current.key.trim() : null,
        current.enabled,
      );
      setDraft(null);
    });

  const canSave = !live && current.server.trim() !== "" && (current.id !== null || current.key.trim() !== "");

  return (
    <>
      {destinations.length > 0 && (
        <div className="destinations">
          {destinations.map((destination) => (
            <article className={`destination ${destination.enabled ? "on" : ""} ${destination.state ?? ""}`} key={destination.id}>
              <PlatformIcon kind={destination.kind} />
              <div className="destination-text">
                <strong>{destination.name}</strong>
                <small>{destination.server}</small>
                {destination.lastError && <small className="error-text">{destination.lastError}</small>}
              </div>
              {live && destination.enabled ? (
                <DestinationLiveState destination={destination} t={t} />
              ) : (
                <label className="switch" title={t("s2.include")}>
                  <input
                    type="checkbox"
                    checked={destination.enabled}
                    disabled={live || busy === `toggle-${destination.id}`}
                    onChange={(event) => void act(`toggle-${destination.id}`, () => setRtmpDestinationEnabled(destination.id, event.target.checked))}
                  />
                  <span />
                </label>
              )}
              {!live && (
                <div className="destination-actions">
                  <button
                    className="link"
                    onClick={() => setDraft({ id: destination.id, kind: destination.kind, name: destination.name, server: destination.server, key: "", enabled: destination.enabled })}
                  >
                    {destination.kind === "Instagram" ? t("s2.newKey") : t("destination.edit")}
                  </button>
                  <button
                    className="link danger-link"
                    onClick={() => {
                      if (window.confirm(t("destination.removeConfirm"))) {
                        void act(`remove-${destination.id}`, async () => {
                          await removeRtmpDestination(destination.id);
                          if (draft?.id === destination.id) setDraft(null);
                        });
                      }
                    }}
                  >
                    {t("destination.remove")}
                  </button>
                </div>
              )}
            </article>
          ))}
        </div>
      )}

      {editorOpen && !live ? (
        <div className="editor">
          <div className="platform-picker" role="radiogroup">
            {PLATFORMS.map((kind) => (
              <button
                key={kind}
                role="radio"
                aria-checked={current.kind === kind}
                className={`platform-choice ${kind.toLowerCase()} ${current.kind === kind ? "selected" : ""}`}
                onClick={() => update({ kind })}
              >
                <PlatformIcon kind={kind} />
                {t(`destination.kind.${kind}`)}
              </button>
            ))}
          </div>
          <p className="hint">{t(`s2.help.${current.kind}`)}</p>
          <label className="field">
            <span>{t("s2.server")}</span>
            <input value={current.server} placeholder={t(`s2.placeholder.${current.kind}`)} spellCheck={false} autoCapitalize="off" onChange={(event) => update({ server: event.target.value })} />
          </label>
          <label className="field">
            <span>{t("s2.key")}</span>
            <input
              type="password"
              value={current.key}
              placeholder={current.id ? t("s2.keyKeep") : t("s2.keyNew")}
              autoFocus={current.id !== null}
              onChange={(event) => update({ key: event.target.value })}
            />
          </label>
          <details className="more">
            <summary>{t("s2.name")}</summary>
            <input value={current.name} maxLength={64} placeholder={t(`destination.kind.${current.kind}`)} onChange={(event) => update({ name: event.target.value })} />
          </details>
          <div className="row-end">
            {(draft !== null && destinations.length > 0) && <button className="ghost" onClick={() => setDraft(null)}>{t("s2.cancel")}</button>}
            <button className="primary" disabled={!canSave || busy === "save-destination"} onClick={() => void save()}>
              {current.id ? t("s2.saveChanges") : t("s2.save")}
            </button>
          </div>
        </div>
      ) : (
        !live && <button className="dashed" onClick={() => setDraft({ id: null, kind: "Instagram", name: "", server: "", key: "", enabled: true })}>+ {t("s2.add")}</button>
      )}
      {destinations.some((destination) => destination.kind === "Instagram") && !live && (
        <p className="hint note">{t("s2.igKeyNote")}</p>
      )}
    </>
  );
}

function DestinationLiveState({ destination, t }: { destination: RtmpDestinationState; t: Translate }) {
  const state = destination.state ?? "starting";
  const tone = state === "forwarding" ? "good" : state === "error" ? "bad" : "warn";
  return <StatusPill label={t(`destination.state.${state}`)} tone={tone} />;
}

function SettingsStep({
  snapshot,
  settings,
  setSettings,
  disabled,
  t,
}: {
  snapshot: BridgeSnapshot;
  settings: NativeProductionSettings;
  setSettings: (update: (current: NativeProductionSettings) => NativeProductionSettings) => void;
  disabled: boolean;
  t: Translate;
}) {
  const set = (patch: Partial<NativeProductionSettings>) => setSettings((current) => ({ ...current, ...patch }));
  return (
    <fieldset className="settings" disabled={disabled}>
      <div className="setting">
        <span>{t("s3.orientation")}</span>
        <div className="segmented">
          <button className={settings.layout === "Portrait" ? "active" : ""} onClick={() => set({ layout: "Portrait" })}>
            <span className="shape portrait" />{t("s3.portrait")}
          </button>
          <button className={settings.layout === "Landscape" ? "active" : ""} onClick={() => set({ layout: "Landscape" })}>
            <span className="shape landscape" />{t("s3.landscape")}
          </button>
        </div>
      </div>
      {settings.layout === "Landscape" && <p className="hint note">{t("s3.portraitHint")}</p>}
      <div className="setting">
        <span>{t("s3.framing")}</span>
        <div className="segmented">
          <button className={settings.fitMode === "Fit" ? "active" : ""} onClick={() => set({ fitMode: "Fit" })}>{t("s3.fit")}</button>
          <button className={settings.fitMode === "Fill" ? "active" : ""} onClick={() => set({ fitMode: "Fill" })}>{t("s3.fill")}</button>
        </div>
      </div>
      <div className="setting">
        <span>{t("s3.mic")}</span>
        <div className="mic-row">
          <select value={settings.microphone ?? ""} onChange={(event) => set({ microphone: event.target.value || null })}>
            <option value="">{t("s3.noMic")}</option>
            {snapshot.audioInputs.map((device) => <option key={device.name} value={device.name}>{device.name}</option>)}
          </select>
          {settings.microphone && (
            <label className="check">
              <input type="checkbox" checked={settings.microphoneMuted} onChange={(event) => set({ microphoneMuted: event.target.checked })} />
              {t("s3.mute")}
            </label>
          )}
        </div>
      </div>
      {settings.microphone && (
        <details className="more">
          <summary>{t("s3.moreAudio")}</summary>
          <div className="audio-grid">
            <label className="field"><span>{t("audio.volume")}</span><input type="number" min={-60} max={12} step={1} value={settings.microphoneVolumeDb} onChange={(event) => set({ microphoneVolumeDb: Number(event.target.value) })} /></label>
            <label className="field"><span>{t("audio.delay")}</span><input type="number" min={0} max={5000} value={settings.microphoneSyncMs} onChange={(event) => set({ microphoneSyncMs: Math.max(0, Number(event.target.value)) })} /></label>
          </div>
          <div className="checks">
            <label className="check"><input type="checkbox" checked={settings.noiseSuppression} onChange={(event) => set({ noiseSuppression: event.target.checked })} />{t("audio.noiseSuppression")}</label>
            <label className="check"><input type="checkbox" checked={settings.compressor} onChange={(event) => set({ compressor: event.target.checked })} />{t("audio.compressor")}</label>
            <label className="check"><input type="checkbox" checked={settings.limiter} onChange={(event) => set({ limiter: event.target.checked })} />{t("audio.limiter")}</label>
          </div>
        </details>
      )}
    </fieldset>
  );
}

function PreviewCard({ snapshot, busy, act, t }: { snapshot: BridgeSnapshot; busy?: string; act: Act; t: Translate }) {
  const [previewError, setPreviewError] = useState<string>();
  const [endpoint, setEndpoint] = useState(snapshot.preview.directWhepUrl);
  const [documentVisible, setDocumentVisible] = useState(!document.hidden);

  useEffect(() => {
    const handleVisibility = () => setDocumentVisible(!document.hidden);
    document.addEventListener("visibilitychange", handleVisibility);
    return () => document.removeEventListener("visibilitychange", handleVisibility);
  }, []);

  useEffect(() => {
    if (!snapshot.publisherPresent) {
      setEndpoint(snapshot.preview.directWhepUrl);
      setPreviewError(undefined);
    }
  }, [snapshot.preview.directWhepUrl, snapshot.publisherPresent]);

  const handleConnected = useCallback(() => {
    setPreviewError(undefined);
    void reportPreviewStatus(true);
  }, []);
  const handleFailure = useCallback((reason: string) => {
    setPreviewError(reason);
    void reportPreviewStatus(false, reason);
  }, []);
  const startFallback = () =>
    act("fallback", async () => {
      setEndpoint(await activatePreviewFallback());
      setPreviewError(undefined);
    });

  return (
    <section className={`card preview-card ${snapshot.production.active ? "is-live" : ""}`}>
      <WhepPreview endpoint={endpoint} active={snapshot.publisherPresent && documentVisible} onConnected={handleConnected} onFailure={handleFailure} t={t} />
      {snapshot.production.active && <span className="live-badge"><span className="dot" />{t("status.live")}</span>}
      {previewError && (
        <div className="preview-note">
          <span>{t("preview.unavailable")}</span>
          <button className="soft" disabled={busy === "fallback"} onClick={() => void startFallback()}>{t("preview.startFallback")}</button>
        </div>
      )}
    </section>
  );
}

function GoLivePanel({
  snapshot,
  settings,
  busy,
  act,
  t,
  liveSince,
}: {
  snapshot: BridgeSnapshot;
  settings: NativeProductionSettings;
  busy?: string;
  act: Act;
  t: Translate;
  liveSince?: number;
}) {
  const [phase, setPhase] = useState<"preparing" | "connecting">();
  const [now, setNow] = useState(Date.now());
  const live = snapshot.production.active;
  const enabled = snapshot.production.destinations.filter((destination) => destination.enabled);
  const targetNames = enabled.map((destination) => destination.name).join(" + ");

  useEffect(() => {
    if (!live) return;
    const timer = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, [live]);

  const checks = [
    { key: "server", ok: snapshot.mediaMtx === "Ready" },
    { key: "drone", ok: snapshot.publisherPresent },
    { key: "destination", ok: enabled.length > 0 },
  ];
  const workflowAllowsStart = snapshot.workflow === "DroneConnected" || snapshot.workflow === "Ready";
  const canStart = checks.every((check) => check.ok) && workflowAllowsStart && !busy;

  const goLive = () =>
    act("go-live", async () => {
      try {
        setPhase("preparing");
        await prepareNativeProduction(settings);
        setPhase("connecting");
        await startLive();
      } finally {
        setPhase(undefined);
      }
    });

  if (live) {
    const partial = snapshot.production.forwardState === "partial";
    return (
      <section className="card golive-card live">
        <div className="live-head">
          <span className="live-badge static"><span className="dot" />{t("status.live")}</span>
          <strong className="live-timer">{formatDuration(liveSince ? Math.floor((now - liveSince) / 1000) : 0)}</strong>
        </div>
        <div className="live-targets">
          {enabled.map((destination) => (
            <div className="live-target" key={destination.id}>
              <PlatformIcon kind={destination.kind} />
              <div>
                <strong>{destination.name}</strong>
                <small>{t("go.sent", { bytes: formatBytes(destination.outboundBytes) })}</small>
              </div>
              <DestinationLiveState destination={destination} t={t} />
            </div>
          ))}
        </div>
        {partial && <p className="hint warn-text">{t("go.partial")}</p>}
        {enabled.some((destination) => destination.kind === "Instagram") && <p className="callout">{t("go.igReminder")}</p>}
        {enabled.some((destination) => destination.kind === "TikTok") && <p className="callout">{t("go.tiktokReminder")}</p>}
        <button className="stop-button" disabled={busy === "stop-live"} onClick={() => void act("stop-live", stopLive)}>
          {busy === "stop-live" ? t("go.stopping") : t("go.stop")}
        </button>
      </section>
    );
  }

  const accent = enabled.length === 1 ? enabled[0].kind.toLowerCase() : "mixed";
  return (
    <section className="card golive-card">
      <div className="golive-head">
        <h2>{t("go.title")}</h2>
        {enabled.length > 0 && (
          <span className="target-chips">
            {enabled.map((destination) => <PlatformIcon key={destination.id} kind={destination.kind} />)}
            <small>{targetNames}</small>
          </span>
        )}
      </div>
      <ul className="checklist">
        {checks.map((check) => (
          <li key={check.key} className={check.ok ? "ok" : ""}>
            <span className="check-mark">{check.ok ? "✓" : ""}</span>
            {t(`go.check.${check.key}`)}
          </li>
        ))}
      </ul>
      <button className={`go-button ${accent}`} disabled={!canStart} onClick={() => void goLive()}>
        {phase === "preparing" ? <><span className="spinner" />{t("go.preparing")}</>
          : phase === "connecting" ? <><span className="spinner" />{t("go.connecting", { target: targetNames })}</>
          : t("go.start")}
      </button>
      {canStart && <p className="hint center">{t("go.ready")}</p>}
    </section>
  );
}

/* -------------------------------------------------------------- Camera tab */

function CameraTab({ snapshot, busy, act, t }: { snapshot: BridgeSnapshot; busy?: string; act: Act; t: Translate }) {
  const active = snapshot.virtualCamera.feedActive;
  const ready = snapshot.virtualCamera.status === "Ready";
  return (
    <div className="single-column">
      <section className="card">
        <header className="card-head">
          <div>
            <h2>{t("camera.primaryTitle")}</h2>
            <p className="hint">{t("camera.tabHelp")}</p>
          </div>
          <StatusPill
            label={active ? t("camera.statusActive") : ready ? t("camera.statusReady") : t("camera.statusSetup")}
            tone={active ? "good" : ready ? "neutral" : "warn"}
          />
        </header>
        <p className="body-text">{t(snapshot.virtualCamera.detailKey)}</p>
        <div className="row-start">
          {!ready ? (
            <button
              className="primary"
              disabled={!snapshot.virtualCamera.bundled || !snapshot.virtualCamera.appInstalled || busy === "enable-native-camera"}
              onClick={() => void act("enable-native-camera", activateVirtualCameraExtension)}
            >
              {snapshot.virtualCamera.status === "Starting" ? t("camera.waitingApproval") : t("camera.enableOnce")}
            </button>
          ) : !active ? (
            <button className="primary" disabled={!snapshot.publisherPresent || busy === "native-camera"} onClick={() => void act("native-camera", () => setNativeVirtualCamera(true))}>
              {snapshot.publisherPresent ? t("camera.start") : t("camera.waitingForVideo")}
            </button>
          ) : (
            <>
              <button className="primary" disabled={busy === "open-live-studio"} onClick={() => void act("open-live-studio", openTikTokLiveStudio)}>{t("camera.openStudio")}</button>
              <button className="danger" disabled={busy === "native-camera"} onClick={() => void act("native-camera", () => setNativeVirtualCamera(false))}>{t("camera.stop")}</button>
            </>
          )}
        </div>
        <ol className="how-to">
          <li>{t("guide.step1")}</li>
          <li>{t("guide.step2")}</li>
          <li>{t("guide.step3")}</li>
          <li>{t("guide.step4")}</li>
          <li>{t("guide.step5")}</li>
        </ol>
        <p className="hint">{t("camera.liveStudioHelp", { device: snapshot.virtualCamera.deviceName })}</p>
      </section>
    </div>
  );
}

/* ------------------------------------------------------------ Advanced tab */

function AdvancedTab({ snapshot, settings, act, t }: { snapshot: BridgeSnapshot; settings: NativeProductionSettings; act: Act; t: Translate }) {
  const [diagnostics, setDiagnostics] = useState<DiagnosticItem[]>([]);
  const [obsHost, setObsHost] = useState("127.0.0.1");
  const [obsPort, setObsPort] = useState(4455);
  const [obsPassword, setObsPassword] = useState("");
  const obsVirtualActive = snapshot.obs.virtualCameraActive === true;

  useEffect(() => {
    void setObsMonitoring(true);
    return () => void setObsMonitoring(false);
  }, []);

  const formatted = useMemo(() => {
    const metadata = snapshot.metadata;
    return {
      bitrate: formatBitrate(metadata.bitrateCalculatedBps, t("common.unavailable")),
      bytes: formatBytes(metadata.receivedBytes),
      uptime: metadata.uptimeSeconds == null ? t("common.unavailable") : formatDuration(metadata.uptimeSeconds),
      fps: metadata.fps ? `${metadata.fps.toFixed(2)} fps` : t("common.unavailable"),
    };
  }, [snapshot.metadata, t]);

  return (
    <div className="advanced-layout">
      <p className="hint">{t("advanced.tabHelp")}</p>
      <section className="metrics">
        <Metric label={t("metrics.resolution")} value={snapshot.metadata.resolution ?? "—"} />
        <Metric label={t("metrics.frameRate")} value={formatted.fps} />
        <Metric label={t("metrics.codecs")} value={`${snapshot.metadata.videoCodec ?? "—"} / ${snapshot.metadata.audioCodec ?? "—"}`} />
        <Metric label={t("metrics.bitrate")} value={formatted.bitrate} />
        <Metric label={t("metrics.received")} value={formatted.bytes} />
        <Metric label={t("metrics.uptime", { value: "" }).trim()} value={formatted.uptime} />
      </section>

      <div className="advanced-grid">
        <section className="card">
          <header className="card-head"><h2>{t("recording.native")}</h2></header>
          <p className="hint">{snapshot.production.recordingPath ?? t("recording.nativeHelp")}</p>
          <div className="row-start">
            <button
              className={snapshot.production.recordingActive ? "danger" : "primary"}
              disabled={!snapshot.publisherPresent}
              onClick={() => void act("native-recording", () => setNativeRecording(!snapshot.production.recordingActive))}
            >
              {snapshot.production.recordingActive ? t("recording.stop") : t("recording.start")}
            </button>
          </div>
        </section>

        <section className="card">
          <header className="card-head">
            <h2>{t("diagnostics.title")}</h2>
            <button className="soft" onClick={() => void act("diagnostics", async () => setDiagnostics(await getDiagnostics()))}>{t("diagnostics.run")}</button>
          </header>
          {diagnostics.length === 0 ? (
            <p className="hint">{t("diagnostics.empty")}</p>
          ) : (
            <div className="diagnostics">
              {diagnostics.map((item) => (
                <div className="diagnostic" key={item.id}>
                  <StatusPill label={t(`common.${item.level.toLowerCase()}`)} tone={item.level === "Pass" ? "good" : item.level === "Warning" ? "warn" : "bad"} />
                  <div>
                    <strong>{t(item.nameKey)}</strong>
                    <p>{t(item.detailKey)}</p>
                    {item.actionKey && <p className="warn-text">{t(item.actionKey)}</p>}
                    <details><summary>{t("common.technicalDetails")}</summary><code>{item.technicalDetail}</code></details>
                  </div>
                </div>
              ))}
            </div>
          )}
        </section>

        <section className="card">
          <header className="card-head"><h2>{t("obs.summary")}</h2></header>
          <p className="hint">{t("obs.help")}</p>
          <div className="obs-grid">
            <label className="field"><span>{t("obs.host")}</span><input value={obsHost} onChange={(event) => setObsHost(event.target.value)} /></label>
            <label className="field"><span>{t("obs.port")}</span><input type="number" value={obsPort} onChange={(event) => setObsPort(Number(event.target.value))} /></label>
            <label className="field"><span>{t("obs.password")}</span><input type="password" value={obsPassword} onChange={(event) => setObsPassword(event.target.value)} /></label>
          </div>
          <div className="row-start">
            <button className="soft" onClick={() => void act("save-obs", () => saveObsConnection(obsHost, obsPort, obsPassword))}>{t("obs.save")}</button>
            <button className="soft" onClick={() => void act("open-obs", openObs)}>{snapshot.obs.installed ? t("obs.open") : t("obs.download")}</button>
            <button className="soft" disabled={!snapshot.publisherPresent || !snapshot.obs.connected} onClick={() => void act("prepare-obs", () => prepareObs(settings.layout, settings.fitMode))}>{t("obs.prepare")}</button>
          </div>
          <div className="row-between spaced">
            <span className="hint">{t("obs.camera")}</span>
            <button className={obsVirtualActive ? "danger" : "soft"} disabled={!snapshot.obs.connected || !snapshot.obs.sceneReady} onClick={() => void act("virtual-cam", () => setObsVirtualCamera(!obsVirtualActive))}>{obsVirtualActive ? t("camera.stop") : t("camera.start")}</button>
          </div>
          <div className="row-between spaced">
            <span className="hint">{snapshot.obs.lastRecordingPath ?? t("obs.recording")}</span>
            <button className={snapshot.obs.recordingActive ? "danger" : "soft"} disabled={!snapshot.obs.connected || !snapshot.obs.sceneReady} onClick={() => void act("recording", () => setRecording(!snapshot.obs.recordingActive))}>{snapshot.obs.recordingActive ? t("obs.stopRecording") : t("obs.startRecording")}</button>
          </div>
        </section>
      </div>
    </div>
  );
}

function Metric({ label, value }: { label: string; value: string }) {
  return <article className="metric"><span>{label}</span><strong>{value}</strong></article>;
}

function formatBitrate(value: number | null | undefined, unavailable: string) {
  if (value == null) return unavailable;
  return value >= 1_000_000 ? `${(value / 1_000_000).toFixed(2)} Mbps` : `${Math.round(value / 1000)} kbps`;
}

function formatBytes(value: number) {
  if (value < 1024) return `${value} B`;
  if (value < 1024 ** 2) return `${(value / 1024).toFixed(1)} KB`;
  if (value < 1024 ** 3) return `${(value / 1024 ** 2).toFixed(1)} MB`;
  return `${(value / 1024 ** 3).toFixed(2)} GB`;
}

function formatDuration(value: number) {
  const hours = Math.floor(value / 3600);
  const minutes = Math.floor((value % 3600) / 60);
  const seconds = value % 60;
  return [hours, minutes, seconds].map((part) => String(part).padStart(2, "0")).join(":");
}

export default App;
