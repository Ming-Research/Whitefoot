# Program-level measurement of ordinary linked I/O functions and native controls.
#
# Three lines per workload, all publishing the same bytes:
#   N  the best hand-written native shape (thread pool, io_uring)
#   S  the Whitefoot program built with --no-overlap
#   C  the same Whitefoot program built with the default lowering
#
# These retained labels do not assert that the two WF modules differ. C2's
# ordinary shared-factory calls serialize, and PAR-3 staging is deleted; the
# measured many-files modules are identical in both modes. See C2-RESULTS.md.
#
# This is a measurement bundle, not a gate. Its protocols are deliberately not
# reachable from the repository's `make check`: they generate a large tree,
# run for minutes, and report timings that no build should depend on. The one
# source target the gate runs is `programs-check`, which compiles every program
# here with the current compiler, so a language change cannot leave one behind.
# On Linux the gate also runs `uring-check`, a deterministic correctness fixture
# for the native reference. It needs Linux headers but no live io_uring or
# network; neither target measures performance.
#
#   make -C research/experiments/io-completion-bench verify   correctness only
#   make -C research/experiments/io-completion-bench bench     the macOS table
#   make -C research/experiments/io-completion-bench bench-pipe
#   make -C research/experiments/io-completion-bench linux     the Linux table
#
# Generated trees, binaries, and raw output stay in the scratch root. Only the
# summarized tables and their owned sample artifacts are retained here and in
# research/investigations/io-model/RESULTS.md (historical completion results).

ROOT := $(abspath ../../..)
WHITEFOOT_SCRATCH_ROOT ?= $(patsubst %/,%,$(if $(TMPDIR),$(TMPDIR),/tmp))/whitefoot
WORK := $(WHITEFOOT_SCRATCH_ROOT)/whitefoot-io-completion-bench
BUILD := $(WORK)/build
TREE := $(WORK)/tree
BIGTREE := $(WORK)/bigtree
CARGO_TARGET := $(WORK)/cargo-target
LINUX_TARGET := $(WORK)/linux-target
CLANG := /usr/bin/clang
CFLAGS := -std=c11 -O2 -Wall -Wextra -Werror
WFC := $(ROOT)/compiler/target/gate/whitefootc

# The workload's shape. FILES x up to MAX_KIB, read through one positioned
# read each. The checksum below is a property of these two numbers alone, so
# changing either means recomputing it with `make expected`.
FILES ?= 8192
MAX_KIB ?= 16
EXPECTED ?= 17098009301725298919 00000000000071024640

# Timing protocol: WARMUP unrecorded runs, then ROUNDS recorded ones.
ROUNDS ?= 7
WARMUP ?= 2

# The pipe workload. Chunks are larger than the host pipe buffer so a write
# genuinely waits for its consumer, and each consumer sleeps DELAY_US per read
# so the consumer, not the producer's CPU, sets the pace.
PIPE_ROUNDS ?= 64
PIPE_DELAY_US ?= 1000
PIPE_BYTES ?= 16777216


# The read-heavy workload's shape: READ_FILES files of READ_KIB KiB each,
# opened once and then read at READS_* positioned offsets. Its tree is
# separate from the many-files tree above because the two answer different
# questions and neither should perturb the other's page-cache state.
READ_FILES ?= 8
READ_KIB ?= 65536
# Both windows do the same number of reads, so the only thing that differs
# between the two tables is the size of one transfer. An uncached read costs
# about the same at either window on this host -- it is a device round trip,
# not a bandwidth question -- so equal read counts also keep the two tables
# roughly equal in wall time.
READS_64K ?= 32768
READS_4K ?= 32768
EXPECTED_READ_64K ?= 09425560703784339892 00000000002147483648
EXPECTED_READ_4K ?= 18028327385673861873 00000000000134217728
READ_NAME_ARGS := f00000.dat f00001.dat f00002.dat f00003.dat \
	f00004.dat f00005.dat f00006.dat f00007.dat
READ_NAME_PLAN_ARGS := f00000.dat\tf00001.dat\tf00002.dat\tf00003.dat\tf00004.dat\tf00005.dat\tf00006.dat\tf00007.dat

# The read table is measured to medians of fifteen on macOS, because the host
# is shared with an endpoint-security stack whose interference is bursty, and
# to medians of nine on Linux.
READ_ROUNDS ?= 15
READ_LINUX_ROUNDS ?= 9

# The residency check that gives the words "uncached" and "warm" their
# meaning. Immediately before and immediately after each table, READ_PROBES
# positioned reads are timed in every file through a descriptor that does not
# populate the cache, and the table is refused unless all but
# READ_TOLERANCE_PERCENT of those reads fell on the side of the threshold the
# label claims. On this host a cached 64 KiB read costs 6 to 20 us and one the
# device answers costs about 134 us, so 40 us separates the two populations
# with room on both sides. read_baseline.c explains why the rule bounds a
# share rather than testing every file.
READ_PROBES ?= 16
READ_THRESHOLD_US ?= 40
READ_TOLERANCE_PERCENT ?= 10

