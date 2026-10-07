import { listen } from "@tauri-apps/api/event";
import { useEffect, useRef, useState } from "react";
import type { TranslationParams } from "../i18n";
import { resetVisionCounter, setVisionSettings } from "../lib/backend";
import type { BridgeSnapshot, DetectionEvent, DetectionSettings, InferenceRate, VisionState } from "../types";

type Translate = (key: string, params?: TranslationParams) => string;
type Act = (name: string, operation: () => Promise<unknown>) => Promise<void>;

const RATES: InferenceRate[] = ["Auto", "Fps5", "Fps10", "Fps15"];

/** Same colours as the overlay burned into the stream: one per class of the
 * animal profile, in its order (`DetectionProfile::classes` in settings.rs). */
const PALETTE = [
  "#ffc107", "#4caf50", "#2196f3", "#e91e63", "#00bcd4", "#ff5722", "#9c27b0", "#cddc39",
  "#ff6347", "#795548", "#f48fb1", "#009688", "#9e9e9e", "#3f51b5", "#ffeb3b",
];
const PROFILE_CLASSES = [
  "goat", "sheep", "cow", "horse", "donkey", "dog", "cat", "chicken", "duck", "goose", "pig", "deer", "wild boar", "bird", "bear",
];
const classColour = (name: string) => PALETTE[Math.max(0, PROFILE_CLASSES.indexOf(name)) % PALETTE.length];

export function VisionCard({ snapshot, busy, act, t }: { snapshot: BridgeSnapshot; busy?: string; act: Act; t: Translate }) {
  const vision = snapshot.vision;
  const settings = vision.settings;
  const [threshold, setThreshold] = useState(settings.confidenceThreshold);

  useEffect(() => setThreshold(settings.confidenceThreshold), [settings.confidenceThreshold]);

  const save = (patch: Partial<DetectionSettings>) =>
    act("vision", () => setVisionSettings({ ...settings, ...patch }));
  const commitThreshold = () => {
    if (Math.abs(threshold - settings.confidenceThreshold) > 0.001) void save({ confidenceThreshold: threshold });
  };
  const allSpecies = settings.classes.length === 0;
  const toggleSpecies = (name: string) => {
    const current = allSpecies ? [] : settings.classes;
    const next = current.includes(name) ? current.filter((item) => item !== name) : [...current, name];
    // Choosing every species, or clearing the last one, means "all".
    void save({ classes: next.length === vision.classes.length ? [] : next });
  };

  return (
    <section className={`card vision-card ${settings.enabled ? "on" : ""}`}>
      <header className="card-head">
        <div>
          <h2>{t("vision.title")}</h2>
          <p className="hint">{t("vision.help")}</p>
        </div>
        <label className="switch" title={t("vision.enable")}>
          <input
            type="checkbox"
            aria-label={t("vision.enable")}
            checked={settings.enabled}
            disabled={busy === "vision"}
            onChange={(event) => void save({ enabled: event.target.checked })}
          />
          <span />
        </label>
      </header>

      {settings.enabled && (
        <>
          <VisionStatusLine vision={vision} act={act} t={t} onRetry={() => void save({})} />
          <div className="checks vision-toggles">
            <label className="check">
              <input type="checkbox" checked={settings.showBoxes} onChange={(event) => void save({ showBoxes: event.target.checked })} />
              {t("vision.showBoxes")}
            </label>
            <label className="check">
              <input type="checkbox" checked={settings.showCounter} onChange={(event) => void save({ showCounter: event.target.checked })} />
              {t("vision.showCounter")}
            </label>
          </div>
          <label className="field vision-threshold">
            <span>{t("vision.threshold")} <strong>{threshold.toFixed(2)}</strong></span>
            <input
              type="range"
              min={0.2}
              max={0.9}
              step={0.05}
              value={threshold}
              onChange={(event) => setThreshold(Number(event.target.value))}
              onPointerUp={commitThreshold}
              onKeyUp={commitThreshold}
              onBlur={commitThreshold}
            />
          </label>
          <details className="more">
            <summary>{t("vision.more")}</summary>
            <div className="settings vision-more">
              <div className="setting">
                <span>{t("vision.rate")}</span>
                <div className="segmented">
                  {RATES.map((rate) => (
                    <button key={rate} className={settings.inferenceRate === rate ? "active" : ""} onClick={() => void save({ inferenceRate: rate })}>
                      {t(`vision.rate.${rate}`)}
                    </button>
                  ))}
                </div>
              </div>
              <label className="setting">
                <span>{t("vision.model")}</span>
                <select value={settings.modelId} onChange={(event) => void save({ modelId: event.target.value })}>
                  {vision.models.map((model) => (
                    <option key={model.id} value={model.id}>
                      {model.name} · {t("vision.speciesCount", { count: model.species })} · {(model.sizeBytes / 1024 / 1024).toFixed(0)} MB{model.downloaded ? "" : ` · ${t("vision.needsDownload")}`}
                    </option>
                  ))}
                </select>
              </label>
              <div className="setting">
                <span>{t("vision.species")}</span>
                <div className="chips" role="group" aria-label={t("vision.species")}>
                  <button className={`chip ${allSpecies ? "selected" : ""}`} aria-pressed={allSpecies} onClick={() => void save({ classes: [] })}>
                    {t("vision.allSpecies")}
                  </button>
                  {vision.classes.map((name) => {
                    const selected = !allSpecies && settings.classes.includes(name);
                    return (
                      <button key={name} className={`chip ${selected ? "selected" : ""}`} aria-pressed={selected} onClick={() => toggleSpecies(name)}>
                        <span className="chip-dot" style={{ background: classColour(name) }} />
                        {t(`vision.class.${name}`)}
                      </button>
                    );
                  })}
                </div>
              </div>
            </div>
          </details>
          {snapshot.production.active && !snapshot.production.visionOverlay && <p className="hint note">{t("vision.nextLive")}</p>}
        </>
      )}
    </section>
  );
}

