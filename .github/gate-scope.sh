#!/bin/sh
# Decide whether a run of gate.yml, io-hosts.yml or sanitizers.yml checks its
# revision. Prints `run=` and `reason=` lines for $GITHUB_OUTPUT; every one of
# them calls it, so they make one decision.
#
# A run checks what is about to merge: a push to main, a push to a branch with
# an open ready pull request, the opening or reopening of a ready pull
# request, the event of marking one ready, and a manual run. A draft pull
# request is still being written, and its author gets results from a temporary
# workflow (AGENTS.md, "Checks"), then starts the full check by hand once the
# review has passed. So a push to a draft pull request's branch and the opening
# or reopening of a draft skip, as does any other push, to a branch with no
# open pull request or to a tag. The event of marking a pull request ready
# skips when its head already has a successful manual run of the same
# workflow, which checked the same revision a moment ago, and runs otherwise.
# A manual run never skips, so a successful one always checked everything; the
# runs of other events do not count, because a run that skipped also succeeds.
#
# A pull request opened after a push starts its own run on its head, so the
# push run counts only pull requests created no later than itself: a branch
# pushed and given a ready pull request moments apart is then checked once, by
# the opening's run, rather than twice. The comparison errs only toward
# checking twice: a run is created after its push, so a pull request it treats
# as earlier really is.
#
# The run of an opening checks the head it was raised for only while the pull
# request still has that head when the run decides. It is created some seconds
# after the pull request, and a push right after the opening has a run of its
# own that counts the pull request as earlier and checks the new head, so the
# opening's run decides run=false once the pull request's head has moved; this
# assumes every move of the branch is a push that starts a run, which a push
# made with a workflow's own token would not. The run has a concurrency group
# apart from the pushes' (gate.yml, io-hosts.yml, sanitizers.yml), so it never
# cancels a push run. A ready pull request's reopening always runs: a push
# while it was closed was skipped as having no open pull request, so no other
# run may check its head. A ready pull request from a fork is not asked: no
# push to the fork starts a run here, so its opening always runs.
#
# It reads GITHUB_EVENT_NAME, GITHUB_EVENT_PATH, GITHUB_REF, GITHUB_REF_NAME,
# GITHUB_REPOSITORY, GITHUB_REPOSITORY_OWNER, GITHUB_RUN_ID and
# GITHUB_WORKFLOW_REF, as Actions sets them, and queries GitHub with `gh`
# (GH_TOKEN) for a push to a branch other than main, for the opening of a pull
# request from this repository and for the marking of one ready. A failed
# query fails the step rather than quietly skipping the checks.
# .github/test-gate-scope.sh exercises it.
set -eu

event() {
    jq -r "$1" "$GITHUB_EVENT_PATH"
}

decide() {
    printf 'run=%s\nreason=%s\n' "$1" "$2"
}

case "$GITHUB_EVENT_NAME" in
    workflow_dispatch)
        decide true 'manual run'
        ;;
    pull_request)
        action=$(event .action)
        number=$(event .pull_request.number)
        draft=$(event .pull_request.draft)
        case "$action" in
            ready_for_review)
                head_sha=$(event .pull_request.head.sha)
                workflow=$(basename "${GITHUB_WORKFLOW_REF%@*}")
                # With --method GET, gh sends each -f field as a query
                # parameter. The first run, if any, is the newest.
                manual=$(gh api --method GET "repos/$GITHUB_REPOSITORY/actions/workflows/$workflow/runs" \
                    -f head_sha="$head_sha" -f event=workflow_dispatch -f status=success -f per_page=1 \
                    --jq '.workflow_runs[0].id // empty')
                case "$manual" in
                    '')
                        decide true "pull request $number marked ready"
                        ;;
                    *[!0-9]*)
                        echo "unexpected manual run of $workflow at $head_sha: $manual" >&2
                        exit 1
                        ;;
                    *)
                        decide false "pull request $number marked ready, and its head $(printf '%.12s' "$head_sha") has the successful manual run $manual"
                        ;;
                esac
                ;;
            opened | reopened)
                head_sha=$(event .pull_request.head.sha)
                head_repo=$(event .pull_request.head.repo.full_name)
                tip=$head_sha
                if [ "$draft" = false ] && [ "$action" = opened ] && [ "$head_repo" = "$GITHUB_REPOSITORY" ]; then
                    tip=$(gh api "repos/$GITHUB_REPOSITORY/pulls/$number" --jq .head.sha)
                    case "$tip" in
                        '' | *[!0-9a-f]*)
                            echo "unexpected head of pull request $number: $tip" >&2
                            exit 1
                            ;;
                    esac
                fi
                if [ "$draft" != false ]; then
                    decide false "pull request $number $action as a draft"
                elif [ "$tip" != "$head_sha" ]; then
                    decide false "pull request $number $action at $(printf '%.12s' "$head_sha"), its branch has moved to $(printf '%.12s' "$tip"), whose push run checks it"
                else
                    decide true "pull request $number $action, ready"
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
            decide true 'push to main'
        elif [ "${GITHUB_REF#refs/heads/}" = "$GITHUB_REF" ]; then
            decide false "push to $GITHUB_REF, which is not a branch"
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
                decide true "push to ready pull request $ready"
            elif [ -n "$drafts" ]; then
                decide false "push to draft pull request $drafts, whose full check is started by hand or by marking it ready"
            elif [ -n "$later" ]; then
                decide false "push before pull request $later was opened; the run of its opening checks the head"
            else
                decide false "push to $GITHUB_REF_NAME, which has no open pull request"
            fi
        fi
        ;;
    *)
        echo "unsupported event: $GITHUB_EVENT_NAME" >&2
        exit 1
        ;;
esac
