CARGO := cargo
.DELETE_ON_ERROR:
CHECK_RUN := perl $(abspath ../.github/run-check.pl)
CC ?= cc
LLVM_CC ?= clang
WINDOWS_CC ?= x86_64-w64-mingw32-gcc
WINDOWS_LLVM_CC ?= clang --target=x86_64-w64-windows-gnu
WINE ?= /usr/lib/wine/wine64
WINEPREFIX ?= $(WHITEFOOT_SCRATCH_ROOT)/wineprefix
WHITEFOOT_SCRATCH_ROOT ?= $(patsubst %/,%,$(if $(TMPDIR),$(TMPDIR),/tmp))/whitefoot
STAGE_DIR := $(WHITEFOOT_SCRATCH_ROOT)/whitefoot-gate-stages

# Every gate target that executes a compiled program runs it with the core
# file limit at zero.
#
# A few runtime-boundary and deliberately damaged-lowering tests end a child
# process while checking resource reporting and test-oracle sensitivity. On a
# host that writes or analyses a core for each one, the gate pays for evidence
# nothing reads. Measured in batch 0093, that is where the Linux gate's time
# went: the same library suite took 1016 seconds on the four-core
# `ubuntu-24.04` runner and 55 on the three-core `macos-14` runner beside it.
#
# This limit is the portable half of declining that, and it is deliberately not
# the whole of it. A Linux `kernel.core_pattern` that begins with a pipe —
# which `ubuntu-24.04` has, naming `systemd-coredump` — makes the kernel ignore
# RLIMIT_CORE entirely, because the limit bounds the size of a file and a pipe
# writes none. Measured: forty aborting runs left forty stored core dumps at a
# soft limit of 0 and forty more at a soft limit of 1, and none once the
# pattern named a file. A host in that state has to change the pattern, which
# needs root; `.github/workflows/gate.yml` does exactly that in one step and
# says so there. What this line covers is every host whose pattern names a
# file, where a limit of zero declines the dump the ordinary way.
#
# Nothing a case asserts depends on a core file existing: the signal that kills
# the process, its exit status and its record channel are all unchanged.
NO_CORE_DUMPS := ulimit -c 0;

# Cargo discovers library/bin cases and all integration targets. No manually
# enumerated sampling partition or redundant list-only Rust build is needed.
# Format and API-documentation commands remain available for focused authoring;
# the correctness gate runs type/lint checks and executable assertions.
COMPILER_STAGES := build lint test-build test-unit test-corpus completion-test

COMPLETION_DIR := src/backend/completion
# Completion builds use the same compute pool and process configuration as
# ordinary compute programs; typed completion records own their own progress.
SCHED_DIR := src/backend/sched
# The core owns current-stack join/help/steal, entry.c owns configuration and
# startup policy, and the native leaf supplies threads and per-lane waiting.
SCHED_SOURCES := $(SCHED_DIR)/core.c $(SCHED_DIR)/prim_host.c $(SCHED_DIR)/entry.c
COMPLETION_TMP ?= $(WHITEFOOT_SCRATCH_ROOT)/whitefoot-completion-test
COMPLETION_DEFINES := \
	-DWF_COMPLETION_GETDIRENTRIES64=wf_completion_test_getdirentries64 \
	-DWF_COMPLETION_GETDENTS64=wf_completion_test_getdents64 \
	-DWF_COMPLETION_POLL=wf_completion_test_poll
COMPLETION_BASE_CFLAGS := -std=c11 -O2 -g -Wall -Wextra -Werror -Wpedantic \
	-pthread
COMPLETION_CFLAGS := $(COMPLETION_BASE_CFLAGS) $(COMPLETION_DEFINES)
# The harness names the WF_IO_NOCACHE application so it can count it. The
# named function performs the same host call the shipped policy performs, so
# only the counting is added, and only this build has it.
#
# It also names the clock the helper policy measures host calls with, so that
# what the growth rule decides can be tested by scripting the measurement
# instead of by sleeping and hoping the machine agreed. Absent a script the
# named clock answers with the host's own monotonic clock, exactly as the
# shipped build does.
#
# It further names the adapter open so a test can stand at the moment the
# host refuses one, and the ledger's schedule point so a test can stand
# between the two reads a give-up decision makes. The named functions
# perform the same host calls the shipped code performs and the point does
# nothing at all where it is unnamed, so only the observation is added.
#
# The defines are named once, here, and used by every build that links the
# harness: a sanitizer build that named a different set would run a different
# runtime from the one the ordinary build runs.
COMPLETION_HARNESS_DEFINES := \
	-DWF_ORDINARY_TEST_IN_HARNESS \
	-DWF_COMPLETION_SHUTDOWN=wf_ordinary_test_shutdown \
	-DWF_FILE_UNCACHED_APPLY=wf_completion_test_uncached_apply \
	-DWF_FILE_MONOTONIC_NS=wf_completion_test_monotonic_ns \
	-DWF_FILE_OPENAT=wf_completion_test_openat \
	-DWF_COMPLETION_WAIT_RETURN=wf_completion_test_wait_return
COMPLETION_HARNESS_CFLAGS := $(COMPLETION_CFLAGS) $(COMPLETION_HARNESS_DEFINES)
COMPLETION_RUNTIME_SOURCES := \
	$(SCHED_SOURCES) \
	$(COMPLETION_DIR)/runtime.c \
	$(COMPLETION_DIR)/wait_host.c \
	$(COMPLETION_DIR)/file_adapter.c \
	$(COMPLETION_DIR)/file_posix.c \
	$(COMPLETION_DIR)/bridge.c \
	$(COMPLETION_DIR)/linux_io_uring.c
COMPLETION_SOURCES := $(COMPLETION_RUNTIME_SOURCES) \
	src/backend/ordinary_values.c src/backend/ordinary_values_probe.c \
	src/backend/ordinary_values.ll src/backend/ordinary_values_probe.ll \
	$(COMPLETION_DIR)/harness.c
# The same runtime the harness links, with the probe in place of the harness.
# It is deliberately built without COMPLETION_DEFINES: the route this probe is
# about is the shipped one, so it makes the shipped host calls and reads the
# host's own clock.
COMPLETION_DEFAULT_ROUTE_SOURCES := \
	$(COMPLETION_RUNTIME_SOURCES) \
	$(COMPLETION_DIR)/bridge_default_probe.c
# Isolated adapter cases supply their own strong completion publication.
# The explicit link inputs preserve that different implementation boundary.
COMPLETION_CORE_READ_SOURCES := \
	$(SCHED_SOURCES) \
	$(COMPLETION_DIR)/runtime.c \
	$(COMPLETION_DIR)/wait_host.c \
	$(COMPLETION_DIR)/file_adapter.c \
	$(COMPLETION_DIR)/file_posix.c \
	$(COMPLETION_DIR)/core_read_probe.c
COMPLETION_CORE_READ_DEFINES := \
	-DWF_COMPLETION_PREAD=wf_completion_test_pread

