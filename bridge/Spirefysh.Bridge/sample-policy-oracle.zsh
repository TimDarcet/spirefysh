#!/bin/zsh
set -euo pipefail
zmodload zsh/parameter

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
timeout=${SPIREFYSH_GAME_TIMEOUT_SECONDS:-600}
retries=${SPIREFYSH_INCOMPLETE_RETRIES:-3}
jobs=${SPIREFYSH_JOBS:-4}
characters=(CHARACTER.IRONCLAD CHARACTER.SILENT CHARACTER.DEFECT CHARACTER.REGENT CHARACTER.NECROBINDER)

(( count > 0 && jobs > 0 )) || { print -u2 'TRAJECTORIES and SPIREFYSH_JOBS must be positive'; exit 2; }
(( timeout > 0 && retries >= 0 )) || { print -u2 'timeout and retries must be nonnegative'; exit 2; }
pgrep -x 'Slay the Spire 2' >/dev/null 2>&1 && { print -u2 'quit the game before sampling'; exit 2; }
for required in "$game" "$install/policy.py" "$checkpoint" \
  "$repo/.venv/bin/python" "$repo/train.py"; do
  [[ -e "$required" ]] || { print -u2 "missing sampler dependency: $required"; exit 2; }
done
cargo build --release --quiet --manifest-path "$repo/Cargo.toml" --bin replay
mkdir -p "$output"
cp "$script_dir/oracle-output.marker" "$output/.spirefysh-oracle-output"
print 'run\tcharacter\tattempt\tgame_exit\treplay_exit\tcomplete\treport' > "$output/summary.tsv"

sample() {
  local run=$1 character=$2 seed=$3 base attempt name trace log report game_pid watchdog_pid game_exit replay_exit complete
  base=$(printf '%04d-%s' $run ${character#CHARACTER.})
  for (( attempt = 0; attempt <= retries; attempt++ )); do
    name=$base${${attempt:#0}:+-retry-$attempt}
    trace="$output/$name.ndjson"
    log="$output/$name.game.log"
    report="$output/$name.replay.txt"
    print "[$run/$count] $character${${attempt:#0}:+ retry $attempt}"
    set +e
    SPIREFYSH_TRACE_PATH="$trace" \
    SPIREFYSH_POLICY_CHECKPOINT="$checkpoint" \
    SPIREFYSH_POLICY_PYTHONPATH="$pythonpath" \
    SPIREFYSH_POLICY_SEED=$(( seed + attempt )) \
    SPIREFYSH_CHARACTER_ID="$character" \
    SteamAppId=2868840 SteamGameId=2868840 \
      "$game" --headless > "$log" 2>&1 &
    game_pid=$!
    ( sleep "$timeout"; kill -TERM "$game_pid" 2>/dev/null; sleep 10; kill -KILL "$game_pid" 2>/dev/null ) &
    watchdog_pid=$!
    wait "$game_pid"; game_exit=$?
    pkill -P "$watchdog_pid" 2>/dev/null; kill "$watchdog_pid" 2>/dev/null  # also stop its sleep
    wait "$watchdog_pid" 2>/dev/null
    if grep -q -e 'Spirefysh policy\] Traceback' -e 'Spirefysh\] Refusing telemetry' "$log"; then
      print -u2 "policy failed to start, see $log"
      touch "$output/.failed"
      return
    fi
    "$repo/target/release/replay" "$trace" > "$report" 2>&1
    replay_exit=$?
    jq -e 'select(.kind == "episode_completed" and .metadata.complete)' "$trace" >/dev/null
    complete=$(( $? == 0 ))
    zstd -q -15 --rm "$trace"  # relic-run traces are ~100 MB raw, ~250 KB compressed
    set -e
    print "$run\t$character\t$attempt\t$game_exit\t$replay_exit\t$complete\t$report" >> "$output/summary.tsv"
    (( replay_exit != 0 || complete )) && break
    print "[$run/$count] retrying incomplete $character"
  done
  if (( replay_exit != 0 || !complete )) && [[ ${SPIREFYSH_CONTINUE_ON_MISMATCH:-0} != 1 ]]; then
    touch "$output/.failed"
  fi
}

for (( block = 0; block * 5 < count; block++ )); do
  order=(${characters[@]})
  for (( i = 5; i > 1; i-- )); do
    j=$(( RANDOM % i + 1 ))
    swap=$order[i]; order[i]=$order[j]; order[j]=$swap
  done
  for character in $order; do
    run=$(( block * 5 + ${order[(i)$character]} ))
    (( run <= count )) || break
    while (( ${#jobstates} >= jobs )); do sleep 0.2; done
    [[ -e "$output/.failed" ]] && break 2
    sample $run $character $(( RANDOM << 16 | RANDOM )) &
  done
done
wait

print "results=$output"
[[ ! -e "$output/.failed" ]]
