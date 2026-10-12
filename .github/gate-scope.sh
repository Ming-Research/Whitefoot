#!/bin/sh
# Decide whether a run of gate.yml or io-hosts.yml checks its revision, and
# whether the gate adds macOS. Prints `run=`, `both=` and `reason=` lines for
# $GITHUB_OUTPUT; both workflows call it, so they make one decision.
#
# A run checks what someone may merge: a push to main, a push to a branch
# with an open pull request, the opening or reopening of a pull request, the
# event of marking one ready, and a manual run. Any other push, to a branch
# with no open pull request or to a tag, is skipped. A pull request opened
# after a push starts its own run on its head, so the push run counts only
# pull requests created no later than itself: a branch pushed and given a pull
# request moments apart is then checked once, by the opening's run, rather
# than twice. The comparison errs only toward checking twice: a run is created
# after its push, so a pull request it treats as earlier really is.
#
# The run of an opening or reopening checks the head it was raised for only
# while the pull request still has that head. It is created some seconds after
# the pull request, and a push right after the opening has a run of its own
# that counts the pull request as earlier and checks the new head, so the
# opening's run decides run=false once the pull request's head has moved. That
# run has a concurrency group apart from the pushes' (gate.yml, io-hosts.yml),
# so it never cancels a push run. A pull request from a fork is not asked: no
# push to the fork starts a run here, so its opening always runs.
#
# It reads GITHUB_EVENT_NAME, GITHUB_EVENT_PATH, GITHUB_REF, GITHUB_REF_NAME,
# GITHUB_REPOSITORY, GITHUB_REPOSITORY_OWNER and GITHUB_RUN_ID, as Actions
# sets them, and queries GitHub with `gh` (GH_TOKEN) for a push to a branch
# other than main and for the opening of a pull request from this repository.
# A failed query fails the step rather than quietly skipping the gate or
# dropping macOS. .github/test-gate-scope.sh exercises it.
set -eu

event() {
    jq -r "$1" "$GITHUB_EVENT_PATH"
}

decide() {
    printf 'run=%s\nboth=%s\nreason=%s\n' "$1" "$2" "$3"
}

case "$GITHUB_EVENT_NAME" in
    workflow_dispatch)
        decide true true 'manual run'
        ;;
    pull_request)
        action=$(event .action)
        number=$(event .pull_request.number)
        draft=$(event .pull_request.draft)
        case "$action" in
            ready_for_review)
                decide true true "pull request $number marked ready"
                ;;
            opened | reopened)
                head_sha=$(event .pull_request.head.sha)
                head_repo=$(event .pull_request.head.repo.full_name)
                tip=$head_sha
                if [ "$head_repo" = "$GITHUB_REPOSITORY" ]; then
                    tip=$(gh api "repos/$GITHUB_REPOSITORY/pulls/$number" --jq .head.sha)
                    case "$tip" in
                        '' | *[!0-9a-f]*)
                            echo "unexpected head of pull request $number: $tip" >&2
                            exit 1
                            ;;
                    esac
                fi
                if [ "$tip" != "$head_sha" ]; then
                    decide false false "pull request $number $action at $(printf '%.12s' "$head_sha"), its branch has moved to $(printf '%.12s' "$tip"), whose push run checks it"
                elif [ "$draft" = false ]; then
                    decide true true "pull request $number $action, ready"
                else
                    decide true false "pull request $number $action as a draft"
                fi
                ;;
            *)
                echo "unsupported pull request action: $action" >&2
                exit 1
                ;;
        esac
        ;;
    push)
        if [ "$GITHUB_REF" = refs/heads/main ]; then
            decide true true 'push to main'
        elif [ "${GITHUB_REF#refs/heads/}" = "$GITHUB_REF" ]; then
            decide false false "push to $GITHUB_REF, which is not a branch"
        else
            created=$(gh api "repos/$GITHUB_REPOSITORY/actions/runs/$GITHUB_RUN_ID" --jq .created_at)
            case "$created" in
                [0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z) ;;
                *)
                    echo "unexpected creation time of run $GITHUB_RUN_ID: $created" >&2
                    exit 1
                    ;;
            esac
            # With --method GET, gh sends each -f field as an encoded query
            # parameter, so a branch name with `&`, `#`, `+` or `%` reaches
            # GitHub as written. --paginate prints one array per page; -s and
            # add join them.
            pulls=$(gh api --method GET --paginate "repos/$GITHUB_REPOSITORY/pulls" \
                -f state=open -f per_page=100 -f head="$GITHUB_REPOSITORY_OWNER:$GITHUB_REF_NAME")
            # GitHub writes both times as UTC seconds ending in Z, so string
            # order is time order.
            pulls_where() {
                printf '%s' "$pulls" | jq -r -s --arg created "$created" \
                    "[add // [] | .[] | select($1) | .number | tostring] | join(\" \")"
            }
            ready=$(pulls_where '.created_at <= $created and .draft == false')
            drafts=$(pulls_where '.created_at <= $created and .draft != false')
            later=$(pulls_where '.created_at > $created')
            if [ -n "$ready" ]; then
                decide true true "push to ready pull request $ready"
            elif [ -n "$drafts" ]; then
                decide true false "push to draft pull request $drafts"
            elif [ -n "$later" ]; then
                decide false false "push before pull request $later was opened; the run of its opening checks the head"
            else
                decide false false "push to $GITHUB_REF_NAME, which has no open pull request"
            fi
        fi
        ;;
    *)
        echo "unsupported event: $GITHUB_EVENT_NAME" >&2
        exit 1
        ;;
esac
