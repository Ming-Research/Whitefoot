#!/usr/bin/env bash
# Deny every host clang to the processes started under a macOS sandbox profile,
# and prove the denial: each host clang path first executes outside the sandbox
# and is refused inside it, while the SDK and the system linker stay available.
# A runner that cannot show this leaves the isolation unverified; PATH hiding or
# a failed sandbox launch is never counted as denial.
#
# Usage: deny-host-clang-macos.sh LOGS_DIRECTORY
# Writes LOGS_DIRECTORY/deny-host-clang.sb (the profile; run a command as
# `/usr/bin/sandbox-exec -f deny-host-clang.sb COMMAND`), host-clangs.txt, the
# denial logs and isolation.txt holding `isolation=verified` or
# `isolation=unverified`. Shared by the package checks (check-package.sh) and
# the acceptance run of the gate's tests with the packaged clang.
set -euo pipefail
if [ "$#" -ne 1 ]; then
  echo 'usage: deny-host-clang-macos.sh LOGS_DIRECTORY' >&2
  exit 2
fi
logs="$1"
mkdir -p "$logs"
isolation=verified
# Deny every visible host clang path and its canonical target. SDK/ld remain
# available. Never equate PATH hiding or a failed sandbox launch with denial.
selected_host_clang=$(/usr/bin/xcrun --sdk macosx --find clang)
python3 - "$logs" "$selected_host_clang" <<'PY'
import glob, json, os, pathlib, sys
logs = pathlib.Path(sys.argv[1])
patterns = [sys.argv[2], '/usr/bin/clang*', '/Library/Developer/CommandLineTools/usr/bin/clang*',
            '/Applications/Xcode*.app/Contents/Developer/Toolchains/*/usr/bin/clang*']
patterns += [str(pathlib.Path(part) / 'clang*') for part in os.environ['PATH'].split(':') if part]
paths = set()
for pattern in patterns:
    for name in glob.glob(pattern):
        path = pathlib.Path(name)
        if path.is_file() and os.access(path, os.X_OK):
            paths.update((str(path.absolute()), str(path.resolve(strict=True))))
assert '/usr/bin/clang' in paths, 'No host clang to prove denial against'
(logs / 'host-clangs.txt').write_text(''.join(path + '\n' for path in sorted(paths)))
profile = '(version 1)\n(allow default)\n(deny process-exec\n'
profile += ''.join('  (literal ' + json.dumps(path) + ')\n' for path in sorted(paths))
(logs / 'deny-host-clang.sb').write_text(profile + ')\n')
PY
profile="$logs/deny-host-clang.sb"
if [ ! -x /usr/bin/sandbox-exec ] || ! /usr/bin/sandbox-exec -f "$profile" /usr/bin/true; then
  isolation=unverified
  echo 'UNVERIFIED: hosted runner cannot execute the isolation profile'
else
  while IFS= read -r host; do
    # The baseline proves the path executes, not that it is a compiler:
    # tools such as clang-cache exit nonzero on --version but still run.
    # Only the shell's cannot-execute statuses mean it never ran.
    status=0
    "$host" --version > "$logs/denial-baseline.txt" 2>&1 || status=$?
    if [ "$status" -eq 126 ] || [ "$status" -eq 127 ]; then
      isolation=unverified
      printf 'UNVERIFIED: host baseline cannot execute: %s\n' "$host"
      break
    fi
    printf 'Prove host denial: %s\n' "$host"
    /usr/bin/sandbox-exec -f "$profile" "$host" --version > "$logs/denial-attempt.txt" 2>&1 || true
    if ! grep -Fq "execvp() of '$host' failed: Operation not permitted" "$logs/denial-attempt.txt"; then
      isolation=unverified
      printf 'UNVERIFIED: sandbox did not refuse executing host clang: %s\n' "$host"
      break
    fi
    cat "$logs/denial-attempt.txt"
    printf '%s: baseline succeeded; sandbox invocation failed\n' "$host" >> "$logs/denial-results.txt"
  done < "$logs/host-clangs.txt"
fi
printf 'isolation=%s\n' "$isolation" > "$logs/isolation.txt"
