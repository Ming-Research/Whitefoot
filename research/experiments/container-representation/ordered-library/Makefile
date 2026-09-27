SHELL := /bin/sh
ROOT := $(abspath ../../../..)
EXE := $(if $(filter Windows_NT,$(OS)),.exe,)
WHITEFOOTC ?= $(ROOT)/compiler/target/gate/whitefootc$(EXE)
WHITEFOOTC_LABEL ?= configured-whitefootc
CLANG ?= clang
BUILD ?= .build
LIBRARY_SOURCE ?= $(ROOT)/lib/std/collections/ordered_map/ordered-map.wf
LIBRARY_LABEL ?= lib/std/collections/ordered_map/ordered-map.wf
SOURCES := ordered-library.wf
INSERTION_BASELINE_REV ?= 1811f2d01
INSERTION_RUN ?= initial
INSERTION_CANDIDATE ?= pair
ifeq ($(INSERTION_CANDIDATE),pair)
INSERTION_PREFIX := insertion
INSERTION_PATCH := insertion-candidate.patch
INSERTION_SHA := ddc53f3bd12e3682c26ea72f33d94da5da787371afb7461fd7d69d1798aca4ec
else ifeq ($(INSERTION_CANDIDATE),pair-promotion)
INSERTION_PREFIX := promotion
INSERTION_PATCH := promotion-candidate.patch
INSERTION_SHA := 5514ce2aecdf2a8074dc8897b0457d6858e7ecf16d2d2fa559dfefb6420f2c8f
else
$(error INSERTION_CANDIDATE must be pair or pair-promotion)
endif
CONTROLS := word-source record-source word-direct record-direct word-avl record-avl
MODES := normal retained
CFLAGS := -std=c11 -O2 -Wall -Wextra -Werror
.DEFAULT_GOAL := check
.DELETE_ON_ERROR:
.SECONDARY:

include ../../../../compiler/runtime.mk

.PHONY: build check verify measure measure-only native-check structural-check identities clean
.PHONY: insertion-build insertion-verify insertion-measure-only insertion-identities

$(BUILD):
	mkdir -p $@

$(BUILD)/ordered-library.raw.ll: $(SOURCES) $(LIBRARY_SOURCE) $(WHITEFOOTC) Makefile | $(BUILD)
	$(WHITEFOOTC) --emit-llvm $(SOURCES) -o $@

$(BUILD)/ordered-library-normal.ll: $(BUILD)/ordered-library.raw.ll
	sed -e 's/@malloc(/@wf_cost_allocate(/g' -e 's/@free(/@wf_cost_release(/g' -e 's/@main(/@wf_fixture_main(/g' \
	    -e 's/^declare ptr @wf_cost_allocate(i64)$$/declare noalias nonnull ptr @wf_cost_allocate(i64)/' $< | \
	  awk '/^define / && !/^define (internal|private|weak) / && /@wf_/ && !/@wf__/ && \
	       !/@wf_ordered_cost_(word|record)_trace[(]/ { sub(/^define /, "define internal ") } { print }' > $@

# Retain exactly the public map operations and supplied callbacks. The private
# node helpers, aggregate-result .body implementations, trace, digest helpers,
# and payload-content helpers stay ordinary.
$(BUILD)/ordered-library-retained.ll: $(BUILD)/ordered-library-normal.ll Makefile
	awk '(/^define .* @"?wf_std[.]collections[.]ordered_map[.]ordered_map_(new|len|put|lookup|edit|remove|each|range|free)([$$(]|[[:space:]])/ || \
	      /^define .* @"?wf_ordered_cost_(compare|word_(make|observe|visit|edit|consume)|record_(make|observe|visit|edit|consume))[$$(]/) && !/[.]body"?[(]/ \
	     { if (sub(/ #0 [{]$$/, " noinline #0 {") != 1) exit 1; retained++ } \
	     { print } END { if (retained == 0) exit 1; print "retained public WF operations/callbacks: " retained > "/dev/stderr" }' $< > $@

