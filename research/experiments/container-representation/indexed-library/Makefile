SHELL := /bin/sh
ROOT := $(abspath ../../../..)
EXE := $(if $(filter Windows_NT,$(OS)),.exe,)
WHITEFOOTC ?= $(ROOT)/compiler/target/gate/whitefootc$(EXE)
CLANG ?= clang
BUILD ?= .build
HEAP_STYLE ?= shared
PLAIN_REVISION := c3c2a50fd
STORE := $(ROOT)/tests/programs/containers/indexed-store.wf
PLAIN_SOURCE := $(BUILD)/plain-priority-queue.wf
ifeq ($(HEAP_STYLE),standalone)
QUEUE_SOURCE := $(PLAIN_SOURCE)
CONTROL_SOURCE := standalone-indexed.wf
else ifeq ($(HEAP_STYLE),shared)
# tests/programs/containers/indexed-store.wf already aliases the shared
# queue's names from std::collections::priority_queue, so the shared style
# needs no separate queue source of its own.
QUEUE_SOURCE :=
CONTROL_SOURCE :=
else
$(error HEAP_STYLE must be shared or standalone)
endif
# indexed-store.wf aliases Slab, HashMap and (for the shared style)
# PriorityQueue from the standard library itself; indexed-library.wf aliases
# ExitStatus/exit_status the same way (an alias binds in its own record
# alone [MOD-4], so each file that writes a bare standard-library name
# aliases it itself).
SOURCES := $(QUEUE_SOURCE) $(BUILD)/indexed-store.wf $(CONTROL_SOURCE) indexed-library.wf
.DELETE_ON_ERROR:
.DEFAULT_GOAL := check
include $(ROOT)/compiler/runtime.mk

.PHONY: admission native-build native-check build check check-built events measure-pair plain-build clean

$(BUILD):
	mkdir -p $@

$(PLAIN_SOURCE): Makefile | $(BUILD)
	git -C $(ROOT) show $(PLAIN_REVISION):lib/containers/priority-queue.wf > $@
	@test "$$(shasum -a 256 $@ | cut -d' ' -f1)" = 8a9f7a529ec61db4b888ce866f4b687b35f8226ccce94de668ee68f35cee08fe

$(BUILD)/indexed-store.wf: $(STORE) Makefile | $(BUILD)
	cp $< $@

$(BUILD)/indexed-native-%$(EXE): indexed-costs.c Makefile | $(BUILD)
	@set -eu; case '$*' in normal) flags= ;; retained) flags=-DRETAIN_HELPERS ;; events) flags=-DAUDIT_EVENTS ;; *) exit 1 ;; esac; \
	  $(CLANG) -std=c11 -O2 -Wall -Wextra -Werror $$flags $< -o $@

native-build: $(addprefix $(BUILD)/indexed-native-,normal$(EXE) retained$(EXE))

native-check: native-build
	$(BUILD)/indexed-native-normal$(EXE) check
	$(BUILD)/indexed-native-retained$(EXE) check

$(BUILD)/indexed-library.raw.ll: $(SOURCES) $(WHITEFOOTC) Makefile | $(BUILD)
	$(WHITEFOOTC) --emit-llvm $(SOURCES) -o $@

admission: $(BUILD)/indexed-library.raw.ll

$(BUILD)/indexed-library-normal.ll: $(BUILD)/indexed-library.raw.ll Makefile
	sed -e 's/@malloc(/@wf_cost_allocate(/g' -e 's/@free(/@wf_cost_release(/g' -e 's/@main(/@wf_fixture_main(/g' \
	    -e 's/^declare ptr @wf_cost_allocate(i64)$$/declare noalias nonnull ptr @wf_cost_allocate(i64)/' $< | \
	  awk '/^define .* @wf_indexed_cost_(small|wide)_trace\(/ { if (sub(/ #0 [{]$$/, " noinline #0 {") != 1) exit 1; traces++ } \
	    /^define / && !/^define (private|internal|weak) / && !/ @wf__/ && !/ @wf_indexed_cost_(small|wide)_trace\(/ { sub(/^define /, "define internal "); closed++ } \
	    { print } END { if (traces != 2 || closed == 0) exit 1 }' > $@

# Composite public operations and actual callbacks are the noinline policy,
# not dynamic exports. Trace bridges and runtime entry symbols remain exported.
# Private library operations/sifts remain eligible for ordinary optimization;
# the plain-PQ comparison retains its own existing public queue policy.
$(BUILD)/indexed-library-retained.ll: $(BUILD)/indexed-library-normal.ll Makefile
	awk '/^define .* @wf_record_store_/ || /^define .* @wf_record_(id_hash|id_equal|due_compare|read_info|set_position|set_id_member|swap_payload|report_position|read_handle|read_due|discard_id|discard_due)[$$(]/ || /^define .* @wf_indexed_cost_(small|wide)_(make|observe|consume)\(/ || /^define .* @wf_indexed_cost_(observe|consume)[$$(]/ \
	  { if (sub(/ #0 [{]$$/, " noinline #0 {") != 1) exit 1; retained++ } \
	  { print } END { if (retained == 0) exit 1; print "Retained composite/callback definitions: " retained > "/dev/stderr" }' $< > $@

