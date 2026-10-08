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
      wodeo = pkgs.callPackage ./nix/package.nix { };
      mediaPackages = with pkgs; [ ffmpeg-full gst_all_1.gstreamer gst_all_1.gst-plugins-base gst_all_1.gst-plugins-good gst_all_1.gst-plugins-bad gst_all_1.gst-plugins-ugly gst_all_1.gst-libav gst_all_1.gst-vaapi ];
    in {
      packages.${system}.default = wodeo;
      apps.${system}.default = { type = "app"; program = "${wodeo}/bin/wodeo"; };
      devShells.${system}.default = pkgs.mkShell {
        packages = with pkgs; [ cargo clippy rustc rustfmt nodejs pkg-config dbus webkitgtk_4_1 gtk3 glib librsvg ] ++ mediaPackages;
        shellHook = ''
          export GDK_BACKEND=wayland
          export GST_PLUGIN_SYSTEM_PATH_1_0="${lib.makeSearchPathOutput "lib" "lib/gstreamer-1.0" mediaPackages}"
        '';
      };
    };
}
