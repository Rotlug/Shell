#!/usr/bin/env bash
set -euo pipefail

TEXT_TO_REMOVE="Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
TEXT_TO_REMOVE_2="Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>"

# Requires git-filter-repo
git filter-repo --message-callback "
return message.replace(b'$TEXT_TO_REMOVE', b'')
" --force

git filter-repo --message-callback "
return message.replace(b'$TEXT_TO_REMOVE_2', b'')
" --force
