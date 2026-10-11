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
EXPERIMENT ?= 1
ARMS := seq demand par twin
ifeq ($(EXPERIMENT),2)
ARMS += idle1
else ifeq ($(EXPERIMENT),3)
ARMS := seq par demand twin order seed extent dedup
else ifeq ($(EXPERIMENT),4)
ARMS := seq par demand static twin
else ifeq ($(EXPERIMENT),5c)
ARMS := seq par demand unversioned twin
else ifeq ($(EXPERIMENT),5a)
# Same seq WF/oracle/runner/runtime objects; only linked placement changes.
WORKLOADS := $(KERNELS)
PADDING_SIZES := 64 4160 65664
ARMS := seq par demand twin $(addprefix seq-shift,$(PADDING_SIZES))
else ifneq ($(EXPERIMENT),1)
$(error EXPERIMENT must be 1, 2, 3, 4, 5a or 5c)
endif
IMAGES := $(foreach arm,$(ARMS),$(addprefix $(BUILD)/$(arm)/,$(WORKLOADS)))
DEFAULT_FLAGS := -std=c11 -O2 -Wall -Wextra -Werror -DWF_ORACLE_NO_MAIN -DWF_ORACLE_PERFORMANCE
.DEFAULT_GOAL := build
.DELETE_ON_ERROR:
include $(ROOT)/compiler/runtime.mk
build: $(IMAGES)
# Ordinary arms preprocess out demand fields and instructions. Only candidate
# and its copied twin link the demand units; idle1 adds the idle-policy guard.
native-seq := $(NATIVE_OBJECTS)
native-par := $(NATIVE_OBJECTS)
native-demand := $(filter-out $(BUILD)/native/sched/core.o $(BUILD)/native/sched/entry.o,$(NATIVE_OBJECTS)) $(BUILD)/demand-core.o $(BUILD)/demand-entry.o
native-order := $(native-demand)
native-extent := $(native-demand)
native-static := $(native-demand)
native-unversioned := $(native-demand)
native-seed := $(filter-out $(BUILD)/demand-core.o,$(native-demand)) $(BUILD)/seed-core.o
native-dedup := $(filter-out $(BUILD)/demand-core.o,$(native-demand)) $(BUILD)/dedup-core.o
$(BUILD)/seed-core.o: $(NATIVE_ROOT)/sched/core.c $(NATIVE_HEADERS) $(NATIVE_CONFIG) Makefile
	$(CLANG) -std=c11 $(NATIVE_OPTIMIZATION_FLAGS) $(NATIVE_COMPILE_FLAGS) -DWF_PAR_DEMAND=1 -DWF_PAR_DEMAND_SEED=1 -c $< -o $@
$(BUILD)/dedup-core.o: $(NATIVE_ROOT)/sched/core.c $(NATIVE_HEADERS) $(NATIVE_CONFIG) Makefile
	$(CLANG) -std=c11 $(NATIVE_OPTIMIZATION_FLAGS) $(NATIVE_COMPILE_FLAGS) -DWF_PAR_DEMAND=1 -DWF_PAR_DEMAND_DEDUP=1 -c $< -o $@
$(BUILD)/demand-core.o: $(NATIVE_ROOT)/sched/core.c $(NATIVE_HEADERS) $(NATIVE_CONFIG) Makefile
	$(CLANG) -std=c11 $(NATIVE_OPTIMIZATION_FLAGS) $(NATIVE_COMPILE_FLAGS) -DWF_PAR_DEMAND=1 -c $< -o $@
$(BUILD)/demand-entry.o: $(NATIVE_ROOT)/sched/entry.c $(NATIVE_HEADERS) $(NATIVE_CONFIG) Makefile
	$(CLANG) -std=c11 $(NATIVE_OPTIMIZATION_FLAGS) $(NATIVE_COMPILE_FLAGS) -DWF_PAR_DEMAND=1 -c $< -o $@


# idle1 reuses demand's exact WF object, oracle and runner. Only scheduler
# objects differ; neither compiler emission nor another optimization run does.
native-idle1 := $(filter-out $(BUILD)/native/sched/core.o $(BUILD)/native/sched/entry.o,$(NATIVE_OBJECTS)) $(BUILD)/idle1-core.o $(BUILD)/idle1-entry.o
$(BUILD)/idle1-%.o: $(NATIVE_ROOT)/sched/%.c $(NATIVE_HEADERS) $(NATIVE_CONFIG) Makefile
	$(CLANG) -std=c11 $(NATIVE_OPTIMIZATION_FLAGS) $(NATIVE_COMPILE_FLAGS) -DWF_PAR_DEMAND=1 -DWF_PAR_IDLE_SINGLE_SPINNER=1 -c $< -o $@
$(addprefix $(BUILD)/idle1/,$(MICROS)): $(BUILD)/idle1/%: $(BUILD)/demand/%.o $(BUILD)/micro-%.o $(BUILD)/runner.o $(native-idle1)
	@mkdir -p $(@D)
	$(CLANG) -O2 $^ $(NATIVE_LINK_FLAGS) -o $@
