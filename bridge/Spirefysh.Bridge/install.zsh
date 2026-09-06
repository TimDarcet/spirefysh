#!/bin/zsh
set -euo pipefail
setopt extendedglob

if (( $# != 4 )); then
  print -u2 "usage: bridge/Spirefysh.Bridge/install.zsh GAME_DATA_DIR TRACE.ndjson PROFILE_DIR PROFILE_SNAPSHOT.json"
  exit 2
fi

script_dir=${0:A:h}
repo_root=${script_dir:h:h}
supported_game_hash=e7ceb80669bfaf5c8fccabaa126ae2bb283aba514be5b5b55612579cfd285f18
supported_content_hash=62c887be791250b7a90c6cd929c19d03d33e17cc76aa7ff610b2889cccdadadb
supported_release_hash=93838093ff803a60a8f086355a1d1a9cb103358089f8a46ae41743ddd8919b42
marker_hash=75dc550950b5d59aa7c8bcf18e67659404dcb8f2769b95fc0ee7262d15e70bcf
test_root_argument=${SPIREFYSH_BRIDGE_INSTALL_SELF_TEST_ROOT:-}
test_hash=${SPIREFYSH_BRIDGE_INSTALL_TEST_GAME_HASH:-}
test_content_hash=${SPIREFYSH_BRIDGE_INSTALL_TEST_CONTENT_HASH:-}
test_release_hash=${SPIREFYSH_BRIDGE_INSTALL_TEST_RELEASE_HASH:-}
test_artifact_argument=${SPIREFYSH_BRIDGE_INSTALL_TEST_ARTIFACT_DIR:-}
test_game_running=${SPIREFYSH_BRIDGE_INSTALL_TEST_GAME_RUNNING:-0}

game_data_dir=${1:A}
trace_argument=$2
profile_dir=${3:A}
profile_snapshot=${4:A}
build_output=""
install_stage=""
local_mods_dir=""
created_mods_dir=false

cleanup_install_temps() {
  if [[ -n "$install_stage" && -d "$install_stage" ]]; then
    case "$install_stage" in
      */.spirefysh-bridge.install.*) rm -rf -- "$install_stage" ;;
      *) print -u2 "refusing to clean unexpected install stage: $install_stage" ;;
    esac
  fi
  if [[ -n "$build_output" && -d "$build_output" ]]; then
    case "$build_output" in
      /tmp/spirefysh-bridge-install-build.*|/private/tmp/spirefysh-bridge-install-build.*)
        rm -rf -- "$build_output"
        ;;
      *) print -u2 "refusing to clean unexpected build stage: $build_output" ;;
    esac
  fi
  if [[ "$created_mods_dir" == true && -n "$local_mods_dir" && -d "$local_mods_dir" ]]; then
    rmdir -- "$local_mods_dir" 2>/dev/null || true
  fi
}
trap cleanup_install_temps EXIT INT TERM

