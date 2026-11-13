# Sourced by a remote.sh run after finding Cargo and Node. Builds run in the
# background; the shell and the benchmark it runs afterwards keep normal
# priority. PATH wrappers also catch builds launched by Node or another shell.
threadbear_build_bin="$(mktemp -d "$TMPDIR/threadbear-build-bin.XXXXXX")"
trap 'rm -rf "$threadbear_build_bin"' EXIT
for threadbear_build_tool in cargo swift swiftc xcodebuild; do
  threadbear_build_real="$(command -v "$threadbear_build_tool")" || continue
  if [ "$(uname -s)" = Darwin ]; then
    printf '#!/bin/bash\nexec /usr/sbin/taskpolicy -b %q "$@"\n' "$threadbear_build_real"
  else
    printf '#!/bin/bash\nexec nice -n 10 %q "$@"\n' "$threadbear_build_real"
  fi >"$threadbear_build_bin/$threadbear_build_tool"
  chmod +x "$threadbear_build_bin/$threadbear_build_tool"
done
export PATH="$threadbear_build_bin:$PATH"