function VisionStatusLine({ vision, act, t, onRetry }: { vision: VisionState; act: Act; t: Translate; onRetry: () => void }) {
  if (vision.status === "Downloading") {
    const total = vision.download?.totalBytes ?? 0;
    const percent = total > 0 ? Math.floor(((vision.download?.receivedBytes ?? 0) / total) * 100) : 0;
    return (
      <div className="vision-status">
        <span className="connection-state"><span className="dot" />{t("vision.status.Downloading", { percent })}</span>
        <div className="progress" role="progressbar" aria-valuenow={percent} aria-valuemin={0} aria-valuemax={100}><span style={{ width: `${percent}%` }} /></div>
      </div>
    );
  }
  if (vision.status === "Failed") {
    return (
      <div className="vision-status">
        <span className="connection-state failed"><span className="dot" />{t("vision.status.Failed")}</span>
        {vision.detail && <p className="hint error-text">{vision.detail}</p>}
        <button className="soft" onClick={onRetry}>{t("vision.retry")}</button>
      </div>
    );
  }
  if (vision.status === "Running") {
    const counts = vision.counts;
    return (
      <div className="vision-status">
        <span className="connection-state on">
          <span className="dot" />
          {t("vision.status.Running", {
            backend: vision.backend ?? "CPU",
            ms: vision.inferenceMs?.toFixed(vision.inferenceMs < 100 ? 1 : 0) ?? "—",
            // OWLv2 does about one picture per second; keep the decimal there.
            fps: vision.inferenceFps?.toFixed(vision.inferenceFps < 10 ? 1 : 0) ?? "0",
          })}
        </span>
        <div className="row-between vision-counts">
          <span>
            <strong>{t("vision.counts", { count: counts.currentTotal, unique: counts.uniqueTotal })}</strong>
            {counts.currentByClass.length > 0 && (
              <small>{counts.currentByClass.map((entry) => `${t(`vision.class.${entry.class}`)} ${entry.count}`).join(" · ")}</small>
            )}
          </span>
          <button className="link" onClick={() => void act("vision-reset", resetVisionCounter)}>{t("vision.reset")}</button>
        </div>
      </div>
    );
  }
  return (
    <div className="vision-status">
      <span className="connection-state"><span className="dot" />{t(`vision.status.${vision.status}`)}</span>
    </div>
  );
}