$(BUILD)/ordered-library-%.opt.ll: $(BUILD)/ordered-library-%.ll
	$(CLANG) -O2 -Wno-override-module -x ir -S -emit-llvm $< -o $@

# One specialization per C translation unit gives C and WF the same opportunity
# to optimize private code without an indirect comparator or payload-size loop.
$(BUILD)/control-%.ll: ordered-costs.c Makefile | $(BUILD)
	@set -eu; stem='$*'; element=$${stem%%-*}; tail=$${stem#*-}; shape=$${tail%%-*}; mode=$${tail#*-}; \
	  flags='-DCONTROL_TEMPLATE'; prefix=$${element}_$${shape}; \
	  case "$$element" in word) ;; record) flags="$$flags -DWIDE_PAIR" ;; *) exit 1 ;; esac; \
	  case "$$shape" in source) flags="$$flags -DSOURCE_CONTROL" ;; direct) ;; avl) flags="$$flags -DAVL_CONTROL" ;; *) exit 1 ;; esac; \
	  case "$$mode" in normal) ;; retained) flags="$$flags -DRETAIN_PUBLIC" ;; audit) flags="$$flags -DSTRUCTURAL_AUDIT" ;; *) exit 1 ;; esac; \
	  $(CLANG) $(CFLAGS) $$flags -DPREFIX=$$prefix -S -emit-llvm $< -o $@

define mode_rules
$(BUILD)/ordered-costs-$(1)$(EXE): ordered-costs.c $(addprefix $(BUILD)/control-,$(addsuffix -$(1).ll,$(CONTROLS))) $(BUILD)/ordered-library-$(1).ll $(NATIVE_OBJECTS)
	$(CLANG) $(CFLAGS) -Wno-override-module $(if $(filter retained,$(1)),-DRETAIN_PUBLIC,) $$^ $(NATIVE_LINK_FLAGS) -o $$@

$(BUILD)/native-costs-$(1)$(EXE): ordered-costs.c $(addprefix $(BUILD)/control-,$(addsuffix -$(1).ll,$(CONTROLS)))
	$(CLANG) $(CFLAGS) -DNATIVE_ONLY -Wno-override-module $(if $(filter retained,$(1)),-DRETAIN_PUBLIC,) $$^ -o $$@
endef
$(foreach mode,$(MODES),$(eval $(call mode_rules,$(mode))))

$(BUILD)/structural-costs$(EXE): ordered-costs.c $(BUILD)/control-word-source-audit.ll \
       $(addprefix $(BUILD)/control-,$(addsuffix -normal.ll,$(filter-out word-source,$(CONTROLS))))
	$(CLANG) $(CFLAGS) -DNATIVE_ONLY -DSTRUCTURAL_DRIVER -Wno-override-module $^ -o $@

build: $(addprefix $(BUILD)/ordered-costs-,$(addsuffix $(EXE),$(MODES))) \
       $(addprefix $(BUILD)/ordered-library-,$(addsuffix .opt.ll,$(MODES))) $(BUILD)/structural-costs$(EXE)

verify:
	@grep -Eq '^declare noalias nonnull ptr @wf_cost_allocate\(i64\)' $(BUILD)/ordered-library-normal.ll
	$(BUILD)/ordered-costs-normal$(EXE) check
	$(BUILD)/ordered-costs-retained$(EXE) check
	$(BUILD)/structural-costs$(EXE) structure > $(BUILD)/structural-paths.csv
	@set -eu; wf=`awk '/call .*@"?wf_std[.]collections[.]ordered_map[.]ordered_map_(put|lookup|edit|remove|each|range|free)[$$(]/ && !/[.]body"?[(]/ { count++ } END { print count + 0 }' $(BUILD)/ordered-library-retained.opt.ll`; \
	  test "$$wf" -gt 0; echo "Retained WF public-operation call sites: $$wf"; \
	  for source in $(BUILD)/control-*-retained.ll; do \
	    count=`grep -Ec 'call .*@(word|record)_(source|direct|avl)_(put|lookup|edit|remove|each|range|free)[(]' "$$source" || true`; \
	    test "$$count" -gt 0; echo "Retained C public-operation call sites: $$source $$count"; \
	  done

