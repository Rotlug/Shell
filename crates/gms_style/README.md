# gms_style

The look of everything Gray Meadows: one stylesheet (SCSS, in `styles/`) shared by the shell
and the apps that come with it.

The stylesheet is copied to `~/.config/gray-meadows/styles` the first time it is needed (files
that are there already are left alone, so they can be edited), and compiled with `sass`.
Compiled stylesheets are cached in `~/.cache/gray-meadows`, so apps that are started
all the time don't have to wait for `sass` every time.

## Environment

* `GMS_BASE16_00` .. `GMS_BASE16_0F`: a base16 palette, as hex without the `#`. All 16 are
  needed for them to be used.
* `GMS_FONT`: the font family to use for all text.

When one of these is set, `~/.config/gray-meadows/styles/_user.scss` is generated from them
and edits to it are overwritten.
