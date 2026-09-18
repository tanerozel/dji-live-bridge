import { invoke } from "@tauri-apps/api/core";
import type {
  BridgeSnapshot,
  DiagnosticItem,
  ErrorPayload,
  FfmpegCapabilities,
  NativeProductionSettings,
} from "../types";

export function getSnapshot() {
  return invoke<BridgeSnapshot>("get_snapshot");
}

export function selectInterface(name: string) {
  return invoke<void>("select_interface", { name });
}

export function startTestDrone(path: string) {
  return invoke<void>("start_test_drone", { path });
}

export function stopTestDrone() {
  return invoke<void>("stop_test_drone");
}

export function activatePreviewFallback() {
  return invoke<string>("activate_preview_fallback");
}

export function stopPreviewFallback() {
  return invoke<void>("stop_preview_fallback");
}

export function reportPreviewStatus(connected: boolean, detail?: string) {
  return invoke<void>("report_preview_status", { connected, detail });
}

export function saveObsConnection(host: string, port: number, password: string) {
  return invoke<void>("save_obs_connection", { host, port, password });
}

export function openObs() {
  return invoke<void>("open_obs");
}

export function openTikTokLiveStudio() {
  return invoke<void>("open_tiktok_live_studio");
}

export function prepareObs(
  layout: "Landscape" | "Portrait",
  fitMode: "Fit" | "Fill",
) {
  return invoke<void>("prepare_obs", { layout, fitMode });
}

export function prepareNativeProduction(settings: NativeProductionSettings) {
  return invoke<void>("prepare_native_production", { settings });
}

export function setObsVirtualCamera(active: boolean) {
  return invoke<boolean>("set_obs_virtual_camera", { active });
}

export function activateVirtualCameraExtension() {
  return invoke<void>("activate_virtual_camera_extension");
}

export function setNativeVirtualCamera(active: boolean) {
  return invoke<void>("set_native_virtual_camera", { active });
}

export function setRecording(active: boolean) {
  return invoke<string | null>("set_recording", { active });
}

export function setNativeRecording(active: boolean) {
  return invoke<string | null>("set_native_recording", { active });
}

export function configureDestination(
  mode: "TikTokRtmp" | "CustomRtmp" | "TikTokLiveStudio",
  server?: string,
  key?: string,
) {
  return invoke<void>("configure_destination", { mode, server, key });
}

export function startLive() {
  return invoke<void>("start_live");
}

export function stopLive() {
  return invoke<void>("stop_live");
}

export function getDiagnostics() {
  return invoke<DiagnosticItem[]>("get_diagnostics");
}

export function getFfmpegCapabilities() {
  return invoke<FfmpegCapabilities>("get_ffmpeg_capabilities");
}

export function normalizeError(error: unknown): ErrorPayload {
  if (
    typeof error === "object" &&
    error !== null &&
    "message" in error &&
    "code" in error
  ) {
    const candidate = error as Partial<ErrorPayload>;
    return {
      code: candidate.code ?? "UNKNOWN",
      message: candidate.message ?? "Unknown backend error",
      action: candidate.action ?? "Open Diagnostics and retry.",
    };
  }
  return {
    code: "UNKNOWN",
    message: error instanceof Error ? error.message : String(error),
    action: "Open Diagnostics and retry.",
  };
}
