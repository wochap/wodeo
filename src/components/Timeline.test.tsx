import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { rulerTicks, ThumbnailStrip, Timeline } from "./Timeline";
describe("Timeline", () => {
  it("exposes independent labelled sliders", () => {
    render(
      <Timeline
        duration={1_000_000}
        start={100_000}
        end={900_000}
        playhead={500_000}
        step={40_000}
        keyframes={[]}
        thumbnails={[]}
        onSeek={() => {}}
        onRange={() => {}}
      />,
    );
    expect(screen.getByRole("slider", { name: "Trim start" })).toHaveAttribute(
      "aria-valuemax",
      "860000",
    );
    expect(screen.getByRole("slider", { name: "Trim end" })).toHaveAttribute(
      "aria-valuemin",
      "140000",
    );
  });
  it("keyboard-adjusts only the focused boundary", () => {
    const range = vi.fn();
    render(
      <Timeline
        duration={1_000_000}
        start={100_000}
        end={900_000}
        playhead={500_000}
        step={40_000}
        keyframes={[]}
        thumbnails={[]}
        onSeek={() => {}}
        onRange={range}
      />,
    );
    fireEvent.keyDown(screen.getByRole("slider", { name: "Trim start" }), {
      key: "ArrowRight",
    });
    expect(range).toHaveBeenCalledWith(140_000, 900_000, "start");
  });
});
describe("complete timeline interaction", () => {
  it("seeks with the track without changing a boundary", () => {
    const seek = vi.fn(),
      range = vi.fn();
    render(
      <Timeline
        duration={1_000_000}
        start={100_000}
        end={900_000}
        playhead={500_000}
        step={40_000}
        keyframes={[]}
        thumbnails={[]}
        onSeek={seek}
        onRange={range}
      />,
    );
    const track = screen.getByRole("slider", {
      name: "Trim start",
    }).parentElement!;
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
    fireEvent.pointerDown(track, { clientX: 25 });
    expect(seek).toHaveBeenCalledWith(250_000);
    expect(range).not.toHaveBeenCalled();
  });
  it("supports Home and End without crossing handles", () => {
    const range = vi.fn();
    render(
      <Timeline
        duration={1_000_000}
        start={100_000}
        end={900_000}
        playhead={500_000}
        step={40_000}
        keyframes={[]}
        thumbnails={[]}
        onSeek={() => {}}
        onRange={range}
      />,
    );
    fireEvent.keyDown(screen.getByRole("slider", { name: "Trim start" }), {
      key: "End",
    });
    expect(range).toHaveBeenCalledWith(860_000, 900_000, "start");
    fireEvent.keyDown(screen.getByRole("slider", { name: "Trim end" }), {
      key: "Home",
    });
    expect(range).toHaveBeenCalledWith(100_000, 140_000, "end");
  });

  it("reports the active boundary for pointer adjustments", () => {
    const range = vi.fn();
    render(
      <Timeline
        duration={1_000_000}
        start={100_000}
        end={900_000}
        playhead={500_000}
        step={40_000}
        keyframes={[]}
        thumbnails={[]}
        onSeek={() => {}}
        onRange={range}
      />,
    );
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
    expect(range).toHaveBeenLastCalledWith(250_000, 900_000, "start");
    fireEvent.pointerDown(screen.getByRole("slider", { name: "Trim end" }), {
      clientX: 75,
      pointerId: 2,
    });
    expect(range).toHaveBeenLastCalledWith(100_000, 750_000, "end");
  });
});
describe("track scrubbing", () => {
  const setup = (thumbnails: (string | null)[] = []) => {
    const seek = vi.fn();
    render(
      <Timeline
        duration={1_000_000}
        start={100_000}
        end={900_000}
        playhead={500_000}
        step={40_000}
        keyframes={[]}
        thumbnails={thumbnails}
        onSeek={seek}
        onRange={() => {}}
      />,
    );
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
    return { seek, track, startHandle };
  };
  it("captures the pointer and keeps scrubbing over handles until release", () => {
    const { seek, track, startHandle } = setup();
    const capture = vi.spyOn(track, "setPointerCapture");
    fireEvent.pointerDown(track, { clientX: 50, pointerId: 1 });
    expect(capture).toHaveBeenCalledWith(1);
    expect(seek).toHaveBeenLastCalledWith(500_000);
    fireEvent.pointerMove(startHandle, { clientX: 10, buttons: 1 });
    expect(seek).toHaveBeenLastCalledWith(100_000);
    fireEvent.pointerUp(track);
    seek.mockClear();
    fireEvent.pointerMove(track, { clientX: 30, buttons: 1 });
    expect(seek).not.toHaveBeenCalled();
  });
  it("clamps moves beyond the track", () => {
    const { seek, track } = setup();
    fireEvent.pointerDown(track, { clientX: 50, pointerId: 1 });
    fireEvent.pointerMove(track, { clientX: -40, buttons: 1 });
    expect(seek).toHaveBeenLastCalledWith(0);
    fireEvent.pointerMove(track, { clientX: 240, buttons: 1 });
    expect(seek).toHaveBeenLastCalledWith(1_000_000);
  });
  it("does not seek from a handle press and keeps thumbnails undraggable", () => {
    const { seek, startHandle } = setup(["asset://a.jpg", null]);
    fireEvent.pointerDown(startHandle, { clientX: 20, pointerId: 1 });
    expect(seek).not.toHaveBeenCalled();
    for (const img of document.querySelectorAll("img"))
      expect(img).toHaveAttribute("draggable", "false");
    render(<ThumbnailStrip thumbnails={["asset://b.jpg"]} />);
    for (const img of document.querySelectorAll("img"))
      expect(img).toHaveAttribute("draggable", "false");
  });
});
describe("adaptive ruler", () => {
  const labels = (duration: number) => {
    render(
      <Timeline
        duration={duration}
        start={0}
        end={duration}
        playhead={0}
        step={40_000}
        keyframes={[]}
        thumbnails={[]}
        onSeek={() => {}}
        onRange={() => {}}
      />,
    );
    return Array.from(
      screen.getByTestId("timeline-ruler").querySelectorAll("span.font-mono"),
      (e) => e.textContent,
    );
  };
  it("labels every second of a 10 s clip with quarter-second minors", () => {
    expect(labels(10_000_000)).toEqual(
      Array.from({ length: 11 }, (_, i) => `0:${String(i).padStart(2, "0")}`),
    );
    expect(rulerTicks(10_000_000).minors).toHaveLength(30);
  });
  it("labels every 30 s of a 4 min clip with 10 s minors", () => {
    expect(labels(240_000_000)).toEqual([
      "0:00",
      "0:30",
      "1:00",
      "1:30",
      "2:00",
      "2:30",
      "3:00",
      "3:30",
      "4:00",
    ]);
    expect(rulerTicks(240_000_000).minors).toHaveLength(16);
  });
  it("fills the inspecting strip left to right with placeholders", () => {
    const thumbnails = Array.from({ length: 14 }, (_, i) =>
      i < 5 ? `asset://frame-${i}.jpg` : null,
    );
    render(<ThumbnailStrip thumbnails={thumbnails} />);
    const strip = screen.getByTestId("timeline-placeholder");
    const cells = Array.from(strip.children);
    expect(cells).toHaveLength(14);
    expect(strip.querySelectorAll("img")).toHaveLength(5);
    expect(screen.getAllByTestId("thumbnail-placeholder")).toHaveLength(9);
    expect(cells.slice(0, 5).every((c) => c.tagName === "IMG")).toBe(true);
  });
});
describe("boundary navigation keys", () => {
  const setup = (keyframes: number[] = [], start = 100_000, end = 900_000) => {
    const range = vi.fn();
    render(
      <Timeline
        duration={1_000_000}
        start={start}
        end={end}
        playhead={500_000}
        step={40_000}
        keyframes={keyframes}
        thumbnails={[]}
        onSeek={() => {}}
        onRange={range}
      />,
    );
    return {
      range,
      handle: (which: "start" | "end") =>
        screen.getByRole("slider", {
          name: which === "start" ? "Trim start" : "Trim end",
        }),
    };
  };
  it("moves the start handle with each key", () => {
    const { range, handle } = setup([0, 50_000, 300_000], 200_000);
    const cases: [object, number][] = [
      [{ key: "ArrowLeft" }, 160_000],
      [{ key: "ArrowRight", shiftKey: true }, 860_000],
      [{ key: "ArrowLeft", shiftKey: true }, 0],
      [{ key: "PageUp" }, 860_000],
      [{ key: "PageDown" }, 0],
      [{ key: "ArrowLeft", altKey: true }, 50_000],
      [{ key: "ArrowRight", altKey: true }, 300_000],
      [{ key: "3" }, 300_000],
    ];
    for (const [init, want] of cases) {
      range.mockClear();
      fireEvent.keyDown(handle("start"), init);
      expect(range).toHaveBeenCalledWith(want, 900_000, "start");
    }
  });
  it("moves the end handle with each key", () => {
    const { range, handle } = setup([0, 700_000, 950_000], 100_000, 800_000);
    const cases: [object, number][] = [
      [{ key: "ArrowRight" }, 840_000],
      [{ key: "ArrowRight", shiftKey: true }, 1_000_000],
      [{ key: "PageDown" }, 140_000],
      [{ key: "ArrowLeft", altKey: true }, 700_000],
      [{ key: "ArrowRight", altKey: true }, 950_000],
      [{ key: "5" }, 500_000],
    ];
    for (const [init, want] of cases) {
      range.mockClear();
      fireEvent.keyDown(handle("end"), init);
      expect(range).toHaveBeenCalledWith(100_000, want, "end");
    }
  });
  it("clamps digit jumps against the other handle", () => {
    const { range, handle } = setup([], 100_000, 500_000);
    fireEvent.keyDown(handle("start"), { key: "9" });
    expect(range).toHaveBeenLastCalledWith(460_000, 500_000, "start");
    fireEvent.keyDown(handle("end"), { key: "0" });
    expect(range).toHaveBeenLastCalledWith(100_000, 140_000, "end");
  });
  it("ignores Alt+arrows without a keyframe but still prevents default", () => {
    const { range, handle } = setup([]);
    const ev = new KeyboardEvent("keydown", {
      key: "ArrowLeft",
      altKey: true,
      bubbles: true,
      cancelable: true,
    });
    handle("start").dispatchEvent(ev);
    expect(ev.defaultPrevented).toBe(true);
    expect(range).not.toHaveBeenCalled();
  });
  it("keeps Home and End", () => {
    const { range, handle } = setup();
    fireEvent.keyDown(handle("start"), { key: "Home" });
    expect(range).toHaveBeenLastCalledWith(0, 900_000, "start");
    fireEvent.keyDown(handle("start"), { key: "End" });
    expect(range).toHaveBeenLastCalledWith(860_000, 900_000, "start");
    fireEvent.keyDown(handle("end"), { key: "Home" });
    expect(range).toHaveBeenLastCalledWith(100_000, 140_000, "end");
    fireEvent.keyDown(handle("end"), { key: "End" });
    expect(range).toHaveBeenLastCalledWith(100_000, 1_000_000, "end");
  });
});
