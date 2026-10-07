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
export type RtmpDestinationKind = "TikTok" | "Instagram" | "YouTube" | "Facebook" | "Custom";

export interface RtmpDestinationState {
  id: string;
  name: string;
  kind: RtmpDestinationKind;
  server: string;
  enabled: boolean;
  state: string | null;
  lastError: string | null;
  outboundBytes: number;
}

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
  destinations: RtmpDestinationState[];
  recordingActive: boolean;
  recordingPath: string | null;
  /** The running live encode carries the AI Vision overlay. */
  visionOverlay: boolean;
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
  vision: VisionState;
  audioInputs: AudioInputDevice[];
  processes: ProcessSnapshot[];
  lastError: ErrorPayload | null;
  updatedAtUnixMs: number;
}

export type InferenceRate = "Auto" | "Fps5" | "Fps10" | "Fps15";
export type VisionStatus = "Off" | "Downloading" | "WaitingForVideo" | "Loading" | "Running" | "Failed";

export interface DetectionSettings {
  enabled: boolean;
  showBoxes: boolean;
  showCounter: boolean;
  confidenceThreshold: number;
  inferenceRate: InferenceRate;
  modelId: string;
  profile: "Animals";
  /** Classes to report; empty means every class of the profile. */
  classes: string[];
}

export interface ClassCount {
  class: string;
  count: number;
}

export interface CountSummary {
  currentTotal: number;
  currentByClass: ClassCount[];
  uniqueTotal: number;
  uniqueByClass: ClassCount[];
}

export interface VisionModelInfo {
  id: string;
  name: string;
  sizeBytes: number;
  license: string;
  downloaded: boolean;
  /** Animal species the model can tell apart. */
  species: number;
}

export interface VisionState {
  settings: DetectionSettings;
  status: VisionStatus;
  detail: string | null;
  models: VisionModelInfo[];
  classes: string[];
  download: { receivedBytes: number; totalBytes: number } | null;
  backend: string | null;
  inferenceMs: number | null;
  inferenceFps: number | null;
  droppedFrames: number;
  counts: CountSummary;
}

export interface NormalizedBox {
  x: number;
  y: number;
  width: number;
  height: number;
}

/** One tracked object, as sent on `vision://detections`. */
export interface DetectionResult {
  class: string;
  confidence: number;
  /** Source-video pixels, when the source size is known. */
  bbox: NormalizedBox | null;
  normalizedBbox: NormalizedBox;
  trackId: number;
}

export interface DetectionEvent {
  timestampMs: number;
  frameWidth: number | null;
  frameHeight: number | null;
  detections: DetectionResult[];
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
