SHELL := /bin/sh
ROOT := $(abspath ../../../..)
EXE := $(if $(filter Windows_NT,$(OS)),.exe,)
WHITEFOOTC ?= $(ROOT)/compiler/target/gate/whitefootc$(EXE)
CLANG ?= clang
BUILD ?= .build
MEASURE_SET ?= primary
SOURCE_SHAPE ?= original
SAMPLES ?= $(BUILD)/measurements-$(MEASURE_SET)-$(SOURCE_SHAPE).csv
STAGED_SOURCE_DIR := $(abspath $(BUILD)/staged-source)
STAGED_BASE := $(STAGED_SOURCE_DIR)/sparse-map.wf
STAGED_SOURCE := $(STAGED_SOURCE_DIR)/staged-map.wf
LIBRARY_SOURCE := $(ROOT)/lib/containers/hash-map.wf
SOURCES := dense-map.wf sparse-map.wf slot-map.wf $(STAGED_SOURCE) $(LIBRARY_SOURCE) map-candidates.wf map-library.wf
COMPACT_SOURCE_DIR := $(abspath $(BUILD)/compact-source)
COMPACT_SOURCES := $(addprefix $(COMPACT_SOURCE_DIR)/,dense-map.wf sparse-map.wf) $(abspath slot-map.wf) $(STAGED_SOURCE) $(LIBRARY_SOURCE) $(COMPACT_SOURCE_DIR)/map-candidates.wf map-library.wf
INLINE_SOURCE_DIR := $(abspath $(BUILD)/inline-source)
INLINE_SOURCE := $(INLINE_SOURCE_DIR)/hash-map.wf
OBSERVER := $(ROOT)/tests/programs/containers/container-allocation-observer.c
.DELETE_ON_ERROR:
.DEFAULT_GOAL := check

include $(ROOT)/compiler/runtime.mk

.PHONY: native-check staged-source build-candidates check-candidates build-costs check-costs compact-source build-compact check-compact inline-source build-inline check-inline check measure measure-inactive summarize clean

$(BUILD):
	mkdir -p $@

# Keep one reviewable overlay rather than another copy of the sparse library.
# The unchanged public names serve its original callers; the prefix permits
# the same candidate beside the planned representation in one timing image.
$(STAGED_BASE): sparse-map.wf staged-rebuild.patch Makefile | $(BUILD)
	mkdir -p $(STAGED_SOURCE_DIR)
	cp sparse-map.wf $@
	patch -s -f -F 0 -p1 -d $(STAGED_SOURCE_DIR) < staged-rebuild.patch

$(STAGED_SOURCE): $(STAGED_BASE) Makefile
	sed -e 's/Sparse/Staged/g' -e 's/sparse_/staged_/g' $< > $@

staged-source: $(STAGED_SOURCE)

$(BUILD)/map-native-%$(EXE): map-costs.c map-oracle.h Makefile | $(BUILD)
	@set -eu; case '$*' in normal) flags= ;; retained) flags=-DRETAIN_HELPERS ;; *) exit 1 ;; esac; \
	  $(CLANG) -std=c11 -O2 -Wall -Wextra -Werror $$flags $< -o $@

native-check: $(addprefix $(BUILD)/map-native-,normal$(EXE) retained$(EXE))
	$(BUILD)/map-native-normal$(EXE) check
	$(BUILD)/map-native-retained$(EXE) check

