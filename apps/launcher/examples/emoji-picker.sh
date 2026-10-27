#!/usr/bin/env bash
# Pick an emoji with gms-launcher: it is copied, and typed into the window that had the focus.
#
#   emoji-picker.sh          pick one
#   emoji-picker.sh --list   print the entries the launcher would be given
#
# The emojis are in emoji-data.txt next to this script (`emoji<TAB>name`), EMOJI_DATA can say
# where else to look. Needs wl-copy (wl-clipboard) and wtype, GMS_LAUNCHER can be set to use
# something else than `gms-launcher`.
set -euo pipefail

data="${EMOJI_DATA:-$(dirname "$(readlink -f "$0")")/emoji-data.txt}"

# `icon<TAB>title<TAB>emoji`: no icon, the emoji is part of the title (that is what is shown
# and searched), and it is also the third field to get back what to type.
list_entries() {
  awk -F'\t' '{ printf "\t%s  %s\t%s\n", $1, $2, $1 }' "$data"
}

if [ "${1:-}" = "--list" ]; then
  list_entries
  exit 0
fi

selected=$(list_entries | "${GMS_LAUNCHER:-gms-launcher}" --prompt 'Emoji') || exit 0
emoji=$(printf '%s' "$selected" | cut -f3)

printf '%s' "$emoji" | wl-copy

# The launcher is gone, but the focus needs a moment to go back to where it was
sleep 0.15
wtype "$emoji" 2> /dev/null || true
