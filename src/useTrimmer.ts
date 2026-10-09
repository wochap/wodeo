import { useCallback, useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import { backend, thumbnailSrc } from "@/lib/backend";
import { defaultOutput, joinOutput, splitOutput } from "@/lib/output";
import {
  clampRange,
  frameStepMicros,
  microsToSeconds,
  secondsToMicros,
} from "@/lib/time";
import type {
  AccelerationRecord,
  ExportFormat,
  ExportProgress,
  ExportQuality,
  InspectProgress,
  InspectStep,
  InspectThumbnail,
  InspectThumbnailsDone,
  LaunchOptions,
  VideoMetadata,
} from "@/lib/types";
export type Phase = "empty" | "loading" | "ready" | "exporting" | "error";
/**
 * Advisory inspection state. `thumbnails` stays live after the editor is
 * ready and holds `null` for cells not yet written.
 */
export interface Inspection {
  step: InspectStep | null;
  fraction: number;
  thumbnails: (string | null)[];
  thumbnailWarning?: string;
}
const NO_INSPECTION: Inspection = { step: null, fraction: 0, thumbnails: [] };
const supported = (p: string, extensions: string[]) => {
  const ext = /\.([^./]+)$/.exec(p)?.[1]?.toLowerCase();
  return !!ext && extensions.includes(ext);
};
// Export records replace earlier export records but keep playback ones.
const withExport = (
  previous: AccelerationRecord[],
  exported: AccelerationRecord[],
) => [
  ...previous.filter((r) => !r.component.startsWith("export_")),
  ...exported,
];
const errorMessage = (error: unknown) =>
  typeof error === "string"
    ? error
    : error &&
        typeof error === "object" &&
        "message" in error &&
        typeof error.message === "string"
      ? error.message
      : "An unexpected error occurred.";
/** Editor state, playback, keyboard, and export logic shared by the editor views. */
export function useTrimmer() {
  const [phase, setPhase] = useState<Phase>("empty"),
    [video, setVideo] = useState<VideoMetadata | null>(null),
    [error, setError] = useState(""),
    [savedPath, setSavedPath] = useState(""),
    [pendingPath, setPendingPath] = useState(""),
    [format, setFormat] = useState<ExportFormat>("mp4"),
    [quality, setQuality] = useState<ExportQuality>("original"),
    [outputDir, setOutputDir] = useState(""),
    [outputStem, setOutputStem] = useState(""),
    [drag, setDrag] = useState(false),
    [start, setStart] = useState(0),
    [end, setEnd] = useState(0),
    [playhead, setPlayhead] = useState(0),
    [playing, setPlaying] = useState(false),
    [previewOk, setPreviewOk] = useState(false),
    [frameReady, setFrameReady] = useState(false),
    [progress, setProgress] = useState<ExportProgress | null>(null),
    [inspection, setInspection] = useState<Inspection>(NO_INSPECTION),
    [confirm, setConfirm] = useState(false),
    [launch, setLaunch] = useState<LaunchOptions>({
      input: null,
      output: null,
      format: "mp4",
      quality: "original",
      onDone: "exit",
      verbose: false,
      inputExtensions: [],
    }),
    [acceleration, setAcceleration] = useState<AccelerationRecord[]>([]);
  const player = useRef<HTMLVideoElement>(null),
    cancelRequested = useRef(false),
    launchRequested = useRef(false),
    // Inspection events carry the id of the load that produced them; only the
    // active load may update state, so a replaced load can never leak in.
    loadCounter = useRef(0),
    activeLoad = useRef(0),
    // Thumbnails keep arriving after ready, so they follow their own load id.
    thumbnailLoad = useRef(0),
    inspectionRef = useRef(inspection),
    // `--output` names only the first loaded input; replacements derive their own.
    launchOutput = useRef<string | null>(null),
    // Read through a ref so a load started from the launch handler sees it.
    inputExtensions = useRef<string[]>([]),
    boundedStop = useRef<number | null>(null),
    boundedVersion = useRef(0),
    boundedSeekTarget = useRef<number | null>(null),
    // At most one media seek runs at a time; the newest request waits in
    // `pendingSeek` and is applied on `seeked`.
    seekInFlight = useRef(false),
    pendingSeek = useRef<number | null>(null);
  inspectionRef.current = inspection;
  const step = frameStepMicros(video?.frameRate ?? 30);
  const outputPath =
    video && outputStem.trim()
      ? joinOutput(outputDir, outputStem.trim(), format, video.path)
      : "";
  const load = useCallback(
    async (path: string) => {
      if (phase === "exporting") return;
      if (!supported(path, inputExtensions.current)) {
        setError(
          `Choose exactly one local video (${inputExtensions.current.join(", ")}).`,
        );
        setPhase(video ? "ready" : "error");
        return;
      }
      boundedStop.current = null;
      boundedSeekTarget.current = null;
      boundedVersion.current += 1;
      const id = ++loadCounter.current;
      const previous = {
        inspection: inspectionRef.current,
        thumbnailLoad: thumbnailLoad.current,
      };
      activeLoad.current = id;
      thumbnailLoad.current = id;
      setInspection(NO_INSPECTION);
      setPhase("loading");
      setPendingPath(path);
      setError("");
      setSavedPath("");
      try {
        const next = await backend.loadInput(path, id);
        if (activeLoad.current !== id) return;
        activeLoad.current = 0;
        const out = launchOutput.current
          ? splitOutput(launchOutput.current)
          : defaultOutput(next.path);
        launchOutput.current = null;
        setOutputDir(out.dir);
        setOutputStem(out.stem);
        setVideo(next);
        setStart(0);
        setEnd(next.durationMicros);
        setPlayhead(0);
        setPreviewOk(false);
        setFrameReady(false);
        setAcceleration(next.playbackAcceleration);
        setInspection((s) => ({
          ...s,
          thumbnails: s.thumbnails.length ? s.thumbnails : next.thumbnails,
          thumbnailWarning: next.thumbnailWarning ?? s.thumbnailWarning,
        }));
        setPhase("ready");
      } catch (e) {
        if (activeLoad.current !== id) return;
        activeLoad.current = 0;
        // The previous video stays open, so its strip does too.
        thumbnailLoad.current = previous.thumbnailLoad;
        setInspection({ ...previous.inspection, step: null, fraction: 0 });
        setError(errorMessage(e));
        setPhase(video ? "ready" : "error");
      } finally {
        if (loadCounter.current === id) setPendingPath("");
      }
    },
    [phase, video],
  );
  const pick = useCallback(async () => {
    if (phase === "loading" || phase === "exporting") return;
    const p = await open({
      multiple: false,
      directory: false,
      filters: [{ name: "Video", extensions: inputExtensions.current }],
    });
    if (typeof p === "string") await load(p);
  }, [load, phase]);
  const chooseFolder = async () => {
    const dir = await open({
      directory: true,
      multiple: false,
      defaultPath: outputDir || undefined,
    });
    if (typeof dir === "string") setOutputDir(dir);
  };
  useEffect(() => {
    if (launchRequested.current) return;
    launchRequested.current = true;
    backend
      .launchOptions()
      .then((o) => {
        setLaunch(o);
        inputExtensions.current = o.inputExtensions;
        setFormat(o.format);
        setQuality(o.quality);
        launchOutput.current = o.output;
        if (o.input) void load(o.input);
      })
      .catch((e) => {
        setError(errorMessage(e));
        setPhase("error");
      });
  }, []);
  useEffect(() => {
    let stop: undefined | (() => void);
    getCurrentWindow()
      .onDragDropEvent((e) => {
        if (phase === "exporting") return;
        if (e.payload.type === "over") setDrag(true);
        else if (e.payload.type === "leave") setDrag(false);
        else {
          setDrag(false);
          const paths = e.payload.paths;
          if (
            paths.length === 1 &&
            supported(paths[0], inputExtensions.current)
          )
            void load(paths[0]);
          else {
            setError(
              `Drop exactly one video (${inputExtensions.current.join(", ")}).`,
            );
            if (!video) setPhase("error");
          }
        }
      })
      .then((u) => (stop = u));
    return () => stop?.();
  }, [load, phase, video]);
  useEffect(() => {
    const stops = [
      listen<ExportProgress>("export-progress", (e) => setProgress(e.payload)),
      listen<AccelerationRecord[]>("acceleration-update", (e) =>
        setAcceleration((previous) => withExport(previous, e.payload)),
      ),
      listen<InspectProgress>("inspect-progress", ({ payload: p }) => {
        if (p.loadId !== activeLoad.current) return;
        setInspection((s) => ({ ...s, step: p.step, fraction: p.fraction }));
      }),
      listen<InspectThumbnail>("inspect-thumbnail", ({ payload: t }) => {
        if (t.loadId !== thumbnailLoad.current) return;
        setInspection((s) => {
          const thumbnails = Array.from(
            { length: t.count },
            (_, i) => s.thumbnails[i] ?? null,
          );
          thumbnails[t.index] = thumbnailSrc(t.path);
          return { ...s, thumbnails };
        });
      }),
      listen<InspectThumbnailsDone>(
        "inspect-thumbnails-done",
        ({ payload: d }) => {
          if (d.loadId !== thumbnailLoad.current) return;
          setInspection((s) => ({
            ...s,
            thumbnails: d.thumbnails.map(thumbnailSrc),
            thumbnailWarning: d.warning ?? undefined,
          }));
        },
      ),
    ];
    return () => {
      for (const p of stops) void p.then((u) => u());
    };
  }, []);
  useEffect(() => {
    if (!previewOk) return;
    const refresh = () =>
      void backend
        .playbackAcceleration()
        .then((records) =>
          setAcceleration((previous) => [
            ...records,
            ...previous.filter((r) => r.component.startsWith("export_")),
          ]),
        );
    refresh();
    const id = window.setInterval(refresh, 2000);
    return () => window.clearInterval(id);
  }, [previewOk]);
  const resetSeeks = () => {
    seekInFlight.current = false;
    pendingSeek.current = null;
  };
  useEffect(resetSeeks, [video?.previewUrl]);
  /** Seeks the media now, replacing any queued seek. */
  const applySeek = (el: HTMLVideoElement, micros: number) => {
    pendingSeek.current = null;
    el.currentTime = microsToSeconds(micros);
    // Without metadata the browser stores the position and fires no `seeked`.
    seekInFlight.current = el.readyState >= el.HAVE_METADATA;
  };
  const seek = (v: number) => {
    boundedStop.current = null;
    boundedSeekTarget.current = null;
    boundedVersion.current += 1;
    const n = Math.max(0, Math.min(v, video?.durationMicros ?? 0));
    setPlayhead(n);
    const el = player.current;
    if (!el) return;
    if (seekInFlight.current) pendingSeek.current = n;
    else applySeek(el, n);
  };
  const range = (s: number, e: number, boundary?: "start" | "end") => {
    if (!video) return;
    const r = clampRange(s, e, video.durationMicros, step);
    setStart(r.start);
    setEnd(r.end);
    seek(
      boundary === "start"
        ? r.start
        : boundary === "end"
          ? r.end
          : Math.max(r.start, Math.min(playhead, r.end)),
    );
  };
  const playInterval = (intervalStart: number, intervalEnd: number) => {
    const el = player.current;
    if (!el || !video || !previewOk || phase !== "ready") return;
    boundedStop.current = null;
    boundedVersion.current += 1;
    const version = boundedVersion.current;
    boundedSeekTarget.current = intervalStart;
    applySeek(el, intervalStart);
    setPlayhead(intervalStart);
    boundedStop.current = intervalEnd;
    void el.play().catch(() => {
      if (boundedVersion.current === version) boundedStop.current = null;
    });
  };
  const stopBoundedPlayback = (el: HTMLVideoElement) => {
    const stop = boundedStop.current;
    if (stop === null) return false;
    boundedStop.current = null;
    boundedSeekTarget.current = stop;
    boundedVersion.current += 1;
    el.pause();
    applySeek(el, stop);
    setPlayhead(stop);
    return true;
  };
  const syncPlaybackTime = (el: HTMLVideoElement) => {
    const current = secondsToMicros(el.currentTime);
    setPlayhead(current);
    if (boundedStop.current !== null && current >= boundedStop.current)
      stopBoundedPlayback(el);
  };
  const toggle = () => {
    const el = player.current;
    if (!el) return;
    boundedStop.current = null;
    boundedSeekTarget.current = null;
    boundedVersion.current += 1;
    if (el.paused) void el.play().catch(() => {});
    else el.pause();
  };
  const trim = useCallback(async () => {
    if (!video || !previewOk || phase !== "ready" || !outputPath) return;
    cancelRequested.current = false;
    boundedStop.current = null;
    boundedSeekTarget.current = null;
    boundedVersion.current += 1;
    player.current?.pause();
    setPhase("exporting");
    setProgress(null);
    setError("");
    setSavedPath("");
    try {
      const result = await backend.exportVideo({
        input: video.path,
        output: outputPath,
        startMicros: start,
        endMicros: end,
        format,
        quality,
      });
      setAcceleration((previous) => withExport(previous, result.acceleration));
      if (launch.onDone === "exit") {
        await backend.exit(0);
        return;
      }
      setProgress(null);
      setSavedPath(result.output);
      setPhase("ready");
    } catch (e) {
      if (cancelRequested.current) {
        await backend.exit(130);
        return;
      }
      setProgress(null);
      setError(errorMessage(e));
      setPhase("ready");
    }
  }, [
    video,
    previewOk,
    phase,
    launch,
    start,
    end,
    format,
    quality,
    outputPath,
  ]);
  const cancel = () =>
    phase === "exporting" ? setConfirm(true) : void backend.exit(130);
  useEffect(() => {
    const key = (e: globalThis.KeyboardEvent) => {
      const t = e.target;
      if (
        t instanceof HTMLElement &&
        (t.matches("input,textarea,select,[contenteditable=true]") ||
          t.getAttribute("role") === "dialog")
      )
        return;
      if (e.ctrlKey && e.key.toLowerCase() === "o") {
        e.preventDefault();
        void pick();
        return;
      }
      if (
        !video &&
        (phase === "empty" || phase === "error") &&
        e.key === "Enter" &&
        !(t instanceof HTMLElement && t.closest("button"))
      ) {
        e.preventDefault();
        void pick();
        return;
      }
      if (phase !== "ready") return;
      let handled = true;
      if (e.key === " ") toggle();
      else if (e.key === "ArrowLeft")
        seek(playhead - (e.shiftKey ? 1_000_000 : step));
      else if (e.key === "ArrowRight")
        seek(playhead + (e.shiftKey ? 1_000_000 : step));
      else if (e.key.toLowerCase() === "i") range(playhead, end);
      else if (e.key.toLowerCase() === "o") range(start, playhead);
      else if (e.key === "Enter") void trim();
      else if (e.key === "Escape") cancel();
      else handled = false;
      if (handled) e.preventDefault();
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [phase, pick, playhead, step, start, end, trim, video]);
  const clearBounded = () => {
    boundedStop.current = null;
    boundedSeekTarget.current = null;
    boundedVersion.current += 1;
  };
  const videoEvents = {
    onLoadedMetadata: () => setPreviewOk(true),
    onLoadedData: () => setFrameReady(true),
    onError: () => {
      clearBounded();
      resetSeeks();
      setPreviewOk(false);
      setError(
        "This MP4 was inspected successfully, but WebKit/GStreamer cannot preview it.",
      );
    },
    onTimeUpdate: (e: { currentTarget: HTMLVideoElement }) =>
      syncPlaybackTime(e.currentTarget),
    onEnded: (e: { currentTarget: HTMLVideoElement }) =>
      stopBoundedPlayback(e.currentTarget),
    onSeeking: (e: { currentTarget: HTMLVideoElement }) => {
      const current = secondsToMicros(e.currentTarget.currentTime);
      const target = boundedSeekTarget.current;
      if (target === null || Math.abs(current - target) > step) clearBounded();
    },
    onSeeked: (e: { currentTarget: HTMLVideoElement }) => {
      const el = e.currentTarget;
      const next = pendingSeek.current;
      if (next !== null && secondsToMicros(el.currentTime) !== next) {
        applySeek(el, next);
        return;
      }
      pendingSeek.current = null;
      seekInFlight.current = false;
      boundedSeekTarget.current = null;
    },
    onPlay: () => setPlaying(true),
    onPause: () => {
      setPlaying(false);
      clearBounded();
    },
  };
  // Typed boundaries snap to the frame grid before the usual range clamping.
  const snap = (v: number) => Math.round(v / step) * step;
  const setIn = (v: number) =>
    range(Math.min(snap(v), end - step), end, "start");
  const setOut = (v: number) =>
    range(
      start,
      Math.max(Math.min(snap(v), video?.durationMicros ?? 0), start + step),
      "end",
    );
  const confirmCancel = () => {
    cancelRequested.current = true;
    void backend.cancelExport();
  };
  return {
    phase,
    video,
    error,
    savedPath,
    pendingPath,
    format,
    setFormat,
    quality,
    setQuality,
    outputDir,
    setOutputDir,
    outputStem,
    setOutputStem,
    outputPath,
    chooseFolder,
    setIn,
    setOut,
    drag,
    start,
    end,
    playhead,
    playing,
    previewOk,
    frameReady,
    progress,
    inspection,
    confirm,
    setConfirm,
    launch,
    acceleration,
    step,
    player,
    videoEvents,
    pick,
    seek,
    range,
    playInterval,
    toggle,
    trim,
    cancel,
    confirmCancel,
  };
}
