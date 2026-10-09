import {
  CaretLeft,
  CaretRight,
  Pause,
  Play,
  PlayCircle,
  SkipBack,
  SkipForward,
} from "@phosphor-icons/react";
import { formatMicros } from "@/lib/time";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { KeyBar } from "@/components/KeyBar";
export function Transport({
  enabled,
  playing,
  playhead,
  duration,
  onGoToIn,
  onPreviousFrame,
  onToggle,
  onNextFrame,
  onGoToOut,
  onPlaySelection,
  focusedHandle,
  referenceOpen,
  onToggleReference,
  moreButton,
}: {
  enabled: boolean;
  playing: boolean;
  playhead: number;
  /** `null` before metadata; `undefined` with no video. */
  duration: number | null | undefined;
  onGoToIn: () => void;
  onPreviousFrame: () => void;
  onToggle: () => void;
  onNextFrame: () => void;
  onGoToOut: () => void;
  onPlaySelection: () => void;
  focusedHandle: "start" | "end" | null;
  referenceOpen: boolean;
  onToggleReference: () => void;
  moreButton: React.Ref<HTMLButtonElement>;
}) {
  const icon = (
    label: string,
    onClick: () => void,
    children: React.ReactNode,
  ) => (
    <Button
      size="icon"
      aria-label={label}
      title={label}
      disabled={!enabled}
      onClick={onClick}
    >
      {children}
    </Button>
  );
  return (
    <div className="flex h-[52px] shrink-0 items-center gap-1.5 border-t border-divider px-4">
      <div className="flex items-center gap-1.5">
        {icon("Go to in", onGoToIn, <Icon icon={SkipBack} />)}
        {icon("Previous frame", onPreviousFrame, <Icon icon={CaretLeft} />)}
        <Button
          size="icon"
          variant="primary"
          aria-label={playing ? "Pause" : "Play"}
          title={playing ? "Pause" : "Play"}
          disabled={!enabled}
          onClick={onToggle}
        >
          {playing ? (
            <Icon icon={Pause} weight="fill" />
          ) : (
            <Icon icon={Play} weight="fill" />
          )}
        </Button>
        {icon("Next frame", onNextFrame, <Icon icon={CaretRight} />)}
        {icon("Go to out", onGoToOut, <Icon icon={SkipForward} />)}
        <Button
          variant="ghost"
          className="ml-1 text-[13px]"
          disabled={!enabled}
          onClick={onPlaySelection}
        >
          <Icon icon={PlayCircle} /> Play selection
        </Button>
      </div>
      <div
        aria-label="Playhead"
        className={cn(
          "ml-2.5 font-mono text-[15px] font-medium tracking-[0.02em] whitespace-nowrap tabular-nums",
          !duration && "text-neutral-500",
        )}
      >
        {duration === undefined ? "—:——.———" : formatMicros(playhead)}
        {duration !== undefined && (
          <span className="text-neutral-600">
            {" / "}
            {duration === null ? "—" : formatMicros(duration)}
          </span>
        )}
      </div>
      <KeyBar
        ref={moreButton}
        focusedHandle={focusedHandle}
        referenceOpen={referenceOpen}
        onToggleReference={onToggleReference}
      />
    </div>
  );
}
