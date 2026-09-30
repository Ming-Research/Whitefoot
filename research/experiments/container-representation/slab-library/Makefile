SHELL := /bin/sh
ROOT := $(abspath ../../../..)
EXE := $(if $(filter Windows_NT,$(OS)),.exe,)
WHITEFOOTC ?= $(ROOT)/compiler/target/gate/whitefootc$(EXE)
CLANG ?= clang
BUILD ?= .build
SOURCES := slab-library.wf
.DELETE_ON_ERROR:

include ../../../../compiler/runtime.mk

.PHONY: check measure clean

$(BUILD):
	mkdir -p $@

$(BUILD)/slab-library.raw.ll: $(SOURCES) $(WHITEFOOTC) Makefile | $(BUILD)
	$(WHITEFOOTC) --emit-llvm $(SOURCES) -o $@

$(BUILD)/slab-library-normal.ll: $(BUILD)/slab-library.raw.ll
	sed -e 's/@malloc(/@wf_cost_allocate(/g' -e 's/@free(/@wf_cost_release(/g' -e 's/@main(/@wf_fixture_main(/g' \
	    -e 's/^declare ptr @wf_cost_allocate(i64)$$/declare noalias nonnull ptr @wf_cost_allocate(i64)/' $< > $@

$(BUILD)/slab-library-retained.ll: $(BUILD)/slab-library-normal.ll
	awk '/^define .* @(wf_slab_|wf_std\.collections\.slab\.slab_)/ { if (sub(/ #0 [{]$$/, " noinline #0 {") != 1) exit 1; retained++ } { print } END { if (retained == 0) exit 1; print "retained Slab helpers: " retained > "/dev/stderr" }' $< > $@

$(BUILD)/slab-library-%.opt.ll: $(BUILD)/slab-library-%.ll
	$(CLANG) -O2 -Wno-override-module -x ir -S -emit-llvm $< -o $@

$(BUILD)/control-%.opt.ll: slab-costs.c | $(BUILD)
	@set -eu; case '$*' in normal) flags= ;; retained) flags=-DRETAIN_HELPERS ;; *) exit 1 ;; esac; \
	  $(CLANG) -std=c11 -O2 -Wall -Wextra -Werror $$flags -S -emit-llvm $< -o $@

$(BUILD)/slab-costs-%$(EXE): slab-costs.c $(BUILD)/slab-library-%.ll $(NATIVE_OBJECTS)
	@set -eu; case '$*' in normal) flags= ;; retained) flags=-DRETAIN_HELPERS ;; *) exit 1 ;; esac; \
	  $(CLANG) -std=c11 -O2 -Wall -Wextra -Werror -Wno-override-module $$flags $< $(BUILD)/slab-library-$*.ll $(NATIVE_OBJECTS) $(NATIVE_LINK_FLAGS) -o $@

check: $(addprefix $(BUILD)/slab-costs-,normal$(EXE) retained$(EXE)) \
       $(addprefix $(BUILD)/slab-library-,normal.opt.ll retained.opt.ll) \
       $(addprefix $(BUILD)/control-,normal.opt.ll retained.opt.ll)
	@grep -Eq '^declare noalias nonnull ptr @wf_cost_allocate\(i64\)' $(BUILD)/slab-library-normal.ll
	$(BUILD)/slab-costs-normal$(EXE) check
	$(BUILD)/slab-costs-retained$(EXE) check
	@set -eu; wf=`grep -Ec 'call .*@"?(wf_slab_|wf_std\.collections\.slab\.slab_)(insert|visit|remove|free)' $(BUILD)/slab-library-retained.opt.ll || true`; \
	  control=`grep -Ec 'call .*@(word|record)_(window|tagged)_(insert|visit|remove|free)' $(BUILD)/control-retained.opt.ll || true`; \
	  test "$$wf" -gt 0; test "$$control" -gt 0; \
	  echo "Retained Slab helper call sites: WF=$$wf C=$$control"

measure: check
	@set -eu; output=$(BUILD)/measurements.csv; \
	  echo 'run,contract,cohort,element_bytes,path,count,variant,sample,rounds,traces,elapsed_ns,checksum,requests,requested_bytes,peak_bytes' > "$$output"; \
	  for run in 0 1; do \
	    if test "$$run" = 0; then modes='normal retained'; else modes='retained normal'; fi; \
	    for mode in $$modes; do \
	      $(BUILD)/slab-costs-$$mode$(EXE) measure > $(BUILD)/run-$$run-$$mode.csv; \
	      awk -v run="$$run" 'NR > 1 { print run "," $$0 }' $(BUILD)/run-$$run-$$mode.csv >> "$$output"; \
	    done; \
	  done; \
	  echo "Raw samples: $$output"

clean:
	rm -rf $(BUILD)