# Three CLI configurations exercise two distinct lowerings: default and
# --no-overlap both select sequential lowering; --par selects parallel.
FIXTURES := dense dense-owned sparse sparse-owned slot staged staged-owned library
MODES := default no-overlap par
FIXTURE_dense := dense-map.wf dense-check.wf
FIXTURE_dense-owned := dense-map.wf dense-owned-check.wf
FIXTURE_sparse := sparse-map.wf sparse-check.wf
FIXTURE_sparse-owned := sparse-map.wf sparse-owned-check.wf
FIXTURE_slot := slot-map.wf slot-check.wf
FIXTURE_staged := $(STAGED_BASE) sparse-check.wf
FIXTURE_staged-owned := $(STAGED_BASE) sparse-owned-check.wf
FIXTURE_library := $(LIBRARY_SOURCE) $(ROOT)/tests/programs/containers/hash-map-program.wf
EXPECTED_dense := 24
EXPECTED_dense-owned := 14
EXPECTED_sparse := 29
EXPECTED_sparse-owned := 14
EXPECTED_slot := 18
EXPECTED_staged := 15
EXPECTED_staged-owned := 12
EXPECTED_library := 28
FLAGS_default :=
FLAGS_no-overlap := --no-overlap
FLAGS_par := --par

define fixture_rules
$(BUILD)/$(1)-$(2).raw.ll: $$(FIXTURE_$(1)) $(WHITEFOOTC) Makefile | $(BUILD)
	$$(WHITEFOOTC) $$(FLAGS_$(2)) --emit-llvm $$(FIXTURE_$(1)) -o $$@

$(BUILD)/$(1)-$(2)$(EXE): $(BUILD)/$(1)-$(2).raw.ll $(NATIVE_OBJECTS)
	$$(CLANG) -O2 -Wno-override-module $$< $$(NATIVE_OBJECTS) $$(NATIVE_LINK_FLAGS) -o $$@

$(BUILD)/$(1)-$(2).observed.ll: $(BUILD)/$(1)-$(2).raw.ll Makefile
	sed -e 's/@malloc(/@wf_observe_allocate(/g' -e 's/@free(/@wf_observe_release(/g' -e 's/@main(/@wf_fixture_main(/g' $$< > $$@

$(BUILD)/$(1)-$(2)-observed$(EXE): $(BUILD)/$(1)-$(2).observed.ll $(OBSERVER) $(NATIVE_OBJECTS)
	$$(CLANG) -std=c11 -O2 -Wall -Wextra -Werror -Wno-override-module -I$$(NATIVE_ROOT) $$< $$(OBSERVER) $$(NATIVE_OBJECTS) $$(NATIVE_LINK_FLAGS) -o $$@
endef
$(foreach fixture,$(FIXTURES),$(foreach mode,$(MODES),$(eval $(call fixture_rules,$(fixture),$(mode)))))

FIXTURE_BINARIES := $(foreach fixture,$(FIXTURES),$(foreach mode,$(MODES),$(BUILD)/$(fixture)-$(mode)$(EXE) $(BUILD)/$(fixture)-$(mode)-observed$(EXE)))
build-candidates: $(FIXTURE_BINARIES)

check-candidates: build-candidates
	@set -eu; \
	  for item in $(foreach fixture,$(FIXTURES),$(foreach mode,$(MODES),$(fixture)-$(mode):$(EXPECTED_$(fixture)))); do \
	    stem=$${item%:*}; count=$${item##*:}; \
	    $(BUILD)/$$stem$(EXE) > $(BUILD)/$$stem.stdout 2> $(BUILD)/$$stem.stderr; \
	    test ! -s $(BUILD)/$$stem.stdout; test ! -s $(BUILD)/$$stem.stderr; \
	    $(BUILD)/$$stem-observed$(EXE) > $(BUILD)/$$stem.observed.stdout 2> $(BUILD)/$$stem.observed.stderr; \
	    test ! -s $(BUILD)/$$stem.observed.stderr; \
	    test "$$(cat $(BUILD)/$$stem.observed.stdout)" = "container allocation observer: $$count allocations, each released exactly once"; \
	    echo "$$stem: ordinary execution and $$count exact allocation identities passed"; \
	  done
	@set -eu; observer=$(BUILD)/dense-par-observed$(EXE); \
	  $$observer concurrent > $(BUILD)/observer.stdout 2> $(BUILD)/observer.stderr; \
	  test ! -s $(BUILD)/observer.stderr; \
	  test "$$(cat $(BUILD)/observer.stdout)" = 'container allocation observer: 32 allocations, each released exactly once'; \
	  for control in double-release foreign-release missing-release; do \
	    status=0; $$observer $$control > $(BUILD)/observer.stdout 2> $(BUILD)/observer.stderr || status=$$?; \
	    test "$$status" = 1; test ! -s $(BUILD)/observer.stdout; \
	    case $$control in \
	      double-release) message='allocation released twice' ;; \
	      foreign-release) message='release did not return an allocated address' ;; \
	      missing-release) message='every allocation is released exactly once' ;; \
	    esac; \
	    test "$$(cat $(BUILD)/observer.stderr)" = "container allocation observer: $$message"; \
	  done; \
	  echo 'Existing quarantining observer: concurrent and three negative controls passed'

