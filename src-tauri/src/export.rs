use crate::{
    acceleration::{render_nodes, AccelerationRecord, AccelerationState},
    config::{Format, Quality},
    error::AppError,
    logging::LogPaths,
    media,
};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    path::{Path, PathBuf},
    process::Stdio,
    sync::atomic::{AtomicBool, AtomicI32, Ordering},
    time::{Duration, Instant},
};
use tauri::{Emitter, Manager};
use tokio::io::{AsyncBufReadExt, BufReader};
#[derive(Default)]
pub struct ExportState {
    active: AtomicBool,
    cancel: AtomicBool,
    pid: AtomicI32,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportRequest {
    input: String,
    output: String,
    start_micros: u64,
    end_micros: u64,
    #[serde(default)]
    format: Format,
    #[serde(default)]
    quality: Quality,
}
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct ExportProgress {
    fraction: f64,
    out_time_micros: u64,
    attempt: String,
    bytes_written: u64,
    estimated_bytes: Option<u64>,
    /// True when `estimated_bytes` comes from a nominal tier bitrate.
    approximate: bool,
    remaining_micros: Option<u64>,
    step: String,
}
const STEP_COPYING: &str = "Copying streams";
const STEP_ENCODING: &str = "Decoding and encoding";
const STEP_VALIDATING: &str = "Validating output";
const STEP_FINALIZING: &str = "Finalizing file";
type ProgressSink<'a> = Box<dyn FnMut(&ExportProgress) + Send + 'a>;
// Weight of the newest sample in the time-remaining moving average.
const ETA_ALPHA: f64 = 0.3;
#[derive(Default)]
struct Eta {
    smoothed: Option<f64>,
}
impl Eta {
    fn update(&mut self, elapsed: Duration, fraction: f64) -> Option<u64> {
        if fraction <= 0.0 {
            return None;
        }
        let raw = elapsed.as_secs_f64() * (1.0 - fraction) / fraction;
        let smoothed = self
            .smoothed
            .map_or(raw, |prev| ETA_ALPHA * raw + (1.0 - ETA_ALPHA) * prev);
        self.smoothed = Some(smoothed);
        Some((smoothed * 1e6).round() as u64)
    }
}
// Holds the latest payload so step changes re-emit it with the fraction
// unchanged; each attempt restarts the clock and the moving average.
struct Reporter<'a> {
    sink: ProgressSink<'a>,
    current: ExportProgress,
    started: Instant,
    eta: Eta,
}
impl<'a> Reporter<'a> {
    fn new(sink: ProgressSink<'a>, estimate: Option<(u64, bool)>) -> Self {
        Self {
            sink,
            current: ExportProgress {
                estimated_bytes: estimate.map(|(bytes, _)| bytes),
                approximate: estimate.is_some_and(|(_, approximate)| approximate),
                ..Default::default()
            },
            started: Instant::now(),
            eta: Eta::default(),
        }
    }
    fn send(&mut self) {
        (self.sink)(&self.current)
    }
    fn begin_attempt(&mut self, attempt: &str, step: &str) {
        self.current.fraction = 0.0;
        self.current.out_time_micros = 0;
        self.current.bytes_written = 0;
        self.current.remaining_micros = None;
        self.current.attempt = attempt.into();
        self.current.step = step.into();
        self.started = Instant::now();
        self.eta = Eta::default();
        self.send()
    }
    fn step(&mut self, step: &str) {
        self.current.step = step.into();
        self.send()
    }
    fn progress(&mut self, out_time_micros: u64, duration: u64, bytes_written: u64) {
        let fraction = (out_time_micros as f64 / duration as f64).clamp(0.0, 1.0);
        self.current.fraction = fraction;
        self.current.out_time_micros = out_time_micros;
        self.current.bytes_written = bytes_written;
        self.current.remaining_micros = self.eta.update(self.started.elapsed(), fraction);
        self.send()
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    output: String,
    acceleration: Vec<AccelerationRecord>,
    effective_start_micros: Option<u64>,
}
struct ActiveGuard<'a>(&'a ExportState);
impl Drop for ActiveGuard<'_> {
    fn drop(&mut self) {
        self.0.active.store(false, Ordering::SeqCst);
        self.0.pid.store(0, Ordering::SeqCst);
        self.0.cancel.store(false, Ordering::SeqCst)
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AttemptKind {
    FullVaapi,
    VaapiEncode,
    Software,
}
impl AttemptKind {
    fn label(&self, format: Format) -> &'static str {
        match (self, format) {
            (Self::FullVaapi, _) => "VA-API decode + encode",
            (Self::VaapiEncode, _) => "Software decode + VA-API encode",
            (Self::Software, Format::Mp4) => "Software decode + libx264",
            (Self::Software, Format::Webm) => "Software decode + libvpx-vp9",
            (Self::Software, Format::Gif) => "Software decode + GIF palette",
            (Self::Software, Format::Copy) => "Stream copy",
        }
    }
}
struct ExportPlan<'a> {
    format: Format,
    quality: Quality,
    input: &'a Path,
    temp: &'a Path,
    start: u64,
    duration: u64,
}
fn attempts(format: Format) -> Vec<AttemptKind> {
    match format {
        Format::Mp4 => vec![
            AttemptKind::FullVaapi,
            AttemptKind::VaapiEncode,
            AttemptKind::Software,
        ],
        Format::Webm | Format::Gif | Format::Copy => vec![AttemptKind::Software],
    }
}
// Accumulates one `-progress` block; FFmpeg ends each block with `progress=`.
// `N/A` values (and negative times before the first frame) keep the last value.
#[derive(Default)]
struct ProgressBlock {
    out_time_micros: Option<u64>,
    total_size: Option<u64>,
}
impl ProgressBlock {
    fn feed(&mut self, line: &str) -> bool {
        match line.split_once('=') {
            Some(("out_time_us", v)) => {
                if let Ok(v) = v.parse() {
                    self.out_time_micros = Some(v)
                }
            }
            Some(("total_size", v)) => {
                if let Ok(v) = v.parse() {
                    self.total_size = Some(v)
                }
            }
            Some(("progress", _)) => return true,
            _ => {}
        }
        false
    }
}
// Formats like the frontend's `formatMicros`: `m:ss.mmm` or `h:mm:ss.mmm`.
fn clock(micros: u64) -> String {
    let ms = (micros + 500) / 1000;
    let (h, m, s) = (ms / 3_600_000, ms % 3_600_000 / 60_000, ms % 60_000 / 1000);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}.{:03}", ms % 1000)
    } else {
        format!("{m}:{s:02}.{:03}", ms % 1000)
    }
}
// Nominal video bitrates (bits/s) per re-encoding tier, plus the fixed audio
// bitrate from `audio_args`. GIF has no bitrate; it assumes about one bit per
// output pixel per frame after palette compression.
const GIF_BITS_PER_PIXEL: f64 = 1.0;
fn nominal_bitrate(format: Format, quality: Quality, has_audio: bool) -> Option<f64> {
    let (video, audio) = match format {
        Format::Mp4 => (
            match quality {
                Quality::Original => 8e6,
                Quality::High => 5e6,
                Quality::Small => 2.5e6,
            },
            192e3,
        ),
        Format::Webm => (
            match quality {
                Quality::Original => 6e6,
                Quality::High => 4e6,
                Quality::Small => 2e6,
            },
            128e3,
        ),
        Format::Gif | Format::Copy => return None,
    };
    Some(video + if has_audio { audio } else { 0.0 })
}
/// Estimated output bytes and whether the figure is approximate. `copy` scales
/// the source bitrate; re-encodes use the tier's nominal bitrate.
fn estimate_bytes(
    format: Format,
    quality: Quality,
    source: &media::VideoMetadata,
    duration_micros: u64,
) -> Option<(u64, bool)> {
    let seconds = duration_micros as f64 / 1e6;
    let bits = match format {
        Format::Copy => source.bit_rate? as f64 * seconds,
        Format::Gif => {
            let (fps, cap) = gif_settings(quality);
            let width = cap.map_or(source.width, |c| c.min(source.width)) as f64;
            let height = source.height as f64 * width / source.width.max(1) as f64;
            width * height * fps as f64 * seconds * GIF_BITS_PER_PIXEL
        }
        Format::Mp4 | Format::Webm => nominal_bitrate(format, quality, source.has_audio)? * seconds,
    };
    Some(((bits / 8.0).round() as u64, format != Format::Copy))
}
fn destination(input: &Path, output: &Path, format: Format) -> Result<PathBuf, AppError> {
    let input = input.canonicalize()?;
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let parent = parent
        .canonicalize()
        .map_err(|e| AppError::Destination(format!("destination directory: {e}")))?;
    let name = output
        .file_name()
        .ok_or_else(|| AppError::Destination("missing filename".into()))?;
    let required = format.output_extension(&input);
    if Path::new(name)
        .extension()
        .and_then(|x| x.to_str())
        .is_none_or(|x| !x.eq_ignore_ascii_case(&required))
    {
        return Err(AppError::Destination(format!(
            "output must end in .{required}"
        )));
    }
    let full = parent.join(name);
    if full.exists() && full.canonicalize()? == input {
        return Err(AppError::Destination(
            "source and destination are the same file".into(),
        ));
    }
    Ok(full)
}
// Bounding box for the longer and shorter side of each re-encoding tier.
fn video_cap(quality: Quality) -> Option<(u32, u32)> {
    match quality {
        Quality::Original => None,
        Quality::High => Some((1920, 1080)),
        Quality::Small => Some((1280, 720)),
    }
}
fn gif_settings(quality: Quality) -> (u32, Option<u32>) {
    match quality {
        Quality::Original => (15, None),
        Quality::High => (12, Some(720)),
        Quality::Small => (10, Some(480)),
    }
}
// Fits the frame inside the tier's box (orientation aware), never upscales,
// and keeps even dimensions for 4:2:0 encoders.
fn scale_filter(filter: &str, quality: Quality) -> String {
    match video_cap(quality) {
        None => format!("{filter}=w=trunc(iw/2)*2:h=trunc(ih/2)*2"),
        Some((long, short)) => format!(
            "{filter}=w='min(iw,if(gte(iw,ih),{long},{short}))':h='min(ih,if(gte(iw,ih),{short},{long}))':force_original_aspect_ratio=decrease:force_divisible_by=2"
        ),
    }
}
fn video_args(plan: &ExportPlan, kind: AttemptKind) -> Vec<String> {
    let q = plan.quality;
    let (crf, qp) = match q {
        Quality::Original => ("18", "20"),
        Quality::High => ("23", "24"),
        Quality::Small => ("28", "28"),
    };
    match (plan.format, kind) {
        (Format::Copy, _) => vec!["-c".into(), "copy".into()],
        (Format::Gif, _) => {
            let (fps, width) = gif_settings(q);
            let scale = width.map_or(String::new(), |w| {
                format!(",scale='min(iw,{w})':-1:flags=lanczos")
            });
            vec![
                "-vf".into(),
                format!("setpts=PTS-STARTPTS,fps={fps}{scale},split[a][b];[a]palettegen=stats_mode=diff[p];[b][p]paletteuse=dither=bayer:bayer_scale=5"),
                "-c:v".into(),
                "gif".into(),
            ]
        }
        (Format::Webm, _) => vec![
            "-vf".into(),
            format!("setpts=PTS-STARTPTS,{}", scale_filter("scale", q)),
            "-c:v".into(),
            "libvpx-vp9".into(),
            "-crf".into(),
            match q {
                Quality::Original => "31",
                Quality::High => "33",
                Quality::Small => "36",
            }
            .into(),
            "-b:v".into(),
            "0".into(),
            "-row-mt".into(),
            "1".into(),
            "-deadline".into(),
            "good".into(),
            "-cpu-used".into(),
            "4".into(),
            "-pix_fmt".into(),
            "yuv420p".into(),
        ],
        (Format::Mp4, AttemptKind::FullVaapi) => vec![
            "-vf".into(),
            format!("setpts=PTS-STARTPTS,{}", scale_filter("scale_vaapi", q)),
            "-c:v".into(),
            "h264_vaapi".into(),
            "-qp".into(),
            qp.into(),
        ],
        (Format::Mp4, AttemptKind::VaapiEncode) => vec![
            "-vf".into(),
            format!(
                "setpts=PTS-STARTPTS,{},format=nv12,hwupload",
                scale_filter("scale", q)
            ),
            "-c:v".into(),
            "h264_vaapi".into(),
            "-qp".into(),
            qp.into(),
        ],
        (Format::Mp4, AttemptKind::Software) => vec![
            "-vf".into(),
            format!("setpts=PTS-STARTPTS,{}", scale_filter("scale", q)),
            "-c:v".into(),
            "libx264".into(),
            "-crf".into(),
            crf.into(),
            "-preset".into(),
            "medium".into(),
        ],
    }
}
fn audio_args(format: Format) -> Vec<String> {
    match format {
        Format::Mp4 => vec!["-c:a", "aac", "-b:a", "192k", "-af", "asetpts=PTS-STARTPTS"],
        Format::Webm => vec![
            "-c:a",
            "libopus",
            "-b:a",
            "128k",
            "-af",
            "asetpts=PTS-STARTPTS",
        ],
        Format::Gif => vec!["-an"],
        Format::Copy => vec![],
    }
    .into_iter()
    .map(String::from)
    .collect()
}
fn args(plan: &ExportPlan, kind: AttemptKind, node: Option<&Path>) -> Vec<String> {
    let mut a = vec!["-hide_banner", "-nostdin", "-y", "-loglevel", "warning"]
        .into_iter()
        .map(String::from)
        .collect::<Vec<_>>();
    if let Some(n) = node {
        match kind {
            AttemptKind::FullVaapi => a.extend(
                [
                    "-hwaccel",
                    "vaapi",
                    "-hwaccel_device",
                    &n.to_string_lossy(),
                    "-hwaccel_output_format",
                    "vaapi",
                ]
                .map(String::from),
            ),
            AttemptKind::VaapiEncode => a.extend(
                [
                    "-init_hw_device",
                    &format!("vaapi=va:{}", n.display()),
                    "-filter_hw_device",
                    "va",
                ]
                .map(String::from),
            ),
            AttemptKind::Software => {}
        }
    }
    let start = format!("{:.6}", plan.start as f64 / 1e6);
    let duration = format!("{:.6}", plan.duration as f64 / 1e6);
    let input = plan.input.to_string_lossy().into_owned();
    // Stream copy seeks on the input so the demuxer lands on the keyframe at
    // or before the start; an output-side seek would drop packets until the
    // next keyframe instead. Re-encoding seeks on the output for exact frames.
    if plan.format == Format::Copy {
        a.extend([
            "-ss".into(),
            start,
            "-i".into(),
            input,
            "-t".into(),
            duration,
        ]);
    } else {
        a.extend([
            "-i".into(),
            input,
            "-ss".into(),
            start,
            "-t".into(),
            duration,
        ]);
    }
    a.extend(["-map", "0:v:0"].map(String::from));
    if plan.format != Format::Gif {
        a.extend(["-map", "0:a?"].map(String::from));
    }
    a.extend(video_args(plan, kind));
    a.extend(audio_args(plan.format));
    let iso_bmff = plan
        .temp
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| {
            ["mp4", "m4v", "mov"]
                .iter()
                .any(|v| e.eq_ignore_ascii_case(v))
        });
    if iso_bmff {
        a.extend(["-movflags", "+faststart"].map(String::from));
    }
    if plan.format != Format::Gif {
        a.extend(["-avoid_negative_ts", "make_zero"].map(String::from));
    }
    a.extend(
        [
            "-progress",
            "pipe:1",
            "-nostats",
            &plan.temp.to_string_lossy(),
        ]
        .map(String::from),
    );
    a
}
async fn attempt(
    reporter: &mut Reporter<'_>,
    state: &ExportState,
    logs: &LogPaths,
    plan: &ExportPlan<'_>,
    kind: AttemptKind,
    node: Option<&Path>,
    first_step: &str,
) -> Result<(), String> {
    let temp = plan.temp;
    let duration = plan.duration;
    let label = kind.label(plan.format);
    let _ = fs::remove_file(temp);
    reporter.begin_attempt(label, first_step);
    let stderr = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&logs.ffmpeg)
        .map_err(|e| e.to_string())?;
    let mut ffmpeg_args = args(plan, kind, node);
    if logs.verbose {
        if let Some(level) = ffmpeg_args
            .iter_mut()
            .skip_while(|value| value.as_str() != "-loglevel")
            .nth(1)
        {
            *level = "verbose".into();
        }
    }
    let mut command = tokio::process::Command::new("ffmpeg");
    command
        .args(ffmpeg_args)
        .stdout(Stdio::piped())
        .stderr(Stdio::from(stderr));
    command.process_group(0);
    let mut child = command.spawn().map_err(|e| e.to_string())?;
    state
        .pid
        .store(child.id().unwrap_or(0) as i32, Ordering::SeqCst);
    let stdout = child
        .stdout
        .take()
        .ok_or("FFmpeg progress pipe unavailable")?;
    let mut lines = BufReader::new(stdout).lines();
    let mut block = ProgressBlock::default();
    let mut first = true;
    loop {
        tokio::select! {
            line = lines.next_line() => match line.map_err(|e| e.to_string())? {
                Some(line) => {
                    if block.feed(&line) {
                        if first && plan.format == Format::Copy {
                            reporter.step(STEP_COPYING);
                        }
                        first = false;
                        // `total_size` counts muxed bytes even while the file
                        // write is still buffered; the file size is the fallback.
                        let bytes = block
                            .total_size
                            .or_else(|| fs::metadata(temp).ok().map(|m| m.len()))
                            .unwrap_or(0);
                        reporter.progress(block.out_time_micros.unwrap_or(0), duration, bytes);
                    }
                }
                None => break,
            },
            _ = tokio::time::sleep(Duration::from_millis(100)) => {
                if state.cancel.load(Ordering::SeqCst) {
                    let pid = state.pid.load(Ordering::SeqCst);
                    if pid > 0 {
                        unsafe {
                            libc::kill(-pid, libc::SIGTERM);
                        }
                    }
                    let _ = child.wait().await;
                    let _ = fs::remove_file(temp);
                    return Err("cancelled".into());
                }
            }
        }
    }
    let status = child.wait().await.map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        let _ = fs::remove_file(temp);
        Err(format!("{label} exited with {status}"))
    }
}
fn records(kind: AttemptKind, format: Format, node: Option<&Path>) -> Vec<AccelerationRecord> {
    let device = node.map(|p| p.display().to_string());
    let software = |encoder: &str, reason: &str| {
        [
            ("export_decode", "FFmpeg software decoder"),
            ("export_encode", encoder),
        ]
        .into_iter()
        .map(|(c, i)| AccelerationRecord {
            component: c.into(),
            state: AccelerationState::Software,
            implementation: Some(i.into()),
            api: None,
            device: None,
            reason: Some(reason.into()),
        })
        .collect()
    };
    match (format, kind) {
        (Format::Copy, _) => vec![],
        (Format::Webm, _) => software("libvpx-vp9", "WebM export always uses software encoding"),
        (Format::Gif, _) => software("gif", "GIF export always uses software encoding"),
        (Format::Mp4, AttemptKind::Software) => {
            software("libx264", "VA-API attempts unavailable or failed")
        }
        (Format::Mp4, AttemptKind::FullVaapi) => vec![
            ("export_decode", "VA-API decoder"),
            ("export_encode", "h264_vaapi"),
        ]
        .into_iter()
        .map(|(c, i)| AccelerationRecord {
            component: c.into(),
            state: AccelerationState::Active,
            implementation: Some(i.into()),
            api: Some("VA-API".into()),
            device: device.clone(),
            reason: None,
        })
        .collect(),
        (Format::Mp4, AttemptKind::VaapiEncode) => vec![
            AccelerationRecord {
                component: "export_decode".into(),
                state: AccelerationState::Software,
                implementation: Some("FFmpeg software decoder".into()),
                api: None,
                device: None,
                reason: None,
            },
            AccelerationRecord {
                component: "export_encode".into(),
                state: AccelerationState::Active,
                implementation: Some("h264_vaapi".into()),
                api: Some("VA-API".into()),
                device,
                reason: None,
            },
        ],
    }
}
struct Exported {
    output: PathBuf,
    kind: AttemptKind,
    node: Option<PathBuf>,
    effective_start: Option<u64>,
}
async fn run_export(
    state: &ExportState,
    logs: &LogPaths,
    request: ExportRequest,
    sink: ProgressSink<'_>,
) -> Result<Exported, AppError> {
    if request.start_micros >= request.end_micros {
        return Err(AppError::InvalidTrim("start must be before end".into()));
    }
    let format = request.format;
    let input = media::validate_input(Path::new(&request.input))?;
    let output = destination(&input, Path::new(&request.output), format)?;
    let temp = tempfile::Builder::new()
        .prefix(".wodeo-")
        .suffix(&format!(".{}", format.output_extension(&input)))
        .tempfile_in(output.parent().unwrap())
        .map_err(|e| AppError::Destination(e.to_string()))?;
    let temp_path = temp.path().to_path_buf();
    drop(temp);
    let mut effective_start = Some(request.start_micros);
    if format == Format::Copy {
        effective_start = match media::keyframes(&input) {
            Ok(k) => media::keyframe_at_or_before(&k, request.start_micros),
            Err(e) => {
                tracing::warn!(reason=%e,"keyframe index unavailable; effective start known after export");
                None
            }
        };
    }
    let seek_from = effective_start.unwrap_or(request.start_micros);
    let estimate = match media::probe_any(&input) {
        Ok(source) => {
            // Stream copy writes from the keyframe, not the selected start.
            let span = if format == Format::Copy {
                request.end_micros.saturating_sub(seek_from)
            } else {
                request.end_micros - request.start_micros
            };
            estimate_bytes(format, request.quality, &source, span)
        }
        Err(e) => {
            tracing::warn!(reason=%e,"source probe failed; export size estimate unavailable");
            None
        }
    };
    let mut reporter = Reporter::new(sink, estimate);
    let plan = ExportPlan {
        format,
        quality: request.quality,
        input: &input,
        temp: &temp_path,
        start: request.start_micros,
        duration: request.end_micros - request.start_micros,
    };
    let first_step = if format == Format::Copy {
        format!("Seek to keyframe at {}", clock(seek_from))
    } else {
        STEP_ENCODING.to_owned()
    };
    let nodes = render_nodes();
    let mut selected = None;
    let mut failures = vec![];
    for kind in attempts(format) {
        if kind != AttemptKind::Software && nodes.is_empty() {
            continue;
        }
        let node = if kind == AttemptKind::Software {
            None
        } else {
            nodes.first().map(PathBuf::as_path)
        };
        tracing::info!(attempt=kind.label(format),format=?format,quality=?request.quality,device=?node,"starting export attempt");
        match attempt(&mut reporter, state, logs, &plan, kind, node, &first_step).await {
            Ok(()) => {
                selected = Some((kind, node.map(Path::to_path_buf)));
                break;
            }
            Err(e) if e == "cancelled" => return Err(AppError::Cancelled),
            Err(e) => {
                tracing::warn!(attempt=kind.label(format),reason=%e,"export attempt failed; trying fallback");
                failures.push(e)
            }
        }
    }
    let (kind, node) = selected.ok_or_else(|| {
        AppError::Export(format!(
            "{}. Detailed diagnostics: {}",
            failures.join("; "),
            logs.ffmpeg.display()
        ))
    })?;
    // Copy has no separate validation step: probing a stream copy is part of
    // finalizing it.
    reporter.step(if format == Format::Copy {
        STEP_FINALIZING
    } else {
        STEP_VALIDATING
    });
    let validated = media::probe_any(&temp_path).map_err(|e| {
        let _ = fs::remove_file(&temp_path);
        AppError::Export(format!("output validation failed: {e}"))
    })?;
    if format == Format::Copy && effective_start.is_none() {
        effective_start = Some(request.end_micros.saturating_sub(validated.duration_micros));
    } else if format != Format::Copy {
        reporter.step(STEP_FINALIZING);
    }
    OpenOptions::new()
        .write(true)
        .open(&temp_path)?
        .sync_all()?;
    fs::rename(&temp_path, &output).map_err(|e| {
        let _ = fs::remove_file(&temp_path);
        AppError::Destination(format!("could not finalize output: {e}"))
    })?;
    Ok(Exported {
        output: output.canonicalize()?,
        kind,
        node,
        effective_start,
    })
}
#[tauri::command]
pub async fn start_export(
    app: tauri::AppHandle,
    state: tauri::State<'_, ExportState>,
    logs: tauri::State<'_, LogPaths>,
    request: ExportRequest,
) -> Result<ExportResult, AppError> {
    if state.active.swap(true, Ordering::SeqCst) {
        return Err(AppError::ExportBusy);
    }
    let _guard = ActiveGuard(&state);
    let format = request.format;
    let emitter = app.clone();
    let Exported {
        output: canonical,
        kind,
        node,
        effective_start,
    } = run_export(
        &state,
        &logs,
        request,
        Box::new(move |p| {
            let _ = emitter.emit("export-progress", p);
        }),
    )
    .await?;
    let acceleration = records(kind, format, node.as_deref());
    let _ = app.emit("acceleration-update", &acceleration);
    tracing::info!(output=%canonical.display(),attempt=kind.label(format),effective_start_micros=?effective_start,"export completed");
    crate::lifecycle::write_success(&canonical)?;
    app.state::<crate::app::LaunchState>()
        .succeeded
        .store(true, Ordering::SeqCst);
    Ok(ExportResult {
        output: canonical.display().to_string(),
        acceleration,
        effective_start_micros: effective_start,
    })
}
#[tauri::command]
pub fn cancel_export(state: tauri::State<'_, ExportState>) -> Result<(), AppError> {
    if state.active.load(Ordering::SeqCst) {
        state.cancel.store(true, Ordering::SeqCst)
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn plan<'a>(
        format: Format,
        quality: Quality,
        input: &'a Path,
        temp: &'a Path,
        start: u64,
        duration: u64,
    ) -> ExportPlan<'a> {
        ExportPlan {
            format,
            quality,
            input,
            temp,
            start,
            duration,
        }
    }
    fn generate(path: &Path, args: &[&str]) {
        let status = std::process::Command::new("ffmpeg")
            .args(["-hide_banner", "-loglevel", "error"])
            .args(args)
            .args(["-y"])
            .arg(path)
            .status()
            .unwrap();
        assert!(status.success(), "failed to generate {}", path.display());
    }
    fn run(plan: &ExportPlan) {
        let status = std::process::Command::new("ffmpeg")
            .args(args(plan, AttemptKind::Software, None))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap();
        assert!(
            status.success(),
            "export failed: {:?}",
            args(plan, AttemptKind::Software, None)
        );
    }
    fn stream_codecs(path: &Path) -> Vec<String> {
        let out = std::process::Command::new("ffprobe")
            .args([
                "-v",
                "error",
                "-show_entries",
                "stream=codec_type,codec_name",
                "-of",
                "csv=p=0",
            ])
            .arg(path)
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(str::to_owned)
            .collect()
    }
    #[test]
    fn destination_requires_format_extension_and_refuses_source() {
        let d = tempfile::tempdir().unwrap();
        let input = d.path().join("in.mp4");
        fs::write(&input, b"x").unwrap();
        for (format, good, bad) in [
            (Format::Mp4, "out.mp4", "out.mov"),
            (Format::Copy, "out.MP4", "out.webm"),
            (Format::Webm, "out.webm", "out.mp4"),
            (Format::Gif, "out.gif", "out"),
        ] {
            assert!(destination(&input, &d.path().join(good), format).is_ok());
            let e = destination(&input, &d.path().join(bad), format).unwrap_err();
            assert!(
                e.to_string().contains(&format!(".{}", format.extension())),
                "{e}"
            );
        }
        let mkv = d.path().join("in.mkv");
        fs::write(&mkv, b"x").unwrap();
        assert!(destination(&mkv, &d.path().join("out.MKV"), Format::Copy).is_ok());
        let e = destination(&mkv, &d.path().join("out.mp4"), Format::Copy).unwrap_err();
        assert!(e.to_string().contains(".mkv"), "{e}");
        let existing = d.path().join("existing.mp4");
        fs::write(&existing, b"old").unwrap();
        assert_eq!(
            destination(&input, &existing, Format::Mp4).unwrap(),
            existing
        );
        let e = destination(&input, &input, Format::Mp4).unwrap_err();
        assert!(e.to_string().contains("same file"));
    }
    #[test]
    fn ffmpeg_args_are_discrete_and_exact() {
        let p = plan(
            Format::Mp4,
            Quality::Original,
            Path::new("a;echo.mp4"),
            Path::new("out.mp4"),
            1_000_000,
            2_000_000,
        );
        let a = args(&p, AttemptKind::Software, None);
        assert!(a.contains(&"a;echo.mp4".into()));
        assert!(a.contains(&"libx264".into()));
        assert!(a.contains(&"18".into()));
        assert!(!a.contains(&"copy".into()));
        let input = a.iter().position(|v| v == "-i").unwrap();
        assert!(a.iter().position(|v| v == "-ss").unwrap() > input);
        let copy = args(
            &plan(
                Format::Copy,
                Quality::Small,
                p.input,
                p.temp,
                1_000_000,
                2_000_000,
            ),
            AttemptKind::Software,
            None,
        );
        assert!(copy.iter().position(|v| v == "-ss") < copy.iter().position(|v| v == "-i"));
        assert!(copy.contains(&"copy".into()));
        assert!(!copy.iter().any(|v| v == "-vf" || v.contains("scale")));
    }
    #[test]
    fn attempts_and_records_follow_format() {
        assert_eq!(
            attempts(Format::Mp4),
            vec![
                AttemptKind::FullVaapi,
                AttemptKind::VaapiEncode,
                AttemptKind::Software
            ]
        );
        for format in [Format::Webm, Format::Gif, Format::Copy] {
            assert_eq!(attempts(format), vec![AttemptKind::Software]);
            let a = args(
                &plan(
                    format,
                    Quality::Original,
                    Path::new("in.mp4"),
                    Path::new("t"),
                    0,
                    1,
                ),
                AttemptKind::Software,
                None,
            );
            assert!(!a.iter().any(|v| v.contains("vaapi")));
        }
        assert!(records(AttemptKind::Software, Format::Copy, None).is_empty());
        let webm = records(AttemptKind::Software, Format::Webm, None);
        assert!(webm.iter().all(|r| r.state == AccelerationState::Software));
        assert_eq!(webm[1].implementation.as_deref(), Some("libvpx-vp9"));
        let gif = records(AttemptKind::Software, Format::Gif, None);
        assert_eq!(gif[1].implementation.as_deref(), Some("gif"));
    }
    #[test]
    fn software_attempt_produces_exact_silent_interval() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("source.mp4");
        let output = dir.path().join("trim.mp4");
        generate(
            &input,
            &[
                "-f",
                "lavfi",
                "-i",
                "testsrc2=s=320x240:r=30:d=3",
                "-an",
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
            ],
        );
        run(&plan(
            Format::Mp4,
            Quality::Original,
            &input,
            &output,
            1_000_000,
            1_000_000,
        ));
        let result = media::probe(&output).unwrap();
        assert!(!result.has_audio);
        assert!((result.duration_micros as i64 - 1_000_000).abs() <= 34_000);
    }
    #[test]
    fn progress_fallback_overwrite_validation_and_cleanup_are_transactional() {
        let software = records(AttemptKind::Software, Format::Mp4, None);
        assert!(software
            .iter()
            .all(|r| r.state == AccelerationState::Software));
        assert_eq!(software[1].implementation.as_deref(), Some("libx264"));
        let hardware = records(
            AttemptKind::FullVaapi,
            Format::Mp4,
            Some(Path::new("/dev/dri/renderD128")),
        );
        assert!(hardware
            .iter()
            .all(|r| r.state == AccelerationState::Active));
        assert_eq!(hardware[0].device.as_deref(), Some("/dev/dri/renderD128"));
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("input.mp4");
        let output = dir.path().join("output.mp4");
        fs::write(&input, b"source").unwrap();
        fs::write(&output, b"existing").unwrap();
        assert_eq!(destination(&input, &output, Format::Mp4).unwrap(), output);
        let partial = dir.path().join(".partial.mp4");
        fs::write(&partial, b"invalid").unwrap();
        assert!(media::probe_any(&partial).is_err());
        fs::remove_file(&partial).unwrap();
        assert!(!partial.exists());
        assert_eq!(fs::read(&output).unwrap(), b"existing");
    }
    #[test]
    fn mp4_quality_tiers_cap_resolution_without_upscaling() {
        let dir = tempfile::tempdir().unwrap();
        let landscape = dir.path().join("landscape.mp4");
        let portrait = dir.path().join("portrait.mp4");
        for (path, size) in [(&landscape, "1920x1080"), (&portrait, "1080x1920")] {
            generate(
                path,
                &[
                    "-f",
                    "lavfi",
                    "-i",
                    &format!("testsrc2=s={size}:r=10:d=0.5"),
                    "-an",
                    "-c:v",
                    "libx264",
                    "-preset",
                    "ultrafast",
                    "-pix_fmt",
                    "yuv420p",
                ],
            );
        }
        for (input, quality, expected) in [
            (&landscape, Quality::Small, (1280, 720)),
            (&landscape, Quality::High, (1920, 1080)),
            (&portrait, Quality::Small, (720, 1280)),
        ] {
            let output = dir.path().join(format!(
                "{quality:?}-{}",
                input.file_name().unwrap().to_string_lossy()
            ));
            let a = args(
                &plan(Format::Mp4, quality, input, &output, 0, 500_000),
                AttemptKind::Software,
                None,
            );
            assert!(a.contains(
                &if quality == Quality::Small {
                    "28"
                } else {
                    "23"
                }
                .into()
            ));
            run(&plan(Format::Mp4, quality, input, &output, 0, 500_000));
            let probed = media::probe_any(&output).unwrap();
            assert_eq!((probed.width, probed.height), expected, "{quality:?}");
        }
    }
    #[test]
    fn webm_export_is_vp9_opus_with_exact_duration() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("source.mp4");
        let output = dir.path().join("trim.webm");
        generate(
            &input,
            &[
                "-f",
                "lavfi",
                "-i",
                "testsrc2=s=320x240:r=30:d=3",
                "-f",
                "lavfi",
                "-i",
                "sine=d=3",
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
                "-c:a",
                "aac",
                "-shortest",
            ],
        );
        run(&plan(
            Format::Webm,
            Quality::Original,
            &input,
            &output,
            1_000_000,
            1_000_000,
        ));
        let probed = media::probe_any(&output).unwrap();
        assert_eq!(probed.codec, "vp9");
        assert!(probed.has_audio);
        assert!(stream_codecs(&output).contains(&"opus,audio".to_owned()));
        let video_duration = std::process::Command::new("ffprobe")
            .args([
                "-v",
                "error",
                "-select_streams",
                "v:0",
                "-count_frames",
                "-show_entries",
                "stream=nb_read_frames",
                "-of",
                "csv=p=0",
            ])
            .arg(&output)
            .output()
            .unwrap();
        let frames: i64 = String::from_utf8_lossy(&video_duration.stdout)
            .trim()
            .parse()
            .unwrap();
        assert!((frames - 30).abs() <= 1, "{frames} frames");
    }
    #[test]
    fn gif_export_has_no_audio_and_tier_width() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("source.mp4");
        generate(
            &input,
            &[
                "-f",
                "lavfi",
                "-i",
                "testsrc2=s=1280x720:r=30:d=2",
                "-f",
                "lavfi",
                "-i",
                "sine=d=2",
                "-c:v",
                "libx264",
                "-preset",
                "ultrafast",
                "-pix_fmt",
                "yuv420p",
                "-c:a",
                "aac",
                "-shortest",
            ],
        );
        for (quality, width) in [
            (Quality::Small, 480),
            (Quality::High, 720),
            (Quality::Original, 1280),
        ] {
            let output = dir.path().join(format!("{quality:?}.gif"));
            run(&plan(
                Format::Gif,
                quality,
                &input,
                &output,
                500_000,
                1_000_000,
            ));
            let probed = media::probe_any(&output).unwrap();
            assert_eq!(probed.codec, "gif");
            assert!(!probed.has_audio);
            assert_eq!(probed.width, width, "{quality:?}");
            assert!(
                (probed.duration_micros as i64 - 1_000_000).abs() <= 100_000,
                "{}",
                probed.duration_micros
            );
        }
    }
    fn format_name(path: &Path) -> String {
        let out = std::process::Command::new("ffprobe")
            .args([
                "-v",
                "error",
                "-show_entries",
                "format=format_name",
                "-of",
                "csv=p=0",
            ])
            .arg(path)
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    }
    fn gop_source(path: &Path, codec: &[&str]) {
        let mut a = vec![
            "-f",
            "lavfi",
            "-i",
            "testsrc2=s=320x240:r=30:d=6",
            "-g",
            "60",
            "-keyint_min",
            "60",
        ];
        a.extend_from_slice(codec);
        generate(path, &a);
    }
    #[test]
    fn mkv_reencodes_to_exact_mp4() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("src.mkv");
        let output = dir.path().join("out.mp4");
        gop_source(&input, &["-c:v", "libx264", "-pix_fmt", "yuv420p"]);
        run(&plan(
            Format::Mp4,
            Quality::Original,
            &input,
            &output,
            1_000_000,
            2_000_000,
        ));
        let probed = media::probe(&output).unwrap();
        assert!(format_name(&output).contains("mov"));
        assert!(
            probed.duration_micros.abs_diff(2_000_000) <= 34_000,
            "{}",
            probed.duration_micros
        );
    }
    #[test]
    fn copy_keeps_matroska_and_webm_containers() {
        let dir = tempfile::tempdir().unwrap();
        let mkv = dir.path().join("src.mkv");
        gop_source(
            &mkv,
            &[
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
                "-sc_threshold",
                "0",
            ],
        );
        let ext = Format::Copy.output_extension(&mkv);
        assert_eq!(ext, "mkv");
        let out = dir.path().join(format!("copy.{ext}"));
        let p = plan(
            Format::Copy,
            Quality::Original,
            &mkv,
            &out,
            2_500_000,
            2_000_000,
        );
        assert!(!args(&p, AttemptKind::Software, None)
            .iter()
            .any(|a| a == "+faststart"));
        run(&p);
        assert!(format_name(&out).contains("matroska"));
        let index = media::probe(&mkv).unwrap().keyframes_micros;
        assert_eq!(
            media::keyframe_at_or_before(&index, 2_500_000),
            Some(2_000_000)
        );
        let probed = media::probe(&out).unwrap();
        assert_eq!(probed.codec, "h264");
        // Same B-frame overshoot as MP4, plus Matroska counts the last
        // frame's display duration.
        assert!(
            (2_400_000..=2_700_000).contains(&probed.duration_micros),
            "{}",
            probed.duration_micros
        );
        assert!(probed
            .keyframes_micros
            .first()
            .is_some_and(|k| *k < 100_000));
        let webm = dir.path().join("src.webm");
        gop_source(
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
        let out = dir
            .path()
            .join(format!("copy.{}", Format::Copy.output_extension(&webm)));
        assert_eq!(out.extension().unwrap(), "webm");
        run(&plan(
            Format::Copy,
            Quality::Original,
            &webm,
            &out,
            2_000_000,
            2_000_000,
        ));
        assert!(format_name(&out).contains("webm"));
        assert_eq!(media::probe(&out).unwrap().codec, "vp9");
    }
    #[test]
    fn copy_export_starts_at_preceding_keyframe() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("gop.mp4");
        let output = dir.path().join("copy.mp4");
        generate(
            &input,
            &[
                "-f",
                "lavfi",
                "-i",
                "testsrc2=s=320x240:r=30:d=6",
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
            ],
        );
        let index = media::probe(&input).unwrap().keyframes_micros;
        let effective = media::keyframe_at_or_before(&index, 2_500_000);
        assert_eq!(effective, Some(2_000_000));
        run(&plan(
            Format::Copy,
            Quality::Small,
            &input,
            &output,
            2_500_000,
            2_000_000,
        ));
        let probed = media::probe(&output).unwrap();
        assert_eq!(probed.codec, "h264");
        assert_eq!(probed.width, 320);
        // Output spans the 2 s keyframe to the 4.5 s end. Stream copy cannot
        // drop the trailing anchor frame B-frames depend on, so the end may
        // overshoot by the encoder's reorder delay (two frames for x264).
        assert!(
            (2_500_000..=2_600_000).contains(&probed.duration_micros),
            "{}",
            probed.duration_micros
        );
        // First packet is the copied keyframe (shifted only by B-frame delay).
        assert!(probed
            .keyframes_micros
            .first()
            .is_some_and(|k| *k < 100_000));
    }
    #[test]
    fn cancellation_terminates_the_ffmpeg_process_group() {
        use std::os::unix::process::CommandExt;
        let mut command = std::process::Command::new("ffmpeg");
        command
            .args([
                "-hide_banner",
                "-loglevel",
                "quiet",
                "-re",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=s=64x64:r=30",
                "-f",
                "null",
                "-",
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        command.process_group(0);
        let mut child = command.spawn().unwrap();
        let pid = child.id() as i32;
        std::thread::sleep(std::time::Duration::from_millis(100));
        unsafe {
            libc::kill(-pid, libc::SIGTERM);
        }
        let status = child.wait().unwrap();
        assert!(!status.success());
    }
    #[test]
    fn representative_codec_cadence_and_dimension_matrix_exports() {
        fn make(path: &Path, source: &str, codec: &str, extra: &[&str]) {
            let mut command = std::process::Command::new("ffmpeg");
            command.args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                source,
                "-an",
                "-c:v",
                codec,
            ]);
            command.args(extra).args(["-y"]).arg(path);
            assert!(
                command.status().unwrap().success(),
                "failed to generate {}",
                path.display()
            );
        }
        let dir = tempfile::tempdir().unwrap();
        let h264 = dir.path().join("h264.mp4");
        let hevc = dir.path().join("hevc.mp4");
        let vfr = dir.path().join("vfr.mp4");
        let odd = dir.path().join("odd.mp4");
        make(&h264, "testsrc2=s=320x240:r=30:d=1", "libx264", &[]);
        make(
            &hevc,
            "testsrc2=s=320x240:r=30:d=1",
            "libx265",
            &["-x265-params", "pools=1:frame-threads=1"],
        );
        make(&odd, "testsrc=s=321x241:r=25:d=1", "libx264rgb", &[]);
        let vfr_status = std::process::Command::new("ffmpeg")
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=s=320x240:r=30:d=2",
                "-vf",
                r"select=not(mod(n\,3))+not(mod(n\,5))",
                "-fps_mode",
                "vfr",
                "-an",
                "-c:v",
                "libx264",
                "-y",
            ])
            .arg(&vfr)
            .status()
            .unwrap();
        assert!(vfr_status.success());
        for (index, input) in [&h264, &hevc, &vfr, &odd].into_iter().enumerate() {
            let inspected = media::probe(input).unwrap();
            let output = dir.path().join(format!("matrix-{index}.mp4"));
            let status = std::process::Command::new("ffmpeg")
                .args(args(
                    &plan(
                        Format::Mp4,
                        Quality::Original,
                        input,
                        &output,
                        0,
                        inspected.duration_micros,
                    ),
                    AttemptKind::Software,
                    None,
                ))
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .unwrap();
            assert!(status.success(), "failed to export {}", input.display());
            let exported = media::probe(&output).unwrap();
            assert_eq!(exported.codec, "h264");
            assert_eq!(exported.width % 2, 0);
            assert_eq!(exported.height % 2, 0);
        }
    }
    #[test]
    fn progress_blocks_prefer_total_size_and_skip_unavailable_values() {
        let mut block = ProgressBlock::default();
        let first = [
            "frame=0",
            "total_size=N/A",
            "out_time_us=N/A",
            "out_time=-577014:32:22.775808",
            "progress=continue",
        ];
        let ends: Vec<bool> = first.iter().map(|l| block.feed(l)).collect();
        assert_eq!(ends, [false, false, false, false, true]);
        assert_eq!((block.out_time_micros, block.total_size), (None, None));
        for line in [
            "frame=30",
            "bitrate=1024.0kbits/s",
            "total_size=131072",
            "out_time_us=1000000",
            "speed=2.0x",
        ] {
            assert!(!block.feed(line));
        }
        assert!(block.feed("progress=end"));
        assert_eq!(block.out_time_micros, Some(1_000_000));
        assert_eq!(block.total_size, Some(131_072));
        // A later N/A keeps the previous value.
        block.feed("total_size=N/A");
        assert_eq!(block.total_size, Some(131_072));
    }
    #[test]
    fn eta_is_smoothed_and_resets_with_each_attempt() {
        let mut eta = Eta::default();
        assert_eq!(eta.update(Duration::from_secs(1), 0.0), None);
        // 25% after 1 s: 3 s left.
        assert_eq!(eta.update(Duration::from_secs(1), 0.25), Some(3_000_000));
        // A jump to 1 s raw is damped by the moving average.
        let next = eta.update(Duration::from_secs(2), 2.0 / 3.0).unwrap();
        assert_eq!(next, 2_400_000);
        let mut sent = vec![];
        let mut reporter = Reporter::new(
            Box::new(|p: &ExportProgress| sent.push(p.clone())),
            Some((1000, true)),
        );
        reporter.begin_attempt("first", STEP_ENCODING);
        reporter.progress(500_000, 1_000_000, 400);
        assert!(reporter.eta.smoothed.is_some());
        reporter.begin_attempt("second", STEP_ENCODING);
        assert!(reporter.eta.smoothed.is_none());
        drop(reporter);
        assert_eq!(sent[1].fraction, 0.5);
        assert_eq!(sent[1].bytes_written, 400);
        assert!(sent[1].remaining_micros.is_some());
        assert_eq!(sent[2].attempt, "second");
        assert_eq!(
            (
                sent[2].fraction,
                sent[2].bytes_written,
                sent[2].remaining_micros
            ),
            (0.0, 0, None)
        );
        assert!(sent
            .iter()
            .all(|p| p.estimated_bytes == Some(1000) && p.approximate));
    }
    #[test]
    fn clock_matches_frontend_timecodes() {
        assert_eq!(clock(2_000_000), "0:02.000");
        assert_eq!(clock(100_338_400), "1:40.338");
        assert_eq!(clock(3_725_000_000), "1:02:05.000");
    }
    #[test]
    fn size_estimates_follow_format() {
        let source = media::VideoMetadata {
            path: String::new(),
            preview_url: String::new(),
            duration_micros: 10_000_000,
            width: 1920,
            height: 1080,
            codec: "h264".into(),
            frame_rate: 30.0,
            has_audio: true,
            audio_codec: Some("aac".into()),
            thumbnails: vec![],
            thumbnail_warning: None,
            playback_acceleration: vec![],
            keyframes_micros: vec![],
            bit_rate: Some(4_000_000),
        };
        let est = |f, q| estimate_bytes(f, q, &source, 2_000_000);
        assert_eq!(est(Format::Copy, Quality::Small), Some((1_000_000, false)));
        assert_eq!(
            est(Format::Mp4, Quality::High),
            Some(((5_192_000.0 * 2.0 / 8.0) as u64, true))
        );
        assert_eq!(
            est(Format::Webm, Quality::Small),
            Some(((2_128_000.0 * 2.0 / 8.0) as u64, true))
        );
        // 480x270 at 10 fps for 2 s, one bit per pixel.
        assert_eq!(
            est(Format::Gif, Quality::Small),
            Some((480 * 270 * 10 * 2 / 8, true))
        );
        let unknown = media::VideoMetadata {
            bit_rate: None,
            ..source
        };
        assert_eq!(
            estimate_bytes(Format::Copy, Quality::Small, &unknown, 1),
            None
        );
    }
    #[test]
    fn copy_estimate_matches_generated_bitrate() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("cbr.mp4");
        let output = dir.path().join("copy.mp4");
        generate(
            &input,
            &[
                "-f",
                "lavfi",
                "-i",
                "testsrc2=s=320x240:r=30:d=4",
                "-an",
                "-c:v",
                "libx264",
                "-g",
                "30",
                "-b:v",
                "1M",
                "-minrate",
                "1M",
                "-maxrate",
                "1M",
                "-bufsize",
                "500k",
                "-x264-params",
                "nal-hrd=cbr",
                "-pix_fmt",
                "yuv420p",
            ],
        );
        let source = media::probe_any(&input).unwrap();
        let bit_rate = source.bit_rate.unwrap();
        assert!(
            (800_000..=1_200_000).contains(&bit_rate),
            "{bit_rate} bits/s"
        );
        let (estimate, approximate) =
            estimate_bytes(Format::Copy, Quality::Original, &source, 2_000_000).unwrap();
        assert!(!approximate);
        assert_eq!(estimate, (bit_rate as f64 * 2.0 / 8.0).round() as u64);
        run(&plan(
            Format::Copy,
            Quality::Original,
            &input,
            &output,
            1_000_000,
            2_000_000,
        ));
        let written = fs::metadata(&output).unwrap().len() as f64;
        let ratio = written / estimate as f64;
        assert!((0.7..=1.3).contains(&ratio), "{written} vs {estimate}");
    }
    fn logs(dir: &Path) -> LogPaths {
        LogPaths {
            state_dir: dir.into(),
            application: dir.join("app.log"),
            ffmpeg: dir.join("ffmpeg.log"),
            gstreamer: dir.join("gst.log"),
            verbose: false,
        }
    }
    async fn collect_steps(dir: &Path, input: &Path, format: Format) -> Vec<ExportProgress> {
        let state = ExportState::default();
        let mut events = vec![];
        let request = ExportRequest {
            input: input.display().to_string(),
            output: dir
                .join(format!("out-{format:?}.{}", format.extension()))
                .display()
                .to_string(),
            start_micros: 1_000_000,
            end_micros: 3_000_000,
            format,
            quality: Quality::Small,
        };
        run_export(
            &state,
            &logs(dir),
            request,
            Box::new(|p: &ExportProgress| events.push(p.clone())),
        )
        .await
        .unwrap();
        events
    }
    fn distinct_steps(events: &[ExportProgress]) -> Vec<String> {
        let mut steps: Vec<String> = events.iter().map(|p| p.step.clone()).collect();
        // VA-API fallbacks repeat the first step once per attempt.
        steps.dedup();
        steps
    }
    #[tokio::test]
    async fn export_emits_step_sequence_for_copy_and_mp4() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("source.mp4");
        generate(
            &input,
            &[
                "-f",
                "lavfi",
                "-i",
                "testsrc2=s=320x240:r=30:d=4",
                "-f",
                "lavfi",
                "-i",
                "sine=d=4",
                "-c:v",
                "libx264",
                "-g",
                "30",
                "-pix_fmt",
                "yuv420p",
                "-c:a",
                "aac",
                "-shortest",
            ],
        );
        let copy = collect_steps(dir.path(), &input, Format::Copy).await;
        assert_eq!(
            distinct_steps(&copy),
            [
                "Seek to keyframe at 0:01.000",
                STEP_COPYING,
                STEP_FINALIZING
            ]
        );
        // The estimate is known before FFmpeg reports anything.
        assert!(copy[0].estimated_bytes.is_some_and(|b| b > 0));
        assert!(!copy[0].approximate);
        assert_eq!(copy[0].fraction, 0.0);
        let last = copy.last().unwrap();
        assert!(last.bytes_written > 0);
        let mp4 = collect_steps(dir.path(), &input, Format::Mp4).await;
        assert_eq!(
            distinct_steps(&mp4),
            [STEP_ENCODING, STEP_VALIDATING, STEP_FINALIZING]
        );
        assert!(mp4.iter().all(|p| p.approximate));
        // Step events keep the fraction reached by the last progress event.
        let reached = mp4[mp4.len() - 3].fraction;
        assert!(reached > 0.9, "{reached}");
        assert_eq!(mp4[mp4.len() - 1].fraction, reached);
    }
}
