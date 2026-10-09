import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ExportProgress, LaunchOptions, VideoMetadata } from "@/lib/types";
type DragDropEvent = { payload: { type: string; paths: string[] } };
const h = vi.hoisted(() => ({
  launch: vi.fn(),
  load: vi.fn(),
  playback: vi.fn(),
  exportVideo: vi.fn(),
  cancel: vi.fn(),
  exit: vi.fn(),
  open: vi.fn(),
  drag: undefined as undefined | ((event: DragDropEvent) => void),
  events: {} as Record<string, (event: { payload: unknown }) => void>,
}));
vi.mock("@/lib/backend", () => ({
  thumbnailSrc: (path: string) =>
    `asset://localhost/${encodeURIComponent(path)}`,
  backend: {
    launchOptions: h.launch,
    loadInput: h.load,
    playbackAcceleration: h.playback,
    exportVideo: h.exportVideo,
    cancelExport: h.cancel,
    exit: h.exit,
  },
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: h.open }));
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    onDragDropEvent: vi.fn(async (cb: (event: DragDropEvent) => void) => {
      h.drag = cb;
      return () => {};
    }),
  }),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(
    async (name: string, cb: (event: { payload: unknown }) => void) => {
      h.events[name] = cb;
      return () => {};
    },
  ),
}));
import VideoTrimmer from "./VideoTrimmer";
const launch: LaunchOptions = {
  input: null,
  output: null,
  format: "mp4",
  quality: "original",
  onDone: "exit",
  verbose: false,
  inputExtensions: ["mp4", "m4v", "mov", "mkv", "webm"],
};
const metadata = (
  path = "/videos/one.mp4",
  warning?: string,
): VideoMetadata => ({
  path,
  previewUrl: "http://127.0.0.1:9/media",
  durationMicros: 2_000_000,
  width: 640,
  height: 360,
  codec: "h264",
  frameRate: 25,
  hasAudio: true,
  audioCodec: "aac",
  thumbnails: warning ? [] : ["asset://thumb.jpg"],
  thumbnailWarning: warning,
  playbackAcceleration: [{ component: "playback_decode", state: "unknown" }],
  keyframesMicros: [0, 1_000_000],
});
beforeEach(() => {
  vi.clearAllMocks();
  h.drag = undefined;
  h.events = {};
  h.launch.mockResolvedValue(launch);
  h.load.mockResolvedValue(metadata());
  h.playback.mockResolvedValue([
    { component: "playback_decode", state: "unknown" },
  ]);
  h.open.mockResolvedValue(null);
});
const trimButton = () => screen.getByRole("button", { name: /trim & save/i });
const field = (name: string) => screen.getByLabelText(name) as HTMLInputElement;
/** Opens `video` through the picker and waits for the ready editor. */
async function ready(video = metadata(), playable = true) {
  h.open.mockResolvedValueOnce(video.path);
  h.load.mockResolvedValueOnce(video);
  render(<VideoTrimmer />);
  await userEvent.click(
    await screen.findByRole("button", { name: "Choose video…" }),
  );
  await screen.findByRole("slider", { name: "Trim start" });
  const el = document.querySelector("video")!;
  if (playable) fireEvent.loadedMetadata(el);
  return el;
}
async function exporting() {
  h.exportVideo.mockReturnValue(new Promise(() => {}));
  await ready();
  await userEvent.click(trimButton());
  return screen.findByRole("alertdialog", { name: "Trimming…" });
}
describe("empty state", () => {
  it("dims the editor and keeps the empty state when the picker is cancelled", async () => {
    render(<VideoTrimmer />);
    expect(screen.getByText("No video open")).toBeVisible();
    expect(trimButton()).toBeDisabled();
    expect(field("In")).toBeDisabled();
    expect(screen.getByRole("radio", { name: "MP4" })).toBeDisabled();
    expect(
      screen.getByRole("button", { name: "Play selection" }),
    ).toBeDisabled();
    expect(screen.getByLabelText("Playhead")).toHaveTextContent("—:——.———");
    expect(
      screen.getByText(
        "Thumbnails and trim handles appear once a video is open.",
      ),
    ).toBeVisible();
    await userEvent.click(
      await screen.findByRole("button", { name: "Choose video…" }),
    );
    expect(h.open).toHaveBeenCalled();
    expect(h.load).not.toHaveBeenCalled();
    expect(screen.getByText("Drop an MP4 here")).toBeVisible();
  });
  it("opens the picker with Enter, Ctrl+O, and the header action", async () => {
    render(<VideoTrimmer />);
    await waitFor(() => expect(h.launch).toHaveBeenCalled());
    fireEvent.keyDown(window, { key: "Enter" });
    await waitFor(() => expect(h.open).toHaveBeenCalledTimes(1));
    fireEvent.keyDown(window, { key: "o", ctrlKey: true });
    await waitFor(() => expect(h.open).toHaveBeenCalledTimes(2));
    await userEvent.click(screen.getByRole("button", { name: /open…/i }));
    await waitFor(() => expect(h.open).toHaveBeenCalledTimes(3));
  });
});
describe("video loading shell", () => {
  it("loads picker and CLI input through the same backend path", async () => {
    h.open.mockResolvedValueOnce("/videos/picked.mp4");
    render(<VideoTrimmer />);
    await userEvent.click(
      await screen.findByRole("button", { name: "Choose video…" }),
    );
    await waitFor(() =>
      expect(h.load).toHaveBeenCalledWith(
        "/videos/picked.mp4",
        expect.any(Number),
      ),
    );
    expect(
      await screen.findByRole("heading", { name: "one.mp4" }),
    ).toBeVisible();
    expect(h.open).toHaveBeenCalledWith(
      expect.objectContaining({
        filters: [
          { name: "Video", extensions: ["mp4", "m4v", "mov", "mkv", "webm"] },
        ],
      }),
    );
    expect(screen.getByText("/videos")).toBeVisible();
    h.launch.mockResolvedValueOnce({ ...launch, input: "/videos/cli.mp4" });
    render(<VideoTrimmer />);
    await waitFor(() =>
      expect(h.load).toHaveBeenCalledWith(
        "/videos/cli.mp4",
        expect.any(Number),
      ),
    );
  });
  it("shows the inspecting state while metadata is pending", async () => {
    let resolve!: (v: VideoMetadata) => void;
    h.load.mockReturnValueOnce(new Promise((r) => (resolve = r)));
    h.open.mockResolvedValueOnce("/home/me/Clips/talk.mp4");
    render(<VideoTrimmer />);
    await userEvent.click(
      await screen.findByRole("button", { name: "Choose video…" }),
    );
    expect(await screen.findByText("Inspecting video…")).toBeVisible();
    expect(
      screen.getByRole("progressbar", { name: "Inspecting video" }),
    ).toBeVisible();
    expect(screen.getByRole("heading", { name: "talk.mp4" })).toBeVisible();
    expect(screen.getByText("/home/me/Clips")).toBeVisible();
    expect(screen.getAllByTestId("tag-skeleton")).toHaveLength(4);
    expect(screen.getByTestId("timeline-placeholder")).toBeVisible();
    expect(screen.getByLabelText("Playhead")).toHaveTextContent("0:00.000 / —");
    expect(trimButton()).toBeDisabled();
    expect(screen.queryByRole("button", { name: /replace/i })).toBeNull();
    await act(async () => resolve(metadata("/home/me/Clips/talk.mp4")));
    expect(
      await screen.findByRole("slider", { name: "Trim start" }),
    ).toBeVisible();
  });
  it("shows completed, active, and pending inspection steps", async () => {
    h.load.mockReturnValueOnce(new Promise(() => {}));
    h.open.mockResolvedValueOnce("/videos/long.mp4");
    render(<VideoTrimmer />);
    await userEvent.click(
      await screen.findByRole("button", { name: "Choose video…" }),
    );
    await screen.findByText("Inspecting video…");
    const loadId = h.load.mock.calls[0][1] as number;
    const steps = () =>
      within(screen.getByRole("list", { name: "Inspection steps" }))
        .getAllByRole("listitem")
        .map((li) => [li.textContent, li.dataset.state]);
    expect(steps()).toEqual([
      ["Reading container", "active"],
      ["Indexing keyframes", "pending"],
      ["Building preview", "pending"],
    ]);
    act(() =>
      h.events["inspect-progress"]({
        payload: { loadId, step: "Indexing keyframes", fraction: 0.1 },
      }),
    );
    expect(steps()).toEqual([
      ["Reading container", "done"],
      ["Indexing keyframes", "active"],
      ["Building preview", "pending"],
    ]);
    expect(
      screen.getByRole("progressbar", { name: "Inspecting video" }),
    ).toHaveAttribute("aria-valuenow", "10");
    expect(screen.getByRole("listitem", { current: "step" })).toHaveTextContent(
      "Indexing keyframes",
    );
  });
  it("fills the strip progressively and ignores events from a replaced load", async () => {
    h.load.mockReturnValue(new Promise(() => {}));
    h.open.mockResolvedValueOnce("/videos/first.mp4");
    render(<VideoTrimmer />);
    await userEvent.click(
      await screen.findByRole("button", { name: "Choose video…" }),
    );
    await screen.findByText("Inspecting video…");
    const first = h.load.mock.calls[0][1] as number;
    const thumb = (loadId: number, index: number) =>
      act(() =>
        h.events["inspect-thumbnail"]({
          payload: {
            loadId,
            index,
            count: 14,
            path: `/cache/${loadId}-${index}.jpg`,
          },
        }),
      );
    thumb(first, 0);
    const strip = screen.getByTestId("timeline-placeholder");
    expect(strip.querySelectorAll("img")).toHaveLength(1);
    act(() =>
      h.drag!({ payload: { type: "drop", paths: ["/videos/second.mp4"] } }),
    );
    await waitFor(() => expect(h.load).toHaveBeenCalledTimes(2));
    const second = h.load.mock.calls[1][1] as number;
    expect(second).not.toBe(first);
    thumb(first, 1);
    act(() =>
      h.events["inspect-progress"]({
        payload: { loadId: first, step: "Building preview", fraction: 0.9 },
      }),
    );
    const current = screen.getByTestId("timeline-placeholder");
    expect(current.querySelectorAll("img")).toHaveLength(0);
    expect(
      within(current).getAllByTestId("thumbnail-placeholder").length,
    ).toBeGreaterThan(0);
    expect(
      screen.getByRole("progressbar", { name: "Inspecting video" }),
    ).toHaveAttribute("aria-valuenow", "0");
    thumb(second, 0);
    const imgs = screen
      .getByTestId("timeline-placeholder")
      .querySelectorAll("img");
    expect(imgs).toHaveLength(1);
    expect(imgs[0].getAttribute("src")).toContain(
      encodeURIComponent(`/cache/${second}-0.jpg`),
    );
  });
  it("keeps filling the strip after ready and applies the done event", async () => {
    let resolve!: (v: VideoMetadata) => void;
    h.load.mockReturnValueOnce(new Promise((r) => (resolve = r)));
    h.open.mockResolvedValueOnce("/videos/one.mp4");
    render(<VideoTrimmer />);
    await userEvent.click(
      await screen.findByRole("button", { name: "Choose video…" }),
    );
    const loadId = h.load.mock.calls[0][1] as number;
    const thumb = (id: number, index: number) =>
      act(() =>
        h.events["inspect-thumbnail"]({
          payload: {
            loadId: id,
            index,
            count: 3,
            path: `/c/${id}-${index}.jpg`,
          },
        }),
      );
    thumb(loadId, 0);
    await act(async () => resolve({ ...metadata(), thumbnails: [] }));
    await screen.findByRole("slider", { name: "Trim start" });
    const strip = () => screen.getByTestId("timeline-selection").parentElement!;
    expect(strip().querySelectorAll("img")).toHaveLength(1);
    expect(
      within(strip()).getAllByTestId("thumbnail-placeholder"),
    ).toHaveLength(2);
    thumb(loadId, 2);
    thumb(loadId + 99, 1);
    expect(strip().querySelectorAll("img")).toHaveLength(2);
    act(() =>
      h.events["inspect-thumbnails-done"]({
        payload: { loadId: loadId + 99, thumbnails: [], warning: "stale" },
      }),
    );
    expect(screen.queryByText("stale")).toBeNull();
    act(() =>
      h.events["inspect-thumbnails-done"]({
        payload: {
          loadId,
          thumbnails: ["/c/a.jpg", "/c/b.jpg", "/c/c.jpg"],
          warning: null,
        },
      }),
    );
    expect(strip().querySelectorAll("img")).toHaveLength(3);
    act(() =>
      h.events["inspect-thumbnails-done"]({
        payload: { loadId, thumbnails: [], warning: "Thumbnails failed" },
      }),
    );
    expect(screen.getByText("Thumbnails failed")).toBeVisible();
    expect(strip().querySelectorAll("img")).toHaveLength(0);
    expect(screen.getByRole("slider", { name: "Trim start" })).toBeVisible();
  });
  it("remounts the video and drops queued seeks when the preview URL changes", async () => {
    const first = await ready({
      ...metadata(),
      previewUrl: "http://127.0.0.1:9/media?load=1",
    });
    const times: number[] = [];
    let current = 0;
    Object.defineProperty(first, "readyState", { value: 4 });
    Object.defineProperty(first, "currentTime", {
      get: () => current,
      set: (v: number) => {
        current = v;
        times.push(v);
      },
    });
    await userEvent.click(screen.getByRole("button", { name: /next frame/i }));
    await userEvent.click(screen.getByRole("button", { name: /next frame/i }));
    expect(times).toHaveLength(1);
    h.open.mockResolvedValueOnce("/clips/two.mp4");
    h.load.mockResolvedValueOnce({
      ...metadata("/clips/two.mp4"),
      previewUrl: "http://127.0.0.1:9/media?load=2",
    });
    await userEvent.click(screen.getByRole("button", { name: /replace/i }));
    await screen.findByRole("heading", { name: "two.mp4" });
    const second = document.querySelector("video")!;
    expect(second).not.toBe(first);
    expect(second.getAttribute("src")).toBe("http://127.0.0.1:9/media?load=2");
    fireEvent.seeked(first);
    expect(times).toHaveLength(1);
  });
  it("shows metadata tags in the loaded header", async () => {
    await ready({
      ...metadata(),
      width: 1920,
      height: 1080,
      frameRate: 30000 / 1001,
    });
    const header = screen.getByRole("banner");
    for (const tag of ["1920×1080", "h264", "29.97 fps", "AAC audio"])
      expect(within(header).getByText(tag)).toBeVisible();
    expect(
      within(header).getByRole("button", {
        name: "Hardware acceleration unknown",
      }),
    ).toBeVisible();
  });
  it("labels a silent video", async () => {
    await ready({ ...metadata(), hasAudio: false, audioCodec: null });
    expect(
      within(screen.getByRole("banner")).getByText("No audio"),
    ).toBeVisible();
  });
  it("accepts one MP4 drop and rejects multiple paths", async () => {
    render(<VideoTrimmer />);
    await waitFor(() => expect(h.drag).toBeTypeOf("function"));
    act(() =>
      h.drag!({
        payload: { type: "drop", paths: ["/videos/drop.mp4"] },
      } as DragDropEvent),
    );
    await waitFor(() =>
      expect(h.load).toHaveBeenCalledWith(
        "/videos/drop.mp4",
        expect.any(Number),
      ),
    );
    act(() =>
      h.drag!({
        payload: { type: "drop", paths: ["a.mp4", "b.mp4"] },
      } as DragDropEvent),
    );
    expect(
      await screen.findByText(
        "Drop exactly one video (mp4, m4v, mov, mkv, webm).",
      ),
    ).toBeVisible();
  });
  it("accepts supported containers on drop and rejects others", async () => {
    render(<VideoTrimmer />);
    await waitFor(() => expect(h.drag).toBeTypeOf("function"));
    act(() =>
      h.drag!({
        payload: { type: "drop", paths: ["/videos/clip.MKV"] },
      } as DragDropEvent),
    );
    await waitFor(() =>
      expect(h.load).toHaveBeenCalledWith(
        "/videos/clip.MKV",
        expect.any(Number),
      ),
    );
    act(() =>
      h.drag!({
        payload: { type: "drop", paths: ["/videos/clip.avi"] },
      } as DragDropEvent),
    );
    expect(
      await screen.findByText(
        "Drop exactly one video (mp4, m4v, mov, mkv, webm).",
      ),
    ).toBeVisible();
  });
  it("reports preview and thumbnail degradation and can replace the input", async () => {
    await ready(metadata("/videos/one.mp4", "Timeline thumbnails unavailable"));
    expect(screen.getByText("Timeline thumbnails unavailable")).toBeVisible();
    fireEvent.error(document.querySelector("video")!);
    expect(screen.getByRole("status")).toHaveTextContent("cannot preview");
    h.open.mockResolvedValueOnce("/clips/two.mp4");
    h.load.mockResolvedValueOnce(metadata("/clips/two.mp4"));
    await userEvent.click(screen.getByRole("button", { name: /replace/i }));
    expect(
      await screen.findByRole("heading", { name: "two.mp4" }),
    ).toBeVisible();
    expect(field("File name")).toHaveValue("two_trim");
    expect(field("Save to")).toHaveValue("/clips");
  });
  it("hides the video until its first frame is decoded", async () => {
    const video = await ready(metadata(), false);
    expect(video).toHaveClass("invisible");
    fireEvent.loadedMetadata(video);
    expect(video).toHaveClass("invisible");
    fireEvent.loadedData(video);
    expect(video).not.toHaveClass("invisible");
    h.open.mockResolvedValueOnce("/clips/two.mp4");
    h.load.mockResolvedValueOnce({
      ...metadata("/clips/two.mp4"),
      previewUrl: "http://127.0.0.1:9/media2",
    });
    await userEvent.click(screen.getByRole("button", { name: /replace/i }));
    await screen.findByRole("heading", { name: "two.mp4" });
    expect(document.querySelector("video")).toHaveClass("invisible");
  });
});
describe("output settings", () => {
  it("preselects launch options and names the output beside the source", async () => {
    h.launch.mockResolvedValue({ ...launch, format: "gif", quality: "small" });
    await ready();
    expect(screen.getByRole("radio", { name: "GIF" })).toBeChecked();
    expect(screen.getByRole("radio", { name: "Small" })).toBeChecked();
    expect(field("File name")).toHaveValue("one_trim");
    expect(screen.getByTestId("output-extension")).toHaveTextContent(".gif");
    expect(field("Save to")).toHaveValue("/videos");
  });
  it("splits --output into folder and name for the first video only", async () => {
    h.launch.mockResolvedValue({ ...launch, output: "/tmp/out/clip.mp4" });
    await ready();
    expect(field("Save to")).toHaveValue("/tmp/out");
    expect(field("File name")).toHaveValue("clip");
    expect(screen.getByTestId("output-extension")).toHaveTextContent(".mp4");
    h.open.mockResolvedValueOnce("/clips/two.mp4");
    h.load.mockResolvedValueOnce(metadata("/clips/two.mp4"));
    await userEvent.click(screen.getByRole("button", { name: /replace/i }));
    await waitFor(() => expect(field("File name")).toHaveValue("two_trim"));
  });
  it("follows the format with the extension and disables quality for Copy", async () => {
    await ready();
    await userEvent.click(screen.getByRole("radio", { name: "WebM" }));
    expect(screen.getByTestId("output-extension")).toHaveTextContent(".webm");
    expect(field("File name")).toHaveValue("one_trim");
    expect(screen.getByRole("radio", { name: "High" })).toBeEnabled();
    await userEvent.click(screen.getByRole("radio", { name: "Copy" }));
    expect(screen.getByTestId("output-extension")).toHaveTextContent(".mp4");
    for (const name of ["Original", "High", "Small"])
      expect(screen.getByRole("radio", { name })).toBeDisabled();
  });
  it("writes to a chosen folder with the edited name, format, and quality", async () => {
    h.exportVideo.mockResolvedValueOnce({
      output: "/tmp/elsewhere/intro.webm",
      acceleration: [],
      effectiveStartMicros: 0,
    });
    await ready();
    h.open.mockResolvedValueOnce("/tmp/elsewhere");
    await userEvent.click(
      screen.getByRole("button", { name: "Choose folder" }),
    );
    await waitFor(() => expect(field("Save to")).toHaveValue("/tmp/elsewhere"));
    expect(h.open).toHaveBeenLastCalledWith(
      expect.objectContaining({ directory: true, defaultPath: "/videos" }),
    );
    await userEvent.clear(field("File name"));
    await userEvent.type(field("File name"), "intro");
    await userEvent.click(screen.getByRole("radio", { name: "WebM" }));
    await userEvent.click(screen.getByRole("radio", { name: "High" }));
    await userEvent.click(trimButton());
    await waitFor(() => expect(h.exit).toHaveBeenCalledWith(0));
    expect(h.exportVideo).toHaveBeenCalledWith({
      input: "/videos/one.mp4",
      output: "/tmp/elsewhere/intro.webm",
      startMicros: 0,
      endMicros: 2_000_000,
      format: "webm",
      quality: "high",
    });
  });
  it("disables Trim & save without a file name", async () => {
    await ready();
    await userEvent.clear(field("File name"));
    expect(trimButton()).toBeDisabled();
  });
});
describe("selection fields", () => {
  it("commits In on Enter and Out on blur, updating the summary", async () => {
    const video = await ready();
    await userEvent.clear(field("In"));
    await userEvent.type(field("In"), "0:00.400{Enter}");
    expect(screen.getByRole("slider", { name: "Trim start" })).toHaveAttribute(
      "aria-valuenow",
      "400000",
    );
    expect(video.currentTime).toBeCloseTo(0.4);
    expect(field("In")).toHaveValue("0:00.400");
    await userEvent.clear(field("Out"));
    await userEvent.type(field("Out"), "1.4");
    await userEvent.tab();
    expect(screen.getByRole("slider", { name: "Trim end" })).toHaveAttribute(
      "aria-valuenow",
      "1400000",
    );
    expect(screen.getByLabelText("Selection duration")).toHaveTextContent(
      "0:01.000",
    );
    expect(screen.getByText("25 frames · 50% of clip")).toBeVisible();
  });
  it("reverts invalid text and Escape without moving a boundary", async () => {
    await ready();
    await userEvent.clear(field("Out"));
    await userEvent.type(field("Out"), "abc");
    await userEvent.tab();
    expect(field("Out")).toHaveValue("0:02.000");
    await userEvent.clear(field("In"));
    await userEvent.type(field("In"), "1{Escape}");
    expect(field("In")).toHaveValue("0:00.000");
    expect(screen.getByRole("slider", { name: "Trim end" })).toHaveAttribute(
      "aria-valuenow",
      "2000000",
    );
    expect(h.exit).not.toHaveBeenCalled();
  });
  it("clamps a typed boundary to a valid frame-aligned selection", async () => {
    await ready();
    await userEvent.clear(field("In"));
    await userEvent.type(field("In"), "9{Enter}");
    expect(field("In")).toHaveValue("0:01.960");
    await userEvent.clear(field("Out"));
    await userEvent.type(field("Out"), "0.51{Enter}");
    expect(field("Out")).toHaveValue("0:02.000");
    await userEvent.clear(field("In"));
    await userEvent.type(field("In"), "0.51{Enter}");
    expect(field("In")).toHaveValue("0:00.520");
  });
  it("keeps editor shortcuts out of a focused field", async () => {
    const video = await ready();
    video.currentTime = 1;
    fireEvent.timeUpdate(video);
    await userEvent.clear(field("In"));
    await userEvent.type(field("In"), "io ");
    expect(screen.getByRole("slider", { name: "Trim start" })).toHaveAttribute(
      "aria-valuenow",
      "0",
    );
    expect(video.play).not.toHaveBeenCalled();
    await userEvent.type(field("In"), "{Enter}");
    expect(h.exportVideo).not.toHaveBeenCalled();
  });
});
describe("transport", () => {
  it("seeks to boundaries and steps frames without moving a boundary", async () => {
    const video = await ready();
    fireEvent.keyDown(screen.getByRole("slider", { name: "Trim start" }), {
      key: "ArrowRight",
    });
    await userEvent.click(screen.getByRole("button", { name: "Go to out" }));
    expect(video.currentTime).toBe(2);
    await userEvent.click(
      screen.getByRole("button", { name: "Previous frame" }),
    );
    expect(video.currentTime).toBeCloseTo(1.96);
    await userEvent.click(screen.getByRole("button", { name: "Go to in" }));
    expect(video.currentTime).toBeCloseTo(0.04);
    await userEvent.click(screen.getByRole("button", { name: "Next frame" }));
    expect(video.currentTime).toBeCloseTo(0.08);
    expect(screen.getByRole("slider", { name: "Trim start" })).toHaveAttribute(
      "aria-valuenow",
      "40000",
    );
    expect(screen.getByRole("slider", { name: "Trim end" })).toHaveAttribute(
      "aria-valuenow",
      "2000000",
    );
    expect(screen.getByLabelText("Playhead")).toHaveTextContent(
      "0:00.080 / 0:02.000",
    );
    await userEvent.click(screen.getByRole("button", { name: "Play" }));
    expect(video.play).toHaveBeenCalledTimes(1);
  });
  it("plays the selection and stops at its exact end", async () => {
    const video = await ready({ ...metadata(), durationMicros: 6_000_000 });
    await userEvent.click(
      screen.getByRole("button", { name: "Play selection" }),
    );
    expect(video.currentTime).toBe(0);
    expect(video.play).toHaveBeenCalledTimes(1);
    video.currentTime = 6;
    fireEvent.ended(video);
    expect(video.pause).toHaveBeenCalledTimes(1);
    expect(video.currentTime).toBe(6);
  });
  it("covers a selection shorter than two seconds", async () => {
    const video = await ready({ ...metadata(), durationMicros: 1_500_000 });
    await userEvent.click(
      screen.getByRole("button", { name: "Play selection" }),
    );
    expect(video.currentTime).toBe(0);
    video.currentTime = 1.6;
    fireEvent.timeUpdate(video);
    expect(video.pause).toHaveBeenCalledTimes(1);
    expect(video.currentTime).toBe(1.5);
  });
  it("discards an active selection preview for Go to in and unrelated seeks", async () => {
    const video = await ready({ ...metadata(), durationMicros: 6_000_000 });
    await userEvent.click(
      screen.getByRole("button", { name: "Play selection" }),
    );
    await userEvent.click(screen.getByRole("button", { name: "Go to in" }));
    video.currentTime = 6.1;
    fireEvent.timeUpdate(video);
    expect(video.pause).not.toHaveBeenCalled();

    await userEvent.click(
      screen.getByRole("button", { name: "Play selection" }),
    );
    video.currentTime = 1;
    fireEvent.seeking(video);
    video.currentTime = 6.1;
    fireEvent.timeUpdate(video);
    expect(video.pause).not.toHaveBeenCalled();
  });
  it("keeps the selection stop after dragging to a rounded preview start", async () => {
    const video = await ready({ ...metadata(), durationMicros: 6_000_000 });
    fireEvent.keyDown(screen.getByRole("slider", { name: "Trim start" }), {
      key: "ArrowRight",
    });
    await userEvent.click(
      screen.getByRole("button", { name: "Play selection" }),
    );
    video.currentTime = 0.0405;
    fireEvent.seeking(video);
    fireEvent.seeking(video);
    video.currentTime = 6.1;
    fireEvent.timeUpdate(video);
    expect(video.pause).toHaveBeenCalledTimes(1);
    expect(video.currentTime).toBe(6);
  });
  it("disables transport until playable and while exporting", async () => {
    await ready(metadata(), false);
    const names = [
      "Go to in",
      "Previous frame",
      "Play",
      "Next frame",
      "Go to out",
      "Play selection",
    ];
    for (const name of names)
      expect(screen.getByRole("button", { name })).toBeDisabled();
    fireEvent.loadedMetadata(document.querySelector("video")!);
    for (const name of names)
      expect(screen.getByRole("button", { name })).toBeEnabled();
    h.exportVideo.mockReturnValue(new Promise(() => {}));
    await userEvent.click(trimButton());
    await screen.findByRole("alertdialog", { name: "Trimming…" });
    for (const name of names)
      expect(screen.getByRole("button", { name, hidden: true })).toBeDisabled();
  });
  it("lists the keyboard shortcuts", async () => {
    await ready();
    const hints = screen.getByRole("list", { name: "Keyboard shortcuts" });
    for (const key of ["Space", "←", "→", "Shift", "I", "O", "Enter"])
      expect(within(hints).getByText(key)).toBeInTheDocument();
  });
  it("supports keyboard seeking, range changes, opening, and immediate cancellation", async () => {
    const video = await ready();
    video.currentTime = 1;
    fireEvent.timeUpdate(video);
    fireEvent.keyDown(window, { key: "i" });
    expect(screen.getByRole("slider", { name: "Trim start" })).toHaveAttribute(
      "aria-valuenow",
      "1000000",
    );
    fireEvent.keyDown(window, { key: "ArrowRight" });
    expect(video.currentTime).toBeCloseTo(1.04);
    fireEvent.keyDown(window, { key: "ArrowLeft", shiftKey: true });
    expect(video.currentTime).toBeCloseTo(0.04);
    fireEvent.keyDown(window, { key: " " });
    expect(video.play).toHaveBeenCalledTimes(1);
    fireEvent.keyDown(window, { key: "o", ctrlKey: true });
    await waitFor(() => expect(h.open).toHaveBeenCalledTimes(2));
    fireEvent.keyDown(window, { key: "Escape" });
    expect(h.exit).toHaveBeenCalledWith(130);
  });
  it("seeks to the active boundary for pointer and keyboard adjustments", async () => {
    const video = await ready();
    const startHandle = screen.getByRole("slider", { name: "Trim start" });
    const track = startHandle.parentElement!;
    vi.spyOn(track, "getBoundingClientRect").mockReturnValue({
      left: 0,
      width: 100,
      right: 100,
      top: 0,
      bottom: 80,
      height: 80,
      x: 0,
      y: 0,
      toJSON: () => ({}),
    });
    fireEvent.pointerDown(startHandle, { clientX: 25, pointerId: 1 });
    expect(video.currentTime).toBeCloseTo(0.5);
    expect(startHandle).toHaveAttribute("aria-valuenow", "500000");
    const endHandle = screen.getByRole("slider", { name: "Trim end" });
    fireEvent.pointerDown(endHandle, { clientX: 75, pointerId: 2 });
    expect(video.currentTime).toBeCloseTo(1.5);
    expect(endHandle).toHaveAttribute("aria-valuenow", "1500000");
    fireEvent.keyDown(startHandle, { key: "End" });
    expect(video.currentTime).toBeCloseTo(1.46);
    fireEvent.keyDown(endHandle, { key: "Home" });
    expect(endHandle).toHaveAttribute("aria-valuenow", "1500000");
  });
});
/** Makes the element report loaded media and counts `currentTime` writes. */
function seekable(video: HTMLVideoElement) {
  const proto = Object.getOwnPropertyDescriptor(
    HTMLMediaElement.prototype,
    "currentTime",
  )!;
  const writes: number[] = [];
  Object.defineProperty(video, "readyState", {
    configurable: true,
    get: () => HTMLMediaElement.HAVE_ENOUGH_DATA,
  });
  Object.defineProperty(video, "currentTime", {
    configurable: true,
    get() {
      return proto.get!.call(this);
    },
    set(v: number) {
      writes.push(v);
      proto.set!.call(this, v);
    },
  });
  return writes;
}
function mockTrack(el: HTMLElement) {
  vi.spyOn(el, "getBoundingClientRect").mockReturnValue({
    left: 0,
    width: 100,
    right: 100,
    top: 0,
    bottom: 80,
    height: 80,
    x: 0,
    y: 0,
    toJSON: () => ({}),
  });
}
describe("seeking", () => {
  const timelineTrack = () => {
    const track = screen.getByRole("slider", {
      name: "Trim start",
    }).parentElement!;
    mockTrack(track);
    return track;
  };
  it("coalesces a timeline drag into at most two seeks ending on the pointer", async () => {
    const video = await ready();
    const writes = seekable(video);
    const track = timelineTrack();
    fireEvent.pointerDown(track, { clientX: 10, buttons: 1 });
    for (const x of [20, 30, 40])
      fireEvent.pointerMove(track, { clientX: x, buttons: 1 });
    expect(screen.getByLabelText("Playhead")).toHaveTextContent(
      "0:00.800 / 0:02.000",
    );
    expect(writes).toEqual([0.2]);
    fireEvent.seeking(video);
    fireEvent.seeked(video);
    expect(writes).toEqual([0.2, 0.8]);
    fireEvent.seeking(video);
    fireEvent.seeked(video);
    expect(video.currentTime).toBeCloseTo(0.8);
    expect(writes).toHaveLength(2);
  });
  it("seeks once for a single timeline click", async () => {
    const video = await ready();
    const writes = seekable(video);
    fireEvent.pointerDown(timelineTrack(), { clientX: 50, buttons: 1 });
    fireEvent.seeking(video);
    fireEvent.seeked(video);
    expect(writes).toEqual([1]);
  });
  it("ends a trim handle drag on the boundary's final frame", async () => {
    const video = await ready();
    const writes = seekable(video);
    const startHandle = screen.getByRole("slider", { name: "Trim start" });
    mockTrack(startHandle.parentElement!);
    fireEvent.pointerDown(startHandle, { clientX: 10, pointerId: 1 });
    for (const x of [20, 30])
      fireEvent.pointerMove(startHandle, {
        clientX: x,
        pointerId: 1,
        buttons: 1,
      });
    expect(startHandle).toHaveAttribute("aria-valuenow", "600000");
    fireEvent.seeking(video);
    fireEvent.seeked(video);
    expect(writes).toHaveLength(2);
    expect(video.currentTime).toBeCloseTo(0.6);
  });
});
describe("footer status", () => {
  it("describes re-encoding and copy from the preceding keyframe", async () => {
    await ready({
      ...metadata(),
      durationMicros: 6_000_000,
      keyframesMicros: [0, 2_190_000, 4_000_000],
    });
    const status = screen.getByRole("status");
    expect(status).toHaveTextContent("Frame-exact · re-encoded as MP4");
    await userEvent.clear(field("In"));
    await userEvent.type(field("In"), "2.4{Enter}");
    await userEvent.click(screen.getByRole("radio", { name: "Copy" }));
    expect(status).toHaveTextContent(
      "Fast copy without re-encoding · output starts at keyframe 2.190 s",
    );
  });
  it("stays open after a successful export with the stay policy", async () => {
    h.launch.mockResolvedValue({
      ...launch,
      format: "gif",
      quality: "small",
      onDone: "stay",
    });
    h.exportVideo.mockResolvedValue({
      output: "/videos/one_trim.gif",
      acceleration: [],
      effectiveStartMicros: 0,
    });
    await ready();
    await userEvent.click(trimButton());
    expect(await screen.findByRole("status")).toHaveTextContent(
      "Saved /videos/one_trim.gif",
    );
    expect(h.exportVideo).toHaveBeenCalledWith(
      expect.objectContaining({
        output: "/videos/one_trim.gif",
        format: "gif",
        quality: "small",
      }),
    );
    expect(h.exit).not.toHaveBeenCalled();
    expect(screen.queryByRole("alertdialog")).toBeNull();
    expect(screen.getByRole("heading", { name: "one.mp4" })).toBeVisible();
    expect(trimButton()).toBeEnabled();
    await userEvent.click(trimButton());
    await waitFor(() => expect(h.exportVideo).toHaveBeenCalledTimes(2));
    expect(h.exit).not.toHaveBeenCalled();
  });
  it("exits after a successful export with the exit policy", async () => {
    h.exportVideo.mockResolvedValueOnce({
      output: "/videos/one_trim.mp4",
      acceleration: [],
      effectiveStartMicros: 0,
    });
    await ready();
    fireEvent.keyDown(window, { key: "Enter" });
    await waitFor(() => expect(h.exit).toHaveBeenCalledWith(0));
    expect(h.exportVideo).toHaveBeenCalledWith({
      input: "/videos/one.mp4",
      output: "/videos/one_trim.mp4",
      startMicros: 0,
      endMicros: 2_000_000,
      format: "mp4",
      quality: "original",
    });
  });
});
describe("export dialog", () => {
  const progress = (payload: Partial<ExportProgress>) =>
    act(() =>
      h.events["export-progress"]({
        payload: {
          fraction: 0,
          outTimeMicros: 0,
          attempt: "Software decode + libx264",
          bytesWritten: 0,
          estimatedBytes: null,
          approximate: true,
          remainingMicros: null,
          step: "Decoding and encoding",
          ...payload,
        },
      }),
    );
  const steps = (dialog: HTMLElement) =>
    within(within(dialog).getByRole("list", { name: "Export steps" }))
      .getAllByRole("listitem")
      .map((li) => [li.textContent, li.dataset.state]);
  it("shows percent, time left, sizes, and steps mid-export", async () => {
    const dialog = await exporting();
    expect(within(dialog).getByText("one_trim.mp4 → /videos")).toBeVisible();
    expect(within(dialog).getByText("Preparing")).toBeVisible();
    progress({
      fraction: 0.03,
      outTimeMicros: 60_000,
      remainingMicros: 40_000_000,
      estimatedBytes: 142_000_000,
    });
    // Time remaining stays hidden below 5%.
    expect(within(dialog).queryByText(/left$/)).toBeNull();
    progress({
      fraction: 0.62,
      outTimeMicros: 1_240_000,
      attempt: "VA-API decode + encode",
      bytesWritten: 88_000_000,
      estimatedBytes: 142_000_000,
      remainingMicros: 6_200_000,
    });
    expect(within(dialog).getByText("62%")).toBeVisible();
    expect(
      within(dialog).getByRole("progressbar", { name: "Export progress" }),
    ).toHaveAttribute("aria-valuenow", "62");
    expect(within(dialog).getByText("about 6 s left")).toBeVisible();
    expect(within(dialog).getByText("0:01.240 / 0:02.000")).toBeVisible();
    expect(within(dialog).getByText("88 MB of ≈ 142 MB")).toBeVisible();
    expect(within(dialog).getByText("VA-API decode + encode")).toBeVisible();
    expect(steps(dialog)).toEqual([
      ["Decoding and encoding", "active"],
      ["Validating output", "pending"],
      ["Finalizing file", "pending"],
    ]);
    progress({
      fraction: 0.62,
      outTimeMicros: 1_240_000,
      bytesWritten: 140_500_000,
      estimatedBytes: 142_000_000,
      step: "Validating output",
    });
    expect(steps(dialog)).toEqual([
      ["Decoding and encoding", "done"],
      ["Validating output", "active"],
      ["Finalizing file", "pending"],
    ]);
  });
  it("shows an exact estimate and the keyframe seek for copy", async () => {
    h.exportVideo.mockReturnValue(new Promise(() => {}));
    await ready();
    await userEvent.click(screen.getByRole("radio", { name: "Copy" }));
    await userEvent.click(trimButton());
    const dialog = await screen.findByRole("alertdialog", {
      name: "Trimming…",
    });
    progress({
      attempt: "Stream copy",
      estimatedBytes: 4_200_000,
      approximate: false,
      step: "Seek to keyframe at 0:00.000",
    });
    expect(within(dialog).getByText("0 B of 4.2 MB")).toBeVisible();
    expect(steps(dialog)).toEqual([
      ["Seek to keyframe at 0:00.000", "active"],
      ["Copying streams", "pending"],
      ["Finalizing file", "pending"],
    ]);
    progress({
      attempt: "Stream copy",
      fraction: 0.5,
      outTimeMicros: 1_000_000,
      bytesWritten: 2_100_000,
      estimatedBytes: 4_200_000,
      approximate: false,
      step: "Copying streams",
    });
    expect(within(dialog).getByText("2.1 MB of 4.2 MB")).toBeVisible();
    expect(steps(dialog)).toEqual([
      ["Seek to keyframe at 0:00.000", "done"],
      ["Copying streams", "active"],
      ["Finalizing file", "pending"],
    ]);
  });
  it("routes Cancel trim to the confirmation and cancels the export", async () => {
    const dialog = await exporting();
    await userEvent.click(
      within(dialog).getByRole("button", { name: "Cancel trim" }),
    );
    expect(
      screen.getByRole("alertdialog", { name: "Cancel export?" }),
    ).toBeVisible();
    await userEvent.click(
      screen.getByRole("button", { name: "Cancel export" }),
    );
    expect(h.cancel).toHaveBeenCalled();
  });
  it("routes Escape to the confirmation", async () => {
    await exporting();
    await userEvent.keyboard("{Escape}");
    expect(
      await screen.findByRole("alertdialog", { name: "Cancel export?" }),
    ).toBeVisible();
    expect(h.exit).not.toHaveBeenCalled();
  });
});
describe("responsive sidebar", () => {
  it("toggles the floating settings panel from the header", async () => {
    render(<VideoTrimmer />);
    const toggle = screen.getByRole("button", { name: "Show settings" });
    const sidebar = screen.getByRole("complementary", {
      name: "Trim settings",
    });
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    expect(sidebar).toHaveClass("max-[959px]:hidden");
    await userEvent.click(toggle);
    expect(
      screen.getByRole("button", { name: "Hide settings" }),
    ).toHaveAttribute("aria-expanded", "true");
    expect(sidebar).not.toHaveClass("max-[959px]:hidden");
    expect(sidebar).toHaveClass("max-[959px]:absolute");
  });
});
