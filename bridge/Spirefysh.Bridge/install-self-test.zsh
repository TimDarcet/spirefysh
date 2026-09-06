#!/bin/zsh
set -euo pipefail
setopt extendedglob

if (( $# != 0 )); then
  print -u2 "usage: bridge/Spirefysh.Bridge/install-self-test.zsh"
  exit 2
fi

script_dir=${0:A:h}
repo_root=${script_dir:h:h}
install_script="$script_dir/install.zsh"
disable_script="$script_dir/disable.zsh"
self_test_root=$(mktemp -d /tmp/spirefysh-bridge-install-self-test.XXXXXX)
cases=0
tools_dll="$repo_root/tools/Spirefysh.Tools/bin/Debug/net10.0/Spirefysh.Tools.dll"

cleanup_self_test() {
  case "$self_test_root" in
    /tmp/spirefysh-bridge-install-self-test.*|/private/tmp/spirefysh-bridge-install-self-test.*)
      rm -rf -- "$self_test_root"
      ;;
    *) print -u2 "refusing to clean unexpected self-test root: $self_test_root" ;;
  esac
}
trap cleanup_self_test EXIT INT TERM

dotnet build "$repo_root/tools/Spirefysh.Tools/Spirefysh.Tools.csproj" \
  --no-restore >/dev/null

require() {
  if ! eval "$1"; then
    print -u2 "self-test assertion failed: $2"
    exit 1
  fi
}

make_fixture() {
  local label=$1
  local settings_fixture=${2:-valid-unlisted}
  fixture_case="$self_test_root/$label"
  fixture_game_data="$fixture_case/game/Contents/Resources/data_sts2_macos_arm64"
  fixture_contents="$fixture_case/game/Contents"
  fixture_mods="$fixture_contents/MacOS/mods"
  fixture_install="$fixture_mods/spirefysh-bridge"
  fixture_profile="$fixture_case/profile/account"
  fixture_output="$fixture_case/output"
  fixture_trace="$fixture_output/capture.ndjson"
  fixture_baseline="$fixture_case/profile-baseline.json"
  fixture_artifacts="$fixture_case/artifacts"
  fixture_archive_parent="$fixture_case/archive"
  fixture_archive="$fixture_archive_parent/removed-spirefysh-bridge"

  mkdir -p -- "$fixture_game_data" "$fixture_contents/MacOS" "$fixture_profile" \
    "$fixture_output" "$fixture_artifacts" "$fixture_archive_parent"
  fixture_trace_resolved="${fixture_output:A}/${fixture_trace:t}"
  print -r -- "fixture-sts2-$label" > "$fixture_game_data/sts2.dll"
  print -r -- "fixture-harmony-$label" > "$fixture_game_data/0Harmony.dll"
  print -r -- "fixture-content-$label" > "${fixture_game_data:h}/Slay the Spire 2.pck"
  print -r -- "fixture-release-$label" > "${fixture_game_data:h}/release_info.json"
  cp -- "$script_dir/fixtures/settings-$settings_fixture.json" "$fixture_profile/settings.save"
  cp -- "$script_dir/oracle-output.marker" "$fixture_output/.spirefysh-oracle-output"
  print -r -- "fixture-bridge-dll-$label" > "$fixture_artifacts/Spirefysh.Bridge.dll"
  cp -- "$script_dir/Spirefysh.Bridge.json" "$fixture_artifacts/Spirefysh.Bridge.json"
  fixture_hash=$(shasum -a 256 "$fixture_game_data/sts2.dll" | cut -d ' ' -f 1)
  fixture_content_hash=$(shasum -a 256 "${fixture_game_data:h}/Slay the Spire 2.pck" | cut -d ' ' -f 1)
  fixture_release_hash=$(shasum -a 256 "${fixture_game_data:h}/release_info.json" | cut -d ' ' -f 1)
  dotnet "$tools_dll" profile snapshot "$fixture_profile" "$fixture_baseline" >/dev/null
}

