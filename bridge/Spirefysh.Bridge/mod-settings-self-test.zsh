#!/bin/zsh
set -euo pipefail

script_dir=${0:A:h}
validator="$script_dir/validate-mod-settings.zsh"
fixtures="$script_dir/fixtures"

for fixture in valid-unlisted valid-bridge; do
  zsh "$validator" "$fixtures/settings-$fixture.json" >/dev/null
done

for fixture in enabled-other disabled-bridge malformed; do
  if zsh "$validator" "$fixtures/settings-$fixture.json" >/dev/null 2>&1; then
    print -u2 "expected settings-$fixture.json to fail closed"
    exit 1
  fi
done

print "bridge-mod-settings self-test valid=true cases=5 failures=0"
