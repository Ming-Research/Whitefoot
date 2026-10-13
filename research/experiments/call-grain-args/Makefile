# Temporary build of the call-grain-args experiment for ONE arm: every workload
# compiled by the compiler WFC with ARMFLAGS and linked with the runtime of the
# revision ROOT. run.py calls it once per arm. Nothing in a gate reads it.
# Remove with the investigation.
HERE := $(abspath $(dir $(lastword $(MAKEFILE_LIST))))
BUILD ?= /tmp/call-grain-arm
ROOT ?= $(abspath $(HERE)/../../..)
FIXT ?= $(ROOT)/tests/programs/compute
GEN ?= $(BUILD)/gen
WFC ?= $(ROOT)/compiler/target/gate/whitefootc
CLANG ?= /usr/bin/clang
ARMFLAGS ?=
STEPS ?= 0
KERNELS := mandelbrot records fir quadrature stencil
WORKLOADS := pair $(KERNELS)
IMAGES := $(addprefix $(BUILD)/,$(WORKLOADS))
DEFAULT_FLAGS := -std=c11 -O2 -Wall -Wextra -Werror -DWF_ORACLE_NO_MAIN -DWF_ORACLE_PERFORMANCE -I$(FIXT) -I$(HERE) -DMIX_STEPS=$(STEPS)
.DEFAULT_GOAL := build
.DELETE_ON_ERROR:
include $(ROOT)/compiler/runtime.mk

build: $(IMAGES)
reference: $(BUILD)/pair_reference

$(BUILD)/pair.ll: $(GEN)/pair.wf $(HERE)/pair_host.ll $(WFC) $(HERE)/Makefile
	@mkdir -p "$(@D)"
	"$(WFC)" $(ARMFLAGS) $(if $(ARMFLAGS),--par-ledger) --emit-llvm "$<" -o "$@.raw" > "$@.ledger"
	awk -f "$(FIXT)/host-adapter.awk" "$@.raw" "$(HERE)/pair_host.ll" > "$@"

$(addprefix $(BUILD)/,$(addsuffix .ll,$(KERNELS))): $(BUILD)/%.ll: $(FIXT)/%.wf $(FIXT)/%_host.ll $(FIXT)/host-adapter.awk $(WFC) $(HERE)/Makefile
	@mkdir -p "$(@D)"
	"$(WFC)" $(ARMFLAGS) $(if $(ARMFLAGS),--par-ledger) --emit-llvm "$<" -o "$@.raw" > "$@.ledger"
	awk -f "$(FIXT)/host-adapter.awk" "$@.raw" "$(FIXT)/$*_host.ll" > "$@.bound"
	sed -e 's/@main(/@wf_fixture_main(/g' -e 's/@wf__main_body(/@wf_fixture_body(/g' "$@.bound" > "$@"

$(BUILD)/%.o: $(BUILD)/%.ll $(NATIVE_CONFIG)
	$(CLANG) $(NATIVE_OPTIMIZATION_FLAGS) -Wno-override-module -x ir -c "$<" -o "$@"

$(BUILD)/runner.o: $(HERE)/runner.c $(FIXT)/oracle.h $(NATIVE_CONFIG) $(HERE)/Makefile
	$(CLANG) $(DEFAULT_FLAGS) -c "$<" -o "$@"
$(BUILD)/oracle-pair.o: $(HERE)/pair_oracle.c $(HERE)/pair_reference.h $(FIXT)/oracle.h $(NATIVE_CONFIG) $(HERE)/Makefile
	$(CLANG) $(DEFAULT_FLAGS) -c "$<" -o "$@"
$(BUILD)/oracle-%.o: $(FIXT)/%_oracle.c $(FIXT)/oracle.h $(NATIVE_CONFIG) $(HERE)/Makefile
	$(CLANG) $(DEFAULT_FLAGS) -c "$<" -o "$@"

$(IMAGES): $(BUILD)/%: $(BUILD)/%.o $(BUILD)/oracle-%.o $(BUILD)/runner.o $(NATIVE_OBJECTS)
	$(CLANG) -O2 $^ $(NATIVE_LINK_FLAGS) -o "$@"

$(BUILD)/pair_reference: $(HERE)/pair_reference.c $(HERE)/pair_reference.h $(HERE)/Makefile
	@mkdir -p "$(@D)"
	$(CLANG) -std=c11 -O2 -Wall -Wextra -Werror -DMIX_STEPS=$(STEPS) -I$(HERE) "$<" -o "$@"

.SECONDARY: $(addprefix $(BUILD)/,$(addsuffix .ll,$(WORKLOADS)) $(addsuffix .o,$(WORKLOADS)) runner.o) $(addprefix $(BUILD)/oracle-,$(addsuffix .o,$(WORKLOADS)))
.PHONY: build reference