# How long a freshly generated tree is left alone before it may be measured.
# Writing half a gigabyte makes the host's malware scanner walk the new files
# once, which holds a few of them resident for a minute or two; `read-settle`
# waits that out rather than measuring through it. See the comment above
# `read-settle` for the observation.
READ_SETTLE_ATTEMPTS ?= 24
READ_SETTLE_SECONDS ?= 15
DOCKER_IMAGE ?= wf-io-bench:linux

.PHONY: programs-check build prepare compiler gen verify bench bench-pipe linux linux-image expected clean chain \
	read-build read-gen read-uncache read-settle read-warm read-verify expected-read bench-read \
	linux-read net-tools uring-check net-verify linux-net

build: prepare compiler $(BUILD)/gen $(BUILD)/baseline $(BUILD)/runner \
	$(BUILD)/pipe_producer $(BUILD)/pipe_harness \
	$(BUILD)/many_files_wide $(BUILD)/many_files_wide_seq \
	$(BUILD)/many_files_wide8 $(BUILD)/many_files_wide8_seq \
	$(BUILD)/many_files_narrow $(BUILD)/many_files_narrow_seq \
	$(BUILD)/many_files_loop $(BUILD)/many_files_loop_seq \
	$(BUILD)/pipe_relay $(BUILD)/pipe_relay_seq

read-build: prepare compiler $(BUILD)/gen $(BUILD)/read_baseline $(BUILD)/runner \
	$(BUILD)/read_heavy_narrow $(BUILD)/read_heavy_narrow_seq \
	$(BUILD)/read_heavy_wide8 $(BUILD)/read_heavy_wide8_seq \
	$(BUILD)/read_heavy_narrow_4k $(BUILD)/read_heavy_narrow_4k_seq \
	$(BUILD)/read_heavy_wide8_4k $(BUILD)/read_heavy_wide8_4k_seq

prepare:
	@mkdir -p $(BUILD) $(CARGO_TARGET)

compiler:
	cd $(ROOT)/compiler && env CARGO_TARGET_DIR=$(ROOT)/compiler/target \
		cargo build --profile gate --bin whitefootc --locked --offline

# Refresh Cargo before comparing image inputs, including under parallel Make.
# A no-op Cargo build leaves this real file unchanged. Depending directly on
# the phony target rebuilt every WF image again for `verify` and `bench`.
$(WFC): compiler

$(BUILD)/gen: gen.c workload.h | prepare
	$(CLANG) $(CFLAGS) $< -o $@

$(BUILD)/baseline: baseline.c workload.h uring_baseline.h | prepare
	$(CLANG) $(CFLAGS) -pthread $< -o $@

$(BUILD)/runner: runner.c | prepare
	$(CLANG) $(CFLAGS) $< -o $@

$(BUILD)/pipe_producer: pipe_producer.c | prepare
	$(CLANG) $(CFLAGS) -pthread $< -o $@

$(BUILD)/pipe_harness: pipe_harness.c | prepare
	$(CLANG) $(CFLAGS) -pthread $< -o $@

