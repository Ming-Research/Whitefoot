SHELL := /bin/sh
ROOT := $(abspath ../../../..)
EXE := $(if $(filter Windows_NT,$(OS)),.exe,)
WHITEFOOTC ?= $(ROOT)/compiler/target/gate/whitefootc$(EXE)
CLANG ?= clang
BUILD ?= .build
SOURCES := vector-library.wf
.DELETE_ON_ERROR:

include ../../../../compiler/runtime.mk

.PHONY: check measure clean

$(BUILD):
	mkdir -p $@

$(BUILD)/vector-library.raw.ll: $(SOURCES) $(WHITEFOOTC) | $(BUILD)
	$(WHITEFOOTC) --emit-llvm $(SOURCES) -o $@

$(BUILD)/vector-library-normal.ll: $(BUILD)/vector-library.raw.ll
	sed -e 's/@malloc(/@wf_cost_allocate(/g' \
	    -e 's/@free(/@wf_cost_release(/g' \
	    -e 's/@main(/@wf_fixture_main(/g' $< > $@

$(BUILD)/vector-library-retained.ll: $(BUILD)/vector-library-normal.ll
	awk '/^define .* @(wf_grow_vector_|wf_std\.collections\.vector\.grow_vector_|wf_vector_library_(work|round|tail_work)|wf_vector_(word|record)_(make|accept))/ { if (sub(/ #0 [{]$$/, " noinline #0 {") != 1) exit 1; retained++ } { print } END { if (retained == 0) exit 1; print "retained Vector helpers: " retained > "/dev/stderr" }' $< > $@

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
	@set -eu; normal_calls=`grep -Ec 'call .*@"?(wf_grow_vector_|wf_std\.collections\.vector\.grow_vector_)' $(BUILD)/vector-library-normal.opt.ll || true`; \
	 retained_calls=`grep -Ec 'call .*@"?(wf_grow_vector_|wf_std\.collections\.vector\.grow_vector_)' $(BUILD)/vector-library-retained.opt.ll || true`; \
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