$(BUILD)/map-library.raw.ll: $(SOURCES) $(WHITEFOOTC) Makefile | $(BUILD)
	$(WHITEFOOTC) --emit-llvm $(SOURCES) -o $@

$(BUILD)/map-library-normal.ll: $(BUILD)/map-library.raw.ll Makefile
	sed -e 's/@malloc(/@wf_cost_allocate(/g' -e 's/@free(/@wf_cost_release(/g' -e 's/@main(/@wf_fixture_main(/g' \
	    -e 's/^declare ptr @wf_cost_allocate(i64)$$/declare noalias nonnull ptr @wf_cost_allocate(i64)/' $< | \
	  awk '/^define .* @wf_map_cost_(dense|sparse|slot|staged|library)_(word|record)_trace\(/ { if (sub(/ #0 [{]$$/, " noinline #0 {") != 1) exit 1; traces++ } { print } END { if (traces != 10) exit 1 }' > $@

$(BUILD)/map-library-retained.ll: $(BUILD)/map-library-normal.ll Makefile
	awk '/^define .* @wf_(dense|sparse|slot|staged|hash)_map_(new|put|remove|lookup|edit|reserve|rehash|free)[$$(]/ || \
	     /^define .* @wf_map_cost_(hash|equal|word_observe|record_observe|word_consume|record_consume|word_edit|record_edit)\(/ \
	     { if (sub(/ #0 [{]$$/, " noinline #0 {") != 1) exit 1; retained++ } \
	     { print } END { if (retained == 0) exit 1; print "Retained public map/callback definitions: " retained > "/dev/stderr" }' $< > $@

$(BUILD)/map-library-%.opt.ll: $(BUILD)/map-library-%.ll
	$(CLANG) -O2 -Wno-override-module -x ir -S -emit-llvm $< -o $@

$(BUILD)/control-%.opt.ll: map-costs.c map-oracle.h Makefile | $(BUILD)
	@set -eu; case '$*' in normal) flags= ;; retained) flags=-DRETAIN_HELPERS ;; *) exit 1 ;; esac; \
	  $(CLANG) -std=c11 -O2 -Wall -Wextra -Werror -DWITH_WF $$flags -S -emit-llvm $< -o $@

$(BUILD)/map-costs-%$(EXE): map-costs.c $(BUILD)/map-library-%.ll $(NATIVE_OBJECTS)
	@set -eu; case '$*' in normal) flags= ;; retained) flags=-DRETAIN_HELPERS ;; *) exit 1 ;; esac; \
	  $(CLANG) -std=c11 -O2 -Wall -Wextra -Werror -DWITH_WF -Wno-override-module $$flags $< $(BUILD)/map-library-$*.ll $(NATIVE_OBJECTS) $(NATIVE_LINK_FLAGS) -o $@

build-costs: $(addprefix $(BUILD)/map-costs-,normal$(EXE) retained$(EXE)) \
             $(addprefix $(BUILD)/map-library-,normal.opt.ll retained.opt.ll) \
             $(addprefix $(BUILD)/control-,normal.opt.ll retained.opt.ll)

