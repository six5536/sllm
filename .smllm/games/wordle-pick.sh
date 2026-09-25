#!/bin/sh
# smllm Wordle: pick a secret five-letter developer word for this instance.
# Stored base64-encoded so a glance at the file does not spoil the game.
set -eu
d=".smllm/state/games/wordle/$SMLLM_INSTANCE"
rm -rf "$d" && mkdir -p "$d"
set -- array async bytes cargo clone const crate debug fetch float frame graph \
  guard hooks index lexer linux macro match merge mutex panic parse patch proxy \
  query queue regex route scope shell slice stack state token trait tuple unify \
  while yield
n=$(( $(od -An -N2 -tu2 /dev/urandom | tr -d ' ') % $# + 1 ))
eval "w=\${$n}"
printf %s "$w" | base64 > "$d/secret"