run_fixture_install() {
  local running=${1:-0}
  SPIREFYSH_BRIDGE_INSTALL_SELF_TEST_ROOT="$self_test_root" \
  SPIREFYSH_BRIDGE_INSTALL_TEST_GAME_HASH="$fixture_hash" \
  SPIREFYSH_BRIDGE_INSTALL_TEST_CONTENT_HASH="$fixture_content_hash" \
  SPIREFYSH_BRIDGE_INSTALL_TEST_RELEASE_HASH="$fixture_release_hash" \
  SPIREFYSH_BRIDGE_INSTALL_TEST_ARTIFACT_DIR="$fixture_artifacts" \
  SPIREFYSH_BRIDGE_INSTALL_TEST_GAME_RUNNING="$running" \
    /bin/zsh "$install_script" \
      "$fixture_game_data" "$fixture_trace" "$fixture_profile" "$fixture_baseline"
}

run_fixture_disable() {
  local running=${1:-0}
  SPIREFYSH_BRIDGE_INSTALL_SELF_TEST_ROOT="$self_test_root" \
  SPIREFYSH_BRIDGE_INSTALL_TEST_GAME_HASH="$fixture_hash" \
  SPIREFYSH_BRIDGE_INSTALL_TEST_GAME_RUNNING="$running" \
    /bin/zsh "$disable_script" "$fixture_game_data" "$fixture_archive"
}

