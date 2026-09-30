#!/usr/bin/env perl
# One owned process group for a heavy command and all nested Make targets.
# Perl and POSIX are already used by CI and available on the supported hosts.
use strict;
use warnings;
use Cwd qw(getcwd);
use File::Basename qw(dirname);
use POSIX qw(WNOHANG setpgid);
use Time::HiRes qw(clock_gettime CLOCK_MONOTONIC sleep);

my $usage = "usage: run-check.pl LABEL COMMAND [ARG ...] | --budget-verdict RECORD\n";
@ARGV >= 2 or die $usage;
if ($ARGV[0] eq '--budget-verdict') {
    @ARGV == 2 && $ARGV[1] ne '' or die $usage;
    budget_verdict($ARGV[1]);
}
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
        unlink "$lock/pid", "$lock/command";
        rmdir $lock;
    }
}
$ENV{WHITEFOOT_CHECK_LOCK_DIR} = $lock;
# Builds and the test pool keep Cargo's and the test harness's own default,
# every processor available to this process, unless the caller names fewer:
# the lock above already keeps other heavy commands off the host.
my $budget_record = $ENV{WHITEFOOT_TIME_BUDGET_RECORD} // '';
if ($budget_record ne '') {
    # Checked before the stage runs, so a record that cannot be written stops
    # the command here instead of changing a finished stage's status.
    $budget_record =~ m{^/} or die "WHITEFOOT_TIME_BUDGET_RECORD must be an absolute path\n";
    open my $record, '>>', $budget_record or die "open budget record $budget_record: $!\n";
    close $record;
}
my $budget_file = $ENV{WHITEFOOT_TIME_BUDGET_FILE} // dirname(__FILE__) . '/time-budgets.txt';
my %hosts = (linux => 'linux', darwin => 'macos', MSWin32 => 'windows', msys => 'windows', cygwin => 'windows');
my $host = $hosts{$^O} // $^O;
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
exit $code;

# Each labeled stage has a wall-time budget per hosted runner class in
# time-budgets.txt. The budget never changes the command's own exit status,
# which some callers read. CI names a record file in
# WHITEFOOT_TIME_BUDGET_RECORD: a stage over its budget, or one without a
# budget for this host, is appended there, and the job's final verdict step
# fails on a nonempty record, after every stage has run. Without a record
# the budget is only printed, since a local host is not the runner it was
# measured on.
sub check_budget {
    my ($label, $elapsed) = @_;
    my ($budget, $problem) = budget_for($label);
    my $verdict;
    if (defined $problem) {
        $verdict = $problem;
    } elsif (!defined $budget || $budget eq '-') {
        return if $budget_record eq '';
        $verdict = sprintf "%s has no %s budget in %s", $label, $host, $budget_file;
    } elsif ($elapsed > $budget) {
        $verdict = sprintf "%s took %.1f s, over its %d s %s budget", $label, $elapsed, $budget, $host;
    } else {
        printf "== BUDGET %s: %.1f s of %d s (%s) ==\n", $label, $elapsed, $budget, $host;
        return;
    }
    warn "== OVER BUDGET: $verdict ==\n";
    return if $budget_record eq '';
    open my $record, '>>', $budget_record or die "record budget overrun in $budget_record: $!\n";
    print {$record} "  $verdict\n";
    close $record;
}

# Returns the budget, undef for no row, or a problem with the table itself.
sub budget_for {
    my ($label) = @_;
    open my $file, '<', $budget_file or return (undef, "cannot read $budget_file: $!");
    my @hosts;
    while (my $line = <$file>) {
        next if $line =~ /^\s*(#|$)/;
        my ($name, @values) = split ' ', $line;
        if (!@hosts) {
            return (undef, "$budget_file: the first row must name the hosts") if $name ne 'label';
            @hosts = @values;
            next;
        }
        next if $name ne $label;
        for my $index (0 .. $#hosts) {
            next if $hosts[$index] ne $host;
            my $value = $values[$index] // '';
            return (undef, "$budget_file: $label needs seconds or - for $host") if $value !~ /^(\d+|-)$/;
            return ($value, undef);
        }
    }
    return (undef, undef);
}

sub budget_verdict {
    my ($record) = @_;
    my $over = read_file($record);
    if ($over eq '') {
        print "time budgets: every recorded stage finished within its budget\n";
        exit 0;
    }
    print "== TIME BUDGETS EXCEEDED ==\n$over",
        "Find what grew (the gate lists each job's ten largest gaps between cases)\n",
        "and make it cheaper, or raise the budget in .github/time-budgets.txt with\n",
        "the owner's approval.\n";
    exit 1;
}
