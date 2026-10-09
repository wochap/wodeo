type NavigationKey = Pick<
  KeyboardEvent,
  "key" | "shiftKey" | "altKey" | "ctrlKey" | "metaKey"
>;
type NavigationContext = {
  value: number;
  duration: number;
  step: number;
  keyframes: number[];
};

/** True for Alt+←/→, which the webview would otherwise treat as history navigation. */
export const isAltArrow = (e: NavigationKey) =>
  e.altKey && (e.key === "ArrowLeft" || e.key === "ArrowRight");

/**
 * Maps a navigation key to a new time in microseconds, clamped to
 * `[0, duration]`, or `null` when the key is not a navigation key or an
 * Alt+arrow has no keyframe in that direction. `keyframes` must be sorted.
 */
export function navigationTarget(
  e: NavigationKey,
  { value, duration, step, keyframes }: NavigationContext,
): number | null {
  let v: number | undefined;
  const dir = e.key === "ArrowLeft" ? -1 : e.key === "ArrowRight" ? 1 : 0;
  if (dir && e.altKey)
    v =
      dir < 0
        ? [...keyframes].reverse().find((k) => k < value)
        : keyframes.find((k) => k > value);
  else if (dir) v = value + dir * (e.shiftKey ? 1_000_000 : step);
  else if (e.key === "PageUp") v = value + 10_000_000;
  else if (e.key === "PageDown") v = value - 10_000_000;
  else if (/^[0-9]$/.test(e.key) && !e.ctrlKey && !e.altKey && !e.metaKey)
    v = (duration * Number(e.key)) / 10;
  if (v === undefined) return null;
  return Math.max(0, Math.min(duration, Math.round(v)));
}
