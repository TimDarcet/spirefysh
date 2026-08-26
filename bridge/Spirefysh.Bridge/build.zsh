#!/bin/zsh
set -euo pipefail

if (( $# > 2 )); then
  print -u2 "usage: bridge/Spirefysh.Bridge/build.zsh [GAME_DATA_DIR] [MODEL.pt]"
  exit 2
fi

script_dir=${0:A:h}
repo_root=${script_dir:h:h}
game_data_dir=${${1:-"/Users/timdarcet/Library/Application Support/Steam/steamapps/common/Slay the Spire 2/SlayTheSpire2.app/Contents/Resources/data_sts2_macos_arm64"}:A}
if [[ ! -f "$game_data_dir/sts2.dll" || ! -f "$game_data_dir/0Harmony.dll" ]]; then
  print -u2 "STS2 managed runtime not found at: $game_data_dir"
  exit 2
fi
supported_game_hash=e7ceb80669bfaf5c8fccabaa126ae2bb283aba514be5b5b55612579cfd285f18
actual_game_hash=$(shasum -a 256 "$game_data_dir/sts2.dll" | cut -d ' ' -f 1)
if [[ "$actual_game_hash" != "$supported_game_hash" ]]; then
  print -u2 "unsupported sts2.dll SHA-256: $actual_game_hash"
  exit 2
fi

compiler_path=$(find /opt/homebrew/Cellar/dotnet -path '*/Roslyn/bincore/csc.dll' -print | sort -V | tail -1)
if [[ -z "$compiler_path" ]]; then
  print -u2 "Roslyn compiler not found"
  exit 2
fi

output_dir=${SPIREFYSH_BRIDGE_OUTPUT_DIR:-"$script_dir/artifacts"}
if [[ "$output_dir" != /* ]]; then
  print -u2 "SPIREFYSH_BRIDGE_OUTPUT_DIR must be absolute"
  exit 2
fi
output_dir=${output_dir:a}
game_contents=${game_data_dir:h:h}
if [[ "$output_dir" == "$game_contents" || "$output_dir" == "$game_contents"/* ]]; then
  print -u2 "bridge build output must be outside the game installation"
  exit 2
fi
mkdir -p "$output_dir"

references=()
for assembly in "$game_data_dir"/*.dll; do
  references+=("-reference:$assembly")
done

sources=("${0:A:h}"/*.cs)
dotnet "$compiler_path" \
  -noconfig -nostdlib+ -target:library -deterministic+ -optimize+ \
  -langversion:latest -nullable:enable -warnaserror+ \
  -out:"$output_dir/Spirefysh.Bridge.dll" \
  "${references[@]}" "${sources[@]}"
cp "${0:A:h}/Spirefysh.Bridge.json" "$output_dir/Spirefysh.Bridge.json"
if [[ ${SPIREFYSH_ADVISOR_PACKAGE:-1} == 0 ]]; then
  print "$output_dir/Spirefysh.Bridge.dll"
  exit
fi
python=${SPIREFYSH_PYTHON:-"$repo_root/.venv/bin/python"}
model=${2:-${SPIREFYSH_MODEL_PATH:-}}
if [[ -z "$model" ]]; then
  if pgrep -f '[t]rain.py train' >/dev/null 2>&1; then
    print -u2 "training is active; pass the completed MODEL.pt explicitly"
    exit 2
  fi
  model=$(find "$repo_root/target" -type f -name model.pt -exec stat -f '%m %N' {} + | sort -nr | head -1 | cut -d' ' -f2-)
fi
model=${model:A}
if [[ ! -x "$python" || ! -f "$model" ]]; then
  print -u2 "advisor Python or model is missing: python=$python model=$model"
  exit 2
fi
"$repo_root/.venv/bin/maturin" develop --release --features python --manifest-path "$repo_root/Cargo.toml"
cp "$script_dir/advisor.py" "$output_dir/advisor.py"
"$python" "$script_dir/advisor.py" export "$model" "$output_dir/Spirefysh.Bridge.model.pt"
jq -n --arg python "$python" --arg source "$model" '{show_win_probability_delta:true, show_current_win_probability:true, python:$python, model:"Spirefysh.Bridge.model.pt", source_model:$source}' > "$output_dir/Spirefysh.Bridge.advisor-config"
print "$output_dir/Spirefysh.Bridge.dll"
