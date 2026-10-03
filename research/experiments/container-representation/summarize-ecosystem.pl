#!/usr/bin/env perl
use strict;
use warnings;

# Consumer: this experiment's ecosystem-summarize target. Retire with the
# ecosystem comparison. These drivers emit unquoted, comma-free scalar fields;
# this is deliberately not a general CSV package or a benchmark runner.
# Usage: perl summarize-ecosystem.pl [--complete] [--targets] vector=path.csv ...
# Repeated family arguments can combine cohort files, but never different work.
# Coverage is checked within observed cells against the input's variant, cohort
# and sample unions. --complete also checks the driver matrix below for each
# supplied family. Partial single-cohort input leaves stability unverified.
# WF/variant divides matched whole-trace medians: above 1 means WF is slower.
# The paired minimum covers both implementations' ranked samples. Cohort spread
# is 100 * (max ratio / min ratio - 1); above 10% is flagged. Ordered sample 0
# is validated and counted as recorded warmup, then excluded from statistics.
# --targets implies --complete and reports the owner target against the slower
# Rust/C++ standard median, with both cohorts and observed sample bounds.
# Its performance statuses are descriptive output, never a correctness gate.
my %headers = (
    vector => 'contract,cohort,element_bytes,path,count,variant,sample,work,rounds,traces,elapsed_ns,checksum',
    deque => 'contract,cohort,element_bytes,path,count,variant,sample,work,rounds,traces,elapsed_ns,checksum',
    map => 'contract,cohort,series,element_bytes,path,requested_capacity,count,hash,variant,sample,rounds,traces,elapsed_ns,checksum',
    priority => 'contract,cohort,series,element_bytes,path,count,variant,sample,seed,rounds,traces,work_multiplier,elapsed_ns,checksum',
    ordered => 'contract,pair_bytes,path,count,variant,sample,cohort,rounds,traces,elapsed_ns,checksum,requests,requested_bytes,peak_allocations,peak_bytes',
);
# Timed-driver inventory: payload bytes, count[:requested_capacity], paths,
# variants, fixed last sample (undef means at least seven sequence samples).
# Update beside measure() in vector/deque/priority *-costs.c and map/ordered
# *-ecosystem.c. This inventory validates omissions; it does not select samples.
my %matrix = (
    vector => ['8 256', '16 256 4096', 'reserved growth reuse suffix-0 suffix-1 suffix-2 suffix-3', 'whitefoot reverse-c direct-c swap-take-c take-swap-c rust-vec cpp-std-vector', undef],
    deque => ['8 256', '16 256 4096', 'forward-churn reverse-churn growth setup-cleanup', 'whitefoot loop-c bulk-c rust-vec-deque cpp-std-deque', undef],
    map => ['8 256', '2:3 56:64 3584:4096', 'hit miss replace-old-value remove-churn edit-first-word fill-free reserve-more-entries', 'whitefoot-hash-map rust-hash-map cpp-unordered-map absl-flat-hash-map c-sparse-direct', 10],
    priority => ['8 256', '16 256 4096', 'pop-push replace-top grow-pop heapify-pop setup-cleanup', 'whitefoot swap-c hole-c rust-binary-heap cpp-std-heap', 6],
    ordered => ['16 264', '8 256 4096', 'build-cleanup hit-miss replace-edit-remove-insert range-16 replace-only', 'whitefoot source-c direct-c avl-c rust-btree-map cpp-map absl-btree-map', 5],
);
my @cell_fields = qw(family contract payload_unit payload_bytes path count requested_capacity);
my @settings = qw(rounds traces work work_multiplier);
my @output_fields = (
    @cell_fields, qw(hash_series cohort variant comparison_class source_series hash),
    @settings, qw(recorded_samples recorded_sample_ids warmup_samples samples sample_ids
        minimum_ns median_ns maximum_ns whitefoot_median_ns wf_over_variant
        paired_minimum_ns sub_1ms cohort_ratio_spread_pct unstable),
);
my @target_fields = (@cell_fields, 'hash_series', @settings, qw(samples sample_ids),
    (map { my $cohort = $_; map { "${_}_$cohort" } qw(slower_standard wf_median_ns rust_median_ns cpp_median_ns absl_median_ns wf_over_slower observed_upper observed_lower) } 0, 1),
    qw(qualification_minimum_ns target_cohort_spread_pct selected_peer_spread_pct median_result status reason));

