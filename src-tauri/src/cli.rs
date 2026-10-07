use crate::config::{Effective, Format, OnDone, Quality};
use clap::Parser;
use serde::Serialize;
use std::path::PathBuf;
#[derive(Debug, Clone, Parser)]
#[command(
    name = "wodeo",
    version,
    about = "Trim one MP4 precisely in a minimal Wayland UI"
)]
pub struct Cli {
    #[arg(value_name = "INPUT")]
    pub input: Option<PathBuf>,
    #[arg(short, long, value_name = "PATH")]
    pub output: Option<PathBuf>,
    #[arg(long, value_enum)]
    pub format: Option<Format>,
    #[arg(long, value_enum)]
    pub quality: Option<Quality>,
    #[arg(long, value_enum)]
    pub on_done: Option<OnDone>,
    #[arg(short, long)]
    pub verbose: bool,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchOptions {
    pub input: Option<String>,
    pub output: Option<String>,
    pub format: Format,
    pub quality: Quality,
    pub on_done: OnDone,
    pub verbose: bool,
}
impl LaunchOptions {
    pub fn new(cli: Cli, effective: Effective) -> Self {
        Self {
            input: cli.input.map(|p| p.to_string_lossy().into_owned()),
            output: effective.output.map(|p| p.to_string_lossy().into_owned()),
            format: effective.format,
            quality: effective.quality,
            on_done: effective.on_done,
            verbose: cli.verbose,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_all_options() {
        let c = Cli::try_parse_from([
            "wodeo",
            "in.mp4",
            "-o",
            "out.webm",
            "--format",
            "webm",
            "--quality",
            "small",
            "--on-done",
            "stay",
            "-v",
        ])
        .unwrap();
        assert_eq!(c.input.as_deref(), Some(std::path::Path::new("in.mp4")));
        assert_eq!(c.format, Some(Format::Webm));
        assert_eq!(c.quality, Some(Quality::Small));
        assert_eq!(c.on_done, Some(OnDone::Stay));
        assert!(c.verbose);
        for format in ["mp4", "webm", "gif", "copy"] {
            assert!(Cli::try_parse_from(["wodeo", "--format", format]).is_ok());
        }
        for quality in ["original", "high", "small"] {
            assert!(Cli::try_parse_from(["wodeo", "--quality", quality]).is_ok());
        }
    }
    #[test]
    fn rejects_unknown_invalid_values_force_and_extra_input() {
        assert!(Cli::try_parse_from(["wodeo", "--wat"]).is_err());
        assert!(Cli::try_parse_from(["wodeo", "a.mp4", "b.mp4"]).is_err());
        assert!(Cli::try_parse_from(["wodeo", "--format", "avi"]).is_err());
        assert!(Cli::try_parse_from(["wodeo", "--quality", "ultra"]).is_err());
        assert!(Cli::try_parse_from(["wodeo", "--on-done", "later"]).is_err());
        assert!(Cli::try_parse_from(["wodeo", "--force"]).is_err());
        assert!(Cli::try_parse_from(["wodeo", "-f"]).is_err());
    }
    #[test]
    fn launch_options_serialize_effective_values() {
        let c = Cli::try_parse_from(["wodeo", "--on-done", "stay"]).unwrap();
        let effective = crate::config::resolve(&c, &Default::default());
        let json = serde_json::to_value(LaunchOptions::new(c, effective)).unwrap();
        assert_eq!(json["format"], "mp4");
        assert_eq!(json["quality"], "original");
        assert_eq!(json["onDone"], "stay");
        assert!(json.get("force").is_none());
    }
}
