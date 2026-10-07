# wodeo

<img width="1231" height="833" alt="grim_Screenshot_2026-09-23_09-42-18" src="https://github.com/user-attachments/assets/cfec390d-1414-4dc3-9e63-e7d1b21c21b2" />

A minimal Wayland-only MP4 trimmer for Hyprland. Select an in/out range, preview it, and export a precise cut as MP4 (H.264, optional VA-API acceleration), WebM, GIF, or a fast stream copy.

## Features

- QuickTime-style timeline with mouse and keyboard controls
- Precise cuts through re-encoding instead of keyframe-only splitting
- Output formats: MP4, WebM (VP9/Opus), GIF, or stream copy
- Quality tiers that cap resolution and bitrate
- Hardware-accelerated MP4 export with an automatic software fallback
- Defaults stored in a config file, overridable per run
- File picker, drag and drop, and command-line input/output paths

## Install

Requires an x86_64 Linux system running native Wayland and [Nix](https://nixos.org/) with flakes enabled.

```sh
nix profile install github:wochap/wodeo
```

Run without installing:

```sh
nix run github:wochap/wodeo -- video.mp4
```

## Usage

```sh
wodeo [INPUT] [-o PATH] [--format FORMAT] [--quality QUALITY] [--on-done POLICY] [-v]
```

```sh
wodeo recording.mp4
wodeo recording.mp4 -o clip.mp4
wodeo recording.mp4 -o clip.gif --quality small
wodeo recording.mp4 --format copy --on-done stay
```

Options:

- `-o, --output PATH` — destination file. Its extension must match the format (`.mp4` for `mp4` and `copy`, `.webm`, `.gif`). Without `--format`, the extension picks the format. An existing file is replaced without asking; only the source file itself is refused. Without `--output`, the editor suggests `<source-stem>_trim.<ext>` next to the source.
- `--format mp4|webm|gif|copy` — `mp4` (default) re-encodes to H.264/AAC. `webm` re-encodes to VP9/Opus in software, which is slow on long selections. `gif` builds a palette GIF without audio. `copy` copies the streams without re-encoding: it is fast, but the output starts at the nearest keyframe at or before the selected start, and it may run a frame or two past the selected end. When `--format` disagrees with the `--output` extension, the flag wins and the extension is rewritten.
- `--quality original|high|small` — `original` (default) keeps the source resolution. `high` fits video inside 1920x1080 and `small` inside 1280x720 (portrait sources use the rotated box); sources are never upscaled. For GIF, `high` caps width at 720 and `small` at 480, at 12 and 10 fps (15 fps for `original`). `copy` ignores quality.
- `--on-done exit|stay` — `exit` (default) closes the app after a successful trim. `stay` returns to the editor so you can trim again; each successful trim prints its path on its own stdout line, and the app exits with status 0 when closed.
- `-v, --verbose` — detailed media logs.

`-f/--force` was removed: the destination is always shown before trimming, so existing files are replaced.

### Config file

Defaults are read from `$XDG_CONFIG_HOME/wodeo/config.toml` (or `~/.config/wodeo/config.toml`). Every key is optional:

```toml
# mp4 | webm | gif | copy
format = "mp4"
# original | high | small
quality = "original"
# exit | stay
on_done = "exit"
```

Precedence, later wins: built-in default, config file, `--output` extension (format only), command-line flag. An unknown key, invalid value, or malformed file stops startup with a message naming the file and key.

Useful controls:

- `Space` — play or pause
- `Left` / `Right` — seek one frame
- `Shift+Left` / `Shift+Right` — seek one second
- `I` / `O` — set the in/out point
- `Enter` — export
- `Escape` — cancel
- `Ctrl+O` — open another MP4

## Development

```sh
git clone https://github.com/wochap/wodeo.git
cd wodeo
nix develop
npm ci
npm run tauri -- dev -- [INPUT]
```

Run the main checks with:

```sh
npm run build
npm test
cargo test --manifest-path src-tauri/Cargo.toml
nix build
```

## Tech stack

- [Tauri 2](https://tauri.app/) and Rust
- React, TypeScript, Vite, and Tailwind CSS
- FFmpeg/ffprobe, GTK3, WebKitGTK, and GStreamer
- Nix for development and packaging

## Notes

Only local MP4 input files and one continuous time range are supported. Re-encoded exports may normalize unusual formats for compatibility. The app requires native Wayland and does not fall back to X11/XWayland.

On success, stdout contains only the absolute output path. Logs are written to stderr and `$XDG_STATE_HOME/wodeo` (usually `~/.local/state/wodeo`). See [docs/verification.md](docs/verification.md) for supported media scenarios.

## License

[MIT](LICENSE)