$(addprefix $(BUILD)/idle1/,$(KERNELS)): $(BUILD)/idle1/%: $(BUILD)/demand/%.o $(BUILD)/oracle-%.o $(BUILD)/runner.o $(native-idle1)
	@mkdir -p $(@D)
	$(CLANG) -O2 $^ $(NATIVE_LINK_FLAGS) -o $@

# Shared native oracle/runtime objects do not vary across the arms.
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
flags-order := --par-demand --par-demand-ablation order
flags-seed := --par-demand --par-demand-ablation seed
# Historical E3 name: extent is now the default; E3's original baseline
# requires the recorded revision, not a replay with this compiler.
flags-extent := --par-demand
flags-static := --par-demand --par-demand-ablation static
flags-unversioned := --par-demand --par-demand-ablation unversioned
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
ifeq ($(EXPERIMENT),3)
$(foreach arm,order seed extent,$(eval $(call arm-rules,$(arm))))
else ifeq ($(EXPERIMENT),4)
$(eval $(call arm-rules,static))
else ifeq ($(EXPERIMENT),5c)
$(eval $(call arm-rules,unversioned))
endif
$(addprefix $(BUILD)/dedup/,$(MICROS)): $(BUILD)/dedup/%: $(BUILD)/demand/%.o $(BUILD)/micro-%.o $(BUILD)/runner.o $(native-dedup)
	@mkdir -p $(@D)
	$(CLANG) -O2 $^ $(NATIVE_LINK_FLAGS) -o $@
$(addprefix $(BUILD)/dedup/,$(KERNELS)): $(BUILD)/dedup/%: $(BUILD)/demand/%.o $(BUILD)/oracle-%.o $(BUILD)/runner.o $(native-dedup)
	@mkdir -p $(@D)
	$(CLANG) -O2 $^ $(NATIVE_LINK_FLAGS) -o $@
# E5a is Linux-only. Uncalled NOP text is linked before the exact seq WF
# object. Its 64-byte alignment preserves existing function alignment while
# the three extents vary line/page placement. No second LLVM compile occurs.
$(BUILD)/layout/%.S: Makefile
	@mkdir -p $(@D)
	printf '%s\n' '.text' '.balign 64' '.globl wf_layout_padding' '.type wf_layout_padding,@function' 'wf_layout_padding:' '.fill $*,1,0x90' '.size wf_layout_padding,.-wf_layout_padding' '.section .note.GNU-stack,"",@progbits' > $@
$(BUILD)/layout/%.o: $(BUILD)/layout/%.S $(NATIVE_CONFIG)
	$(CLANG) -c $< -o $@
define layout-rules
$(addprefix $(BUILD)/seq-shift$(1)/,$(KERNELS)): $(BUILD)/seq-shift$(1)/%: $(BUILD)/layout/$(1).o $(BUILD)/seq/%.o $(BUILD)/oracle-%.o $(BUILD)/runner.o $(native-seq)
	@mkdir -p $$(@D)
	$(CLANG) -O2 $$^ $(NATIVE_LINK_FLAGS) -o $$@
endef
ifeq ($(EXPERIMENT),5a)
$(foreach size,$(PADDING_SIZES),$(eval $(call layout-rules,$(size))))
endif

$(BUILD)/twin/%: $(BUILD)/demand/%
	@mkdir -p $(@D)
	cp $< $@
	cmp $< $@
verify: build
	python3 measure.py --build "$(BUILD)" --verify-only --experiment $(EXPERIMENT)
measure: build
	python3 measure.py --build "$(BUILD)" --experiment $(EXPERIMENT) $(if $(ROUNDS),--rounds $(ROUNDS)) $(MEASURE_FLAGS)
summarize:
	python3 summarize.py "$(BUILD)/measurements.tsv" --inspection "$(BUILD)/inspection.json" --experiment $(EXPERIMENT)
# CI supplies the pre-prototype compiler; this compares complete --par modules.
# The revision immediately before the idle-policy experiment. Compare the
# macro-off scheduler object, so the candidate cannot alter the shipped arm
# even through code placement. The demand object is not compared: experiment 2
# deliberately carries the fix that withdraws an exiting owner's request word
# (DESIGN.md, "A crash at exit in the demand prototype"). CI only.
IDLE_BASE := 75f91b6ba6637a6d33340d8116b2a512ff12b33a
idle-policy-identity: $(BUILD)/native/sched/core.o
	@mkdir -p $(BUILD)/idle-base
	git -C "$(ROOT)" show $(IDLE_BASE):compiler/src/backend/sched/core.c > $(BUILD)/idle-base/core.c
	$(CLANG) -std=c11 $(NATIVE_OPTIMIZATION_FLAGS) $(NATIVE_COMPILE_FLAGS) -I$(NATIVE_ROOT)/sched -c $(BUILD)/idle-base/core.c -o $(BUILD)/idle-base-core.o
	cmp $(BUILD)/idle-base-core.o $(BUILD)/native/sched/core.o

