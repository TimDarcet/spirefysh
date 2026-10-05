#!/bin/zsh
set -euo pipefail

if (( $# > 2 )); then
  print -u2 "usage: $0 [TRAJECTORIES] [OUTPUT_DIR]"
  exit 2
fi

script_dir=${0:A:h}
repo=${script_dir:h:h}
game='/Users/timdarcet/Library/Application Support/Steam/steamapps/common/Slay the Spire 2/SlayTheSpire2.app/Contents/MacOS/Slay the Spire 2'
install=${game:h}/mods/spirefysh-bridge
checkpoint=${SPIREFYSH_POLICY_CHECKPOINT:-"$repo/experiments/261004_2310_necrofix/latest.pt"}
pythonpath=${SPIREFYSH_POLICY_PYTHONPATH:-"$repo:$repo/.venv/lib/python3.13/site-packages"}
count=${1:-5}
output=${${2:-"$repo/target/oracle-policy-samples/$(date +%Y%m%d-%H%M%S)"}:A}
characters=(CHARACTER.IRONCLAD CHARACTER.SILENT CHARACTER.DEFECT CHARACTER.REGENT CHARACTER.NECROBINDER)

(( count > 0 )) || { print -u2 'TRAJECTORIES must be positive'; exit 2; }
pgrep -x 'Slay the Spire 2' >/dev/null 2>&1 && { print -u2 'quit the game before sampling'; exit 2; }
for required in "$game" "$install/policy.py" "$checkpoint" \
  "$repo/.venv/bin/python" "$repo/train.py"; do
  [[ -e "$required" ]] || { print -u2 "missing sampler dependency: $required"; exit 2; }
done
mkdir -p "$output"
cp "$script_dir/oracle-output.marker" "$output/.spirefysh-oracle-output"
print 'run\tcharacter\tgame_exit\treplay_exit\treport' > "$output/summary.tsv"

for (( block = 0; block * 5 < count; block++ )); do
  order=(${characters[@]})
  for (( i = 5; i > 1; i-- )); do
    j=$(( RANDOM % i + 1 ))
    swap=$order[i]; order[i]=$order[j]; order[j]=$swap
  done
  for character in $order; do
    run=$(( block * 5 + ${order[(i)$character]} ))
    (( run <= count )) || break
    name=$(printf '%04d-%s' $run ${character#CHARACTER.})
    trace="$output/$name.ndjson"
    log="$output/$name.game.log"
    report="$output/$name.replay.txt"
    print "[$run/$count] $character"
    set +e
    SPIREFYSH_TRACE_PATH="$trace" \
    SPIREFYSH_POLICY_CHECKPOINT="$checkpoint" \
    SPIREFYSH_POLICY_PYTHONPATH="$pythonpath" \
    SPIREFYSH_POLICY_SEED=$(( RANDOM << 16 | RANDOM )) \
    SPIREFYSH_CHARACTER_ID="$character" \
    SteamAppId=2868840 SteamGameId=2868840 \
      "$game" > "$log" 2>&1
    game_exit=$?
    cargo run --release --quiet --bin replay -- "$trace" > "$report" 2>&1
    replay_exit=$?
    set -e
    print "$run\t$character\t$game_exit\t$replay_exit\t$report" >> "$output/summary.tsv"
    if (( replay_exit != 0 )) || ! jq -e 'select(.kind == "episode_completed" and .metadata.complete)' "$trace" >/dev/null; then
      print "results=$output"
      [[ ${SPIREFYSH_CONTINUE_ON_MISMATCH:-0} == 1 ]] || exit 1
    fi
  done
done

print "results=$output"
