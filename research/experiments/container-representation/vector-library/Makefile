SHELL := /bin/sh
ROOT := $(abspath ../../../..)
EXE := $(if $(filter Windows_NT,$(OS)),.exe,)
WHITEFOOTC ?= $(ROOT)/compiler/target/gate/whitefootc$(EXE)
CLANG ?= clang
BUILD ?= .build
SOURCES := vector-library.wf
LIBRARY_INPUTS := $(wildcard $(ROOT)/lib/std/collections/vector/*) $(ROOT)/lib/std/modules.wfg
.DELETE_ON_ERROR:

include ../../../../compiler/runtime.mk

.PHONY: check measure clean

$(BUILD):
	mkdir -p $@

$(BUILD)/vector-library.raw.ll: $(SOURCES) $(LIBRARY_INPUTS) $(WHITEFOOTC) Makefile | $(BUILD)
	$(WHITEFOOTC) --emit-llvm $(SOURCES) -o $@

$(BUILD)/vector-library-normal.ll: $(BUILD)/vector-library.raw.ll Makefile
	sed -e 's/@malloc(/@wf_cost_allocate(/g' \
	    -e 's/@free(/@wf_cost_release(/g' \
	    -e 's/@main(/@wf_fixture_main(/g' $< > $@

$(BUILD)/vector-library-retained.ll: $(BUILD)/vector-library-normal.ll Makefile
	awk '/^define .* @"?wf_(std[.]collections[.]vector[.]grow_vector_|vector_library_(work|round|tail_work)|vector_(word|record)_(make|accept))/ && !/[.]body["(]/ { if (sub(/ #0 [{]$$/, " noinline #0 {") != 1) exit 1; retained++ } { print } END { if (retained == 0) exit 1; print "retained Vector helpers: " retained > "/dev/stderr" }' $< > $@

$(BUILD)/vector-library-%.opt.ll: $(BUILD)/vector-library-%.ll
	$(CLANG) -O2 -Wno-override-module -x ir -S -emit-llvm $< -o $@

$(BUILD)/control-%.opt.ll: vector-costs.c | $(BUILD)
	@set -eu; case '$*' in normal) flags= ;; retained) flags=-DRETAIN_HELPERS ;; *) exit 1 ;; esac; \
	  $(CLANG) -std=c11 -O2 -Wall -Wextra -Werror $$flags -S -emit-llvm $< -o $@

$(BUILD)/vector-costs-%$(EXE): vector-costs.c $(BUILD)/vector-library-%.ll $(NATIVE_OBJECTS)
	@set -eu; case '$*' in normal) flags= ;; retained) flags=-DRETAIN_HELPERS ;; *) exit 1 ;; esac; \
	  $(CLANG) -std=c11 -O2 -Wall -Wextra -Werror -Wno-override-module $$flags $< $(BUILD)/vector-library-$*.ll $(NATIVE_OBJECTS) $(NATIVE_LINK_FLAGS) -o $@

check: $(addprefix $(BUILD)/vector-costs-,normal$(EXE) retained$(EXE)) \
       $(addprefix $(BUILD)/vector-library-,normal.opt.ll retained.opt.ll) \
       $(addprefix $(BUILD)/control-,normal.opt.ll retained.opt.ll)
	$(BUILD)/vector-costs-normal$(EXE) check
	$(BUILD)/vector-costs-retained$(EXE) check
	@set -eu; normal_calls=`grep -Ec 'call .*@"?wf_std[.]collections[.]vector[.]grow_vector_' $(BUILD)/vector-library-normal.opt.ll || true`; \
	 retained_calls=`grep -Ec 'call .*@"?wf_std[.]collections[.]vector[.]grow_vector_' $(BUILD)/vector-library-retained.opt.ll || true`; \
	 test "$$retained_calls" -gt 0; \
	 control_calls=`grep -Ec 'call .*@(word|record)_(reverse|direct)_(append|truncate)' $(BUILD)/control-retained.opt.ll || true`; \
	 test "$$control_calls" -gt 0; \
	 echo "Vector optimized helper call sites: normal=$$normal_calls retained=$$retained_calls; retained C append/truncate=$$control_calls"

measure: check
	@set -eu; \
	 $(BUILD)/vector-costs-normal$(EXE) measure > $(BUILD)/normal.csv; \
	 $(BUILD)/vector-costs-retained$(EXE) measure > $(BUILD)/retained.csv; \
	 cat $(BUILD)/normal.csv > $(BUILD)/measurements.csv; \
	 tail -n +2 $(BUILD)/retained.csv >> $(BUILD)/measurements.csv; \
	 echo "Raw samples: $(BUILD)/measurements.csv"

clean:
	rm -rf $(BUILD)

# Opt-in practical native-library comparison. Historical O2/retained targets
# remain above; this O3 series has ordinary timed allocation and a separate
# allocation observer image. Neither is part of canonical correctness CI.
ECO_FAMILY := vector
ECO_WORK ?= 1048576
ECO_REPEATS ?= 7
include ../ecosystem.mk

.PHONY: ecosystem-build ecosystem-check ecosystem-account ecosystem-measure
.SECONDARY: $(ECO_BUILD)/whitefoot-timed.ll $(ECO_BUILD)/whitefoot-account.ll \
            $(ECO_BUILD)/whitefoot-timed.o $(ECO_BUILD)/whitefoot-account.o \
            $(ECO_BUILD)/driver-timed.o $(ECO_BUILD)/driver-account.o \
            $(ECO_BUILD)/rust-timed.a $(ECO_BUILD)/rust-account.a \
            $(ECO_BUILD)/cpp-timed.o $(ECO_BUILD)/cpp-account.o

$(ECO_BUILD)/whitefoot-timed.ll: $(BUILD)/vector-library.raw.ll Makefile $(ECO_CONFIG)
	sed 's/@main(/@wf_fixture_main(/g' $< > $@

$(ECO_BUILD)/whitefoot-account.ll: $(BUILD)/vector-library.raw.ll Makefile $(ECO_CONFIG)
	sed -e 's/@malloc(/@wf_cost_allocate(/g' -e 's/@free(/@wf_cost_release(/g' \
	    -e 's/@main(/@wf_fixture_main(/g' \
	    -e 's/^declare ptr @wf_cost_allocate(i64)$$/declare noalias nonnull ptr @wf_cost_allocate(i64)/' $< > $@

$(ECO_BUILD)/whitefoot-%.o: $(ECO_BUILD)/whitefoot-%.ll $(ECO_CONFIG)
	$(CLANG) $(ECO_LDFLAGS) -Wno-override-module -x ir -c $< -o $@

$(ECO_BUILD)/driver-%.o: vector-costs.c Makefile $(ECO_CONFIG)
	@set -eu; case '$*' in timed) account= ;; account) account=-DACCOUNT_ONLY ;; *) exit 1 ;; esac; \
	  $(CLANG) $(ECO_CPPFLAGS) $(ECO_CFLAGS) $$account -c $< -o $@

$(ECO_BUILD)/vector-costs-%$(EXE): $(ECO_BUILD)/driver-%.o $(ECO_BUILD)/whitefoot-%.o \
                                   $(ECO_BUILD)/cpp-%.o $(ECO_BUILD)/rust-%.a $(NATIVE_OBJECTS) $(ECO_CONFIG)
	$(CXX) $(ECO_LDFLAGS) $(ECO_BUILD)/driver-$*.o $(ECO_BUILD)/whitefoot-$*.o \
	  $(ECO_BUILD)/cpp-$*.o $(ECO_BUILD)/rust-$*.a $(NATIVE_OBJECTS) \
	  $(NATIVE_LINK_FLAGS) $(ECO_RUST_LINK_FLAGS) -o $@

ecosystem-build: $(ECO_BUILD)/vector-costs-timed$(EXE) $(ECO_BUILD)/vector-costs-account$(EXE)

ecosystem-check: ecosystem-build
	$(ECO_BUILD)/vector-costs-timed$(EXE) check
	$(ECO_BUILD)/vector-costs-account$(EXE) check
	@set -eu; \
	  if $(ECO_BUILD)/vector-costs-account$(EXE) fail-checksum > $(ECO_BUILD)/fail-checksum.txt 2>&1; then \
	    echo 'Corrupted checksum was accepted' >&2; exit 1; else status=$$?; test "$$status" = 1; fi; \
	  grep -Fx 'vector library costs: independent logical-order checksum' $(ECO_BUILD)/fail-checksum.txt; \
	  if $(ECO_BUILD)/vector-costs-account$(EXE) fail-cleanup > $(ECO_BUILD)/fail-cleanup.txt 2>&1; then \
	    echo 'Unreleased allocation was accepted' >&2; exit 1; else status=$$?; test "$$status" = 1; fi; \
	  grep -Fx 'vector library costs: complete cleanup' $(ECO_BUILD)/fail-cleanup.txt; \
	  if $(ECO_BUILD)/vector-costs-account$(EXE) fail-construction > $(ECO_BUILD)/fail-construction.txt 2>&1; then \
	    echo 'Extra empty backing was accepted' >&2; exit 1; else status=$$?; test "$$status" = 1; fi; \
	  grep -Fx 'vector library costs: independent allocation count, byte and peak formula' $(ECO_BUILD)/fail-construction.txt
	@set -eu; \
	  if grep -Eq '@wf_(cost_allocate|cost_release|ecosystem_note_)' $(ECO_BUILD)/whitefoot-timed.ll; then \
	    echo 'Timed Whitefoot includes allocation observer hooks' >&2; exit 1; fi; \
	  nm -g $(ECO_BUILD)/vector-costs-timed$(EXE) > $(ECO_BUILD)/timed-symbols.txt; \
	  if grep -Eq 'wf_(cost_allocate|cost_release|ecosystem_note_)' $(ECO_BUILD)/timed-symbols.txt; then \
	    echo 'Timed native image includes allocation observer hooks' >&2; exit 1; fi

ecosystem-account: $(ECO_BUILD)/vector-costs-account$(EXE)
	$(ECO_BUILD)/vector-costs-account$(EXE) account > '$(ECO_ACCOUNT)'
	@echo 'Allocation observations: $(ECO_ACCOUNT)'

ecosystem-measure: $(ECO_BUILD)/vector-costs-timed$(EXE)
	$(ECO_BUILD)/vector-costs-timed$(EXE) measure $(ECO_WORK) $(ECO_REPEATS) > '$(ECO_SAMPLE_FILE)'
	@echo 'Raw practical samples: $(ECO_SAMPLE_FILE)'