check-costs: build-costs
	@grep -Eq '^declare noalias nonnull ptr @wf_cost_allocate\(i64\)' $(BUILD)/map-library-normal.ll
	$(BUILD)/map-costs-normal$(EXE) check
	$(BUILD)/map-costs-retained$(EXE) check
	@set -eu; for operation in new put remove lookup edit reserve rehash free; do \
	  grep -Eq "call .*@\"?wf_(dense|sparse)_map_$${operation}"'[$$(]' $(BUILD)/map-library-retained.opt.ll; \
	  grep -Eq "call .*@\"?wf_hash_map_$${operation}"'[$$(]' $(BUILD)/map-library-retained.opt.ll; \
	  grep -Eq "call .*@(word|record)_[a-z]+_$${operation}[.(]" $(BUILD)/control-retained.opt.ll; \
	done; \
	for representation in slot staged; do \
	  for operation in new put remove lookup reserve rehash free; do \
	    grep -Eq "call .*@\"?wf_$${representation}_map_$${operation}"'[$$(]' $(BUILD)/map-library-retained.opt.ll; \
	    grep -Eq "call .*@(word|record)_$${representation}_$${operation}[.(]" $(BUILD)/control-retained.opt.ll; \
	  done; \
	done; \
	for callback in hash equal word_observe record_observe word_consume record_consume word_edit record_edit; do \
	  grep -Eq "call .*@wf_map_cost_$${callback}[(]" $(BUILD)/map-library-retained.opt.ll; \
	done; \
	for callback in key_hash key_equal word_observe record_observe word_consume record_consume word_edit record_edit; do \
	  grep -Eq "call .*@$${callback}[.(]" $(BUILD)/control-retained.opt.ll; \
	done; \
	echo 'Public map operations and supplied callbacks survive in both retained modules'

check: native-check check-candidates check-costs

# A reproducible source-shape control, kept separate from the original result
# and from ordinary correctness gates. It reuses the same build and oracle.
$(COMPACT_SOURCE_DIR)/.patched: dense-map.wf sparse-map.wf map-candidates.wf compact-put.patch
	mkdir -p $(COMPACT_SOURCE_DIR)
	cp dense-map.wf sparse-map.wf map-candidates.wf $(COMPACT_SOURCE_DIR)/
	sed -e 's,a/map-library.wf,a/map-candidates.wf,' -e 's,b/map-library.wf,b/map-candidates.wf,' compact-put.patch > $(COMPACT_SOURCE_DIR)/compact-put.patch
	patch -s -f -F 0 -p1 -d $(COMPACT_SOURCE_DIR) < $(COMPACT_SOURCE_DIR)/compact-put.patch
	touch $@

compact-source: $(COMPACT_SOURCE_DIR)/.patched

build-compact: compact-source $(STAGED_SOURCE)
	$(MAKE) build-costs BUILD=$(BUILD)/compact-build SOURCES='$(COMPACT_SOURCES)'

check-compact: build-compact
	$(MAKE) check-costs BUILD=$(BUILD)/compact-build SOURCES='$(COMPACT_SOURCES)'

# Replay the rejected constant-interface migration rewrite without retaining
# another library copy. Ordinary targets always consume the maintained lib.
$(INLINE_SOURCE): $(LIBRARY_SOURCE) inline-rebuild.patch Makefile | $(BUILD)
	mkdir -p $(INLINE_SOURCE_DIR)
	cp $(LIBRARY_SOURCE) $@
	patch -s -f -F 0 -p1 -d $(INLINE_SOURCE_DIR) < inline-rebuild.patch

inline-source: $(INLINE_SOURCE)

build-inline: inline-source
	$(MAKE) build-costs BUILD=$(BUILD)/inline-build LIBRARY_SOURCE='$(INLINE_SOURCE)'

check-inline: build-inline
	$(MAKE) check-costs BUILD=$(BUILD)/inline-build LIBRARY_SOURCE='$(INLINE_SOURCE)'

