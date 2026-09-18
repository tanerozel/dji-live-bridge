export type WorkflowState =
  | "Idle"
  | "Preparing"
  | "WaitingForDrone"
  | "DroneConnected"
  | "PreparingObs"
  | "Ready"
  | "GoingLive"
  | "Live"
  | "Stopping";

export type ServiceStatus = "Unavailable" | "Starting" | "Ready" | "Failed";
export type PreviewMode = "Direct" | "Transcoded";

export interface ErrorPayload {
  code: string;
  messageKey: string;
  detail: string;
  actionKey: string;
}

export interface NetworkInterface {
  name: string;
  ipv4: string;
  isDefaultRoute: boolean;
  recommended: boolean;
}

export interface StreamMetadata {
  resolution: string | null;
  fps: number | null;
  videoCodec: string | null;
  audioCodec: string | null;
  videoBitrateBps: number | null;
  audioBitrateBps: number | null;
  bitrateCalculatedBps: number | null;
  receivedBytes: number;
  uptimeSeconds: number | null;
}

export interface PreviewState {
  directWhepUrl: string;
  fallbackWhepUrl: string;
  mode: PreviewMode;
  status: ServiceStatus;
  reasonKey: string | null;
  reason: string | null;
}

export interface ObsState {
  installed: boolean;
  running: boolean;
  connected: boolean;
  obsVersion: string | null;
  websocketVersion: string | null;
  availableRequests: string[];
  sceneReady: boolean;
  virtualCameraActive: boolean | null;
  streamActive: boolean | null;
  recordingActive: boolean | null;
  recordingPaused: boolean | null;
  lastRecordingPath: string | null;
  lastError: string | null;
}

export type ProductionEngine = "NativeFfmpeg" | "Obs";

export interface ProductionState {
  engine: ProductionEngine;
  prepared: boolean;
  active: boolean;
  pathStatus: ServiceStatus;
  encoder: string | null;
  selectedMicrophone: string | null;
  forwardState: string | null;
  forwardError: string | null;
  outboundBytes: number;
  recordingActive: boolean;
  recordingPath: string | null;
}

export interface VirtualCameraState {
  deviceName: string;
  bundled: boolean;
  appInstalled: boolean;
  status: ServiceStatus;
  feedActive: boolean;
  width: number;
  height: number;
  fps: number;
  detailKey: string;
  detail: string | null;
}

export interface NativeProductionSettings {
  layout: "Landscape" | "Portrait";
  fitMode: "Fit" | "Fill";
  microphone: string | null;
  microphoneMuted: boolean;
  microphoneVolumeDb: number;
  microphoneSyncMs: number;
  noiseSuppression: boolean;
  compressor: boolean;
  limiter: boolean;
}

export interface ProcessSnapshot {
  name: string;
  pid: number | null;
  status: "Starting" | "Running" | "BackingOff" | "Stopped" | "Failed" | "CrashLoop";
  restartPolicy: "Never" | "OnFailure" | "Always";
  restartCount: number;
  lastError: string | null;
}

export interface AudioInputDevice {
  name: string;
  manufacturer: string | null;
  transport: string | null;
}

export interface BridgeSnapshot {
  workflow: WorkflowState;
  interfaces: NetworkInterface[];
  selectedInterface: string | null;
  lanIpv4: string | null;
  rtmpUrl: string | null;
  ipChangeWarning: boolean;
  mediaMtx: ServiceStatus;
  publisherPresent: boolean;
  publisherSinceUnixMs: number | null;
  metadata: StreamMetadata;
  preview: PreviewState;
  obs: ObsState;
  production: ProductionState;
  virtualCamera: VirtualCameraState;
  audioInputs: AudioInputDevice[];
  processes: ProcessSnapshot[];
  lastError: ErrorPayload | null;
  updatedAtUnixMs: number;
}

export interface DiagnosticItem {
  id: string;
  nameKey: string;
  level: "Pass" | "Warning" | "Fail";
  detailKey: string;
  actionKey: string | null;
  technicalDetail: string;
}

export interface FfmpegCapabilities {
  ffmpegPath: string | null;
  ffprobePath: string | null;
  version: string | null;
  h264Videotoolbox: boolean;
  libx264: boolean;
  libopus: boolean;
  avfoundation: boolean;
  aac: boolean;
  afftdn: boolean;
  compressor: boolean;
  limiter: boolean;
  amix: boolean;
}
