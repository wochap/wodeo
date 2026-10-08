import { describe, expect, it } from "vitest";
import { defaultOutput, joinOutput, splitOutput } from "./output";
describe("output naming", () => {
  it("defaults to <stem>_trim beside the source", () =>
    expect(defaultOutput("/home/me/Videos/talk.v2.mp4")).toEqual({
      dir: "/home/me/Videos",
      stem: "talk.v2_trim",
    }));
  it("splits an explicit --output path", () =>
    expect(splitOutput("/tmp/out/clip.mp4")).toEqual({
      dir: "/tmp/out",
      stem: "clip",
    }));
  it("derives the extension from the format", () => {
    expect(joinOutput("/tmp/out", "clip", "webm")).toBe("/tmp/out/clip.webm");
    expect(joinOutput("/tmp/out", "clip", "gif")).toBe("/tmp/out/clip.gif");
    expect(joinOutput("/", "clip", "copy")).toBe("/clip.mp4");
  });
  it("keeps the source extension for copy", () => {
    expect(joinOutput("/tmp", "clip", "copy", "/v/in.MKV")).toBe("/tmp/clip.mkv");
    expect(joinOutput("/tmp", "clip", "copy", "/v/in.webm")).toBe("/tmp/clip.webm");
    expect(joinOutput("/tmp", "clip", "mp4", "/v/in.mkv")).toBe("/tmp/clip.mp4");
  });
});
