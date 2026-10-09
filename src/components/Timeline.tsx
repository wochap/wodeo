import { useRef, useState, type KeyboardEvent, type PointerEvent } from "react";
import { isAltArrow, navigationTarget } from "@/lib/navigation";
import { formatMicros } from "@/lib/time";
import { Kbd } from "@/components/ui/kbd";
type Props = {
  duration: number;
  start: number;
  end: number;
  playhead: number;
  step: number;
  keyframes: number[];
  thumbnails: (string | null)[];
  onSeek: (v: number) => void;
  onRange: (s: number, e: number, boundary: "start" | "end") => void;
  onHandleFocus?: (which: "start" | "end" | null) => void;
};
// [major, minor] tick spacing in seconds; the first with at most ten majors wins.
const TICK_STEPS: [number, number][] = [
  [0.1, 0.025],
  [0.25, 0.05],
  [0.5, 0.1],
  [1, 0.25],
  [2, 0.5],
  [5, 1],
  [10, 2],
  [30, 10],
  [60, 15],
  [120, 30],
  [300, 60],
  [600, 120],
  [1800, 600],
  [3600, 900],
];
function tickLabel(micros: number, majorSeconds: number) {
  const total = micros / 1_000_000;
  const h = Math.floor(total / 3600),
    m = Math.floor((total % 3600) / 60),
    s = total % 60;
  const decimals = majorSeconds >= 1 ? 0 : majorSeconds >= 0.5 ? 1 : 2;
  const [whole, frac] = s.toFixed(decimals).split(".");
  const sec = `${whole.padStart(2, "0")}${frac ? `.${frac}` : ""}`;
  return h ? `${h}:${String(m).padStart(2, "0")}:${sec}` : `${m}:${sec}`;
}
/** Ruler ticks for a clip, in microseconds. */
export function rulerTicks(duration: number) {
  const seconds = duration / 1_000_000;
  const [major, minor] =
    TICK_STEPS.find(([m]) => seconds / m <= 10) ??
    TICK_STEPS[TICK_STEPS.length - 1];
  const majorMicros = Math.round(major * 1_000_000),
    minorMicros = Math.round(minor * 1_000_000);
  const majors: { at: number; label: string }[] = [],
    minors: number[] = [];
  for (let at = 0; at <= duration; at += minorMicros) {
    if (at % majorMicros === 0)
      majors.push({ at, label: tickLabel(at, major) });
    else minors.push(at);
  }
  return { majors, minors };
}
const PLACEHOLDER_FRAMES = 9;
/** Inspecting strip: written thumbnails fill left to right, the rest stay placeholders. */
export function ThumbnailStrip({
  thumbnails,
}: {
  thumbnails: (string | null)[];
}) {
  const cells = thumbnails.length
    ? thumbnails
    : Array<null>(PLACEHOLDER_FRAMES).fill(null);
  return (
    <div
      data-testid="timeline-placeholder"
      className="flex h-[84px] gap-0.5 overflow-hidden rounded-md"
    >
      {cells.map((src, i) =>
        src ? (
          <img
            key={i}
            src={src}
            alt=""
            draggable={false}
            className="h-full min-w-0 flex-1 object-cover"
          />
        ) : (
          <div
            key={i}
            data-testid="thumbnail-placeholder"
            className="flex-1 bg-neutral-800"
          />
        ),
      )}
    </div>
  );
}
export function Timeline({
  duration,
  start,
  end,
  playhead,
  step,
  keyframes,
  thumbnails,
  onSeek,
  onRange,
  onHandleFocus,
}: Props) {
  const ref = useRef<HTMLDivElement>(null);
  // The tip shows only for keyboard focus, not after a pointer press.
  const [tip, setTip] = useState<"start" | "end" | null>(null);
  const pointerFocus = useRef(false);
  const focusVisible = (el: Element) => {
    try {
      return el.matches(":focus-visible");
    } catch {
      return true;
    }
  };
  const focusEvents = (which: "start" | "end") => ({
    onFocus: (e: { currentTarget: Element }) => {
      onHandleFocus?.(which);
      setTip(
        !pointerFocus.current && focusVisible(e.currentTarget) ? which : null,
      );
      pointerFocus.current = false;
    },
    onBlur: () => {
      onHandleFocus?.(null);
      setTip(null);
    },
  });
  const isScrubbing = useRef(false);
  const pct = (v: number) => `${duration ? (100 * v) / duration : 0}%`;
  const at = (e: PointerEvent) => {
    const r = ref.current!.getBoundingClientRect();
    return Math.max(
      0,
      Math.min(
        duration,
        Math.round(((e.clientX - r.left) / r.width) * duration),
      ),
    );
  };
  const drag = (which: "start" | "end", e: PointerEvent) => {
    e.currentTarget.setPointerCapture(e.pointerId);
    const v = at(e);
    which === "start"
      ? onRange(Math.min(v, end - step), end, which)
      : onRange(start, Math.max(v, start + step), which);
  };
  const key = (which: "start" | "end", e: KeyboardEvent) => {
    const value = which === "start" ? start : end;
    let v = navigationTarget(e, { value, duration, step, keyframes });
    if (e.key === "Home") v = which === "start" ? 0 : start + step;
    else if (e.key === "End") v = which === "end" ? duration : end - step;
    // Alt+arrow with no keyframe still must not navigate webview history.
    if (v === null && isAltArrow(e)) e.preventDefault();
    if (v === null) return;
    e.preventDefault();
    which === "start"
      ? onRange(Math.max(0, Math.min(v, end - step)), end, which)
      : onRange(start, Math.min(duration, Math.max(v, start + step)), which);
  };
  const { majors, minors } = rulerTicks(duration);
  const grip = (
    <span className="pointer-events-none absolute top-1/2 left-[3px] box-border h-[22px] w-1 -translate-y-1/2 border-x border-bg" />
  );
  const handle =
    "absolute -top-0.5 -bottom-0.5 z-20 w-2.5 cursor-ew-resize rounded-md before:absolute before:inset-y-0 before:-inset-x-1 before:content-['']";
  return (
    <div className="flex flex-col gap-1.5">
      <div
        aria-hidden
        data-testid="timeline-ruler"
        className="relative h-[18px] overflow-hidden"
      >
        {minors.map((t) => (
          <span
            key={t}
            className="absolute bottom-0 h-1 w-px bg-neutral-700"
            style={{ left: pct(t) }}
          />
        ))}
        {majors.map((t) => (
          <span key={t.at}>
            <span
              className="absolute bottom-0 h-2 w-px bg-neutral-500"
              style={{ left: pct(t.at) }}
            />
            <span
              className="absolute top-0 translate-x-1 font-mono text-[10px] leading-none text-neutral-500"
              style={{ left: pct(t.at) }}
            >
              {t.label}
            </span>
          </span>
        ))}
      </div>
      <div
        ref={ref}
        className="relative h-[84px] touch-none"
        onPointerDown={(e) => {
          if (e.target !== e.currentTarget) return;
          e.preventDefault();
          e.currentTarget.setPointerCapture(e.pointerId);
          isScrubbing.current = true;
          onSeek(at(e));
        }}
        onPointerMove={(e) => {
          if (isScrubbing.current && e.buttons) onSeek(at(e));
        }}
        onPointerUp={() => (isScrubbing.current = false)}
        onPointerCancel={() => (isScrubbing.current = false)}
        onLostPointerCapture={() => (isScrubbing.current = false)}
      >
        <div className="pointer-events-none absolute inset-0 flex gap-0.5 overflow-hidden rounded-md">
          {thumbnails.length ? (
            thumbnails.map((t, i) =>
              t ? (
                <img
                  key={i}
                  src={t}
                  alt=""
                  draggable={false}
                  className="h-full min-w-0 flex-1 object-cover"
                />
              ) : (
                <div
                  key={i}
                  data-testid="thumbnail-placeholder"
                  className="flex-1 bg-neutral-800"
                />
              ),
            )
          ) : (
            <div className="flex-1 bg-linear-to-r from-neutral-800 to-neutral-700" />
          )}
        </div>
        <div
          className="pointer-events-none absolute inset-y-0 left-0 rounded-l-md bg-bg/72"
          style={{ width: pct(start) }}
        />
        <div
          className="pointer-events-none absolute inset-y-0 right-0 rounded-r-md bg-bg/72"
          style={{ width: pct(duration - end) }}
        />
        <div
          data-testid="timeline-selection"
          className="pointer-events-none absolute -top-0.5 -bottom-0.5 box-border rounded-md border-2 border-x-10 border-accent shadow-[0_0_18px_color-mix(in_srgb,var(--color-accent)_35%,transparent)]"
          style={{ left: pct(start), width: pct(end - start) }}
        />
        <div
          className="pointer-events-none absolute -top-2.5 -bottom-1 z-10 w-0.5 -translate-x-px bg-text"
          style={{ left: pct(playhead) }}
        />
        <div
          className="pointer-events-none absolute -top-3.5 z-10 size-2.5 -translate-x-1/2 rotate-45 rounded-[2px] bg-text"
          style={{ left: pct(playhead) }}
        />
        <div
          role="slider"
          tabIndex={0}
          aria-label="Trim start"
          aria-valuemin={0}
          aria-valuemax={Math.max(0, end - step)}
          aria-valuenow={start}
          aria-valuetext={formatMicros(start)}
          className={handle}
          style={{ left: pct(start) }}
          {...focusEvents("start")}
          onPointerDown={(e) => {
            e.stopPropagation();
            pointerFocus.current = document.activeElement !== e.currentTarget;
            drag("start", e);
          }}
          onPointerMove={(e) => e.buttons && drag("start", e)}
          onKeyDown={(e) => {
            setTip("start");
            key("start", e);
          }}
        >
          {grip}
        </div>
        <div
          role="slider"
          tabIndex={0}
          aria-label="Trim end"
          aria-valuemin={start + step}
          aria-valuemax={duration}
          aria-valuenow={end}
          aria-valuetext={formatMicros(end)}
          className={`${handle} -translate-x-full`}
          style={{ left: pct(end) }}
          {...focusEvents("end")}
          onPointerDown={(e) => {
            e.stopPropagation();
            pointerFocus.current = document.activeElement !== e.currentTarget;
            drag("end", e);
          }}
          onPointerMove={(e) => e.buttons && drag("end", e)}
          onKeyDown={(e) => {
            setTip("end");
            key("end", e);
          }}
        >
          {grip}
        </div>
        {tip && (
          <div
            data-testid="handle-tip"
            className="keyhint-tip -translate-x-1/2"
            style={{ left: pct(tip === "start" ? start : end) }}
          >
            <span className="font-mono text-text tabular-nums">
              <span className="text-accent">
                {tip === "start" ? "IN" : "OUT"}
              </span>{" "}
              {formatMicros(tip === "start" ? start : end)}
            </span>
            <span className="keyhint">
              <span className="keys">
                <Kbd>Home</Kbd>
                <Kbd>End</Kbd>
              </span>
              <span>Limits</span>
            </span>
          </div>
        )}
      </div>
    </div>
  );
}
