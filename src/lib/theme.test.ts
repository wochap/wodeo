import { afterEach, describe, expect, it, vi } from "vitest";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { followSystemTheme } from "./theme";

vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: vi.fn() }));

type Handler = (event: { payload: "dark" | "light" }) => void;

function mockWindow(theme: "dark" | "light" | null) {
  const h: { changed?: Handler } = {};
  vi.mocked(getCurrentWindow).mockReturnValue({
    theme: vi.fn().mockResolvedValue(theme),
    onThemeChanged: vi.fn(async (cb: Handler) => {
      h.changed = cb;
      return () => {};
    }),
  } as unknown as ReturnType<typeof getCurrentWindow>);
  return h;
}

describe("followSystemTheme", () => {
  afterEach(() => {
    delete document.documentElement.dataset.theme;
  });

  it("applies the resolved window theme", async () => {
    mockWindow("light");
    await followSystemTheme();
    expect(document.documentElement.dataset.theme).toBe("light");
  });

  it("applies a later theme change", async () => {
    const h = mockWindow("dark");
    await followSystemTheme();
    h.changed?.({ payload: "light" });
    expect(document.documentElement.dataset.theme).toBe("light");
  });

  it("leaves data-theme unset when the theme is unknown", async () => {
    mockWindow(null);
    await followSystemTheme();
    expect(document.documentElement.dataset.theme).toBeUndefined();
  });
});
