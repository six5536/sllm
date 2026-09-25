#!/bin/sh
# smllm Wordle: score $SMLLM_PARAM_WORD against the secret, append the row to
# the board, print the board, and exit 0 only when the guess is right.
set -eu
d=".smllm/state/games/wordle/$SMLLM_INSTANCE"
secret=$(base64 -d < "$d/secret")
guess=$(printf %s "$SMLLM_PARAM_WORD" | tr '[:upper:]' '[:lower:]')
row=$(awk -v s="$secret" -v g="$guess" 'BEGIN {
  for (i = 1; i <= 5; i++) {
    if (substr(s, i, 1) == substr(g, i, 1)) r[i] = "G"; else left[substr(s, i, 1)]++
  }
  for (i = 1; i <= 5; i++) {
    if (r[i] == "G") continue
    c = substr(g, i, 1)
    if (left[c] > 0) { r[i] = "Y"; left[c]-- } else r[i] = "B"
  }
  for (i = 1; i <= 5; i++) printf "%s", (r[i] == "G" ? "🟩" : (r[i] == "Y" ? "🟨" : "⬛"))
}')
printf '%s %s\n' "$row" "$(printf %s "$guess" | tr '[:lower:]' '[:upper:]')" >> "$d/board"
cat "$d/board"
[ "$guess" = "$secret" ]