check:
	@$(CHECK_RUN) compiler/check $(MAKE) --no-print-directory _check

_check:
	@mkdir -p "$(STAGE_DIR)"
	@: > "$(STAGE_DIR)/compiler-summary"
	@for stage in $(COMPILER_STAGES); do \
		started=$$(date +%s); \
		$(CHECK_RUN) "compiler/$$stage" $(MAKE) --no-print-directory "$$stage" || exit 1; \
		printf '  compiler/%-18s %6d s\n' "$$stage" \
			"$$(( $$(date +%s) - started ))" >> "$(STAGE_DIR)/compiler-summary"; \
	done
	@echo ""
	@echo "compiler stage wall, this run:"
	@cat "$(STAGE_DIR)/compiler-summary"
	@echo "== WHITEFOOT COMPILER GATE GREEN =="

# Compatibility entry for type/lint and native runtime correctness checks.
# Formatting and API-documentation generation have their own explicit targets.
static:
	@$(CHECK_RUN) compiler/static $(MAKE) --no-print-directory _static

_static: lint completion-test
	@echo "== WHITEFOOT COMPILER STATIC GATE GREEN =="

# These targets construct artifacts without running the Rust test cases.
build:
	$(CHECK_RUN) compiler/build $(CARGO) build --profile gate --bin whitefootc --locked --offline

test-build:
	$(CHECK_RUN) compiler/test-build $(CARGO) test --profile gate --all-targets --no-run --locked --offline

# The library's own tests build only the library in test mode. The CLI's tests
# link the ordinary library, which the corpus harness builds anyway, so they
# run with the corpus instead of costing the unit group a second library build.
test-build-unit:
	$(CHECK_RUN) compiler/test-build-unit $(CARGO) test --profile gate --lib --no-run --locked --offline

test-build-corpus:
	$(CHECK_RUN) compiler/test-build-corpus $(CARGO) test --profile gate --bins --test '*' --no-run --locked --offline

format:
	$(CARGO) fmt --all -- --check

lint:
	$(CHECK_RUN) compiler/lint $(CARGO) clippy --all-targets --locked --offline -- -D warnings

# Gate code generation retains debug assertions and overflow checks. Cargo's
# default dev/test profile remains available for debugging, not a second gate.
test:
	@$(CHECK_RUN) compiler/test $(MAKE) --no-print-directory _test

_test: test-build test-unit test-corpus

# Private compiler obligations in the library's own test modules.
test-unit:
	$(NO_CORE_DUMPS) $(CHECK_RUN) compiler/test-unit $(CARGO) test --profile gate --lib --locked --offline

# Every integration target is discovered by Cargo, so adding one cannot fall
# out of a copied target list. The shared corpus owns programs, conformance,
# canonical rendering and the CLI's own tests; no baseline compiler or timed
# comparison is involved.
test-corpus:
	$(NO_CORE_DUMPS) $(CHECK_RUN) compiler/test-corpus $(CARGO) test --profile gate --bins --test '*' --locked --offline

docs:
	RUSTDOCFLAGS="-D warnings" $(CARGO) doc --no-deps --locked --offline

