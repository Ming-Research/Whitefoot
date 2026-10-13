#!/usr/bin/env perl
# libtest has no shard option. Use its own list and exact filters, preserving
# one test process per Cargo target and checking its native summary.
use strict;
use warnings;
use JSON::PP qw(decode_json);

my $usage = "usage: shard-tests.pl --validate SHARD | --partition SHARD | GROUP SHARD CARGO\n";
@ARGV >= 2 or die $usage;
my ($mode, $shard, $cargo) = @ARGV;
$shard =~ m{^([1-9][0-9]*)/([1-9][0-9]*)$} && $1 <= $2
    or die "test shards: SHARD must be i/N with 1 <= i <= N, got '$shard'\n";
my ($index, $count) = ($1, $2);
if ($mode eq '--validate') {
    @ARGV == 2 or die $usage;
    exit 0;
}

sub partition {
    my @names = sort { $a cmp $b } @_;
    return @names[grep { $_ % $count == $index - 1 } 0 .. $#names];
}
if ($mode eq '--partition') {
    @ARGV == 2 or die $usage;
    my @names = <STDIN>;
    chomp @names;
    print "$_\n" for partition(@names);
    exit 0;
}
@ARGV == 3 && ($mode eq 'unit' || $mode eq 'corpus') or die $usage;
$| = 1;

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

my @targets = (['--lib']);
if ($mode eq 'corpus') {
    my ($text, $status) = command(0, $cargo, 'metadata', '--no-deps',
        '--format-version', '1', '--locked', '--offline');
    exit $status if $status;
    my $metadata = decode_json($text);
    my @packages = grep { $_->{manifest_path} eq "$metadata->{workspace_root}/Cargo.toml" }
        @{$metadata->{packages}};
    @packages == 1 or die "test shards: expected one compiler package at the current manifest\n";
    @targets = ();
    for my $kind ('bin', 'test') {
        for my $target (@{$packages[0]->{targets}}) {
            push @targets, ["--$kind", $target->{name}]
                if grep { $_ eq $kind } @{$target->{kind}};
        }
    }
    @targets or die "test shards: Cargo found no bin or integration targets\n";
}

for my $target (@targets) {
    my @test = ($cargo, 'test', '--profile', 'gate', @$target, '--locked', '--offline');
    my ($listing, $listed_status) = command(0, @test, '--', '--list', '--format', 'terse');
    exit $listed_status if $listed_status;
    my (@names, %seen);
    for my $line (split /\n/, $listing) {
        next if $line eq '';
        $line =~ /^(.*): (?:test|benchmark)$/
            or die "test shards: unexpected libtest list line for @$target: $line\n";
        my $name = $1;
        $name ne '' && !$seen{$name}++
            or die "test shards: empty or duplicate test name for @$target: '$name'\n";
        push @names, $name;
    }
    # Terse lists include ignored tests without a marker. No ignore override
    # is passed when running, so their native ignored verdict stays unchanged.
    my @selected = partition(@names);
    print "test shards: @$target $shard selected ", scalar(@selected),
        " of ", scalar(@names), " names\n";
    # An empty exact filter cannot match a nonempty Rust test name. Omitting
    # filters instead would run the entire target on an empty shard.
    my ($result, $status) = command(1, @test, '--', '--exact', @selected ? @selected : ('') );
    my @reported = $result =~ /^test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;/mg;
    @reported == 3 or die "test shards: @$target $shard expected one libtest summary, found "
        . (@reported / 3) . "\n";
    my $reported = $reported[0] + $reported[1] + $reported[2];
    $reported == @selected or die "test shards: @$target $shard completeness mismatch: selected "
        . scalar(@selected) . " names, libtest reported $reported (passed + failed + ignored)\n";
    print "test shards: @$target $shard completeness checked: $reported reported\n";
    exit $status if $status;
}
