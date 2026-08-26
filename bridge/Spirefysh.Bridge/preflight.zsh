#!/bin/zsh
set -euo pipefail

if (( $# != 4 )); then
  print -u2 "usage: bridge/Spirefysh.Bridge/preflight.zsh GAME_DATA_DIR TRACE.ndjson PROFILE_DIR PROFILE_SNAPSHOT.json"
  exit 2
fi

script_dir=${0:A:h}
repo_root=${script_dir:h:h}
game_data_dir=${1:A}
trace_argument=$2
profile_dir=${3:A}
profile_snapshot=${4:A}
supported_game_hash=e7ceb80669bfaf5c8fccabaa126ae2bb283aba514be5b5b55612579cfd285f18
supported_content_hash=62c887be791250b7a90c6cd929c19d03d33e17cc76aa7ff610b2889cccdadadb
supported_release_hash=93838093ff803a60a8f086355a1d1a9cb103358089f8a46ae41743ddd8919b42
marker_hash=75dc550950b5d59aa7c8bcf18e67659404dcb8f2769b95fc0ee7262d15e70bcf

if [[ "$trace_argument" != /* ]]; then
  print -u2 "trace path must be absolute"
  exit 2
fi
trace_path=${trace_argument:a}
trace_parent=${trace_path:h}

resources_dir=${game_data_dir:h}
content_path="$resources_dir/Slay the Spire 2.pck"
release_path="$resources_dir/release_info.json"
if [[ ! -f "$game_data_dir/sts2.dll" || -L "$game_data_dir/sts2.dll" ||
      ! -f "$game_data_dir/0Harmony.dll" || -L "$game_data_dir/0Harmony.dll" ||
      ! -f "$content_path" || -L "$content_path" ||
      ! -f "$release_path" || -L "$release_path" ]]; then
  print -u2 "pinned STS2 managed runtime is missing: $game_data_dir"
  exit 2
fi
actual_game_hash=$(shasum -a 256 "$game_data_dir/sts2.dll" | cut -d ' ' -f 1)
actual_content_hash=$(shasum -a 256 "$content_path" | cut -d ' ' -f 1)
actual_release_hash=$(shasum -a 256 "$release_path" | cut -d ' ' -f 1)
if [[ "$actual_game_hash" != "$supported_game_hash" ]]; then
  print -u2 "unsupported sts2.dll SHA-256: $actual_game_hash"
  exit 2
fi
if [[ "$actual_content_hash" != "$supported_content_hash" ]]; then
  print -u2 "unsupported content archive SHA-256: $actual_content_hash"
  exit 2
fi
if [[ "$actual_release_hash" != "$supported_release_hash" ]]; then
  print -u2 "unsupported release metadata SHA-256: $actual_release_hash"
  exit 2
fi
if pgrep -x "Slay the Spire 2" >/dev/null 2>&1 ||
   pgrep -x "SlayTheSpire2" >/dev/null 2>&1; then
  print -u2 "the game is running; quit it before preflight or installation"
  exit 2
fi

if [[ ${trace_path:e:l} != ndjson ]]; then
  print -u2 "trace path must end in .ndjson"
  exit 2
fi
if [[ ! -d "$trace_parent" ]]; then
  print -u2 "trace parent must already exist: $trace_parent"
  exit 2
fi
if [[ -e "$trace_path" ]]; then
  print -u2 "trace path must be unused: $trace_path"
  exit 2
fi
marker_path="$trace_parent/.spirefysh-oracle-output"
if [[ ! -f "$marker_path" ]]; then
  print -u2 "trace parent is missing $marker_path"
  exit 2
fi
actual_marker_hash=$(shasum -a 256 "$marker_path" | cut -d ' ' -f 1)
if [[ "$actual_marker_hash" != "$marker_hash" ]]; then
  print -u2 "oracle-output marker has unexpected content"
  exit 2
fi
if [[ ! -d "$profile_dir" || ! -f "$profile_snapshot" ]]; then
  print -u2 "profile directory or baseline snapshot is missing"
  exit 2
fi
settings_path="$profile_dir/settings.save"
if [[ ! -f "$settings_path" ]]; then
  print -u2 "profile settings are missing: $settings_path"
  exit 2
fi
zsh "$script_dir/validate-mod-settings.zsh" "$settings_path"
if [[ "$trace_path" == "$profile_dir"/* ]]; then
  print -u2 "trace output may not be inside the protected profile"
  exit 2
fi

game_contents=${game_data_dir:h:h}
if [[ "$trace_path" == "$game_contents"/* ]]; then
  print -u2 "trace output may not be inside the game installation"
  exit 2
fi
local_mods_dir="$game_contents/MacOS/mods"
install_dir="$local_mods_dir/spirefysh-bridge"
if [[ -e "$install_dir" ]]; then
  print -u2 "a bridge installation already exists: $install_dir"
  exit 2
fi
if [[ -d "$local_mods_dir" && -n "$(find "$local_mods_dir" -mindepth 1 -maxdepth 1 -print -quit)" ]]; then
  print -u2 "local mods directory is not empty; preflight requires exclusive local-mod ownership: $local_mods_dir"
  exit 2
fi

dotnet run --project "$repo_root/tools/Spirefysh.ProfileGuard" -- \
  verify "$profile_dir" "$profile_snapshot"

build_output=$(mktemp -d /tmp/spirefysh-bridge-preflight.XXXXXX)
trap 'rm -rf -- "$build_output"' EXIT INT TERM
SPIREFYSH_ADVISOR_PACKAGE=0 SPIREFYSH_BRIDGE_OUTPUT_DIR="$build_output" zsh "$script_dir/build.zsh" "$game_data_dir"
dotnet run --project "$repo_root/tools/Spirefysh.OracleProbe" -- \
  "$game_data_dir/sts2.dll" validate-bridge "$build_output/Spirefysh.Bridge.dll"

print "live-oracle preflight valid=true"
print "game_sha256=$actual_game_hash"
print "content_sha256=$actual_content_hash"
print "release_sha256=$actual_release_hash"
print "install_dir=$install_dir"
print "trace_path=$trace_path"
print "profile_snapshot=$profile_snapshot"
print "enabled_mods=spirefysh.bridge-only-or-unlisted"
print "note=No game files, mods, traces, profiles, or saves were changed."