assert_no_install_stage() {
  local stages
  stages=("$fixture_mods"/.spirefysh-bridge.install.*(N))
  if (( ${#stages} != 0 )); then
    print -u2 "temporary install stage leaked in fixture $fixture_case"
    exit 1
  fi
}

expect_install_failure() {
  local label=$1
  shift
  if "$@" >/dev/null 2>&1; then
    print -u2 "expected install case '$label' to fail closed"
    exit 1
  fi
  assert_no_install_stage
  if [[ "$label" != existing-install && ( -e "$fixture_install" || -L "$fixture_install" ) ]]; then
    print -u2 "failed install case '$label' left a dedicated install behind"
    exit 1
  fi
  (( cases += 1 ))
}

expect_disable_failure() {
  local label=$1
  shift
  if "$@" >/dev/null 2>&1; then
    print -u2 "expected disable case '$label' to fail closed"
    exit 1
  fi
  if [[ ! -d "$fixture_install" || -L "$fixture_install" ]]; then
    print -u2 "disable failure '$label' changed the exact install directory"
    exit 1
  fi
  (( cases += 1 ))
}

# Complete install/disable round trip. The archive remains intact until the temporary root cleanup.
make_fixture success
run_fixture_install >/dev/null
installed_entries=("$fixture_install"/*(DN))
require '(( ${#installed_entries} == 3 ))' "success install did not contain exactly three files"
require '[[ "$(<"$fixture_install/Spirefysh.Bridge.trace-path")" == "$fixture_trace_resolved" ]]' \
  "installed trace path changed"
mods_entries=("$fixture_mods"/*(DN))
require '(( ${#mods_entries} == 1 )) && [[ "$mods_entries[1]" == "$fixture_install" ]]' \
  "success install did not retain exclusive local-mod ownership"
run_fixture_disable >/dev/null
require '[[ ! -e "$fixture_install" && -d "$fixture_archive" ]]' \
  "disable did not move the exact install to its explicit archive"
archived_entries=("$fixture_archive"/*(DN))
require '(( ${#archived_entries} == 3 ))' "archive did not retain all exact install files"
(( cases += 1 ))

make_fixture wrong-hash
wrong_hash=0000000000000000000000000000000000000000000000000000000000000000
expect_install_failure wrong-hash env \
  SPIREFYSH_BRIDGE_INSTALL_SELF_TEST_ROOT="$self_test_root" \
  SPIREFYSH_BRIDGE_INSTALL_TEST_GAME_HASH="$wrong_hash" \
  SPIREFYSH_BRIDGE_INSTALL_TEST_CONTENT_HASH="$fixture_content_hash" \
  SPIREFYSH_BRIDGE_INSTALL_TEST_RELEASE_HASH="$fixture_release_hash" \
  SPIREFYSH_BRIDGE_INSTALL_TEST_ARTIFACT_DIR="$fixture_artifacts" \
  /bin/zsh "$install_script" \
    "$fixture_game_data" "$fixture_trace" "$fixture_profile" "$fixture_baseline"

make_fixture wrong-content
print -r -- "tampered" > "${fixture_game_data:h}/Slay the Spire 2.pck"
expect_install_failure wrong-content run_fixture_install

make_fixture wrong-release
print -r -- "tampered" > "${fixture_game_data:h}/release_info.json"
expect_install_failure wrong-release run_fixture_install

make_fixture running-game
expect_install_failure running-game run_fixture_install 1

make_fixture wrong-marker
print -r -- "wrong-marker" > "$fixture_output/.spirefysh-oracle-output"
expect_install_failure wrong-marker run_fixture_install

make_fixture existing-trace
print -r -- "existing" > "$fixture_trace"
expect_install_failure existing-trace run_fixture_install

make_fixture trace-in-game
mkdir -p -- "$fixture_contents/capture"
cp -- "$script_dir/oracle-output.marker" "$fixture_contents/capture/.spirefysh-oracle-output"
trace_inside_game="$fixture_contents/capture/capture.ndjson"
expect_install_failure trace-in-game env \
  SPIREFYSH_BRIDGE_INSTALL_SELF_TEST_ROOT="$self_test_root" \
  SPIREFYSH_BRIDGE_INSTALL_TEST_GAME_HASH="$fixture_hash" \
  SPIREFYSH_BRIDGE_INSTALL_TEST_CONTENT_HASH="$fixture_content_hash" \
  SPIREFYSH_BRIDGE_INSTALL_TEST_RELEASE_HASH="$fixture_release_hash" \
  SPIREFYSH_BRIDGE_INSTALL_TEST_ARTIFACT_DIR="$fixture_artifacts" \
  /bin/zsh "$install_script" \
    "$fixture_game_data" "$trace_inside_game" "$fixture_profile" "$fixture_baseline"

make_fixture trace-in-profile
mkdir -p -- "$fixture_profile/capture"
cp -- "$script_dir/oracle-output.marker" "$fixture_profile/capture/.spirefysh-oracle-output"
trace_inside_profile="$fixture_profile/capture/capture.ndjson"
expect_install_failure trace-in-profile env \
  SPIREFYSH_BRIDGE_INSTALL_SELF_TEST_ROOT="$self_test_root" \
  SPIREFYSH_BRIDGE_INSTALL_TEST_GAME_HASH="$fixture_hash" \
  SPIREFYSH_BRIDGE_INSTALL_TEST_CONTENT_HASH="$fixture_content_hash" \
  SPIREFYSH_BRIDGE_INSTALL_TEST_RELEASE_HASH="$fixture_release_hash" \
  SPIREFYSH_BRIDGE_INSTALL_TEST_ARTIFACT_DIR="$fixture_artifacts" \
  /bin/zsh "$install_script" \
    "$fixture_game_data" "$trace_inside_profile" "$fixture_profile" "$fixture_baseline"

make_fixture enabled-other enabled-other
expect_install_failure enabled-other run_fixture_install

make_fixture profile-drift
print -r -- "drift" > "$fixture_profile/drift.save"
expect_install_failure profile-drift run_fixture_install

make_fixture existing-install
mkdir -p -- "$fixture_install"
print -r -- "preserve" > "$fixture_install/user-owned-marker"
expect_install_failure existing-install run_fixture_install
require '[[ "$(<"$fixture_install/user-owned-marker")" == preserve ]]' \
  "existing install refusal modified its contents"

make_fixture nonempty-mods
mkdir -p -- "$fixture_mods/another-mod"
print -r -- "preserve" > "$fixture_mods/another-mod/marker"
expect_install_failure nonempty-mods run_fixture_install
require '[[ "$(<"$fixture_mods/another-mod/marker")" == preserve ]]' \
  "nonempty mods refusal modified another mod"

make_fixture extra-artifact
print -r -- "unexpected" > "$fixture_artifacts/extra.file"
expect_install_failure extra-artifact run_fixture_install

make_fixture disable-running
run_fixture_install >/dev/null
expect_disable_failure disable-running run_fixture_disable 1

make_fixture disable-existing-archive
run_fixture_install >/dev/null
mkdir -p -- "$fixture_archive"
expect_disable_failure disable-existing-archive run_fixture_disable

make_fixture disable-malformed-install
run_fixture_install >/dev/null
print -r -- "unexpected" > "$fixture_install/extra.file"
expect_disable_failure disable-malformed-install run_fixture_disable

print "bridge-install self-test valid=true cases=$cases failures=0"
print "scope=$self_test_root"
print "note=All game, profile, trace, artifact, install, and archive fixtures were confined to the temporary scope."
