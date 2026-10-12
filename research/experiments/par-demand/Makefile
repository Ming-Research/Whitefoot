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
# E6 reconstructs E5c with the request-gated budget ablation; E6b keeps that
# matrix and reconstructs E6's one-sided call grain as its previous arm.
else ifneq ($(filter 6 6b,$(EXPERIMENT)),)
PADDING_SIZES := 64 4160 65664
ARMS := seq par demand unversioned previous twin $(addprefix seq-shift,$(PADDING_SIZES))
else ifeq ($(EXPERIMENT),5a)
# Same seq WF/oracle/runner/runtime objects; only linked placement changes.
WORKLOADS := $(KERNELS)
PADDING_SIZES := 64 4160 65664
ARMS := seq par demand twin $(addprefix seq-shift,$(PADDING_SIZES))
else ifneq ($(EXPERIMENT),1)
$(error EXPERIMENT must be 1, 2, 3, 4, 5a, 5c, 6 or 6b)
endif
IMAGES := $(foreach arm,$(ARMS),$(addprefix $(BUILD)/$(arm)/,$(WORKLOADS)))
DEFAULT_FLAGS := -std=c11 -O2 -Wall -Wextra -Werror -DWF_ORACLE_NO_MAIN -DWF_ORACLE_PERFORMANCE
.DEFAULT_GOAL := build
.DELETE_ON_ERROR:
include $(ROOT)/compiler/runtime.mk
ifneq ($(filter 6 6b,$(EXPERIMENT)),)
# Identity needs private function and data-object names in the final ELF.
# Keep assembler temporaries and linker locals; do not change optimization.
NATIVE_OPTIMIZATION_FLAGS += -Wa,-L
NATIVE_LINK_FLAGS += -Wl,--discard-none
endif
build: $(IMAGES)
ifneq ($(filter 6 6b,$(EXPERIMENT)),)
build: $(BUILD)/entry-dispatch
endif
# Ordinary arms preprocess out demand fields and instructions. Only candidate
# and its copied twin link the demand units; idle1 adds the idle-policy guard.
native-seq := $(NATIVE_OBJECTS)
native-par := $(NATIVE_OBJECTS)
native-demand := $(filter-out $(BUILD)/native/sched/core.o $(BUILD)/native/sched/entry.o,$(NATIVE_OBJECTS)) $(BUILD)/demand-core.o $(BUILD)/demand-entry.o
native-order := $(native-demand)
native-extent := $(native-demand)
native-static := $(native-demand)
native-unversioned := $(native-demand)
native-previous := $(native-demand)
$(BUILD)/entry-dispatch.o: entry_dispatch.c $(NATIVE_CONFIG) Makefile
	$(CLANG) -std=c11 $(NATIVE_OPTIMIZATION_FLAGS) $(NATIVE_COMPILE_FLAGS) -Wall -Wextra -Werror -c $< -o $@
$(BUILD)/entry-leaf-%.o: entry_leaf.c $(NATIVE_CONFIG) Makefile
	$(CLANG) -std=c11 $(NATIVE_OPTIMIZATION_FLAGS) -DWF_ENTRY_LEAF=wf_entry_$* -c $< -o $@
$(BUILD)/entry-dispatch: $(BUILD)/entry-dispatch.o $(BUILD)/entry-leaf-active.o $(BUILD)/entry-leaf-inactive.o $(native-demand)
	$(CLANG) -O2 $^ $(NATIVE_LINK_FLAGS) -o $@
	objdump -d --no-show-raw-insn $@ > $@.disassembly
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
flags-previous := --par-demand --par-demand-ablation requested
ifeq ($(EXPERIMENT),6b)
flags-previous := --par-demand --par-demand-ablation one-sided
endif

# Stage-one attribution instruments only; retire with this investigation.
# The default core is never compiled with these observation/counter macros.
DIAGNOSTIC_MODE ?= spine
$(BUILD)/diagnostic_runner.o: diagnostic_runner.c $(FIXTURES)/oracle.h $(NATIVE_CONFIG) Makefile
	$(CLANG) $(DEFAULT_FLAGS) $(NATIVE_COMPILE_FLAGS) -c $< -o $@
