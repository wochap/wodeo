mod acceleration;
mod app;
mod cli;
mod config;
mod error;
mod export;
mod lifecycle;
mod logging;
mod media;
mod preview_server;
use clap::Parser;
use std::{os::unix::net::UnixStream, path::PathBuf};
use tauri::{Manager, RunEvent};
fn validate_wayland() -> Result<String, String> {
    std::env::set_var("GDK_BACKEND", "wayland");
    let display = std::env::var("WAYLAND_DISPLAY").map_err(|_| {
        "WAYLAND_DISPLAY is not set; wodeo requires a native Wayland session".to_string()
    })?;
    let socket =
        if PathBuf::from(&display).is_absolute() {
            PathBuf::from(&display)
        } else {
            PathBuf::from(std::env::var("XDG_RUNTIME_DIR").map_err(|_| {
                "XDG_RUNTIME_DIR is not set; cannot locate Wayland display".to_string()
            })?)
            .join(&display)
        };
    UnixStream::connect(&socket)
        .map_err(|e| format!("Wayland display {} is unavailable: {e}", socket.display()))?;
    Ok(display)
}
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let cli = cli::Cli::parse();
    if let Some(shell) = cli.completions {
        print!("{}", shell.script());
        std::process::exit(0);
    }
    let wayland_name = match validate_wayland() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("wodeo: {e}");
            std::process::exit(lifecycle::EXIT_STARTUP)
        }
    };
    let file_config = match config::load() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("wodeo: {e}");
            std::process::exit(lifecycle::EXIT_STARTUP)
        }
    };
    let effective = config::resolve(&cli, &file_config);
    let (log_paths, guard) = match logging::init(cli.verbose) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("wodeo: logging initialization failed: {e}");
            std::process::exit(lifecycle::EXIT_STARTUP)
        }
    };
    let state_dir = log_paths.state_dir.display().to_string();
    let application_log = log_paths.application.display().to_string();
    let gstreamer_log = log_paths.gstreamer.display().to_string();
    tracing::info!(
        wayland_display = %wayland_name,
        state_dir = %state_dir,
        application_log = %application_log,
        gstreamer_log = %gstreamer_log,
        format = ?effective.format,
        quality = ?effective.quality,
        on_done = ?effective.on_done,
        "starting native Wayland application"
    );
    let launch = cli::LaunchOptions::new(cli, effective);
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(app::LaunchState::new(launch))
        .manage(media::MediaState::default())
        .manage(export::ExportState::default())
        .manage(log_paths)
        .manage(app::LogGuard { _guard: guard })
        .setup(|app| {
            let server =
                preview_server::PreviewServer::start(app.handle().clone()).unwrap_or_else(|e| {
                    eprintln!("wodeo: preview server failed to start: {e}");
                    std::process::exit(lifecycle::EXIT_STARTUP)
                });
            app.manage(server);
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(1000));
                if let Some(window) = handle.get_webview_window("main") {
                    if !window.is_visible().unwrap_or(false) {
                        tracing::info!("frontend did not show the window; showing it");
                        let _ = window.show();
                    }
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app::take_launch_options,
            app::exit_application,
            media::load_input,
            export::start_export,
            export::cancel_export,
            acceleration::playback_acceleration
        ])
        .build(tauri::generate_context!())
        .unwrap_or_else(|e| {
            eprintln!("wodeo: application initialization failed: {e}");
            std::process::exit(lifecycle::EXIT_STARTUP)
        });
    app.run(|handle, event| {
        if matches!(event, RunEvent::ExitRequested { .. }) {
            handle.state::<preview_server::PreviewServer>().stop();
            media::cleanup(handle.state::<media::MediaState>().inner())
        }
        if matches!(event, RunEvent::Exit) {
            handle.state::<logging::LogPaths>().remove_trace()
        }
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_wayland_is_rejected() {
        let old = std::env::var_os("WAYLAND_DISPLAY");
        std::env::remove_var("WAYLAND_DISPLAY");
        assert!(validate_wayland().is_err());
        if let Some(v) = old {
            std::env::set_var("WAYLAND_DISPLAY", v)
        }
    }
}
