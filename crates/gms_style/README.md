# gms_style

The look of everything Gray Meadows: one stylesheet (SCSS, in `styles/`) shared by the shell
and the apps that come with it.

The stylesheet is part of the program, it is compiled into it. Nothing is copied to the home
directory and there are no files to edit: the only way to change the look is the environment
(below), so the look can never be out of date with the program it belongs to.

It is compiled with `sass` when a program starts. The compiled stylesheets are cached in
`~/.cache/gray-meadows` (keyed by what they were made from, so a new version or a different
environment never uses an old one), so apps that are started all the time don't have to wait
for `sass` every time.

## Environment

* `GMS_BASE16_00` .. `GMS_BASE16_0F`: a base16 palette, as hex without the `#`. All 16 are
  needed for them to be used. Slots 00 to 07 are the shades the look is made of, slots 08 to
  0F are the colors that things cycle through (workspaces, audio streams, monitors).
* `GMS_FONT`: the font family to use for all text.

Without them the bundled colors and fonts are used (`styles/_user.scss`), which are all shades
of gray.
