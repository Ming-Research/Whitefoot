#!/usr/bin/env perl
# One owned process group for a heavy command and all nested Make targets.
# Perl and POSIX are already used by CI and available on the supported hosts.
use strict;
use warnings;
use Cwd qw(getcwd);
use File::Basename qw(dirname);
use POSIX qw(WNOHANG setpgid);
use Time::HiRes qw(clock_gettime CLOCK_MONOTONIC sleep);

@ARGV >= 2 or die "usage: run-check.pl LABEL COMMAND [ARG ...]\n";
my $label = shift;
my $lock = $ENV{WHITEFOOT_CHECK_LOCK_DIR} // "/tmp/whitefoot-check-$<.lock";
my $owner_pid = $$;
my $owns_lock = 0;
sub read_file {
    my ($path) = @_;
    open my $file, '<', $path or return '';
    local $/;
    return <$file> // '';
}
my $recorded = read_file("$lock/pid");
chomp $recorded;
if (($ENV{WHITEFOOT_CHECK_OWNER} // '') eq $recorded
    && $recorded =~ /^\d+$/ && kill(0, $recorded)) {
    # The same top-level command owns nested targets, even across worktrees.
} elsif (mkdir $lock, 0700) {
    $owns_lock = 1;
    $ENV{WHITEFOOT_CHECK_OWNER} = $$;
    open my $pid, '>', "$lock/pid" or die "write lock PID: $!\n";
    print {$pid} "$$\n";
    close $pid;
    open my $command, '>', "$lock/command" or die "write lock command: $!\n";
    print {$command} scalar(gmtime) . " UTC: " . getcwd() . ": $label @ARGV\n";
    close $command;
} else {
    warn "verification is already owned by another command:\n",
        read_file("$lock/pid"), read_file("$lock/command"),
        "lock: $lock; inspect the recorded PID before removing a stale lock\n";
    exit 75;
}
END {
    if ($owns_lock && $$ == $owner_pid) {
        unlink "$lock/pid", "$lock/command", "$lock/over-budget";
        rmdir $lock;
    }
}
$ENV{WHITEFOOT_CHECK_LOCK_DIR} = $lock;
# One owner at a time holds the host, so its builds and test pool use every
# online processor unless the caller names fewer.
my $processors = `getconf _NPROCESSORS_ONLN 2>/dev/null` // '';
chomp $processors;
$processors = 2 if $processors !~ /^[1-9]\d*$/;
for my $name (qw(CARGO_BUILD_JOBS RUST_TEST_THREADS JOBS)) {
    $ENV{$name} //= $processors;
}
my $budget_mode = $ENV{WHITEFOOT_TIME_BUDGETS} // 'report';
$budget_mode =~ /^(report|enforce)$/ or die "WHITEFOOT_TIME_BUDGETS must be report or enforce\n";
my $budget_file = $ENV{WHITEFOOT_TIME_BUDGET_FILE} // dirname(__FILE__) . '/time-budgets.txt';
my $host = $^O eq 'linux' ? 'linux' : $^O eq 'darwin' ? 'macos' : 'windows';
my $limit = $ENV{WHITEFOOT_CHECK_TIMEOUT} // 1800;
$limit =~ /^\d+$/ && $limit > 0 or die "WHITEFOOT_CHECK_TIMEOUT must be positive seconds\n";
my $started = clock_gettime(CLOCK_MONOTONIC);
my $next_report = $started + 30;
my ($cancelled, $stopping);
$SIG{INT} = sub { $cancelled //= 130 };
$SIG{TERM} = sub { $cancelled //= 143 };
$SIG{HUP} = sub { $cancelled //= 129 };
$SIG{PIPE} = sub { $cancelled //= 141 };
$| = 1;
print "== START $label (limit ${limit}s): @ARGV ==\n";
my $child = fork;
defined $child or die "fork check: $!\n";
if ($child == 0) {
    $SIG{INT} = $SIG{TERM} = $SIG{HUP} = $SIG{PIPE} = 'DEFAULT';
    if ($owns_lock) {
        defined setpgid(0, 0) or die "create check process group: $!\n";
        $ENV{WHITEFOOT_CHECK_PGID} = $$;
    }
    exec '/usr/bin/time', '-p', @ARGV;
    die "execute check: $!\n";
}
my $group = $owns_lock ? $child : $ENV{WHITEFOOT_CHECK_PGID};
setpgid($child, $child) if $owns_lock;
defined $group && $group =~ /^\d+$/ or die "missing owned check process group\n";
my ($status, $completed);
while (1) {
    my $now = clock_gettime(CLOCK_MONOTONIC);
    if (!defined $status) {
        my $waited = waitpid($child, WNOHANG);
        if ($waited == $child) {
            $status = $?;
            $completed = $now;
        }
    }
    if (defined $status && $owns_lock && kill(0, -$group)
        && $now - $completed >= 0.5 && !defined $cancelled) {
        warn "== $label left child processes after command exit ==\n";
        $cancelled = ($status & 127) ? 128 + ($status & 127) : ($status >> 8) || 1;
    }
    $cancelled //= 124 if $now - $started >= $limit;
    if (defined $cancelled && !defined $stopping) {
        $stopping = $now;
        warn "== STOP $label: ", ($cancelled == 124 ? 'deadline exceeded' : 'interrupted'), " ==\n";
        # A nested wrapper is inside the owned group. Ignore our own TERM;
        # the top-level owner stays outside, reaps, and releases the lock.
        $SIG{TERM} = 'IGNORE';
        kill 'TERM', $ENV{WHITEFOOT_CHECK_OWNER} if !$owns_lock;
        kill 'TERM', -$group;
    }
    if (defined $stopping && $now - $stopping >= 2) {
        kill 'KILL', -$group;
        waitpid($child, 0) if !defined $status;
        last;
    }
    last if defined $status && !defined $stopping
        && (!$owns_lock || !kill(0, -$group));
    if ($now >= $next_report) {
        printf "== RUNNING %s: %.0f s, child %d, group %d ==\n", $label, $now - $started, $child, $group;
        $next_report = $now + 30;
    }
    sleep 0.1;
}
my $code = $cancelled // (($status & 127) ? 128 + ($status & 127) : $status >> 8);
my $elapsed = clock_gettime(CLOCK_MONOTONIC) - $started;
printf "== END %s: %.2f s, exit %d ==\n", $label, $elapsed, $code;
check_budget($label, $elapsed) if !defined $cancelled;
if ($owns_lock && $budget_mode eq 'enforce') {
    my $over = read_file("$lock/over-budget");
    if ($over ne '') {
        print "== TIME BUDGETS EXCEEDED ==\n$over",
            "Find what grew (CI lists each job's ten largest gaps between cases) and\n",
            "make it cheaper, or raise the budget in $budget_file with the owner's\n",
            "approval.\n";
        $code ||= 3;
    }
}
exit $code;

# Each labeled stage has a wall-time budget per hosted runner class in
# time-budgets.txt, sized to what the stage does. CI sets
# WHITEFOOT_TIME_BUDGETS=enforce: a stage over its budget, or one without a
# budget for this host, fails the top-level command once every stage has run,
# so the job still reports all its results. Elsewhere the budget is only
# printed, since a local host is not the runner the budget was measured on.
sub check_budget {
    my ($label, $elapsed) = @_;
    my $budget = budget_for($label);
    my $verdict;
    if (!defined $budget || $budget eq '-') {
        return if $budget_mode ne 'enforce';
        $verdict = sprintf "%s has no %s budget in %s", $label, $host, $budget_file;
    } elsif ($elapsed > $budget) {
        $verdict = sprintf "%s took %.0f s, over its %d s %s budget", $label, $elapsed, $budget, $host;
    } else {
        printf "== BUDGET %s: %.0f s of %d s (%s) ==\n", $label, $elapsed, $budget, $host;
        return;
    }
    warn "== OVER BUDGET: $verdict ==\n";
    return if $budget_mode ne 'enforce';
    open my $record, '>>', "$lock/over-budget" or die "record budget overrun: $!\n";
    print {$record} "  $verdict\n";
    close $record;
}

sub budget_for {
    my ($label) = @_;
    open my $file, '<', $budget_file or die "read $budget_file: $!\n";
    my @hosts;
    while (my $line = <$file>) {
        next if $line =~ /^\s*(#|$)/;
        my ($name, @values) = split ' ', $line;
        if (!@hosts) {
            $name eq 'label' or die "$budget_file: the first row must name the hosts\n";
            @hosts = @values;
            next;
        }
        next if $name ne $label;
        for my $index (0 .. $#hosts) {
            next if $hosts[$index] ne $host;
            my $value = $values[$index] // '';
            $value =~ /^(\d+|-)$/ or die "$budget_file: $label needs seconds or - for $host\n";
            return $value;
        }
    }
    return undef;
}
