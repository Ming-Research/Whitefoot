SHELL := /bin/sh
ROOT := $(abspath ../../../..)
EXE := $(if $(filter Windows_NT,$(OS)),.exe,)
WHITEFOOTC ?= $(ROOT)/compiler/target/gate/whitefootc$(EXE)
CLANG ?= clang
BUILD ?= .build
SOURCES := deque-library.wf
.DELETE_ON_ERROR:

include ../../../../compiler/runtime.mk

.PHONY: check measure probe-scalar-gep clean

$(BUILD):
	mkdir -p $@

$(BUILD)/deque-library.raw.ll: $(SOURCES) $(WHITEFOOTC) Makefile | $(BUILD)
	$(WHITEFOOTC) --emit-llvm $(SOURCES) -o $@

$(BUILD)/deque-library-normal.ll: $(BUILD)/deque-library.raw.ll
	sed -e 's/@malloc(/@wf_cost_allocate(/g' -e 's/@free(/@wf_cost_release(/g' -e 's/@main(/@wf_fixture_main(/g' \
	    -e 's/^declare ptr @wf_cost_allocate(i64)$$/declare noalias nonnull ptr @wf_cost_allocate(i64)/' $< > $@

$(BUILD)/deque-library-retained.ll: $(BUILD)/deque-library-normal.ll
	awk '/^define .* @(wf_deque_|wf_std\.collections\.deque\.deque_)/ { if (sub(/ #0 [{]$$/, " noinline #0 {") != 1) exit 1; retained++ } { print } END { if (retained == 0) exit 1; print "retained Deque helpers: " retained > "/dev/stderr" }' $< > $@

$(BUILD)/deque-library-%.opt.ll: $(BUILD)/deque-library-%.ll
	$(CLANG) -O2 -Wno-override-module -x ir -S -emit-llvm $< -o $@

$(BUILD)/control-%.opt.ll: deque-costs.c | $(BUILD)
	@set -eu; case '$*' in normal) flags= ;; retained) flags=-DRETAIN_HELPERS ;; *) exit 1 ;; esac; \
	  $(CLANG) -std=c11 -O2 -Wall -Wextra -Werror $$flags -S -emit-llvm $< -o $@

$(BUILD)/deque-costs-%$(EXE): deque-costs.c $(BUILD)/deque-library-%.ll $(NATIVE_OBJECTS)
	@set -eu; case '$*' in normal) flags= ;; retained) flags=-DRETAIN_HELPERS ;; *) exit 1 ;; esac; \
	  $(CLANG) -std=c11 -O2 -Wall -Wextra -Werror -Wno-override-module $$flags $< $(BUILD)/deque-library-$*.ll $(NATIVE_OBJECTS) $(NATIVE_LINK_FLAGS) -o $@

check: $(addprefix $(BUILD)/deque-costs-,normal$(EXE) retained$(EXE)) \
       $(addprefix $(BUILD)/deque-library-,normal.opt.ll retained.opt.ll) \
       $(addprefix $(BUILD)/control-,normal.opt.ll retained.opt.ll)
	@grep -Eq '^declare noalias nonnull ptr @wf_cost_allocate\(i64\)' $(BUILD)/deque-library-normal.ll
	$(BUILD)/deque-costs-normal$(EXE) check
	$(BUILD)/deque-costs-retained$(EXE) check
	@set -eu; wf=`grep -Ec 'call .*@"?(wf_deque_|wf_std\.collections\.deque\.deque_)(push|pop|rebase|drain)' $(BUILD)/deque-library-retained.opt.ll || true`; \
	  control=`grep -Ec 'call .*@(word|record)_(loop|bulk)_(push|pop|rebase|drain)' $(BUILD)/control-retained.opt.ll || true`; \
	  test "$$wf" -gt 0; test "$$control" -gt 0; \
	  echo "Retained Deque helper call sites: WF=$$wf C=$$control"

measure: check
	@set -eu; output=$(BUILD)/measurements.csv; \
	  echo 'run,contract,cohort,element_bytes,path,count,variant,sample,rounds,traces,elapsed_ns,checksum,requests,requested_bytes,peak_bytes' > "$$output"; \
	  for run in 0 1; do \
	    if test "$$run" = 0; then modes='normal retained'; else modes='retained normal'; fi; \
	    for mode in $$modes; do \
	      $(BUILD)/deque-costs-$$mode$(EXE) measure > $(BUILD)/run-$$run-$$mode.csv; \
	      awk -v run="$$run" 'NR > 1 { print run "," $$0 }' $(BUILD)/run-$$run-$$mode.csv >> "$$output"; \
	    done; \
	  done; \
	  echo "Raw samples: $$output"

