# Whitefoot's canonical all-tests entry point: compiler checks and tests, the
# complete native conformance adapter, conformance structure and coverage,
# and specification/archive identity.
# The adapter prints its current tally rather than baking a count into this
# file.

PY := python3 -B
CHECK_RUN := perl $(CURDIR)/.github/run-check.pl
# Everything built or measured outside the checkout is written under this
# root, and no path any developer's machine happens to have is encoded in it:
# the default is the system temporary directory, which every supported host
# already defines. macOS exports TMPDIR with a trailing slash, so the trailing
# slash is stripped and the shell spellings elsewhere (`${TMPDIR:-/tmp}`) at
# worst produce a harmless doubled separator. Set the variable to keep the
# work somewhere durable: a temporary directory may be cleared on reboot.
WHITEFOOT_SCRATCH_ROOT ?= $(patsubst %/,%,$(if $(TMPDIR),$(TMPDIR),/tmp))/whitefoot
RESEARCH_TEST_TMP := $(WHITEFOOT_SCRATCH_ROOT)/whitefoot-research-tests-tmp
RESEARCH_CARGO_TARGET := $(WHITEFOOT_SCRATCH_ROOT)/whitefoot-research-tests-target

# One group inventory for local execution and the hosted correctness matrix.
# The _check-<group> recipes below own the commands on both paths. CI reads
# check-groups and invokes check-group; it keeps no second command inventory.
CHECK_GROUPS := static unit corpus runtime
ifeq ($(strip $(CHECK_GROUPS)),)
$(error the correctness group inventory must not be empty)
endif

# Where the stage table is assembled. A gate nobody can profile is a gate that
# silently grows: `check` times each stage and ends with the breakdown, so a
# stage that doubled is visible in the run that doubled it rather than a month
# later. `make -C compiler check` prints its own breakdown the same way.
STAGE_DIR := $(WHITEFOOT_SCRATCH_ROOT)/whitefoot-gate-stages

# Keep an unexpected executable stop from leaving a large core artifact in the
# gate workspace. Such a stop fails the adapter; it is never a corpus verdict.
NO_CORE_DUMPS := ulimit -c 0;

check:
	@$(CHECK_RUN) gate $(MAKE) --no-print-directory _check

_check:
	@mkdir -p "$(STAGE_DIR)"
	@: > "$(STAGE_DIR)/summary"
	@echo "correctness groups: $(CHECK_GROUPS)"
	@for group in $(CHECK_GROUPS); do \
		started=$$(date +%s); \
		$(MAKE) --no-print-directory check-group GROUP="$$group" || exit 1; \
		printf '%-28s %6d s\n' "$$group" "$$(( $$(date +%s) - started ))" \
			>> "$(STAGE_DIR)/summary"; \
	done
	@echo ""
	@echo "stage wall, this run:"
	@cat "$(STAGE_DIR)/summary"
	@echo "== WHITEFOOT ALL TESTS GREEN =="

# JSON is consumed directly by the GitHub Actions matrix. This is a view of
# CHECK_GROUPS, not generated source or another list to maintain.
check-groups:
	@printf '['; separator=''; \
	for group in $(CHECK_GROUPS); do \
		printf '%s"%s"' "$$separator" "$$group"; separator=,; \
	done; printf ']\n'

check-group:
	@$(if $(and $(filter 1,$(words $(GROUP))),$(filter $(CHECK_GROUPS),$(GROUP))),:,$(error GROUP must name one correctness group from: $(CHECK_GROUPS)))
	@$(CHECK_RUN) "check/$(GROUP)" $(MAKE) --no-print-directory "_check-$(GROUP)"

.PHONY: _check-static
_check-static:
	@$(MAKE) static
	@for stage in conformance performance-instrument; do \
		$(CHECK_RUN) "$$stage" $(MAKE) --no-print-directory "$$stage" || exit 1; \
	done
	@$(MAKE) -C compiler lint

.PHONY: _check-unit
_check-unit:
	@$(MAKE) -C compiler build
	@$(MAKE) -C compiler test-build-unit
	@$(MAKE) -C compiler test-unit

.PHONY: _check-corpus
_check-corpus:
	@$(MAKE) -C compiler test-build-corpus
	@$(MAKE) -C compiler test-corpus

.PHONY: _check-runtime
_check-runtime:
	@$(MAKE) -C compiler completion-test

# AGENTS.md is the repository's agent entry point and carries its project rules.
# The repository-level stages that read the tree without running a compiled
# program. CI's `static` job runs this instead of restating their names: a
# second copy of the list is a copy that goes stale, and did — retiring two
# stages left the workflow naming targets that no longer exist.
static:
	@for stage in repository-invariants spec-archives spec-prose-integrity guidance source-size design-lint; do \
		$(CHECK_RUN) "$$stage" $(MAKE) --no-print-directory "$$stage" || exit 1; \
	done

