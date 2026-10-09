import { getCurrentWindow, type Theme } from "@tauri-apps/api/window";

function apply(theme: Theme | null) {
  if (theme) document.documentElement.dataset.theme = theme;
}

/** Mirrors the window's system theme onto `<html data-theme>`; until it is
 * known (or when it is null) `prefers-color-scheme` decides in CSS. */
export async function followSystemTheme(): Promise<() => void> {
  const win = getCurrentWindow();
  const unlisten = await win.onThemeChanged(({ payload }) => apply(payload));
  apply(await win.theme());
  return unlisten;
}