path_is_within() {
  local child=$1
  local parent=$2
  [[ "$child" == "$parent" || "$child" == "$parent"/* ]]
}

directory_is_empty() {
  local directory=$1
  [[ -z "$(find "$directory" -mindepth 1 -maxdepth 1 -print -quit)" ]]
}

game_is_running() {
  if [[ "$test_game_running" == 1 ]]; then
    return 0
  fi
  pgrep -x "Slay the Spire 2" >/dev/null 2>&1 ||
    pgrep -x "SlayTheSpire2" >/dev/null 2>&1
}

verify_exact_artifact_directory() {
  local directory=$1
  local entries
  entries=("$directory"/*(DN))
  if (( ${#entries} != 2 )); then
    print -u2 "bridge artifact directory must contain exactly DLL and manifest: $directory"
    return 1
  fi
  local expected
  for expected in Spirefysh.Bridge.dll Spirefysh.Bridge.json; do
    if [[ ! -f "$directory/$expected" || -L "$directory/$expected" ]]; then
      print -u2 "bridge artifact is missing or not a regular file: $directory/$expected"
      return 1
    fi
  done
  if [[ ! -s "$directory/Spirefysh.Bridge.dll" ]]; then
    print -u2 "bridge DLL is empty"
    return 1
  fi
  if ! cmp -s "$script_dir/Spirefysh.Bridge.json" "$directory/Spirefysh.Bridge.json"; then
    print -u2 "bridge manifest differs from the reviewed repository manifest"
    return 1
  fi
  if ! jq -e '
    .id == "spirefysh.bridge" and
    .min_game_version == "0.107.1" and
    .has_dll == true and .has_pck == false and
    .affects_gameplay == true and .dependencies == []
  ' "$directory/Spirefysh.Bridge.json" >/dev/null; then
    print -u2 "bridge manifest does not preserve the pinned gameplay-affecting shape"
    return 1
  fi
}

verify_exact_install_directory() {
  local directory=$1
  local entries
  entries=("$directory"/*(DN))
  if (( ${#entries} != 3 )); then
    print -u2 "installed bridge directory must contain exactly three files"
    return 1
  fi
  local expected
  for expected in Spirefysh.Bridge.dll Spirefysh.Bridge.json Spirefysh.Bridge.trace-path; do
    if [[ ! -f "$directory/$expected" || -L "$directory/$expected" ]]; then
      print -u2 "installed bridge file is missing or not regular: $expected"
      return 1
    fi
  done
  if [[ ! -s "$directory/Spirefysh.Bridge.dll" ]] ||
     ! cmp -s "$script_dir/Spirefysh.Bridge.json" "$directory/Spirefysh.Bridge.json"; then
    print -u2 "installed DLL is empty or manifest differs from the reviewed manifest"
    return 1
  fi
  local configured_trace
  configured_trace=$(<"$directory/Spirefysh.Bridge.trace-path")
  if [[ "$configured_trace" != /* || ${configured_trace:e:l} != ndjson ||
        "$(wc -l < "$directory/Spirefysh.Bridge.trace-path" | tr -d ' ')" != 1 ]]; then
    print -u2 "installed trace-path configuration must be one absolute .ndjson line"
    return 1
  fi
}

test_mode=false
test_root=""
if [[ -n "$test_root_argument" ]]; then
  if [[ "$test_root_argument" != /* || ! -d "$test_root_argument" ]]; then
    print -u2 "self-test root must be an existing absolute directory"
    exit 2
  fi
  test_root=${test_root_argument:A}
  if [[ ${test_root:t} != spirefysh-bridge-install-self-test.* ||
        ( ${test_root:h} != /tmp && ${test_root:h} != /private/tmp ) ]]; then
    print -u2 "self-test root must resolve under /tmp with the dedicated prefix"
    exit 2
  fi
  for hash in "$test_hash" "$test_content_hash" "$test_release_hash"; do
    if [[ "$hash" != [0-9a-f]## || ${#hash} != 64 ]]; then
      print -u2 "self-test hashes must be lowercase 64-character SHA-256 values"
      exit 2
    fi
  done
  if [[ "$test_game_running" != 0 && "$test_game_running" != 1 ]]; then
    print -u2 "self-test running-game flag must be 0 or 1"
    exit 2
  fi
  if [[ -z "$test_artifact_argument" || "$test_artifact_argument" != /* ||
        ! -d "$test_artifact_argument" ]]; then
    print -u2 "self-test artifact directory must be an existing absolute directory"
    exit 2
  fi
  test_artifact_dir=${test_artifact_argument:A}
  for protected_path in "$game_data_dir" "$profile_dir" "$profile_snapshot" "$test_artifact_dir"; do
    if ! path_is_within "$protected_path" "$test_root"; then
      print -u2 "self-test path escapes its temporary root: $protected_path"
      exit 2
    fi
  done
  test_mode=true
elif [[ -n "$test_hash" || -n "$test_content_hash" || -n "$test_release_hash" ||
        -n "$test_artifact_argument" || "$test_game_running" != 0 ]]; then
  print -u2 "test-only overrides require a constrained self-test root"
  exit 2
fi

if [[ "$trace_argument" != /* ]]; then
  print -u2 "trace path must be absolute"
  exit 2
fi
trace_parent_argument=${trace_argument:h}
if [[ ! -d "$trace_parent_argument" ]]; then
  print -u2 "trace parent must already exist: $trace_parent_argument"
  exit 2
fi
trace_parent=${trace_parent_argument:A}
trace_path="$trace_parent/${trace_argument:t}"
if [[ "$test_mode" == true ]] && ! path_is_within "$trace_path" "$test_root"; then
  print -u2 "self-test trace path escapes its temporary root"
  exit 2
fi

if game_is_running; then
  print -u2 "the game is running; quit it before bridge installation"
  exit 2
fi
resources_dir=${game_data_dir:h}
content_path="$resources_dir/Slay the Spire 2.pck"
release_path="$resources_dir/release_info.json"
if [[ ! -f "$game_data_dir/sts2.dll" || -L "$game_data_dir/sts2.dll" ||
      ! -f "$game_data_dir/0Harmony.dll" || -L "$game_data_dir/0Harmony.dll" ||
      ! -f "$content_path" || -L "$content_path" ||
      ! -f "$release_path" || -L "$release_path" ]]; then
  print -u2 "pinned STS2 managed runtime is missing or indirect: $game_data_dir"
  exit 2
fi
expected_game_hash=$supported_game_hash
expected_content_hash=$supported_content_hash
expected_release_hash=$supported_release_hash
if [[ "$test_mode" == true ]]; then
  expected_game_hash=$test_hash
  expected_content_hash=$test_content_hash
  expected_release_hash=$test_release_hash
fi
actual_game_hash=$(shasum -a 256 "$game_data_dir/sts2.dll" | cut -d ' ' -f 1)
actual_content_hash=$(shasum -a 256 "$content_path" | cut -d ' ' -f 1)
actual_release_hash=$(shasum -a 256 "$release_path" | cut -d ' ' -f 1)
if [[ "$actual_game_hash" != "$expected_game_hash" ]]; then
  print -u2 "unsupported sts2.dll SHA-256: $actual_game_hash"
  exit 2
fi
if [[ "$actual_content_hash" != "$expected_content_hash" ]]; then
  print -u2 "unsupported content archive SHA-256: $actual_content_hash"
  exit 2
fi
if [[ "$actual_release_hash" != "$expected_release_hash" ]]; then
  print -u2 "unsupported release metadata SHA-256: $actual_release_hash"
  exit 2
fi

if [[ ${trace_path:e:l} != ndjson || ${trace_path:t} == .* ]]; then
  print -u2 "trace path must be a non-hidden .ndjson file"
  exit 2
fi
if [[ "$trace_path" == *$'\n'* || "$trace_path" == *$'\r'* ]]; then
  print -u2 "trace path may not contain line breaks"
  exit 2
fi
if [[ -e "$trace_path" || -L "$trace_path" ]]; then
  print -u2 "trace path must be unused: $trace_path"
  exit 2
fi
marker_path="$trace_parent/.spirefysh-oracle-output"
if [[ ! -f "$marker_path" || -L "$marker_path" ]]; then
  print -u2 "trace parent is missing a regular $marker_path"
  exit 2
fi
actual_marker_hash=$(shasum -a 256 "$marker_path" | cut -d ' ' -f 1)
if [[ "$actual_marker_hash" != "$marker_hash" ]]; then
  print -u2 "oracle-output marker has unexpected content"
  exit 2
fi

if [[ ! -d "$profile_dir" || -L "$profile_dir" ||
      ! -f "$profile_snapshot" || -L "$profile_snapshot" ]]; then
  print -u2 "profile directory or baseline snapshot is missing or indirect"
  exit 2
fi
settings_path="$profile_dir/settings.save"
if [[ ! -f "$settings_path" || -L "$settings_path" ]]; then
  print -u2 "profile settings are missing or indirect: $settings_path"
  exit 2
fi

game_contents=${game_data_dir:h:h}
if path_is_within "$trace_path" "$profile_dir"; then
  print -u2 "trace output may not be inside the protected profile"
  exit 2
fi
if path_is_within "$trace_path" "$game_contents"; then
  print -u2 "trace output may not be inside the game installation"
  exit 2
fi
if path_is_within "$profile_snapshot" "$profile_dir" ||
   path_is_within "$profile_snapshot" "$game_contents"; then
  print -u2 "profile baseline must be outside the profile and game installation"
  exit 2
fi

/bin/zsh "$script_dir/validate-mod-settings.zsh" "$settings_path"
if [[ "$test_mode" == true ]]; then
  tools_dll="$repo_root/tools/Spirefysh.Tools/bin/Debug/net10.0/Spirefysh.Tools.dll"
  if [[ ! -f "$tools_dll" ]]; then
    print -u2 "self-test tools artifact is missing: $tools_dll"
    exit 2
  fi
  dotnet "$tools_dll" profile verify "$profile_dir" "$profile_snapshot"
else
  dotnet run --project "$repo_root/tools/Spirefysh.Tools" -- \
    profile verify "$profile_dir" "$profile_snapshot"
fi

local_mods_dir="$game_contents/MacOS/mods"
install_dir="$local_mods_dir/spirefysh-bridge"
if [[ -L "$local_mods_dir" || ( -e "$local_mods_dir" && ! -d "$local_mods_dir" ) ]]; then
  print -u2 "local mods path is not a direct directory: $local_mods_dir"
  exit 2
fi
if [[ -e "$install_dir" || -L "$install_dir" ]]; then
  print -u2 "a bridge installation already exists: $install_dir"
  exit 2
fi
if [[ -d "$local_mods_dir" ]] && ! directory_is_empty "$local_mods_dir"; then
  print -u2 "local mods directory is not empty; installation requires exclusive ownership: $local_mods_dir"
  exit 2
fi

artifact_dir=""
if [[ "$test_mode" == true ]]; then
  artifact_dir=$test_artifact_dir
else
  build_output=$(mktemp -d /tmp/spirefysh-bridge-install-build.XXXXXX)
  SPIREFYSH_BRIDGE_OUTPUT_DIR="$build_output" \
    SPIREFYSH_ADVISOR_PACKAGE=0 /bin/zsh "$script_dir/build.zsh" "$game_data_dir"
  artifact_dir=$build_output
fi
verify_exact_artifact_directory "$artifact_dir"
if [[ "$test_mode" != true ]]; then
  dotnet run --project "$repo_root/tools/Spirefysh.Tools" -- \
    bridge "$game_data_dir/sts2.dll" "$artifact_dir/Spirefysh.Bridge.dll"
fi

if [[ ! -d "$local_mods_dir" ]]; then
  mkdir -p -- "${local_mods_dir:h}"
  mkdir -- "$local_mods_dir"
  created_mods_dir=true
fi
if ! directory_is_empty "$local_mods_dir" || [[ -e "$install_dir" || -L "$install_dir" ]]; then
  print -u2 "local mods state changed during installation; refusing to continue"
  exit 2
fi

install_stage=$(mktemp -d "$local_mods_dir/.spirefysh-bridge.install.XXXXXX")
cp -- "$artifact_dir/Spirefysh.Bridge.dll" "$install_stage/Spirefysh.Bridge.dll"
cp -- "$artifact_dir/Spirefysh.Bridge.json" "$install_stage/Spirefysh.Bridge.json"
print -r -- "$trace_path" > "$install_stage/Spirefysh.Bridge.trace-path"
verify_exact_install_directory "$install_stage"
if ! cmp -s "$artifact_dir/Spirefysh.Bridge.dll" "$install_stage/Spirefysh.Bridge.dll"; then
  print -u2 "staged bridge DLL differs from the validated build artifact"
  exit 2
fi
if [[ "$(<"$install_stage/Spirefysh.Bridge.trace-path")" != "$trace_path" ]]; then
  print -u2 "staged trace-path configuration changed"
  exit 2
fi

unexpected_entries=("$local_mods_dir"/*(DN))
if (( ${#unexpected_entries} != 1 )) || [[ "$unexpected_entries[1]" != "$install_stage" ]]; then
  print -u2 "local mods state changed while staging; refusing atomic rename"
  exit 2
fi
if [[ -e "$install_dir" || -L "$install_dir" ]]; then
  print -u2 "bridge install target appeared while staging"
  exit 2
fi
/bin/mv -n -- "$install_stage" "$install_dir"
if [[ -e "$install_stage" || ! -d "$install_dir" || -L "$install_dir" ]]; then
  print -u2 "atomic bridge install rename did not complete"
  exit 2
fi
install_stage=""
created_mods_dir=false
verify_exact_install_directory "$install_dir"
if ! cmp -s "$artifact_dir/Spirefysh.Bridge.dll" "$install_dir/Spirefysh.Bridge.dll"; then
  print -u2 "installed bridge DLL differs from the validated build artifact"
  exit 2
fi

print "bridge-install valid=true"
print "game_sha256=$actual_game_hash"
print "content_sha256=$actual_content_hash"
print "release_sha256=$actual_release_hash"
print "install_dir=$install_dir"
print "trace_path=$trace_path"
print "installed_files=Spirefysh.Bridge.dll,Spirefysh.Bridge.json,Spirefysh.Bridge.trace-path"
