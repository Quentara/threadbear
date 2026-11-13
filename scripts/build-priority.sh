# Sourced by a remote.sh run after finding Cargo and Node. Builds run in the
# background; the shell and the benchmark it runs afterwards keep normal
# priority. PATH wrappers also catch builds launched by Node or another shell.
threadbear_build_bin="$(mktemp -d "$TMPDIR/threadbear-build-bin.XXXXXX")"
trap 'rm -rf "$threadbear_build_bin"' EXIT
for threadbear_build_tool in cargo swift swiftc xcodebuild; do
  threadbear_build_real="$(command -v "$threadbear_build_tool")" || continue
  # cargo run/bench and swift run can be measurements themselves. Only the
  # compilation commands get the lower priority; executables run normally.
  case "$threadbear_build_tool" in
    cargo) threadbear_build_actions='build|check|clippy|rustc' ;;
    swift) threadbear_build_actions='build' ;;
    *) threadbear_build_actions='*' ;;
  esac
  {
    printf '#!/bin/bash\ncase "${1:-}" in\n  %s) ' "$threadbear_build_actions"
    if [ "$(uname -s)" = Darwin ]; then
      printf 'exec /usr/sbin/taskpolicy -b %q "$@" ;;\n' "$threadbear_build_real"
    else
      printf 'exec nice -n 10 %q "$@" ;;\n' "$threadbear_build_real"
    fi
    printf 'esac\nexec %q "$@"\n' "$threadbear_build_real"
  } >"$threadbear_build_bin/$threadbear_build_tool"
  chmod +x "$threadbear_build_bin/$threadbear_build_tool"
done
export PATH="$threadbear_build_bin:$PATH"
