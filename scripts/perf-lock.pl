#!/usr/bin/perl
# One build or measurement per host. Both files stay locked until the hangup
# guard has stopped the command's tree, including Desk Runner's GUI child.
# Usage: perl scripts/perf-lock.pl <pidfile|-> <command...>
#        perl scripts/perf-lock.pl --desk <pidfile> <command...>
use strict;
use warnings;
use Fcntl qw(:flock);
use Errno qw(EAGAIN EWOULDBLOCK EINTR);
use File::Path qw(make_path);
use FindBin;
use File::Spec;
use IO::Select;
use POSIX qw(WNOHANG dup2);
use Sys::Hostname qw(hostname);

my $desk = @ARGV && $ARGV[0] eq '--desk' ? shift(@ARGV) : '';
my $pidfile = shift(@ARGV) // die "perf-lock: missing pidfile or -\n";
@ARGV or die "perf-lock: missing command\n";
die "perf-lock: --desk needs a pidfile\n" if $desk && $pidfile eq '-';
$pidfile = File::Spec->rel2abs($pidfile) if $pidfile ne '-';
my $home = $ENV{HOME} // die "perf-lock: no HOME\n";
my $host = hostname();
my $owner = $ENV{THREADBEAR_PERF_LOCK_OWNER} // '';
# A marker only belongs to this host and home, and to a living lock holder.
# Desk Runner gets it explicitly below, since its service has its own env.
if ($owner =~ /^\Q$host\E:(\d+):\Q$home\E$/ && $1 > 0 && kill(0, $1)) {
    die "perf-lock: --desk starts a remote run; call its workload directly inside an existing run\n" if $desk;
    exec { $ARGV[0] } @ARGV or die "perf-lock: exec: $!\n";
}

my $cancelled = 0;
$SIG{TERM} = sub { $cancelled = 143 };
$SIG{HUP} = sub { $cancelled = 129 };
$SIG{INT} = sub { $cancelled = 130 };
# A dropped ssh can make even a diagnostic write fail. Cleanup must finish.
$SIG{PIPE} = 'IGNORE';
my $input = IO::Select->new(\*STDIN);
sub channel_gone {
    return 0 unless $input->can_read(0);
    my $n = sysread(STDIN, my $byte, 1);
    return defined($n) && $n == 0;
}
sub stopping {
    $cancelled ||= 129 if channel_gone();
    return $cancelled;
}

make_path("$home/.cache/threadbear-check");
my @locks;
for my $spec (["$home/.cache/threadbear-perf.lock", 'another build or measurement'],
              ["$home/.cache/threadbear-check/lock", 'another check']) {
    my $said = 0;
    # Keep the lock we actually acquired, including while waiting for the next
    # one. A free probe followed by a build races a check starting after it.
    for (;;) {
        open(my $lock, '>>', $spec->[0]) or die "perf-lock: $spec->[0]: $!\n";
        while (!flock($lock, LOCK_EX | LOCK_NB)) {
            die "perf-lock: lock $spec->[0]: $!\n" unless $! == EAGAIN || $! == EWOULDBLOCK || $! == EINTR;
            exit $cancelled if stopping();
            print STDERR "Waiting for $spec->[1] on $host ($spec->[0])...\n" unless $said++;
            select(undef, undef, undef, 0.1);
        }
        exit $cancelled if stopping();
        # Old lockf calls without -k unlink on exit. A waiter can then own a
        # deleted inode while a new caller owns its replacement. Reopen it.
        my @opened = stat($lock);
        my @named = stat($spec->[0]);
        next unless @named && $opened[0] == $named[0] && $opened[1] == $named[1];
        push @locks, $lock;
        last;
    }
}
$ENV{THREADBEAR_PERF_LOCK_OWNER} = "$host:$$:$home";

my @command = @ARGV;
if ($desk) {
    die "perf-lock: --desk needs a fresh pidfile\n" if $pidfile eq '-' || -e $pidfile || -e "$pidfile.gone";
    my $quote = sub { my $s = shift; $s =~ s/'/'\\''/g; return "'$s'"; };
    my $line = join(' ', map { $quote->($_) }
        ('env', "THREADBEAR_PERF_LOCK_OWNER=$ENV{THREADBEAR_PERF_LOCK_OWNER}",
         'perl', '-e', 'setpgrp(0, 0); exec @ARGV', '/bin/bash', '-c',
         'echo $$ >"$1"; [ ! -e "$1.gone" ] || exit 129; shift; exec "$@"',
         'perf-desk', $pidfile, @command));
    @command = ("$home/.local/bin/desk", $line);
}
# The guard gets a pipe of its own. Only this supervisor writes it; closing it
# on TERM or ssh EOF asks the guard to clean up while we keep both locks.
pipe(my $read, my $write) or die "perf-lock: pipe: $!\n";
my $child = fork() // die "perf-lock: fork: $!\n";
if (!$child) {
    # A terminal/group cancellation must reach this supervisor, whose pipe
    # then asks the guard to clean up. It mustn't kill the guard and watcher.
    setpgrp(0, 0);
    $SIG{PIPE} = 'DEFAULT';
    close $write;
    # The guard keeps copies too: even KILL of this supervisor must leave the
    # locks held while EOF makes the guard stop its command. Use high fds,
    # since the guard uses fd 3 for its channel. dup2 clears close-on-exec.
    for my $i (0 .. $#locks) {
        dup2(fileno($locks[$i]), 198 + $i) >= 0 or die "perf-lock: dup: $!\n";
    }
    close $_ for @locks;
    open(STDIN, '<&', $read) or die "perf-lock: stdin: $!\n";
    close $read;
    # The guard holds the copies, its workload doesn't. A successful run can
    # leave a harmless heartbeat sleeping briefly; it mustn't keep the turn.
    exec '/bin/bash', "$FindBin::Bin/hangup-guard.sh", $pidfile,
        '/bin/bash', '-c', 'exec 198>&- 199>&-; exec "$@"', 'perf-command', @command;
    die "perf-lock: guard: $!\n";
}
close $read;
my $closed = 0;
for (;;) {
    my $done = waitpid($child, WNOHANG);
    if ($done == $child) {
        my $status = $?;
        close $write unless $closed;
        exit($cancelled || ($status & 127 ? 128 + ($status & 127) : $status >> 8));
    }
    if (stopping() && !$closed) { close $write; $closed = 1; }
    select(undef, undef, undef, 0.1);
}