# Construction is shared by local, focused and host-CI callers; phony test
# targets always rerun assertions. Distinct hook/sanitizer flags name distinct
# objects. The stamp prevents a shared scratch root reusing another checkout
# or compiler/flag configuration, without caching a previous test verdict.
COMPLETION_HEADERS := $(wildcard src/backend/*.h $(SCHED_DIR)/*.h $(COMPLETION_DIR)/*.h)
COMPLETION_CONFIG := $(COMPLETION_TMP)/configuration.txt
.PHONY: completion-configuration-force
completion-configuration-force:

$(COMPLETION_CONFIG): completion-configuration-force
	@mkdir -p "$(COMPLETION_TMP)"
	@{ printf '%s\n' "$(CURDIR)" "$(CC)" "$(LLVM_CC)" "$(COMPLETION_BASE_CFLAGS)" "$(COMPLETION_HARNESS_CFLAGS)" "$(COMPLETION_CORE_READ_DEFINES)" "$(COMPLETION_SANITIZE_FLAGS)" "$(COMPLETION_TSAN_FLAGS)"; $(CC) --version; $(LLVM_CC) --version; } > "$@.new"
	@cmp -s "$@.new" "$@" || mv -f "$@.new" "$@"
	@rm -f "$@.new"

define completion-image
$(COMPLETION_TMP)/$(1): $(patsubst src/backend/%.ll,$(COMPLETION_TMP)/objects/$(1)/%_ir.o,$(patsubst src/backend/%.c,$(COMPLETION_TMP)/objects/$(1)/%.o,$(2)))
	$$(CC) $(3) $$^ -o "$$@"
$(COMPLETION_TMP)/objects/$(1)/%.o: src/backend/%.c $(COMPLETION_HEADERS) Makefile $(COMPLETION_CONFIG)
	@mkdir -p "$$(dir $$@)"
	$$(CC) $(3) -c "$$<" -o "$$@"
$(COMPLETION_TMP)/objects/$(1)/%_ir.o: src/backend/%.ll Makefile $(COMPLETION_CONFIG)
	@mkdir -p "$$(dir $$@)"
	$$(LLVM_CC) $(filter-out -std=c11 -pthread -D%,$(3)) -Wno-override-module -Wno-unused-command-line-argument -x ir -c "$$<" -o "$$@"
.SECONDARY: $(patsubst src/backend/%.ll,$(COMPLETION_TMP)/objects/$(1)/%_ir.o,$(patsubst src/backend/%.c,$(COMPLETION_TMP)/objects/$(1)/%.o,$(2)))
endef

COMPLETION_SANITIZE_FLAGS := -std=c11 -O1 -g -Wall -Wextra -Werror -Wpedantic -pthread -fsanitize=address,undefined -fno-sanitize-recover=undefined
COMPLETION_TSAN_FLAGS := -std=c11 -O1 -g -Wall -Wextra -Werror -Wpedantic -pthread -fsanitize=thread
# A required host job makes missing native capability an error; local checks
# report the native probe's unavailable result and still test the adapter.
REQUIRE_NATIVE ?= 0
NATIVE_REQUIREMENT := $(if $(filter 1,$(REQUIRE_NATIVE)),WF_REQUIRE_LINUX_IO_URING=1,)
COMPLETION_NATIVE_SOURCES := $(SCHED_SOURCES) $(COMPLETION_DIR)/runtime.c \
    $(COMPLETION_DIR)/wait_host.c $(COMPLETION_DIR)/linux_io_uring.c \
    $(COMPLETION_DIR)/native_adapter_probe.c
$(eval $(call completion-image,linux-native-probe,$(COMPLETION_NATIVE_SOURCES),$(COMPLETION_BASE_CFLAGS)))

$(eval $(call completion-image,harness,$(COMPLETION_SOURCES),$(COMPLETION_HARNESS_CFLAGS)))
$(eval $(call completion-image,bridge-default-probe,$(COMPLETION_DEFAULT_ROUTE_SOURCES),$(COMPLETION_BASE_CFLAGS)))
$(eval $(call completion-image,core-read-probe,$(COMPLETION_CORE_READ_SOURCES),$(COMPLETION_BASE_CFLAGS) $(COMPLETION_CORE_READ_DEFINES)))
$(eval $(call completion-image,harness-sanitize,$(COMPLETION_SOURCES),$(COMPLETION_SANITIZE_FLAGS) $(COMPLETION_DEFINES) $(COMPLETION_HARNESS_DEFINES)))
$(eval $(call completion-image,bridge-default-probe-sanitize,$(COMPLETION_DEFAULT_ROUTE_SOURCES),$(COMPLETION_SANITIZE_FLAGS)))
$(eval $(call completion-image,core-read-probe-sanitize,$(COMPLETION_CORE_READ_SOURCES),$(COMPLETION_SANITIZE_FLAGS) $(COMPLETION_CORE_READ_DEFINES)))
$(eval $(call completion-image,harness-tsan,$(COMPLETION_SOURCES),$(COMPLETION_TSAN_FLAGS) $(COMPLETION_DEFINES) $(COMPLETION_HARNESS_DEFINES)))
$(eval $(call completion-image,bridge-default-probe-tsan,$(COMPLETION_DEFAULT_ROUTE_SOURCES),$(COMPLETION_TSAN_FLAGS)))

completion-test: $(COMPLETION_TMP)/harness completion-core-read-test completion-default-route-test sched-smoke sched-deque-test sched-policy-test floor-test concurrent-map-test shared-object-test completion-native-test
	@mkdir -p "$(COMPLETION_TMP)"
	$(NO_CORE_DUMPS) $(NATIVE_REQUIREMENT) WF_IO_HELPERS=1 "$(COMPLETION_TMP)/harness" "$(COMPLETION_TMP)" all
	$(NO_CORE_DUMPS) $(NATIVE_REQUIREMENT) WF_IO_HELPERS=0 "$(COMPLETION_TMP)/harness" "$(COMPLETION_TMP)" bridge
	$(NO_CORE_DUMPS) $(NATIVE_REQUIREMENT) WF_IO_HELPERS=4 "$(COMPLETION_TMP)/harness" "$(COMPLETION_TMP)" bridge
	@if [ "$$(uname -s)" = Linux ]; then \
		for helpers in 0 4; do \
			$(NO_CORE_DUMPS) env -u WF_REQUIRE_LINUX_IO_URING WF_IO_NO_NATIVE_RING=1 WF_IO_HELPERS=$$helpers \
				"$(COMPLETION_TMP)/harness" "$(COMPLETION_TMP)" bridge || exit $$?; \
		done; \
	fi
	@# The full run checks the absent cache hint; this fresh process checks
	@# only its enabled policy, without repeating unrelated runtime cases.
	WF_IO_NOCACHE=1 WF_IO_HELPERS=1 "$(COMPLETION_TMP)/harness" "$(COMPLETION_TMP)" cache
# Build once and execute once. A native probe's 77 is observable locally and
# fatal in qualification CI; fallback success never substitutes for it.
ifeq ($(shell uname -s),Linux)
completion-native-test: $(COMPLETION_TMP)/linux-native-probe
endif
completion-native-test:
	@if [ "$$(uname -s)" = Linux ]; then \
		$(NO_CORE_DUMPS) "$(COMPLETION_TMP)/linux-native-probe" "$(COMPLETION_TMP)/linux-native-probe.data"; \
		status=$$?; \
		if [ $$status -eq 77 ] && [ "$(REQUIRE_NATIVE)" != 1 ]; then \
			echo "native io_uring: unavailable on this local host (not native evidence)"; \
		else exit $$status; fi; \
	elif [ "$(REQUIRE_NATIVE)" = 1 ]; then \
		echo "required io_uring evidence needs a Linux host" >&2; exit 1; \
	fi

# The bridge on the helper policy a delivered program actually gets.
#
# Every harness run names WF_IO_HELPERS -- the three arms above name 0, 1 and 4
# and harness main pins 1 for any run that named nothing -- so the demand-driven
# pool and the declined positioned read, both of which choose between two routes
# for one writer-visible operation, were reachable from no test in the tree.
# `unset` rather than a bare invocation because make inherits this shell's
# environment, and the probe refuses an inherited value rather than quietly
# testing the arm the harness already covers.
ordinary-values-test: $(COMPLETION_TMP)/harness
	@mkdir -p "$(COMPLETION_TMP)"
	"$(COMPLETION_TMP)/harness" "$(COMPLETION_TMP)" ordinary-text
	WF_IO_HELPERS=0 "$(COMPLETION_TMP)/harness" "$(COMPLETION_TMP)" ordinary-io
	WF_IO_HELPERS=2 "$(COMPLETION_TMP)/harness" "$(COMPLETION_TMP)" ordinary-io

completion-default-route-test: $(COMPLETION_TMP)/bridge-default-probe
	@mkdir -p "$(COMPLETION_TMP)"
	$(NO_CORE_DUMPS) unset WF_IO_HELPERS; $(NATIVE_REQUIREMENT) \
		"$(COMPLETION_TMP)/bridge-default-probe" "$(COMPLETION_TMP)"
	@# The adapter branch of the probe's route assertion -- the demand-driven
	@# policy declined a read, or grew a helper -- cannot fire on a Linux host
	@# whose io_uring takes every positioned read, so on Linux the arm above
	@# covers only the native ring and the negative control was carried by the
	@# Darwin CI host alone. WF_IO_NO_NATIVE_RING puts this host on the route a
	@# kernel without io_uring already takes, and the probe refuses to pass if
	@# the ring answered anyway.
	@if [ "$$(uname -s)" = Linux ]; then \
		unset WF_IO_HELPERS; env -u WF_REQUIRE_LINUX_IO_URING WF_IO_NO_NATIVE_RING=1 \
			"$(COMPLETION_TMP)/bridge-default-probe" "$(COMPLETION_TMP)"; \
	fi

# Pure native scheduler cases have one Make owner, not duplicate Rust wrappers.
# Compatible entry/host objects are shared; statistics and sanitizers retain
# distinct object sets. Each case execution is fresh even when construction is reused.
SCHED_PROBE_OBJECT_DIR := $(COMPLETION_TMP)/objects/scheduler
$(SCHED_PROBE_OBJECT_DIR)/%.o: $(SCHED_DIR)/%.c $(COMPLETION_HEADERS) Makefile $(COMPLETION_CONFIG)
	@mkdir -p "$(dir $@)"
	$(CC) $(COMPLETION_BASE_CFLAGS) -c "$<" -o "$@"
SCHED_PROBE_SUPPORT := $(SCHED_PROBE_OBJECT_DIR)/entry.o $(SCHED_PROBE_OBJECT_DIR)/prim_host.o
SCHED_POLICY_INPUTS := $(SCHED_DIR)/wake_probe.c $(SCHED_DIR)/recursion_budget_probe.c

define scheduler-image
$(COMPLETION_TMP)/$(1): $(if $(4),$(4),$(SCHED_PROBE_SUPPORT)) $(SCHED_PROBE_OBJECT_DIR)/$(1).o
	$$(CC) $(COMPLETION_BASE_CFLAGS) $$^ -o "$$@"
$(SCHED_PROBE_OBJECT_DIR)/$(1).o: $(SCHED_DIR)/$(2).c $(SCHED_DIR)/core.c $(SCHED_POLICY_INPUTS) $(COMPLETION_HEADERS) Makefile $(COMPLETION_CONFIG)
	@mkdir -p "$$(dir $$@)"
	$$(CC) $(COMPLETION_BASE_CFLAGS) $(3) -c "$$<" -o "$$@"
endef
$(eval $(call scheduler-image,sched-smoke,smoke,-DWF_SCHED_TEST))
$(SCHED_PROBE_OBJECT_DIR)/entry-deque.o: $(SCHED_DIR)/entry.c $(COMPLETION_HEADERS) Makefile $(COMPLETION_CONFIG)
	@mkdir -p "$(dir $@)"
	$(CC) $(COMPLETION_BASE_CFLAGS) -DWF_SCHED_LANE_SLOTS=8u -c "$<" -o "$@"
$(eval $(call scheduler-image,sched-deque-probe,deque_probe,-DWF_SCHED_LANE_SLOTS=8u -DWF_SCHED_STATS=1,$(SCHED_PROBE_OBJECT_DIR)/entry-deque.o $(SCHED_PROBE_OBJECT_DIR)/prim_host.o))
$(eval $(call scheduler-image,sched-policy,cpu_levels_probe,))
$(eval $(call scheduler-image,sched-policy-asymmetric,cpu_levels_probe,-DWF_PAR_IDLE_WINDOW_ON_ASYMMETRIC=1))
$(eval $(call scheduler-image,sched-policy-disabled,cpu_levels_probe,-DWF_PAR_IDLE_WINDOW_US=0))
$(eval $(call scheduler-image,sched-policy-clamped,cpu_levels_probe,-DWF_PAR_RECURSION_LEAVES_PER_LANE='(1ull << 40)'))
# Entry observes WF_SCHED_STATS too; its disabled object cannot be borrowed
# from the ordinary image. TSan objects likewise retain their own full flags.
$(eval $(call completion-image,sched-deque-probe-no-stats,$(SCHED_DIR)/deque_probe.c $(SCHED_DIR)/entry.c $(SCHED_DIR)/prim_host.c,$(COMPLETION_BASE_CFLAGS) -DWF_SCHED_LANE_SLOTS=8u -DWF_SCHED_STATS=0))
$(eval $(call completion-image,sched-deque-tsan,$(SCHED_DIR)/deque_probe.c $(SCHED_DIR)/entry.c $(SCHED_DIR)/prim_host.c,$(COMPLETION_TSAN_FLAGS) -DWF_SCHED_LANE_SLOTS=8u -DWF_SCHED_STATS=1))
$(eval $(call completion-image,sched-deque-tsan-no-stats,$(SCHED_DIR)/deque_probe.c $(SCHED_DIR)/entry.c $(SCHED_DIR)/prim_host.c,$(COMPLETION_TSAN_FLAGS) -DWF_SCHED_LANE_SLOTS=8u -DWF_SCHED_STATS=0))
$(COMPLETION_TMP)/objects/sched-deque-probe-no-stats/sched/deque_probe.o \
$(COMPLETION_TMP)/objects/sched-deque-tsan/sched/deque_probe.o \
$(COMPLETION_TMP)/objects/sched-deque-tsan-no-stats/sched/deque_probe.o: $(SCHED_DIR)/core.c

sched-smoke: $(COMPLETION_TMP)/sched-smoke
	@set -e; for mode in normal owner-fail worker-fail partial partial-three startup-delayed startup-partial; do \
		$(NO_CORE_DUMPS) WF_WORKERS=4 "$(COMPLETION_TMP)/sched-smoke" $$mode; \
	done
	WF_WORKERS=1 "$(COMPLETION_TMP)/sched-smoke"
	@set +e; WF_WORKERS=invalid "$(COMPLETION_TMP)/sched-smoke" > "$(COMPLETION_TMP)/startup.out" 2> "$(COMPLETION_TMP)/startup.err"; status=$$?; \
		test $$status = 1 && test ! -s "$(COMPLETION_TMP)/startup.out" && \
		printf '%s\n' 'whitefoot scheduler: WF_WORKERS must be an integer from 0 through 64' | cmp - "$(COMPLETION_TMP)/startup.err"

sched-deque-test: $(COMPLETION_TMP)/sched-deque-probe $(COMPLETION_TMP)/sched-deque-probe-no-stats
	"$(COMPLETION_TMP)/sched-deque-probe"
	"$(COMPLETION_TMP)/sched-deque-probe-no-stats"

sched-deque-tsan: $(COMPLETION_TMP)/sched-deque-tsan $(COMPLETION_TMP)/sched-deque-tsan-no-stats
	TSAN_OPTIONS=halt_on_error=1 "$(COMPLETION_TMP)/sched-deque-tsan"
	TSAN_OPTIONS=halt_on_error=1 "$(COMPLETION_TMP)/sched-deque-tsan-no-stats"

sched-policy-test: sched-split-policy-cases $(COMPLETION_TMP)/sched-policy $(COMPLETION_TMP)/sched-policy-asymmetric $(COMPLETION_TMP)/sched-policy-disabled $(COMPLETION_TMP)/sched-policy-clamped
	WF_WORKERS=2 "$(COMPLETION_TMP)/sched-policy" host
	"$(COMPLETION_TMP)/sched-policy" wake
	@set -e; for variant in '' -asymmetric -disabled; do \
		for mode in fit oversubscribed unknown-count mixed; do \
			WF_WORKERS=4 "$(COMPLETION_TMP)/sched-policy$$variant" $$mode; \
		done; \
	done
	@set -e; for pair in '0 6' '1 6' '2 7' '4 8' '8 9' '16 10'; do \
		set -- $$pair; WF_WORKERS=$$1 "$(COMPLETION_TMP)/sched-policy" budget $$2; \
	done
	@set -e; for workers in 1 4; do \
		WF_WORKERS=$$workers "$(COMPLETION_TMP)/sched-policy-clamped" budget 24; \
	done

# Shared on POSIX and native Windows; each environment is a fresh process.
SCHED_POLICY_EXE ?= $(COMPLETION_TMP)/sched-policy
sched-split-policy-cases: $(SCHED_POLICY_EXE)
	@set -e; unset WF_SPLIT_WORK; WF_WORKERS=4 "$(SCHED_POLICY_EXE)" split 0 2 6 6
	@set -e; for row in 'empty 0 2 6 6' '0 0 0 0 0' '1 0 6 6 6' '60000 0 3 6 6' '1200000 0 0 3 6' '1000000000 0 0 0 6'; do \
		set -- $$row; policy=$$1; shift; test "$$policy" != empty || policy=''; \
		WF_WORKERS=4 WF_SPLIT_WORK="$$policy" "$(SCHED_POLICY_EXE)" split "$$@"; \
	done
	WF_WORKERS=64 WF_SPLIT_WORK=1 "$(SCHED_POLICY_EXE)" split 0 10 10 10
	@set -e; for policy in -1 no 1000000001 18446744073709551616; do \
		status=0; WF_WORKERS=4 WF_SPLIT_WORK="$$policy" "$(SCHED_POLICY_EXE)" split 0 0 0 0 > "$(COMPLETION_TMP)/split.out" 2> "$(COMPLETION_TMP)/split.err" || status=$$?; \
		test "$$status" = 1; test ! -s "$(COMPLETION_TMP)/split.out"; \
		tr -d '\r' < "$(COMPLETION_TMP)/split.err" > "$(COMPLETION_TMP)/split.normalized"; \
		printf '%s\n' 'whitefoot scheduler: WF_SPLIT_WORK must be an integer from 0 through 1000000000' | cmp - "$(COMPLETION_TMP)/split.normalized"; \
	done

# The floor body is C-only host evidence. The hooked variant substitutes only
# floor setup/park calls; compatible scheduler support objects are shared.
$(COMPLETION_TMP)/floor-probe: $(SCHED_PROBE_SUPPORT) $(SCHED_PROBE_OBJECT_DIR)/core.o $(SCHED_PROBE_OBJECT_DIR)/floor-probe.o
	$(CC) $(COMPLETION_BASE_CFLAGS) $^ -o "$@"
$(COMPLETION_TMP)/floor-probe-hooks: $(SCHED_PROBE_SUPPORT) $(SCHED_PROBE_OBJECT_DIR)/core.o $(SCHED_PROBE_OBJECT_DIR)/floor-probe-hooks.o
	$(CC) $(COMPLETION_BASE_CFLAGS) $^ -o "$@"
$(SCHED_PROBE_OBJECT_DIR)/floor-probe.o: src/backend/floor_probe.c src/backend/wf_floor.c $(COMPLETION_HEADERS) Makefile $(COMPLETION_CONFIG)
	@mkdir -p "$(dir $@)"
	$(CC) $(COMPLETION_BASE_CFLAGS) -c "$<" -o "$@"
$(SCHED_PROBE_OBJECT_DIR)/floor-probe-hooks.o: src/backend/floor_probe.c src/backend/wf_floor.c $(COMPLETION_HEADERS) Makefile $(COMPLETION_CONFIG)
	@mkdir -p "$(dir $@)"
	$(CC) $(COMPLETION_BASE_CFLAGS) -DWF_FLOOR_TEST_HOOKS -c "$<" -o "$@"
floor-test: $(COMPLETION_TMP)/floor-probe $(COMPLETION_TMP)/floor-probe-hooks
	WF_WORKERS=4 "$(COMPLETION_TMP)/floor-probe"
	WF_WORKERS=4 "$(COMPLETION_TMP)/floor-probe-hooks"

# The concurrent map, and its locked read built for measurement, each against
# a reference, accounting checks and recorded histories checked for
# linearizability (concurrent_map_test.c). The locked read changes only the
# tests of word keys and the narrowed hashes only those of entries, so those
# builds run only the tests they change; the default build runs every test.
# The test includes the map's source, so that it can drive interleavings
# through the map's own steps.
CONCURRENT_MAP_SOURCES := src/backend/concurrent_map.c src/backend/concurrent_map_test.c
CONCURRENT_MAP_VARIANTS := copy-out locked collide
concurrent-map-test: $(foreach v,$(CONCURRENT_MAP_VARIANTS),$(COMPLETION_TMP)/concurrent-map-test-$(v))
	@set -e; for v in $(CONCURRENT_MAP_VARIANTS); do "$(COMPLETION_TMP)/concurrent-map-test-$$v"; done
$(COMPLETION_TMP)/concurrent-map-test-copy-out: $(CONCURRENT_MAP_SOURCES) src/backend/concurrent_map.h Makefile
	@mkdir -p "$(dir $@)"
	$(CC) $(COMPLETION_BASE_CFLAGS) src/backend/concurrent_map_test.c -o "$@"
$(COMPLETION_TMP)/concurrent-map-test-locked: $(CONCURRENT_MAP_SOURCES) src/backend/concurrent_map.h Makefile
	@mkdir -p "$(dir $@)"
	$(CC) $(COMPLETION_BASE_CFLAGS) -DWF_CMAP_LOCKED_READ src/backend/concurrent_map_test.c -o "$@"
# Entries' hashes narrowed to four bits, so that most keys share one.
$(COMPLETION_TMP)/concurrent-map-test-collide: $(CONCURRENT_MAP_SOURCES) src/backend/concurrent_map.h Makefile
	@mkdir -p "$(dir $@)"
	$(CC) $(COMPLETION_BASE_CFLAGS) -DWF_CMAP_TAG_MASK=15 src/backend/concurrent_map_test.c -o "$@"

# The shared-object runtime's waits, with contexts whose frames the test
# writes and the moments the runtime lets it observe (shared_object_test.c),
# on one driver beside threads that hold the object. It links the runtime
# objects the default-route probe builds with the same flags, so the runtime
# is not compiled again for it. On one processor the threads never run
# beside the driver, and the test reports that it checked no hand-off.
SHARED_OBJECT_TEST_OBJECTS := \
	$(patsubst src/backend/%.c,$(COMPLETION_TMP)/objects/bridge-default-probe/%.o,$(COMPLETION_RUNTIME_SOURCES)) \
	$(COMPLETION_TMP)/objects/bridge-default-probe/completion/shared_object_test.o
$(COMPLETION_TMP)/shared-object-test: $(SHARED_OBJECT_TEST_OBJECTS)
	$(CC) $(COMPLETION_BASE_CFLAGS) $^ -o "$@"
shared-object-test: $(COMPLETION_TMP)/shared-object-test
	$(NO_CORE_DUMPS) "$(COMPLETION_TMP)/shared-object-test"

# Manual calibration, never a correctness or automatic performance input.
sched-wake-measure: $(COMPLETION_TMP)/sched-policy
	"$(COMPLETION_TMP)/sched-policy" wake measure

# The managed-stack enumerator is retired with its scheduler. Native compute
# protocol tests above and completion wake tests below exercise the new boundary.
# sched-deque-tsan checks observed concurrent deque reuse, task completion and
# counter accesses on real threads. It does not enumerate all interleavings,
# prove weak-memory ordering or liveness, or cover completion and stack-floor
# protocols. Their focused tests remain separate; this is not equivalent to
# the retired managed-stack state-space gate.
completion-default-route-sanitize: $(COMPLETION_TMP)/bridge-default-probe-sanitize
	@mkdir -p "$(COMPLETION_TMP)"
	unset WF_IO_HELPERS; ASAN_OPTIONS=detect_leaks=0 \
		$(NATIVE_REQUIREMENT) "$(COMPLETION_TMP)/bridge-default-probe-sanitize" "$(COMPLETION_TMP)"
	@if [ "$$(uname -s)" = Linux ]; then \
		unset WF_IO_HELPERS; env -u WF_REQUIRE_LINUX_IO_URING WF_IO_NO_NATIVE_RING=1 ASAN_OPTIONS=detect_leaks=0 \
			"$(COMPLETION_TMP)/bridge-default-probe-sanitize" "$(COMPLETION_TMP)"; \
	fi

# Thread sanitizer over the bridge rather than over the isolated core: the
# helper pool, the queue wake and the join spin are all on this route, and the
# core probe by construction links none of them.
completion-default-route-tsan: $(COMPLETION_TMP)/bridge-default-probe-tsan
	@mkdir -p "$(COMPLETION_TMP)"
	unset WF_IO_HELPERS; TSAN_OPTIONS=halt_on_error=1 \
		$(NATIVE_REQUIREMENT) "$(COMPLETION_TMP)/bridge-default-probe-tsan" "$(COMPLETION_TMP)"
	@if [ "$$(uname -s)" = Linux ]; then \
		unset WF_IO_HELPERS; env -u WF_REQUIRE_LINUX_IO_URING WF_IO_NO_NATIVE_RING=1 TSAN_OPTIONS=halt_on_error=1 \
			"$(COMPLETION_TMP)/bridge-default-probe-tsan" "$(COMPLETION_TMP)"; \
	fi

completion-core-read-test: $(COMPLETION_TMP)/core-read-probe
	@mkdir -p "$(COMPLETION_TMP)"
	"$(COMPLETION_TMP)/core-read-probe" "$(COMPLETION_TMP)"

completion-sanitize: $(COMPLETION_TMP)/harness-sanitize completion-core-read-sanitize \
		completion-default-route-sanitize
	@mkdir -p "$(COMPLETION_TMP)"
	$(NO_CORE_DUMPS) $(NATIVE_REQUIREMENT) ASAN_OPTIONS=detect_leaks=0 "$(COMPLETION_TMP)/harness-sanitize" "$(COMPLETION_TMP)"
	@if [ "$$(uname -s)" = Linux ]; then \
		env -u WF_REQUIRE_LINUX_IO_URING WF_IO_NO_NATIVE_RING=1 WF_IO_HELPERS=0 ASAN_OPTIONS=detect_leaks=0 \
			"$(COMPLETION_TMP)/harness-sanitize" "$(COMPLETION_TMP)" bridge; \
	fi

completion-core-read-sanitize: $(COMPLETION_TMP)/core-read-probe-sanitize
	@mkdir -p "$(COMPLETION_TMP)"
	ASAN_OPTIONS=detect_leaks=0 \
		"$(COMPLETION_TMP)/core-read-probe-sanitize" "$(COMPLETION_TMP)"

# The main image includes the deliberate ordinary-value shutdown race. Core/
# read is single-threaded and has no TSan or repeated-stress variant.
completion-tsan: $(COMPLETION_TMP)/harness-tsan
	@mkdir -p "$(COMPLETION_TMP)"
	$(NO_CORE_DUMPS) env -u WF_REQUIRE_LINUX_IO_URING WF_IO_NO_NATIVE_RING=1 TSAN_OPTIONS=halt_on_error=1 WF_IO_HELPERS=0 \
		"$(COMPLETION_TMP)/harness-tsan" "$(COMPLETION_TMP)" bridge
	$(NO_CORE_DUMPS) $(NATIVE_REQUIREMENT) TSAN_OPTIONS=halt_on_error=1 WF_IO_HELPERS=1 \
		"$(COMPLETION_TMP)/harness-tsan" "$(COMPLETION_TMP)"
	$(NO_CORE_DUMPS) env -u WF_REQUIRE_LINUX_IO_URING WF_IO_NO_NATIVE_RING=1 TSAN_OPTIONS=halt_on_error=1 WF_IO_HELPERS=4 \
		"$(COMPLETION_TMP)/harness-tsan" "$(COMPLETION_TMP)" bridge

# The Windows link set, in the same three groups the driver stages: the
# scheduler core with its Windows leaf, the completion runtime with its Windows
# wait, host leaf and ring, and the Windows host runtime beside them. It is the
# same list as the POSIX one with the leaves swapped, which is the whole point
# of step (iv).
WINDOWS_LINK_SOURCES := \
	$(SCHED_DIR)/core.c \
	$(SCHED_DIR)/prim_windows.c \
	$(SCHED_DIR)/entry.c \
	$(COMPLETION_DIR)/runtime.c \
	$(COMPLETION_DIR)/wait_windows.c \
	$(COMPLETION_DIR)/file_adapter.c \
	$(COMPLETION_DIR)/file_windows.c \
	$(COMPLETION_DIR)/bridge.c \
	$(COMPLETION_DIR)/windows_iocp.c \
	src/backend/windows_runtime.c
WINDOWS_CFLAGS := -std=c11 -O2 -g -Wall -Wextra -Werror -Wpedantic \
	-I$(COMPLETION_DIR) -Isrc/backend
# Winsock is the one facility this target does not get from the import
# libraries a link takes by default, so a link that carries the TCP routes
# names it (`src/bin/whitefootc.rs`, TARGET_LINK_LIBRARIES, says the same for
# the driver's own links).
WINDOWS_LINK_LIBRARIES := -lws2_32 -lshell32

# Native Windows CI and the optional cross-build aid use identical inputs.
# These are build targets, not cached test verdicts. Unchanged runtime objects
# are shared; the shutdown observer and fault-injected bridge are explicit
# per-unit variants. Real Windows execution remains a separate requirement.
WINDOWS_OBJECT_DIR := $(COMPLETION_TMP)/objects/windows
WINDOWS_CONFIG := $(WINDOWS_OBJECT_DIR)/configuration.txt
WINDOWS_OBJECTS := $(patsubst src/backend/%.c,$(WINDOWS_OBJECT_DIR)/%.o,$(WINDOWS_LINK_SOURCES))
WINDOWS_NATIVE_SOURCES := $(SCHED_DIR)/core.c $(SCHED_DIR)/prim_windows.c \
    $(SCHED_DIR)/entry.c $(COMPLETION_DIR)/runtime.c $(COMPLETION_DIR)/wait_windows.c \
    $(COMPLETION_DIR)/windows_iocp.c src/backend/windows_runtime.c
WINDOWS_NATIVE_OBJECTS := $(patsubst src/backend/%.c,$(WINDOWS_OBJECT_DIR)/%.o,$(WINDOWS_NATIVE_SOURCES))

$(WINDOWS_CONFIG): completion-configuration-force
	@mkdir -p "$(dir $@)"
	@{ printf '%s\n' "$(CURDIR)" "$(WINDOWS_CC)" "$(WINDOWS_LLVM_CC)" "$(WINDOWS_CFLAGS)" "$(WINDOWS_LINK_LIBRARIES)"; $(WINDOWS_CC) --version; $(WINDOWS_LLVM_CC) --version; } > "$@.new"
	@cmp -s "$@.new" "$@" || mv -f "$@.new" "$@"
	@rm -f "$@.new"

$(WINDOWS_OBJECT_DIR)/%.o: src/backend/%.c $(COMPLETION_HEADERS) Makefile $(WINDOWS_CONFIG)
	@mkdir -p "$(dir $@)"
	$(WINDOWS_CC) $(WINDOWS_CFLAGS) -c "$<" -o "$@"

$(WINDOWS_OBJECT_DIR)/%_ir.o: src/backend/%.ll Makefile $(WINDOWS_CONFIG)
	@mkdir -p "$(dir $@)"
	$(WINDOWS_LLVM_CC) $(filter-out -std=c11 -D%,$(WINDOWS_CFLAGS)) -Wno-override-module -Wno-unused-command-line-argument -x ir -c "$<" -o "$@"

$(WINDOWS_OBJECT_DIR)/file-windows-observed.o: $(COMPLETION_DIR)/file_windows.c $(COMPLETION_HEADERS) Makefile $(WINDOWS_CONFIG)
	$(WINDOWS_CC) $(WINDOWS_CFLAGS) -DWF_COMPLETION_SHUTDOWN=wf_ordinary_test_shutdown -c "$<" -o "$@"

$(WINDOWS_OBJECT_DIR)/ordinary_values_probe.o: src/backend/ordinary_values_probe.c $(COMPLETION_HEADERS) Makefile $(WINDOWS_CONFIG)
	$(WINDOWS_CC) $(WINDOWS_CFLAGS) -DWF_COMPLETION_SHUTDOWN=wf_ordinary_test_shutdown -c "$<" -o "$@"

$(WINDOWS_OBJECT_DIR)/bridge-init-fault.o: $(COMPLETION_DIR)/bridge.c $(COMPLETION_DIR)/windows_bridge_init_fail_stop_probe.c $(COMPLETION_HEADERS) Makefile $(WINDOWS_CONFIG)
	$(WINDOWS_CC) $(WINDOWS_CFLAGS) -DWF_WINDOWS_BRIDGE_INIT_PROBE_DECLARATIONS=1 \
		-include $(COMPLETION_DIR)/windows_bridge_init_fail_stop_probe.c -c "$<" -o "$@"

$(COMPLETION_TMP)/windows-default-route-probe.exe: $(WINDOWS_OBJECTS) $(WINDOWS_OBJECT_DIR)/completion/bridge_default_probe.o
	$(WINDOWS_CC) $(WINDOWS_CFLAGS) $^ -o "$@" $(WINDOWS_LINK_LIBRARIES)

$(COMPLETION_TMP)/windows-adapter-probe.exe: $(WINDOWS_NATIVE_OBJECTS) $(WINDOWS_OBJECT_DIR)/completion/native_adapter_probe.o
	$(WINDOWS_CC) $(WINDOWS_CFLAGS) $^ -o "$@" $(WINDOWS_LINK_LIBRARIES)

$(COMPLETION_TMP)/windows-ordinary-values-probe.exe: $(filter-out $(WINDOWS_OBJECT_DIR)/completion/file_windows.o,$(WINDOWS_OBJECTS)) $(WINDOWS_OBJECT_DIR)/file-windows-observed.o $(WINDOWS_OBJECT_DIR)/ordinary_values.o $(WINDOWS_OBJECT_DIR)/ordinary_values_probe.o $(WINDOWS_OBJECT_DIR)/ordinary_values_ir.o $(WINDOWS_OBJECT_DIR)/ordinary_values_probe_ir.o
	$(WINDOWS_CC) $(WINDOWS_CFLAGS) $^ -o "$@" $(WINDOWS_LINK_LIBRARIES)

$(COMPLETION_TMP)/windows-bridge-init-fail-stop-probe.exe: $(filter-out $(WINDOWS_OBJECT_DIR)/completion/bridge.o,$(WINDOWS_OBJECTS)) $(WINDOWS_OBJECT_DIR)/bridge-init-fault.o $(WINDOWS_OBJECT_DIR)/completion/windows_bridge_init_fail_stop_probe.o
	$(WINDOWS_CC) $(WINDOWS_CFLAGS) $^ -o "$@" $(WINDOWS_LINK_LIBRARIES)

# Scheduler probes include core.c to expose its test hooks; their ordinary
# entry/host leaf objects are still compatible with the runtime images above.
$(WINDOWS_OBJECT_DIR)/sched/smoke.o: $(SCHED_DIR)/smoke.c $(SCHED_DIR)/core.c $(COMPLETION_HEADERS) Makefile $(WINDOWS_CONFIG)
	@mkdir -p "$(dir $@)"
	$(WINDOWS_CC) $(WINDOWS_CFLAGS) -DWF_SCHED_TEST -c "$<" -o "$@"

$(WINDOWS_OBJECT_DIR)/sched/deque-probe.o: $(SCHED_DIR)/deque_probe.c $(SCHED_DIR)/core.c $(COMPLETION_HEADERS) Makefile $(WINDOWS_CONFIG)
	@mkdir -p "$(dir $@)"
	$(WINDOWS_CC) $(WINDOWS_CFLAGS) -DWF_SCHED_LANE_SLOTS=8u -DWF_SCHED_STATS=1 -c "$<" -o "$@"

$(WINDOWS_OBJECT_DIR)/sched/deque-probe-no-stats.o: $(SCHED_DIR)/deque_probe.c $(SCHED_DIR)/core.c $(COMPLETION_HEADERS) Makefile $(WINDOWS_CONFIG)
	@mkdir -p "$(dir $@)"
	$(WINDOWS_CC) $(WINDOWS_CFLAGS) -DWF_SCHED_LANE_SLOTS=8u -DWF_SCHED_STATS=0 -c "$<" -o "$@"

$(WINDOWS_OBJECT_DIR)/sched/cpu_levels_probe.o: $(SCHED_DIR)/core.c $(SCHED_POLICY_INPUTS)
WINDOWS_SCHED_SUPPORT := $(WINDOWS_OBJECT_DIR)/sched/entry.o $(WINDOWS_OBJECT_DIR)/sched/prim_windows.o
$(WINDOWS_OBJECT_DIR)/sched/entry-deque.o: $(SCHED_DIR)/entry.c $(COMPLETION_HEADERS) Makefile $(WINDOWS_CONFIG)
	@mkdir -p "$(dir $@)"
	$(WINDOWS_CC) $(WINDOWS_CFLAGS) -DWF_SCHED_LANE_SLOTS=8u -DWF_SCHED_STATS=1 -c "$<" -o "$@"
$(WINDOWS_OBJECT_DIR)/sched/entry-deque-no-stats.o: $(SCHED_DIR)/entry.c $(COMPLETION_HEADERS) Makefile $(WINDOWS_CONFIG)
	@mkdir -p "$(dir $@)"
	$(WINDOWS_CC) $(WINDOWS_CFLAGS) -DWF_SCHED_LANE_SLOTS=8u -DWF_SCHED_STATS=0 -c "$<" -o "$@"
$(COMPLETION_TMP)/windows-sched-smoke.exe: $(WINDOWS_SCHED_SUPPORT) $(WINDOWS_OBJECT_DIR)/sched/smoke.o
	$(WINDOWS_CC) $(WINDOWS_CFLAGS) $^ -o "$@" $(WINDOWS_LINK_LIBRARIES)
$(COMPLETION_TMP)/windows-sched-deque.exe: $(WINDOWS_OBJECT_DIR)/sched/prim_windows.o $(WINDOWS_OBJECT_DIR)/sched/entry-deque.o $(WINDOWS_OBJECT_DIR)/sched/deque-probe.o
	$(WINDOWS_CC) $(WINDOWS_CFLAGS) $^ -o "$@" $(WINDOWS_LINK_LIBRARIES)
$(COMPLETION_TMP)/windows-sched-deque-no-stats.exe: $(WINDOWS_OBJECT_DIR)/sched/prim_windows.o $(WINDOWS_OBJECT_DIR)/sched/entry-deque-no-stats.o $(WINDOWS_OBJECT_DIR)/sched/deque-probe-no-stats.o
	$(WINDOWS_CC) $(WINDOWS_CFLAGS) $^ -o "$@" $(WINDOWS_LINK_LIBRARIES)
$(COMPLETION_TMP)/windows-sched-cpu-levels.exe: $(WINDOWS_SCHED_SUPPORT) $(WINDOWS_OBJECT_DIR)/sched/cpu_levels_probe.o
	$(WINDOWS_CC) $(WINDOWS_CFLAGS) $^ -o "$@" $(WINDOWS_LINK_LIBRARIES)

.SECONDARY: $(WINDOWS_OBJECTS) $(WINDOWS_NATIVE_OBJECTS)

windows-runtime-sources:
	@printf '%s\n' $(addprefix compiler/,$(WINDOWS_LINK_SOURCES))

completion-windows-route-build: $(COMPLETION_TMP)/windows-default-route-probe.exe $(COMPLETION_TMP)/windows-adapter-probe.exe
completion-windows-cross: completion-windows-route-build \
    $(COMPLETION_TMP)/windows-ordinary-values-probe.exe \
    $(COMPLETION_TMP)/windows-bridge-init-fail-stop-probe.exe \
    $(WINDOWS_OBJECT_DIR)/wf_floor_windows.o \
    $(COMPLETION_TMP)/windows-sched-smoke.exe $(COMPLETION_TMP)/windows-sched-cpu-levels.exe \
    $(COMPLETION_TMP)/windows-sched-deque.exe $(COMPLETION_TMP)/windows-sched-deque-no-stats.exe
	@echo "Windows strict native construction: PASS (execution is a separate check)"

# Runs what the cross build produced, under wine, when a wine is installed.
# A missing wine is not a failure of this target: it is a development aid, not
# evidence, and the evidence is the native job.
completion-windows-wine:
	@if [ ! -x "$(WINE)" ]; then \
		echo "completion Windows wine run: SKIP ($(WINE) is not installed)"; \
		exit 0; \
	fi; \
	$(MAKE) --no-print-directory completion-windows-route-build || exit $$?; \
	mkdir -p "$(WINEPREFIX)"; \
	set -e; \
	echo "== native adapter probe =="; \
	WINEDEBUG=-all WINEPREFIX="$(WINEPREFIX)" "$(WINE)" \
		"$(COMPLETION_TMP)/windows-adapter-probe.exe" \
		"$(COMPLETION_TMP)/windows-adapter-probe.data"; \
	echo "== default route, native ring =="; \
	WINEDEBUG=-all WINEPREFIX="$(WINEPREFIX)" "$(WINE)" \
		"$(COMPLETION_TMP)/windows-default-route-probe.exe" \
		"$(COMPLETION_TMP)" > "$(COMPLETION_TMP)/windows-ring.txt" 2>&1 \
		|| { status=$$?; cat "$(COMPLETION_TMP)/windows-ring.txt"; exit $$status; }; \
	cat "$(COMPLETION_TMP)/windows-ring.txt"; \
	grep -q 'route=native-ring' "$(COMPLETION_TMP)/windows-ring.txt"; \
	echo "== default route, bounded adapter =="; \
	WF_IO_NO_NATIVE_RING=1 WINEDEBUG=-all WINEPREFIX="$(WINEPREFIX)" "$(WINE)" \
		"$(COMPLETION_TMP)/windows-default-route-probe.exe" \
		"$(COMPLETION_TMP)" > "$(COMPLETION_TMP)/windows-adapter.txt" 2>&1 \
		|| { status=$$?; cat "$(COMPLETION_TMP)/windows-adapter.txt"; exit $$status; }; \
	cat "$(COMPLETION_TMP)/windows-adapter.txt"; \
	grep -q 'route=posix-adapter' "$(COMPLETION_TMP)/windows-adapter.txt"; \
	echo "== the TCP round trip, on both routes =="; \
	if ! grep -q 'tcp-port=[1-9]' "$(COMPLETION_TMP)/windows-ring.txt" \
		|| ! grep -q 'tcp-port=[1-9]' "$(COMPLETION_TMP)/windows-adapter.txt"; then \
		echo "wine ran no loopback round trip on one of the two routes" >&2; \
		exit 1; \
	fi; \
	if grep -q 'tcp-ring=[1-9]' "$(COMPLETION_TMP)/windows-ring.txt"; then \
		echo "wine's Winsock carried the connect, the receive and the send on the completion port"; \
	else \
		echo "wine's Winsock ran the loopback round trip but carried none of it on the completion port; a real Windows host is what judges that (.github/workflows/io-hosts.yml, completion-windows)"; \
	fi; \
	if ! grep -q 'tcp-ring=0' "$(COMPLETION_TMP)/windows-adapter.txt"; then \
		echo "the refused-ring run still reached the completion port" >&2; \
		exit 1; \
	fi; \
	echo "the accept is the shared file adapter's on every Windows host, not only under wine: an AcceptEx address pair is 88 bytes and the record has none to give (completion/windows_iocp.c, wf_windows_iocp_carries), so wine's own AcceptEx is not in question here"; \
	echo "completion Windows wine run: PASS (wine is not Windows)"


# Managed-stack park-on-miss measurements are retired from this runtime.
# Their dated evidence and reproduction revision remain in the investigation.
.PHONY: check _check static _static build test-build test-build-unit test-build-corpus format lint test _test test-unit test-corpus \
	docs completion-test completion-native-test concurrent-map-test shared-object-test \
	completion-core-read-test \
	completion-tsan \
	completion-sanitize \
	completion-core-read-sanitize completion-default-route-test ordinary-values-test \
	completion-default-route-sanitize completion-default-route-tsan sched-smoke \
	sched-deque-test sched-deque-tsan sched-policy-test sched-split-policy-cases floor-test sched-wake-measure completion-windows-cross completion-windows-route-build windows-runtime-sources completion-windows-wine
