#!/usr/bin/env perl
# libtest has no shard option. Use its own list and exact filters, keeping
# selected names together in each invocation and checking its native summary.
# Only the named corpus tests receive a case shard, in a separate invocation.
# A stale name is intersected with libtest's list; an unlisted reader runs
# its whole case list once in its ordinary name shard. A listed nonreader
# repeats its full work on every shard; drift can add work, never omit cases.
use strict;
use warnings;
use JSON::PP qw(decode_json encode_json);

my $usage = "usage: shard-tests.pl --self-test | --validate SHARD | --partition SHARD | --whole-exact CARGO TEST-TARGET NAME | GROUP SHARD CARGO\n";
my %case_tests = map { $_ => 1 } (
    'conformance::adapter::the_corpus_reaches_its_declared_verdict_through_the_ordinary_compiler_path',
    'conformance::adapter::a_shared_proof_receipt_cache_reaches_every_declared_source_verdict',
);
exit main(@ARGV);

sub main {
    if (@_ == 1 && $_[0] eq '--self-test') {
        self_test();
        return 0;
    }
    if (@_ && $_[0] eq '--whole-exact') {
        @_ == 4 or die $usage;
        $| = 1;
        return run_whole_exact($_[1], $_[2], $_[3], \&command);
    }
    @_ >= 2 or die $usage;
    my ($mode, $shard, $cargo) = @_;
    my ($index, $count) = parse_shard($shard);
    if ($mode eq '--validate') {
        @_ == 2 or die $usage;
        return 0;
    }
    if ($mode eq '--partition') {
        @_ == 2 or die $usage;
        my @names = <STDIN>;
        chomp @names;
        print "$_\n" for partition(\@names, $index, $count);
        return 0;
    }
    @_ == 3 && ($mode eq 'unit' || $mode eq 'corpus') or die $usage;
    $| = 1;
    return run_group($mode, $shard, $cargo, \&command);
}

sub parse_shard {
    my ($shard) = @_;
    $shard =~ m{^([1-9][0-9]*)/([1-9][0-9]*)$} && $1 <= $2
        or die "test shards: SHARD must be i/N with 1 <= i <= N, got '$shard'\n";
    return ($1, $2);
}

