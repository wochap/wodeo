import { useState } from "react";
import { ConfirmCancel } from "@/components/ui/alert-dialog";
import { ExportDialog } from "@/components/ExportDialog";
import { Header } from "@/components/Header";
import { Sidebar } from "@/components/Sidebar";
import { EmptyState, Inspecting, VideoStage } from "@/components/Stage";
import { ThumbnailStrip, Timeline } from "@/components/Timeline";
import { Transport } from "@/components/Transport";
import { useTrimmer } from "@/useTrimmer";
import { cn } from "@/lib/utils";
export default function VideoTrimmer() {
  const t = useTrimmer();
  const [sidebarOpen, setSidebarOpen] = useState(false);
  const { phase, video } = t;
  const inspecting = phase === "loading";
  const ready = phase === "ready";
  const transportEnabled = ready && t.previewOk;
  return (
    <main className="relative grid h-full grid-rows-[52px_minmax(0,1fr)_176px] bg-bg">
      {t.drag && (
        <div className="pointer-events-none absolute inset-3 z-30 grid place-items-center rounded-lg border-2 border-dashed border-accent bg-bg/90 text-xl font-medium">
          Drop MP4 to open
        </div>
      )}
      <Header
        video={video}
        loadingPath={inspecting ? t.pendingPath : ""}
        acceleration={t.acceleration}
        busy={inspecting || phase === "exporting"}
        sidebarOpen={sidebarOpen}
        onOpen={() => void t.pick()}
        onToggleSidebar={() => setSidebarOpen((v) => !v)}
      />
      <div className="relative grid min-h-0 min-[960px]:grid-cols-[minmax(0,1fr)_320px]">
        <div className="flex min-h-0 min-w-0 flex-col">
          {inspecting ? (
            <Inspecting
              step={t.inspection.step}
              fraction={t.inspection.fraction}
            />
          ) : video ? (
            <VideoStage>
              <video
                key={video.previewUrl}
                ref={t.player}
                src={video.previewUrl}
                className={cn(
                  "max-h-full max-w-full rounded-sm bg-black shadow-sm",
                  !t.frameReady && "invisible",
                )}
                {...t.videoEvents}
              />
            </VideoStage>
          ) : (
            <EmptyState onOpen={() => void t.pick()} />
          )}
          <Transport
            enabled={transportEnabled}
            playing={t.playing}
            playhead={video && !inspecting ? t.playhead : 0}
            duration={
              inspecting ? null : video ? video.durationMicros : undefined
            }
            onGoToIn={() => t.seek(t.start)}
            onPreviousFrame={() => t.seek(t.playhead - t.step)}
            onToggle={t.toggle}
            onNextFrame={() => t.seek(t.playhead + t.step)}
            onGoToOut={() => t.seek(t.end)}
            onPlaySelection={() => t.playInterval(t.start, t.end)}
          />
        </div>
        <Sidebar
          open={sidebarOpen}
          ready={ready && !!video}
          canTrim={ready && t.previewOk && !!t.outputPath}
          video={video}
          start={t.start}
          end={t.end}
          format={t.format}
          quality={t.quality}
          outputDir={t.outputDir}
          outputStem={t.outputStem}
          savedPath={t.savedPath}
          error={t.error}
          onIn={t.setIn}
          onOut={t.setOut}
          onFormat={t.setFormat}
          onQuality={t.setQuality}
          onOutputDir={t.setOutputDir}
          onOutputStem={t.setOutputStem}
          onChooseFolder={() => void t.chooseFolder()}
          onCancel={t.cancel}
          onTrim={() => void t.trim()}
        />
      </div>
      <section
        aria-label="Timeline"
        className="flex min-w-0 flex-col gap-1.5 border-t border-divider bg-surface px-5 pt-2 pb-3"
      >
        <div className="flex items-center gap-2.5 text-xs text-neutral-400">
          <span>Timeline</span>
          {video && !inspecting && t.inspection.thumbnailWarning && (
            <span className="truncate text-neutral-500">
              {t.inspection.thumbnailWarning}
            </span>
          )}
        </div>
        {video && !inspecting ? (
          <Timeline
            duration={video.durationMicros}
            start={t.start}
            end={t.end}
            playhead={t.playhead}
            step={t.step}
            thumbnails={t.inspection.thumbnails}
            onSeek={t.seek}
            onRange={t.range}
          />
        ) : (
          <>
            <div className="h-[18px]" />
            {inspecting ? (
              <ThumbnailStrip thumbnails={t.inspection.thumbnails} />
            ) : (
              <div className="flex h-[84px] items-center rounded-md border-[1.5px] border-dashed border-neutral-800 px-5 text-[12.5px] text-neutral-500">
                Thumbnails and trim handles appear once a video is open.
              </div>
            )}
          </>
        )}
      </section>
      <ExportDialog
        open={phase === "exporting"}
        output={t.outputPath}
        length={t.end - t.start}
        format={t.format}
        progress={t.progress}
        onCancel={() => t.setConfirm(true)}
      />
      <ConfirmCancel
        open={t.confirm}
        onOpenChange={t.setConfirm}
        onConfirm={t.confirmCancel}
      />
    </main>
  );
}
