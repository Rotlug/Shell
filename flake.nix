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
          # Native deps for the gtk4/libadwaita/relm4 UI plus the astal-wireplumber
          # FFI bridge (src/ffi/astalwp) used for volume control.
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

            astal.wireplumber
            wireplumber
            pipewire
            util-linux # libmount/libblkid, needed by astal-wireplumber's FFI headers
            sysprof
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
              clang
            ];

            buildInputs = nativeLibs;

            # cxx-build (build.rs) needs these on the include/lib search path for
            # the astalwp FFI bridge; pkg-config alone isn't enough since that
            # C++ file's #includes are resolved by the compiler invocation cxx_build
            # drives directly, not through `pkg-config --cflags`.
            shellHook = ''
              export PKG_CONFIG_PATH="${pkgs.lib.makeSearchPathOutput "dev" "lib/pkgconfig" nativeLibs}:$PKG_CONFIG_PATH"
            '';
          };
        };
    };
}
