#!/bin/sh
# Feed .github/gate-scope.sh the events gate.yml and io-hosts.yml receive,
# with a stand-in `gh` that answers from fixtures, and check each decision.
# A stand-in that fails marks the events that must decide without a query. The
# stand-in reads its arguments as gh does: only --paginate returns every page
# of a list, and fields given with -f reach a list only under --method GET, so
# a script that drops either gets the wrong answer. Every call's arguments are
# logged one per line, to check that a branch name travels as one field.
set -eu
test_matrix() {
    case "$0" in */*) workflow=${0%/*}/workflows/gate.yml ;; *) workflow=workflows/gate.yml ;; esac
    perl - "$workflow" <<'MATRIX'
use strict;
use warnings;
use JSON::PP qw(decode_json encode_json);
open my $file, '<', $ARGV[0] or die $!;
my $workflow = do { local $/; <$file> };
my ($matrix) = $workflow =~ /matrix=\$\(jq [^\n]*\n\s*'([^']+)'\)/;
my ($qualify) = $workflow =~ /groups=\$\(jq -er '([^']+)' <<< "\$GATE_GROUPS"\)/;
defined $matrix && defined $qualify or die "gate matrix: missing workflow jq expressions\n";
sub jq {
    open my $output, '-|', 'jq', @_ or die $!;
    my $text = do { local $/; <$output> };
    close $output or die "gate matrix: jq failed with status $?\n";
    return $text;
}
sub names_equal {
    my ($actual, $expected, $name) = @_;
    join("\n", sort @$actual) eq join("\n", sort @$expected)
        or die "gate matrix: $name has missing or extra jobs\n";
}
my @ordinary = (
    {group => 'ordinary', shard => '1/3', name => 'ordinary 1/3'},
    {group => 'stable', shard => '1/1', name => 'stable'},
);
my @expected = (
    'ordinary 1/3 (ubuntu-24.04)', 'ordinary 1/3 (macos-15)',
    'stable (ubuntu-24.04)', 'stable (macos-15)',
);
for my $main (0, 1) {
    my @groups = @ordinary;
    my @required = @expected;
    if ($main) {
        push @groups, {group => 'whole', shard => '1/1',
            name => 'proof receipts, all cases', linux_only => JSON::PP::true};
        push @required, 'proof receipts, all cases (ubuntu-24.04)';
    }
    my $jobs = decode_json(jq('-cn', '--argjson', 'groups', encode_json(\@groups),
        '--argjson', 'platforms', '["ubuntu-24.04","macos-15"]', $matrix));
    names_equal([map { "$_->{name} ($_->{os})" } @$jobs], \@required, "main=$main matrix");
    my %groups = map { $_->{group} => $_ } @groups;
    for my $job (@$jobs) {
        my $source = $groups{$job->{group}} or die "gate matrix: unknown command group\n";
        $job->{shard} eq $source->{shard} && $job->{name} eq $source->{name}
            or die "gate matrix: matrix changed the Make-owned group or shard\n";
    }
    my $required = jq('-nr', '--argjson', 'inventory', encode_json(\@groups),
        '$inventory | ' . $qualify);
    names_equal([grep { $_ ne '' } split /\n/, $required], \@required, "main=$main qualification");
}
my $draft = decode_json(jq('-cn', '--argjson', 'groups', encode_json(\@ordinary),
    '--argjson', 'platforms', '["ubuntu-24.04"]', $matrix));
names_equal([map { "$_->{name} ($_->{os})" } @$draft],
    ['ordinary 1/3 (ubuntu-24.04)', 'stable (ubuntu-24.04)'], 'draft Linux-only matrix');
print "gate matrix: regular platforms, main-only Linux group and qualification agree\n";
MATRIX
}
test_matrix
if test "${1:-}" = --matrix-only; then exit 0; fi
here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
script=$here/gate-scope.sh
work=$(mktemp -d "${TMPDIR:-/tmp}/whitefoot-gate-scope.XXXXXX")
trap 'rm -rf "$work"' EXIT

mkdir "$work/bin"
cat > "$work/bin/gh" <<'EOF'
#!/bin/sh
echo "$*" >> "$FAKE_GH_LOG"
{ for arg in "$@"; do printf '%s\n' "$arg"; done; echo '--'; } >> "$FAKE_GH_ARGV"
if [ -n "$FAKE_GH_FAIL" ]; then
    echo "gh: stand-in refused: $*" >&2
    exit 1
fi
paginate=
method=
path=
previous=
for arg in "$@"; do
    [ "$previous" != --method ] || method=$arg
    case "$arg" in
        --paginate) paginate=1 ;;
        repos/*) path=$arg ;;
    esac
    previous=$arg
done
case "$path" in
    */actions/runs/*) printf '%s\n' "$FAKE_RUN_CREATED" ;;
    */pulls/[0-9]*) printf '%s\n' "$FAKE_TIP" ;;
    */pulls)
        if [ "$method" != GET ]; then
            echo "gh: fields without --method GET are sent as a body: $*" >&2
            exit 2
        fi
        if [ -n "$paginate" ]; then
            printf '%s\n' "$FAKE_PULLS"
        else
            printf '%s' "$FAKE_PULLS" | jq -c -s '.[0]'
        fi
        ;;
    *) echo "gh: unexpected query: $*" >&2; exit 2 ;;
