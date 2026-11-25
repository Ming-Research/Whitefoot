#!/usr/bin/env perl
# Run one shard of a test group, for the gate's Linux jobs (compiler/verification).
#
#   shard-tests.pl unit i/N CARGO
#       The library's tests (`--lib`): libtest's own sorted list, positions
#       p with p mod N == i-1, run by exact name.
#   shard-tests.pl corpus walk-i/N CARGO
#       The conformance walk, the one test that checks every manifest case,
#       over every N-th case from the i-th (WHITEFOOT_CONFORMANCE_SHARD).
#   shard-tests.pl corpus receipts CARGO
#       The proof-receipt test alone, over every case in one process.
#   shard-tests.pl corpus rest CARGO
#       Every other test of the CLI and integration targets.
#   shard-tests.pl --self-test
#
# libtest has no shard option, so this uses its list and exact names. Every
# shard checks that libtest reported as many results (passed, failed and
# ignored) as it selected, so a name that matched no test, or one that matched
# more, fails the shard instead of passing it with less coverage. The two
# manifest-wide tests are named in full and must each be listed exactly once:
# a renamed test stops the corpus parts instead of leaving them to run it twice
# or not at all. A filter is always exact: a test whose name only contains one
# of those names is neither skipped nor run by their parts.
use strict;
use warnings;

my $usage = "usage: shard-tests.pl unit I/N CARGO | corpus walk-I/N CARGO | corpus receipts CARGO | corpus rest CARGO | --self-test\n";
my $walk = 'conformance::adapter::the_corpus_reaches_its_declared_verdict_through_the_ordinary_compiler_path';
my $receipts = 'conformance::adapter::a_shared_proof_receipt_cache_reaches_every_declared_source_verdict';
my @corpus_targets = ('--bins', '--test', '*');
my $quiet = 0;
exit main(@ARGV);

sub main {
    if (@_ == 1 && $_[0] eq '--self-test') {
        self_test();
        return 0;
    }
    $| = 1;
    if (@_ == 3 && $_[0] eq 'unit') {
        my ($index, $count) = parse_shard($_[1]);
        return run_unit($_[2], $index, $count, \&command);
    }
    if (@_ == 3 && $_[0] eq 'corpus') {
        return run_corpus($_[2], [parse_corpus_part($_[1])], \&command);
    }
    die $usage;
}

# `i/N` with 1 <= i <= N.
sub parse_shard {
    my ($shard) = @_;
    $shard =~ m{^([1-9][0-9]*)/([1-9][0-9]*)$} && $1 <= $2
        or die "test shards: a shard is I/N with 1 <= I <= N, got '$shard'\n";
    return ($1, $2);
}

# `walk-i/N`, `receipts` or `rest`: the part and, for the walk, its shard.
sub parse_corpus_part {
    my ($part) = @_;
    return ($part) if $part eq 'receipts' || $part eq 'rest';
    $part =~ m{^walk-(.+)$}
        or die "test shards: a corpus part is walk-I/N, receipts or rest, got '$part'\n";
    return ('walk', parse_shard($1));
}

