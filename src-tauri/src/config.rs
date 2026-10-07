use crate::cli::Cli;
use clap::ValueEnum;
use directories::BaseDirs;
use serde::{Deserialize, Serialize};
use std::{
    ffi::OsString,
    fs, io,
    path::{Path, PathBuf},
};
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Format {
    #[default]
    Mp4,
    Webm,
    Gif,
    Copy,
}
impl Format {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Mp4 | Self::Copy => "mp4",
            Self::Webm => "webm",
            Self::Gif => "gif",
        }
    }
    fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_ascii_lowercase().as_str() {
            "mp4" => Some(Self::Mp4),
            "webm" => Some(Self::Webm),
            "gif" => Some(Self::Gif),
            _ => None,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Quality {
    #[default]
    Original,
    High,
    Small,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OnDone {
    #[default]
    Exit,
    Stay,
}
#[derive(Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileConfig {
    pub format: Option<Format>,
    pub quality: Option<Quality>,
    pub on_done: Option<OnDone>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Effective {
    pub format: Format,
    pub quality: Quality,
    pub on_done: OnDone,
    pub output: Option<PathBuf>,
}
fn path_from(xdg: Option<OsString>, home: Option<PathBuf>) -> Option<PathBuf> {
    xdg.filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| home.map(|h| h.join(".config")))
        .map(|base| base.join("wodeo").join("config.toml"))
}
pub fn path() -> Option<PathBuf> {
    path_from(
        std::env::var_os("XDG_CONFIG_HOME"),
        BaseDirs::new().map(|b| b.home_dir().to_path_buf()),
    )
}
pub fn load_from(path: &Path) -> Result<FileConfig, String> {
    let text = match fs::read_to_string(path) {
        Ok(v) => v,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(FileConfig::default()),
        Err(e) => return Err(format!("cannot read config {}: {e}", path.display())),
    };
    toml::from_str(&text).map_err(|e| format!("invalid config {}: {e}", path.display()))
}
pub fn load() -> Result<FileConfig, String> {
    path().map_or_else(|| Ok(FileConfig::default()), |p| load_from(&p))
}
// Precedence, later wins: built-in default, config file, output extension
// (format only, when no --format flag), explicit flag. A --format flag that
// disagrees with the output extension rewrites the extension.
pub fn resolve(cli: &Cli, file: &FileConfig) -> Effective {
    let mut output = cli.output.clone();
    let extension = output
        .as_deref()
        .and_then(Path::extension)
        .and_then(|e| e.to_str())
        .map(str::to_owned);
    let file_format = file.format.unwrap_or_default();
    let format = match (cli.format, extension.as_deref()) {
        (Some(flag), _) => flag,
        (None, Some(ext)) if ext.eq_ignore_ascii_case(file_format.extension()) => file_format,
        (None, Some(ext)) => Format::from_extension(ext).unwrap_or(file_format),
        (None, None) => file_format,
    };
    if cli.format.is_some() {
        if let Some(out) = output.as_mut() {
            if !extension
                .as_deref()
                .is_some_and(|e| e.eq_ignore_ascii_case(format.extension()))
            {
                out.set_extension(format.extension());
            }
        }
    }
    Effective {
        format,
        quality: cli.quality.or(file.quality).unwrap_or_default(),
        on_done: cli.on_done.or(file.on_done).unwrap_or_default(),
        output,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    fn cli(args: &[&str]) -> Cli {
        Cli::try_parse_from(std::iter::once("wodeo").chain(args.iter().copied())).unwrap()
    }
    fn file(text: &str) -> FileConfig {
        toml::from_str(text).unwrap()
    }
    #[test]
    fn path_prefers_xdg_then_home_config() {
        assert_eq!(
            path_from(Some("/x".into()), Some("/home/u".into())).unwrap(),
            PathBuf::from("/x/wodeo/config.toml")
        );
        assert_eq!(
            path_from(None, Some("/home/u".into())).unwrap(),
            PathBuf::from("/home/u/.config/wodeo/config.toml")
        );
        assert_eq!(
            path_from(Some("".into()), Some("/home/u".into())).unwrap(),
            PathBuf::from("/home/u/.config/wodeo/config.toml")
        );
    }
    #[test]
    fn load_reads_only_xdg_config_home() {
        let dir = tempfile::tempdir().unwrap();
        let old = std::env::var_os("XDG_CONFIG_HOME");
        std::env::set_var("XDG_CONFIG_HOME", dir.path());
        assert_eq!(load().unwrap(), FileConfig::default());
        let conf = dir.path().join("wodeo");
        fs::create_dir_all(&conf).unwrap();
        fs::write(conf.join("config.toml"), "format = \"gif\"\n").unwrap();
        let loaded = load();
        match old {
            Some(v) => std::env::set_var("XDG_CONFIG_HOME", v),
            None => std::env::remove_var("XDG_CONFIG_HOME"),
        }
        assert_eq!(loaded.unwrap().format, Some(Format::Gif));
    }
    #[test]
    fn load_reports_invalid_toml_unknown_keys_and_bad_values() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        assert_eq!(load_from(&path).unwrap(), FileConfig::default());
        for (text, needle) in [
            ("format = ", "invalid config"),
            ("formt = \"gif\"", "formt"),
            ("quality = \"ultra\"", "ultra"),
            ("on_done = \"later\"", "later"),
        ] {
            fs::write(&path, text).unwrap();
            let e = load_from(&path).unwrap_err();
            assert!(e.contains(&path.display().to_string()), "{e}");
            assert!(e.contains(needle), "{e}");
        }
        fs::write(
            &path,
            "format = \"webm\"\nquality = \"small\"\non_done = \"stay\"\n",
        )
        .unwrap();
        assert_eq!(
            load_from(&path).unwrap(),
            FileConfig {
                format: Some(Format::Webm),
                quality: Some(Quality::Small),
                on_done: Some(OnDone::Stay),
            }
        );
    }
    #[test]
    fn readme_example_parses() {
        let readme = include_str!("../../README.md");
        let example = readme
            .split("```toml\n")
            .nth(1)
            .and_then(|rest| rest.split("```").next())
            .expect("README has a toml example");
        assert_eq!(
            toml::from_str::<FileConfig>(example).unwrap(),
            FileConfig {
                format: Some(Format::Mp4),
                quality: Some(Quality::Original),
                on_done: Some(OnDone::Exit),
            }
        );
    }
    #[test]
    fn defaults_and_partial_file() {
        let e = resolve(&cli(&[]), &FileConfig::default());
        assert_eq!(
            (e.format, e.quality, e.on_done),
            (Format::Mp4, Quality::Original, OnDone::Exit)
        );
        let e = resolve(&cli(&[]), &file("format = \"gif\""));
        assert_eq!(
            (e.format, e.quality, e.on_done),
            (Format::Gif, Quality::Original, OnDone::Exit)
        );
    }
    #[test]
    fn flag_overrides_file() {
        let e = resolve(&cli(&["--quality", "high"]), &file("quality = \"small\""));
        assert_eq!(e.quality, Quality::High);
        let e = resolve(&cli(&["--on-done", "exit"]), &file("on_done = \"stay\""));
        assert_eq!(e.on_done, OnDone::Exit);
    }
    #[test]
    fn output_extension_implies_format() {
        let e = resolve(&cli(&["-o", "clip.gif"]), &file("format = \"webm\""));
        assert_eq!(e.format, Format::Gif);
        assert_eq!(e.output.unwrap(), PathBuf::from("clip.gif"));
        let e = resolve(&cli(&["-o", "clip.mp4"]), &file("format = \"copy\""));
        assert_eq!(e.format, Format::Copy);
        let e = resolve(&cli(&["-o", "clip.MP4"]), &file("format = \"gif\""));
        assert_eq!(e.format, Format::Mp4);
    }
    #[test]
    fn flag_overrides_output_extension() {
        let e = resolve(
            &cli(&["--format", "mp4", "-o", "clip.gif"]),
            &FileConfig::default(),
        );
        assert_eq!(e.format, Format::Mp4);
        assert_eq!(e.output.unwrap(), PathBuf::from("clip.mp4"));
        let e = resolve(
            &cli(&["--format", "copy", "-o", "clip.mp4"]),
            &FileConfig::default(),
        );
        assert_eq!(e.output.unwrap(), PathBuf::from("clip.mp4"));
        let e = resolve(
            &cli(&["--format", "webm", "-o", "dir/clip"]),
            &FileConfig::default(),
        );
        assert_eq!(e.output.unwrap(), PathBuf::from("dir/clip.webm"));
    }
}
