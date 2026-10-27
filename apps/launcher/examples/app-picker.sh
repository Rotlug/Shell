#!/usr/bin/env bash
# Pick an application with gms-launcher, and start it.
#
#   app-picker.sh          pick one and launch it
#   app-picker.sh --list   print the entries the launcher would be given
#
# The entries are `icon<TAB>name<TAB>desktop file`: the launcher shows the icon and the name,
# and prints the whole line of what is picked, which is how the desktop file gets back here.
#
# Set GMS_LAUNCHER to use something else than `gms-launcher`.
set -euo pipefail

# Everything that is a desktop entry, in the order that the spec says they win over each other:
# the user's own first, then the system's.
list_entries() {
  local -a dirs=()
  local dir
  IFS=: read -ra data_dirs <<< "${XDG_DATA_DIRS:-/usr/local/share:/usr/share}"
  for dir in "${XDG_DATA_HOME:-$HOME/.local/share}" "${data_dirs[@]}" "$HOME/.nix-profile/share"; do
    [ -d "$dir/applications" ] && dirs+=("$dir/applications")
  done

  [ "${#dirs[@]}" -gt 0 ] || return 0

  # One find and one awk for all of them, not a process per file.
  find -L "${dirs[@]}" -name '*.desktop' -print0 2>/dev/null |
    xargs -0 awk -F= '
      FNR == 1 { in_entry = 0; hidden = 0; name = ""; icon = ""; app = 0 }
      /^\[/ {
        # Only the main group, the actions of an app are not apps
        in_entry = ($0 == "[Desktop Entry]")
        next
      }
      !in_entry { next }
      /^(NoDisplay|Hidden)=true/ { hidden = 1 }
      /^Type=Application/ { app = 1 }
      /^Name=/ && name == "" { name = substr($0, index($0, "=") + 1) }
      /^Icon=/ && icon == "" { icon = substr($0, index($0, "=") + 1) }
      ENDFILE {
        id = FILENAME
        sub(".*/", "", id)
        # The same app in more than one place is one app: the first one counts
        if (app && !hidden && name != "" && !(id in seen)) {
          seen[id] = 1
          printf "%s\t%s\t%s\n", icon, name, FILENAME
        }
      }
    ' 2>/dev/null |
    sort -f -t$'\t' -k2,2
}

if [ "${1:-}" = "--list" ]; then
  list_entries
  exit 0
fi

selected=$(list_entries | "${GMS_LAUNCHER:-gms-launcher}" --prompt 'Launch') || exit 0
desktop_file=$(printf '%s' "$selected" | cut -f3)

# `setsid` so that the app doesn't depend on whatever started this: it would be taken down along
# with it otherwise. `gio launch` takes care of everything a desktop file can ask for.
exec setsid --fork gio launch "$desktop_file" > /dev/null 2>&1
