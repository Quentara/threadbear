#!/bin/bash
# Runs a command from this checkout on another machine, so the Rust builds and
# the spec runs stay off the Mac you're working on (AGENTS.md).
#
#   desktop/scripts/remote.sh <ssh host> [--desk] -- <command...>
#
# It copies the checkout, committed or not, less its build folders, to a folder
# of its own under ~/.cache/threadbear-check/gpui there (so two checkouts at once
# don't overwrite each other), and runs the command from that folder, with Rust
# and Node on the PATH, npm packages installed the first time, and cargo's target
# folder kept between runs. --desk runs it on Tinker Desk's desktop (Desk Runner),
# which is what a window needs there.
#
# Everything lives under ~/.cache/threadbear-check, as scripts/check-remote.sh
# has it, so the host still looks like one with no Threadbear on it. Tinker
# Desk is also where clean installs get tried.
#
# Runs from one checkout to one host take turns: each copies over the folder
# the last one is building from, so an overlap tested whichever copy landed
# last, or files deleted mid-build. A script run there that edits the copy and
# puts a file back must touch it afterwards: put back, it's older than the build
# of the edit, and cargo calls that build fresh.
set -euo pipefail
host="${1:?usage: remote.sh <ssh host> [--desk] -- <command>}"; shift
desk=0
if [ "${1:-}" = "--desk" ]; then desk=1; shift; fi
[ "${1:-}" != "--" ] || shift
[ $# -gt 0 ] || { echo "no command given" >&2; exit 2; }
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
base=".cache/threadbear-check/gpui"
# Each checkout's folders there are its own, by its path here. A caller whose
# path is the same on every Mac (the app's update clone, under the same cache
# folder for anyone with the same user name) adds THREADBEAR_REMOTE_KEY, this
# Mac's own id, so two Macs sharing a host never build in each other's
# folders (Codex's review, 12 Nov). stage-update.sh works out the same key.
key="$(printf '%s%s' "$root" "${THREADBEAR_REMOTE_KEY:+@$THREADBEAR_REMOTE_KEY}" | shasum | cut -c1-10)"
src="$base/src-$key"
target="$base/target-$key"

# The turn is a lock on a file this script holds open (fd 9), so it goes when
# the script does, however it ends. Not lockf or flock running this script as
# their child: kill that and the turn was free while this went on. ssh drops fd
# 9 as it starts. The setup steps before the run are short, and a TERM or HUP
# then is taken once the step ends. The run itself is a background ssh this
# waits for, so a signal stops it at once: the trap kills that ssh, and
# hangup-guard.sh on the host stops the command when its ssh goes (13 Nov: a
# stopped --desk run went on building and recording on Tinker Desk). A KILL
# can't be trapped, but it takes the ssh too when it's sent to the group, as
# Ctrl-C and Claude Code's stop are, and the guard does the rest.
# The folder is fixed, not TMPDIR, so every run from the checkout finds
# the same file. perl for macOS, which has no flock command.
. "$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)/scripts/remote-guard-lib.sh"
ssh_pid=""
# The turn is kept until the host says the run has gone (30 s at most): the
# guard there takes up to 5 s to stop a command that ignores TERM, and the
# next run mustn't copy over its folder while it's still going (Codex's
# review). [r] so the pgrep's own shell, whose command line has the pattern
# in it, doesn't count. A host that can't be reached says nothing either way.
on_signal() {
  if [ -n "$ssh_pid" ]; then
    kill "$ssh_pid" 2>/dev/null
    ssh -o BatchMode=yes -o ConnectTimeout=5 "$host" \
      "i=0; while pgrep -f '[r]un-$tag' >/dev/null && [ \$i -lt 60 ]; do sleep 0.5; i=\$((i + 1)); done" </dev/null >/dev/null 2>&1 || true
  fi
  exit "$1"
}
trap 'on_signal 143' TERM
trap 'on_signal 129' HUP
trap 'on_signal 130' INT
if [ "$(uname -s)" = Darwin ]; then turns="$HOME/Library/Caches/Threadbear"; else turns="${XDG_CACHE_HOME:-$HOME/.cache}/threadbear"; fi
mkdir -p "$turns/remote"
exec 9>>"$turns/remote/$key-$(printf '%s' "$host" | tr -c 'A-Za-z0-9.-' _).lock"
waiting="Waiting for another remote.sh run from this checkout on $host to finish..."
if command -v flock >/dev/null 2>&1; then
  flock -n 9 || { echo "$waiting" >&2; flock 9; }
else
  WAITING="$waiting" perl -e 'use Fcntl ":flock"; open(my $f, ">&=", 9) or die "remote.sh: no lock file: $!\n"; flock($f, LOCK_EX | LOCK_NB) or do { print STDERR "$ENV{WAITING}\n"; flock($f, LOCK_EX) or die "remote.sh: lock: $!\n" }'
fi

# A checkout that hasn't been run for a few days gives its folders back: a
# target is many GB and the machines are short of disk. The bare src and target
# are from before each checkout had its own. The target is touched as well as
# the src because cargo working inside it doesn't change its time, and a folder
# in daily use would otherwise look a few days old.
#
# A target with no .synced-by-content was built from copies made with rsync -a
# (below), and may hold a build cargo wrongly calls fresh that a copy by content
# would never fix, as nothing in it changes. Its sources are touched once, so
# its own crates build again, and a run that can't do that doesn't go on.
# Exit 9 is that touch failing; any other failure here is let go, as before (a
# marker not written, on a full disk, just means touching them again next time).
rc=0
ssh "$host" "mkdir -p $src $target; [ -e $target/.synced-by-content ] || { if [ -d $src ]; then find $src -type f -exec touch -c {} + || exit 9; fi; touch $target/.synced-by-content 2>/dev/null; }; touch $src $target; find $base -mindepth 1 -maxdepth 1 \\( -name 'src-*' -o -name 'target-*' -o -name src -o -name target \\) -mtime +3 ! -name 'src-$key' ! -name 'target-$key' -exec rm -rf {} + 2>/dev/null; find $base -mindepth 1 -maxdepth 1 -name 'run-*' -mmin +720 -exec rm -f {} + 2>/dev/null; true" || rc=$?
if [ "$rc" = 9 ]; then
  echo "Couldn't mark this checkout's copy on $host to build again, so it may hold a stale build. Not running." >&2
  exit 1
fi
# And a host short of disk gets room back sooner, or refuses the run.
ssh "$host" "cd $base && bash -s -- $key" <"$root/desktop/scripts/trim-remote-cache.sh"
# Named by hand, not read from .gitignore: rsync takes those rules from the
# receiving side when it decides what to delete, which removed part of a
# Swift build and a node_modules here on 9 Nov.
#
# Files are compared by content (-c) and written with the host's own time, not
# this Mac's (no -t, so not -a). Cargo calls a crate fresh when its files are
# older than its last build, and a Mac time can be: a file changed here before a
# build there that compiled the old copy (an overlapping run, a script that
# edited the copy) arrived looking older than that build, and the next run
# passed on stale code (11 Nov, thread 699f31d2). Unchanged files keep their
# time, so they don't rebuild. desktop/scripts/test-remote-freshness.sh <host>
# plays that case.
rsync -rlpDcz --delete --exclude .git --exclude node_modules --exclude .build --exclude target --exclude reports \
  --exclude .fixtures --exclude .swiftpm --exclude dist --exclude DerivedData --exclude .DS_Store \
  "$root/" "$host:$src/"

printf -v cmd '%q ' "$@"
# The run's first argument, with --desk, is its pidfile (hangup-guard.sh):
# Desk Runner starts it away from the guard, so it says where it is.
read -r -d '' prelude <<PRELUDE || true
if [ -n "\${1:-}" ]; then
  echo \$\$ >"\$1"
  [ ! -e "\$1.gone" ] || exit 129
fi
set -e
cd "\$HOME/$src"
# rsync changes this folder's time only when it writes into it, and the age limit above reads it.
touch .
# So trim-remote-cache.sh can tell this target's in use for the whole run. Its
# output goes nowhere, or ssh would wait for it after the command ends.
# The mkdir is for a host so full the one above failed, until the trim made room.
# || true because this inherits set -e, and a touch on a full disk mustn't end it.
mkdir -p "\$HOME/$target"
( while kill -0 \$\$ 2>/dev/null; do touch "\$HOME/$target/.in-use" || true; sleep 60; done ) >/dev/null 2>&1 </dev/null &
. runner/scripts/find-node.sh
NODE="\$(threadbear_find_node)"
export PATH="\${NODE%/*}:\$PATH"
if [ -x "\$HOME/$base/cargo/bin/cargo" ]; then
  export CARGO_HOME="\$HOME/$base/cargo" RUSTUP_HOME="\$HOME/$base/rustup"
  export PATH="\$CARGO_HOME/bin:\$PATH"
elif [ -x "\$HOME/.cargo/bin/cargo" ]; then
  export PATH="\$HOME/.cargo/bin:\$PATH"
fi
# A target folder per checkout. Cargo's names for path crates ignore where the
# checkout is, so with a shared one each checkout's build replaced the other's.
export CARGO_TARGET_DIR="\$HOME/$target" CARGO_HTTP_MULTIPLEXING=false CARGO_NET_RETRY=10
# Debug builds keep line numbers (enough for a panic's backtrace) but not the
# rest of the debug info, which was most of a target: one checkout's reached
# 15 GB in an hour on the 75 GB cloud box, the threadbear-gpui test binary
# alone 1.1 GB, and four of them filled it (13 Nov). Nothing reads the rest
# there: no debugger runs on these hosts.
export CARGO_PROFILE_DEV_DEBUG=line-tables-only
export TMPDIR="\$HOME/$base/tmp"
mkdir -p "\$TMPDIR"
# What the specs need, the first time this folder's used.
for dir in features app/Tools/mock-runner runner; do
  [ -d "\$dir/node_modules" ] || npm ci --silent --prefix "\$dir" >/dev/null 2>&1 || true
done
PRELUDE

# A disk that fills during the run fails it in ways that can look like a pass:
# on 13 Nov every spec scenario failed with ENOSPC on the cloud box, cargo said
# nothing, and the run ended 0. So a run that leaves the host under 1 GiB free
# fails, whatever its command said, and says why.
read -r -d '' postlude <<POSTLUDE || true
kb="\$(df -Pk "\$HOME/$base" 2>/dev/null | awk 'END { print \$4 }')"
case "\$kb" in
  ""|*[!0123456789-]*) ;;
  *) if [ "\$kb" -lt 1048576 ]; then
       echo "remote.sh: $host has only \$((kb / 1024)) MiB free after this run, so whatever it wrote may have failed for want of space, even if it said it passed. Treating it as failed. Run it again: it gives back idle build folders there first, or says why it can't." >&2
       [ "\$rc" != 0 ] || rc=1
     fi ;;
esac
exit "\$rc"
POSTLUDE
run="$prelude
rc=0
$cmd || rc=\$?
$postlude"

# The run goes up as a file, as ssh's stdin is the guard's: nothing is sent on
# it, and it ending is how the host knows this has gone.
# Named by this run alone: a stopped --desk run's .gone file stays there (the
# guard's comment says why) until the clear-out above, half a day on.
tag="$key-$$-$RANDOM$RANDOM"
script="$base/run-$tag.sh"
ssh "$host" "cat > $script" <<<"$run"
alive_stdin 7
if [ "$desk" = 1 ]; then
  # Desk Runner needs the screen awake for anything it draws.
  pidfile="$base/run-$tag.pid"
  remote="caffeinate -u -t 2 >/dev/null 2>&1 || true; $(guarded_cmd "$pidfile" .local/bin/desk "perl -e 'setpgrp(0, 0); exec @ARGV' bash $script $pidfile")"
else
  remote="$(guarded_cmd - bash "$script")"
fi
ssh "$host" "$remote; rc=\$?; rm -f $script; exit \$rc" <&7 &
ssh_pid=$!
rc=0
wait "$ssh_pid" || rc=$?
exit "$rc"
