use std::process::{Command, Stdio};

fn binary() -> Command {
    Command::new(env!("CARGO_BIN_EXE_wodeo"))
}

#[test]
fn help_version_and_malformed_arguments_finish_before_gui() {
    let help = binary().arg("--help").output().unwrap();
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("Trim one MP4"));
    assert!(help.stderr.is_empty());
    let version = binary().arg("--version").output().unwrap();
    assert!(version.status.success());
    assert_eq!(
        String::from_utf8_lossy(&version.stdout).trim(),
        format!("wodeo {}", env!("CARGO_PKG_VERSION"))
    );
    assert!(version.stderr.is_empty());
    let invalid = binary().arg("--unknown").output().unwrap();
    assert!(!invalid.status.success());
    assert!(invalid.stdout.is_empty());
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("unexpected argument"));
}

#[test]
fn zsh_completions_print_without_wayland() {
    let output = binary()
        .env_remove("WAYLAND_DISPLAY")
        .args(["--completions", "zsh"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&output.stdout).starts_with("#compdef wodeo"));
    assert!(output.stderr.is_empty());
}

#[test]
fn unavailable_wayland_is_nonzero_and_stdout_clean() {
    let output = binary()
        .env_remove("WAYLAND_DISPLAY")
        .arg("example.mp4")
        .args(["--output", "out.mp4", "--verbose"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("requires a native Wayland session"));
}

#[test]
fn simultaneous_invocations_own_independent_streams() {
    let first = binary()
        .arg("--help")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let second = binary()
        .arg("--version")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let first = first.wait_with_output().unwrap();
    let second = second.wait_with_output().unwrap();
    assert!(first.status.success() && second.status.success());
    assert!(first.stderr.is_empty() && second.stderr.is_empty());
    assert!(String::from_utf8_lossy(&first.stdout).contains("Usage:"));
    assert!(String::from_utf8_lossy(&second.stdout).contains(env!("CARGO_PKG_VERSION")));
}

#[test]
fn removed_force_and_invalid_option_values_are_rejected() {
    for args in [
        vec!["--force"],
        vec!["-f"],
        vec!["--format", "avi"],
        vec!["--quality", "ultra"],
        vec!["--on-done", "later"],
    ] {
        let output = binary()
            .env_remove("WAYLAND_DISPLAY")
            .args(&args)
            .output()
            .unwrap();
        assert!(!output.status.success(), "{args:?} was accepted");
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains("--help"));
    }
}

#[test]
fn invalid_config_file_exits_before_window() {
    let dir = tempfile::tempdir().unwrap();
    // A listening socket is enough for the Wayland reachability check, so the
    // config error is the first failure the binary can hit.
    let _listener =
        std::os::unix::net::UnixListener::bind(dir.path().join("wayland-test")).unwrap();
    let config = dir.path().join("config/wodeo");
    std::fs::create_dir_all(&config).unwrap();
    std::fs::write(config.join("config.toml"), "fromat = \"gif\"\n").unwrap();
    let output = binary()
        .env("WAYLAND_DISPLAY", "wayland-test")
        .env("XDG_RUNTIME_DIR", dir.path())
        .env("XDG_CONFIG_HOME", dir.path().join("config"))
        .env("XDG_STATE_HOME", dir.path().join("state"))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("config.toml"), "{stderr}");
    assert!(stderr.contains("fromat"), "{stderr}");
    assert!(!dir.path().join("state").exists());
}
