{
  lib,
  rustPlatform,
  fetchNpmDeps,
  npmHooks,
  cargo-tauri,
  nodejs,
  pkg-config,
  wrapGAppsHook3,
  installShellFiles,
  ffmpeg-full,
  gst_all_1,
  dbus,
  glib,
  gtk3,
  librsvg,
  webkitgtk_4_1,
  withShellCompletions ? true,
}:

let
  mediaPackages = [
    ffmpeg-full
    gst_all_1.gstreamer
    gst_all_1.gst-plugins-base
    gst_all_1.gst-plugins-good
    gst_all_1.gst-plugins-bad
    gst_all_1.gst-plugins-ugly
    gst_all_1.gst-libav
  ];
in
rustPlatform.buildRustPackage {
  pname = "wodeo";
  version = (lib.importTOML ../src-tauri/Cargo.toml).package.version;
  src = ../.;

  cargoRoot = "src-tauri";
  buildAndTestSubdir = "src-tauri";
  tauriBundleType = "deb";
  cargoLock.lockFile = ../src-tauri/Cargo.lock;

  npmDeps = fetchNpmDeps {
    src = ../.;
    fetcherVersion = 2;
    hash = "sha256-tcEv43w4cS80EQB9JYWfpes+3IjaYDa8pY9Fdxe7Cc4=";
  };
  NIX_NPM_FETCHER_VERSION = "2";

  nativeBuildInputs = [
    cargo-tauri.hook
    nodejs
    npmHooks.npmConfigHook
    pkg-config
    wrapGAppsHook3
    ffmpeg-full
  ] ++ lib.optional withShellCompletions installShellFiles;
  buildInputs = [ dbus glib gtk3 librsvg webkitgtk_4_1 ] ++ mediaPackages;

  # Tests need a Wayland session and media tooling; run them with `cargo test` in the dev shell.
  doCheck = false;

  postInstall = lib.optionalString withShellCompletions ''
    installShellCompletion --cmd wodeo --zsh src-tauri/completions/_wodeo
  '';

  preFixup = ''
    gappsWrapperArgs+=(
      --set GDK_BACKEND wayland
      --prefix PATH : "${lib.makeBinPath [ ffmpeg-full ]}"
      --prefix GST_PLUGIN_SYSTEM_PATH_1_0 : "${lib.makeSearchPathOutput "lib" "lib/gstreamer-1.0" mediaPackages}"
    )
  '';

  meta = {
    description = "Minimal exact MP4 trimmer for Hyprland";
    license = lib.licenses.mit;
    mainProgram = "wodeo";
    platforms = [ "x86_64-linux" ];
  };
}
