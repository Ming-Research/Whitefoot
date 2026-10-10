# Manual experiment only; no gate imports this directory. Remove/replace with
# the investigation. Each arm uses one source and one compiler; twin is a copy.
HERE := $(abspath $(dir $(lastword $(MAKEFILE_LIST))))
ROOT := $(abspath $(HERE)/../../..)
BUILD ?= /tmp/whitefoot-par-demand
WFC ?= $(ROOT)/compiler/target/gate/whitefootc
CLANG ?= /usr/bin/clang
FIXTURES := $(ROOT)/tests/programs/compute
MICROS := small_constant small_split recursion spine hot_helper large_helper
KERNELS := mandelbrot records fir stencil prefix histogram
WORKLOADS := $(MICROS) $(KERNELS)
ARMS := seq demand par twin
IMAGES := $(foreach arm,$(ARMS),$(addprefix $(BUILD)/$(arm)/,$(WORKLOADS)))
DEFAULT_FLAGS := -std=c11 -O2 -Wall -Wextra -Werror -DWF_ORACLE_NO_MAIN -DWF_ORACLE_PERFORMANCE
.DEFAULT_GOAL := build
.DELETE_ON_ERROR:
include $(ROOT)/compiler/runtime.mk
build: $(IMAGES)
# Ordinary arms preprocess out demand fields and instructions. Only candidate
# and its copied twin link these two opt-in scheduler units.
native-seq := $(NATIVE_OBJECTS)
native-par := $(NATIVE_OBJECTS)
native-demand := $(filter-out $(BUILD)/native/sched/core.o $(BUILD)/native/sched/entry.o,$(NATIVE_OBJECTS)) $(BUILD)/demand-core.o $(BUILD)/demand-entry.o
$(BUILD)/demand-core.o: $(NATIVE_ROOT)/sched/core.c $(NATIVE_HEADERS) $(NATIVE_CONFIG) Makefile
	$(CLANG) -std=c11 $(NATIVE_OPTIMIZATION_FLAGS) $(NATIVE_COMPILE_FLAGS) -DWF_PAR_DEMAND=1 -c $< -o $@
$(BUILD)/demand-entry.o: $(NATIVE_ROOT)/sched/entry.c $(NATIVE_HEADERS) $(NATIVE_CONFIG) Makefile
	$(CLANG) -std=c11 $(NATIVE_OPTIMIZATION_FLAGS) $(NATIVE_COMPILE_FLAGS) -DWF_PAR_DEMAND=1 -c $< -o $@


# Small shared native oracle/runtime objects do not vary across the three arms.
$(BUILD)/runner.o: runner.c $(FIXTURES)/oracle.h $(NATIVE_CONFIG) Makefile
	$(CLANG) $(DEFAULT_FLAGS) -c $< -o $@
$(BUILD)/micro-%.o: micro_oracle.c $(FIXTURES)/oracle.h $(NATIVE_CONFIG) Makefile
	$(CLANG) $(DEFAULT_FLAGS) -DMICRO='"$*"' -c $< -o $@
$(BUILD)/oracle-%.o: $(FIXTURES)/%_oracle.c $(FIXTURES)/oracle.h $(NATIVE_CONFIG) Makefile
	$(CLANG) $(DEFAULT_FLAGS) -c $< -o $@
$(BUILD)/oracle-prefix.o: blocked_performance.c $(FIXTURES)/blocked_oracle.c $(NATIVE_CONFIG) Makefile
	$(CLANG) $(DEFAULT_FLAGS) -DWFB_PREFIX -c $< -o $@
$(BUILD)/oracle-histogram.o: blocked_performance.c $(FIXTURES)/blocked_oracle.c $(NATIVE_CONFIG) Makefile
	$(CLANG) $(DEFAULT_FLAGS) -DWFB_HISTOGRAM -c $< -o $@

flags-seq :=
flags-demand := --par-demand
flags-par := --par
# Static pattern rules let the maintained formal sources remain single-owned.
define arm-rules
$(addprefix $(BUILD)/$(1)/,$(addsuffix .ll,$(MICROS))): $(BUILD)/$(1)/%.ll: %.wf micro_host.ll $(WFC) Makefile
	@mkdir -p $$(@D)
	"$(WFC)" $(flags-$(1)) --par-ledger --emit-llvm $$< -o $$@.raw > $$@.ledger
	awk -f "$(FIXTURES)/host-adapter.awk" $$@.raw micro_host.ll > $$@
$(addprefix $(BUILD)/$(1)/,$(addsuffix .ll,$(KERNELS))): $(BUILD)/$(1)/%.ll: $(FIXTURES)/%.wf $(FIXTURES)/%_host.ll $(WFC) Makefile
	@mkdir -p $$(@D)
	"$(WFC)" $(flags-$(1)) --par-ledger --emit-llvm $$< -o $$@.raw > $$@.ledger
	awk -f "$(FIXTURES)/host-adapter.awk" $$@.raw "$(FIXTURES)/$$*_host.ll" > $$@.bound
	sed -e 's/@main(/@wf_fixture_main(/g' -e 's/@wf__main_body(/@wf_fixture_body(/g' $$@.bound > $$@
$(BUILD)/$(1)/%.o: $(BUILD)/$(1)/%.ll $(NATIVE_CONFIG)
	$(CLANG) $(NATIVE_OPTIMIZATION_FLAGS) -Wno-override-module -c -x ir $$< -o $$@
	$(CLANG) $(NATIVE_OPTIMIZATION_FLAGS) -Wno-override-module -S -x ir $$< -o $$@.s
$(addprefix $(BUILD)/$(1)/,$(MICROS)): $(BUILD)/$(1)/%: $(BUILD)/$(1)/%.o $(BUILD)/micro-%.o $(BUILD)/runner.o $(native-$(1))
	$(CLANG) -O2 $$^ $(NATIVE_LINK_FLAGS) -o $$@
$(addprefix $(BUILD)/$(1)/,$(KERNELS)): $(BUILD)/$(1)/%: $(BUILD)/$(1)/%.o $(BUILD)/oracle-%.o $(BUILD)/runner.o $(native-$(1))
	$(CLANG) -O2 $$^ $(NATIVE_LINK_FLAGS) -o $$@
endef
$(foreach arm,seq demand par,$(eval $(call arm-rules,$(arm))))
$(BUILD)/twin/%: $(BUILD)/demand/%
	@mkdir -p $(@D)
	cp $< $@
	cmp $< $@
verify: build
	python3 measure.py --build "$(BUILD)" --verify-only
measure: build
	python3 measure.py --build "$(BUILD)" --rounds $(or $(ROUNDS),10) $(MEASURE_FLAGS)
summarize:
	python3 summarize.py "$(BUILD)/measurements.tsv" --inspection "$(BUILD)/inspection.json"
# CI supplies the pre-prototype compiler; this compares complete --par modules.
legacy-identity:
	test -n "$(BASE_WFC)"
	python3 legacy_identity.py --base "$(BASE_WFC)" --candidate "$(WFC)" --output "$(BUILD)/legacy"
.SECONDARY:
.PHONY: build verify measure summarize legacy-identity
