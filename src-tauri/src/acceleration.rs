use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::Mutex};
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AccelerationState {
    Available,
    Active,
    Software,
    Failed,
    Unknown,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccelerationRecord {
    pub component: String,
    pub state: AccelerationState,
    pub implementation: Option<String>,
    pub api: Option<String>,
    pub device: Option<String>,
    pub reason: Option<String>,
}
const HW_CODECS: &[&str] = &["h264", "h265", "av1", "vp9", "vp8", "mpeg2", "jpeg"];
fn is_hw_codec(s: &str) -> bool {
    HW_CODECS.contains(&s)
}
/// Hardware API of a VA-API (`va[renderD<N>]<codec>dec`, `vaapi<codec>dec`)
/// or NVDEC (`nv<codec>[sl]dec`) decoder factory.
fn hardware_api(name: &str) -> Option<&'static str> {
    let core = name.strip_suffix("dec")?;
    if let Some(codec) = core.strip_prefix("vaapi") {
        return is_hw_codec(codec).then_some("VA-API");
    }
    if let Some(rest) = core.strip_prefix("va") {
        let codec = match rest.strip_prefix("renderD") {
            Some(r) => {
                let digits = r.bytes().take_while(u8::is_ascii_digit).count();
                if digits == 0 {
                    return None;
                }
                &r[digits..]
            }
            None => rest,
        };
        return is_hw_codec(codec).then_some("VA-API");
    }
    let codec = core.strip_prefix("nv")?;
    let codec = codec.strip_suffix("sl").unwrap_or(codec);
    is_hw_codec(codec).then_some("NVDEC")
}
pub fn classify_decoder(name: &str) -> AccelerationState {
    let n = name.to_ascii_lowercase();
    if hardware_api(&n).is_some() || hardware_api(name).is_some() {
        AccelerationState::Active
    } else if n.starts_with("avdec_") || n.contains("openh264dec") {
        AccelerationState::Software
    } else {
        AccelerationState::Unknown
    }
}
pub fn render_nodes() -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir("/dev/dri") else {
        return vec![];
    };
    entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with("renderD"))
        })
        .collect()
}
pub fn unknown_playback() -> Vec<AccelerationRecord> {
    vec![
        AccelerationRecord {
            component: "playback_decode".into(),
            state: AccelerationState::Unknown,
            implementation: None,
            api: None,
            device: None,
            reason: Some("WebKitGTK has not exposed decoder evidence yet".into()),
        },
        AccelerationRecord {
            component: "playback_render".into(),
            state: AccelerationState::Unknown,
            implementation: None,
            api: None,
            device: None,
            reason: Some("Rendering path has not been observed".into()),
        },
    ]
}
const FACTORIES: &[&str] = &[
    "vah264dec",
    "vaapih264dec",
    "vavp9dec",
    "vaav1dec",
    "avdec_h264",
    "avdec_h265",
    "openh264dec",
];
const VIDEO_CODECS: &[&str] = &[
    "h264", "h265", "hevc", "vp8", "vp9", "av1", "mpeg2", "mpeg4", "theora",
];
// Unknown factories count only when they look like a video decoder, so audio
// decoders such as avdec_aac created after the video decoder are ignored.
fn is_video_decoder(name: &str) -> bool {
    FACTORIES.contains(&name)
        || hardware_api(name).is_some()
        || ((name.ends_with("dec") || name.starts_with("avdec_"))
            && VIDEO_CODECS.iter().any(|c| name.contains(c)))
}
/// Last decoder the pipeline created, from GST_ELEMENT_FACTORY
/// `creating element "<factory>"` lines.
fn last_created_decoder(text: &str) -> Option<&str> {
    const MARK: &str = "creating element \"";
    text.match_indices(MARK)
        .filter_map(|(i, _)| {
            let rest = &text[i + MARK.len()..];
            rest.find('"').map(|end| &rest[..end])
        })
        .filter(|name| is_video_decoder(name))
        .last()
}
pub fn playback_from_diagnostics(text: &str) -> Vec<AccelerationRecord> {
    let decoder = last_created_decoder(text).or_else(|| {
        FACTORIES
            .iter()
            .rev()
            .find(|name| text.contains(**name))
            .copied()
    });
    let decode = decoder.map_or_else(
        || unknown_playback()[0].clone(),
        |name| {
            let state = classify_decoder(name);
            AccelerationRecord {
                component: "playback_decode".into(),
                api: hardware_api(name).map(Into::into),
                reason: (state == AccelerationState::Unknown)
                    .then(|| "Unrecognized decoder factory".into()),
                state,
                implementation: Some(name.into()),
                device: None,
            }
        },
    );
    let render = if text.to_ascii_lowercase().contains("dmabuf") {
        AccelerationRecord {
            component: "playback_render".into(),
            state: AccelerationState::Active,
            implementation: Some("DMA-BUF".into()),
            api: Some("DMA-BUF".into()),
            device: None,
            reason: None,
        }
    } else if text.contains("\"webkitglvideosink\"") {
        AccelerationRecord {
            component: "playback_render".into(),
            state: AccelerationState::Active,
            implementation: Some("OpenGL (webkitglvideosink)".into()),
            api: Some("OpenGL".into()),
            device: None,
            reason: None,
        }
    } else {
        unknown_playback()[1].clone()
    };
    vec![decode, render]
}
#[tauri::command]
pub fn playback_acceleration(
    logs: tauri::State<'_, crate::logging::LogPaths>,
) -> Vec<AccelerationRecord> {
    let records = std::fs::read_to_string(&logs.gstreamer)
        .map(|s| playback_from_diagnostics(&s))
        .unwrap_or_else(|_| unknown_playback());
    log_decoder_change(&records[0]);
    records
}
fn log_decoder_change(decode: &AccelerationRecord) {
    static LAST: Mutex<Option<String>> = Mutex::new(None);
    let Some(name) = decode.implementation.as_deref() else {
        return;
    };
    let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
    if last.as_deref() != Some(name) {
        tracing::info!(decoder = name, state = ?decode.state, "playback decoder selected");
        *last = Some(name.into())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn decoder_classification_is_conservative() {
        assert_eq!(classify_decoder("vah264dec"), AccelerationState::Active);
        assert_eq!(classify_decoder("avdec_h264"), AccelerationState::Software);
        assert_eq!(classify_decoder("futuredec"), AccelerationState::Unknown);
        assert_eq!(classify_decoder("nvh265sldec"), AccelerationState::Active);
        assert_eq!(classify_decoder("vaapih264dec"), AccelerationState::Active);
        assert_eq!(
            classify_decoder("varenderDh264dec"),
            AccelerationState::Unknown
        )
    }
    fn created(names: &[&str]) -> String {
        names
            .iter()
            .map(|n| format!("INFO GST_ELEMENT_FACTORY creating element \"{n}\" named \"{n}0\"\n"))
            .collect()
    }
    #[test]
    fn nvdec_is_active_without_device() {
        let r = playback_from_diagnostics(&created(&["nvh264dec"]));
        assert_eq!(r[0].state, AccelerationState::Active);
        assert_eq!(r[0].api.as_deref(), Some("NVDEC"));
        assert_eq!(r[0].device, None);
        let r = playback_from_diagnostics(&created(&["nvh265sldec"]));
        assert_eq!(r[0].state, AccelerationState::Active);
        assert_eq!(r[0].api.as_deref(), Some("NVDEC"))
    }
    #[test]
    fn multi_device_va_decoder_is_detected_and_last_wins() {
        let r = playback_from_diagnostics(&created(&["varenderD129h264dec"]));
        assert_eq!(r[0].implementation.as_deref(), Some("varenderD129h264dec"));
        assert_eq!(r[0].state, AccelerationState::Active);
        assert_eq!(r[0].api.as_deref(), Some("VA-API"));
        let r =
            playback_from_diagnostics(&created(&["vah264dec", "varenderD129h264dec", "avdec_aac"]));
        assert_eq!(r[0].implementation.as_deref(), Some("varenderD129h264dec"))
    }
    #[test]
    fn render_reports_gl_sink_dmabuf_or_unknown() {
        let r = playback_from_diagnostics(&created(&["vah264dec", "webkitglvideosink"]));
        assert_eq!(r[1].state, AccelerationState::Active);
        assert_eq!(
            r[1].implementation.as_deref(),
            Some("OpenGL (webkitglvideosink)")
        );
        assert_eq!(r[1].api.as_deref(), Some("OpenGL"));
        let r = playback_from_diagnostics(&created(&["vah264dec"]));
        assert_eq!(r[1].state, AccelerationState::Unknown);
        assert_eq!(
            r[1].reason.as_deref(),
            Some("Rendering path has not been observed")
        )
    }
    #[test]
    fn diagnostics_preserve_factory_and_dmabuf() {
        let r = playback_from_diagnostics("selected vah264dec and dmabuf sink");
        assert_eq!(r[0].implementation.as_deref(), Some("vah264dec"));
        assert_eq!(r[1].state, AccelerationState::Active);
        assert_eq!(r[1].api.as_deref(), Some("DMA-BUF"))
    }
    #[test]
    fn last_created_decoder_wins_after_fallback() {
        let trace = concat!(
            "INFO GST_ELEMENT_FACTORY gstelementfactory.c:489:gst_element_factory_create_with_properties: creating element \"vah264dec\" named \"vah264dec0\"\n",
            "INFO GST_ELEMENT_FACTORY gstelementfactory.c:489:gst_element_factory_create_with_properties: creating element \"avdec_h264\" named \"avdec_h264-0\"\n",
            "INFO GST_ELEMENT_FACTORY gstelementfactory.c:489:gst_element_factory_create_with_properties: creating element \"avdec_aac\" named \"avdec_aac0\"\n",
        );
        let r = playback_from_diagnostics(trace);
        assert_eq!(r[0].implementation.as_deref(), Some("avdec_h264"));
        assert_eq!(r[0].state, AccelerationState::Software);
        assert_eq!(r[0].api, None)
    }
    #[test]
    fn unknown_decoder_is_named_and_unknown() {
        let r = playback_from_diagnostics(
            "creating element \"vah264dec\"\ncreating element \"futureh264dec\" named \"f0\"\n",
        );
        assert_eq!(r[0].implementation.as_deref(), Some("futureh264dec"));
        assert_eq!(r[0].state, AccelerationState::Unknown);
        assert!(r[0].reason.is_some())
    }
}