check: build
	$(MAKE) verify

native-check: $(addprefix $(BUILD)/native-costs-,$(addsuffix $(EXE),$(MODES))) $(BUILD)/structural-costs$(EXE)
	$(BUILD)/native-costs-normal$(EXE) native-check
	$(BUILD)/native-costs-retained$(EXE) native-check
	$(BUILD)/structural-costs$(EXE) structure > $(BUILD)/structural-paths.csv

structural-check: $(BUILD)/structural-costs$(EXE)
	$(BUILD)/structural-costs$(EXE) structure

measure: check
	$(MAKE) measure-only

# Sample 0 is the single warm-up; 1--5 are the prespecified five samples.
# Within every sample the four implementation positions reverse on odd samples.
measure-only:
	@set -eu; for mode in $(MODES); do \
	  $(BUILD)/ordered-costs-$$mode$(EXE) measure > $(BUILD)/measurements-$$mode.csv; \
	done; \
	  cat $(BUILD)/measurements-normal.csv > $(BUILD)/measurements.csv; \
	  sed '1d' $(BUILD)/measurements-retained.csv >> $(BUILD)/measurements.csv; \
	  echo "Raw samples including warm-up: $(BUILD)/measurements.csv"

identities:
	@set -eu; { git -C "$(ROOT)" rev-parse HEAD; git -C "$(ROOT)" status --short; uname -srm; \
	  $(CLANG) --version | sed '/^InstalledDir:/d'; \
	  shasum -a 256 Makefile ordered-costs.c ordered-library.wf; \
	  shasum -a 256 "$(LIBRARY_SOURCE)" | awk -v label='$(LIBRARY_LABEL)' '{ print $$1 "  " label }'; \
	  shasum -a 256 "$(WHITEFOOTC)" | awk -v label='$(WHITEFOOTC_LABEL)' '{ print $$1 "  " label }'; \
	  for file in $(BUILD)/ordered-library*.ll $(BUILD)/control-*.ll $(BUILD)/ordered-costs-*$(EXE); do \
	    shasum -a 256 "$$file" | awk -v label="build/$${file##*/}" '{ print $$1 "  " label }'; \
	  done; \
	} > $(BUILD)/identities.txt

# These are the five-path sources frozen for both registered source comparisons.
# A later driver revision needs its own criterion and evidence identities.
.PHONY: insertion-driver-identity
insertion-driver-identity:
	@printf '%s\n' \
	  'cd3c8cca4d1332360f4df4cee8f07df43cea030144d683b8044a51d7479be71b  ordered-costs.c' \
	  '779ffc6065c0c1b46cb0906ae1c14c167e32d385f37b4569ba35603710d90f4d  ordered-library.wf' | shasum -a 256 -c -

# Each candidate is evidence for its discriminator, reconstructed only in the
# build directory; it is not a second published library implementation.
$(BUILD)/$(INSERTION_PREFIX)-baseline-source.wf: Makefile | $(BUILD)
	git -C "$(ROOT)" show $(INSERTION_BASELINE_REV):lib/containers/ordered-map.wf > $@
	@actual=`shasum -a 256 $@ | awk '{ print $$1 }'`; \
	  test "$$actual" = affaee669a09320aafc9ec5badbea6c11fd95623e1ea8987ad9811742c4b70bb

$(BUILD)/$(INSERTION_PREFIX)-candidate-source.wf: $(BUILD)/$(INSERTION_PREFIX)-baseline-source.wf $(INSERTION_PATCH)
	patch -s -o $@ $< $(INSERTION_PATCH)
	@actual=`shasum -a 256 $@ | awk '{ print $$1 }'`; \
	  test "$$actual" = $(INSERTION_SHA)

