#!/usr/bin/perl
# Detached apps/mock runners must not outlive a failed measurement worker.
# --child registers its own process group before exec; --reap runs the hangup
# guard and stops registered groups before returning to the lock holder.
use strict;
use warnings;
use Fcntl qw(:flock :DEFAULT);
use File::Path qw(remove_tree);
use File::Basename qw(dirname);
use FindBin;

my ($mode, $dir) = splice @ARGV, 0, 2;
my $log = dirname($dir) . '/perf-cleanup.log';
my $last_note = time;
sub note_if_stuck {
    return if time - $last_note < 30;
    $last_note = time;
    if (sysopen(my $out, $log, O_WRONLY | O_CREAT | O_APPEND, 0600)) {
        print {$out} scalar(gmtime) . " UTC: $_[0]\n";
        close $out;
    }
}
sub started {
    my ($pid) = @_;
    local $ENV{LC_ALL} = 'C';
    local $ENV{TZ} = 'UTC';
    open(my $ps, '-|', 'ps', '-p', $pid, '-o', 'lstart=') or return '';
    my $time = do { local $/; <$ps> } // '';
    close $ps;
    $time =~ s/^\s+|\s+$//g;
    return $time;
}
sub active_groups {
    for (;;) {
        if (open(my $ps, '-|', 'ps', '-axo', 'pgid=,stat=')) {
            my %active;
            while (<$ps>) { $active{$1} = 1 if /^\s*(\d+)\s+(\S+)/ && $2 !~ /^Z/; }
            return %active if close $ps;
        }
        # A host too busy to answer hasn't proved that the work stopped.
        note_if_stuck('ps unavailable while stopping registered performance groups; locks retained');
        select(undef, undef, undef, 0.2);
    }
}

if ($mode eq '--child') {
    # Shared while registering; cleanup marks the run gone under an exclusive
    # lock. A late child either registers before that or exits before exec.
    open(my $state, '<', "$dir/state") or exit 129;
    flock($state, LOCK_SH) or die "perf-children: registration lock: $!\n";
    exit 129 if -e "$dir/gone";
    die "perf-children: child must lead its own group\n" unless getpgrp(0) == $$;
    my $time = started($$);
    die "perf-children: can't identify child\n" unless length $time;
    open(my $record, '>', "$dir/$$") or exit 129;
    print {$record} $time;
    close $record or die "perf-children: record: $!\n";
    close $state;
    exec { $ARGV[0] } @ARGV or die "perf-children: exec: $!\n";
}
$mode eq '--reap' or die "perf-children: unknown mode\n";
$SIG{PIPE} = 'IGNORE';
my $guard = fork() // die "perf-children: fork: $!\n";
if (!$guard) {
    $SIG{PIPE} = 'DEFAULT';
    exec '/bin/bash', "$FindBin::Bin/hangup-guard.sh", @ARGV;
    die "perf-children: guard: $!\n";
}
waitpid($guard, 0);
my $status = $?;
# From here cleanup must survive the caller's stdout/stderr having gone.
open(STDERR, '>', '/dev/null');
open(my $state, '<', "$dir/state") or die "perf-children: no registry\n";
flock($state, LOCK_EX) or die "perf-children: cleanup lock\n";
open(my $gone, '>', "$dir/gone") or die "perf-children: mark gone\n";
close $gone;
# Retire the public name atomically before removing anything. A late child
# holding the old state fd can never recreate a record after the scan.
my $retired = "$dir.reaping";
my $renamed = rename($dir, $retired);
$dir = $retired if $renamed;
opendir(my $entries, $dir) or die "perf-children: registry\n";
my %groups;
while (my $pid = readdir($entries)) {
    next unless $pid =~ /^\d+$/;
    open(my $record, '<', "$dir/$pid") or next;
    my $time = do { local $/; <$record> } // '';
    close $record;
    my $now = started($pid);
    # A live reused pid is somebody else's. With no leader left, a still-live
    # group is the original one: the kernel keeps that group id reserved.
    next if length($now) && $now ne $time;
    $groups{$pid} = 1 if kill(0, -$pid);
}
closedir $entries;
close $state;
kill 'TERM', map { -$_ } keys %groups if %groups;
for (1 .. 25) {
    my %active = active_groups();
    last unless grep { $active{$_} } keys %groups;
    select(undef, undef, undef, 0.2);
}
my %killed;
for my $pid (keys %groups) {
    my $now = started($pid);
    open(my $record, '<', "$dir/$pid") or next;
    my $time = do { local $/; <$record> } // '';
    close $record;
    next if length($now) && $now ne $time;
    $killed{$pid} = $time if kill('KILL', -$pid);
}
# Sending KILL isn't the same as observing it stop. Keep the lock copies
# until no registered group can still do work; zombies only await reaping.
while (%killed) {
    my %active = active_groups();
    for my $pid (keys %killed) {
        my $now = started($pid);
        delete $killed{$pid} if !$active{$pid} || (length($now) && $now ne $killed{$pid});
    }
    note_if_stuck('still stopping groups ' . join(',', keys %killed) . '; locks retained') if %killed;
    select(undef, undef, undef, 0.05) if %killed;
}
remove_tree($dir) if $renamed;
exit($status & 127 ? 128 + ($status & 127) : $status >> 8);
