import {
  SHORTCUT_GROUPS,
  SHORTCUTS,
  type ShortcutGroup,
} from "@/lib/shortcuts";
import { Kbd } from "@/components/ui/kbd";
function Group({ group }: { group: ShortcutGroup }) {
  return (
    <section className="keyref-group" aria-label={group}>
      <h3 className="keyref-title">{group}</h3>
      {group === "Navigate" && (
        <p className="keyref-note">
          <span className="keyref-scope" data-scope="playhead">
            Playhead
          </span>
          or the
          <span className="keyref-scope" data-scope="handle">
            focused handle
          </span>
        </p>
      )}
      {SHORTCUTS.filter((s) => s.group === group).map((s) => (
        <div key={s.title} className="keyref-row">
          <span className="keys">
            {s.keys.map((k) => (
              <Kbd key={k}>{k}</Kbd>
            ))}
          </span>
          <span className="keyref-act">
            {s.title}
            {s.scope === "handle" && (
              <span className="keyref-scope" data-scope="handle">
                Focused handle only
              </span>
            )}
          </span>
        </div>
      ))}
    </section>
  );
}
/** Non-modal reference of every shortcut, opened by `?`. */
export function ShortcutReference() {
  return (
    <div
      id="shortcut-reference"
      role="dialog"
      aria-label="Keyboard shortcuts"
      className="keyref absolute right-4 bottom-[60px] z-20 max-w-[calc(100%-32px)]"
    >
      <div className="keyref-head">
        <h2 className="keyref-heading">Keyboard shortcuts</h2>
        <span className="ml-auto flex items-center gap-[5px] text-[11.5px] text-neutral-400">
          <Kbd>?</Kbd> or <Kbd>Esc</Kbd> to close
        </span>
      </div>
      <div className="keyref-grid">
        <div className="keyref-col">
          {SHORTCUT_GROUPS.slice(0, 2).map((g) => (
            <Group key={g} group={g} />
          ))}
        </div>
        <div className="keyref-col">
          {SHORTCUT_GROUPS.slice(2).map((g) => (
            <Group key={g} group={g} />
          ))}
        </div>
      </div>
    </div>
  );
}