insertion-build: insertion-driver-identity $(BUILD)/$(INSERTION_PREFIX)-baseline-source.wf $(BUILD)/$(INSERTION_PREFIX)-candidate-source.wf
	$(MAKE) build BUILD=$(BUILD)/$(INSERTION_PREFIX)-baseline \
	  LIBRARY_SOURCE=$(abspath $(BUILD)/$(INSERTION_PREFIX)-baseline-source.wf)
	$(MAKE) build BUILD=$(BUILD)/$(INSERTION_PREFIX)-candidate \
	  LIBRARY_SOURCE=$(abspath $(BUILD)/$(INSERTION_PREFIX)-candidate-source.wf)

insertion-verify:
	$(MAKE) verify BUILD=$(BUILD)/$(INSERTION_PREFIX)-baseline
	$(MAKE) verify BUILD=$(BUILD)/$(INSERTION_PREFIX)-candidate
	@set -eu; for file in $(BUILD)/$(INSERTION_PREFIX)-baseline/control-*.ll; do \
	  cmp "$$file" "$(BUILD)/$(INSERTION_PREFIX)-candidate/$${file##*/}"; \
	  done
	$(BUILD)/$(INSERTION_PREFIX)-baseline/ordered-costs-normal$(EXE) clock-resolution > $(BUILD)/$(INSERTION_PREFIX)-clock.csv

# Each fresh arm includes its own native controls. Cohort 1 reverses arm/mode
# order and the alternating within-cell implementation order. A/A precedes A/B.
# 2 comparisons * 2 cohorts * 2 arms * 2 modes * 30 cells * 6 samples * 4 variants.
insertion-measure-only:
	@set -eu; case '$(INSERTION_RUN)' in initial|repeat) ;; *) exit 1 ;; esac; \
	  output='$(BUILD)/$(INSERTION_PREFIX)-$(INSERTION_RUN).csv'; \
	  printf '%s\n' 'run,comparison,cohort,arm,image,contract,pair_bytes,path,count,variant,sample,rounds,traces,elapsed_ns,checksum,requests,requested_bytes,peak_nodes,peak_bytes' > "$$output"; \
	  for comparison in aa ab; do for cohort in 0 1; do \
	    modes='normal retained'; arms='a b'; command=measure; \
	    if test "$$cohort" = 1; then modes='retained normal'; arms='b a'; command=measure-reversed; fi; \
	    for mode in $$modes; do for arm in $$arms; do \
	      image=baseline; if test "$$comparison-$$arm" = ab-b; then image=candidate; fi; \
	      $(BUILD)/$(INSERTION_PREFIX)-$$image/ordered-costs-$$mode$(EXE) $$command > $(BUILD)/$(INSERTION_PREFIX)-arm.csv; \
	      awk -v prefix='$(INSERTION_RUN)'","$$comparison","$$cohort","$$arm","$$image 'NR > 1 { print prefix "," $$0 }' \
	        $(BUILD)/$(INSERTION_PREFIX)-arm.csv >> "$$output"; \
	    done; done; \
	  done; done; \
	  echo "Complete $(INSERTION_PREFIX) comparison: $$output"

insertion-identities:
	$(MAKE) identities BUILD=$(BUILD)/$(INSERTION_PREFIX)-baseline \
	  LIBRARY_SOURCE=$(abspath $(BUILD)/$(INSERTION_PREFIX)-baseline-source.wf) LIBRARY_LABEL=$(INSERTION_PREFIX)-baseline-source.wf
	$(MAKE) identities BUILD=$(BUILD)/$(INSERTION_PREFIX)-candidate \
	  LIBRARY_SOURCE=$(abspath $(BUILD)/$(INSERTION_PREFIX)-candidate-source.wf) LIBRARY_LABEL=$(INSERTION_PREFIX)-candidate-source.wf
	@set -eu; { \
	  printf '%s\n' 'Baseline image identities:'; cat $(BUILD)/$(INSERTION_PREFIX)-baseline/identities.txt; \
	  printf '%s\n' 'Candidate image identities:'; cat $(BUILD)/$(INSERTION_PREFIX)-candidate/identities.txt; \
	  shasum -a 256 $(INSERTION_PATCH); \
	  for file in $(BUILD)/$(INSERTION_PREFIX)-clock.csv $(BUILD)/$(INSERTION_PREFIX)-$(INSERTION_RUN).csv; do \
	    shasum -a 256 "$$file" | awk -v label="$${file##*/}" '{ print $$1 "  " label }'; \
	  done; \
	} > $(BUILD)/$(INSERTION_PREFIX)-$(INSERTION_RUN)-identities.txt

