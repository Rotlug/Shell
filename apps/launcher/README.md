# gms-launcher

A generic launcher. It doesn't know what it launches, or what happens when something is picked:
it shows the entries it is given, and prints the one that was picked. What to do about that is
up to whatever ran it, in the usual unix way.

```sh
printf 'firefox\tFirefox\nkitty\tKitty\n' | gms-launcher --prompt Launch
```

It looks like the rest of Gray Meadows, and follows the same `GMS_*` environment variables
(see `gms_style`).

## Entries

Read from stdin, one per line:

```
title
icon<TAB>title
icon<TAB>title<TAB>anything<TAB>else
```

* **icon**: the name of an icon from the icon theme, or the absolute path of an image. It can be
  empty (`<TAB>title`) or left out. If no entry has one, there is no room made for them.
* **title**: what is shown, and what is searched.
* Any more tab separated fields are not looked at. They are for whoever gave the entries.

## What is picked

The whole line of the entry that was picked is printed, as it was given. That is how a script
gets its own data back, without the launcher knowing about it: put it in a third field.

| | |
| --- | --- |
| exit status `0` | something was picked, and printed |
| exit status `1` | it was closed without picking (`Escape`, a click elsewhere), or there were no entries |
| exit status `2` | it was used wrong |

## Using it

Type to search: every word has to be in the title, the beginning of the title and of its words
matches best, and the letters in order match last (`ffx` finds Firefox). `Up` and `Down` (or
`Ctrl`+`P` and `Ctrl`+`N`, `Ctrl`+`K` and `Ctrl`+`J`) move, `Page Up` and `Page Down` move
further, `Enter` (or a click) picks.

Options:

* `-p`, `--prompt TEXT`: what the search box says while nothing is typed in it.

The list is meant for hundreds of entries, a few thousand at most.

## Examples

In `examples/`, to be copied and changed, or used as they are. Both can print the entries they give
the launcher with `--list`, and use `$GMS_LAUNCHER` instead of `gms-launcher` if it is set.

* `app-picker.sh`: all the applications that have a desktop file, with their icons. Launches the
  one that is picked with `gio launch`.
* `emoji-picker.sh`: the emojis of `emoji-data.txt`. Copies the one that is picked, and types it
  in the window that had the focus (needs `wl-copy` and `wtype`).

With the flake, they are the packages `gms-app-picker` and `gms-emoji-picker`.

For example, with niri:

```kdl
binds {
    Mod+D { spawn "gms-app-picker"; }
    Mod+Period { spawn "gms-emoji-picker"; }
}
```

### Writing one

A window switcher, for niri:

```sh
niri msg --json windows \
  | jq -r '.[] | "\t\(.app_id // "?") — \(.title // "")\t\(.id)"' \
  | gms-launcher --prompt Window \
  | cut -f3 \
  | xargs -r -I{} niri msg action focus-window --id {}
```