# Structural lint for the design tree; form only, see design/skill/lint.py.
# CI pins the event's review base instead of comparing main with its own tip.
DESIGN_REVIEW_BASE ?= origin/main
design-lint:
	@$(PY) -m unittest discover -s design/skill -p 'test_lint.py'
	@$(PY) design/skill/lint.py --trees language compiler --base "$(DESIGN_REVIEW_BASE)"

design-ready:
	@$(PY) design/skill/lint.py --trees language compiler --base "$(DESIGN_REVIEW_BASE)" --require-no-amendments

repository-invariants:
	@$(PY) .github/check-research-inputs.py --self-test
	@$(PY) .github/check-research-inputs.py
	@sh .github/test-run-check.sh
	@test -s AGENTS.md || { echo "AGENTS.md missing" >&2; exit 1; }
	@mac_home="$$(printf '/%s/' Users)"; \
	linux_home="$$(printf '/%s/' home)"; \
	encoded_home="$$(printf -- '-%s-' Users)"; \
	windows_home="$$(printf '\\%s\\' Users)"; \
	matches="$$(git grep -a -l -F -e "$$mac_home" -e "$$linux_home" -e "$$encoded_home" -e "$$windows_home" -- . || { status=$$?; test "$$status" -eq 1 || exit "$$status"; })" || exit 1; \
	name_matches="$$(git ls-files | grep -F -e "$$mac_home" -e "$$linux_home" -e "$$encoded_home" -e "$$windows_home" || { status=$$?; test "$$status" -eq 1 || exit "$$status"; })" || exit 1; \
	if test -n "$$matches$$name_matches"; then \
		echo "repository invariants: tracked content or filenames contain a personal home path:" >&2; \
		test -z "$$matches" || echo "$$matches" >&2; \
		test -z "$$name_matches" || echo "$$name_matches" >&2; \
		exit 1; \
	fi
# A personal home path is not the only way a developer's own machine leaks
# into the tree: a bare directory name does it too, and reads as a convention
# every reader is expected to have. The one that got in was a local
# antivirus skip folder used as the default scratch root; the scratch root is
# now the system temporary directory, which every host defines for itself.
# The name is spelled here as a concatenation so this rule does not match
# itself. `archive/` is frozen and keeps its historical text.
	@local_dir="$$(printf '%s_%s_%s' do not scan)"; \
	matches="$$(git grep -a -l -F -e "$$local_dir" -- . ':(exclude)archive' || { status=$$?; test "$$status" -eq 1 || exit "$$status"; })" || exit 1; \
	name_matches="$$(git ls-files -- . ':(exclude)archive' | grep -F -e "$$local_dir" || { status=$$?; test "$$status" -eq 1 || exit "$$status"; })" || exit 1; \
	if test -n "$$matches$$name_matches"; then \
		echo "repository invariants: tracked content or filenames encode a local machine directory name; no directory of any developer's own machine belongs in the repository:" >&2; \
		test -z "$$matches" || echo "$$matches" >&2; \
		test -z "$$name_matches" || echo "$$name_matches" >&2; \
		exit 1; \
	fi

# Released version archives are never edited, and an amendment archives the
# outgoing bytes under their version and advances the title. The script
# compares with the merge base of main, which is what a merge would change in
# main; a branch behind main is not charged with main's newer archives.
spec-archives:
	@git rev-parse --verify --quiet refs/heads/main >/dev/null || { echo "spec archives: local main ref is required" >&2; exit 1; }
	@sh .github/check-spec-archives.sh --self-test
	@sh .github/check-spec-archives.sh main

# Cited review items, entry-document paths and the two agents' skill links
# resolve; this reads references only, never the guidance's meaning.
guidance:
	@$(PY) .github/check-guidance.py --self-test
	@$(PY) .github/check-guidance.py

# A compiler source over the limit is named in the Code structure section of
# docs/todo.md, so its split is recorded work instead of unnoticed growth
# (AGENTS.md, "Fix or record what you notice"). The item may defer the split;
# it may not be missing. A mention in another section, about something else,
# does not count. The limit sits above every source except the entailment
# module's three, each thousands of lines past it, so it asks for no items the
# evidence does not already call for; lower it once those are split.
SOURCE_LINE_LIMIT ?= 4000
source-size:
	@status=0; checked=0; \
	recorded="$$(awk '/^## / { in_section = ($$0 == "## Code structure") } in_section' docs/todo.md)"; \
	test -n "$$recorded" || { echo "source size: docs/todo.md has no Code structure section" >&2; exit 1; }; \
	for file in $$(git ls-files -- 'compiler/src/*.rs'); do \
		checked=$$((checked + 1)); \
		lines="$$(wc -l < "$$file" | tr -d ' ')"; \
		if test "$$lines" -gt $(SOURCE_LINE_LIMIT) && ! printf '%s\n' "$$recorded" | grep -q -F -e "$$file"; then \
			echo "source size: $$file has $$lines lines, over $(SOURCE_LINE_LIMIT); split it along its responsibilities, or name it in the Code structure section of docs/todo.md with the split you would make" >&2; \
			status=1; \
		fi; \
	done; \
	test "$$checked" -gt 0 || { echo "source size: no compiler sources found" >&2; exit 1; }; \
	exit $$status