measure: check
	@set -eu; output='$(SAMPLES)'; first=yes; \
	  if test '$(MEASURE_SET)' = inactive; then \
	    for mode in normal retained; do \
	      $(BUILD)/map-costs-$$mode$(EXE) clock-quantum > $(BUILD)/clock-quantum-$$mode.txt; \
	      cat $(BUILD)/clock-quantum-$$mode.txt; \
	    done; \
	  fi; \
	  for cohort in 0 1; do \
	    if test "$$cohort" = 0; then modes='normal retained'; else modes='retained normal'; fi; \
	    for mode in $$modes; do \
	      sample_file='$(BUILD)/$(MEASURE_SET)-$(SOURCE_SHAPE)-cohort-'$$cohort-$$mode.csv; \
	      $(BUILD)/map-costs-$$mode$(EXE) measure $$cohort $(MEASURE_SET) $(SOURCE_SHAPE) > "$$sample_file"; \
	      if test "$$first" = yes; then head -n 1 "$$sample_file" > "$$output"; first=no; fi; \
	      awk 'NR > 1' "$$sample_file" >> "$$output"; \
	    done; \
	  done; \
	  echo "Raw samples: $$output"

# The same-source compiler comparison uses its fixed fourteen-cell selection.
measure-inactive:
	$(MAKE) measure MEASURE_SET=inactive SOURCE_SHAPE=original

# One median per mode, reversed cohort, workload, implementation and source
# shape. Keep those distinctions; this is not a weighted application score.
summarize:
	@set -eu; \
	  awk -F, 'NR == 1 { if ($$1 != "contract" || $$17 != "peak_bytes") exit 1; next } NF != 17 { exit 1 } { print }' "$(SAMPLES)" > $(BUILD)/summary-input.csv; \
	  LC_ALL=C sort -t, -k1,9 -k13,13n $(BUILD)/summary-input.csv > $(BUILD)/summary-sorted.csv; \
	  awk -F, 'BEGIN { OFS=","; print "contract,cohort,element_bytes,path,capacity,count,hash,variant,source_shape,rounds,traces,median_elapsed_ns,unit,median_units,min_elapsed_ns,max_elapsed_ns" } \
	    function emit() { if (count != 11) { print "expected eleven samples: " key > "/dev/stderr"; exit 1 } \
	      unit = (path == "grow" || path == "setup-cleanup") ? "ns/trace" : "ns/item-round"; \
	      denominator = unit == "ns/trace" ? traces : rounds * population; \
	      print key, rounds, traces, median, unit, median / denominator, minimum, maximum } \
	    { nextkey = $$1; for (i=2; i<=9; i++) nextkey=nextkey OFS $$i; \
	      if (NR > 1 && nextkey != key) emit(); \
	      if (NR == 1 || nextkey != key) { key=nextkey; count=0; path=$$4; population=$$6; rounds=$$11; traces=$$12; minimum=$$13 } \
	      if ($$11 != rounds || $$12 != traces) { print "inconsistent trace sizes: " key > "/dev/stderr"; exit 1 } \
	      count++; if (count == 6) median=$$13; maximum=$$13 } \
	    END { if (NR > 0) emit() }' $(BUILD)/summary-sorted.csv > $(BUILD)/medians-$(MEASURE_SET)-$(SOURCE_SHAPE).csv; \
	  rm $(BUILD)/summary-input.csv $(BUILD)/summary-sorted.csv; \
	  echo "Grouped medians: $(BUILD)/medians-$(MEASURE_SET)-$(SOURCE_SHAPE).csv"

clean:
	rm -rf $(BUILD)

# Explicit current-library comparison. The legacy representation targets above
# retain their historical sources and recorded results; this path imports the
# compiler-bundled std module through map-library.wf alone.
ECO_FAMILY := map
ECO_ABSL_PACKAGES := absl_flat_hash_map
ECO_WORK ?= 262144
ECO_LOOKUP_WORK ?= 2097152
include ../ecosystem.mk
ECO_OCCUPANCY_WORK ?= 1048576
ECO_GEOMETRY_FILE ?= $(ECO_BUILD)/geometry.csv
ECO_GEOMETRY_METADATA ?= $(ECO_BUILD)/geometry-environment.txt
ECO_OCCUPANCY_ACCOUNT ?= $(ECO_BUILD)/occupancy-accounting.csv
ECO_OCCUPANCY_SAMPLES ?= $(ECO_BUILD)/occupancy-measurements.csv

