import type { ExportFormat } from "./types";
/** Copy keeps the source container; other formats have fixed extensions. */
export const extension = (format: ExportFormat, sourcePath?: string) =>
  format === "copy"
    ? (/\.([^./]+)$/.exec(sourcePath ?? "")?.[1]?.toLowerCase() ?? "mp4")
    : format;
/** Splits a POSIX path into its directory and file name. */
export function splitPath(path: string) {
  const i = path.lastIndexOf("/");
  if (i < 0) return { dir: "", name: path };
  return { dir: i === 0 ? "/" : path.slice(0, i), name: path.slice(i + 1) };
}
const stem = (name: string) => name.replace(/\.[^./]+$/, "");
/** Default destination: next to the source as `<stem>_trim`. */
export function defaultOutput(inputPath: string) {
  const { dir, name } = splitPath(inputPath);
  return { dir, stem: `${stem(name)}_trim` };
}
/** Splits an explicit `--output` path into directory and stem. */
export function splitOutput(outputPath: string) {
  const { dir, name } = splitPath(outputPath);
  return { dir, stem: stem(name) };
}
export function joinOutput(
  dir: string,
  fileStem: string,
  format: ExportFormat,
  sourcePath?: string,
) {
  const file = `${fileStem}.${extension(format, sourcePath)}`;
  if (!dir) return file;
  return dir.endsWith("/") ? `${dir}${file}` : `${dir}/${file}`;
}
/** Decimal size such as `88 MB` or `4.2 GB`; one decimal below 10 units. */
export function formatBytes(bytes: number) {
  const units = ["B", "KB", "MB", "GB", "TB"];
  let value = Math.max(0, bytes);
  let i = 0;
  while (value >= 1000 && i < units.length - 1) {
    value /= 1000;
    i++;
  }
  return `${i === 0 || value >= 10 ? Math.round(value) : value.toFixed(1)} ${units[i]}`;
}
