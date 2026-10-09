import { render } from "@testing-library/react";
import { expect, it, vi } from "vitest";
const show = vi.fn().mockResolvedValue(undefined);
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    onDragDropEvent: vi.fn().mockResolvedValue(() => {}),
    theme: vi.fn().mockResolvedValue("dark"),
    onThemeChanged: vi.fn().mockResolvedValue(() => {}),
    show,
  }),
}));
vi.mock("./VideoTrimmer", () => ({ default: () => null }));
import App from "./App";

it("shows the window once after mounting", () => {
  render(<App />);
  expect(show).toHaveBeenCalledTimes(1);
});