$(BUILD)/indexed-library-%.opt.ll: $(BUILD)/indexed-library-%.ll
	$(CLANG) -O2 -Wno-override-module -x ir -S -emit-llvm $< -o $@

$(BUILD)/control-%.opt.ll: indexed-costs.c Makefile | $(BUILD)
	@set -eu; case '$*' in normal) flags= ;; retained) flags=-DRETAIN_HELPERS ;; *) exit 1 ;; esac; \
	  $(CLANG) -std=c11 -O2 -Wall -Wextra -Werror -DWITH_WF $$flags -S -emit-llvm $< -o $@

$(BUILD)/indexed-costs-%$(EXE): indexed-costs.c $(BUILD)/indexed-library-%.ll $(NATIVE_OBJECTS)
	@set -eu; case '$*' in normal) flags= ;; retained) flags=-DRETAIN_HELPERS ;; *) exit 1 ;; esac; \
	  $(CLANG) -std=c11 -O2 -Wall -Wextra -Werror -DWITH_WF -Wno-override-module $$flags $< $(BUILD)/indexed-library-$*.ll $(NATIVE_OBJECTS) $(NATIVE_LINK_FLAGS) -o $@

build: $(addprefix $(BUILD)/indexed-costs-,normal$(EXE) retained$(EXE)) \
       $(addprefix $(BUILD)/indexed-library-,normal.opt.ll retained.opt.ll) \
       $(addprefix $(BUILD)/control-,normal.opt.ll retained.opt.ll)

check: build check-built

check-built:
	$(BUILD)/indexed-costs-normal$(EXE) check
	$(BUILD)/indexed-costs-retained$(EXE) check
	@set -eu; for operation in new insert info visit lookup attach detach position schedule reschedule cancel replace delete expire free; do \
	  grep -Eq 'call .*@"?wf_record_store_'$$operation'[$$(]' $(BUILD)/indexed-library-retained.opt.ll; \
	  grep -Eq 'call .*@(small|wide)_(swap|hole)_'$$operation'[.(]' $(BUILD)/control-retained.opt.ll; \
	done; \
	echo 'Composite public operations survive in both retained modules'

events: $(BUILD)/indexed-native-events$(EXE)
	$(BUILD)/indexed-native-events$(EXE) events > $(BUILD)/events.csv
	@echo 'Algorithm-level C counts: $(BUILD)/events.csv'

# Use the unchanged existing plain matrix and trace. Each call chooses original
# or shared queue source explicitly; BUILD keeps their artifacts independent.
plain-build: $(PLAIN_SOURCE) $(NATIVE_OBJECTS)
	$(MAKE) -C ../priority-library build BUILD='$(abspath $(BUILD))/plain-original' \
	  WHITEFOOTC='$(WHITEFOOTC)' CLANG='$(CLANG)' \
	  NATIVE_OBJECTS='$(abspath $(NATIVE_OBJECTS))' \
	  SOURCES='$(abspath $(PLAIN_SOURCE)) $(abspath ../priority-library/priority-library.wf)'
	$(MAKE) -C ../priority-library build BUILD='$(abspath $(BUILD))/plain-noop' \
	  WHITEFOOTC='$(WHITEFOOTC)' CLANG='$(CLANG)' NATIVE_OBJECTS='$(abspath $(NATIVE_OBJECTS))'

# Explicit research invocation only, after both images pass their complete
# checks. LEFT and RIGHT name already constructed image directories. AA uses
# the same unchanged directory twice. No construction or check time is hidden.
LEFT ?= $(BUILD)/standalone
RIGHT ?= $(BUILD)/shared
PAIR_KIND ?= indexed
SERIES ?= ab
PAIR_PROGRAM = $(if $(filter plain,$(PAIR_KIND)),priority-costs,indexed-costs)
measure-pair: | $(BUILD)
	@set -eu; case '$(PAIR_KIND)' in indexed|plain) ;; *) exit 1 ;; esac; \
	  case '$(SERIES)' in aa|ab|repeat-aa|repeat-ab) ;; *) exit 1 ;; esac; \
	  if test '$(PAIR_KIND)' = indexed; then case '$(SERIES)' in aa|repeat-aa) \
	    test ! -e '$(BUILD)/clock-$(SERIES).csv'; \
	    '$(LEFT)/indexed-costs-normal$(EXE)' quantum > '$(BUILD)/clock-$(SERIES).csv' ;; esac; fi; \
	  for cohort in 0 1; do \
	    if test "$$cohort" = 0; then modes='normal retained'; images='left right'; else modes='retained normal'; images='right left'; fi; \
	    for mode in $$modes; do \
	      for image in $$images; do \
	        if test "$$image" = left; then directory='$(LEFT)'; else directory='$(RIGHT)'; fi; \
	        output='$(BUILD)/$(PAIR_KIND)-$(SERIES)-'$$cohort-$$mode-$$image.csv; \
	        test ! -e "$$output"; \
	        "$$directory/$(PAIR_PROGRAM)-$$mode$(EXE)" measure $$cohort > "$$output"; \
	      done; \
	    done; \
	  done; \
	  echo 'Raw paired samples: $(BUILD)/$(PAIR_KIND)-$(SERIES)-*.csv'

clean:
	rm -rf $(BUILD)