# Compare both unchanged scheduler variants with the pre-ablation revision.
ABLATION_BASE := fadbc866b7423ab50606177f962724df309bf8e6
ablation-identity: $(BUILD)/native/sched/core.o $(BUILD)/demand-core.o
	@mkdir -p $(BUILD)/ablation-base
	git -C "$(ROOT)" show $(ABLATION_BASE):compiler/src/backend/sched/core.c > $(BUILD)/ablation-base/core.c
	$(CLANG) -std=c11 $(NATIVE_OPTIMIZATION_FLAGS) $(NATIVE_COMPILE_FLAGS) -I$(NATIVE_ROOT)/sched -c $(BUILD)/ablation-base/core.c -o $(BUILD)/ablation-base/core.o
	cmp $(BUILD)/ablation-base/core.o $(BUILD)/native/sched/core.o
	$(CLANG) -std=c11 $(NATIVE_OPTIMIZATION_FLAGS) $(NATIVE_COMPILE_FLAGS) -I$(NATIVE_ROOT)/sched -DWF_PAR_DEMAND=1 -c $(BUILD)/ablation-base/core.c -o $(BUILD)/ablation-base/demand-core.o
	cmp $(BUILD)/ablation-base/demand-core.o $(BUILD)/demand-core.o

# Counter images reuse timed WF objects. Only the scheduler/clock objects
# carry counters, and their output never enters measurements.tsv.
counter-flags-seq :=
counter-flags-par :=
counter-flags-demand := -DWF_PAR_DEMAND=1
counter-flags-order := $(counter-flags-demand)
counter-flags-extent := $(counter-flags-demand)
counter-flags-seed := $(counter-flags-demand) -DWF_PAR_DEMAND_SEED=1
counter-flags-dedup := $(counter-flags-demand) -DWF_PAR_DEMAND_DEDUP=1
counter-entry-seq := $(BUILD)/native/sched/entry.o
counter-entry-par := $(BUILD)/native/sched/entry.o
$(foreach arm,demand order seed extent dedup,$(eval counter-entry-$(arm) := $(BUILD)/demand-entry.o))
$(BUILD)/counter-prim.o: $(NATIVE_ROOT)/sched/prim_host.c $(NATIVE_HEADERS) $(NATIVE_CONFIG) Makefile
	$(CLANG) -std=c11 $(NATIVE_OPTIMIZATION_FLAGS) $(NATIVE_COMPILE_FLAGS) -DWF_PAR_DEMAND_COUNTERS=1 -c $< -o $@
define counter-rules
$(BUILD)/counter-$(1)-core.o: $(NATIVE_ROOT)/sched/core.c $(NATIVE_HEADERS) $(NATIVE_CONFIG) Makefile $(if $(filter dedup,$(1)),$(BUILD)/dedup-core.o)
	$(CLANG) -std=c11 $(NATIVE_OPTIMIZATION_FLAGS) $(NATIVE_COMPILE_FLAGS) -DWF_PAR_DEMAND_COUNTERS=1 $(counter-flags-$(1)) -c $$< -o $$@
$(addprefix $(BUILD)/counter/$(1)/,$(MICROS)): $(BUILD)/counter/$(1)/%: $(BUILD)/$(if $(filter dedup,$(1)),demand,$(1))/%.o $(BUILD)/micro-%.o $(BUILD)/runner.o $(filter-out %/core.o %/entry.o %/prim_host.o,$(NATIVE_OBJECTS)) $(counter-entry-$(1)) $(BUILD)/counter-$(1)-core.o $(BUILD)/counter-prim.o
	@mkdir -p $$(@D)
	$(CLANG) -O2 $$^ $(NATIVE_LINK_FLAGS) -o $$@
$(addprefix $(BUILD)/counter/$(1)/,$(KERNELS)): $(BUILD)/counter/$(1)/%: $(BUILD)/$(if $(filter dedup,$(1)),demand,$(1))/%.o $(BUILD)/oracle-%.o $(BUILD)/runner.o $(filter-out %/core.o %/entry.o %/prim_host.o,$(NATIVE_OBJECTS)) $(counter-entry-$(1)) $(BUILD)/counter-$(1)-core.o $(BUILD)/counter-prim.o
	@mkdir -p $$(@D)
	$(CLANG) -O2 $$^ $(NATIVE_LINK_FLAGS) -o $$@
endef
$(foreach arm,seq par demand order seed extent dedup,$(eval $(call counter-rules,$(arm))))
$(BUILD)/counter/twin/%: $(BUILD)/counter/demand/%
	@mkdir -p $(@D)
	cp $< $@
counter-build: $(foreach arm,$(ARMS),$(addprefix $(BUILD)/counter/$(arm)/,$(WORKLOADS)))
	touch $(BUILD)/counter/counter-build
instrumented: counter-build
	python3 measure.py --build "$(BUILD)/counter" --experiment 3 --instrumented

legacy-identity:
	test -n "$(BASE_WFC)"
	python3 legacy_identity.py --base "$(BASE_WFC)" --candidate "$(WFC)" --output "$(BUILD)/legacy"
.SECONDARY:
.PHONY: build verify measure summarize legacy-identity idle-policy-identity ablation-identity counter-build instrumented