$(BUILD)/diagnostic_fir_adapter.o: diagnostic_fir_adapter.c $(NATIVE_CONFIG) Makefile
	$(CLANG) -std=c11 $(NATIVE_OPTIMIZATION_FLAGS) $(NATIVE_COMPILE_FLAGS) -Wall -Wextra -Werror -ffp-contract=off -c $< -o $@
$(BUILD)/diagnostic-shipped-core.o: $(NATIVE_ROOT)/sched/core.c $(NATIVE_HEADERS) $(NATIVE_CONFIG) Makefile
	$(CLANG) -std=c11 $(NATIVE_OPTIMIZATION_FLAGS) $(NATIVE_COMPILE_FLAGS) -DWF_PAR_DEMAND=1 -DWF_PAR_DIAGNOSTIC_OBSERVATIONS=1 -c $< -o $@
$(BUILD)/diagnostic-zero-core.o: $(NATIVE_ROOT)/sched/core.c $(NATIVE_HEADERS) $(NATIVE_CONFIG) Makefile
	$(CLANG) -std=c11 $(NATIVE_OPTIMIZATION_FLAGS) $(NATIVE_COMPILE_FLAGS) -DWF_PAR_DEMAND=1 -DWF_PAR_DIAGNOSTIC_OBSERVATIONS=1 -DWF_PAR_IDLE_WINDOW_US=0 -c $< -o $@
$(BUILD)/diagnostic-counter-core.o: $(NATIVE_ROOT)/sched/core.c $(NATIVE_HEADERS) $(NATIVE_CONFIG) Makefile
	$(CLANG) -std=c11 $(NATIVE_OPTIMIZATION_FLAGS) $(NATIVE_COMPILE_FLAGS) -DWF_PAR_DEMAND=1 -DWF_PAR_DIAGNOSTICS=1 -c $< -o $@
$(BUILD)/diagnostic-prim.o: $(NATIVE_ROOT)/sched/prim_host.c $(NATIVE_HEADERS) $(NATIVE_CONFIG) Makefile
	$(CLANG) -std=c11 $(NATIVE_OPTIMIZATION_FLAGS) $(NATIVE_COMPILE_FLAGS) -DWF_PAR_DEMAND_COUNTERS=1 -c $< -o $@
diagnostic-build: build $(BUILD)/diagnostic_runner.o $(BUILD)/diagnostic_fir_adapter.o $(BUILD)/diagnostic-shipped-core.o $(BUILD)/diagnostic-zero-core.o $(BUILD)/diagnostic-counter-core.o $(BUILD)/diagnostic-prim.o $(addprefix $(BUILD)/layout/,$(addsuffix .o,64 4160 65664))
	python3 diagnostics.py --prepare --mode "$(DIAGNOSTIC_MODE)" --build "$(BUILD)" --clang "$(CLANG)" --cflags="$(NATIVE_OPTIMIZATION_FLAGS)" --ldflags="$(NATIVE_LINK_FLAGS)" --native-objects $(native-demand)
.PHONY: diagnostic-build
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
else ifneq ($(filter 6 6b,$(EXPERIMENT)),)
$(foreach arm,unversioned previous,$(eval $(call arm-rules,$(arm))))
endif
$(addprefix $(BUILD)/dedup/,$(MICROS)): $(BUILD)/dedup/%: $(BUILD)/demand/%.o $(BUILD)/micro-%.o $(BUILD)/runner.o $(native-dedup)
	@mkdir -p $(@D)
	$(CLANG) -O2 $^ $(NATIVE_LINK_FLAGS) -o $@
$(addprefix $(BUILD)/dedup/,$(KERNELS)): $(BUILD)/dedup/%: $(BUILD)/demand/%.o $(BUILD)/oracle-%.o $(BUILD)/runner.o $(native-dedup)
	@mkdir -p $(@D)
	$(CLANG) -O2 $^ $(NATIVE_LINK_FLAGS) -o $@
# E5a/E6 are Linux-only. Uncalled NOP text is linked before the exact seq WF
# object. Its 64-byte alignment preserves existing function alignment while
# the three extents vary line/page placement. No second LLVM compile occurs.
$(BUILD)/layout/%.S: Makefile
	@mkdir -p $(@D)
	printf '%s\n' '.text' '.balign 64' '.globl wf_layout_padding' '.type wf_layout_padding,@function' 'wf_layout_padding:' '.fill $*,1,0x90' '.size wf_layout_padding,.-wf_layout_padding' '.section .note.GNU-stack,"",@progbits' > $@
