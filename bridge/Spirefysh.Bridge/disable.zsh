#!/bin/zsh
set -euo pipefail
setopt extendedglob

if (( $# != 2 )); then
  print -u2 "usage: bridge/Spirefysh.Bridge/disable.zsh GAME_DATA_DIR ARCHIVE_PATH"
  exit 2
fi

script_dir=${0:A:h}
supported_game_hash=e7ceb80669bfaf5c8fccabaa126ae2bb283aba514be5b5b55612579cfd285f18
test_root_argument=${SPIREFYSH_BRIDGE_INSTALL_SELF_TEST_ROOT:-}
test_hash=${SPIREFYSH_BRIDGE_INSTALL_TEST_GAME_HASH:-}
test_game_running=${SPIREFYSH_BRIDGE_INSTALL_TEST_GAME_RUNNING:-0}
game_data_dir=${1:A}
archive_argument=$2

path_is_within() {
  local child=$1
  local parent=$2
  [[ "$child" == "$parent" || "$child" == "$parent"/* ]]
}

game_is_running() {
  if [[ "$test_game_running" == 1 ]]; then
    return 0
  fi
  pgrep -x "Slay the Spire 2" >/dev/null 2>&1 ||
    pgrep -x "SlayTheSpire2" >/dev/null 2>&1
}

verify_exact_install_directory() {
  local directory=$1
  local entries
  entries=("$directory"/*(DN))
  if (( ${#entries} != 3 && ${#entries} != 6 )); then
    print -u2 "bridge install must contain exactly three telemetry files or six advisor files"
    return 1
  fi
  local expected
  for expected in Spirefysh.Bridge.dll Spirefysh.Bridge.json; do
    if [[ ! -f "$directory/$expected" || -L "$directory/$expected" ]]; then
      print -u2 "bridge install file is missing or not regular: $expected"
      return 1
    fi
  done
  if [[ ! -s "$directory/Spirefysh.Bridge.dll" ]]; then
    print -u2 "bridge DLL is empty"
    return 1
  fi
  if ! cmp -s "$script_dir/Spirefysh.Bridge.json" "$directory/Spirefysh.Bridge.json"; then
    print -u2 "installed bridge manifest differs from the reviewed manifest"
    return 1
  fi
  if (( ${#entries} == 3 )); then
    local configured_trace
    configured_trace=$(<"$directory/Spirefysh.Bridge.trace-path")
    if [[ "$configured_trace" != /* || ${configured_trace:e:l} != ndjson ||
          "$(wc -l < "$directory/Spirefysh.Bridge.trace-path" | tr -d ' ')" != 1 ]]; then
      print -u2 "installed trace-path configuration is malformed"
      return 1
    fi
  else
    for expected in Spirefysh.Bridge.advisor-config Spirefysh.Bridge.model.pt advisor.py model.py; do
      if [[ ! -f "$directory/$expected" || -L "$directory/$expected" ]]; then
        print -u2 "installed advisor file is missing or not regular: $expected"
        return 1
      fi
    done
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
  if [[ -z "$test_hash" || "$test_hash" != [0-9a-f]## || ${#test_hash} != 64 ]]; then
    print -u2 "self-test game hash must be a lowercase 64-character SHA-256"
    exit 2
  fi
  if [[ "$test_game_running" != 0 && "$test_game_running" != 1 ]]; then
    print -u2 "self-test running-game flag must be 0 or 1"
    exit 2
  fi
  if ! path_is_within "$game_data_dir" "$test_root"; then
    print -u2 "self-test game path escapes its temporary root"
    exit 2
  fi
  test_mode=true
elif [[ -n "$test_hash" || "$test_game_running" != 0 ]]; then
  print -u2 "test-only overrides require a constrained self-test root"
  exit 2
fi

if [[ "$archive_argument" != /* ]]; then
  print -u2 "archive path must be absolute"
  exit 2
fi
archive_parent_argument=${archive_argument:h}
if [[ ! -d "$archive_parent_argument" || -L "$archive_parent_argument" ]]; then
  print -u2 "archive parent must be an existing direct directory: $archive_parent_argument"
  exit 2
fi
archive_parent=${archive_parent_argument:A}
archive_path="$archive_parent/${archive_argument:t}"
if [[ "$test_mode" == true ]] && ! path_is_within "$archive_path" "$test_root"; then
  print -u2 "self-test archive path escapes its temporary root"
  exit 2
fi
if [[ -e "$archive_path" || -L "$archive_path" ]]; then
  print -u2 "archive target must not already exist: $archive_path"
  exit 2
fi

if game_is_running; then
  print -u2 "the game is running; quit it before disabling the bridge"
  exit 2
fi
if [[ ! -f "$game_data_dir/sts2.dll" || -L "$game_data_dir/sts2.dll" ||
      ! -f "$game_data_dir/0Harmony.dll" || -L "$game_data_dir/0Harmony.dll" ]]; then
  print -u2 "pinned STS2 managed runtime is missing or indirect: $game_data_dir"
  exit 2
fi
expected_game_hash=$supported_game_hash
if [[ "$test_mode" == true ]]; then
  expected_game_hash=$test_hash
fi
actual_game_hash=$(shasum -a 256 "$game_data_dir/sts2.dll" | cut -d ' ' -f 1)
if [[ "$actual_game_hash" != "$expected_game_hash" ]]; then
  print -u2 "unsupported sts2.dll SHA-256: $actual_game_hash"
  exit 2
fi

game_contents=${game_data_dir:h:h}
local_mods_dir="$game_contents/MacOS/mods"
install_dir="$local_mods_dir/spirefysh-bridge"
if [[ ! -d "$install_dir" || -L "$install_dir" ]]; then
  print -u2 "exact dedicated bridge install is absent or indirect: $install_dir"
  exit 2
fi
if path_is_within "$archive_path" "$game_contents"; then
  print -u2 "archive path must be outside the game installation"
  exit 2
fi
verify_exact_install_directory "$install_dir"

/bin/mv -n -- "$install_dir" "$archive_path"
if [[ -e "$install_dir" || -L "$install_dir" || ! -d "$archive_path" || -L "$archive_path" ]]; then
  print -u2 "bridge archive move did not complete exactly"
  exit 2
fi
verify_exact_install_directory "$archive_path"

print "bridge-disable valid=true"
print "game_sha256=$actual_game_hash"
print "disabled_install_dir=$install_dir"
print "archive_path=$archive_path"
print "note=No files were deleted; the exact dedicated install directory was moved intact."
