# Serves concurrent-map-bench: the measurement of
# research/investigations/concurrent-map/DESIGN.md. Research only: neither
# `make check` nor CI runs any target here.
#
#   make deps      fetch (network) and build the pinned comparators
#   make build     the Zipf generator and one driver binary per implementation
#   make verify    every implementation at a small size, every mix, short
#                  cells; fails on any failed check
#   make quick     the quick profile over every implementation (minutes)
#   make duel      the index against the fastest comparators (minutes)
#   make full      the whole matrix, hours; only when unavoidable
#   make entry-reads  one key of a map of entries read exclusively against
#                  shared, for keyed statements that only read (seconds)
#
# Generated trees, binaries and results stay in the scratch root. Nothing is
# written inside the repository.

ROOT  := $(abspath ../../..)
HERE  := $(abspath .)
WHITEFOOT_SCRATCH_ROOT ?= $(patsubst %/,%,$(if $(TMPDIR),$(TMPDIR),/tmp))/whitefoot
WORK  := $(WHITEFOOT_SCRATCH_ROOT)/whitefoot-concurrent-map-bench
BUILD := $(WORK)/build
DEPS  := $(WORK)/deps
RESULTS ?= $(WORK)/results/$(shell date -u +%Y%m%dT%H%M%SZ)
RESULTS := $(RESULTS)
CC    := clang
CXX   := clang++
ARCH  := $(shell uname -m)
# Never -march=native (compute-bench records why).
MARCH := $(if $(filter x86_64,$(ARCH)),-march=x86-64-v3,)
CFLAGS   := -O3 $(MARCH) -g0 -std=c11 -Wall -Wextra -I$(HERE) -isystem $(DEPS)/include -pthread
CXXFLAGS := -O3 $(MARCH) -g0 -std=c++20 -Wall -Wextra -I$(HERE) -isystem $(DEPS)/include -pthread
RUSTLIB  := $(DEPS)/cmaps-target/release/libcmaps.a

NATIVE := wf_index wf_index_locked empty mutex_flat flat boost_cfm tbb_chm libcuckoo urcu_lfht growt phmap papaya dashmap scc stdmap
BINS   := $(addprefix $(BUILD)/bench-,$(NATIVE))
MANAGED := $(BUILD)/java/Bench.class $(BUILD)/bench-go $(BUILD)/dotnet/Bench.dll
# The runtime's concurrent map, compiled into its driver adapter.
RUNTIME_BACKEND := $(ROOT)/compiler/src/backend

.PHONY: deps build verify quick duel full entry-reads
deps:
	sh deps.sh fetch
	OUT=$(DEPS) sh deps.sh build

build: $(BUILD)/zipfgen $(BINS) $(MANAGED)

$(BUILD)/entry-reads: entry_reads.c $(RUNTIME_BACKEND)/concurrent_map.c $(RUNTIME_BACKEND)/concurrent_map.h | $(BUILD)
	$(CC) $(CFLAGS) -I$(RUNTIME_BACKEND) -o $@ entry_reads.c

# The holds of DESIGN.md's shared reads: an empty block and replies of 100
# and 400 elements, with no work, 100 and 300 rounds between reads.
entry-reads: $(BUILD)/entry-reads
	@set -e; for elements in 0 100 400; do for outside in 0 100 300; do $(BUILD)/entry-reads $$elements $$outside; done; done

$(BUILD)/zipfgen: zipfgen.c workload.h | $(BUILD)
	$(CC) $(CFLAGS) -o $@ zipfgen.c -lm

$(BUILD)/driver-%.o: driver.c cmap.h workload.h | $(BUILD)
	$(CC) $(CFLAGS) -DCM_PREFIX=$* -c -o $@ driver.c

$(BUILD)/impl-wf_index.o: native/wf_index.c cmap.h $(RUNTIME_BACKEND)/concurrent_map.c $(RUNTIME_BACKEND)/concurrent_map.h | $(BUILD)
	$(CC) $(CFLAGS) -I$(RUNTIME_BACKEND) -DCM_PREFIX=wf_index -c -o $@ $<
$(BUILD)/impl-wf_index_locked.o: native/wf_index.c cmap.h $(RUNTIME_BACKEND)/concurrent_map.c $(RUNTIME_BACKEND)/concurrent_map.h | $(BUILD)
	$(CC) $(CFLAGS) -I$(RUNTIME_BACKEND) -DCM_PREFIX=wf_index_locked -DWF_CMAP_LOCKED_READ -c -o $@ $<
$(BUILD)/impl-empty.o: native/empty.c cmap.h | $(BUILD)
	$(CC) $(CFLAGS) -DCM_PREFIX=empty -c -o $@ $<
$(BUILD)/impl-mutex_flat.o: native/flat.cpp cmap.h | $(BUILD)
	$(CXX) $(CXXFLAGS) -DCM_PREFIX=mutex_flat -DFLAT_LOCKED -c -o $@ $<
$(BUILD)/impl-flat.o: native/flat.cpp cmap.h | $(BUILD)
	$(CXX) $(CXXFLAGS) -DCM_PREFIX=flat -c -o $@ $<