.PHONY: ecosystem-build ecosystem-check ecosystem-account ecosystem-measure \
        ecosystem-lookup-check ecosystem-lookup-account ecosystem-lookup-measure \
        ecosystem-occupancy-check ecosystem-geometry ecosystem-occupancy-account ecosystem-occupancy-measure

$(ECO_BUILD)/map.raw.ll: map-library.wf $(WHITEFOOTC) $(ECO_CONFIG) Makefile | $(ECO_BUILD)
	$(WHITEFOOTC) --emit-llvm $< -o $@

$(ECO_BUILD)/wf-timed.ll: $(ECO_BUILD)/map.raw.ll Makefile
	sed -e 's/@main(/@wf_fixture_main(/g' $< > $@

$(ECO_BUILD)/wf-account.ll: $(ECO_BUILD)/map.raw.ll Makefile
	sed -e 's/@malloc(/@wf_cost_allocate(/g' -e 's/@free(/@wf_cost_release(/g' \
	    -e 's/@main(/@wf_fixture_main(/g' $< > $@

$(ECO_BUILD)/wf-%.o: $(ECO_BUILD)/wf-%.ll $(ECO_CONFIG)
	$(CLANG) -O3 -Wno-override-module -c $< -o $@

$(ECO_BUILD)/driver-%.o: map-ecosystem.c map-oracle.h $(ECO_CONFIG) Makefile
	@set -eu; case '$*' in timed) account= ;; account) account=-DACCOUNT_ONLY ;; *) exit 1 ;; esac; \
	  $(CLANG) $(ECO_CPPFLAGS) $(ECO_CFLAGS) $$account -c $< -o $@

$(ECO_BUILD)/control-%.o: map-costs.c map-oracle.h $(ECO_CONFIG) Makefile
	@set -eu; case '$*' in timed) account= ;; account) account=-DACCOUNT_ONLY ;; *) exit 1 ;; esac; \
	  $(CLANG) $(ECO_CPPFLAGS) $(ECO_CFLAGS) -DECO_MAP_CONTROL $$account -c $< -o $@

$(ECO_BUILD)/map-%$(EXE): $(ECO_BUILD)/driver-%.o $(ECO_BUILD)/rust-%.a \
                         $(ECO_BUILD)/cpp-%.o $(ECO_BUILD)/control-%.o $(NATIVE_OBJECTS) $(ECO_BUILD)/wf-%.o
	$(CXX) $(ECO_LDFLAGS) $^ $(ECO_ABSL_LDFLAGS) $(ECO_RUST_LINK_FLAGS) $(NATIVE_LINK_FLAGS) -o $@

.SECONDARY: $(addprefix $(ECO_BUILD)/,wf-timed.ll wf-account.ll wf-timed.o wf-account.o driver-timed.o driver-account.o rust-timed.a rust-account.a cpp-timed.o cpp-account.o control-timed.o control-account.o)

ecosystem-build: $(ECO_BUILD)/map-timed$(EXE) $(ECO_BUILD)/map-account$(EXE)

