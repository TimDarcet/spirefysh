#!/bin/zsh
set -euo pipefail

script_dir=${0:A:h}
game_data_dir=${${1:-"/Users/timdarcet/Library/Application Support/Steam/steamapps/common/Slay the Spire 2/SlayTheSpire2.app/Contents/Resources/data_sts2_macos_arm64"}:A}
model=${2:-}
if pgrep -x "Slay the Spire 2" >/dev/null 2>&1 || pgrep -x "SlayTheSpire2" >/dev/null 2>&1; then
  print -u2 "quit the game before installing Spirefysh"
  exit 2
fi

output=$(mktemp -d /tmp/spirefysh-advisor-build.XXXXXX)
trap 'rm -rf -- "$output"' EXIT INT TERM
args=("$game_data_dir")
[[ -n "$model" ]] && args+=("$model")
SPIREFYSH_ADVISOR_PACKAGE=1 SPIREFYSH_BRIDGE_OUTPUT_DIR=$output zsh "$script_dir/build.zsh" "${args[@]}"

mods_dir=${game_data_dir:h:h}/MacOS/mods
install_dir=$mods_dir/spirefysh-bridge
stage=$mods_dir/.spirefysh-bridge.install.$$
backup_dir=${mods_dir:h}/mod-backups
mkdir -p "$mods_dir" "$stage" "$backup_dir"
for stale in "$mods_dir"/spirefysh-bridge.backup.*(N); do
  mv "$stale" "$backup_dir/"
done
cp "$output"/Spirefysh.Bridge.{dll,json,advisor-config,model.pt} "$output/advisor.py" "$stage/"
if [[ -e "$install_dir" ]]; then
  backup="$backup_dir/spirefysh-bridge.backup.$(date +%Y%m%d-%H%M%S)"
  mv "$install_dir" "$backup"
  print "previous_install=$backup"
fi
mv "$stage" "$install_dir"
print "installed=$install_dir"
