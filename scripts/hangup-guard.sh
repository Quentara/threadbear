# hangup-guard.sh <pidfile|-> <command...>
#
# Runs on a check host, as `bash -c "<this file>" hangup-guard ...` in an ssh
# command (guarded_cmd in scripts/remote-guard-lib.sh puts it together), so it needn't
# be there already. It runs the command, and when the ssh that started it goes
# (killed, Ctrl-C, the connection dropped), stops the command and everything
# under it.
#
# Without it they ran on: ssh without a terminal gives the remote side no
# hangup, so a command under it only finds out its caller's gone when it next
# writes, and a build or a recording may not. On 13 Nov a stopped remote.sh
# --desk run on Tinker Desk went on to build and record its clips over another
# thread's (27401630).
#
# How it knows: its stdin is the ssh channel, and the local side never writes
# to it (alive_stdin in remote-guard-lib.sh), so a read
# that ends means the ssh client is gone. The command gets /dev/null instead.
#
# The command runs in a process group of its own (perl's setpgrp, as macOS has
# no setsid command), so the group catches anything that left the tree, and the
# tree catches anything that made a group of its own.
#
# Desk Runner (Tinker Desk's ~/.local/bin/desk) runs its command in the GUI
# session, not under this, so the desk client dying wouldn't touch it. For that,
# the command run there writes its pid to <pidfile> as it starts (remote.sh's
# run script does), and exits at once if <pidfile>.gone is there, which is
# written before the pidfile is read, in case the ssh went before it started.
# Its process group is only signalled when it leads one, never Desk Runner's.

pidfile="$1"
shift
[ "$pidfile" != - ] || pidfile=""
exec 3<&0 </dev/null

tree_of() {
  local kid
  echo "$1"
  for kid in $(pgrep -P "$1" 2>/dev/null); do tree_of "$kid"; done
}
# stop <pid>...: each one's group (where it leads one) and tree, asked first and
# made to after 5 s. Everything is found before anything is signalled, so none
# is handed to init and missed.
stop() {
  local p pids="" groups="" left n=0
  for p in "$@"; do
    [ -n "$p" ] || continue
    [ "$(ps -o pgid= -p "$p" 2>/dev/null | tr -d ' ')" != "$p" ] || groups+=" -$p"
    pids+=" $(tree_of "$p")"
  done
  [ -n "$pids" ] || return 0
  for p in $groups; do kill -TERM -- "$p" 2>/dev/null; done
  kill -TERM $pids 2>/dev/null
  while [ "$n" -lt 25 ]; do
    left=""
    for p in $pids; do kill -0 "$p" 2>/dev/null && left+=" $p"; done
    [ -n "$left" ] || return 0
    sleep 0.2
    n=$((n + 1))
  done
  for p in $groups; do kill -KILL -- "$p" 2>/dev/null; done
  kill -KILL $left 2>/dev/null
}

perl -e 'setpgrp(0, 0); exec { $ARGV[0] } @ARGV or die "hangup-guard: $ARGV[0]: $!\n"' "$@" &
child=$!
(
  # Nothing ever comes; this returns when the ssh client's gone. Nothing is
  # written to say so: stdout and stderr went with it, and a write would end
  # this subshell (SIGPIPE) before it stopped anything.
  while read -r -u 3 _; do :; done
  # From here it finishes, though the command may end under it and the guard
  # below try to stop this as it would a watcher still waiting.
  trap '' TERM
  desk=""
  if [ -n "$pidfile" ]; then
    : >"$pidfile.gone"
    desk="$(cat "$pidfile" 2>/dev/null)"
    case "$desk" in ""|*[!0-9]*) desk="" ;; esac
  fi
  stop "$child" $desk
) &
watcher=$!
# The guard's own stderr goes too, the command keeping its own: bash tells
# stderr when a job it waits for was killed, and that write, on a channel
# that's gone, ended the guard (SIGPIPE) before it cleaned up.
exec 3<&- 2>/dev/null
wait "$child"
rc=$?
# A watcher still waiting goes; one that's stopping things finishes first.
kill "$watcher" 2>/dev/null
wait "$watcher" 2>/dev/null
[ -z "$pidfile" ] || rm -f "$pidfile" "$pidfile.gone"
exit "$rc"