clean:
	rm -rf $(BUILD)

# Current library comparisons are separate from the frozen source-change runs
# above. Replay those historical targets from their recorded source/compiler
# checkout; their hashes intentionally still reject a migrated caller.
ECO_FAMILY := ordered
ECO_ABSL_PACKAGES := absl_btree
ECO_SCALE ?= 16
include ../ecosystem.mk

.PHONY: ecosystem-build ecosystem-check ecosystem-negative-check ecosystem-account ecosystem-measure ecosystem-measure-only ecosystem-identities

$(ECO_BUILD)/whitefoot.raw.ll: ordered-library.wf $(LIBRARY_SOURCE) $(WHITEFOOTC) $(ECO_CONFIG) Makefile | $(ECO_BUILD)
	$(WHITEFOOTC) --emit-llvm ordered-library.wf -o $@

# Preserve the allocator and ordinary optimizer visibility in timed images.
# The whole-trace bridges remain the only exported user entry points.
$(ECO_BUILD)/whitefoot-timed.ll: $(ECO_BUILD)/whitefoot.raw.ll Makefile
	sed -e 's/@main(/@wf_fixture_main(/g' $< | \
	  awk '/^define / && !/^define (internal|private|weak) / && /@"?wf_/ && !/@"?wf__/ && \
	       !/@wf_ordered_cost_(word|record)_trace[(]/ { sub(/^define /, "define internal ") } { print }' > $@

$(ECO_BUILD)/whitefoot-account.ll: $(ECO_BUILD)/whitefoot-timed.ll Makefile
	sed -e 's/@malloc(/@wf_cost_allocate(/g' -e 's/@free(/@wf_cost_release(/g' \
	    -e 's/^declare ptr @wf_cost_allocate(i64)$$/declare noalias nonnull ptr @wf_cost_allocate(i64)/' $< > $@

$(ECO_BUILD)/whitefoot-%.o: $(ECO_BUILD)/whitefoot-%.ll $(ECO_CONFIG)
	$(CLANG) -O3 -Wno-override-module -x ir -c $< -o $@

# These are the unchanged source-shaped, direct B-tree and AVL controls. Only
# the accounting image retains the allocation observer used by the old driver.
$(ECO_BUILD)/control-%.o: ordered-costs.c $(ECO_CONFIG) Makefile | $(ECO_BUILD)
	@set -eu; stem='$*'; element=$${stem%%-*}; tail=$${stem#*-}; shape=$${tail%%-*}; mode=$${tail#*-}; \
	  flags='-DCONTROL_TEMPLATE'; prefix=$${element}_$${shape}; \
	  case "$$element" in word) ;; record) flags="$$flags -DWIDE_PAIR" ;; *) exit 1 ;; esac; \
	  case "$$shape" in source) flags="$$flags -DSOURCE_CONTROL" ;; direct) ;; avl) flags="$$flags -DAVL_CONTROL" ;; *) exit 1 ;; esac; \
	  $(CLANG) $(ECO_CFLAGS) $$flags -DPREFIX=$$prefix -S -emit-llvm $< -o $@.raw.ll; \
	  case "$$mode" in \
	    timed) sed -e 's/@wf_cost_allocate(/@malloc(/g' -e 's/@wf_cost_release(/@free(/g' $@.raw.ll > $@.ll ;; \
	    account) cp $@.raw.ll $@.ll ;; *) exit 1 ;; \
	  esac; \
	  $(CLANG) -O3 -Wno-override-module -x ir -c $@.ll -o $@

