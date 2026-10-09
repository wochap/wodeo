use directories::BaseDirs;
use std::{
    fs,
    path::{Path, PathBuf},
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
pub struct LogPaths {
    pub state_dir: PathBuf,
    pub application: PathBuf,
    pub ffmpeg: PathBuf,
    pub gstreamer: PathBuf,
    pub verbose: bool,
}
pub fn init(
    verbose: bool,
) -> Result<(LogPaths, tracing_appender::non_blocking::WorkerGuard), String> {
    let base = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| BaseDirs::new().map(|b| b.home_dir().join(".local/state")))
        .ok_or("cannot resolve state directory")?;
    let dir = base.join("wodeo");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let app = dir.join("wodeo.log");
    let ffmpeg = dir.join("ffmpeg.log");
    remove_stale_traces(&dir);
    let gst = trace_path(&dir, std::process::id());
    std::env::set_var("GST_DEBUG", gst_debug(verbose));
    std::env::set_var("GST_DEBUG_FILE", &gst);
    let rank = feature_rank(std::env::var("GST_PLUGIN_FEATURE_RANK").ok().as_deref());
    std::env::set_var("GST_PLUGIN_FEATURE_RANK", &rank);
    if verbose {
        std::env::set_var("WEBKIT_DEBUG", "Media")
    }
    let file = tracing_appender::rolling::never(&dir, "wodeo.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file);
    let filter = if verbose { "wodeo=debug" } else { "wodeo=info" };
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(filter))
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(std::io::stderr)
                .without_time()
                .with_target(false),
        )
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(non_blocking)
                .with_ansi(false),
        )
        .try_init()
        .map_err(|e| e.to_string())?;
    tracing::debug!(rank = %rank, "GST_PLUGIN_FEATURE_RANK");
    Ok((
        LogPaths {
            state_dir: dir,
            application: app,
            ffmpeg,
            gstreamer: gst,
            verbose,
        },
        guard,
    ))
}
// GStreamer truncates GST_DEBUG_FILE on open, so each instance gets its own
// trace. GST_ELEMENT_FACTORY:4 logs one "creating element" line per element,
// which is enough to detect the playback decoder in normal runs.
fn trace_path(dir: &Path, pid: u32) -> PathBuf {
    dir.join(format!("gstreamer-{pid}.log"))
}
fn gst_debug(verbose: bool) -> &'static str {
    if verbose {
        "2,webkit*:4,va*:4,dmabuf*:4,GST_ELEMENT_FACTORY:4"
    } else {
        "GST_ELEMENT_FACTORY:4"
    }
}
// VA-API and NVDEC decoders share rank primary+1 and decodebin breaks the tie
// by name, so NVDEC wins on hybrid machines. One step above primary+1 makes
// VA-API win while NVDEC stays the next candidate. A user value is appended
// so its entries win for features both name.
const VA_RANK: u32 = 258;
const VA_CODECS: &[&str] = &["h264", "h265", "av1", "vp9", "vp8", "mpeg2"];
fn feature_rank(existing: Option<&str>) -> String {
    let mut entries: Vec<String> = VA_CODECS
        .iter()
        .flat_map(|c| {
            std::iter::once(format!("va{c}dec:{VA_RANK}"))
                .chain((128..=135).map(move |n| format!("varenderD{n}{c}dec:{VA_RANK}")))
        })
        .collect();
    if let Some(user) = existing.filter(|s| !s.is_empty()) {
        entries.push(user.into())
    }
    entries.join(",")
}
fn trace_pid(name: &str) -> Option<u32> {
    name.strip_prefix("gstreamer-")?
        .strip_suffix(".log")?
        .parse()
        .ok()
}
fn remove_stale_traces_with(dir: &Path, running: impl Fn(u32) -> bool) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        if let Some(pid) = trace_pid(&name.to_string_lossy()) {
            if !running(pid) {
                let _ = fs::remove_file(entry.path());
            }
        }
    }
}
fn remove_stale_traces(dir: &Path) {
    remove_stale_traces_with(dir, |pid| Path::new("/proc").join(pid.to_string()).exists())
}
impl LogPaths {
    /// Removes this instance's GStreamer trace; call on normal exit.
    pub fn remove_trace(&self) {
        let _ = fs::remove_file(&self.gstreamer);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn element_factory_trace_is_enabled_in_both_modes() {
        assert_eq!(gst_debug(false), "GST_ELEMENT_FACTORY:4");
        assert_eq!(
            gst_debug(true),
            "2,webkit*:4,va*:4,dmabuf*:4,GST_ELEMENT_FACTORY:4"
        );
        assert_eq!(
            trace_path(Path::new("/state"), 42),
            PathBuf::from("/state/gstreamer-42.log")
        )
    }
    #[test]
    fn feature_rank_prefers_va_and_keeps_user_value_last() {
        let rank = feature_rank(None);
        let entries: Vec<_> = rank.split(',').collect();
        assert!(entries.contains(&"vah264dec:258"));
        assert!(entries.contains(&"varenderD129h264dec:258"));
        assert!(!entries.iter().any(|e| e.starts_with("nv")));
        let user = "vah264dec:MAX,nvh264dec:NONE";
        let rank = feature_rank(Some(user));
        assert!(rank.ends_with(&format!(",{user}")));
        assert!(rank.starts_with("vah264dec:258,"))
    }
    #[test]
    fn stale_traces_of_dead_instances_are_removed() {
        let dir = tempfile::tempdir().unwrap();
        for name in [
            "gstreamer-1.log",
            "gstreamer-2.log",
            "gstreamer.log",
            "wodeo.log",
        ] {
            fs::write(dir.path().join(name), "x").unwrap();
        }
        remove_stale_traces_with(dir.path(), |pid| pid == 1);
        let mut left: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        left.sort();
        assert_eq!(left, ["gstreamer-1.log", "gstreamer.log", "wodeo.log"])
    }
    #[test]
    fn instance_trace_is_removed_on_exit() {
        let dir = tempfile::tempdir().unwrap();
        let gst = trace_path(dir.path(), 7);
        fs::write(&gst, "x").unwrap();
        let paths = LogPaths {
            state_dir: dir.path().into(),
            application: dir.path().join("wodeo.log"),
            ffmpeg: dir.path().join("ffmpeg.log"),
            gstreamer: gst.clone(),
            verbose: false,
        };
        paths.remove_trace();
        assert!(!gst.exists())
    }
}