ecosystem-check: ecosystem-build
	$(ECO_BUILD)/map-timed$(EXE) check
	$(ECO_BUILD)/map-account$(EXE) check
	@set -eu; \
	  status=0; $(ECO_BUILD)/map-timed$(EXE) negative-checksum > $(ECO_BUILD)/negative.stdout 2> $(ECO_BUILD)/negative.stderr || status=$$?; \
	  test "$$status" = 1; test ! -s $(ECO_BUILD)/negative.stdout; \
	  test "$$(cat $(ECO_BUILD)/negative.stderr)" = 'map ecosystem: independent key-ID/content/outcome oracle'; \
	  status=0; $(ECO_BUILD)/map-account$(EXE) negative-leak > $(ECO_BUILD)/negative.stdout 2> $(ECO_BUILD)/negative.stderr || status=$$?; \
	  test "$$status" = 1; test ! -s $(ECO_BUILD)/negative.stdout; \
	  test "$$(cat $(ECO_BUILD)/negative.stderr)" = 'map ecosystem: every allocation is reclaimed'; \
	  for variant in 0 1 2 3 5 6 7 8; do \
	    status=0; $(ECO_BUILD)/map-timed$(EXE) negative-reserve $$variant > $(ECO_BUILD)/negative.stdout 2> $(ECO_BUILD)/negative.stderr || status=$$?; \
	    test "$$status" = 1; test ! -s $(ECO_BUILD)/negative.stdout; \
	    test "$$(cat $(ECO_BUILD)/negative.stderr)" = 'map ecosystem: reserve capacity floor'; \
	  done; \
	  echo 'Map ecosystem: checksum, cleanup and eight omitted-reserve controls rejected'

ecosystem-account: ecosystem-build
	$(ECO_BUILD)/map-account$(EXE) account $(ECO_WORK) > '$(ECO_ACCOUNT)'
	@echo 'Map allocation accounting: $(ECO_ACCOUNT)'

ecosystem-measure: ecosystem-build
	@set -eu; \
	  for cohort in 0 1; do \
	    $(ECO_BUILD)/map-timed$(EXE) measure $$cohort $(ECO_WORK) > $(ECO_BUILD)/cohort-$$cohort.csv; \
	  done; \
	  cat $(ECO_BUILD)/cohort-0.csv > '$(ECO_SAMPLE_FILE)'; \
	  awk 'NR > 1' $(ECO_BUILD)/cohort-1.csv >> '$(ECO_SAMPLE_FILE)'; \
	  echo 'Map raw samples: $(ECO_SAMPLE_FILE)'

ecosystem-lookup-check: ecosystem-build
	$(ECO_BUILD)/map-timed$(EXE) lookup-check
	$(ECO_BUILD)/map-account$(EXE) lookup-check
	@set -eu; \
	  for fault in population capacity checksum cleanup; do \
	    case $$fault in \
	      population) expected='lookup filled population' ;; \
	      capacity) expected='lookup filled backing geometry' ;; \
	      checksum) expected='lookup independent query oracle' ;; \
	      cleanup) expected='lookup independent full-content cleanup oracle' ;; \
	    esac; \
	    status=0; $(ECO_BUILD)/map-timed$(EXE) negative-lookup $$fault \
	      > $(ECO_BUILD)/lookup-negative.stdout 2> $(ECO_BUILD)/lookup-negative.stderr || status=$$?; \
	    test "$$status" = 1; test ! -s $(ECO_BUILD)/lookup-negative.stdout; \
	    test "$$(cat $(ECO_BUILD)/lookup-negative.stderr)" = "map ecosystem: $$expected"; \
	  done; \
	  for fault in allocation leak; do \
	    case $$fault in allocation) expected='lookup query has no allocation' ;; \
	      leak) expected='every allocation is reclaimed' ;; esac; \
	    status=0; $(ECO_BUILD)/map-account$(EXE) negative-lookup $$fault \
	      > $(ECO_BUILD)/lookup-negative.stdout 2> $(ECO_BUILD)/lookup-negative.stderr || status=$$?; \
	    test "$$status" = 1; test ! -s $(ECO_BUILD)/lookup-negative.stdout; \
	    test "$$(cat $(ECO_BUILD)/lookup-negative.stderr)" = "map ecosystem: $$expected"; \
	  done

ecosystem-lookup-account: ecosystem-build
	$(ECO_BUILD)/map-account$(EXE) lookup-account $(ECO_LOOKUP_WORK) > $(ECO_BUILD)/lookup-accounting.csv

