{
  description = "gray-meadows-shell dev/build environment";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
  };

  outputs =
    inputs@{ nixpkgs, flake-parts, ... }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      systems = [ "x86_64-linux" ];

      perSystem =
        { pkgs, ... }:
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
          devShells.default = pkgs.mkShell {
            nativeBuildInputs = with pkgs; [
              pkg-config
              rustc
              cargo
              rustfmt
              clippy
            ];

            buildInputs = nativeLibs ++ [ pkgs.libclang.lib ];

            shellHook = ''
              export PKG_CONFIG_PATH="${pkgs.lib.makeSearchPathOutput "dev" "lib/pkgconfig" nativeLibs}:$PKG_CONFIG_PATH"
              # libspa-sys uses bindgen, which needs libclang at build time.
              # bindgen calls libclang directly (not through the nixpkgs cc
              # wrapper), so it doesn't automatically see glibc's headers -
              # point it there explicitly or it can't even find <inttypes.h>.
              export LIBCLANG_PATH="${pkgs.libclang.lib}/lib"
              export BINDGEN_EXTRA_CLANG_ARGS="-isystem ${pkgs.glibc.dev}/include"
            '';
          };
        };
    };
}
