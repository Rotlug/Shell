{
  description = "gray-meadows-shell";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
  };

  outputs =
    inputs@{ nixpkgs, flake-parts, ... }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      systems = [ "x86_64-linux" ];

      perSystem =
        { pkgs, self', ... }:
        let
          # Native deps for the gtk4/libadwaita/relm4 UI. Audio (src/services/
          # wireplumber.rs) talks to PipeWire directly via the pure-Rust
          # `pwmixer` crate, so no WirePlumber/astal C headers are needed.
          nativeLibs = with pkgs; [
            glib
            gtk4
            libadwaita
            gtk4-layer-shell
            cairo
            pango
            gdk-pixbuf
            graphene
            dbus
            wayland
            wayland-protocols
            openssl
            fontconfig
            freetype

            pipewire # libpipewire/libspa, for the pwmixer crate
            lm_sensors # `sensors` crate (CPU temps in the sysstats bar module)
          ];
        in
        {
          packages.default = pkgs.rustPlatform.buildRustPackage {
            pname = "gray-meadows-shell";
            version = "0.1.0";
            src = ./.;

            cargoLock.lockFile = ./Cargo.lock;

            nativeBuildInputs = with pkgs; [
              pkg-config
              rustPlatform.bindgenHook # libspa-sys needs libclang at build time
              wrapGAppsHook4
            ];

            buildInputs = nativeLibs;

            # relm4-macros' `view!` expansions and a couple of other spots
            # trip clippy-level lints under -D warnings; keep this a normal
            # release build rather than fighting those.
            RUSTFLAGS = "--cap-lints warn";

            # scss.rs shells out to the `sass` CLI (Dart Sass) at startup to
            # compile styles - it's not a build-time dependency so it isn't
            # caught by buildInputs/cargo at all, just a hard runtime
            # requirement. wrapGAppsHook4 already wraps the binary; this
            # appends to that same wrapper instead of adding a second,
            # conflicting one.
            # nvml-wrapper (sysstats GPU usage) dlopen()s libnvidia-ml.so at
            # runtime rather than linking it - it's the proprietary driver's
            # library, which on NixOS only ever lives under
            # /run/opengl-driver/lib (not a normal ld.so search path), so
            # without this the GPU usage readout silently stays at 0%.
            preFixup = ''
              gappsWrapperArgs+=(
                --prefix PATH : "${pkgs.lib.makeBinPath [ pkgs.dart-sass ]}"
                --prefix LD_LIBRARY_PATH : /run/opengl-driver/lib
              )
            '';

            meta = {
              description = "Inparsian's gray-meadows-shell (niri/Hyprland GTK4 desktop shell)";
              homepage = "https://github.com/Inparsian/gray-meadows-shell";
              mainProgram = "gray-meadows-shell";
              platforms = pkgs.lib.platforms.linux;
            };
          };

          devShells.default = pkgs.mkShell {
            inputsFrom = [ self'.packages.default ];

            nativeBuildInputs = with pkgs; [
              rustc
              cargo
              rustfmt
              clippy
            ];

            shellHook = ''
              export PKG_CONFIG_PATH="${pkgs.lib.makeSearchPathOutput "dev" "lib/pkgconfig" nativeLibs}:$PKG_CONFIG_PATH"
            '';
          };
        };
    };
}
