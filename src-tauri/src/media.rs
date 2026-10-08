use crate::{
    acceleration::{unknown_playback, AccelerationRecord},
    error::AppError,
};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    path::{Path, PathBuf},
    process::Command,
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
};
use tauri::{Emitter, Manager};
#[derive(Default)]
pub struct MediaState {
    pub current: Mutex<Option<CurrentMedia>>,
    pub latest_load: AtomicU64,
}
pub struct CurrentMedia {
    pub preview: PathBuf,
    pub cache: PathBuf,
    pub thumbnails: Vec<PathBuf>,
}
fn replace_current(current: &mut Option<CurrentMedia>, next: CurrentMedia) {
    if let Some(old) = current.take() {
        let _ = fs::remove_dir_all(old.cache);
    }
    *current = Some(next);
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoMetadata {
    pub path: String,
    pub preview_url: String,
    pub duration_micros: u64,
    pub width: u32,
    pub height: u32,
    pub codec: String,
    pub frame_rate: f64,
    pub has_audio: bool,
    pub audio_codec: Option<String>,
    pub thumbnails: Vec<String>,
    pub thumbnail_warning: Option<String>,
    pub playback_acceleration: Vec<AccelerationRecord>,
    pub keyframes_micros: Vec<u64>,
    /// Container bitrate in bits per second, used for export size estimates.
    #[serde(skip)]
    pub bit_rate: Option<u64>,
}
#[derive(Deserialize)]
struct Probe {
    streams: Vec<Stream>,
    format: Format,
}
#[derive(Deserialize)]
struct Stream {
    codec_type: Option<String>,
    codec_name: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    avg_frame_rate: Option<String>,
    r_frame_rate: Option<String>,
    duration: Option<String>,
}
#[derive(Deserialize)]
struct Format {
    duration: Option<String>,
    format_name: Option<String>,
    bit_rate: Option<String>,
}
pub const INPUT_EXTENSIONS: &[&str] = &["mp4", "m4v", "mov", "mkv", "webm"];
pub fn validate_input(raw: &Path) -> Result<PathBuf, AppError> {
    if raw
        .extension()
        .and_then(|v| v.to_str())
        .is_none_or(|v| !INPUT_EXTENSIONS.iter().any(|e| v.eq_ignore_ascii_case(e)))
    {
        return Err(AppError::UnsupportedInput(raw.display().to_string()));
    }
    if !raw.exists() {
        return Err(AppError::MissingInput(raw.display().to_string()));
    }
    let canonical = raw
        .canonicalize()
        .map_err(|_| AppError::UnreadableInput(raw.display().to_string()))?;
    if !canonical.is_file() || File::open(&canonical).is_err() {
        return Err(AppError::UnreadableInput(canonical.display().to_string()));
    }
    Ok(canonical)
}
fn parse_rate(s: Option<&str>) -> f64 {
    let Some(s) = s else { return 30.0 };
    let mut p = s.split('/');
    let a = p.next().and_then(|x| x.parse::<f64>().ok()).unwrap_or(30.0);
    let b = p.next().and_then(|x| x.parse::<f64>().ok()).unwrap_or(1.0);
    if b > 0.0 && a > 0.0 {
        a / b
    } else {
        30.0
    }
}
#[cfg(test)]
pub fn probe(path: &Path) -> Result<VideoMetadata, AppError> {
    let mut metadata = inspect(path, Some(path))?;
    metadata.keyframes_micros = keyframes(path).unwrap_or_else(|e| {
        tracing::warn!(path = %path.display(), reason = %e, "keyframe index unavailable");
        vec![]
    });
    Ok(metadata)
}
// Validates any container FFmpeg can read, for non-MP4 export outputs.
pub fn probe_any(path: &Path) -> Result<VideoMetadata, AppError> {
    inspect(path, None)
}
// Reads packet flags instead of decoding frames, so indexing stays cheap on
// long files. Timestamps come back ascending and deduplicated.
pub fn keyframes(path: &Path) -> Result<Vec<u64>, String> {
    let out = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "packet=pts_time,flags",
            "-of",
            "csv=p=0",
        ])
        .arg(path)
        .output()
        .map_err(|e| format!("could not start ffprobe: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_owned());
    }
    let mut times = String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|line| {
            let (pts, flags) = line.split_once(',')?;
            if !flags.starts_with('K') {
                return None;
            }
            let seconds = pts.parse::<f64>().ok()?;
            Some((seconds.max(0.0) * 1_000_000.0).round() as u64)
        })
        .collect::<Vec<_>>();
    times.sort_unstable();
    times.dedup();
    Ok(times)
}
// Latest keyframe at or before `start` (the first keyframe when `start`
// precedes it); `None` when the index is empty.
pub fn keyframe_at_or_before(keyframes: &[u64], start: u64) -> Option<u64> {
    match keyframes.partition_point(|k| *k <= start) {
        0 => keyframes.first().copied(),
        index => Some(keyframes[index - 1]),
    }
}
// ffprobe family name required for a supported input extension: every
// ISO-BMFF file reports `mov,...`, every Matroska/WebM file `matroska,webm`.
fn container_family(path: &Path) -> Option<&'static str> {
    match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "mp4" | "m4v" | "mov" => Some("mov"),
        "mkv" | "webm" => Some("matroska"),
        _ => None,
    }
}
// `require_supported` names the input whose extension must agree with the
// probed container family; `None` accepts any container with video.
fn inspect(path: &Path, require_supported: Option<&Path>) -> Result<VideoMetadata, AppError> {
    let out = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_streams",
            "-show_format",
            "-of",
            "json",
        ])
        .arg(path)
        .output()
        .map_err(|e| AppError::Probe(format!("could not start ffprobe: {e}")))?;
    if !out.status.success() {
        return Err(AppError::Probe(
            String::from_utf8_lossy(&out.stderr).trim().to_owned(),
        ));
    }
    let p: Probe = serde_json::from_slice(&out.stdout)
        .map_err(|e| AppError::Probe(format!("invalid ffprobe JSON: {e}")))?;
    if let Some(input) = require_supported {
        let family = container_family(input);
        if !family.is_some_and(|family| {
            p.format
                .format_name
                .as_deref()
                .unwrap_or("")
                .split(',')
                .any(|v| v == family)
        }) {
            return Err(AppError::Probe(
                "container is not a supported video container".into(),
            ));
        }
    }
    let v = p
        .streams
        .iter()
        .find(|s| s.codec_type.as_deref() == Some("video"))
        .ok_or(AppError::NoVideo)?;
    let duration = v
        .duration
        .as_ref()
        .or(p.format.duration.as_ref())
        .and_then(|x| x.parse::<f64>().ok())
        .filter(|x| *x > 0.0)
        .ok_or_else(|| AppError::Probe("missing positive duration".into()))?;
    let audio = p
        .streams
        .iter()
        .find(|s| s.codec_type.as_deref() == Some("audio"));
    let rate = parse_rate(v.avg_frame_rate.as_deref().or(v.r_frame_rate.as_deref()));
    Ok(VideoMetadata {
        path: path.to_string_lossy().into_owned(),
        preview_url: String::new(),
        duration_micros: (duration * 1_000_000.0).round() as u64,
        width: v.width.unwrap_or(0),
        height: v.height.unwrap_or(0),
        codec: v.codec_name.clone().unwrap_or_else(|| "unknown".into()),
        frame_rate: rate,
        has_audio: audio.is_some(),
        audio_codec: audio.and_then(|s| s.codec_name.clone()),
        thumbnails: vec![],
        thumbnail_warning: None,
        playback_acceleration: unknown_playback(),
        keyframes_micros: vec![],
        bit_rate: p.format.bit_rate.as_deref().and_then(|b| b.parse().ok()),
    })
}
pub const THUMBNAIL_COUNT: u64 = 14;
fn thumbnail_count(duration: u64) -> u64 {
    THUMBNAIL_COUNT.min((duration / 1_000_000).max(1))
}
// One FFmpeg spawn per timestamp so each frame can be reported as soon as it
// exists; `-ss` before `-i` keeps every seek cheap on long files.
fn thumbnails_per_file(
    path: &Path,
    duration: u64,
    dir: &Path,
    on_file: &mut dyn FnMut(usize, usize, &Path),
) -> Result<Vec<String>, String> {
    let count = thumbnail_count(duration) as usize;
    let interval = duration as f64 / 1_000_000.0 / count as f64;
    let mut files = Vec::with_capacity(count);
    for index in 0..count {
        let file = dir.join(format!("frame-{:02}.jpg", index + 1));
        let status = Command::new("ffmpeg")
            .args(["-hide_banner", "-nostdin", "-loglevel", "error", "-ss"])
            .arg(format!("{:.6}", interval * (index as f64 + 0.5)))
            .arg("-i")
            .arg(path)
            .args(["-frames:v", "1", "-vf", "scale=240:-2", "-q:v", "4", "-y"])
            .arg(&file)
            .status()
            .map_err(|e| e.to_string())?;
        if !status.success() || !file.is_file() {
            return Err(format!("FFmpeg could not write thumbnail {}", index + 1));
        }
        on_file(index, count, &file);
        files.push(file.to_string_lossy().into_owned());
    }
    Ok(files)
}
// Single-invocation fallback used when per-file generation fails.
fn thumbnails_batch(path: &Path, duration: u64, dir: &Path) -> Result<Vec<String>, String> {
    let count = thumbnail_count(duration);
    let interval = duration as f64 / 1_000_000.0 / count as f64;
    let pattern = dir.join("frame-%02d.jpg");
    let status = Command::new("ffmpeg")
        .args(["-hide_banner", "-nostdin", "-loglevel", "error", "-i"])
        .arg(path)
        .args([
            "-vf",
            &format!("fps=1/{interval:.6},scale=240:-2"),
            "-frames:v",
            &count.to_string(),
            "-q:v",
            "4",
            "-y",
        ])
        .arg(&pattern)
        .status()
        .map_err(|e| e.to_string())?;
    if !status.success() {
        return Err("FFmpeg could not generate timeline thumbnails".into());
    }
    let mut files = fs::read_dir(dir)
        .map_err(|e| e.to_string())?
        .flatten()
        .map(|e| e.path().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    files.sort();
    Ok(files)
}
fn thumbnails(
    path: &Path,
    duration: u64,
    dir: &Path,
    on_file: &mut dyn FnMut(usize, usize, &Path),
) -> Result<Vec<String>, String> {
    if dir.exists() {
        fs::remove_dir_all(dir).map_err(|e| e.to_string())?
    }
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    match thumbnails_per_file(path, duration, dir, on_file) {
        Ok(files) => Ok(files),
        Err(e) => {
            tracing::warn!(reason = %e, "per-file thumbnails failed; using the batch path");
            let _ = fs::remove_dir_all(dir);
            fs::create_dir_all(dir).map_err(|e| e.to_string())?;
            let files = thumbnails_batch(path, duration, dir).inspect_err(|_| {
                let _ = fs::remove_dir_all(dir);
            })?;
            for (index, file) in files.iter().enumerate() {
                on_file(index, files.len(), Path::new(file));
            }
            Ok(files)
        }
    }
}
// Source MP4s often carry sparse keyframes, invalid H.264 levels, or VUI
// timing that GStreamer's h264parse rejects, all of which make WebKitGTK
// drop the frames at a seek target and flash black. Re-encoding a proxy
// with dense keyframes, a leading moov atom, and fresh timing metadata
// keeps trim-point timestamps intact while making seeks land instantly.
// `-t` pins the proxy to the probed duration so the player's clock agrees
// with the timeline even when the source container duration is wrong.
fn preview_proxy(input: &Path, duration_micros: u64, dir: &Path) -> Result<PathBuf, String> {
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let proxy = dir.join("proxy.mp4");
    let status = Command::new("ffmpeg")
        .args(["-hide_banner", "-nostdin", "-loglevel", "error"])
        .arg("-t")
        .arg(format!("{:.6}", duration_micros as f64 / 1e6))
        .arg("-i")
        .arg(input)
        .args([
            "-map",
            "0:v:0",
            "-map",
            "0:a?",
            "-vf",
            "scale=trunc(iw/2)*2:trunc(ih/2)*2",
            "-c:v",
            "libx264",
            "-preset",
            "veryfast",
            "-crf",
            "28",
            "-g",
            "30",
            "-keyint_min",
            "30",
            "-sc_threshold",
            "0",
            "-fps_mode",
            "vfr",
            "-c:a",
            "copy",
            "-movflags",
            "+faststart",
            "-y",
        ])
        .arg(&proxy)
        .status()
        .map_err(|e| e.to_string())?;
    if !status.success() {
        let _ = fs::remove_file(&proxy);
        return Err("FFmpeg could not build the preview proxy".into());
    }
    Ok(proxy)
}
pub const STEP_CONTAINER: &str = "Reading container";
pub const STEP_KEYFRAMES: &str = "Indexing keyframes";
pub const STEP_PREVIEW: &str = "Building preview";
pub const STEP_THUMBNAILS: &str = "Building thumbnails";
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InspectProgress {
    pub load_id: u64,
    pub step: &'static str,
    pub fraction: f64,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InspectThumbnail {
    pub load_id: u64,
    pub index: usize,
    pub count: usize,
    pub path: String,
}
#[derive(Debug, Clone, PartialEq)]
pub enum InspectEvent {
    Progress(InspectProgress),
    Thumbnail(InspectThumbnail),
}
pub struct Inspected {
    pub metadata: VideoMetadata,
    pub preview: PathBuf,
    pub cache: PathBuf,
    pub thumbnails: Vec<PathBuf>,
}
fn cache_dir(load_id: u64) -> PathBuf {
    ProjectDirs::from("com", "wochap", "wodeo")
        .map(|p| p.cache_dir().to_path_buf())
        .unwrap_or_else(std::env::temp_dir)
        .join(format!("preview-{}-{load_id}", std::process::id()))
}
// Runs the inspection steps in the order the editor lists them. Step weights
// are probe 0.1, keyframes 0.2, preview 0.5, thumbnails 0.2; the thumbnail
// step advances per written file. Only the probe is fatal.
pub fn inspect_input(
    canonical: &Path,
    load_id: u64,
    cache: &Path,
    emit: &mut dyn FnMut(InspectEvent),
) -> Result<Inspected, AppError> {
    let mut progress = |step, fraction| {
        emit(InspectEvent::Progress(InspectProgress {
            load_id,
            step,
            fraction,
        }))
    };
    progress(STEP_CONTAINER, 0.0);
    let mut metadata = inspect(canonical, Some(canonical))?;
    progress(STEP_KEYFRAMES, 0.1);
    metadata.keyframes_micros = keyframes(canonical).unwrap_or_else(|e| {
        tracing::warn!(path = %canonical.display(), reason = %e, "keyframe index unavailable");
        vec![]
    });
    progress(STEP_PREVIEW, 0.3);
    let _ = fs::remove_dir_all(cache);
    let preview = preview_proxy(canonical, metadata.duration_micros, cache).unwrap_or_else(|e| {
        tracing::warn!(reason = %e, "preview proxy unavailable; serving the original file");
        canonical.to_path_buf()
    });
    progress(STEP_THUMBNAILS, 0.8);
    let generated = thumbnails(
        canonical,
        metadata.duration_micros,
        &cache.join("thumbs"),
        &mut |index, count, file| {
            emit(InspectEvent::Thumbnail(InspectThumbnail {
                load_id,
                index,
                count,
                path: file.to_string_lossy().into_owned(),
            }));
            emit(InspectEvent::Progress(InspectProgress {
                load_id,
                step: STEP_THUMBNAILS,
                fraction: 0.8 + 0.2 * (index + 1) as f64 / count as f64,
            }));
        },
    );
    metadata.thumbnails = generated.unwrap_or_else(|e| {
        metadata.thumbnail_warning = Some(e);
        vec![]
    });
    Ok(Inspected {
        thumbnails: metadata.thumbnails.iter().map(PathBuf::from).collect(),
        metadata,
        preview,
        cache: cache.to_path_buf(),
    })
}
#[tauri::command]
pub async fn load_input(
    app: tauri::AppHandle,
    state: tauri::State<'_, MediaState>,
    preview: tauri::State<'_, crate::preview_server::PreviewServer>,
    path: String,
    load_id: u64,
) -> Result<VideoMetadata, AppError> {
    let canonical = validate_input(Path::new(&path))?;
    state.latest_load.fetch_max(load_id, Ordering::SeqCst);
    let cache = cache_dir(load_id);
    let inspected = inspect_input(&canonical, load_id, &cache, &mut |event| match event {
        InspectEvent::Progress(p) => {
            let _ = app.emit("inspect-progress", p);
        }
        InspectEvent::Thumbnail(t) => {
            // The webview can only load the file once the asset scope allows it.
            if let Err(e) = app.asset_protocol_scope().allow_file(&t.path) {
                tracing::warn!(reason = %e, "thumbnail not allowed in asset scope");
                return;
            }
            let _ = app.emit("inspect-thumbnail", t);
        }
    })
    .inspect_err(|_| {
        let _ = fs::remove_dir_all(&cache);
    })?;
    let mut metadata = inspected.metadata;
    metadata.preview_url = preview.media_url().to_owned();
    let mut current = state
        .current
        .lock()
        .map_err(|_| AppError::Internal("media state poisoned".into()))?;
    // A newer load already owns the preview; drop this one's files.
    if state.latest_load.load(Ordering::SeqCst) != load_id {
        let _ = fs::remove_dir_all(&inspected.cache);
        return Ok(metadata);
    }
    replace_current(
        &mut current,
        CurrentMedia {
            preview: inspected.preview,
            cache: inspected.cache,
            thumbnails: inspected.thumbnails,
        },
    );
    Ok(metadata)
}
pub fn cleanup(state: &MediaState) {
    if let Ok(mut c) = state.current.lock() {
        if let Some(old) = c.take() {
            let _ = fs::remove_dir_all(old.cache);
        };
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_extension_before_probe() {
        assert!(matches!(
            validate_input(Path::new("x.avi")),
            Err(AppError::UnsupportedInput(_))
        ));
        assert!(matches!(
            validate_input(Path::new("x.ts")),
            Err(AppError::UnsupportedInput(_))
        ));
        // Supported extensions pass the extension gate and fail on existence.
        for name in ["x.mp4", "x.M4V", "x.mov", "x.MKV", "x.webm"] {
            assert!(matches!(
                validate_input(Path::new(name)),
                Err(AppError::MissingInput(_))
            ));
        }
    }
    fn generate(path: &Path, codec: &[&str]) {
        let status = Command::new("ffmpeg")
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=s=320x240:r=30:d=4",
                "-an",
                "-g",
                "60",
            ])
            .args(codec)
            .arg("-y")
            .arg(path)
            .status()
            .unwrap();
        assert!(status.success(), "failed to generate {}", path.display());
    }
    #[test]
    fn matroska_and_webm_inputs_probe_index_and_proxy() {
        let dir = tempfile::tempdir().unwrap();
        let mkv = dir.path().join("clip.mkv");
        let webm = dir.path().join("clip.webm");
        generate(&mkv, &["-c:v", "libx264", "-pix_fmt", "yuv420p"]);
        generate(
            &webm,
            &[
                "-c:v",
                "libvpx-vp9",
                "-deadline",
                "realtime",
                "-cpu-used",
                "8",
            ],
        );
        for (path, codec) in [(&mkv, "h264"), (&webm, "vp9")] {
            let metadata = probe(path).unwrap();
            assert_eq!(metadata.codec, codec);
            assert_eq!((metadata.width, metadata.height), (320, 240));
            assert!(metadata.keyframes_micros.len() >= 2, "{path:?}");
            assert_eq!(metadata.keyframes_micros[0], 0);
            let cache = dir.path().join(codec);
            fs::create_dir_all(&cache).unwrap();
            let proxy = preview_proxy(path, metadata.duration_micros, &cache).unwrap();
            assert_eq!(probe(&proxy).unwrap().codec, "h264");
        }
    }
    #[test]
    fn rejects_container_that_disagrees_with_extension() {
        let dir = tempfile::tempdir().unwrap();
        let mkv = dir.path().join("real.mkv");
        generate(&mkv, &["-c:v", "libx264", "-pix_fmt", "yuv420p"]);
        let mislabeled = dir.path().join("fake.mp4");
        fs::copy(&mkv, &mislabeled).unwrap();
        assert!(matches!(probe(&mislabeled), Err(AppError::Probe(_))));
        let avi = dir.path().join("clip.avi");
        generate(&avi, &["-c:v", "mpeg4"]);
        assert!(matches!(
            validate_input(&avi),
            Err(AppError::UnsupportedInput(_))
        ));
        let avi_as_mp4 = dir.path().join("avi.mp4");
        fs::copy(&avi, &avi_as_mp4).unwrap();
        assert!(matches!(probe(&avi_as_mp4), Err(AppError::Probe(_))));
    }
    #[test]
    fn parses_fractional_rate() {
        assert!((parse_rate(Some("30000/1001")) - 29.970).abs() < 0.01);
        assert_eq!(parse_rate(Some("0/0")), 30.0)
    }
    #[test]
    fn probes_valid_silent_and_rejects_malformed_media() {
        let dir = tempfile::tempdir().unwrap();
        let valid = dir.path().join("silent.mp4");
        let status = Command::new("ffmpeg")
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                "color=c=blue:s=320x240:r=30:d=1",
                "-an",
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
                "-y",
            ])
            .arg(&valid)
            .status()
            .unwrap();
        assert!(status.success());
        let metadata = probe(&valid).unwrap();
        assert_eq!((metadata.width, metadata.height), (320, 240));
        assert!(!metadata.has_audio);
        assert_eq!(metadata.audio_codec, None);
        let malformed = dir.path().join("broken.mp4");
        fs::write(&malformed, b"not media").unwrap();
        assert!(matches!(probe(&malformed), Err(AppError::Probe(_))));
    }
    #[test]
    fn indexes_keyframes_ascending_from_zero() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("gop.mp4");
        let status = Command::new("ffmpeg")
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=s=160x120:r=30:d=5",
                "-c:v",
                "libx264",
                "-g",
                "60",
                "-keyint_min",
                "60",
                "-sc_threshold",
                "0",
                "-pix_fmt",
                "yuv420p",
                "-y",
            ])
            .arg(&source)
            .status()
            .unwrap();
        assert!(status.success());
        let metadata = probe(&source).unwrap();
        assert_eq!(metadata.keyframes_micros, vec![0, 2_000_000, 4_000_000]);
        let broken = dir.path().join("broken.mp4");
        fs::write(&broken, b"not media").unwrap();
        assert!(keyframes(&broken).is_err());
        let k = [0, 2_000_000, 4_000_000];
        assert_eq!(keyframe_at_or_before(&k, 2_500_000), Some(2_000_000));
        assert_eq!(keyframe_at_or_before(&k, 2_000_000), Some(2_000_000));
        assert_eq!(keyframe_at_or_before(&k, 9_000_000), Some(4_000_000));
        assert_eq!(keyframe_at_or_before(&[33_000], 0), Some(33_000));
        assert_eq!(keyframe_at_or_before(&[], 1), None);
    }
    #[test]
    fn replacement_cleans_previous_cache() {
        let dir = tempfile::tempdir().unwrap();
        let old_cache = dir.path().join("old-cache");
        fs::create_dir(&old_cache).unwrap();
        fs::write(old_cache.join("frame.jpg"), b"x").unwrap();
        let input = dir.path().join("new.mp4");
        let mut current = Some(CurrentMedia {
            preview: dir.path().join("old.mp4"),
            cache: old_cache.clone(),
            thumbnails: vec![],
        });
        replace_current(
            &mut current,
            CurrentMedia {
                preview: input.clone(),
                cache: dir.path().join("new-cache"),
                thumbnails: vec![],
            },
        );
        assert!(!old_cache.exists());
        assert_eq!(current.as_ref().unwrap().preview, input);
        cleanup(&MediaState {
            current: Mutex::new(current),
            latest_load: AtomicU64::new(0),
        });
    }
    fn atom_order(path: &Path) -> Vec<String> {
        let bytes = fs::read(path).unwrap();
        let mut atoms = vec![];
        let mut pos = 0usize;
        while pos + 8 <= bytes.len() {
            let len = u32::from_be_bytes(bytes[pos..pos + 4].try_into().unwrap()) as usize;
            atoms.push(String::from_utf8_lossy(&bytes[pos + 4..pos + 8]).into_owned());
            if len < 8 {
                break;
            }
            pos += len;
        }
        atoms
    }
    fn keyframe_times(path: &Path) -> Vec<f64> {
        let out = Command::new("ffprobe")
            .args([
                "-v",
                "error",
                "-select_streams",
                "v",
                "-skip_frame",
                "nokey",
                "-show_entries",
                "frame=pts_time",
                "-of",
                "csv",
            ])
            .arg(path)
            .output()
            .unwrap();
        assert!(out.status.success());
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter_map(|line| line.split(',').nth(1)?.parse::<f64>().ok())
            .collect()
    }
    #[test]
    fn preview_proxy_moves_moov_forward_and_densifies_keyframes() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("sparse.mp4");
        let status = Command::new("ffmpeg")
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=s=320x240:r=30:d=4",
                "-an",
                "-c:v",
                "libx264",
                "-g",
                "120",
                "-keyint_min",
                "120",
                "-sc_threshold",
                "0",
                "-pix_fmt",
                "yuv420p",
                "-y",
            ])
            .arg(&source)
            .status()
            .unwrap();
        assert!(status.success());
        let source_atoms = atom_order(&source);
        assert!(
            source_atoms.iter().position(|a| a == "mdat")
                < source_atoms.iter().position(|a| a == "moov")
        );
        assert!(keyframe_times(&source).len() <= 2);
        let proxy = preview_proxy(&source, 4_000_000, &dir.path().join("cache")).unwrap();
        let atoms = atom_order(&proxy);
        let moov = atoms.iter().position(|a| a == "moov").expect("moov atom");
        let mdat = atoms.iter().position(|a| a == "mdat").expect("mdat atom");
        assert!(moov < mdat);
        let dense = keyframe_times(&proxy);
        assert!(dense.len() >= 3);
        assert!(dense.windows(2).all(|pair| pair[1] - pair[0] <= 1.5));
        let metadata = probe(&proxy).unwrap();
        assert_eq!(metadata.codec, "h264");
        assert!((metadata.duration_micros as i64 - 4_000_000).abs() <= 150_000);
    }
    fn generated_clip(dir: &Path, seconds: u32) -> PathBuf {
        let source = dir.join("clip.mp4");
        let status = Command::new("ffmpeg")
            .args(["-hide_banner", "-loglevel", "error", "-f", "lavfi", "-i"])
            .arg(format!("testsrc2=s=160x120:r=10:d={seconds}"))
            .args(["-an", "-c:v", "libx264", "-pix_fmt", "yuv420p", "-y"])
            .arg(&source)
            .status()
            .unwrap();
        assert!(status.success());
        source
    }
    fn collect(source: &Path, load_id: u64, cache: &Path) -> (Inspected, Vec<InspectEvent>) {
        let mut events = vec![];
        let inspected = inspect_input(source, load_id, cache, &mut |e| events.push(e)).unwrap();
        (inspected, events)
    }
    #[test]
    fn inspection_reports_steps_in_order_with_weighted_fractions() {
        let dir = tempfile::tempdir().unwrap();
        let source = generated_clip(dir.path(), 3);
        let (inspected, events) = collect(&source, 7, &dir.path().join("cache"));
        let progress = events
            .iter()
            .filter_map(|e| match e {
                InspectEvent::Progress(p) => Some(p.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(progress.iter().all(|p| p.load_id == 7));
        let mut steps = progress.iter().map(|p| p.step).collect::<Vec<_>>();
        steps.dedup();
        assert_eq!(
            steps,
            [
                STEP_CONTAINER,
                STEP_KEYFRAMES,
                STEP_PREVIEW,
                STEP_THUMBNAILS
            ]
        );
        let fractions = progress.iter().map(|p| p.fraction).collect::<Vec<_>>();
        assert_eq!(&fractions[..4], &[0.0, 0.1, 0.3, 0.8]);
        assert!(fractions.windows(2).all(|w| w[0] <= w[1]));
        assert!((fractions.last().unwrap() - 1.0).abs() < 1e-9);
        assert_ne!(inspected.preview, source);
        assert!(inspected.preview.starts_with(&inspected.cache));
    }
    #[test]
    fn thumbnails_are_reported_one_by_one_and_match_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let source = generated_clip(dir.path(), 15);
        let (inspected, events) = collect(&source, 3, &dir.path().join("cache"));
        let thumbs = events
            .iter()
            .filter_map(|e| match e {
                InspectEvent::Thumbnail(t) => Some(t.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(thumbs.len(), 14);
        assert!(thumbs
            .iter()
            .enumerate()
            .all(|(i, t)| t.index == i && t.count == 14 && t.load_id == 3));
        assert_eq!(
            thumbs.iter().map(|t| t.path.clone()).collect::<Vec<_>>(),
            inspected.metadata.thumbnails
        );
        assert!(inspected.thumbnails.iter().all(|p| p.is_file()));
        assert_eq!(inspected.metadata.thumbnail_warning, None);
    }
    #[test]
    fn inspection_rejects_malformed_media_before_later_steps() {
        let dir = tempfile::tempdir().unwrap();
        let bad = dir.path().join("bad.mp4");
        fs::write(&bad, b"not media").unwrap();
        let mut events = vec![];
        let result = inspect_input(&bad, 1, &dir.path().join("cache"), &mut |e| events.push(e));
        assert!(matches!(result, Err(AppError::Probe(_))));
        assert_eq!(events.len(), 1);
    }
    #[test]
    fn preview_proxy_rejects_unusable_input() {
        let dir = tempfile::tempdir().unwrap();
        let bad = dir.path().join("bad.mp4");
        fs::write(&bad, b"not media").unwrap();
        assert!(preview_proxy(&bad, 1_000_000, &dir.path().join("cache")).is_err());
    }
}
