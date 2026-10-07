{
  description = "wodeo: exact MP4 trimming for Wayland";
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?rev=0ad6f47ea4fe188f4bc8f0380f93ae8523337c6c";
  };
  outputs = { self, nixpkgs }:
    let
      system = "x86_64-linux";
      pkgs = import nixpkgs { inherit system; };
      lib = nixpkgs.lib;
      version = (lib.importTOML ./src-tauri/Cargo.toml).package.version;
      mediaPackages = with pkgs; [ ffmpeg-full gst_all_1.gstreamer gst_all_1.gst-plugins-base gst_all_1.gst-plugins-good gst_all_1.gst-plugins-bad gst_all_1.gst-plugins-ugly gst_all_1.gst-libav gst_all_1.gst-vaapi ];
    in {
      packages.${system}.default = pkgs.rustPlatform.buildRustPackage {
        pname = "wodeo"; inherit version; src = self;
        cargoRoot = "src-tauri"; buildAndTestSubdir = "src-tauri";
        tauriBundleType = "deb";
        cargoLock.lockFile = ./src-tauri/Cargo.lock;
        npmDeps = pkgs.fetchNpmDeps {
          src = self;
          fetcherVersion = 2;
          hash = "sha256-Nf1BGb13Za4yodrRUBrRxKSlfUClAa+J45+n6iU2Wq4=";
        };
        NIX_NPM_FETCHER_VERSION = "2";
        nativeBuildInputs = [ pkgs.cargo-tauri.hook pkgs.nodejs pkgs.npmHooks.npmConfigHook pkgs.pkg-config pkgs.wrapGAppsHook3 pkgs.ffmpeg-full ];
        buildInputs = with pkgs; [ dbus glib gtk3 librsvg webkitgtk_4_1 ] ++ mediaPackages;
        preFixup = ''
          gappsWrapperArgs+=(
            --set GDK_BACKEND wayland
            --prefix PATH : "${lib.makeBinPath [ pkgs.ffmpeg-full ]}"
            --prefix GST_PLUGIN_SYSTEM_PATH_1_0 : "${lib.makeSearchPathOutput "lib" "lib/gstreamer-1.0" mediaPackages}"
          )
        '';
        meta = { description = "Minimal exact MP4 trimmer for Hyprland"; license = lib.licenses.mit; mainProgram = "wodeo"; platforms = [ system ]; };
      };
      apps.${system}.default = { type = "app"; program = "${self.packages.${system}.default}/bin/wodeo"; };
      devShells.${system}.default = pkgs.mkShell {
        packages = with pkgs; [ cargo clippy rustc rustfmt nodejs pkg-config dbus webkitgtk_4_1 gtk3 glib librsvg ] ++ mediaPackages;
        shellHook = ''
          export GDK_BACKEND=wayland
          export GST_PLUGIN_SYSTEM_PATH_1_0="${lib.makeSearchPathOutput "lib" "lib/gstreamer-1.0" mediaPackages}"
        '';
      };
    };
}
