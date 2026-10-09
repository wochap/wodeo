import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import type {
  ExportRequest,
  ExportResult,
  LaunchOptions,
  VideoMetadata,
} from "./types";
/** Webview URL for a thumbnail file written by the backend. */
export const thumbnailSrc = (path: string) => convertFileSrc(path);
export const backend = {
  launchOptions: () => invoke<LaunchOptions>("take_launch_options"),
  loadInput: async (path: string, loadId: number) => {
    const m = await invoke<VideoMetadata>("load_input", { path, loadId });
    return {
      ...m,
      thumbnails: m.thumbnails.map(thumbnailSrc),
    };
  },
  playbackAcceleration: () =>
    invoke<import("./types").AccelerationRecord[]>("playback_acceleration"),
  exportVideo: (request: ExportRequest) =>
    invoke<ExportResult>("start_export", { request }),
  cancelExport: () => invoke<void>("cancel_export"),
  exit: (code: number) => invoke<void>("exit_application", { code }),
};