ecosystem-lookup-measure: ecosystem-build
	@set -eu; \
	  for cohort in 0 1; do \
	    $(ECO_BUILD)/map-timed$(EXE) lookup-measure $$cohort $(ECO_LOOKUP_WORK) > $(ECO_BUILD)/lookup-cohort-$$cohort.csv; \
	  done; \
	  cat $(ECO_BUILD)/lookup-cohort-0.csv > $(ECO_BUILD)/lookup-measurements.csv; \
	  awk 'NR > 1' $(ECO_BUILD)/lookup-cohort-1.csv >> $(ECO_BUILD)/lookup-measurements.csv

# The capacity discriminator is opt-in and preserves every practical output.
# Its timed trace bodies are shared with ecosystem-measure; observers occur
# only in the separate ACCOUNT_ONLY image and untimed filled-map witnesses.
ecosystem-occupancy-check: ecosystem-build
	$(ECO_BUILD)/map-timed$(EXE) occupancy-check
	$(ECO_BUILD)/map-account$(EXE) occupancy-check
	$(ECO_BUILD)/map-account$(EXE) geometry-check
	@set -eu; \
	  for control in missing population identity capacity allocation checksum diagnostic; do \
	    case "$$control" in \
	      missing) reason='one filled geometry snapshot' ;; \
	      population) reason='filled geometry population' ;; \
	      identity) reason='geometry implementation identity' ;; \
	      capacity) reason='exposed capacity geometry' ;; \
	      allocation) reason='filled geometry allocation snapshot' ;; \
	      checksum) reason='geometry independent setup oracle' ;; \
	      diagnostic) reason='diagnostic matches trace allocation' ;; \
	    esac; \
	    status=0; $(ECO_BUILD)/map-account$(EXE) negative-geometry "$$control" \
	      > $(ECO_BUILD)/geometry-negative.stdout 2> $(ECO_BUILD)/geometry-negative.stderr || status=$$?; \
	    test "$$status" = 1; test ! -s $(ECO_BUILD)/geometry-negative.stdout; \
	    test "$$(cat $(ECO_BUILD)/geometry-negative.stderr)" = "map ecosystem: $$reason"; \
	  done; \
	  echo 'Map geometry: seven missing/corrupted-observation controls rejected'

ecosystem-geometry: ecosystem-build
	$(ECO_BUILD)/map-account$(EXE) geometry > '$(ECO_GEOMETRY_FILE)'
	@set -eu; \
	  cat '$(ECO_CONFIG)' > '$(ECO_GEOMETRY_METADATA)'; \
	  $(ECO_BUILD)/map-account$(EXE) geometry-identities >> '$(ECO_GEOMETRY_METADATA)'; \
	  git -C '$(ROOT)' rev-parse HEAD >> '$(ECO_GEOMETRY_METADATA)'; \
	  echo 'Map geometry: $(ECO_GEOMETRY_FILE); environment: $(ECO_GEOMETRY_METADATA)'

ecosystem-occupancy-account: ecosystem-build
	$(ECO_BUILD)/map-account$(EXE) occupancy-account $(ECO_OCCUPANCY_WORK) > '$(ECO_OCCUPANCY_ACCOUNT)'
	@echo 'Map occupancy accounting: $(ECO_OCCUPANCY_ACCOUNT)'

ecosystem-occupancy-measure: ecosystem-build
	@set -eu; \
	  for cohort in 0 1; do \
	    $(ECO_BUILD)/map-timed$(EXE) occupancy-measure $$cohort $(ECO_OCCUPANCY_WORK) \
	      > $(ECO_BUILD)/occupancy-cohort-$$cohort.csv; \
	  done; \
	  cat $(ECO_BUILD)/occupancy-cohort-0.csv > '$(ECO_OCCUPANCY_SAMPLES)'; \
	  awk 'NR > 1' $(ECO_BUILD)/occupancy-cohort-1.csv >> '$(ECO_OCCUPANCY_SAMPLES)'; \
	  echo 'Map occupancy raw samples: $(ECO_OCCUPANCY_SAMPLES)'
