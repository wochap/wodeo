import { forwardRef } from "react";
import { HANDLE_HINTS, NORMAL_HINTS, type Shortcut } from "@/lib/shortcuts";
import { Button } from "@/components/ui/button";
import { Kbd } from "@/components/ui/kbd";
export function KeyHint({ hint }: { hint: Shortcut }) {
  return (
    <li className="keyhint" title={hint.title} data-p={hint.priority}>
      <span className="keys">
        {hint.keys.map((k) => (
          <Kbd key={k}>{k}</Kbd>
        ))}
      </span>
      <span className="keyhint-label">{hint.label}</span>
    </li>
  );
}
/** Always-on transport hints; degrades by its own width (see `.keybar` CSS). */
export const KeyBar = forwardRef<
  HTMLButtonElement,
  {
    focusedHandle: "start" | "end" | null;
    referenceOpen: boolean;
    onToggleReference: () => void;
  }
>(function KeyBar({ focusedHandle, referenceOpen, onToggleReference }, ref) {
  const hints = focusedHandle ? HANDLE_HINTS : NORMAL_HINTS;
  return (
    <div className="keybar">
      <div
        className="keybar-row"
        data-mode={focusedHandle ? "handle" : "normal"}
      >
        {focusedHandle && (
          <span className="keybar-mode">
            {focusedHandle === "start" ? "In handle" : "Out handle"}
          </span>
        )}
        <ul aria-label="Key hints" className="keybar-row">
          {hints.map((h) => (
            <KeyHint key={h.label} hint={h} />
          ))}
        </ul>
        <Button
          ref={ref}
          className="btn keybar-more"
          aria-label="All keys"
          aria-expanded={referenceOpen}
          aria-controls="shortcut-reference"
          title="All shortcuts (?)"
          onClick={onToggleReference}
        >
          <Kbd>?</Kbd>
          <span className="keybar-more-label">All keys</span>
        </Button>
      </div>
    </div>
  );
});