sub partition {
    my ($names, $index, $count) = @_;
    my @names = sort { $a cmp $b } @$names;
    return @names[grep { $_ % $count == $index - 1 } 0 .. $#names];
}

sub listed_names {
    my ($listing, $where) = @_;
    my (@names, %seen);
    for my $line (split /\n/, $listing) {
        next if $line eq '';
        $line =~ /^(.*): (?:test|benchmark)$/
            or die "test shards: unexpected libtest list line for $where: $line\n";
        my $name = $1;
        $name ne '' && !$seen{$name}++
            or die "test shards: empty or duplicate test name for $where: '$name'\n";
        push @names, $name;
    }
    return @names;
}

sub targets {
    my ($metadata) = @_;
    my @packages = grep { $_->{manifest_path} eq "$metadata->{workspace_root}/Cargo.toml" }
        @{$metadata->{packages}};
    @packages == 1 or die "test shards: expected one compiler package at the current manifest\n";
    my @targets;
    for my $kind ('bin', 'test') {
        for my $target (@{$packages[0]->{targets}}) {
            push @targets, ["--$kind", $target->{name}]
                if grep { $_ eq $kind } @{$target->{kind}};
        }
    }
    @targets or die "test shards: Cargo found no bin or integration targets\n";
    return @targets;
}

sub plan_invocations {
    my ($target, $names, $index, $count) = @_;
    my @repeated = @$target == 2 && $target->[0] eq '--test' && $target->[1] eq 'corpus'
        ? grep { $case_tests{$_} } @$names : ();
    my %repeated = map { $_ => 1 } @repeated;
    my @ordinary = grep { !$repeated{$_} } @$names;
    return ([partition(\@ordinary, $index, $count)], \@repeated);
}

sub completeness {
    my ($result, $selected, $where) = @_;
    my @reported = $result =~ /^test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;/mg;
    @reported == 3 or die "test shards: $where expected one libtest summary, found "
        . (@reported / 3) . "\n";
    my $reported = $reported[0] + $reported[1] + $reported[2];
    $reported == @$selected or die "test shards: $where completeness mismatch: selected "
        . scalar(@$selected) . " names, libtest reported $reported (passed + failed + ignored)\n";
    return $reported;
}

# Read stdout without a shell pipeline; close reports Cargo's own status.
# Running output is copied immediately so the gate's completion gaps survive.
sub command {
    my ($stream, @command) = @_;
    open my $output, '-|', @command or die "test shards: execute @command: $!\n";
    my $text = '';
    while (my $line = <$output>) {
        $text .= $line;
        print $line if $stream;
    }
    close $output;
    my $status = $?;
    return ($text, ($status & 127) ? 128 + ($status & 127) : $status >> 8);
}

sub run_tests {
    my ($runner, $cargo, $target, $selected, $case_shard, $shard, $show_output) = @_;
    my @test = ($cargo, 'test', '--profile', 'gate', @$target, '--locked', '--offline');
    # An empty exact filter cannot match a nonempty Rust test name. Omitting
    # filters instead would run the entire target on an empty shard.
    my @filters = @$selected ? @$selected : ('');
    local $ENV{WHITEFOOT_CASE_SHARD};
    if (defined $case_shard) {
        $ENV{WHITEFOOT_CASE_SHARD} = $case_shard;
    } else {
        delete $ENV{WHITEFOOT_CASE_SHARD};
    }
    my ($result, $status) = $runner->(1, @test, '--', '--exact', @filters,
        defined $case_shard || $show_output ? ('--nocapture') : ());
    my $reported = completeness($result, $selected, "@$target $shard");
    print "test shards: @$target $shard completeness checked: $reported reported\n";
    return $status;
}

sub run_whole_exact {
    my ($cargo, $target, $name, $runner) = @_;
    $cargo ne '' && $target ne '' && $name ne '' or die $usage;
    return run_tests($runner, $cargo, ['--test', $target], [$name], undef, '1/1', 1);
}

sub run_group {
    my ($mode, $shard, $cargo, $runner) = @_;
    my ($index, $count) = parse_shard($shard);
    local $ENV{WHITEFOOT_CASE_SHARD};
    delete $ENV{WHITEFOOT_CASE_SHARD};
    my @targets = (['--lib']);
    if ($mode eq 'corpus') {
        my ($text, $status) = $runner->(0, $cargo, 'metadata', '--no-deps',
            '--format-version', '1', '--locked', '--offline');
        return $status if $status;
        @targets = targets(decode_json($text));
    }
    for my $target (@targets) {
        my @test = ($cargo, 'test', '--profile', 'gate', @$target, '--locked', '--offline');
        my ($listing, $status) = $runner->(0, @test, '--', '--list', '--format', 'terse');
        return $status if $status;
        # Terse lists include ignored tests without a marker; runs pass no
        # ignore override, preserving their native ignored verdict.
        my @names = listed_names($listing, "@$target");
        my ($selected, $repeated) = plan_invocations($target, \@names, $index, $count);
        print "test shards: @$target $shard selected ", scalar(@$selected),
            " of ", scalar(@names), " names\n";
        $status = run_tests($runner, $cargo, $target, $selected, undef, $shard);
        return $status if $status;
        if (@$repeated) {
            print "test shards: @$target $shard repeats ", scalar(@$repeated), " case-sharded tests\n";
            $status = run_tests($runner, $cargo, $target, $repeated, $shard, $shard);
            return $status if $status;
        }
    }
    return 0;
}

# The same orchestration consumes synthetic Cargo replies without launching
# Cargo processes. Only command's OS pipe/status boundary needs real children.
sub check {
    my ($condition, $name) = @_;
    $condition or die "test shards self-test: $name\n";
}
sub same {
    my ($actual, $expected, $name) = @_;
    check(encode_json($actual) eq encode_json($expected), $name);
}
sub refuses {
    my ($action, $message) = @_;
    my $passed = eval { $action->(); 1 };
    check(!$passed && $@ =~ $message, "expected failure matching $message, got " . ($@ || 'success'));
}
sub capture {
    my ($action) = @_;
    my $output = '';
    local *STDOUT;
    open STDOUT, '>', \$output or die "capture self-test output: $!";
    my @result = $action->();
    return ($output, @result);
}

sub self_test {
    my @names = qw(A a aa b c d e f g h i j k l m z);
    for my $count (1 .. 5) {
        my @union;
        for my $index (1 .. $count) {
            my @selected = partition(\@names, parse_shard("$index/$count"));
            my @expected;
            for (my $position = $index - 1; $position < @names; $position += $count) {
                push @expected, $names[$position];
            }
            same(\@selected, \@expected, "round robin $index/$count");
            push @union, @selected;
        }
        same([sort @union], \@names, "coverage and multiplicity for $count shards");
    }
    same([partition(\@names, 1, 1)], \@names, '1/1 identity');
    same([partition([qw(z a A aa)], 1, 1)], [qw(A a aa z)], 'bytewise sort');
    same([partition([], 3, 3)], [], 'empty list');
    for my $bad ('0/3', '4/3', '1/0', '1', '1/2/3', '') {
        refuses(sub { main('--validate', $bad) }, qr/SHARD must be/);
    }
    check(main('--validate', '2/3') == 0, 'validate CLI');
    {
        local *STDIN;
        my $input = "z\na\nA\naa\n";
        open STDIN, '<', \$input or die $!;
        my ($output, $status) = capture(sub { main('--partition', '1/1') });
        same([$output, $status], ["A\na\naa\nz\n", 0], 'partition CLI');
    }
    same([listed_names("a: test\nb: benchmark\n", 'sample')], [qw(a b)], 'native list kinds');
    for my $bad ("not a test\n", ": test\n", "a: test\na: test\n") {
        refuses(sub { listed_names($bad, 'sample') }, qr/unexpected|empty or duplicate/);
    }

    # Fixture names independently describe the two allowlisted native tests.
    my $ordinary = 'conformance::adapter::the_corpus_reaches_its_declared_verdict_through_the_ordinary_compiler_path';
    my $cached = 'conformance::adapter::a_shared_proof_receipt_cache_reaches_every_declared_source_verdict';
    my $reader = 'conformance::adapter::renamed_case_reader';
    my $metadata = {workspace_root => '/compiler', packages => [
        {manifest_path => '/compiler/Cargo.toml', targets => [
            {kind => ['lib'], name => 'whitefoot'},
            {kind => ['test'], name => 'corpus', test => 1},
            {kind => ['test'], name => 'discovered', test => 0},
            {kind => ['bin'], name => 'whitefootc', test => 1},
        ]},
        {manifest_path => '/dependency/Cargo.toml', targets => []},
    ]};
    same([targets($metadata)], [['--bin', 'whitefootc'], ['--test', 'corpus'],
        ['--test', 'discovered']], 'Cargo discovery, bin order and explicit test=false');

    my $trial = sub {
        my ($group, $shard, $fixture, $mode) = @_;
        my ($full_readers, $nonreaders, $calls) = (0, 0, 0);
        my $runner = sub {
            my ($stream, @command) = @_;
            $calls++;
            if ($command[1] eq 'metadata') {
                check(!exists $ENV{WHITEFOOT_CASE_SHARD} && !$stream, 'metadata environment');
                same(\@command, [qw(cargo metadata --no-deps --format-version 1 --locked --offline)],
                    'metadata command');
                return (encode_json($metadata), $mode eq 'metadata-failed' ? 17 : 0);
            }
            my @names = qw(a::test b::test c::test d::ignored e::test f::test g::test);
            my @target = ($command[4]);
            push @target, $command[5] if $command[4] ne '--lib';
            my $target = $target[1] // '';
            @names = qw(a::test d::ignored) if $target eq 'whitefootc';
            @names = () if $target eq 'discovered';
            my @case_tests;
            if ($target eq 'corpus' && $fixture ne '') {
                @case_tests = ($ordinary, $cached);
                @case_tests = ($cached) if $fixture eq 'renamed';
                @case_tests = () if $fixture eq 'absent';
                push @names, @case_tests;
                push @names, $reader if $fixture eq 'renamed' || $fixture eq 'absent';
            }
            @names = ($ordinary) if $target eq 'discovered' && $fixture ne '';
            my ($tail) = grep { $command[$_] eq '--' } 0 .. $#command;
            check(defined $tail, 'test argument separator');
            same([@command[0 .. $tail - 1]], [qw(cargo test --profile gate), @target,
                qw(--locked --offline)], 'Cargo test flags');
            my @args = @command[$tail + 1 .. $#command];
            check(!(grep { /^--(?:include-ignored|ignored)$/ } @args), 'no ignore override');
            if ($args[0] eq '--list') {
                check(!exists $ENV{WHITEFOOT_CASE_SHARD} && !$stream, 'listing environment');
                same(\@args, [qw(--list --format terse)], 'native listing flags');
                return (join('', map { "$_: test\n" } @names), $mode eq 'list-failed' ? 17 : 0);
            }
            check($stream && shift(@args) eq '--exact', 'stream and exact filters');
            my $case_shard = $ENV{WHITEFOOT_CASE_SHARD};
            if (defined $case_shard) {
                check($target eq 'corpus' && $case_shard eq $shard, 'case environment scope');
                check(pop(@args) eq '--nocapture', 'repeated diagnostics');
                same([sort @args], [sort @case_tests], 'only present case tests repeat');
            } else {
                check(!(grep { $_ eq '--nocapture' } @args), 'ordinary output handling');
                my %repeated = map { $_ => 1 } @case_tests;
                check(!(grep { $repeated{$_} } @args), 'case tests removed from ordinary partition');
                my @ordinary = sort grep { !$repeated{$_} } @names;
                my ($index, $count) = split '/', $shard;
                my @expected;
                for (my $position = $index - 1; $position < @ordinary; $position += $count) {
                    push @expected, $ordinary[$position];
                }
                same(\@args, @expected ? \@expected : [''], 'ordinary filters and empty exact filter');
            }
            check(@args > 0, 'empty selections keep an exact empty filter');
            my %filters = map { $_ => 1 } @args;
            my @ran = grep { $filters{$_} } @names;
            $full_readers += grep { $_ eq $reader } @ran;
            $nonreaders += grep { $_ eq $ordinary } @ran
                if $fixture eq 'nonreading' && $target eq 'corpus';
            my $ignored = grep { /ignored$/ } @ran;
            my $passed = @ran - $ignored;
            return ('', 0) if $mode eq 'missing';
            $passed++ if $mode eq 'extra';
            $passed = $ignored = 0 if $mode eq 'unmatched'
                || ($mode eq 'repeated-unmatched' && defined $case_shard);
            my $failed = $mode eq 'failed' ? $passed-- && 1 : 0;
            my $summary = 'test result: ' . ($failed ? 'FAILED' : 'ok')
                . ". $passed passed; $failed failed; $ignored ignored; 0 measured; 0 filtered out; finished in 0.00s\n";
            $summary .= $summary if $mode eq 'duplicate';
            print $summary;
            return ($summary, $failed ? 17 : 0);
        };
        local $ENV{WHITEFOOT_CASE_SHARD} = 'invalid-inherited';
        my ($output, $status) = capture(sub { run_group($group, $shard, 'cargo', $runner) });
        check($ENV{WHITEFOOT_CASE_SHARD} eq 'invalid-inherited', 'environment restored after group');
        return ($output, $status, $full_readers, $nonreaders, $calls);
    };
    my ($output, $status) = $trial->('unit', '1/3', '', '');
    check($status == 0 && $output =~ /selected 3 of 7 names/
        && $output =~ /2 passed; 0 failed; 1 ignored;/ && $output =~ /completeness checked: 3/,
        'ordinary selection and ignored summary');
    ($output, $status) = $trial->('corpus', '1/3', '', '');
    check($status == 0 && $output =~ /--bin whitefootc 1\/3 completeness checked: 1/
        && $output =~ /--test corpus 1\/3 completeness checked: 3/
        && $output =~ /--test discovered 1\/3 completeness checked: 0/, 'per-binary completeness');
    for my $mode ('unmatched', 'extra', 'missing', 'duplicate') {
        refuses(sub { $trial->('unit', '1/3', '', $mode) },
            $mode eq 'missing' || $mode eq 'duplicate' ? qr/expected one libtest summary/ : qr/completeness mismatch/);
    }
    ($output, $status) = $trial->('unit', '1/3', '', 'failed');
    check($status == 17 && $output =~ /completeness checked: 3/, 'native failed test status');
    for my $fixture ('both', 'renamed', 'absent', 'nonreading') {
        my $full_readers = 0;
        for my $index (1 .. 3) {
            my ($output, $status, $readers, $nonreaders) = $trial->('corpus', "$index/3", $fixture, '');
            my $repeated = $fixture eq 'absent' ? 0 : $fixture eq 'renamed' ? 1 : 2;
            check($status == 0, "case fixture $fixture $index/3");
            check($output =~ /--test discovered $index\/3 completeness checked:/,
                'same allowlisted spelling in another target stays ordinary');
            check($repeated ? $output =~ /repeats $repeated case-sharded tests/
                : $output !~ /repeats .*case-sharded tests/, 'allowlist intersection');
            check($nonreaders == 1, 'listed nonreader repeats full work') if $fixture eq 'nonreading';
            $full_readers += $readers;
        }
        check($full_readers == ($fixture eq 'renamed' || $fixture eq 'absent' ? 1 : 0),
            'unlisted reader checks all cases exactly once');
    }
    refuses(sub { $trial->('corpus', '2/3', 'both', 'repeated-unmatched') },
        qr/completeness mismatch: selected 2 names, libtest reported 0/);
    for my $mode ('metadata-failed', 'list-failed') {
        my (undef, $status, undef, undef, $calls) = $trial->('corpus', '1/3', '', $mode);
        check($status == 17 && $calls == ($mode eq 'metadata-failed' ? 1 : 2), 'discovery/list failure stops');
    }

    # Whole exact runs use the same native count assertion, without listing
    # or case sharding. A stale filter must fail even when Cargo exits zero.
    my $whole = sub {
        my ($reported, $status) = @_;
        local $ENV{WHITEFOOT_CASE_SHARD} = '2/3';
        local $ENV{SHARD} = '2/3';
        my @result = capture(sub {
            run_whole_exact('cargo', 'corpus', 'whole::receipt_test', sub {
                my ($stream, @command) = @_;
                check($stream && !exists $ENV{WHITEFOOT_CASE_SHARD}, 'whole case environment absent');
                same(\@command, [qw(cargo test --profile gate --test corpus --locked --offline
                    -- --exact whole::receipt_test --nocapture)], 'whole exact native arguments');
                my $summary = "test result: ok. $reported passed; 0 failed; 0 ignored;\n";
                print $summary;
                return ($summary, $status);
            });
        });
        check($ENV{WHITEFOOT_CASE_SHARD} eq '2/3', 'whole run restores inherited environment');
        return @result;
    };
    ($output, $status) = $whole->(1, 0);
    check($status == 0 && $output =~ /completeness checked: 1 reported/, 'whole native count one');
    refuses(sub { $whole->(0, 0) }, qr/completeness mismatch: selected 1 names, libtest reported 0/);
    refuses(sub { $whole->(2, 0) }, qr/completeness mismatch: selected 1 names, libtest reported 2/);
    ($output, $status) = $whole->(1, 17);
    check($status == 17, 'whole run preserves native failure');

    # Two real children retain OS argv, streaming, exit and signal evidence.
    my ($streamed, $text, $code) = capture(sub {
        command(1, $^X, '-e', 'print join("\n", @ARGV), "\n"; exit 17;',
            'a::test', 'b::test', '', 'a b');
    });
    same([$streamed, $text, $code], ["a::test\nb::test\n\na b\n", "a::test\nb::test\n\na b\n", 17],
        'real pipe argv, streamed stdout and exit status');
    ($text, $code) = command(0, $^X, '-e', 'kill "TERM", $$;');
    same([$text, $code], ['', 143], 'real pipe signal status');
    print "test shards: partition, exact filters, target discovery, ignored counts and completeness pass\n";
}