$(BUILD)/layout/%.o: $(BUILD)/layout/%.S $(NATIVE_CONFIG)
	$(CLANG) -c $< -o $@
define layout-rules
$(addprefix $(BUILD)/seq-shift$(1)/,$(MICROS)): $(BUILD)/seq-shift$(1)/%: $(BUILD)/layout/$(1).o $(BUILD)/seq/%.o $(BUILD)/micro-%.o $(BUILD)/runner.o $(native-seq)
	@mkdir -p $$(@D)
	$(CLANG) -O2 $$^ $(NATIVE_LINK_FLAGS) -o $$@
$(addprefix $(BUILD)/seq-shift$(1)/,$(KERNELS)): $(BUILD)/seq-shift$(1)/%: $(BUILD)/layout/$(1).o $(BUILD)/seq/%.o $(BUILD)/oracle-%.o $(BUILD)/runner.o $(native-seq)
	@mkdir -p $$(@D)
	$(CLANG) -O2 $$^ $(NATIVE_LINK_FLAGS) -o $$@
endef
ifneq ($(filter 5a 6 6b,$(EXPERIMENT)),)
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

# Compare the scheduler objects with the pre-ablation revision. The macro-off
# object must equal the base. The demand object equals it once the startup
# exposure repair's one call is removed (DESIGN.md, "Startup exposure repair"),
# so that call is the only change to the demand scheduler; the timed demand
# object must differ from the base, which shows it carries the repair and that
# the comparison detects a one-line change. The unrepaired copy is named core.c
# in its own directory because the object records the source's base name.
# CI only.
ABLATION_BASE := fadbc866b7423ab50606177f962724df309bf8e6
ablation-identity: $(BUILD)/native/sched/core.o $(BUILD)/demand-core.o
	@mkdir -p $(BUILD)/ablation-base/unrepaired
	git -C "$(ROOT)" show $(ABLATION_BASE):compiler/src/backend/sched/core.c > $(BUILD)/ablation-base/core.c
	$(CLANG) -std=c11 $(NATIVE_OPTIMIZATION_FLAGS) $(NATIVE_COMPILE_FLAGS) -I$(NATIVE_ROOT)/sched -c $(BUILD)/ablation-base/core.c -o $(BUILD)/ablation-base/core.o
	cmp $(BUILD)/ablation-base/core.o $(BUILD)/native/sched/core.o
	$(CLANG) -std=c11 $(NATIVE_OPTIMIZATION_FLAGS) $(NATIVE_COMPILE_FLAGS) -I$(NATIVE_ROOT)/sched -DWF_PAR_DEMAND=1 -c $(BUILD)/ablation-base/core.c -o $(BUILD)/ablation-base/demand-core.o
	n=$$(grep -c -E '^[[:space:]]*wf__par_request_self\(\);$$' $(NATIVE_ROOT)/sched/core.c); test "$$n" = 1 || { echo "ablation-identity: core.c must hold exactly one wf__par_request_self(); call, found $$n" >&2; exit 1; }
	sed -E '/^[[:space:]]*wf__par_request_self\(\);$$/d' $(NATIVE_ROOT)/sched/core.c > $(BUILD)/ablation-base/unrepaired/core.c
	$(CLANG) -std=c11 $(NATIVE_OPTIMIZATION_FLAGS) $(NATIVE_COMPILE_FLAGS) -I$(NATIVE_ROOT)/sched -DWF_PAR_DEMAND=1 -c $(BUILD)/ablation-base/unrepaired/core.c -o $(BUILD)/ablation-base/unrepaired/demand-core.o
	cmp $(BUILD)/ablation-base/demand-core.o $(BUILD)/ablation-base/unrepaired/demand-core.o
	! cmp -s $(BUILD)/ablation-base/demand-core.o $(BUILD)/demand-core.o || { echo "ablation-identity: the timed demand object equals the base's, so it lacks the startup exposure repair" >&2; exit 1; }

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
