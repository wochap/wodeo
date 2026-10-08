import { useEffect, useId, useState, type ReactNode } from "react";
import { Folder, Scissors } from "@phosphor-icons/react";
import { Icon } from "@/components/ui/icon";
import { extension } from "@/lib/output";
import { formatMicros, parseTimecode } from "@/lib/time";
import type { ExportFormat, ExportQuality, VideoMetadata } from "@/lib/types";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Field, Input } from "@/components/ui/field";
import { Segmented } from "@/components/ui/segmented";
const FORMATS: { value: ExportFormat; label: string }[] = [
  { value: "mp4", label: "MP4" },
  { value: "webm", label: "WebM" },
  { value: "gif", label: "GIF" },
  { value: "copy", label: "Copy" },
];
const QUALITIES: { value: ExportQuality; label: string }[] = [
  { value: "original", label: "Original" },
  { value: "high", label: "High" },
  { value: "small", label: "Small" },
];
const seconds = (micros: number) => `${(micros / 1_000_000).toFixed(3)} s`;
/** Last keyframe at or before `start`; `null` without an index. */
export function effectiveCopyStart(keyframes: number[], start: number) {
  let found: number | null = null;
  for (const k of keyframes)
    if (k <= start && (found === null || k > found)) found = k;
  return found;
}
function statusText({
  video,
  format,
  start,
}: {
  video: VideoMetadata | null;
  format: ExportFormat;
  start: number;
}) {
  if (!video) return "Open a video to set a selection";
  if (format !== "copy")
    return `Frame-exact · re-encoded as ${format === "webm" ? "WebM" : format.toUpperCase()}`;
  const k = effectiveCopyStart(video.keyframesMicros, start);
  return k === null
    ? "Fast copy without re-encoding · starts at the nearest earlier keyframe"
    : `Fast copy without re-encoding · output starts at keyframe ${seconds(k)}`;
}
function SectionTitle({ children }: { children: ReactNode }) {
  return (
    <h2 className="text-[11px] tracking-[0.08em] text-neutral-500 uppercase">
      {children}
    </h2>
  );
}
/** Timecode input that edits a local draft and commits on Enter or blur. */
function BoundaryField({
  label,
  value,
  onCommit,
}: {
  label: string;
  value: number | null;
  onCommit: (micros: number) => void;
}) {
  const id = useId();
  const shown = value === null ? "" : formatMicros(value);
  const [draft, setDraft] = useState(shown);
  useEffect(() => setDraft(shown), [shown]);
  const commit = () => {
    const parsed = parseTimecode(draft);
    // A clamped commit may leave the value unchanged, so always resync.
    setDraft(shown);
    if (parsed !== null && value !== null && parsed !== value) onCommit(parsed);
  };
  return (
    <Field label={label} htmlFor={id}>
      <Input
        id={id}
        value={draft}
        placeholder="0:00.000"
        spellCheck={false}
        autoComplete="off"
        className="font-mono"
        onChange={(e) => setDraft(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            commit();
          } else if (e.key === "Escape") {
            e.preventDefault();
            e.stopPropagation();
            setDraft(shown);
          }
        }}
      />
    </Field>
  );
}
export function Sidebar({
  open,
  ready,
  canTrim,
  video,
  start,
  end,
  format,
  quality,
  outputDir,
  outputStem,
  savedPath,
  error,
  onIn,
  onOut,
  onFormat,
  onQuality,
  onOutputDir,
  onOutputStem,
  onChooseFolder,
  onCancel,
  onTrim,
}: {
  /** Floating panel visibility below 960px. */
  open: boolean;
  ready: boolean;
  canTrim: boolean;
  video: VideoMetadata | null;
  start: number;
  end: number;
  format: ExportFormat;
  quality: ExportQuality;
  outputDir: string;
  outputStem: string;
  savedPath: string;
  error: string;
  onIn: (micros: number) => void;
  onOut: (micros: number) => void;
  onFormat: (f: ExportFormat) => void;
  onQuality: (q: ExportQuality) => void;
  onOutputDir: (dir: string) => void;
  onOutputStem: (stem: string) => void;
  onChooseFolder: () => void;
  onCancel: () => void;
  onTrim: () => void;
}) {
  const stemId = useId(),
    dirId = useId();
  const length = end - start;
  const frames = video ? Math.round((length / 1_000_000) * video.frameRate) : 0;
  const percent = video?.durationMicros
    ? Math.round((100 * length) / video.durationMicros)
    : 0;
  return (
    <aside
      id="settings-sidebar"
      aria-label="Trim settings"
      className={cn(
        "flex min-h-0 flex-col gap-3.5 overflow-y-auto bg-surface p-4",
        "min-[960px]:border-l min-[960px]:border-divider",
        open
          ? "max-[959px]:absolute max-[959px]:top-3 max-[959px]:right-3 max-[959px]:bottom-3 max-[959px]:z-20 max-[959px]:w-[320px] max-[959px]:max-w-[calc(100%-24px)] max-[959px]:rounded-lg max-[959px]:shadow-lg"
          : "max-[959px]:hidden",
      )}
    >
      <fieldset
        disabled={!ready}
        className="m-0 flex min-w-0 flex-col gap-3.5 border-0 p-0 disabled:opacity-45 [&_:disabled]:opacity-100!"
      >
        <section className="flex flex-col gap-2">
          <SectionTitle>Selection</SectionTitle>
          <div className="grid grid-cols-2 gap-2.5">
            <BoundaryField
              label="In"
              value={video ? start : null}
              onCommit={onIn}
            />
            <BoundaryField
              label="Out"
              value={video ? end : null}
              onCommit={onOut}
            />
          </div>
          <div className="flex items-baseline justify-between pt-1">
            <span className="text-xs text-neutral-400">Duration</span>
            <span
              aria-label="Selection duration"
              className={cn(
                "font-mono text-[22px] font-medium tabular-nums",
                video ? "text-accent-200" : "text-neutral-500",
              )}
            >
              {video ? formatMicros(length) : "—"}
            </span>
          </div>
          <p className="text-[11.5px] text-neutral-500">
            {video
              ? `${frames.toLocaleString("en-US")} frames · ${percent}% of clip`
              : "No selection"}
          </p>
        </section>
        <section className="flex flex-col gap-[9px]">
          <SectionTitle>Output</SectionTitle>
          <Field label="Format">
            <Segmented
              label="Format"
              value={format}
              options={FORMATS}
              onChange={onFormat}
            />
          </Field>
          <Field label="Quality">
            <Segmented
              label="Quality"
              value={quality}
              options={QUALITIES}
              onChange={onQuality}
              disabled={ready && format === "copy"}
            />
          </Field>
          <Field label="File name" htmlFor={stemId}>
            <div className="relative">
              <Input
                id={stemId}
                value={video ? outputStem : ""}
                placeholder="Set after opening"
                spellCheck={false}
                autoComplete="off"
                className="pr-14"
                onChange={(e) => onOutputStem(e.target.value)}
              />
              <span
                data-testid="output-extension"
                className="pointer-events-none absolute inset-y-0 right-2.5 flex items-center font-mono text-[13px] text-neutral-500"
              >
                .{extension(format, video?.path)}
              </span>
            </div>
          </Field>
          <Field label="Save to" htmlFor={dirId}>
            <div className="flex gap-1.5">
              <Input
                id={dirId}
                value={video ? outputDir : ""}
                placeholder="Same folder as source"
                spellCheck={false}
                autoComplete="off"
                onChange={(e) => onOutputDir(e.target.value)}
              />
              <Button
                size="icon"
                className="shrink-0"
                aria-label="Choose folder"
                title="Choose folder"
                onClick={onChooseFolder}
              >
                <Icon icon={Folder} />
              </Button>
            </div>
          </Field>
        </section>
      </fieldset>
      <div className="mt-auto flex flex-col gap-2">
        <p
          role="status"
          aria-live="polite"
          className={cn(
            "text-[11.5px] break-all",
            error ? "text-danger" : "text-neutral-500",
          )}
        >
          {error ||
            (savedPath
              ? `Saved ${savedPath}`
              : statusText({ video: ready ? video : null, format, start }))}
        </p>
        <div className="flex gap-2">
          <Button className="flex-1" onClick={onCancel}>
            Cancel
          </Button>
          <Button
            variant="primary"
            className="flex-2"
            disabled={!canTrim}
            onClick={onTrim}
          >
            <Icon icon={Scissors} /> Trim &amp; save
          </Button>
        </div>
      </div>
    </aside>
  );
}
