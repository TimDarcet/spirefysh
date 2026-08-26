#!/bin/zsh
set -euo pipefail

if (( $# != 1 )); then
  print -u2 "usage: bridge/Spirefysh.Bridge/validate-mod-settings.zsh SETTINGS.save"
  exit 2
fi

settings_path=${1:A}
if [[ ! -f "$settings_path" ]]; then
  print -u2 "profile settings are missing: $settings_path"
  exit 2
fi
if ! command -v jq >/dev/null; then
  print -u2 "jq is required to validate the enabled-mod set"
  exit 2
fi
if ! jq -e '
  (.mod_settings | type == "object") and
  (.mod_settings.mods_enabled == true) and
  (.mod_settings.mod_list | type == "array") and
  all(.mod_settings.mod_list[]?;
    (.id | type == "string") and (.is_enabled | type == "boolean") and (.source | type == "string"))
' "$settings_path" >/dev/null; then
  print -u2 "profile mod settings are malformed or global mod loading is disabled"
  exit 2
fi

unexpected_enabled_mods=$(jq -r '
  [.mod_settings.mod_list[]? |
    select(.is_enabled == true and .id != "spirefysh.bridge") |
    .id] | sort | join(",")
' "$settings_path")
if [[ -n "$unexpected_enabled_mods" ]]; then
  print -u2 "non-bridge mods are enabled in profile settings: $unexpected_enabled_mods"
  exit 2
fi

disabled_bridge_entries=$(jq -r '
  [.mod_settings.mod_list[]? |
    select(.id == "spirefysh.bridge" and .is_enabled != true)] | length
' "$settings_path")
if [[ "$disabled_bridge_entries" != 0 ]]; then
  print -u2 "spirefysh.bridge is explicitly disabled in profile settings"
  exit 2
fi

print "bridge-mod-settings valid=true"