# Positions p of the bytewise-sorted names with p mod COUNT == INDEX-1.
sub partition {
    my ($names, $index, $count) = @_;
    my @names = sort { $a cmp $b } @$names;
    return @names[grep { $_ % $count == $index - 1 } 0 .. $#names];
}

# The names of libtest's terse list: lines `name: test`; any other line (the
# summary, blank lines) names no test.
sub listed_names {
    my ($listing) = @_;
    return map { /^(.+): (?:test|benchmark)$/ ? ($1) : () } split /\n/, $listing;
}

# The sum of passed, failed and ignored over the summaries libtest printed,
# one for each test binary cargo ran.
sub reported_total {
    my ($text) = @_;
    my ($total, $summaries) = (0, 0);
    while ($text =~ /^test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;/mg) {
        $total += $1 + $2 + $3;
        $summaries++;
    }
    return ($total, $summaries);
}

# The text a command printed to standard output, copied to ours as it comes
# when $stream is set, and its status; a signal is 128 plus its number. A
# streamed command's standard error joins its standard output in one pipe, so
# cargo's `Running` line for a test binary stays before that binary's results
# in what the gate's log and .github/junit.py read.
sub command {
    my ($stream, @command) = @_;
    my $pid = open my $output, '-|';
    defined $pid or die "test shards: fork: $!\n";
    if (!$pid) {
        open STDERR, '>&', \*STDOUT if $stream;
        exec @command or die "test shards: execute @command: $!\n";
    }
    my $text = '';
    while (my $line = <$output>) {
        $text .= $line;
        print $line if $stream && !$quiet;
    }
    close $output;
    my $status = $?;
    return ($text, ($status & 127) ? 128 + ($status & 127) : $status >> 8);
}

sub cargo_test {
    my ($cargo, @targets) = @_;
    return ($cargo, 'test', '--profile', 'gate', @targets, '--locked', '--offline');
}

# Run the tests named (exact) or all but those skipped, require libtest to
# report the number expected, and give cargo's status. The walk's shard
# reaches only the run that names the walk.
sub run_selected {
    my ($runner, $label, $expected, $walk_shard, @command) = @_;
    my $status;
    my $text;
    {
        local $ENV{WHITEFOOT_CONFORMANCE_SHARD};
        if (defined $walk_shard) {
            $ENV{WHITEFOOT_CONFORMANCE_SHARD} = $walk_shard;
        } else {
            delete $ENV{WHITEFOOT_CONFORMANCE_SHARD};
        }
        ($text, $status) = $runner->(1, @command);
    }
    return $status if $status;
    my ($reported, $summaries) = reported_total($text);
    $summaries > 0 or die "test shards: $label: libtest printed no summary\n";
    $reported == $expected or die "test shards: $label: selected $expected tests, libtest reported $reported "
        . "(passed + failed + ignored)\n";
    print "test shards: $label: $reported tests reported, as selected\n" unless $quiet;
    return 0;
}

sub run_unit {
    my ($cargo, $index, $count, $runner) = @_;
    my @test = cargo_test($cargo, '--lib');
    my ($listing, $status) = $runner->(0, @test, '--', '--list', '--format', 'terse');
    return $status if $status;
    my @names = listed_names($listing);
    my %seen;
    $seen{$_}++ and die "test shards: unit: the test '$_' is listed twice\n" for @names;
    my @selected = partition(\@names, $index, $count);
    @selected or die "test shards: unit $index/$count selects none of the " . scalar(@names) . " tests\n";
    print "test shards: unit $index/$count selects ", scalar(@selected), " of ", scalar(@names), " tests\n" unless $quiet;
    return run_selected($runner, "unit $index/$count", scalar(@selected), undef,
        @test, '--', '--exact', @selected);
}

sub run_corpus {
    my ($cargo, $selection, $runner) = @_;
    my ($part, $index, $count) = @$selection;
    my @test = cargo_test($cargo, @corpus_targets);
    my ($listing, $status) = $runner->(0, @test, '--', '--list', '--format', 'terse');
    return $status if $status;
    my @names = listed_names($listing);
    for my $name ($walk, $receipts) {
        my $listed = grep { $_ eq $name } @names;
        $listed == 1 or die "test shards: corpus: the test '$name' is listed $listed times, expected once\n";
    }
    if ($part eq 'walk') {
        return run_selected($runner, "corpus walk $index/$count", 1, "$index/$count",
            @test, '--', '--exact', '--nocapture', $walk);
    }
    if ($part eq 'receipts') {
        return run_selected($runner, 'corpus receipts', 1, undef,
            @test, '--', '--exact', '--nocapture', $receipts);
    }
    return run_selected($runner, 'corpus rest', scalar(@names) - 2, undef,
        @test, '--', '--exact', '--skip', $walk, '--skip', $receipts);
}

# The same orchestration against a stand-in for cargo and libtest, which
# reports what libtest would for the arguments it was given. Only `command`
# starts real children, to check the pipe, the status and the signal.
sub check {
    my ($condition, $name) = @_;
    $condition or die "test shards self-test: $name\n";
}

sub dies_with {
    my ($action, $pattern, $name) = @_;
    my $passed = eval { $action->(); 1 };
    my $message = $@;
    my $matched = $message =~ $pattern ? 1 : 0;
    check(!$passed && $matched, "$name: expected failure matching $pattern, got " . ($passed ? 'success' : $message));
}

sub self_test {
    $quiet = 1;
    my @names = map { "m::t$_" } 0 .. 9;
    my @shards = map { [partition(\@names, $_, 3)] } 1 .. 3;
    check(join(',', @{$shards[0]}) eq 'm::t0,m::t3,m::t6,m::t9', 'shard 1/3 takes positions 0 3 6 9');
    my %all = map { $_ => 1 } map { @$_ } @shards;
    check(keys(%all) == 10 && 10 == map({ @$_ } @shards), 'the shards are disjoint and complete');
    check(join(',', partition([qw(b a B)], 1, 1)) eq 'B,a,b', 'names sort bytewise');

    for my $bad ('0/3', '4/3', '1/0', '1', 'a/b', '1/3/4', '', 'walk-1/3') {
        dies_with(sub { parse_shard($bad) }, qr/a shard is I\/N/, "unit shard '$bad'");
    }
    for my $bad ('walk', 'walk-4/3', 'walk-0/3', 'rest/3', 'receipt', 'walk-') {
        dies_with(sub { parse_corpus_part($bad) }, qr/corpus part is|a shard is/, "corpus part '$bad'");
    }
    check(join(',', parse_corpus_part('walk-2/3')) eq 'walk,2,3', 'a walk part names its shard');

    # Stand-in cargo: listings by target, and libtest's own exact or substring
    # selection over the listed names, recorded with the environment variable.
    my (@calls, %listing_of);
    my $runner = sub {
        my ($stream, @command) = @_;
        push @calls, {command => [@command], shard => $ENV{WHITEFOOT_CONFORMANCE_SHARD}, present => (exists $ENV{WHITEFOOT_CONFORMANCE_SHARD} ? 1 : 0)};
        my $key = join(' ', grep { $_ eq '--lib' } @command) || 'corpus';
        my @listed = @{$listing_of{$key}};
        if (grep { $_ eq '--list' } @command) {
            return (join('', map { "$_: test\n" } @listed) . scalar(@listed) . " tests, 0 benchmarks\n", 0);
        }
        my $after = (grep { $command[$_] eq '--' } 0 .. $#command)[0];
        my @arguments = @command[$after + 1 .. $#command];
        my $exact = grep { $_ eq '--exact' } @arguments;
        my (@filters, @skips);
        for my $i (0 .. $#arguments) {
            push @skips, $arguments[$i + 1] if $arguments[$i] eq '--skip';
            push @filters, $arguments[$i] if $arguments[$i] !~ /^--/ && ($i == 0 || $arguments[$i - 1] ne '--skip');
        }
        my $matches = sub {
            my ($name, $filter) = @_;
            return $exact ? $name eq $filter : index($name, $filter) >= 0;
        };
        my @run = grep {
            my $name = $_;
            (!@filters || grep { $matches->($name, $_) } @filters) && !grep { $matches->($name, $_) } @skips;
        } @listed;
        return ("compiling...\n", 0) if $ENV{SELF_TEST_NO_SUMMARY};
        return ("test result: FAILED. " . (@run - 1) . " passed; 1 failed; 0 ignored; 0 measured; 0 filtered out\n", 101)
            if $ENV{SELF_TEST_FAILURE};
        my $short = $ENV{SELF_TEST_REPORT_SHORT} ? 1 : 0;
        return ("test result: ok. " . (@run - $short) . " passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n", 0);
    };

    # Unit: shard 2/3 of ten tests runs exactly its names, with no variable.
    %listing_of = ('--lib' => [@names]);
    @calls = ();
    check(run_unit('cargo', 2, 3, $runner) == 0, 'unit shard passes');
    my @run_call = grep { !grep { $_ eq '--list' } @{$_->{command}} } @calls;
    check(@run_call == 1, 'unit runs one command');
    check(join(' ', @{$run_call[0]{command}}) eq 'cargo test --profile gate --lib --locked --offline -- --exact m::t1 m::t4 m::t7',
        'unit shard 2/3 runs positions 1 4 7 by exact name');
    check(!$run_call[0]{present}, 'a unit shard carries no walk variable');
    {
        local $ENV{SELF_TEST_REPORT_SHORT} = 1;
        dies_with(sub { run_unit('cargo', 1, 3, $runner) }, qr/selected 4 tests, libtest reported 3/, 'a missing result');
    }
    {
        local $ENV{SELF_TEST_NO_SUMMARY} = 1;
        dies_with(sub { run_unit('cargo', 1, 3, $runner) }, qr/printed no summary/, 'a run with no libtest summary');
    }
    {
        local $ENV{SELF_TEST_FAILURE} = 1;
        check(run_unit('cargo', 1, 3, $runner) == 101, 'a failing test gives cargo\'s own status');
    }
    dies_with(sub { run_unit('cargo', 11, 11, $runner) },
        qr/selects none/, 'a shard past the test count');
    %listing_of = ('--lib' => ['m::t0', 'm::t0']);
    dies_with(sub { run_unit('cargo', 1, 1, $runner) }, qr/listed twice/, 'a name listed twice');

    # Corpus: the walk and receipt tests named in full, and a longer name that
    # merely contains the walk's.
    my $longer = "$walk" . '_with_more';
    my @corpus = ('a::one', 'b::two', $walk, $receipts, $longer, "x::$receipts");
    %listing_of = (corpus => [@corpus]);
    @calls = ();
    check(run_corpus('cargo', ['walk', 2, 3], $runner) == 0, 'the walk part passes');
    @run_call = grep { !grep { $_ eq '--list' } @{$_->{command}} } @calls;
    check(join(' ', @{$run_call[0]{command}}) eq "cargo test --profile gate --bins --test * --locked --offline -- --exact --nocapture $walk",
        'the walk part runs the walk by exact full name');
    check($run_call[0]{shard} eq '2/3', 'the walk part carries its shard');
    @calls = ();
    check(run_corpus('cargo', ['receipts'], $runner) == 0, 'the receipts part passes');
    @run_call = grep { !grep { $_ eq '--list' } @{$_->{command}} } @calls;
    check($run_call[0]{command}[-1] eq $receipts && !$run_call[0]{present},
        'the receipts part runs the receipt test alone, with no variable');
    {
        local $ENV{WHITEFOOT_CONFORMANCE_SHARD} = '3/3';
        @calls = ();
        check(run_corpus('cargo', ['rest'], $runner) == 0, 'the rest part passes');
        @run_call = grep { !grep { $_ eq '--list' } @{$_->{command}} } @calls;
        check(!$run_call[0]{present}, 'the rest part clears an ambient walk variable');
        check(join(' ', @{$run_call[0]{command}}[-6 .. -1]) eq "-- --exact --skip $walk --skip $receipts",
            'the rest part skips the two tests by exact full name');
    }
    {
        local $ENV{SELF_TEST_REPORT_SHORT} = 1;
        dies_with(sub { run_corpus('cargo', ['rest'], $runner) }, qr/selected 4 tests, libtest reported 3/,
            'a corpus part that reports fewer tests');
    }
    for my $missing ($walk, $receipts) {
        %listing_of = (corpus => [grep { $_ ne $missing } @corpus]);
        dies_with(sub { run_corpus('cargo', ['rest'], $runner) }, qr/listed 0 times/, 'a renamed test');
    }
    %listing_of = (corpus => [@corpus, $walk]);
    dies_with(sub { run_corpus('cargo', ['walk', 1, 3], $runner) }, qr/listed 2 times/, 'a test listed twice');

    # The real pipe: output, status and signal.
    my ($text, $status) = command(0, 'perl', '-e', 'print "a\n"; exit 3');
    check($text eq "a\n" && $status == 3, 'command reports output and exit status');
    ($text, $status) = command(1, 'perl', '-e', '$| = 1; print STDERR "first\n"; print "second\n"');
    check($text eq "first\nsecond\n" && $status == 0, 'a streamed command keeps its error output before its output');
    ($text, $status) = command(0, 'perl', '-e', 'kill 9, $$');
    check($status == 137, 'command reports a signal as 128 plus its number');
    print "test shards: self-test ok\n";
}
