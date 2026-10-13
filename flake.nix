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

            cargoLock = {
              lockFile = ./Cargo.lock;
              # pwmixer is a git dependency (https://github.com/Rotlug/shell),
              # not a crates.io one, so Nix needs a fixed-output hash for it
              # to fetch deterministically in the sandbox.
              outputHashes = {
                "pwmixer-0.1.0" = "sha256-s9asB7abo0Gxn7NeA/Rq7A7V37PoSyVHvB8jzxVkZFI=";
              };
            };

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
