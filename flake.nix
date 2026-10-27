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
          # `volume_mixer` crate, so no WirePlumber/astal C headers are needed.
          nativeLibs = with pkgs; [
            glib
            gtk4
            libadwaita
            gtk4-layer-shell
            cairo
            pango
            gdk-pixbuf
            graphene
            wayland
            wayland-protocols
            openssl
            fontconfig
            freetype

            pipewire # libpipewire/libspa, for the volume_mixer crate
            udev # libudev, for the brightness crate (DDC/CI monitor enumeration)
          ];
        in
        {
          packages.default = pkgs.rustPlatform.buildRustPackage {
            pname = "gray-meadows-shell";
            version = "0.1.0";
            src = ./.;

            # The libraries are workspace members (libraries/ and crates/), so there
            # are no git dependencies to hash.
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

            # This builds the shell and the apps that come with it
            # (gms-session, gms-launcher): the default members of the
            # workspace. The libraries' demo binaries are left out.
            #
            # gms_style shells out to the `sass` CLI (Dart Sass) to compile
            # styles, in every one of those - it's not a build-time
            # dependency so it isn't caught by buildInputs/cargo at all,
            # just a hard runtime requirement. wrapGAppsHook4 already wraps
            # the binaries; this appends to those same wrappers instead of
            # adding second, conflicting ones.
            # system_stats' nvidia feature (GPU memory) dlopen()s libnvidia-ml.so at
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

          # The example scripts of gms-launcher, ready to bind to keys.
          # The awk programs are single-quoted on purpose: they are awk
          # source, not meant to undergo shell expansion.
          packages.gms-app-picker = pkgs.writeShellApplication {
            name = "gms-app-picker";
            runtimeInputs = with pkgs; [
              self'.packages.default # gms-launcher
              glib # gio
              findutils
              gawk
              coreutils
              util-linux # setsid
            ];
            excludeShellChecks = [ "SC2016" ];
            text = builtins.readFile ./apps/launcher/examples/app-picker.sh;
          };

          packages.gms-emoji-picker = pkgs.writeShellApplication {
            name = "gms-emoji-picker";
            runtimeInputs = with pkgs; [
              self'.packages.default # gms-launcher
              gawk
              coreutils
              wl-clipboard
              wtype
            ];
            runtimeEnv.EMOJI_DATA = ./apps/launcher/examples/emoji-data.txt;
            excludeShellChecks = [ "SC2016" ];
            text = builtins.readFile ./apps/launcher/examples/emoji-picker.sh;
          };

          devShells.default = pkgs.mkShell {
            inputsFrom = [ self'.packages.default ];

            nativeBuildInputs = with pkgs; [
              rustc
              cargo
              rustfmt
              clippy
              dart-sass
            ];

            shellHook = ''
              export PKG_CONFIG_PATH="${
                pkgs.lib.makeSearchPathOutput "dev" "lib/pkgconfig" nativeLibs
              }:$PKG_CONFIG_PATH"
            '';
          };
        };
    };
}