esac
EOF
chmod +x "$work/bin/gh"
FAKE_GH_LOG=$work/gh.log
FAKE_GH_ARGV=$work/gh.argv
export FAKE_GH_LOG FAKE_GH_ARGV

# The run under test was created at 10:00:05; pull requests are made around it.
run_created=2026-10-12T10:00:05Z
early_draft='{"number":11,"draft":true,"created_at":"2026-10-12T09:00:00Z"}'
early_ready='{"number":12,"draft":false,"created_at":"2026-10-12T09:00:00Z"}'
same_draft='{"number":13,"draft":true,"created_at":"2026-10-12T10:00:05Z"}'
late_draft='{"number":14,"draft":true,"created_at":"2026-10-12T10:00:06Z"}'
late_ready='{"number":15,"draft":false,"created_at":"2026-10-12T10:00:06Z"}'
run_time=$run_created

# A pull request's head, and the tip the stand-in reports for its branch.
head_a=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
head_b=bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
tip=$head_a

# scope EVENT REF EVENT-JSON PULLS [refuse-queries]
scope() {
    printf '%s\n' "$3" > "$work/event.json"
    : > "$FAKE_GH_LOG"
    : > "$FAKE_GH_ARGV"
    FAKE_PULLS=$4 FAKE_RUN_CREATED=$run_time FAKE_TIP=$tip FAKE_GH_FAIL=${5:-} \
        GITHUB_EVENT_NAME=$1 GITHUB_REF=$2 GITHUB_REF_NAME=${2#refs/*/} \
        GITHUB_EVENT_PATH=$work/event.json GITHUB_REPOSITORY=org/repo \
        GITHUB_REPOSITORY_OWNER=org GITHUB_RUN_ID=7 PATH="$work/bin:$PATH" \
        sh "$script" > "$work/out" 2> "$work/err"
}

fail() {
    {
        echo "gate-scope: $1"
        echo '--- decision:'; cat "$work/out"
        echo '--- stderr:'; cat "$work/err"
        echo '--- gh queries:'; cat "$FAKE_GH_LOG"
    } >&2
    exit 1
}

# expect NAME RUN BOTH [REASON-FRAGMENT] -- after a scope call that succeeded
expect() {
    grep -qx "run=$2" "$work/out" || fail "$1: expected run=$2"
    grep -qx "both=$3" "$work/out" || fail "$1: expected both=$3"
    [ -z "${4:-}" ] || grep -q "^reason=.*$4" "$work/out" || fail "$1: expected a reason naming '$4'"
}

decides() {
    name=$1
    shift
    scope "$@" || fail "$name: the decision failed"
}

refuses() {
    name=$1
    shift
    if scope "$@"; then
        fail "$name: expected the decision to fail"
    fi
}

dispatch='{}'
# pr_event ACTION DRAFT [HEAD-REPOSITORY]: a pull request whose head is head_a.
pr_event() {
    printf '{"action":"%s","pull_request":{"number":5,"draft":%s,"head":{"sha":"%s","ref":"claude/topic","repo":{"full_name":"%s"}}}}' \
        "$1" "$2" "$head_a" "${3:-org/repo}"
}
opened_draft=$(pr_event opened true)
opened_ready=$(pr_event opened false)
reopened_draft=$(pr_event reopened true)
reopened_ready=$(pr_event reopened false)
opened_from_fork=$(pr_event opened false fork/repo)
ready='{"action":"ready_for_review","pull_request":{"number":5,"draft":false}}'
synchronize='{"action":"synchronize","pull_request":{"number":5,"draft":true}}'

# Main, tags, manual runs, the event of marking ready and a fork's opening
# decide without a query.
decides 'push to main' push refs/heads/main '{}' '[]' refuse
expect 'push to main' true true 'push to main'
decides 'push of a tag' push refs/tags/v1 '{}' '[]' refuse
expect 'push of a tag' false false 'not a branch'
decides 'manual run' workflow_dispatch refs/heads/feature/x "$dispatch" '[]' refuse
expect 'manual run' true true 'manual run'
decides 'marked ready' pull_request refs/pull/5/merge "$ready" '[]' refuse
expect 'marked ready' true true 'marked ready'
tip=$head_b
decides 'ready opened from a fork' pull_request refs/pull/5/merge "$opened_from_fork" '[]' refuse
expect 'ready opened from a fork' true true 'opened, ready'
tip=$head_a

# The opening of a pull request of this repository asks for the pull
# request's current head and runs while it is the head the run was for; a
# reopening runs without asking, as a push while it was closed was skipped.
decides 'draft opened' pull_request refs/pull/5/merge "$opened_draft" '[]'
expect 'draft opened' true false 'opened as a draft'
grep -q 'repos/org/repo/pulls/5' "$FAKE_GH_LOG" || fail 'draft opened: the pull request was not queried'
decides 'ready opened' pull_request refs/pull/5/merge "$opened_ready" '[]'
expect 'ready opened' true true 'opened, ready'
decides 'draft reopened' pull_request refs/pull/5/merge "$reopened_draft" '[]' refuse
expect 'draft reopened' true false 'reopened as a draft'
decides 'ready reopened' pull_request refs/pull/5/merge "$reopened_ready" '[]' refuse
expect 'ready reopened' true true 'reopened, ready'
tip=$head_b
decides 'ready opened, branch moved' pull_request refs/pull/5/merge "$opened_ready" '[]'
expect 'ready opened, branch moved' false false 'opened at aaaaaaaaaaaa, its branch has moved to bbbbbbbbbbbb, whose push run checks it'
decides 'draft reopened, branch moved' pull_request refs/pull/5/merge "$reopened_draft" '[]' refuse
expect 'draft reopened, branch moved' true false 'reopened as a draft'
tip=null
refuses 'unreadable head' pull_request refs/pull/5/merge "$opened_ready" '[]'
grep -q 'unexpected head of pull request 5' "$work/err" || fail 'unreadable head: wrong refusal'
tip=$head_a
refuses 'failed head query' pull_request refs/pull/5/merge "$opened_ready" '[]' refuse
refuses 'unhandled pull request action' pull_request refs/pull/5/merge "$synchronize" '[]' refuse
grep -q 'unsupported pull request action: synchronize' "$work/err" || fail 'unhandled pull request action: wrong refusal'
refuses 'unhandled event' schedule refs/heads/main '{}' '[]' refuse
grep -q 'unsupported event: schedule' "$work/err" || fail 'unhandled event: wrong refusal'

# A push to any other branch asks GitHub for the branch's open pull requests.
decides 'no pull request' push refs/heads/feature/topic '{}' '[]'
expect 'no pull request' false false 'has no open pull request'
grep -q 'repos/org/repo/actions/runs/7' "$FAKE_GH_LOG" || fail 'no pull request: the run was not queried'
for line in GET --paginate repos/org/repo/pulls state=open per_page=100 head=org:feature/topic; do
    grep -qxF -- "$line" "$FAKE_GH_ARGV" || fail "no pull request: the branch query lacks '$line'"
done
decides 'earlier draft' push refs/heads/claude/topic '{}' "[$early_draft]"
expect 'earlier draft' true false 'draft pull request 11'
decides 'earlier ready' push refs/heads/claude/topic '{}' "[$early_ready]"
expect 'earlier ready' true true 'ready pull request 12'
decides 'draft and ready' push refs/heads/claude/topic '{}' "[$early_draft,$early_ready]"
expect 'draft and ready' true true 'ready pull request 12'
decides 'same second' push refs/heads/claude/topic '{}' "[$same_draft]"
expect 'same second' true false 'draft pull request 13'
decides 'opened after the push' push refs/heads/claude/topic '{}' "[$late_ready]"
expect 'opened after the push' false false 'pull request 15 was opened'
decides 'earlier draft, later ready' push refs/heads/claude/topic '{}' "[$early_draft,$late_ready]"
expect 'earlier draft, later ready' true false 'draft pull request 11'
decides 'second page' push refs/heads/claude/topic '{}' "[$late_draft][$early_ready]"
expect 'second page' true true 'ready pull request 12'

# A branch name made of query separators still travels as the one field.
odd='claude/a&b#c+d%e'
decides 'branch name with separators' push "refs/heads/$odd" '{}' "[$early_ready]"
expect 'branch name with separators' true true 'ready pull request 12'
grep -qxF -- "head=org:$odd" "$FAKE_GH_ARGV" || fail 'branch name with separators: not sent as one field'
if grep -q 'pulls?' "$FAKE_GH_ARGV"; then
    fail 'branch name with separators: the query was written into the URL'
fi

refuses 'failed query' push refs/heads/claude/topic '{}' "[$early_ready]" refuse
run_time=null
refuses 'missing creation time' push refs/heads/claude/topic '{}' "[$early_ready]"
grep -q 'unexpected creation time' "$work/err" || fail 'missing creation time: wrong refusal'
run_time=$run_created

echo "gate-scope: every event decided as expected"