sub key { return join "\x1e", @_; }
sub median {
    my @values = sort { $a <=> $b } @_;
    my $middle = int(@values / 2);
    return @values % 2 ? $values[$middle] : ($values[$middle - 1] + $values[$middle]) / 2;
}
sub classification {
    my ($r) = @_;
    return 'overhead-control' if $r->{family} eq 'vector' && $r->{path} eq 'suffix-0';
    return 'storage-control' if $r->{family} eq 'priority' && $r->{source_series} eq 'storage-control';
    return 'reference' if $r->{variant} =~ /^whitefoot(?:-|$)/;
    return 'c-control' if $r->{variant} =~ /(?:^c-|-c$)/;
    return 'ecosystem' if $r->{variant} =~ /^(?:rust|cpp|absl)-/;
    die "unknown variant classification: $r->{variant}\n";
}

sub complete_coverage {
    my ($cells, $coverage, @families) = @_;
    my %expected;
    for my $family (sort @families) {
        my ($payloads, $shapes, $paths, $variants, $last) = @{$matrix{$family}};
        my $contract = $family =~ /\A(?:vector|deque)\z/ ? 'normal-o3' : 'normal';
        my $scope = $coverage->{key($family, $contract)} // die "complete $family missing contract: $contract\n";
        die "complete $family variant coverage mismatch\n"
            unless join(' ', sort keys %{$scope->{variant}}) eq join(' ', sort split / /, $variants);
        die "complete $family cohort coverage mismatch\n" unless join(' ', sort keys %{$scope->{cohort}}) eq '0 1';
        die "complete $family sample coverage mismatch\n" if defined $last
            && join(' ', sort { $a <=> $b } keys %{$scope->{sample}}) ne join(' ', 0 .. $last);
        die "complete $family requires at least 7 samples\n" if !defined $last && keys %{$scope->{sample}} < 7;
        for my $payload (split / /, $payloads) { for my $shape (split / /, $shapes) {
            my ($count, $capacity) = split /:/, $shape;
            for my $path (split / /, $paths) { for my $series ($family eq 'map' ? qw(native-default aligned-hash) : '') {
                my $id = key($family, $contract, $family eq 'ordered' ? 'pair_bytes' : 'element_bytes', $payload, $path, $count, $capacity // '', $series);
                $expected{$id} = 1;
                die "complete $family missing cell: $payload/$path/$shape/$series\n" unless exists $cells->{$id};
            }}
        }}
    }
    die "complete unexpected cell: " . join('/', split /\x1e/, $_) . "\n"
        for grep { !$expected{$_} } sort keys %$cells;
}

sub summarize {
    my ($complete, @sources) = @_;
    my (%cells, %coverage, %work, %checksums, %families);
    for my $source (@sources) {
        my ($family, $label, $fh) = @$source;
        die "unknown family: $family\n" unless exists $headers{$family};
        $families{$family} = 1;
        my $header = <$fh> // die "$label: empty input\n";
        $header =~ s/\r?\n\z//;
        die "$label: unexpected $family timing header\n" unless $header eq $headers{$family};
        my @columns = split /,/, $header;
        my $line = 1;
        while (my $text = <$fh>) {
            ++$line;
            $text =~ s/\r?\n\z//;
            my @values = split /,/, $text, -1;
            my $where = "$label:$line";
            die "$where: malformed scalar CSV row\n"
                unless @values == @columns && !grep { !/\A[A-Za-z0-9_.-]+\z/ } @values;
            my %r;
            @r{@columns} = @values;
            for my $column (@columns) {
                next if $column =~ /\A(?:contract|series|path|hash|variant)\z/;
                die "$where: invalid unsigned $column\n" unless $r{$column} =~ /\A(?:0|[1-9][0-9]*)\z/;
            }
            die "$where: nonpositive timing or trace count\n" if $r{elapsed_ns} eq '0' || $r{traces} eq '0';
            die "$where: unknown cohort\n" unless $r{cohort} eq '0' || $r{cohort} eq '1';
            die "$where: accounting or retained input is not a practical timing\n"
                unless $r{contract} eq 'normal' || $r{contract} eq 'normal-o3';
            $r{family} = $family;
            $r{payload_unit} = $family eq 'ordered' ? 'pair_bytes' : 'element_bytes';
            $r{payload_bytes} = $r{$r{payload_unit}};
            die "$where: nonpositive payload size\n" if $r{payload_bytes} eq '0';
            $r{$_} //= '' for qw(requested_capacity hash work work_multiplier);
            $r{source_series} = $r{series} // '';
            $r{hash_series} = $family eq 'map' ? $r{source_series} : '';
            die "$where: unknown hash series\n" if $family eq 'map'
                && $r{hash_series} !~ /\A(?:native-default|aligned-hash)\z/;
            $r{comparison_class} = classification(\%r);
            if ($family eq 'priority') {
                my $expected = $r{path} eq 'setup-cleanup' ? 'storage-control'
                    : $r{comparison_class} eq 'c-control' ? 'c-control' : 'practical';
                die "$where: priority classification disagrees with path/variant\n"
                    unless $r{source_series} eq $expected;
            }
            my $seed = '' . (101 + $r{sample});
            die "$where: seed does not match sample\n" if exists $r{seed} && $r{seed} ne $seed;
            my $semantic = key(@r{@cell_fields});
            my $settings = key(@r{@settings});
            die "$where: mismatched rounds/traces/work in observed cell\n"
                if exists $work{$semantic} && $work{$semantic} ne $settings;
            $work{$semantic} = $settings;
            my $checksum_key = key($semantic, $seed);
            # Never coerce checksums to numbers: adjacent u64 values above 2^53
            # must remain distinguishable across variants, hashers and cohorts.
            die "$where: checksum mismatch for seed $seed\n"
                if exists $checksums{$checksum_key} && $checksums{$checksum_key} ne $r{checksum};
            $checksums{$checksum_key} = $r{checksum};
            my $scope = key($family, $r{contract});
            $coverage{$scope}{$_}{$r{$_}} = 1 for qw(variant cohort sample);
            my $cell = $cells{key($semantic, $r{hash_series})} //= { scope => $scope, row => \%r, groups => {} };
            my $group = $cell->{groups}{key($r{cohort}, $r{variant})} //= { row => \%r, samples => {} };
            die "$where: variant metadata changed within cell\n"
                if $group->{row}{hash} ne $r{hash} || $group->{row}{source_series} ne $r{source_series};
            die "$where: duplicate sample\n" if exists $group->{samples}{$r{sample}};
            $group->{samples}{$r{sample}} = $r{elapsed_ns};
        }
        die "$label: no timing rows\n" if $line == 1;
    }
    complete_coverage(\%cells, \%coverage, keys %families) if $complete;
    for my $scope (values %coverage) {
        my @ids = sort { $a <=> $b } keys %{$scope->{sample}};
        die "missing sample ID in observed coverage\n" unless join(',', @ids) eq join(',', 0 .. $#ids);
        my @whitefoot = grep { /^whitefoot(?:-|$)/ } keys %{$scope->{variant}};
        die "expected one Whitefoot reference and a comparator\n"
            unless @whitefoot == 1 && keys %{$scope->{variant}} > 1;
        $scope->{whitefoot} = $whitefoot[0];
        $scope->{sample_ids} = \@ids;
    }
    my @output;
    for my $cell (map { $cells{$_} } sort keys %cells) {
        my $scope = $coverage{$cell->{scope}};
        my @recorded = @{$scope->{sample_ids}};
        my @ranked = grep { $cell->{row}{family} ne 'ordered' || $_ != 0 } @recorded;
        die "ordered input has no measured samples after warmup\n" unless @ranked;
        my $description = join '/', @{$cell->{row}}{@cell_fields}, $cell->{row}{hash_series};
        for my $cohort (sort keys %{$scope->{cohort}}) {
            for my $variant (sort keys %{$scope->{variant}}) {
                my $group = $cell->{groups}{key($cohort, $variant)};
                die "missing variant/cohort coverage: $description/$cohort/$variant\n" unless $group;
                for my $id (@recorded) {
                    die "missing sample $id: $description/$cohort/$variant\n"
                        unless exists $group->{samples}{$id};
                }
                my @times = sort { $a <=> $b } @{$group->{samples}}{@ranked};
                $group->{stats} = { minimum_ns => $times[0], median_ns => median(@times), maximum_ns => $times[-1] };
            }
        }
        for my $group (values %{$cell->{groups}}) {
            my $r = $group->{row};
            my $wf = $cell->{groups}{key($r->{cohort}, $scope->{whitefoot})};
            $group->{ratio} = $wf->{stats}{median_ns} / $group->{stats}{median_ns};
        }
        for my $cohort (sort keys %{$scope->{cohort}}) {
            for my $variant (sort keys %{$scope->{variant}}) {
                my $group = $cell->{groups}{key($cohort, $variant)};
                my $wf = $cell->{groups}{key($cohort, $scope->{whitefoot})};
                my $other = $cell->{groups}{key(1 - $cohort, $variant)};
                my %out = (%{$group->{row}}, %{$group->{stats}});
                @out{qw(recorded_samples recorded_sample_ids warmup_samples samples sample_ids)} =
                    (scalar @recorded, join(';', @recorded), @recorded - @ranked, scalar @ranked, join(';', @ranked));
                $out{whitefoot_median_ns} = $wf->{stats}{median_ns};
                $out{wf_over_variant} = sprintf '%.6f', $group->{ratio};
                $out{paired_minimum_ns} = $out{minimum_ns} < $wf->{stats}{minimum_ns} ? $out{minimum_ns} : $wf->{stats}{minimum_ns};
                $out{sub_1ms} = $out{paired_minimum_ns} < 1_000_000 ? 1 : 0;
                @out{qw(cohort_ratio_spread_pct unstable)} = ('', '');
                if ($other) {
                    my ($lo, $hi) = sort { $a <=> $b } ($group->{ratio}, $other->{ratio});
                    my $spread = $hi / $lo - 1;
                    $out{cohort_ratio_spread_pct} = sprintf '%.4f', 100 * $spread;
                    $out{unstable} = $spread > 0.10 ? 1 : 0;
                }
                push @output, \%out;
            }
        }
    }
    return \@output;
}

sub target_summary {
    my ($summary) = @_;
    my (%cells, @output);
    for my $r (@$summary) {
        $cells{key(@$r{@cell_fields}, $r->{hash_series})}{$r->{cohort}}{$r->{variant}} = $r;
    }
    for my $id (sort keys %cells) {
        my $cell = $cells{$id};
        die "target summary requires both cohorts\n" unless $cell->{0} && $cell->{1};
        my (@wf, @rust, @cpp, @absl) = ();
        @wf = grep /^whitefoot(?:-|$)/, keys %{$cell->{0}};
        @rust = grep /^rust-/, keys %{$cell->{0}};
        @cpp = grep /^cpp-/, keys %{$cell->{0}};
        @absl = grep /^absl-/, keys %{$cell->{0}};
        die "target summary requires one WF, Rust and C++ standard peer\n" unless @wf == 1 && @rust == 1 && @cpp == 1;
        my ($wf, $rust, $cpp) = ($wf[0], $rust[0], $cpp[0]);
        my %out = %{$cell->{0}{$wf}};
        my (@ratios, @upper, @lower, %selected);
        for my $cohort (0, 1) {
            my ($w, $r, $c) = @{$cell->{$cohort}}{$wf, $rust, $cpp};
            my @slower = $r->{median_ns} > $c->{median_ns} ? ($rust)
                : $c->{median_ns} > $r->{median_ns} ? ($cpp) : ($rust, $cpp);
            $selected{$_} = 1 for @slower;
            my @minima = sort { $a <=> $b } map { $cell->{$cohort}{$_}{minimum_ns} } @slower;
            my $denominator = $r->{median_ns} > $c->{median_ns} ? $r->{median_ns} : $c->{median_ns};
            my $largest_native = $r->{maximum_ns} > $c->{maximum_ns} ? $r->{maximum_ns} : $c->{maximum_ns};
            $ratios[$cohort] = $w->{median_ns} / $denominator;
            # Observed sample bounds are sufficient separation, not confidence
            # intervals. A tied standard median uses the less favorable minimum.
            $upper[$cohort] = $w->{maximum_ns} / $minima[0];
            $lower[$cohort] = $w->{minimum_ns} / $largest_native;
            @out{map { "${_}_$cohort" } qw(slower_standard wf_median_ns rust_median_ns cpp_median_ns absl_median_ns)} =
                (join(';', @slower), $w->{median_ns}, $r->{median_ns}, $c->{median_ns}, @absl ? $cell->{$cohort}{$absl[0]}{median_ns} : '');
            @out{map { "${_}_$cohort" } qw(wf_over_slower observed_upper observed_lower)} =
                map { sprintf '%.9f', $_ } ($ratios[$cohort], $upper[$cohort], $lower[$cohort]);
        }
        my @ratio_order = sort { $a <=> $b } @ratios;
        my $spread = $ratio_order[1] / $ratio_order[0] - 1;
        my ($peer_spread, $minimum) = (0, $cell->{0}{$wf}{minimum_ns});
        # A denominator switch cannot conceal a selected peer's instability.
        for my $peer (sort keys %selected) {
            my @paired;
            for my $cohort (0, 1) {
                my ($w, $native) = @{$cell->{$cohort}}{$wf, $peer};
                push @paired, $w->{median_ns} / $native->{median_ns};
                for ($w->{minimum_ns}, $native->{minimum_ns}) { $minimum = $_ if $_ < $minimum; }
            }
            @paired = sort { $a <=> $b } @paired;
            my $variation = $paired[1] / $paired[0] - 1;
            $peer_spread = $variation if $variation > $peer_spread;
        }
        $out{qualification_minimum_ns} = $minimum;
        $out{target_cohort_spread_pct} = sprintf '%.4f', 100 * $spread;
        $out{selected_peer_spread_pct} = sprintf '%.4f', 100 * $peer_spread;
        $out{median_result} = $ratios[0] < 1 && $ratios[1] < 1 ? 'win'
            : $ratios[0] > 1 && $ratios[1] > 1 ? 'deficit' : 'inconclusive';
        my @reasons;
        push @reasons, 'short-selected-peer' if $minimum < 1_000_000;
        push @reasons, 'selected-peer-unstable' if $peer_spread > 0.10;
        push @reasons, 'target-cohort-unstable' if $spread > 0.10;
        $out{status} = @reasons ? 'inconclusive' : $upper[0] < 1 && $upper[1] < 1 ? 'pass'
            : $lower[0] > 1 && $lower[1] > 1 ? 'deficit' : 'inconclusive';
        $out{reason} = @reasons ? join(';', @reasons)
            : $out{status} eq 'inconclusive' ? 'sample-overlap-or-tie' : 'observed-sample-separation';
        if ($out{comparison_class} =~ /\A(?:overhead|storage)-control\z/) {
            @out{qw(status median_result reason)} = ('unranked', 'unranked', $out{comparison_class});
        }
        push @output, \%out;
    }
    return \@output;
}

sub take_flags {
    my ($arguments) = @_;
    my ($complete, $targets) = (0, 0);
    while (@$arguments && $arguments->[0] =~ /^--/) {
        my $flag = shift @$arguments;
        if ($flag eq '--complete') { $complete = 1; }
        elsif ($flag eq '--targets') { $targets = 1; }
        else { die "unknown option: $flag\n"; }
    }
    return ($complete || $targets, $targets);
}

sub self_test {
    my $make_csv = sub {
        my ($family, $rows) = @_;
        my @columns = split /,/, $headers{$family};
        return $headers{$family} . "\n" . join('', map {
            my $row = $_; join(',', map { $row->{$_} // 0 } @columns) . "\n"
        } @$rows);
    };
    my $run = sub {
        my ($family, $rows, $complete) = @_;
        my $csv = $make_csv->($family, $rows);
        open my $fh, '<', \$csv or die "open fixture: $!\n";
        return summarize($complete, [$family, 'self-test', $fh]);
    };
    my $assert = sub { die "self-test failed: $_[1]\n" unless $_[0]; };
    my @rows;
    for my $cohort (0, 1) {
        for my $variant (qw(whitefoot rust-vec)) {
            for my $sample (0 .. 2) {
                push @rows, { contract => 'normal-o3', cohort => $cohort, element_bytes => 8,
                    path => 'reserved', count => 16, variant => $variant, sample => $sample,
                    work => 48, rounds => 3, traces => 1,
                    elapsed_ns => (2_000_000, 6_000_000, 4_000_000)[$sample] / ($variant eq 'whitefoot' ? 1 : 2),
                    checksum => ('9007199254740993', '18446744073709551615', '9007199254740995')[$sample] };
            }
        }
    }
    my $result = $run->('vector', \@rows);
    my ($native) = grep { $_->{cohort} == 0 && $_->{variant} eq 'rust-vec' } @$result;
    $assert->($native->{median_ns} == 2_000_000 && $native->{minimum_ns} == 1_000_000
        && $native->{maximum_ns} == 3_000_000 && $native->{wf_over_variant} == 2
        && $native->{samples} == 3 && $native->{unstable} == 0, 'known odd median and ratio');
    $assert->(median(9, 2, 4, 1) == 3 && median(1, 2) == 1.5, 'even medians');
    my @overhead = map { +{%$_, path => 'suffix-0'} } @rows;
    push @overhead, map { +{%$_, variant => 'direct-c'} } grep { $_->{variant} eq 'rust-vec' } @overhead;
    $result = $run->('vector', \@overhead);
    ($native) = grep { $_->{variant} eq 'rust-vec' } @$result;
    $assert->($native->{wf_over_variant} == 2 && !grep({ $_->{comparison_class} ne 'overhead-control' } @$result), 'zero-removal overhead controls retain ratios without ecosystem classification');
    my $fails = sub {
        my ($label, $pattern, $mutate) = @_;
        my @bad = map { +{%$_} } @rows;
        $mutate->(\@bad);
        my $ok = eval { $run->('vector', \@bad); 1 };
        $assert->(!$ok && $@ =~ $pattern, $label);
    };
    $fails->('adjacent checksum above 2^53', qr/checksum mismatch/, sub { $_[0][3]{checksum} = '9007199254740992' });
    $fails->('cross-cohort checksum', qr/checksum mismatch/, sub { $_[0][6]{checksum} = '9007199254740992' });
    $fails->('missing row', qr/missing sample/, sub { pop @{$_[0]} });
    $fails->('duplicate row', qr/duplicate sample/, sub { push @{$_[0]}, {%{$_[0][0]}} });
    $fails->('mismatched rounds', qr/mismatched rounds/, sub { $_[0][3]{rounds} = 4 });
    $fails->('mismatched traces', qr/mismatched rounds/, sub { $_[0][3]{traces} = 2 });
    $fails->('mismatched work', qr/mismatched rounds/, sub { $_[0][3]{work} = 49 });
    $fails->('zero time', qr/nonpositive timing/, sub { $_[0][3]{elapsed_ns} = 0 });
    $fails->('missing sample ID everywhere', qr/missing sample ID/, sub { @{$_[0]} = grep { $_->{sample} != 1 } @{$_[0]} });
    my @unstable = map { +{%$_, elapsed_ns => $_->{cohort} == 1 && $_->{variant} eq 'rust-vec'
        ? $_->{elapsed_ns} / 2 : $_->{elapsed_ns}} } @rows;
    $result = $run->('deque', \@unstable);
    ($native) = grep { $_->{variant} eq 'rust-vec' && $_->{cohort} == 1 } @$result;
    $assert->($native->{unstable} == 1 && $native->{cohort_ratio_spread_pct} == 100
        && $native->{sub_1ms} == 1 && $native->{paired_minimum_ns} == 500_000, 'cohort discrepancy and short samples');
    my @map_rows;
    for my $series (qw(native-default aligned-hash)) {
        push @map_rows, map { +{%$_, series => $series, requested_capacity => 64,
            hash => $series eq 'aligned-hash' ? 'aligned' : 'default',
            variant => $_->{variant} eq 'whitefoot' ? 'whitefoot-hash-map' : 'rust-hash-map'} } @rows;
    }
    $result = $run->('map', \@map_rows);
    $assert->(@$result == 8 && !grep({ $_->{wf_over_variant} != ($_->{comparison_class} eq 'reference' ? 1 : 2) } @$result), 'hash series stay separate');
    my @priority = map { +{%$_, variant => $_->{variant} eq 'rust-vec' ? 'hole-c' : 'whitefoot',
        series => $_->{variant} eq 'rust-vec' ? 'c-control' : 'practical',
        seed => 101 + $_->{sample}, work_multiplier => 1} } @rows;
    $result = $run->('priority', \@priority);
    my ($control) = grep { $_->{variant} eq 'hole-c' } @$result;
    $assert->($control->{wf_over_variant} == 2 && $control->{comparison_class} eq 'c-control', 'priority classification is not a grouping axis');
    $_->{path} = 'setup-cleanup', $_->{series} = 'storage-control' for @priority;
    $result = $run->('priority', \@priority);
    $assert->(!grep({ $_->{comparison_class} ne 'storage-control' } @$result), 'storage controls are never ecosystem peers');
    my @ordered = map { +{%$_, pair_bytes => 16, elapsed_ns => $_->{sample} == 0 ? 999_000_000 : $_->{elapsed_ns}} } @rows;
    $result = $run->('ordered', \@ordered);
    ($native) = grep { $_->{variant} eq 'rust-vec' } @$result;
    $assert->($native->{median_ns} == 2_500_000 && $native->{samples} == 2
        && $native->{warmup_samples} == 1 && $native->{sample_ids} eq '1;2', 'ordered recorded warmup excluded');
    my @single = grep { $_->{cohort} == 0 } @rows;
    $result = $run->('vector', \@single);
    $assert->($result->[0]{unstable} eq '', 'single cohort stability unverified');
    # Independent literal fixtures, not generated from the validator inventory.
    my @full;
    for my $payload (8, 256) { for my $count (16, 256, 4096) {
        for my $path (qw(reserved growth reuse suffix-0 suffix-1 suffix-2 suffix-3)) {
            for my $cohort (0, 1) { for my $variant (qw(whitefoot reverse-c direct-c swap-take-c take-swap-c rust-vec cpp-std-vector)) {
                push @full, {%{$rows[0]}, element_bytes => $payload, count => $count, path => $path, cohort => $cohort, variant => $variant, sample => $_} for 0 .. 6;
            }}
        }
    }}
    $assert->(@{$run->('vector', \@full, 1)} == 588, 'complete vector fixture');
    my @extended = (@full, map { +{%$_, sample => 7} } grep { $_->{sample} == 0 } @full);
    $assert->($run->('vector', \@extended, 1)->[0]{samples} == 8, 'complete sequence accepts additional samples');
    for my $deletion (
        ['complete vector missing cell: 8/reserved/16/', sub { $_->{element_bytes} == 8 && $_->{path} eq 'reserved' && $_->{count} == 16 }],
        ['complete vector variant coverage mismatch', sub { $_->{variant} eq 'rust-vec' }],
        ['complete vector cohort coverage mismatch', sub { $_->{cohort} == 1 }],
        ['complete vector requires at least 7 samples', sub { $_->{sample} > 0 }],
    ) {
        my @bad = grep { !$deletion->[1]->() } @full;
        my $ok = eval { $run->('vector', \@bad, 1); 1 };
        $assert->(!$ok && $@ eq "$deletion->[0]\n", $deletion->[0]);
    }
    @full = ();
    for my $payload (8, 256) { for my $shape ([2, 3], [56, 64], [3584, 4096]) {
        for my $path (qw(hit miss replace-old-value remove-churn edit-first-word fill-free reserve-more-entries)) {
            for my $cohort (0, 1) { for my $variant (qw(whitefoot-hash-map rust-hash-map cpp-unordered-map absl-flat-hash-map c-sparse-direct)) {
                for my $series (qw(native-default aligned-hash)) { for my $sample (0 .. 10) {
                    push @full, {%{$rows[0]}, contract => 'normal', element_bytes => $payload, count => $shape->[0], requested_capacity => $shape->[1], path => $path,
                        cohort => $cohort, variant => $variant, series => $series, hash => 'salted-mix64', sample => $sample};
                }}
            }}
        }
    }}
    $assert->(@{$run->('map', \@full, 1)} == 840, 'complete map fixture');
    my @short = grep { $_->{sample} != 10 } @full;
    my $ok = eval { $run->('map', \@short, 1); 1 };
    $assert->(!$ok && $@ eq "complete map sample coverage mismatch\n", 'missing fixed final sample everywhere');
    my @target_rows;
    my %timings = ('whitefoot-hash-map' => 8_000_000, 'rust-hash-map' => 4_000_000,
        'cpp-unordered-map' => 10_000_000, 'absl-flat-hash-map' => 100_000_000);
    for my $cohort (0, 1) { for my $sample (0 .. 2) { for my $variant (sort keys %timings) {
        push @target_rows, {%{$rows[0]}, contract => 'normal', path => 'hit', count => 56, requested_capacity => 64,
            series => 'native-default', hash => 'default', cohort => $cohort, sample => $sample, variant => $variant, elapsed_ns => $timings{$variant}};
    }}}
    my $target_case = sub {
        my ($mutate) = @_; my @input = map { +{%$_} } @target_rows;
        $mutate->(\@input) if $mutate;
        return target_summary($run->('map', \@input))->[0];
    };
    my $target = $target_case->();
    $assert->($target->{status} eq 'pass' && $target->{slower_standard_0} eq 'cpp-unordered-map'
        && $target->{wf_over_slower_0} == 0.8 && $target->{absl_median_ns_0} == 100_000_000, 'target selects slower standard and retains Abseil reference');
    for my $case ([12_000_000, 'deficit'], [10_000_000, 'inconclusive']) {
        $target = $target_case->(sub { $_->{elapsed_ns} = $case->[0] for grep { $_->{variant} =~ /^whitefoot/ } @{$_[0]} });
        $assert->($target->{status} eq $case->[1], 'Abseil cannot rescue deficit and ties are not wins or deficits');
    }
    $target = $target_case->(sub { $_->{elapsed_ns} = 11_000_000 for grep { $_->{variant} =~ /^whitefoot/ && $_->{sample} == 2 } @{$_[0]} });
    $assert->($target->{status} eq 'inconclusive' && $target->{median_result} eq 'win', 'overlap preserves descriptive median win');
    $target = $target_case->(sub { for (@{$_[0]}) { next unless $_->{variant} =~ /^(?:rust|cpp)-/;
        $_->{elapsed_ns} = (($_->{variant} =~ /^rust/) == $_->{cohort}) ? 10_100_000 : 10_000_000; } });
    $assert->($target->{status} eq 'pass' && $target->{slower_standard_0} eq 'cpp-unordered-map'
        && $target->{slower_standard_1} eq 'rust-hash-map', 'stable slower-peer switch');
    $target = $target_case->(sub { for (@{$_[0]}) { next unless $_->{variant} =~ /^(?:rust|cpp)-/;
        $_->{elapsed_ns} = (($_->{variant} =~ /^rust/) == $_->{cohort}) ? 10_000_000 : 4_000_000; } });
    $assert->($target->{status} eq 'inconclusive' && $target->{target_cohort_spread_pct} == 0
        && $target->{reason} eq 'selected-peer-unstable', 'max selection cannot conceal selected-peer instability');
    push @overhead, map { +{%$_, variant => 'cpp-std-vector'} } grep { $_->{variant} eq 'rust-vec' } @overhead;
    $assert->(target_summary($run->('vector', \@overhead))->[0]{status} eq 'unranked', 'target preserves overhead controls');
    for my $variant (qw(rust-binary-heap cpp-std-heap)) {
        push @priority, map { +{%$_, variant => $variant} } grep { $_->{variant} eq 'hole-c' } @priority;
    }
    $assert->(target_summary($run->('priority', \@priority))->[0]{status} eq 'unranked', 'target preserves storage controls');
    my @flags = ('--targets', 'map=fixture'); my ($complete, $targets) = take_flags(\@flags);
    $ok = eval { target_summary($run->('map', \@target_rows, $complete)); 1 };
    $assert->($complete && $targets && !$ok && $@ eq "complete map variant coverage mismatch\n", 'target mode requires complete matrices');
    print "ecosystem summarizer self-test passed\n";
}

if (@ARGV == 1 && $ARGV[0] eq '--self-test') {
    self_test();
    exit 0;
}
my ($complete, $targets) = take_flags(\@ARGV);
die "usage: $0 [--complete] [--targets] family=timing.csv ... | --self-test\n" unless @ARGV;
my @sources;
for my $argument (@ARGV) {
    my ($family, $path) = split /=/, $argument, 2;
    die "expected family=timing.csv: $argument\n" unless defined $path && exists $headers{$family};
    open my $fh, '<', $path or die "$path: $!\n";
    push @sources, [$family, $path, $fh];
}
my $result = summarize($complete, @sources);
close $_->[2] or die "close $_->[1]: $!\n" for @sources;
if ($targets) {
    $result = target_summary($result);
    my %totals;
    for my $row (@$result) {
        ++$totals{$_}{$row->{status}} for ($row->{family}, 'all');
    }
    for my $family (sort keys %totals) {
        my $t = $totals{$family}; $t->{$_} //= 0 for qw(pass deficit inconclusive unranked);
        $t->{eligible} = $t->{pass} + $t->{deficit} + $t->{inconclusive};
        print STDERR "target totals: $family ", join(' ', map { "$_=$t->{$_}" } qw(eligible pass deficit inconclusive unranked)), "\n";
    }
}
my @fields = $targets ? @target_fields : @output_fields;
print join(',', @fields), "\n";
print join(',', @{$_}{@fields}), "\n" for @$result;
