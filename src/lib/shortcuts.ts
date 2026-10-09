export type ShortcutGroup = "Playback" | "Navigate" | "Trim" | "Actions";
/** What a shortcut moves: the playhead, the focused trim handle, or whichever has focus. */
export type ShortcutScope = "playhead" | "handle" | "either";
export interface Shortcut {
  /** Key caps, in order; `"0–9"` is one cap. */
  keys: string[];
  /** Short key bar label. */
  label: string;
  /** Full meaning, used in the reference and as the key bar tooltip. */
  title: string;
  group: ShortcutGroup;
  scope: ShortcutScope;
  /** Key bar membership. */
  bar?: "normal" | "handle";
  /** `3` drops first as the key bar narrows. */
  priority?: 3;
}
export const SHORTCUT_GROUPS: ShortcutGroup[] = [
  "Playback",
  "Navigate",
  "Trim",
  "Actions",
];
/** Every shortcut wodeo binds; the key bar and the reference read from here. */
export const SHORTCUTS: Shortcut[] = [
  {
    keys: ["Space"],
    label: "Play",
    title: "Play / pause",
    group: "Playback",
    scope: "playhead",
    bar: "normal",
  },
  {
    keys: ["←", "→"],
    label: "Frame",
    title: "±1 frame",
    group: "Navigate",
    scope: "either",
    bar: "normal",
  },
  {
    keys: ["Shift", "←", "→"],
    label: "±1 s",
    title: "±1 second",
    group: "Navigate",
    scope: "either",
  },
  {
    keys: ["PgUp", "PgDn"],
    label: "±10 s",
    title: "±10 seconds",
    group: "Navigate",
    scope: "either",
  },
  {
    keys: ["Alt", "←", "→"],
    label: "Keyframe",
    title: "Previous / next keyframe",
    group: "Navigate",
    scope: "either",
  },
  {
    keys: ["0–9"],
    label: "Jump",
    title: "Jump to 0–90%",
    group: "Navigate",
    scope: "either",
  },
  {
    keys: ["I", "O"],
    label: "In / out",
    title: "Set in / out at the playhead",
    group: "Trim",
    scope: "playhead",
    bar: "normal",
    priority: 3,
  },
  {
    keys: ["Home", "End"],
    label: "Limits",
    title: "Handle to its start / end limit",
    group: "Trim",
    scope: "handle",
  },
  {
    keys: ["Enter"],
    label: "Trim",
    title: "Trim & save",
    group: "Actions",
    scope: "playhead",
  },
  {
    keys: ["Esc"],
    label: "Cancel",
    title: "Cancel",
    group: "Actions",
    scope: "playhead",
  },
  {
    keys: ["Ctrl", "O"],
    label: "Open",
    title: "Open video",
    group: "Actions",
    scope: "playhead",
  },
  {
    keys: ["?"],
    label: "All keys",
    title: "This reference",
    group: "Actions",
    scope: "playhead",
  },
];
/** Hints shown in handle mode: Frame, ±1 s, and Keyframe (dropped first). */
export const HANDLE_HINTS: Shortcut[] = [
  { ...SHORTCUTS[1], keys: ["←", "→"] },
  { ...SHORTCUTS[2], keys: ["Shift"] },
  { ...SHORTCUTS[4], keys: ["Alt"], priority: 3 },
];
export const NORMAL_HINTS = SHORTCUTS.filter((s) => s.bar === "normal");
