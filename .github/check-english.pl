#!/usr/bin/perl
# Repository artifacts are in English (AGENTS.md). This rejects Han
# characters, the script of Chinese writing, in the names and the text of
# tracked files outside archive/, which is frozen and keeps its historical
# text. Text is what `git grep -I` reads, so a file git treats as binary is
# skipped. A line that is not valid UTF-8 is read as bytes and never matches.
# The match is the Script property, `\p{Script=Han}`: the short `\p{Han}`
# means Script_Extensions, which in older Unicode versions, such as macOS's
# perl, also covers shared punctuation like the middle dot U+00B7.
# `--self-test` builds a throwaway repository and requires the scan to reject
# a Han file name, a Han line and a Han line after an invalid one, and to
# accept English text with a middle dot, a binary file holding Han bytes and
# Han under archive/.
use strict;
use warnings;
use File::Temp qw(tempdir);

sub usage {
    print STDERR "usage: check-english.pl [--self-test]\n";
    exit 2;
}

# Paths a git command prints NUL-separated; git grep exits 1 on no match.
sub git_paths {
    my @arguments = @_;
    open(my $pipe, '-|', 'git', @arguments) or die "check-english: git: $!\n";
    my @paths = do { local $/ = "\0"; my @lines = <$pipe>; chomp(@lines); @lines };
    close($pipe) or $? >> 8 == 1 or die "check-english: git @arguments failed\n";
    return @paths;
}

# The first line of the file holding a Han character, or 0.
sub first_han_line {
    my ($path) = @_;
    open(my $file, '<:raw', $path) or die "check-english: $path: $!\n";
    my $bytes = do { local $/; <$file> };
    close($file);
    return 0 unless defined $bytes;
    my $text = $bytes;
    if (utf8::decode($text)) {
        return 0 unless $text =~ /\p{Script=Han}/;
        return 1 + (substr($text, 0, $-[0]) =~ tr/\n//);
    }
    my $number = 0;
    for my $line (split /\n/, $bytes, -1) {
        $number++;
        utf8::decode($line);
        return $number if $line =~ /\p{Script=Han}/;
    }
    return 0;
}

sub scan {
    my @outside_archive = ('--', '.', ':(exclude)archive');
    my @findings;
    my @names = git_paths('ls-files', '-z', @outside_archive);
    die "check-english: git ls-files listed no files\n" unless @names;
    for my $name (@names) {
        my $decoded = $name;
        utf8::decode($decoded);
        push @findings, "$name: file name" if $decoded =~ /\p{Script=Han}/;
    }
    my @texts = git_paths('grep', '-I', '-l', '-z', '-e', '^', @outside_archive);
    die "check-english: git grep listed no text files\n" unless @texts;
    for my $path (@texts) {
        my $line = first_han_line($path);
        push @findings, "$path:$line" if $line;
    }
    return @findings;
}

sub self_test {
    my $han = "\xe4\xb8\xad";
    my $work = tempdir('whitefoot-english-test.XXXXXX', TMPDIR => 1, CLEANUP => 1);
    my %files = (
        'english.md'          => "plain English \xc2\xb7 with a middle dot\n",
        'binary.bin'          => "\0$han\n",
        "archive/$han.md"     => "$han\n",
        'notes.md'            => "first\n$han\n",
        'invalid.md'          => "\xff\n$han\n",
        "$han.md"             => "English under a Han name\n",
    );
    delete @ENV{qw(GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE)};
    chdir($work) or die "check-english: $work: $!\n";
    system('git', 'init', '-q', '-b', 'main') == 0
        or die "check-english: git init failed\n";
    mkdir('archive') or die "check-english: archive: $!\n";
    for my $path (sort keys %files) {
        open(my $file, '>:raw', $path) or die "check-english: $path: $!\n";
        print $file $files{$path};
        close($file) or die "check-english: $path: $!\n";
    }
    system('git', 'add', '--', sort keys %files) == 0
        or die "check-english: git add failed\n";
    my $found = join("\n", sort(scan()));
    my $expected = join("\n", sort("$han.md: file name", 'invalid.md:2', 'notes.md:2'));
    chdir('/') or die "check-english: /: $!\n";
    return if $found eq $expected;
    print STDERR "check-english: self-test found\n$found\nexpected\n$expected\n";
    exit 1;
}

usage() if @ARGV > 1 or (@ARGV == 1 and $ARGV[0] ne '--self-test');
if (@ARGV) {
    self_test();
    exit 0;
}
my @findings = scan();
exit 0 unless @findings;
print STDERR "english: tracked files outside archive/ contain Han characters; "
    . "repository artifacts are in English (AGENTS.md):\n";
print STDERR "$_\n" for @findings;
exit 1;