# What a completion review covers: base, depth, groups and excluded paths.
review-scope:
	@sh docs/skills/completion-review/scripts/review-scope.sh main

spec-append-only-staged:
	@changes="$$(git diff --cached --name-status --diff-filter=MDRCT -- 'spec/kernel-spec-v*.md')" || exit 1; \
	if test -n "$$changes"; then \
		echo "spec append-only violation: released specifications changed:" >&2; \
		echo "$$changes" >&2; \
		exit 1; \
	fi
	@echo "spec append-only: no released kernel specification was modified or removed"

# The specification's own bytes are its identity, and build.rs derives them
# on every build that touches those bytes. Live prose quotes neither a digest
# nor an "active vN" sentence: both went stale at every activation, so this
# negative check keeps them out of the guidance files.
spec-prose-integrity:
	@failed=0; \
	for file in README.md AGENTS.md docs/*.md; do \
		if grep -nE '(^|[^0-9a-f])[0-9a-f]{64}([^0-9a-f]|$$)' "$$file"; then \
			echo "spec prose integrity: $$file quotes a specification digest; the identity is derived from the specification's own bytes" >&2; failed=1; \
		fi; \
	done; \
	for file in README.md AGENTS.md docs/*.md; do \
		if grep -nE 'Kernel specification v[0-9]+\.[0-9]+ is the active|[Aa]ctive language authority(:| is) v[0-9]+\.[0-9]+|active v[0-9]+\.[0-9]+ (guidance|authority)|the exact v[0-9]+\.[0-9]+ bytes' "$$file"; then \
			echo "spec prose integrity: $$file names a version as the active authority; say 'the active specification at spec/kernel-spec.md' instead" >&2; failed=1; \
		fi; \
	done; \
	test "$$failed" -eq 0 || exit 1; \
	echo "spec prose integrity: live prose quotes no specification digest and names no version as the active authority"

conformance:
	cd tests/conformance && $(PY) test_runner.py
	$(PY) tests/conformance/runner.py coverage

compiler:
	$(MAKE) -C compiler check

# Test the separate paired runner's result integrity with synthetic data only.
# No compiler, native image or timing campaign is part of this gate stage.
performance-instrument:
	@sh tests/performance/test-verdict.sh

# Reproduce the self-tests of completed research instruments when revisiting
# their dated results. This is deliberately outside the active compiler gate.
# research-boundary: manual-only
historical-tool-tests:
	@$(CHECK_RUN) historical-tool-tests $(MAKE) --no-print-directory _historical-tool-tests

# research-boundary: manual-only
_historical-tool-tests:
	@mkdir -p "$(RESEARCH_TEST_TMP)/frequency" "$(RESEARCH_CARGO_TARGET)"
	TMPDIR="$(RESEARCH_TEST_TMP)/frequency" $(MAKE) -C research/experiments/frequency-study check PYTHON=python3 CARGO_TARGET_DIR="$(RESEARCH_CARGO_TARGET)/frequency"
	cd research/experiments/default-floor && TMPDIR="$(RESEARCH_TEST_TMP)" $(PY) -m unittest discover -s tests -p 'test_*.py' -v
	TMPDIR="$(RESEARCH_TEST_TMP)" CARGO_TARGET_DIR="$(RESEARCH_CARGO_TARGET)/utf8-baseline" cargo test --locked --offline --manifest-path research/experiments/default-floor/utf8parse/rust-baseline/Cargo.toml
	TMPDIR="$(RESEARCH_TEST_TMP)" CARGO_TARGET_DIR="$(RESEARCH_CARGO_TARGET)/utf8-harness" cargo test --locked --offline --manifest-path research/experiments/default-floor/utf8parse/harness/Cargo.toml
	TMPDIR="$(RESEARCH_TEST_TMP)" CARGO_TARGET_DIR="$(RESEARCH_CARGO_TARGET)/percent-baseline" cargo test --locked --offline --manifest-path research/experiments/default-floor/percent-decode/rust-baseline/Cargo.toml
	TMPDIR="$(RESEARCH_TEST_TMP)" CARGO_TARGET_DIR="$(RESEARCH_CARGO_TARGET)/percent-harness" cargo test --locked --offline --manifest-path research/experiments/default-floor/percent-decode/harness/Cargo.toml

# Focused conformance invocation. The full gate already reaches this ordinary
# test through the shared corpus executable and must not run it twice.
conformance-run:
	$(NO_CORE_DUMPS) cd compiler && $(CHECK_RUN) conformance-run cargo test --profile gate --test corpus --locked --offline -- conformance::adapter:: --nocapture

# one-time: point git at the tracked hooks (pre-commit and pre-merge-commit)
install-hooks:
	git config core.hooksPath governance/hooks
	@echo "installed governance/hooks (pre-commit, pre-merge-commit)"

.PHONY: historical-tool-tests _historical-tool-tests check _check check-groups check-group static repository-invariants spec-archives guidance source-size review-scope spec-append-only-staged spec-prose-integrity design-lint design-ready conformance compiler performance-instrument conformance-run install-hooks
