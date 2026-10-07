import { FolderOpen, SidebarSimple } from "@phosphor-icons/react";
import { Icon } from "@/components/ui/icon";
import type { AccelerationRecord, VideoMetadata } from "@/lib/types";
import { splitPath } from "@/lib/output";
import { AccelerationBadge } from "@/components/AccelerationBadge";
import { Button } from "@/components/ui/button";
import { Kbd } from "@/components/ui/kbd";
import { Tag } from "@/components/ui/tag";
const SKELETON_WIDTHS = [64, 36, 58, 44];
const fps = (rate: number) => `${Number(rate.toFixed(2))} fps`;
const audio = (v: VideoMetadata) =>
  v.hasAudio
    ? `${v.audioCodec ? v.audioCodec.toUpperCase() : "With"} audio`
    : "No audio";
export function Header({
  video,
  loadingPath,
  acceleration,
  busy,
  sidebarOpen,
  onOpen,
  onToggleSidebar,
}: {
  video: VideoMetadata | null;
  /** Path being inspected, if any. */
  loadingPath: string;
  acceleration: AccelerationRecord[];
  busy: boolean;
  sidebarOpen: boolean;
  onOpen: () => void;
  onToggleSidebar: () => void;
}) {
  const path = loadingPath || video?.path;
  const file = path ? splitPath(path) : null;
  return (
    <header className="flex min-w-0 items-center gap-3.5 border-b border-divider px-5">
      {file ? (
        <div className="flex min-w-0 flex-col gap-0.5 leading-tight">
          <h1 className="max-w-[380px] truncate text-sm font-medium">
            {file.name}
          </h1>
          <p className="truncate text-[11.5px] text-neutral-500">{file.dir}</p>
        </div>
      ) : (
        <>
          <h1 className="text-sm font-medium whitespace-nowrap">
            Wodeo
          </h1>
          <p className="text-xs whitespace-nowrap text-neutral-500">
            No video open
          </p>
        </>
      )}
      {loadingPath ? (
        <div
          className="ml-2 flex gap-1.5"
          aria-label="Reading metadata"
          role="img"
        >
          {SKELETON_WIDTHS.map((w) => (
            <span
              key={w}
              data-testid="tag-skeleton"
              className="h-5 animate-pulse rounded-sm bg-linear-to-r from-neutral-800 via-surface to-neutral-800"
              style={{ width: w }}
            />
          ))}
        </div>
      ) : (
        video && (
          <div className="ml-2 flex min-w-0 gap-1.5 overflow-hidden max-[760px]:hidden">
            <Tag>
              {video.width}×{video.height}
            </Tag>
            <Tag>{video.codec}</Tag>
            <Tag>{fps(video.frameRate)}</Tag>
            <Tag>{audio(video)}</Tag>
          </div>
        )
      )}
      <div className="ml-auto flex items-center gap-3.5">
        {!loadingPath && (
          <>
            {video && <AccelerationBadge records={acceleration} />}
            {video ? (
              <Button
                variant="ghost"
                className="text-[13px]"
                onClick={onOpen}
                disabled={busy}
              >
                <Icon icon={FolderOpen} /> Replace
              </Button>
            ) : (
              <Button onClick={onOpen} disabled={busy}>
                <Icon icon={FolderOpen} /> Open…
                <Kbd className="ml-1">Ctrl O</Kbd>
              </Button>
            )}
          </>
        )}
        <Button
          size="icon"
          variant="secondary"
          className="min-[960px]:hidden"
          aria-label={sidebarOpen ? "Hide settings" : "Show settings"}
          aria-expanded={sidebarOpen}
          aria-controls="settings-sidebar"
          onClick={onToggleSidebar}
        >
          <Icon icon={SidebarSimple} mirrored />
        </Button>
      </div>
    </header>
  );
}
