export type AccelerationState =
  "available" | "active" | "software" | "failed" | "unknown";
export type AccelerationComponent =
  "playback_decode" | "playback_render" | "export_decode" | "export_encode";
export interface AccelerationRecord {
  component: AccelerationComponent;
  state: AccelerationState;
  implementation?: string;
  api?: string;
  device?: string;
  reason?: string;
}
export type ExportFormat = "mp4" | "webm" | "gif" | "copy";
export type ExportQuality = "original" | "high" | "small";
export type OnDone = "exit" | "stay";
export interface LaunchOptions {
  input: string | null;
  output: string | null;
  format: ExportFormat;
  quality: ExportQuality;
  onDone: OnDone;
  verbose: boolean;
  /** Supported input extensions, lowercase, without the dot. */
  inputExtensions: string[];
}
export interface VideoMetadata {
  path: string;
  previewUrl: string;
  durationMicros: number;
  width: number;
  height: number;
  codec: string;
  frameRate: number;
  hasAudio: boolean;
  /** ffprobe codec name of the first audio stream, e.g. `aac`. */
  audioCodec?: string | null;
  thumbnails: string[];
  thumbnailWarning?: string;
  playbackAcceleration: AccelerationRecord[];
  keyframesMicros: number[];
}
export interface ExportRequest {
  input: string;
  output: string;
  startMicros: number;
  endMicros: number;
  format: ExportFormat;
  quality: ExportQuality;
}
export interface ExportProgress {
  fraction: number;
  outTimeMicros: number;
  attempt: string;
  /** Bytes muxed so far: FFmpeg `total_size`, else the temporary file size. */
  bytesWritten: number;
  /** Expected output size, sent with the first event; `null` when unknown. */
  estimatedBytes: number | null;
  /** True when `estimatedBytes` comes from a nominal tier bitrate. */
  approximate: boolean;
  /** Smoothed time remaining for the current attempt; `null` until progress starts. */
  remainingMicros: number | null;
  /** Active pipeline step, one of `exportSteps(format)` (copy's seek step carries the time). */
  step: string;
}
export const SEEK_STEP = "Seek to keyframe";
/** Pipeline steps in order; step events name one of these. */
export const exportSteps = (format: ExportFormat) =>
  format === "copy"
    ? [SEEK_STEP, "Copying streams", "Finalizing file"]
    : ["Decoding and encoding", "Validating output", "Finalizing file"];
export interface ExportResult {
  output: string;
  acceleration: AccelerationRecord[];
  /** Where the output actually begins; for `copy` this is the keyframe at or before the selected start, `null` when unknown. */
  effectiveStartMicros: number | null;
}
export const INSPECT_STEPS = [
  "Reading container",
  "Indexing keyframes",
  "Building preview",
  "Building thumbnails",
] as const;
export type InspectStep = (typeof INSPECT_STEPS)[number];
/** Advisory `inspect-progress` event; the `load_input` result stays final. */
export interface InspectProgress {
  loadId: number;
  step: InspectStep;
  fraction: number;
}
/** `inspect-thumbnail` event, sent as soon as thumbnail `index` of `count` is written. */
export interface InspectThumbnail {
  loadId: number;
  index: number;
  count: number;
  path: string;
}
