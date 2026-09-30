SHELL := /bin/sh
ROOT := $(abspath ../../../..)
EXE := $(if $(filter Windows_NT,$(OS)),.exe,)
WHITEFOOTC ?= $(ROOT)/compiler/target/gate/whitefootc$(EXE)
CLANG ?= clang
BUILD ?= .build
SAMPLES ?= $(BUILD)/measurements.csv
SOURCES := priority-library.wf
STD_SOURCES := $(wildcard $(ROOT)/lib/std/collections/priority_queue/* $(ROOT)/lib/std/process/*)
.DELETE_ON_ERROR:
.DEFAULT_GOAL := check

include $(ROOT)/compiler/runtime.mk

ECO_FAMILY := priority
ECO_RUST_SOURCE := priority-rust.rs
ECO_CPP_SOURCE := priority-cpp.cc
include ../ecosystem.mk
ECO_WORK ?= 16
.SECONDARY: $(foreach mode,timed account,$(addprefix $(ECO_BUILD)/,driver-$(mode).o whitefoot-$(mode).o cpp-$(mode).o rust-$(mode).a))

.PHONY: native-check build check measure events summarize clean \
        ecosystem-build ecosystem-check ecosystem-account ecosystem-measure

$(BUILD):
	mkdir -p $@

$(BUILD)/priority-native-%$(EXE): priority-costs.c Makefile | $(BUILD)
	@set -eu; case '$*' in normal) flags= ;; retained) flags=-DRETAIN_HELPERS ;; events) flags=-DAUDIT_EVENTS ;; *) exit 1 ;; esac; \
	  $(CLANG) -std=c11 -O2 -Wall -Wextra -Werror $$flags $< -o $@

native-check: $(addprefix $(BUILD)/priority-native-,normal$(EXE) retained$(EXE))
	$(BUILD)/priority-native-normal$(EXE) check
	$(BUILD)/priority-native-retained$(EXE) check

$(BUILD)/priority-library.raw.ll: $(SOURCES) $(STD_SOURCES) $(WHITEFOOTC) Makefile | $(BUILD)
	$(WHITEFOOTC) --emit-llvm $(SOURCES) -o $@

$(BUILD)/priority-library-normal.ll: $(BUILD)/priority-library.raw.ll Makefile
	sed -e 's/@malloc(/@wf_cost_allocate(/g' -e 's/@free(/@wf_cost_release(/g' -e 's/@main(/@wf_fixture_main(/g' \
	    -e 's/^declare ptr @wf_cost_allocate(i64)$$/declare noalias nonnull ptr @wf_cost_allocate(i64)/' $< | \
	  awk '/^define .* @wf_priority_cost_(word|record)_trace\(/ { if (sub(/ #0 [{]$$/, " noinline #0 {") != 1) exit 1; traces++ } { print } END { if (traces != 2) exit 1 }' > $@

$(BUILD)/priority-library-retained.ll: $(BUILD)/priority-library-normal.ll Makefile
	awk '(/^define .* @"?wf_(std[.]collections[.]priority_queue[.])?priority_queue_(new|len|reserve|push|peek|pop|replace_top|heapify|drain|free)[$$(]/ || /^define .* @"?wf_priority_cost_cleanup[$$(]/ || /^define .* @wf_priority_cost_(word|record)_(make|accept|compare|observe)\(/) && !/[.]body"?[(]/ \
	  { if (sub(/ #0 [{]$$/, " noinline #0 {") != 1) exit 1; retained++ } \
	  { print } END { if (retained == 0) exit 1; print "Retained queue/callback definitions: " retained > "/dev/stderr" }' $< > $@

$(BUILD)/priority-library-%.opt.ll: $(BUILD)/priority-library-%.ll
	$(CLANG) -O2 -Wno-override-module -x ir -S -emit-llvm $< -o $@

$(BUILD)/control-%.opt.ll: priority-costs.c Makefile | $(BUILD)
	@set -eu; case '$*' in normal) flags= ;; retained) flags=-DRETAIN_HELPERS ;; *) exit 1 ;; esac; \
	  $(CLANG) -std=c11 -O2 -Wall -Wextra -Werror -DWITH_WF $$flags -S -emit-llvm $< -o $@

$(BUILD)/priority-costs-%$(EXE): priority-costs.c $(BUILD)/priority-library-%.ll $(NATIVE_OBJECTS)
	@set -eu; case '$*' in normal) flags= ;; retained) flags=-DRETAIN_HELPERS ;; *) exit 1 ;; esac; \
	  $(CLANG) -std=c11 -O2 -Wall -Wextra -Werror -DWITH_WF -Wno-override-module $$flags $< $(BUILD)/priority-library-$*.ll $(NATIVE_OBJECTS) $(NATIVE_LINK_FLAGS) -o $@

build: $(addprefix $(BUILD)/priority-costs-,normal$(EXE) retained$(EXE)) \
       $(addprefix $(BUILD)/priority-library-,normal.opt.ll retained.opt.ll) \
       $(addprefix $(BUILD)/control-,normal.opt.ll retained.opt.ll)

check: native-check build
	@grep -Eq '^declare noalias nonnull ptr @wf_cost_allocate\(i64\)' $(BUILD)/priority-library-normal.ll
	$(BUILD)/priority-costs-normal$(EXE) check
	$(BUILD)/priority-costs-retained$(EXE) check
	@set -eu; for operation in new len reserve push peek pop replace_top heapify drain free; do \
	  grep -Eq "call .*@\"?wf_(std[.]collections[.]priority_queue[.])?priority_queue_$${operation}"'[$$(]' $(BUILD)/priority-library-retained.opt.ll; \
	done; \
	for operation in new len reserve push peek pop replace_top heapify drain free; do \
	  grep -Eq "call .*@(word|record)_(swap|hole)_$${operation}[.(]" $(BUILD)/control-retained.opt.ll; \
	done; \
	for callback in make accept compare observe; do \
	  grep -Eq "call .*@wf_priority_cost_(word|record)_$${callback}[(]" $(BUILD)/priority-library-retained.opt.ll; \
	  grep -Eq "call .*@(word|record)_$${callback}[.(]" $(BUILD)/control-retained.opt.ll; \
	done; \
	grep -Eq 'call .*@"?wf_priority_cost_cleanup[$$(]' $(BUILD)/priority-library-retained.opt.ll; \
	echo 'Public queue operations and payload callbacks survive in both retained modules'; \
	wf=$$(grep -Ec 'call .*@"?wf_(std[.]collections[.]priority_queue[.])?priority_queue_(child|make_room|sift_rise|sift_sink)' $(BUILD)/priority-library-retained.opt.ll || true); \
	control=$$(grep -Ec 'call .*@(word|record)_(swap|hole)_(child|make_room|rise|sink)' $(BUILD)/control-retained.opt.ll || true); \
	echo "Ordinarily optimized private helper call sites: WF=$$wf C=$$control"

# Explicit research invocation only. The ordinary correctness gate has no
# dependency on this Makefile, either source, or its output.
measure: check
	@set -eu; output='$(SAMPLES)'; first=yes; \
	  for cohort in 0 1; do \
	    if test "$$cohort" = 0; then modes='normal retained'; else modes='retained normal'; fi; \
	    for mode in $$modes; do \
	      sample_file='$(BUILD)/cohort-'$$cohort-$$mode.csv; \
	      $(BUILD)/priority-costs-$$mode$(EXE) measure $$cohort > "$$sample_file"; \
	      if test "$$first" = yes; then head -n 1 "$$sample_file" > "$$output"; first=no; fi; \
	      awk 'NR > 1' "$$sample_file" >> "$$output"; \
	    done; \
	  done; \
	  echo "Raw samples: $$output"

events: $(BUILD)/priority-native-events$(EXE)
	$(BUILD)/priority-native-events$(EXE) events > $(BUILD)/events.csv
	@echo 'Algorithm-level C counts: $(BUILD)/events.csv'

summarize:
	@set -eu; \
	  awk -F, 'NR == 1 { if ($$1 != "contract" || $$14 != "peak_bytes") exit 1; next } NF != 14 { exit 1 } { print }' "$(SAMPLES)" > $(BUILD)/summary-input.csv; \
	  LC_ALL=C sort -t, -k1,6 -k10,10n $(BUILD)/summary-input.csv > $(BUILD)/summary-sorted.csv; \
	  awk -F, 'BEGIN { OFS=","; print "contract,cohort,element_bytes,path,count,variant,rounds,traces,median_elapsed_ns,ns_per_item,min_elapsed_ns,max_elapsed_ns" } \
	    function emit() { if (samples != 7) { print "expected seven samples: " key > "/dev/stderr"; exit 1 } \
	      denominator = population * (rounds == 0 ? traces : rounds); \
	      print key, rounds, traces, median, median / denominator, minimum, maximum } \
	    { nextkey = $$1; for (i=2; i<=6; i++) nextkey=nextkey OFS $$i; \
	      if (NR > 1 && nextkey != key) emit(); \
	      if (NR == 1 || nextkey != key) { key=nextkey; samples=0; population=$$5; rounds=$$8; traces=$$9; minimum=$$10 } \
	      if ($$8 != rounds || $$9 != traces) { print "inconsistent trace sizes: " key > "/dev/stderr"; exit 1 } \
	      samples++; if (samples == 4) median=$$10; maximum=$$10 } \
	    END { if (NR > 0) emit() }' $(BUILD)/summary-sorted.csv > $(BUILD)/medians.csv; \
	  rm $(BUILD)/summary-input.csv $(BUILD)/summary-sorted.csv; \
	  echo 'Grouped medians: $(BUILD)/medians.csv'

# Practical builds preserve only the complete-trace call boundary. Queue
# operations, callbacks and native container algorithms optimize ordinarily.
# Timed WF/C allocations remain plain malloc/free; only accounting redirects.
$(ECO_BUILD)/priority-library-timed.ll: $(BUILD)/priority-library.raw.ll Makefile $(ECO_CONFIG) | $(ECO_BUILD)
	sed 's/@main(/@wf_fixture_main(/g' $< | \
	  awk '/^define .* @wf_priority_cost_(word|record)_trace\(/ { if (sub(/ #0 [{]$$/, " noinline #0 {") != 1) exit 1; traces++ } { print } END { if (traces != 2) exit 1 }' > $@

$(ECO_BUILD)/priority-library-account.ll: $(ECO_BUILD)/priority-library-timed.ll Makefile
	sed -e 's/@malloc(/@wf_cost_allocate(/g' -e 's/@free(/@wf_cost_release(/g' \
	    -e 's/^declare ptr @wf_cost_allocate(i64)$$/declare noalias nonnull ptr @wf_cost_allocate(i64)/' $< > $@

$(ECO_BUILD)/whitefoot-%.o: $(ECO_BUILD)/priority-library-%.ll $(ECO_CONFIG)
	$(CLANG) $(ECO_LDFLAGS) -Wno-override-module -x ir -c $< -o $@

$(ECO_BUILD)/driver-%.o: priority-costs.c Makefile $(ECO_CONFIG) | $(ECO_BUILD)
	@set -eu; case '$*' in timed) account= ;; account) account=-DACCOUNT_ONLY ;; *) exit 1 ;; esac; \
	  $(CLANG) $(ECO_CPPFLAGS) $(ECO_CFLAGS) -DWITH_WF $$account -c $< -o $@

$(ECO_BUILD)/priority-%$(EXE): $(ECO_BUILD)/driver-%.o $(ECO_BUILD)/whitefoot-%.o \
                             $(ECO_BUILD)/cpp-%.o $(ECO_BUILD)/rust-%.a $(NATIVE_OBJECTS) $(ECO_CONFIG)
	$(CXX) $(ECO_LDFLAGS) $(filter %.o %.a,$^) $(NATIVE_LINK_FLAGS) $(ECO_RUST_LINK_FLAGS) -o $@

ecosystem-build: $(ECO_BUILD)/priority-timed$(EXE) $(ECO_BUILD)/priority-account$(EXE)

ecosystem-check: ecosystem-build
	$(ECO_BUILD)/priority-timed$(EXE) check
	$(ECO_BUILD)/priority-account$(EXE) check
	@! grep -q 'wf_cost_allocate\|wf_cost_release\|wf_ecosystem_note_' $(ECO_BUILD)/priority-library-timed.ll
	@grep -Eq '^declare noalias nonnull ptr @wf_cost_allocate\(i64\)' $(ECO_BUILD)/priority-library-account.ll
	@set -eu; set +e; $(ECO_BUILD)/priority-timed$(EXE) negative-checksum > $(ECO_BUILD)/negative-checksum.log 2>&1; status=$$?; set -e; \
	  test "$$status" = 1; grep -Fx 'priority library costs: independent sorted-sequence checksum' $(ECO_BUILD)/negative-checksum.log
	@set -eu; set +e; $(ECO_BUILD)/priority-account$(EXE) negative-accounting > $(ECO_BUILD)/negative-accounting.log 2>&1; status=$$?; set -e; \
	  test "$$status" = 1; grep -Fx 'priority library costs: every observed backing request is released or reallocated' $(ECO_BUILD)/negative-accounting.log

ecosystem-account: ecosystem-build
	$(ECO_BUILD)/priority-account$(EXE) account > '$(ECO_ACCOUNT)'
	@echo 'Allocation observations: $(ECO_ACCOUNT)'

ecosystem-measure: ecosystem-build
	@set -eu; output='$(ECO_SAMPLE_FILE)'; first=yes; \
	  for cohort in 0 1; do \
	    sample_file='$(ECO_BUILD)/cohort-'$$cohort.csv; \
	    $(ECO_BUILD)/priority-timed$(EXE) measure $$cohort $(ECO_WORK) > "$$sample_file"; \
	    if test "$$first" = yes; then head -n 1 "$$sample_file" > "$$output"; first=no; fi; \
	    awk 'NR > 1' "$$sample_file" >> "$$output"; \
	  done; \
	  echo "Raw ecosystem samples: $$output"

clean:
	rm -rf $(BUILD)