# The gate compiles every WF program and executes the bounded native caller
# correctness checks. Full workload protocols remain below; independent WF
# compilations use JOBS (the guarded gate supplies two by default).
PROGRAMS := $(wildcard programs/*.wf)
CHECKED := $(PROGRAMS:programs/%.wf=$(BUILD)/checked_%)
JOBS ?= $(shell nproc 2>/dev/null || sysctl -n hw.ncpu)

programs-check: compiler ordinary-check $(if $(filter Linux,$(shell uname -s)),uring-check) | prepare
	$(MAKE) -j$(JOBS) $(CHECKED)

$(BUILD)/checked_%: programs/%.wf $(WFC) Makefile | prepare
	$(WFC) -o $@ $<

# The C and S labels retain the same source under two lowering configurations.
# An identical emitted module makes this pair a measurement-noise control.
$(BUILD)/%: programs/%.wf $(WFC) Makefile | prepare
	$(WFC) -o $@ $<

$(BUILD)/many_files_wide_seq: programs/many_files_wide.wf $(WFC) Makefile | prepare
	$(WFC) --no-overlap -o $@ $<

$(BUILD)/many_files_wide8_seq: programs/many_files_wide8.wf $(WFC) Makefile | prepare
	$(WFC) --no-overlap -o $@ $<

$(BUILD)/many_files_narrow_seq: programs/many_files_narrow.wf $(WFC) Makefile | prepare
	$(WFC) --no-overlap -o $@ $<

$(BUILD)/many_files_loop_seq: programs/many_files_loop.wf $(WFC) Makefile | prepare
	$(WFC) --no-overlap -o $@ $<

$(BUILD)/pipe_relay_seq: programs/pipe_relay.wf $(WFC) Makefile | prepare
	$(WFC) --no-overlap -o $@ $<


$(BUILD)/read_baseline: read_baseline.c workload.h uring_baseline.h | prepare
	$(CLANG) $(CFLAGS) -pthread $< -o $@

$(BUILD)/read_heavy_narrow_seq: programs/read_heavy_narrow.wf $(WFC) Makefile | prepare
	$(WFC) --no-overlap -o $@ $<

$(BUILD)/read_heavy_wide8_seq: programs/read_heavy_wide8.wf $(WFC) Makefile | prepare
	$(WFC) --no-overlap -o $@ $<

$(BUILD)/read_heavy_narrow_4k_seq: programs/read_heavy_narrow_4k.wf $(WFC) Makefile | prepare
	$(WFC) --no-overlap -o $@ $<

$(BUILD)/read_heavy_wide8_4k_seq: programs/read_heavy_wide8_4k.wf $(WFC) Makefile | prepare
	$(WFC) --no-overlap -o $@ $<

gen: $(BUILD)/gen
	@rm -rf $(TREE)
	$(BUILD)/gen $(TREE) $(FILES) $(MAX_KIB)
	@echo "generated $$(ls $(TREE) | wc -l) files in $(TREE)"

# Every line must publish the same bytes before any of them reports a time.
verify: build gen
	@cd $(TREE) && for line in \
		"$(BUILD)/baseline $(TREE) direct $(FILES)" \
		"$(BUILD)/many_files_narrow" \
		"$(BUILD)/many_files_narrow_seq" \
		"$(BUILD)/many_files_loop" \
		"$(BUILD)/many_files_loop_seq" \
		"$(BUILD)/many_files_wide" \
		"$(BUILD)/many_files_wide_seq" \
		"$(BUILD)/many_files_wide8" \
		"$(BUILD)/many_files_wide8_seq"; do \
		got=$$($$line); \
		if [ "$$got" != "$(EXPECTED)" ]; then \
			echo "MISMATCH: $$line published [$$got], expected [$(EXPECTED)]"; \
			exit 1; \
		fi; \
	done
	@echo "all lines publish $(EXPECTED)"

# Prints the checksum the current tree actually folds, for when FILES or
# MAX_KIB move.
expected: $(BUILD)/baseline gen
	@$(BUILD)/baseline $(TREE) direct $(FILES)

bench: verify
	@{ \
	printf 'N.direct\t\t$(BUILD)/baseline\t$(TREE)\tdirect\t$(FILES)\n'; \
	for t in 1 2 4 6 8 10; do \
		printf 'N.pool%s\t\t$(BUILD)/baseline\t$(TREE)\tpool\t$(FILES)\t%s\n' $$t $$t; \
	done; \
	printf 'S.narrow\t\t$(BUILD)/many_files_narrow_seq\n'; \
	printf 'S.loop\t\t$(BUILD)/many_files_loop_seq\n'; \
	printf 'S.wide\t\t$(BUILD)/many_files_wide_seq\n'; \
	printf 'S.wide8\t\t$(BUILD)/many_files_wide8_seq\n'; \
	printf 'C.narrow.default\t\t$(BUILD)/many_files_narrow\n'; \
	printf 'C.loop.default\t\t$(BUILD)/many_files_loop\n'; \
	printf 'C.wide.default\t\t$(BUILD)/many_files_wide\n'; \
	printf 'C.wide8.default\t\t$(BUILD)/many_files_wide8\n'; \
	for w in 0 1 2 4; do for h in 0 1 2 4; do \
		printf 'C.wide.w%s.h%s\tWF_WORKERS=%s,WF_IO_HELPERS=%s\t$(BUILD)/many_files_wide\n' \
			$$w $$h $$w $$h; \
	done; done; \
	for h in 0 1 2 4 6; do \
		printf 'C.wide8.h%s\tWF_IO_HELPERS=%s\t$(BUILD)/many_files_wide8\n' $$h $$h; \
	done; \
	} > $(WORK)/plan.txt
	cd $(TREE) && $(BUILD)/runner $(WORK)/plan.txt $(ROUNDS) $(WARMUP) "$(EXPECTED)"

bench-pipe: build
	@echo "pipe relay: $(PIPE_ROUNDS) rounds, consumer sleeps $(PIPE_DELAY_US)us per read"
	@for line in \
		"N.seq:$(BUILD)/pipe_producer seq $(PIPE_ROUNDS)" \
		"N.threads:$(BUILD)/pipe_producer threads $(PIPE_ROUNDS)" \
		"S.relay:$(BUILD)/pipe_relay_seq" \
		"C.relay:$(BUILD)/pipe_relay"; do \
		label=$${line%%:*}; command=$${line#*:}; \
		for warm in $$(seq 1 $(WARMUP)); do \
			$(BUILD)/pipe_harness $(PIPE_DELAY_US) $(PIPE_BYTES) $$command >/dev/null || exit 1; \
		done; \
		printf '%-12s ' $$label; \
		for round in $$(seq 1 $(ROUNDS)); do \
			$(BUILD)/pipe_harness $(PIPE_DELAY_US) $(PIPE_BYTES) $$command | cut -d' ' -f1; \
		done | sort -n | awk '{a[NR]=$$1} END {printf "median %.2f ms (min %.2f, max %.2f, n=%d)\n", (NR%2 ? a[(NR+1)/2] : (a[NR/2]+a[NR/2+1])/2), a[1], a[NR], NR}'; \
	done

linux-image: linux.Dockerfile
	docker build -f linux.Dockerfile -t $(DOCKER_IMAGE) .

# io_uring is not in Docker's default seccomp allowlist, so both the native
# baseline and the Whitefoot Linux adapter fall back without this flag. The
# whole pipeline runs inside one container because the generated tree must sit
# on a container-local filesystem: measuring a bind mount would measure the
# host's file sharing rather than the kernel's I/O path.
linux: linux-image
	@mkdir -p $(LINUX_TARGET)
	docker run --rm --security-opt seccomp=unconfined \
		-v $(ROOT):/work \
		-v $(LINUX_TARGET):/target \
		-e CARGO_TARGET_DIR=/target \
		-e FILES=$(FILES) -e MAX_KIB=$(MAX_KIB) \
		-e ROUNDS=$(ROUNDS) -e WARMUP=$(WARMUP) \
		-e EXPECTED="$(EXPECTED)" \
		$(DOCKER_IMAGE) \
		sh /work/research/experiments/io-completion-bench/linux-bench.sh


# --- the read-heavy workload --------------------------------------------
#
# The many-files table above is dominated by openat, which on the macOS host
# costs 116 us because of an endpoint-security stack: it measures the host's
# open path far more than it measures the completion framework. This table
# opens eight large files once, outside any loop, and then does tens of
# thousands of positioned reads into them, so what the time is made of is
# reads.
#
# The opens are inside the timed region — the runner times whole processes —
# but there are exactly eight of them in every line, N, S, and C alike, and at
# 116 us each that is 0.93 ms against a table whose fastest line is over a
# hundred. A constant that every line pays identically cannot move a ratio.

read-gen: $(BUILD)/gen
	@rm -rf $(BIGTREE)
	$(BUILD)/gen $(BIGTREE) $(READ_FILES) $(READ_KIB) fixed
	@echo "generated $(READ_FILES) x $(READ_KIB) KiB in $(BIGTREE)"

# Every line must publish the same bytes before any of them reports a time,
# and must publish them with the cache policy off and on, because a policy
# that changed a byte would not be a policy.
read-verify: read-build read-gen
	@cd $(BIGTREE) && for nocache in 0 1; do \
		for line in \
			"$(BUILD)/read_baseline $(BIGTREE) direct $(READS_64K) 65536:$(EXPECTED_READ_64K)" \
			"$(BUILD)/read_heavy_narrow:$(EXPECTED_READ_64K)" \
			"$(BUILD)/read_heavy_narrow_seq:$(EXPECTED_READ_64K)" \
			"$(BUILD)/read_heavy_wide8:$(EXPECTED_READ_64K)" \
			"$(BUILD)/read_heavy_wide8_seq:$(EXPECTED_READ_64K)" \
			"$(BUILD)/read_baseline $(BIGTREE) direct $(READS_4K) 4096:$(EXPECTED_READ_4K)" \
			"$(BUILD)/read_heavy_narrow_4k:$(EXPECTED_READ_4K)" \
			"$(BUILD)/read_heavy_narrow_4k_seq:$(EXPECTED_READ_4K)" \
			"$(BUILD)/read_heavy_wide8_4k $(READ_NAME_ARGS):$(EXPECTED_READ_4K)" \
			"$(BUILD)/read_heavy_wide8_4k_seq $(READ_NAME_ARGS):$(EXPECTED_READ_4K)"; do \
			command=$${line%%:*}; want=$${line#*:}; \
			if [ "$$nocache" = 1 ]; then got=$$(WF_IO_NOCACHE=1 $$command); \
			else got=$$($$command); fi; \
			if [ "$$got" != "$$want" ]; then \
				echo "MISMATCH (WF_IO_NOCACHE=$$nocache): $$command published [$$got], expected [$$want]"; \
				exit 1; \
			fi; \
		done; \
	done
	@echo "every read-heavy line publishes the same bytes with the cache policy off and on"

# Prints the checksums the current tree actually folds, for when the shape
# constants above move.
expected-read: $(BUILD)/read_baseline read-gen
	@$(BUILD)/read_baseline $(BIGTREE) direct $(READS_64K) 65536
	@$(BUILD)/read_baseline $(BIGTREE) direct $(READS_4K) 4096

$(WORK)/read-plan-64k.txt: Makefile | prepare
	@{ \
	printf 'N.direct\t\t$(BUILD)/read_baseline\t$(BIGTREE)\tdirect\t$(READS_64K)\t65536\n'; \
	for t in 2 4 6 8; do \
		printf 'N.pool%s\t\t$(BUILD)/read_baseline\t$(BIGTREE)\tpool\t$(READS_64K)\t65536\t%s\n' $$t $$t; \
	done; \
	printf 'S.narrow\t\t$(BUILD)/read_heavy_narrow_seq\n'; \
	printf 'S.wide8\t\t$(BUILD)/read_heavy_wide8_seq\n'; \
	printf 'C.narrow.default\t\t$(BUILD)/read_heavy_narrow\n'; \
	printf 'C.wide8.default\t\t$(BUILD)/read_heavy_wide8\n'; \
	for h in 0 1 4; do \
		printf 'C.narrow.h%s\tWF_IO_HELPERS=%s\t$(BUILD)/read_heavy_narrow\n' $$h $$h; \
	done; \
	for h in 0 1 2 4 8; do \
		printf 'C.wide8.h%s\tWF_IO_HELPERS=%s\t$(BUILD)/read_heavy_wide8\n' $$h $$h; \
	done; \
	} > $@

$(WORK)/read-plan-4k.txt: Makefile | prepare
	@{ \
	printf 'N.direct\t\t$(BUILD)/read_baseline\t$(BIGTREE)\tdirect\t$(READS_4K)\t4096\n'; \
	for t in 2 4 6 8; do \
		printf 'N.pool%s\t\t$(BUILD)/read_baseline\t$(BIGTREE)\tpool\t$(READS_4K)\t4096\t%s\n' $$t $$t; \
	done; \
	printf 'S.narrow\t\t$(BUILD)/read_heavy_narrow_4k_seq\n'; \
	printf 'S.wide8\t\t$(BUILD)/read_heavy_wide8_4k_seq\t$(READ_NAME_PLAN_ARGS)\n'; \
	printf 'C.narrow.default\t\t$(BUILD)/read_heavy_narrow_4k\n'; \
	printf 'C.wide8.default\t\t$(BUILD)/read_heavy_wide8_4k\t$(READ_NAME_PLAN_ARGS)\n'; \
	for h in 0 1 4; do \
		printf 'C.narrow.h%s\tWF_IO_HELPERS=%s\t$(BUILD)/read_heavy_narrow_4k\n' $$h $$h; \
	done; \
	for h in 0 1 2 4 8; do \
		printf 'C.wide8.h%s\tWF_IO_HELPERS=%s\t$(BUILD)/read_heavy_wide8_4k\t$(READ_NAME_PLAN_ARGS)\n' $$h $$h; \
	done; \
	} > $@

# Ordering, and the check that makes the word "uncached" mean something.
#
# F_NOCACHE stops a read populating the page cache; it does not evict a page
# that is already resident. A table therefore cannot make itself uncached by
# asking for the policy: it has to be handed a tree that is not resident.
# `read-uncache` produces one — the generator writes through a non-populating
# descriptor and flushes each file, so nothing it wrote is left in the cache.
#
# That is checked rather than assumed. `read_baseline probe-uncached` times
# READ_PROBES positioned reads in every file immediately before the table and
# again immediately after it, and make stops unless all but
# READ_TOLERANCE_PERCENT of them cost more than READ_THRESHOLD_US; the warm
# tables are checked the same way in the other direction. The pair of probes
# is what rules out both mistakes: the tree being resident when the table
# starts, and something making it resident while the table runs.
#
# Batch 0092's first attempt had no such check. It took its "uncached" numbers
# straight after generating the tree, when every page was still resident from
# the writes, and published a page-cache table under the uncached label;
# That cost is recorded in this directory's README.
#
# The uncached tables run first, because they are the ones the design question
# turns on and the ones a shared host can spoil; the warm tables follow, after
# `read-warm` reads the whole tree back in. A full sequential pass, not a rerun
# of the workload: 32,768 pseudo-random reads would leave about two per cent of
# the blocks untouched.
read-uncache: $(BUILD)/gen
	@rm -rf $(BIGTREE)
	@$(BUILD)/gen $(BIGTREE) $(READ_FILES) $(READ_KIB) fixed
	@sync

# The wait that a generation needs before it may be measured.
#
# Writing the tree does not only put bytes on the device: it hands the host a
# fresh half-gigabyte of files, and something on this host reads them. A probe
# run every fifteen seconds on an otherwise idle machine watches residency
# walk from f00000 towards f00007 over a minute or two and then disappear as
# the pages age out -- three files resident at one probe, a different three at
# the next, none a minute later. No benchmark line does it: the same walk
# happens across idle gaps with nothing running, and every line reads through
# the non-populating policy. The host's malware scanner is the obvious
# candidate; `XprotectService` holds about ten per cent of a core throughout.
#
# So the tree is left to settle instead of being measured through the scan.
# `read-settle` runs the same probe the table is gated on, quietly, until it
# passes -- which is a wait for an outside reader to finish, not a softer
# threshold. The gate probes below are unchanged and still fail the table.
read-settle: $(BUILD)/read_baseline
	@attempt=0; \
	while :; do \
		if $(BUILD)/read_baseline $(BIGTREE) probe-uncached $(READ_PROBES) 65536 \
			$(READ_THRESHOLD_US) $(READ_TOLERANCE_PERCENT) > /dev/null 2>&1; then \
			echo "read-settle: the tree is not resident after $$attempt wait(s)"; \
			exit 0; \
		fi; \
		attempt=$$((attempt + 1)); \
		if [ $$attempt -ge $(READ_SETTLE_ATTEMPTS) ]; then \
			echo "read-settle: still resident after $(READ_SETTLE_ATTEMPTS) waits" >&2; \
			$(BUILD)/read_baseline $(BIGTREE) probe-uncached $(READ_PROBES) 65536 \
				$(READ_THRESHOLD_US) $(READ_TOLERANCE_PERCENT); \
			exit 1; \
		fi; \
		sleep $(READ_SETTLE_SECONDS); \
	done

read-warm: $(BUILD)/read_baseline
	@$(BUILD)/read_baseline $(BIGTREE) warm 1 65536

bench-read: read-verify $(WORK)/read-plan-64k.txt $(WORK)/read-plan-4k.txt
	@echo "== read-heavy 64 KiB, uncached (WF_IO_NOCACHE=1) =="
	$(MAKE) read-uncache
	$(MAKE) read-settle
	$(BUILD)/read_baseline $(BIGTREE) probe-uncached $(READ_PROBES) 65536 $(READ_THRESHOLD_US) $(READ_TOLERANCE_PERCENT)
	cd $(BIGTREE) && env WF_IO_NOCACHE=1 $(BUILD)/runner $(WORK)/read-plan-64k.txt \
		$(READ_ROUNDS) $(WARMUP) "$(EXPECTED_READ_64K)"
	$(BUILD)/read_baseline $(BIGTREE) probe-uncached $(READ_PROBES) 65536 $(READ_THRESHOLD_US) $(READ_TOLERANCE_PERCENT)
	@echo "== read-heavy 4 KiB, uncached (WF_IO_NOCACHE=1) =="
	$(MAKE) read-uncache
	$(MAKE) read-settle
	$(BUILD)/read_baseline $(BIGTREE) probe-uncached $(READ_PROBES) 65536 $(READ_THRESHOLD_US) $(READ_TOLERANCE_PERCENT)
	cd $(BIGTREE) && env WF_IO_NOCACHE=1 $(BUILD)/runner $(WORK)/read-plan-4k.txt \
		$(READ_ROUNDS) $(WARMUP) "$(EXPECTED_READ_4K)"
	$(BUILD)/read_baseline $(BIGTREE) probe-uncached $(READ_PROBES) 65536 $(READ_THRESHOLD_US) $(READ_TOLERANCE_PERCENT)
	@echo "== read-heavy 64 KiB, page cache warm =="
	$(MAKE) read-warm
	$(BUILD)/read_baseline $(BIGTREE) probe-warm $(READ_PROBES) 65536 $(READ_THRESHOLD_US) $(READ_TOLERANCE_PERCENT)
	cd $(BIGTREE) && $(BUILD)/runner $(WORK)/read-plan-64k.txt \
		$(READ_ROUNDS) $(WARMUP) "$(EXPECTED_READ_64K)"
	$(BUILD)/read_baseline $(BIGTREE) probe-warm $(READ_PROBES) 65536 $(READ_THRESHOLD_US) $(READ_TOLERANCE_PERCENT)
	@echo "== read-heavy 4 KiB, page cache warm =="
	$(BUILD)/read_baseline $(BIGTREE) probe-warm $(READ_PROBES) 65536 $(READ_THRESHOLD_US) $(READ_TOLERANCE_PERCENT)
	cd $(BIGTREE) && $(BUILD)/runner $(WORK)/read-plan-4k.txt \
		$(READ_ROUNDS) $(WARMUP) "$(EXPECTED_READ_4K)"
	$(BUILD)/read_baseline $(BIGTREE) probe-warm $(READ_PROBES) 65536 $(READ_THRESHOLD_US) $(READ_TOLERANCE_PERCENT)

# The Linux read tables, in the same container the many-files `linux` target
# uses and for the same reason: the tree must sit on a container-local
# filesystem, and io_uring needs the unconfined seccomp profile.
#
# `read-bench.sh` is one protocol for every host that can run it -- this
# container, a hosted Linux runner, a hosted macOS runner -- and differs from
# `bench-read` above in one deliberate way: it labels a table by what the
# residency probe measured instead of refusing to print one the probe did not
# confirm. Its header says why. `bench-read` keeps the refusal, because on
# this machine the two populations the threshold separates are known.
linux-read: linux-image
	@mkdir -p $(LINUX_TARGET)
	docker run --rm --security-opt seccomp=unconfined \
		-v $(ROOT):/work \
		-v $(LINUX_TARGET):/target \
		-e CARGO_TARGET_DIR=/target \
		-e READ_FILES=$(READ_FILES) -e READ_KIB=$(READ_KIB) \
		-e READS_64K=$(READS_64K) -e READS_4K=$(READS_4K) \
		-e ROUNDS=$(READ_LINUX_ROUNDS) -e WARMUP=$(WARMUP) \
		-e EXPECTED_READ_64K="$(EXPECTED_READ_64K)" \
		-e EXPECTED_READ_4K="$(EXPECTED_READ_4K)" \
		$(DOCKER_IMAGE) \
		sh /work/research/experiments/io-completion-bench/read-bench.sh

# Historical four-stage chain: its managed wf_sched_core API has been retired.
# This optional evidence target does not build against the current core.
# Not one of the three lines: it is a C program on raw io_uring, it links the
# scheduler core for the stack-switch shape, and what it reports is the
# dependent stage's in-flight depth beside the wall time.
CHAIN_JOBS ?= 1000
CHAIN_THREADS ?= 8
CHAIN_SLOTS ?= 32
CHAIN_STACKS ?= 72
SCHED_DIR := $(ROOT)/compiler/src/backend/sched

$(BUILD)/chain: chain.c uring_baseline.h workload.h $(SCHED_DIR)/core.c $(SCHED_DIR)/prim_host.c | prepare
	$(CLANG) $(CFLAGS) -Wpedantic -pthread -I. -I$(SCHED_DIR) \
		chain.c $(SCHED_DIR)/core.c $(SCHED_DIR)/prim_host.c -o $@

chain: $(BUILD)/chain $(BUILD)/gen
	@rm -rf $(WORK)/chaintree
	$(BUILD)/gen $(WORK)/chaintree $(CHAIN_JOBS) $(MAX_KIB)
	@for shape in nested compensate switch pipeline; do \
		$(BUILD)/chain $(WORK)/chaintree $$shape $(CHAIN_JOBS) \
			$(CHAIN_THREADS) $(CHAIN_SLOTS) $(CHAIN_STACKS) || exit 1; \
	done

# --- the TCP echo workload ------------------------------------------------
#
# Three servers on one contract -- the io_uring reference, the epoll reference,
# and the Whitefoot program -- measured by one load generator over the same
# loopback. `linux-net-bench.sh` owns the protocol; these targets only build
# the pieces and hand it this repository's paths, so the container, this host
# and the runner all execute the same bytes.
#
#   make net-tools    build the five C tools
#   make uring-check  the io_uring reference's own deterministic traces
#   make net-verify   every server against netload at 4 connections
#   make linux-net    the protocol, here
NET_OUT := $(WORK)/net
NET_ROUNDS ?= 5
NET_WARMUP ?= 1
# The Whitefoot line is the maintained context server, one context per
# connection [PAR-4]; the bundle's earlier echo program is kept as evidence.
WF_ECHO_SOURCE := $(ROOT)/tests/programs/tcp_contexts.wf

net-tools: $(BUILD)/uring_echo $(BUILD)/epoll_echo $(BUILD)/netload $(BUILD)/waiting_echo \
	$(BUILD)/idleload

$(BUILD)/uring_echo: uring_echo.c | prepare
	$(CLANG) $(CFLAGS) -pthread $< -o $@

# The reference's own correctness, separate from any measurement. The check
# program includes `uring_echo.c` and replaces only kernel setup, enter and the
# synchronous send result, so the buffer-loan queue, the receive re-arm and the
# CQE handling under check are the ones the measured binary runs. It therefore
# needs no io_uring on the host and no network, and it decides a verdict rather
# than reporting a rate. The inline-send pair is built twice because the two
# configurations retire a send along different paths.
URING_CHECK_INLINE := 0 1
URING_CHECK_BINARIES := \
	$(URING_CHECK_INLINE:%=$(BUILD)/uring_echo_check_inline%)

$(BUILD)/uring_echo_check_inline%: uring_echo_check.c uring_echo.c | prepare
	$(CLANG) $(CFLAGS) -Wpedantic -g -pthread \
		-DWF_BENCH_URING_INLINE_SEND=$* uring_echo_check.c -o $@

uring-check: $(URING_CHECK_BINARIES)
	@for binary in $(URING_CHECK_BINARIES); do \
		"$$binary" || exit 1; \
	done

$(BUILD)/epoll_echo: epoll_echo.c | prepare
	$(CLANG) $(CFLAGS) -pthread $< -o $@

$(BUILD)/netload: netload.c | prepare
	$(CLANG) $(CFLAGS) -pthread $< -o $@

$(BUILD)/waiting_echo: waiting_echo.c | prepare
	$(CLANG) $(CFLAGS) -pthread $< -o $@

$(BUILD)/idleload: idleload.c | prepare
	$(CLANG) $(CFLAGS) $< -o $@

$(BUILD)/wf_echo: $(WF_ECHO_SOURCE) $(WFC) Makefile | prepare
	$(WFC) -o $@ $<

# Every line, the Whitefoot one included, answers the load generator first.
net-verify: net-tools $(BUILD)/wf_echo
	@env ROOT=$(ROOT) OUT=$(BUILD) sh linux-net-bench.sh verify

linux-net:
	@mkdir -p $(NET_OUT)
	env ROOT=$(ROOT) OUT=$(NET_OUT) CLANG=$(CLANG) \
		CARGO_TARGET_DIR=$(ROOT)/compiler/target \
		ROUNDS=$(NET_ROUNDS) WARMUP=$(NET_WARMUP) \
		sh linux-net-bench.sh

clean:
	rm -rf $(WORK)

# Matched public/body callers use the existing ordinary-library object list.
# Correctness is gated; the optional table uses the original runner and seeds.
include $(ROOT)/compiler/runtime.mk
.PHONY: ordinary-build ordinary-check ordinary-bench
ordinary-build: prepare $(BUILD)/ordinary-public $(BUILD)/ordinary-body $(BUILD)/gen $(BUILD)/runner

$(BUILD)/ordinary-caller.o: ordinary-caller.ll | prepare
	$(CLANG) -O2 -Wno-override-module -x ir -c $< -o $@

$(BUILD)/ordinary-public: ordinary-caller.c workload.h $(BUILD)/ordinary-caller.o $(NATIVE_OBJECTS) | prepare
	$(CLANG) $(CFLAGS) -I$(NATIVE_ROOT) $< $(BUILD)/ordinary-caller.o $(NATIVE_OBJECTS) $(NATIVE_LINK_FLAGS) -o $@

$(BUILD)/ordinary-body: ordinary-caller.c workload.h $(BUILD)/ordinary-caller.o $(NATIVE_OBJECTS) | prepare
	$(CLANG) $(CFLAGS) -DWF_BENCH_PRIVATE_BODY=1 -I$(NATIVE_ROOT) $< $(BUILD)/ordinary-caller.o $(NATIVE_OBJECTS) $(NATIVE_LINK_FLAGS) -o $@

ordinary-check: ordinary-build
	$(BUILD)/gen $(BUILD)/ordinary-check-tree 4 16
	$(BUILD)/ordinary-public $(BUILD)/ordinary-check-tree check
	$(BUILD)/ordinary-body $(BUILD)/ordinary-check-tree check

# This cohort isolates the view wrapper route; public minus body is not the
# whole ordinary-call cost. WF minus public also includes its source lowering.
ordinary-bench: ordinary-build compiler $(BUILD)/baseline $(BUILD)/many_files_narrow $(BUILD)/many_files_narrow_seq $(BUILD)/many_files_loop $(BUILD)/many_files_loop_seq
	$(BUILD)/gen $(TREE) $(FILES) $(MAX_KIB)
	@{ \
	  printf 'N.direct\t\t%s\t%s\tdirect\t%s\n' '$(BUILD)/baseline' '$(TREE)' '$(FILES)'; \
	  printf 'N.public\t\t%s\t%s\t%s\n' '$(BUILD)/ordinary-public' '$(TREE)' '$(FILES)'; \
	  printf 'N.body\t\t%s\t%s\t%s\n' '$(BUILD)/ordinary-body' '$(TREE)' '$(FILES)'; \
	  printf 'WF.narrow\t\t%s\n' '$(BUILD)/many_files_narrow'; \
	  printf 'WF.narrow.seq\t\t%s\n' '$(BUILD)/many_files_narrow_seq'; \
	  printf 'WF.loop\t\t%s\n' '$(BUILD)/many_files_loop'; \
	  printf 'WF.loop.seq\t\t%s\n' '$(BUILD)/many_files_loop_seq'; \
	} > $(BUILD)/ordinary-plan.tsv
	cd $(TREE) && $(BUILD)/runner $(BUILD)/ordinary-plan.tsv $(ROUNDS) $(WARMUP) '$(EXPECTED)'