$(ECO_BUILD)/driver-%.o: ordered-ecosystem.c ordered-costs.c $(ECO_CONFIG) Makefile | $(ECO_BUILD)
	@set -eu; case '$*' in timed) account= ;; account) account=-DACCOUNT_ONLY ;; *) exit 1 ;; esac; \
	  $(CLANG) $(ECO_CPPFLAGS) $(ECO_CFLAGS) $$account -c $< -o $@

define ecosystem_mode_rules
$(ECO_BUILD)/ordered-ecosystem-$(1)$(EXE): $(ECO_BUILD)/driver-$(1).o \
        $(addprefix $(ECO_BUILD)/control-,$(addsuffix -$(1).o,$(CONTROLS))) \
        $(ECO_BUILD)/whitefoot-$(1).o $(ECO_BUILD)/rust-$(1).a $(ECO_BUILD)/cpp-$(1).o $(NATIVE_OBJECTS)
	$(CXX) $(ECO_LDFLAGS) $$^ $(ECO_ABSL_LDFLAGS) $(ECO_RUST_LINK_FLAGS) $(NATIVE_LINK_FLAGS) -o $$@
endef
$(foreach mode,timed account,$(eval $(call ecosystem_mode_rules,$(mode))))

ecosystem-build: $(addprefix $(ECO_BUILD)/ordered-ecosystem-,$(addsuffix $(EXE),timed account))

ecosystem-check: ecosystem-build
	$(ECO_BUILD)/ordered-ecosystem-timed$(EXE) check
	$(ECO_BUILD)/ordered-ecosystem-account$(EXE) check
	$(MAKE) ecosystem-negative-check

ecosystem-negative-check: ecosystem-build
	@set -eu; for fault in checksum cleanup; do \
	  set +e; $(ECO_BUILD)/ordered-ecosystem-account$(EXE) check-fault-$$fault > $(ECO_BUILD)/fault-$$fault.log 2>&1; status=$$?; set -e; \
	  test "$$status" -ne 0; \
	  case "$$fault" in checksum) reason='ecosystem independent sorted sequence checksum' ;; cleanup) reason='ecosystem complete allocation cleanup' ;; esac; \
	  grep -Fq "$$reason" $(ECO_BUILD)/fault-$$fault.log; \
	  printf '%s\n' "Expected $$fault failure observed (status $$status)"; \
	  done

ecosystem-account: ecosystem-build
	$(ECO_BUILD)/ordered-ecosystem-account$(EXE) account > '$(ECO_ACCOUNT)'

ecosystem-measure: ecosystem-check
	$(MAKE) ecosystem-measure-only

ecosystem-measure-only:
	@set -eu; for cohort in 0 1; do \
	  $(ECO_BUILD)/ordered-ecosystem-timed$(EXE) measure $$cohort $(ECO_SCALE) > $(ECO_BUILD)/measurements-$$cohort.csv; \
	  done; \
	  cat $(ECO_BUILD)/measurements-0.csv > '$(ECO_SAMPLE_FILE)'; \
	  sed '1d' $(ECO_BUILD)/measurements-1.csv >> '$(ECO_SAMPLE_FILE)'
	$(ECO_BUILD)/ordered-ecosystem-timed$(EXE) clock-resolution > $(ECO_BUILD)/clock.csv

ecosystem-identities: ecosystem-build
	@set -eu; { git -C '$(ROOT)' rev-parse HEAD; git -C '$(ROOT)' status --short; uname -srm; \
	  cat $(ECO_CONFIG); \
	  shasum -a 256 Makefile ordered-costs.c ordered-library.wf ordered-ecosystem.c ordered-ecosystem.rs ordered-ecosystem.cpp \
	    ../ecosystem.mk ../ecosystem-allocator.rs ../ecosystem-allocator.hpp '$(LIBRARY_SOURCE)' '$(WHITEFOOTC)'; \
	  for file in $(ECO_BUILD)/*.ll $(ECO_BUILD)/*.o $(ECO_BUILD)/*.a $(ECO_BUILD)/ordered-ecosystem-*$(EXE); do shasum -a 256 "$$file"; done; \
	} > $(ECO_BUILD)/identities.txt