# Optional causal probe over the four positive-stride u64 Ring endpoint GEPs.
# Compare the unsigned GEP flag with an assumption on the actual index.
# Run under the repository guard. This requires local Clang support for GEP
# `nuw`; it neither changes compiler emission nor promises optimizer parity.
probe-scalar-gep: $(BUILD)/deque-library-normal.ll deque-costs.c $(NATIVE_OBJECTS)
	@perl -e 'use strict; use warnings; local $$/; \
	  my ($$source, $$build) = @ARGV; \
	  open my $$in, "<", $$source or die $$!; my $$baseline = <$$in>; \
	  my $$pattern = qr/^([ \t]*)(%[-a-zA-Z0-9._]+) = getelementptr inbounds (\{ i64, i64, i64, \[0 x i64\] \}), ptr ([^,\n]+), i64 0, i32 3, i64 ([^\n]+)$$/m; \
	  my $$nuw = $$baseline; \
	  my $$nuw_count = ($$nuw =~ s/$$pattern/$$1$$2 = getelementptr inbounds nuw $$3, ptr $$4, i64 0, i32 3, i64 $$5/g); \
	  my $$split = $$baseline; \
	  my $$split_count = ($$split =~ s/$$pattern/$$1$$2.payload_base = getelementptr inbounds $$3, ptr $$4, i64 0, i32 3\n$$1$$2 = getelementptr inbounds [0 x i64], ptr $$2.payload_base, i64 0, i64 $$5/g); \
	  my $$nonnegative = $$baseline; \
	  die "scalar GEP probe requires an unmodified baseline without llvm.assume or probe suffixes\n" if $$baseline =~ /\@llvm\.assume\b|\.wf_probe_nonnegative\b/; \
	  my $$nonnegative_count = ($$nonnegative =~ s/$$pattern/$$1$$2.wf_probe_nonnegative = icmp sge i64 $$5, 0\n$${1}call void \@llvm.assume(i1 $$2.wf_probe_nonnegative)\n$$1$$2 = getelementptr inbounds $$3, ptr $$4, i64 0, i32 3, i64 $$5/g); \
	  $$nonnegative .= "\ndeclare void \@llvm.assume(i1)\n"; \
	  die "scalar GEP probe drift: expected 4 u64 Ring payload GEPs, got nuw=$$nuw_count split=$$split_count nonnegative=$$nonnegative_count\n" unless $$nuw_count == 4 && $$split_count == 4 && $$nonnegative_count == 4; \
	  for my $$variant (["baseline", $$baseline], ["nuw", $$nuw], ["split", $$split], ["nonnegative", $$nonnegative]) { \
	    open my $$out, ">", "$$build/deque-gep-$$variant->[0].ll" or die $$!; \
	    print {$$out} $$variant->[1]; close $$out or die $$!; \
	  } \
	  print "Scalar GEP probe: 4 u64 Ring payload GEPs selected per variant\n";' "$<" "$(BUILD)"
	@set -eu; for variant in baseline nuw split nonnegative; do \
	  source="$(BUILD)/deque-gep-$$variant.ll"; \
	  $(CLANG) -O2 -Wno-override-module -x ir -S -emit-llvm "$$source" -o "$(BUILD)/deque-gep-$$variant.opt.ll" || { \
	    echo "Scalar GEP probe $$variant failed; local Clang must accept the requested LLVM syntax, including GEP nuw. See its diagnostic above." >&2; exit 1; \
	  }; \
	  $(CLANG) -std=c11 -O2 -Wall -Wextra -Werror -Wno-override-module deque-costs.c "$$source" $(NATIVE_OBJECTS) $(NATIVE_LINK_FLAGS) -o "$(BUILD)/deque-gep-$$variant$(EXE)"; \
	  "$(BUILD)/deque-gep-$$variant$(EXE)" check > "$(BUILD)/deque-gep-$$variant.check.txt"; \
	  cat "$(BUILD)/deque-gep-$$variant.check.txt"; \
	done

clean:
	rm -rf $(BUILD)