/** "AI Vision ●  Animals: 14" over the preview while detection runs. */
export function VisionBadge({ vision, t }: { vision: VisionState; t: Translate }) {
  if (!vision.settings.enabled) return null;
  const running = vision.status === "Running";
  return (
    <span className={`vision-badge ${running ? "running" : ""}`}>
      <span className="dot" />
      {t("vision.badge")}
      {running && <strong>{t("vision.badgeCount", { count: vision.counts.currentTotal })}</strong>}
    </span>
  );
}

/**
 * Draws the latest detections over the in-app preview. The stream and the
 * virtual camera get the same boxes burned in by FFmpeg; this only shows them
 * here without waiting for an output to be running.
 */
export function DetectionLayer({ t }: { t: Translate }) {
  const canvasRef = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    let latest: DetectionEvent | null = null;
    let staleTimer: number | undefined;

    const draw = () => {
      const context = canvas.getContext("2d");
      if (!context) return;
      const ratio = window.devicePixelRatio || 1;
      const width = canvas.clientWidth;
      const height = canvas.clientHeight;
      if (canvas.width !== Math.round(width * ratio) || canvas.height !== Math.round(height * ratio)) {
        canvas.width = Math.round(width * ratio);
        canvas.height = Math.round(height * ratio);
      }
      context.setTransform(ratio, 0, 0, ratio, 0, 0);
      context.clearRect(0, 0, width, height);
      if (!latest || latest.detections.length === 0) return;
      // The video is letterboxed (object-fit: contain); place boxes on the picture.
      const aspect = latest.frameWidth && latest.frameHeight ? latest.frameWidth / latest.frameHeight : 16 / 9;
      const pictureWidth = Math.min(width, height * aspect);
      const pictureHeight = pictureWidth / aspect;
      const left = (width - pictureWidth) / 2;
      const top = (height - pictureHeight) / 2;
      context.font = "600 11px -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif";
      context.textBaseline = "top";
      for (const detection of latest.detections) {
        const box = detection.normalizedBbox;
        const x = left + box.x * pictureWidth;
        const y = top + box.y * pictureHeight;
        const colour = classColour(detection.class);
        context.strokeStyle = colour;
        context.lineWidth = 2;
        context.strokeRect(x, y, box.width * pictureWidth, box.height * pictureHeight);
        const label = `${t(`vision.class.${detection.class}`)} #${detection.trackId} ${Math.round(detection.confidence * 100)}%`;
        const labelWidth = context.measureText(label).width + 8;
        const labelTop = y >= 16 ? y - 16 : y;
        context.fillStyle = colour;
        context.fillRect(x, labelTop, labelWidth, 16);
        context.fillStyle = "#fff";
        context.fillText(label, x + 4, labelTop + 2);
      }
    };

    const observer = new ResizeObserver(draw);
    observer.observe(canvas);
    let unlisten: (() => void) | undefined;
    let disposed = false;
    void listen<DetectionEvent>("vision://detections", ({ payload }) => {
      latest = payload;
      draw();
      window.clearTimeout(staleTimer);
      // Detections that stop arriving (video paused, AI turned off) must not
      // linger. OWLv2 sends about one update a second, so allow a few.
      staleTimer = window.setTimeout(() => {
        latest = null;
        draw();
      }, 3000);
    }).then((fn) => {
      if (disposed) fn();
      else unlisten = fn;
    });
    return () => {
      disposed = true;
      observer.disconnect();
      window.clearTimeout(staleTimer);
      unlisten?.();
    };
  }, [t]);

  return <canvas ref={canvasRef} className="detection-layer" aria-hidden="true" />;
}