$(BUILD)/impl-boost_cfm.o: native/boost_cfm.cpp cmap.h | $(BUILD)
	$(CXX) $(CXXFLAGS) -DCM_PREFIX=boost_cfm -c -o $@ $<
$(BUILD)/impl-tbb_chm.o: native/tbb_chm.cpp cmap.h | $(BUILD)
	$(CXX) $(CXXFLAGS) -DCM_PREFIX=tbb_chm -c -o $@ $<
$(BUILD)/impl-libcuckoo.o: native/libcuckoo.cpp cmap.h | $(BUILD)
	$(CXX) $(CXXFLAGS) -DCM_PREFIX=libcuckoo -c -o $@ $<
$(BUILD)/impl-urcu_lfht.o: native/urcu_lfht.c cmap.h | $(BUILD)
	$(CC) $(CFLAGS) -DCM_PREFIX=urcu_lfht -c -o $@ $<
$(BUILD)/impl-growt.o: native/growt.cpp cmap.h | $(BUILD)
	$(CXX) $(CXXFLAGS) -isystem $(DEPS)/growt -DCM_PREFIX=growt -c -o $@ $<
$(BUILD)/impl-phmap.o: native/phmap.cpp cmap.h | $(BUILD)
	$(CXX) $(CXXFLAGS) -DCM_PREFIX=phmap -c -o $@ $<

LINK_C   = $(CC) $(CFLAGS) -o $@ $^
LINK_CXX = $(CXX) $(CXXFLAGS) -o $@ $^
$(BUILD)/bench-wf_index: $(BUILD)/driver-wf_index.o $(BUILD)/impl-wf_index.o
	$(LINK_C)
$(BUILD)/bench-wf_index_locked: $(BUILD)/driver-wf_index_locked.o $(BUILD)/impl-wf_index_locked.o
	$(LINK_C)
$(BUILD)/bench-empty: $(BUILD)/driver-empty.o $(BUILD)/impl-empty.o
	$(LINK_C)
$(BUILD)/bench-mutex_flat: $(BUILD)/driver-mutex_flat.o $(BUILD)/impl-mutex_flat.o
	$(LINK_CXX)
$(BUILD)/bench-flat: $(BUILD)/driver-flat.o $(BUILD)/impl-flat.o
	$(LINK_CXX)
$(BUILD)/bench-boost_cfm: $(BUILD)/driver-boost_cfm.o $(BUILD)/impl-boost_cfm.o
	$(LINK_CXX)
$(BUILD)/bench-tbb_chm: $(BUILD)/driver-tbb_chm.o $(BUILD)/impl-tbb_chm.o
	$(LINK_CXX) -L$(DEPS)/lib -Wl,-rpath,$(DEPS)/lib -ltbb
$(BUILD)/bench-libcuckoo: $(BUILD)/driver-libcuckoo.o $(BUILD)/impl-libcuckoo.o
	$(LINK_CXX)
$(BUILD)/bench-growt: $(BUILD)/driver-growt.o $(BUILD)/impl-growt.o
	$(LINK_CXX)
$(BUILD)/bench-phmap: $(BUILD)/driver-phmap.o $(BUILD)/impl-phmap.o
	$(LINK_CXX)
$(BUILD)/bench-urcu_lfht: $(BUILD)/driver-urcu_lfht.o $(BUILD)/impl-urcu_lfht.o
	$(LINK_C) $(DEPS)/lib/liburcu-cds.a $(DEPS)/lib/liburcu.a $(DEPS)/lib/liburcu-common.a
$(BUILD)/bench-papaya $(BUILD)/bench-dashmap $(BUILD)/bench-scc $(BUILD)/bench-stdmap: \
$(BUILD)/bench-%: $(BUILD)/driver-%.o $(RUSTLIB)
	$(CC) $(CFLAGS) -o $@ $< $(RUSTLIB) -ldl -lm

$(BUILD)/java/Bench.class: java/Bench.java | $(BUILD)
	mkdir -p $(BUILD)/java
	javac -d $(BUILD)/java java/Bench.java
# Go keeps its module cache outside the tree; `deps` downloads the module.
$(BUILD)/bench-go: go/main.go go/go.mod go/go.sum | $(BUILD)
	cd go && go build -o $@ .
# dotnet writes beside its project, so the project is built from a copy.
$(BUILD)/dotnet/Bench.dll: dotnet/Program.cs dotnet/Bench.csproj | $(BUILD)
	rm -rf $(BUILD)/dotnet-src && mkdir -p $(BUILD)/dotnet-src
	cp dotnet/Program.cs dotnet/Bench.csproj $(BUILD)/dotnet-src/
	dotnet build $(BUILD)/dotnet-src/Bench.csproj -c Release -o $(BUILD)/dotnet --nologo -v q

$(BUILD):
	mkdir -p $@

verify: build
	sh run.sh verify $(BUILD) $(WORK)/verify

quick: build
	sh run.sh quick $(BUILD) $(RESULTS)

duel: build
	sh run.sh duel $(BUILD) $(RESULTS)

full: build
	sh run.sh full $(BUILD) $(RESULTS)
