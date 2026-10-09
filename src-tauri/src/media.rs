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
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
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
    /// The thumbnail job's slots, so `/thumb/<i>` serves files as they land.
    pub thumbnails: ThumbnailSlots,
    pub job: Option<ThumbnailJob>,
}
/// Thumbnail paths by index; `None` until that file is written.
pub type ThumbnailSlots = Arc<Mutex<Vec<Option<PathBuf>>>>;
fn lock_slots(slots: &ThumbnailSlots) -> std::sync::MutexGuard<'_, Vec<Option<PathBuf>>> {
    slots.lock().unwrap_or_else(|e| e.into_inner())
}
/// Thumbnail generation for one load, running beside the proxy encode.
pub struct ThumbnailJob {
    pub load_id: u64,
    cancel: Arc<AtomicBool>,
    pub slots: ThumbnailSlots,
    handle: JoinHandle<()>,
}
impl ThumbnailJob {
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }
    pub fn join(self) {
        let _ = self.handle.join();
    }
    /// Waits up to `limit` for the job to stop; returns whether it did.
    pub fn join_within(self, limit: Duration) -> bool {
        let deadline = Instant::now() + limit;
        while !self.handle.is_finished() {
            if Instant::now() >= deadline {
                return false;
            }
            thread::sleep(Duration::from_millis(20));
        }
        let _ = self.handle.join();
        true
    }
}
// Cancels a superseded load's job and removes its cache only after the job
// has stopped writing into it, on a thread so the caller never waits.
fn retire(job: Option<ThumbnailJob>, cache: PathBuf) -> JoinHandle<()> {
    if let Some(job) = &job {
        tracing::debug!(load_id = job.load_id, "cancelling superseded thumbnail job");
        job.cancel();
    }
    thread::spawn(move || {
        if let Some(job) = job {
            job.join();
        }
        let _ = fs::remove_dir_all(cache);
    })
}
fn replace_current(
    current: &mut Option<CurrentMedia>,
    next: CurrentMedia,
) -> Option<JoinHandle<()>> {
    let retired = current.take().map(|old| retire(old.job, old.cache));
    *current = Some(next);
    retired
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
pub const THUMBNAIL_WORKERS: usize = 4;
/// Writes one thumbnail at `seconds` into `file`; injectable for tests.
pub type ThumbnailSpawner = Arc<dyn Fn(&Path, f64, &Path) -> Result<(), String> + Send + Sync>;
// One FFmpeg spawn per timestamp so each frame can be reported as soon as it
// exists; `-ss` before `-i` keeps every seek cheap on long files.
fn thumbnail_file(path: &Path, seconds: f64, file: &Path) -> Result<(), String> {
    let status = Command::new("ffmpeg")
        .args(["-hide_banner", "-nostdin", "-loglevel", "error", "-ss"])
        .arg(format!("{seconds:.6}"))
        .arg("-i")
        .arg(path)
        .args(["-frames:v", "1", "-vf", "scale=240:-2", "-q:v", "4", "-y"])
        .arg(file)
        .status()
        .map_err(|e| e.to_string())?;
    if !status.success() || !file.is_file() {
        return Err(format!(
            "FFmpeg could not write thumbnail {}",
            file.display()
        ));
    }
    Ok(())
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
// A pool of `THUMBNAIL_WORKERS` threads takes indices from a shared counter;
// any per-file failure stops the pool and falls back to one batch run.
#[allow(clippy::too_many_arguments)]
fn run_thumbnails(
    path: &Path,
    duration: u64,
    dir: &Path,
    load_id: u64,
    cancel: &AtomicBool,
    slots: &ThumbnailSlots,
    emit: &(dyn Fn(InspectEvent) + Send + Sync),
    spawner: &ThumbnailSpawner,
) {
    let count = lock_slots(slots).len();
    let interval = duration as f64 / 1_000_000.0 / count as f64;
    let report = |index: usize, file: &Path| {
        lock_slots(slots)[index] = Some(file.to_path_buf());
        if !cancel.load(Ordering::SeqCst) {
            emit(InspectEvent::Thumbnail(InspectThumbnail {
                load_id,
                index,
                count,
                path: file.to_string_lossy().into_owned(),
            }));
        }
    };
    let next = AtomicUsize::new(0);
    let failure = Mutex::new(None::<String>);
    thread::scope(|scope| {
        for _ in 0..THUMBNAIL_WORKERS.min(count) {
            scope.spawn(|| loop {
                if cancel.load(Ordering::SeqCst) || failure.lock().is_ok_and(|f| f.is_some()) {
                    return;
                }
                let index = next.fetch_add(1, Ordering::SeqCst);
                if index >= count {
                    return;
                }
                let file = dir.join(format!("frame-{:02}.jpg", index + 1));
                match spawner(path, interval * (index as f64 + 0.5), &file) {
                    Ok(()) => report(index, &file),
                    Err(e) => {
                        if let Ok(mut f) = failure.lock() {
                            f.get_or_insert(e);
                        }
                        return;
                    }
                }
            });
        }
    });
    if cancel.load(Ordering::SeqCst) {
        return;
    }
    let failure = failure.into_inner().unwrap_or_else(|e| e.into_inner());
    let result = match failure {
        None => Ok(()),
        Some(e) => {
            tracing::warn!(reason = %e, "per-file thumbnails failed; using the batch path");
            lock_slots(slots).iter_mut().for_each(|s| *s = None);
            let _ = fs::remove_dir_all(dir);
            fs::create_dir_all(dir)
                .map_err(|e| e.to_string())
                .and_then(|_| thumbnails_batch(path, duration, dir))
                .map(|files| {
                    lock_slots(slots).resize(files.len(), None);
                    for (index, file) in files.iter().enumerate() {
                        report(index, Path::new(file));
                    }
                })
                .inspect_err(|_| {
                    lock_slots(slots).clear();
                    let _ = fs::remove_dir_all(dir);
                })
        }
    };
    if cancel.load(Ordering::SeqCst) {
        return;
    }
    let (thumbnails, warning) = match result {
        Ok(()) => (
            lock_slots(slots)
                .iter()
                .flatten()
                .map(|p| p.to_string_lossy().into_owned())
                .collect(),
            None,
        ),
        Err(e) => (vec![], Some(e)),
    };
    emit(InspectEvent::ThumbnailsDone(InspectThumbnailsDone {
        load_id,
        thumbnails,
        warning,
    }));
}
pub fn start_thumbnail_job(
    path: &Path,
    duration: u64,
    dir: &Path,
    load_id: u64,
    emit: InspectEmitter,
    spawner: ThumbnailSpawner,
) -> ThumbnailJob {
    let count = thumbnail_count(duration) as usize;
    let cancel = Arc::new(AtomicBool::new(false));
    let slots: ThumbnailSlots = Arc::new(Mutex::new(vec![None; count]));
    let (path, dir) = (path.to_path_buf(), dir.to_path_buf());
    let handle = {
        let (cancel, slots) = (cancel.clone(), slots.clone());
        thread::spawn(move || {
            if let Err(e) = fs::create_dir_all(&dir) {
                lock_slots(&slots).clear();
                if !cancel.load(Ordering::SeqCst) {
                    emit(InspectEvent::ThumbnailsDone(InspectThumbnailsDone {
                        load_id,
                        thumbnails: vec![],
                        warning: Some(e.to_string()),
                    }));
                }
                return;
            }
            run_thumbnails(
                &path, duration, &dir, load_id, &cancel, &slots, &*emit, &spawner,
            );
        })
    };
    ThumbnailJob {
        load_id,
        cancel,
        slots,
        handle,
    }
}
// Source MP4s often carry sparse keyframes, invalid H.264 levels, or VUI
// timing that GStreamer's h264parse rejects, all of which make WebKitGTK
// drop the frames at a seek target and flash black. Re-encoding a proxy
// with dense keyframes, no B-frames, a leading moov atom, and fresh timing
// metadata keeps trim-point timestamps intact while making seeks land
// instantly. The shorter side is capped at 1080 px (never upscaled, even
// sizes) and encoded with `ultrafast`, which roughly halves the encode time;
// the proxy is only for previewing, and export always reads the original
// file at its original resolution. `-t` pins the proxy to the probed
// duration so the player's clock agrees with the timeline even when the
// source container duration is wrong.
const PROXY_SCALE: &str = "scale=w='if(gte(iw,ih),-2,min(trunc(iw/2)*2,1080))':h='if(gte(iw,ih),min(trunc(ih/2)*2,1080),-2)'";
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
            PROXY_SCALE,
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-bf",
            "0",
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
/// Sent once per load when thumbnail generation finishes (not when cancelled).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InspectThumbnailsDone {
    pub load_id: u64,
    pub thumbnails: Vec<String>,
    pub warning: Option<String>,
}
#[derive(Debug, Clone, PartialEq)]
pub enum InspectEvent {
    Progress(InspectProgress),
    Thumbnail(InspectThumbnail),
    ThumbnailsDone(InspectThumbnailsDone),
}
/// Shared with the thumbnail workers, so it must be callable from any thread.
pub type InspectEmitter = Arc<dyn Fn(InspectEvent) + Send + Sync>;
pub struct Inspected {
    pub metadata: VideoMetadata,
    pub preview: PathBuf,
    pub cache: PathBuf,
    pub job: ThumbnailJob,
}
fn cache_dir(load_id: u64) -> PathBuf {
    ProjectDirs::from("com", "wochap", "wodeo")
        .map(|p| p.cache_dir().to_path_buf())
        .unwrap_or_else(std::env::temp_dir)
        .join(format!("preview-{}-{load_id}", std::process::id()))
}
// Runs the inspection steps in the order the editor lists them, with weights
// probe 0.1, keyframes 0.2, preview 0.7. Thumbnails start right after the
// probe and keep running after this returns. Only the probe is fatal.
pub fn inspect_input(
    canonical: &Path,
    load_id: u64,
    cache: &Path,
    emit: InspectEmitter,
    spawner: ThumbnailSpawner,
) -> Result<Inspected, AppError> {
    let progress = |step, fraction| {
        emit(InspectEvent::Progress(InspectProgress {
            load_id,
            step,
            fraction,
        }))
    };
    progress(STEP_CONTAINER, 0.0);
    let mut metadata = inspect(canonical, Some(canonical))?;
    let _ = fs::remove_dir_all(cache);
    let job = start_thumbnail_job(
        canonical,
        metadata.duration_micros,
        &cache.join("thumbs"),
        load_id,
        emit.clone(),
        spawner,
    );
    progress(STEP_KEYFRAMES, 0.1);
    metadata.keyframes_micros = keyframes(canonical).unwrap_or_else(|e| {
        tracing::warn!(path = %canonical.display(), reason = %e, "keyframe index unavailable");
        vec![]
    });
    progress(STEP_PREVIEW, 0.3);
    let preview = preview_proxy(canonical, metadata.duration_micros, cache).unwrap_or_else(|e| {
        tracing::warn!(reason = %e, "preview proxy unavailable; serving the original file");
        canonical.to_path_buf()
    });
    progress(STEP_PREVIEW, 1.0);
    // Whatever has landed so far; `inspect-thumbnails-done` is authoritative.
    metadata.thumbnails = lock_slots(&job.slots)
        .iter()
        .flatten()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    Ok(Inspected {
        metadata,
        preview,
        cache: cache.to_path_buf(),
        job,
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
    let handle = app.clone();
    let emit: InspectEmitter = Arc::new(move |event| match event {
        InspectEvent::Progress(p) => {
            let _ = handle.emit("inspect-progress", p);
        }
        InspectEvent::Thumbnail(t) => {
            // The webview can only load the file once the asset scope allows it.
            if let Err(e) = handle.asset_protocol_scope().allow_file(&t.path) {
                tracing::warn!(reason = %e, "thumbnail not allowed in asset scope");
                return;
            }
            let _ = handle.emit("inspect-thumbnail", t);
        }
        InspectEvent::ThumbnailsDone(d) => {
            let _ = handle.emit("inspect-thumbnails-done", d);
        }
    });
    let inspected = inspect_input(&canonical, load_id, &cache, emit, Arc::new(thumbnail_file))
        .inspect_err(|_| {
            let _ = fs::remove_dir_all(&cache);
        })?;
    let mut metadata = inspected.metadata;
    metadata.preview_url = format!("{}?load={load_id}", preview.media_url());
    let mut current = state
        .current
        .lock()
        .map_err(|_| AppError::Internal("media state poisoned".into()))?;
    // A newer load already owns the preview; drop this one's files.
    if state.latest_load.load(Ordering::SeqCst) != load_id {
        retire(Some(inspected.job), inspected.cache);
        return Ok(metadata);
    }
    replace_current(
        &mut current,
        CurrentMedia {
            preview: inspected.preview,
            cache: inspected.cache,
            thumbnails: inspected.job.slots.clone(),
            job: Some(inspected.job),
        },
    );
    Ok(metadata)
}
// On exit FFmpeg must never block quitting: wait about 2 s for in-flight
// thumbnails, then remove the cache regardless.
pub fn cleanup(state: &MediaState) {
    if let Ok(mut c) = state.current.lock() {
        if let Some(old) = c.take() {
            if let Some(job) = old.job {
                job.cancel();
                job.join_within(Duration::from_secs(2));
            }
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
            thumbnails: Arc::default(),
            job: None,
        });
        let retired = replace_current(
            &mut current,
            CurrentMedia {
                preview: input.clone(),
                cache: dir.path().join("new-cache"),
                thumbnails: Arc::default(),
                job: None,
            },
        );
        retired.unwrap().join().unwrap();
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
    type Events = Arc<Mutex<Vec<InspectEvent>>>;
    fn recorder() -> (Events, InspectEmitter) {
        let events: Events = Arc::default();
        let sink = events.clone();
        (events, Arc::new(move |e| sink.lock().unwrap().push(e)))
    }
    fn collect(source: &Path, load_id: u64, cache: &Path) -> (Inspected, Events) {
        let (events, emit) = recorder();
        let inspected =
            inspect_input(source, load_id, cache, emit, Arc::new(thumbnail_file)).unwrap();
        (inspected, events)
    }
    fn thumbnail_events(events: &Events) -> (Vec<InspectThumbnail>, Vec<InspectThumbnailsDone>) {
        let events = events.lock().unwrap();
        let thumbs = events
            .iter()
            .filter_map(|e| match e {
                InspectEvent::Thumbnail(t) => Some(t.clone()),
                _ => None,
            })
            .collect();
        let done = events
            .iter()
            .filter_map(|e| match e {
                InspectEvent::ThumbnailsDone(d) => Some(d.clone()),
                _ => None,
            })
            .collect();
        (thumbs, done)
    }
    #[test]
    fn inspection_reports_steps_in_order_with_weighted_fractions() {
        let dir = tempfile::tempdir().unwrap();
        let source = generated_clip(dir.path(), 3);
        let (inspected, events) = collect(&source, 7, &dir.path().join("cache"));
        let progress = events
            .lock()
            .unwrap()
            .iter()
            .filter_map(|e| match e {
                InspectEvent::Progress(p) => Some(p.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(progress.iter().all(|p| p.load_id == 7));
        let mut steps = progress.iter().map(|p| p.step).collect::<Vec<_>>();
        steps.dedup();
        assert_eq!(steps, [STEP_CONTAINER, STEP_KEYFRAMES, STEP_PREVIEW]);
        let fractions = progress.iter().map(|p| p.fraction).collect::<Vec<_>>();
        assert_eq!(fractions, [0.0, 0.1, 0.3, 1.0]);
        assert_ne!(inspected.preview, source);
        assert!(inspected.preview.starts_with(&inspected.cache));
        inspected.job.join();
    }
    #[test]
    fn thumbnails_stream_per_file_and_finish_with_a_done_event() {
        let dir = tempfile::tempdir().unwrap();
        let source = generated_clip(dir.path(), 15);
        let active = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let spawner: ThumbnailSpawner = {
            let (active, peak) = (active.clone(), peak.clone());
            Arc::new(move |path, seconds, file| {
                let now = active.fetch_add(1, Ordering::SeqCst) + 1;
                peak.fetch_max(now, Ordering::SeqCst);
                let result = thumbnail_file(path, seconds, file);
                active.fetch_sub(1, Ordering::SeqCst);
                result
            })
        };
        let (events, emit) = recorder();
        let cache = dir.path().join("cache");
        let inspected = inspect_input(&source, 3, &cache, emit, spawner).unwrap();
        assert_eq!(inspected.metadata.thumbnail_warning, None);
        let slots = inspected.job.slots.clone();
        inspected.job.join();
        let (thumbs, done) = thumbnail_events(&events);
        assert_eq!(thumbs.len(), 14);
        assert!(thumbs.iter().all(|t| t.count == 14 && t.load_id == 3));
        let mut indices = thumbs.iter().map(|t| t.index).collect::<Vec<_>>();
        indices.sort_unstable();
        assert_eq!(indices, (0..14).collect::<Vec<_>>());
        assert_eq!(done.len(), 1);
        assert_eq!(done[0].load_id, 3);
        assert_eq!(done[0].warning, None);
        let filled = slots
            .lock()
            .unwrap()
            .iter()
            .map(|p| p.clone().unwrap())
            .collect::<Vec<_>>();
        assert!(filled.iter().all(|p| p.is_file()));
        assert_eq!(
            done[0].thumbnails,
            filled
                .iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect::<Vec<_>>()
        );
        let max = peak.load(Ordering::SeqCst);
        assert!((1..=THUMBNAIL_WORKERS).contains(&max), "peak {max}");
    }
    #[test]
    fn superseded_job_is_cancelled_and_its_cache_removed_after_join() {
        let dir = tempfile::tempdir().unwrap();
        let cache = dir.path().join("cache");
        let thumbs = cache.join("thumbs");
        let orphaned = Arc::new(AtomicUsize::new(0));
        let spawner: ThumbnailSpawner = {
            let orphaned = orphaned.clone();
            Arc::new(move |_, _, file| {
                thread::sleep(Duration::from_millis(150));
                if !file.parent().unwrap().is_dir() {
                    orphaned.fetch_add(1, Ordering::SeqCst);
                    return Err("cache removed under the job".into());
                }
                fs::write(file, b"jpeg").map_err(|e| e.to_string())
            })
        };
        let (events, emit) = recorder();
        let job = start_thumbnail_job(
            Path::new("unused.mp4"),
            15_000_000,
            &thumbs,
            9,
            emit,
            spawner,
        );
        thread::sleep(Duration::from_millis(50));
        retire(Some(job), cache.clone()).join().unwrap();
        assert!(!cache.exists());
        assert_eq!(orphaned.load(Ordering::SeqCst), 0);
        let (written, done) = thumbnail_events(&events);
        assert!(done.is_empty());
        assert!(written.len() < 14);
    }
    #[test]
    fn inspection_rejects_malformed_media_before_later_steps() {
        let dir = tempfile::tempdir().unwrap();
        let bad = dir.path().join("bad.mp4");
        fs::write(&bad, b"not media").unwrap();
        let (events, emit) = recorder();
        let result = inspect_input(
            &bad,
            1,
            &dir.path().join("cache"),
            emit,
            Arc::new(thumbnail_file),
        );
        assert!(matches!(result, Err(AppError::Probe(_))));
        assert_eq!(events.lock().unwrap().len(), 1);
    }
    fn sized_clip(dir: &Path, w: u32, h: u32) -> PathBuf {
        let source = dir.join(format!("{w}x{h}.mp4"));
        let status = Command::new("ffmpeg")
            .args(["-hide_banner", "-loglevel", "error", "-f", "lavfi", "-i"])
            .arg(format!("testsrc2=s={w}x{h}:r=10:d=1"))
            .args([
                "-an",
                "-c:v",
                "libx264",
                "-preset",
                "ultrafast",
                "-pix_fmt",
                "yuv420p",
                "-y",
            ])
            .arg(&source)
            .status()
            .unwrap();
        assert!(status.success());
        source
    }
    fn stream_field(path: &Path, entries: &str) -> String {
        let out = Command::new("ffprobe")
            .args([
                "-v",
                "error",
                "-select_streams",
                "v:0",
                "-show_entries",
                entries,
            ])
            .args(["-of", "csv=p=0"])
            .arg(path)
            .output()
            .unwrap();
        assert!(out.status.success());
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    }
    fn proxy_of(dir: &Path, source: &Path) -> PathBuf {
        let cache = dir.join(source.file_stem().unwrap());
        preview_proxy(source, 1_000_000, &cache).unwrap()
    }
    #[test]
    fn preview_proxy_caps_the_shorter_side_at_1080() {
        let dir = tempfile::tempdir().unwrap();
        for ((w, h), expected) in [
            ((3840, 2160), (1920, 1080)),
            ((1080, 1920), (1080, 1920)),
            ((1440, 2560), (1080, 1920)),
            ((640, 360), (640, 360)),
            ((1080, 1080), (1080, 1080)),
        ] {
            let source = sized_clip(dir.path(), w, h);
            let proxy = probe(&proxy_of(dir.path(), &source)).unwrap();
            assert_eq!((proxy.width, proxy.height), expected, "{w}x{h}");
            assert!(proxy.width.is_multiple_of(2) && proxy.height.is_multiple_of(2));
        }
    }
    #[test]
    fn preview_proxy_keeps_timestamps_and_has_no_b_frames() {
        let dir = tempfile::tempdir().unwrap();
        let source = sized_clip(dir.path(), 3840, 2160);
        let proxy = proxy_of(dir.path(), &source);
        let times = |p: &Path| stream_field(p, "frame=pts_time");
        assert_eq!(times(&source), times(&proxy));
        assert_eq!(stream_field(&proxy, "stream=has_b_frames"), "0");
    }
    #[test]
    fn preview_proxy_rejects_unusable_input() {
        let dir = tempfile::tempdir().unwrap();
        let bad = dir.path().join("bad.mp4");
        fs::write(&bad, b"not media").unwrap();
        assert!(preview_proxy(&bad, 1_000_000, &dir.path().join("cache")).is_err());
    }
}
