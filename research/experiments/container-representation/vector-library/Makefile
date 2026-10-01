SHELL := /bin/sh
ROOT := $(abspath ../../../..)
EXE := $(if $(filter Windows_NT,$(OS)),.exe,)
WHITEFOOTC ?= $(ROOT)/compiler/target/gate/whitefootc$(EXE)
CLANG ?= clang
BUILD ?= .build
SOURCES := vector-library.wf
LIBRARY_INPUTS := $(wildcard $(ROOT)/lib/std/collections/vector/*) $(ROOT)/lib/std/modules.wfg
# Explicit observed-ledger selection; frozen old-compiler replay stays at 0.
WF_FULL_SLOTS_REALLOC ?= 1
WF_ZERO_CAPACITY_NO_ALLOC ?= 1
.DELETE_ON_ERROR:

include ../../../../compiler/runtime.mk

.PHONY: check measure clean

$(BUILD):
	mkdir -p $@

$(BUILD)/vector-library.raw.ll: $(SOURCES) $(LIBRARY_INPUTS) $(WHITEFOOTC) Makefile | $(BUILD)
	$(WHITEFOOTC) --emit-llvm $(SOURCES) -o $@

$(BUILD)/vector-library-normal.ll: $(BUILD)/vector-library.raw.ll Makefile
	sed -e 's/@malloc(/@wf_cost_allocate(/g' \
	    -e 's/@realloc(/@wf_cost_reallocate(/g' \
	    -e 's/@free(/@wf_cost_release(/g' \
	    -e 's/@main(/@wf_fixture_main(/g' $< > $@

$(BUILD)/vector-library-retained.ll: $(BUILD)/vector-library-normal.ll Makefile
	awk '/^define .* @"?wf_(grow_vector_|std[.]collections[.]vector[.]grow_vector_|vector_library_(work|round|tail_work)|vector_(word|record)_(make|accept))/ && !/[.]body["(]/ { if (sub(/ #0 [{]$$/, " noinline #0 {") != 1) exit 1; retained++ } { print } END { if (retained == 0) exit 1; print "retained Vector helpers: " retained > "/dev/stderr" }' $< > $@

$(BUILD)/vector-library-%.opt.ll: $(BUILD)/vector-library-%.ll
	$(CLANG) -O2 -Wno-override-module -x ir -S -emit-llvm $< -o $@

$(BUILD)/control-%.opt.ll: vector-costs.c | $(BUILD)
	@set -eu; case '$*' in normal) flags= ;; retained) flags=-DRETAIN_HELPERS ;; *) exit 1 ;; esac; \
	  $(CLANG) -std=c11 -O2 -Wall -Wextra -Werror -DWF_FULL_SLOTS_REALLOC=$(WF_FULL_SLOTS_REALLOC) $$flags -S -emit-llvm $< -o $@

$(BUILD)/vector-costs-%$(EXE): vector-costs.c $(BUILD)/vector-library-%.ll $(NATIVE_OBJECTS)
	@set -eu; case '$*' in normal) flags= ;; retained) flags=-DRETAIN_HELPERS ;; *) exit 1 ;; esac; \
	  $(CLANG) -std=c11 -O2 -Wall -Wextra -Werror -Wno-override-module -DWF_FULL_SLOTS_REALLOC=$(WF_FULL_SLOTS_REALLOC) $$flags $< $(BUILD)/vector-library-$*.ll $(NATIVE_OBJECTS) $(NATIVE_LINK_FLAGS) -o $@

check: $(addprefix $(BUILD)/vector-costs-,normal$(EXE) retained$(EXE)) \
       $(addprefix $(BUILD)/vector-library-,normal.opt.ll retained.opt.ll) \
       $(addprefix $(BUILD)/control-,normal.opt.ll retained.opt.ll)
	$(BUILD)/vector-costs-normal$(EXE) check
	$(BUILD)/vector-costs-retained$(EXE) check
	$(BUILD)/vector-costs-normal$(EXE) allocator-check
	@set -eu; for fault in values accounting; do \
	  log='$(BUILD)/allocator-fail-'$$fault'.txt'; \
	  if $(BUILD)/vector-costs-normal$(EXE) allocator-fail-$$fault > "$$log" 2>&1; then \
	    echo "Allocator observer $$fault fault was accepted" >&2; exit 1; \
	  else status=$$?; test "$$status" = 1; fi; \
	  case $$fault in values) message='preserved bytes' ;; accounting) message='exact ledger' ;; esac; \
	  grep -Fx "vector library costs: reallocation observer $$message" "$$log"; \
	 done
	@set -eu; normal_calls=`grep -Ec 'call .*@"?wf_(grow_vector_|std[.]collections[.]vector[.]grow_vector_)' $(BUILD)/vector-library-normal.opt.ll || true`; \
	 retained_calls=`grep -Ec 'call .*@"?wf_(grow_vector_|std[.]collections[.]vector[.]grow_vector_)' $(BUILD)/vector-library-retained.opt.ll || true`; \
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
APPEND_WORK ?= 4194304
APPEND_SAMPLE_FILE ?= $(ECO_BUILD)/append-measurements.csv
GROWTH_ACCOUNT ?= $(ECO_BUILD)/append-growth-accounting.csv
GROWTH_WORK ?= 67108864
GROWTH_LARGE_WORK ?= $(GROWTH_WORK)
GROWTH_SAMPLE_FILE ?= $(ECO_BUILD)/append-growth-measurements.csv
EDIT_WORK ?= 4194304
EDIT_SAMPLE_FILE ?= $(ECO_BUILD)/edit-api-measurements.csv
include ../ecosystem.mk
# Read only by observed code; the existing configuration records the choice
# so changing it also invalidates a cached account driver.
ECO_CFLAGS += -DWF_FULL_SLOTS_REALLOC=$(WF_FULL_SLOTS_REALLOC) -DWF_ZERO_CAPACITY_NO_ALLOC=$(WF_ZERO_CAPACITY_NO_ALLOC)

.PHONY: ecosystem-build ecosystem-check ecosystem-account ecosystem-measure ecosystem-append-check ecosystem-append-measure ecosystem-append-growth-check ecosystem-append-growth-account ecosystem-append-growth-measure ecosystem-edit-check ecosystem-edit-measure
.SECONDARY: $(ECO_BUILD)/whitefoot-timed.ll $(ECO_BUILD)/whitefoot-account.ll \
            $(ECO_BUILD)/whitefoot-timed.o $(ECO_BUILD)/whitefoot-account.o \
            $(ECO_BUILD)/driver-timed.o $(ECO_BUILD)/driver-account.o \
            $(ECO_BUILD)/rust-timed.a $(ECO_BUILD)/rust-account.a \
            $(ECO_BUILD)/cpp-timed.o $(ECO_BUILD)/cpp-account.o

$(ECO_BUILD)/whitefoot-timed.ll: $(BUILD)/vector-library.raw.ll Makefile $(ECO_CONFIG)
	sed 's/@main(/@wf_fixture_main(/g' $< > $@

$(ECO_BUILD)/whitefoot-account.ll: $(BUILD)/vector-library.raw.ll Makefile $(ECO_CONFIG)
	sed -e 's/@malloc(/@wf_cost_allocate(/g' -e 's/@realloc(/@wf_cost_reallocate(/g' \
	    -e 's/@free(/@wf_cost_release(/g' \
	    -e 's/@main(/@wf_fixture_main(/g' \
	    -e 's/^declare ptr @wf_cost_allocate(i64)$$/declare noalias nonnull ptr @wf_cost_allocate(i64)/' $< > $@

$(ECO_BUILD)/whitefoot-%.o: $(ECO_BUILD)/whitefoot-%.ll $(ECO_CONFIG)
	$(CLANG) $(ECO_LDFLAGS) -Wno-override-module -x ir -c $< -o $@

$(ECO_BUILD)/driver-%.o: vector-costs.c Makefile $(ECO_CONFIG)
	@set -eu; case '$*' in timed) account= ;; account) account=-DACCOUNT_ONLY ;; *) exit 1 ;; esac; \
	  $(CLANG) $(ECO_CPPFLAGS) $(ECO_CFLAGS) $$account -c $< -o $@

$(ECO_BUILD)/vector-costs-%$(EXE): $(ECO_BUILD)/driver-%.o $(ECO_BUILD)/whitefoot-%.o \
                                   $(ECO_BUILD)/cpp-%.o $(ECO_BUILD)/rust-%.a $(NATIVE_OBJECTS) $(ECO_CONFIG)
	$(CXX) $(ECO_LDFLAGS) $(ECO_BUILD)/driver-$*.o \
	  $(ECO_BUILD)/cpp-$*.o $(ECO_BUILD)/rust-$*.a $(NATIVE_OBJECTS) \
	  $(ECO_BUILD)/whitefoot-$*.o \
	  $(NATIVE_LINK_FLAGS) $(ECO_RUST_LINK_FLAGS) -o $@

ecosystem-build: $(ECO_BUILD)/vector-costs-timed$(EXE) $(ECO_BUILD)/vector-costs-account$(EXE)

ecosystem-append-check: ecosystem-build
	$(ECO_BUILD)/vector-costs-timed$(EXE) api-check
	$(ECO_BUILD)/vector-costs-account$(EXE) api-check
	@set -eu; for fault in values allocation; do \
	  if $(ECO_BUILD)/vector-costs-account$(EXE) api-fail-$$fault > $(ECO_BUILD)/api-fail-$$fault.txt 2>&1; then \
	    echo "Spare append $$fault fault was accepted" >&2; exit 1; else status=$$?; test "$$status" = 1; fi; \
	 done
	grep -Fx 'vector library costs: spare append full values and checksum' $(ECO_BUILD)/api-fail-values.txt
	grep -Fx 'vector library costs: spare append zero allocation' $(ECO_BUILD)/api-fail-allocation.txt

ecosystem-append-growth-check: ecosystem-build
	$(ECO_BUILD)/vector-costs-timed$(EXE) growth-api-check
	$(ECO_BUILD)/vector-costs-account$(EXE) growth-api-check
	$(ECO_BUILD)/vector-costs-account$(EXE) allocator-check
	@set -eu; for fault in values accounting; do \
	  log='$(ECO_BUILD)/allocator-fail-'$$fault'.txt'; \
	  if $(ECO_BUILD)/vector-costs-account$(EXE) allocator-fail-$$fault > "$$log" 2>&1; then \
	    echo "Allocator observer $$fault fault was accepted" >&2; exit 1; \
	  else status=$$?; test "$$status" = 1; fi; \
	  case $$fault in values) message='preserved bytes' ;; accounting) message='exact ledger' ;; esac; \
	  grep -Fx "vector library costs: reallocation observer $$message" "$$log"; \
	 done
	@set -eu; log='$(ECO_BUILD)/growth-api-fail-clock.txt'; \
	  if $(ECO_BUILD)/vector-costs-timed$(EXE) growth-api-fail-clock > "$$log" 2>&1; then \
	    echo 'Append growth coarse clock was accepted' >&2; exit 1; \
	  else status=$$?; test "$$status" = 1; fi; \
	  grep -Fx 'vector library costs: growth append clock precision' "$$log"
	@set -eu; for image in timed account; do for fault in values state; do \
	  log='$(ECO_BUILD)/growth-api-fail-'$$image'-'$$fault'.txt'; \
	  if $(ECO_BUILD)/vector-costs-$$image$(EXE) growth-api-fail-$$fault > "$$log" 2>&1; then \
	    echo "Append growth $$image $$fault fault was accepted" >&2; exit 1; \
	  else status=$$?; test "$$status" = 1; fi; \
	  case $$fault in values) message='growth append full values and checksum' ;; state) message='growth append post state' ;; esac; \
	  grep -Fx "vector library costs: $$message" "$$log"; \
	 done; done

ecosystem-append-growth-account: $(ECO_BUILD)/vector-costs-account$(EXE)
	$(ECO_BUILD)/vector-costs-account$(EXE) growth-api-account > '$(GROWTH_ACCOUNT)'
	@echo 'Append growth allocation observations: $(GROWTH_ACCOUNT)'

ecosystem-append-growth-measure: $(ECO_BUILD)/vector-costs-timed$(EXE)
	$(ECO_BUILD)/vector-costs-timed$(EXE) growth-api-measure $(GROWTH_WORK) $(ECO_REPEATS) $(GROWTH_LARGE_WORK) > '$(GROWTH_SAMPLE_FILE)'
	@echo 'Raw append growth samples: $(GROWTH_SAMPLE_FILE)'

ecosystem-edit-check: ecosystem-build
	$(ECO_BUILD)/vector-costs-timed$(EXE) edit-api-check
	$(ECO_BUILD)/vector-costs-account$(EXE) edit-api-check
	@set -eu; for image in timed account; do for fault in values state; do \
	  log='$(ECO_BUILD)/edit-fail-'$$image'-'$$fault'.txt'; \
	  if $(ECO_BUILD)/vector-costs-$$image$(EXE) edit-api-fail-$$fault > "$$log" 2>&1; then \
	    echo "Vector edit $$image $$fault fault was accepted" >&2; exit 1; \
	  else status=$$?; test "$$status" = 1; fi; \
	  case $$fault in values) message='edit full ordered values and checksum' ;; state) message='edit post state' ;; esac; \
	  grep -Fx "vector library costs: $$message" "$$log"; \
	 done; done
	@set -eu; log='$(ECO_BUILD)/edit-fail-cleanup.txt'; \
	  if $(ECO_BUILD)/vector-costs-account$(EXE) edit-api-fail-cleanup > "$$log" 2>&1; then \
	    echo 'Vector edit cleanup fault was accepted' >&2; exit 1; \
	  else status=$$?; test "$$status" = 1; fi; \
	  grep -Fx 'vector library costs: edit complete cleanup' "$$log"

ecosystem-edit-measure: $(ECO_BUILD)/vector-costs-timed$(EXE)
	$(ECO_BUILD)/vector-costs-timed$(EXE) edit-api-measure $(EDIT_WORK) $(ECO_REPEATS) > '$(EDIT_SAMPLE_FILE)'
	@echo 'Raw reserve/insert/remove/drain samples: $(EDIT_SAMPLE_FILE)'

ecosystem-check: ecosystem-build
	$(ECO_BUILD)/vector-costs-timed$(EXE) check
	$(ECO_BUILD)/vector-costs-account$(EXE) check
	@set -eu; \
	  if $(ECO_BUILD)/vector-costs-account$(EXE) fail-checksum > $(ECO_BUILD)/fail-checksum.txt 2>&1; then \
	    echo 'Corrupted checksum was accepted' >&2; exit 1; else status=$$?; test "$$status" = 1; fi; \
	  grep -Fx 'vector library costs: independent logical-order checksum' $(ECO_BUILD)/fail-checksum.txt; \
	  if $(ECO_BUILD)/vector-costs-account$(EXE) fail-cleanup > $(ECO_BUILD)/fail-cleanup.txt 2>&1; then \
	    echo 'Unreleased allocation was accepted' >&2; exit 1; else status=$$?; test "$$status" = 1; fi; \
	  grep -Fx 'vector library costs: complete cleanup' $(ECO_BUILD)/fail-cleanup.txt
	@set -eu; \
	  if grep -Eq '@wf_(cost_allocate|cost_reallocate|cost_release|ecosystem_note_)' $(ECO_BUILD)/whitefoot-timed.ll; then \
	    echo 'Timed Whitefoot includes allocation observer hooks' >&2; exit 1; fi; \
	  nm -g $(ECO_BUILD)/vector-costs-timed$(EXE) > $(ECO_BUILD)/timed-symbols.txt; \
	  if grep -Eq 'wf_(cost_allocate|cost_reallocate|cost_release|ecosystem_note_)' $(ECO_BUILD)/timed-symbols.txt; then \
	    echo 'Timed native image includes allocation observer hooks' >&2; exit 1; fi

ecosystem-account: $(ECO_BUILD)/vector-costs-account$(EXE)
	$(ECO_BUILD)/vector-costs-account$(EXE) account > '$(ECO_ACCOUNT)'
	@echo 'Allocation observations: $(ECO_ACCOUNT)'

ecosystem-measure: $(ECO_BUILD)/vector-costs-timed$(EXE)
	$(ECO_BUILD)/vector-costs-timed$(EXE) measure $(ECO_WORK) $(ECO_REPEATS) > '$(ECO_SAMPLE_FILE)'
	@echo 'Raw practical samples: $(ECO_SAMPLE_FILE)'

ecosystem-append-measure: $(ECO_BUILD)/vector-costs-timed$(EXE)
	$(ECO_BUILD)/vector-costs-timed$(EXE) api-measure $(APPEND_WORK) $(ECO_REPEATS) > '$(APPEND_SAMPLE_FILE)'
	@echo 'Raw spare-capacity append samples: $(APPEND_SAMPLE_FILE)'

# Opt-in plain-C append layout discriminator; no compiler/runtime inputs.
LAYOUT_BUILD ?= $(BUILD)/append-allocation-layout
LAYOUT_FLAGS := -std=c11 -O2 -Wall -Wextra -Werror -fno-lto
.PHONY: append-layout-build append-layout-check append-layout-measure
$(LAYOUT_BUILD):
	mkdir -p $@
$(LAYOUT_BUILD)/helper.o: append-allocation-layout.c Makefile | $(LAYOUT_BUILD)
	$(CLANG) $(LAYOUT_FLAGS) -DLAYOUT_HELPER -c $< -o $@
$(LAYOUT_BUILD)/caller.o: append-allocation-layout.c Makefile | $(LAYOUT_BUILD)
	$(CLANG) $(LAYOUT_FLAGS) -c $< -o $@
$(LAYOUT_BUILD)/append-layout: $(LAYOUT_BUILD)/helper.o $(LAYOUT_BUILD)/caller.o
	$(CLANG) -fno-lto $^ -o $@
append-layout-build: $(LAYOUT_BUILD)/append-layout
append-layout-check: append-layout-build
	$(LAYOUT_BUILD)/append-layout check
	@set -eu; for fault in payload capacity clock; do \
	  log=$(LAYOUT_BUILD)/fail-$$fault.txt; \
	  if $(LAYOUT_BUILD)/append-layout fail-$$fault > "$$log" 2>&1; then \
	    cat "$$log"; echo "fault $$fault unexpectedly passed"; exit 1; \
	  else status=$$?; test "$$status" = 1; fi; \
	  case $$fault in payload) expected='old payload';; capacity) expected='post capacity';; clock) expected='clock precision';; esac; \
	  grep -Fx "append layout: $$expected" "$$log"; \
	done
append-layout-measure: append-layout-build
	$(LAYOUT_BUILD)/append-layout measure
