# Manual suite only. Share the ordinary native runtime and formal host binder.
HERE := $(abspath $(dir $(lastword $(MAKEFILE_LIST))))
ROOT := $(abspath $(HERE)/../../..)
BUILD ?= /tmp/whitefoot-par-suite
CLANG ?= /usr/bin/clang
WFC ?= $(ROOT)/compiler/target/gate/whitefootc
.DEFAULT_GOAL := support
include $(ROOT)/compiler/runtime.mk

support: $(NATIVE_OBJECTS) $(BUILD)/driver.o $(BUILD)/measure

$(BUILD)/driver.o: $(HERE)/driver.c $(NATIVE_CONFIG)
	$(CLANG) -std=c11 -O2 -Wall -Wextra -Werror -Wpedantic -c $< -o $@

$(BUILD)/measure: $(HERE)/measure.c
	@mkdir -p $(BUILD)
	$(CLANG) -std=c11 -O2 -Wall -Wextra -Werror -Wpedantic $< -o $@

# The caller supplies SOURCE, IMAGE and ARM. Keep .ll and .o for inspection of
# permission, optimized-away allocation, and differences between lowerings.
image: support
	"$(WFC)" $(if $(filter par,$(ARM)),--par --par-ledger) --emit-llvm "$(SOURCE)" -o "$(IMAGE).raw.ll"
	awk -f "$(ROOT)/tests/programs/compute/host-adapter.awk" "$(IMAGE).raw.ll" "$(HERE)/entry.ll" > "$(IMAGE).bound.ll"
	sed -e 's/@main(/@wf_suite_original_main(/g' -e 's/@wf__main_body(/@wf_suite_original_body(/g' "$(IMAGE).bound.ll" > "$(IMAGE).ll"
	$(CLANG) $(NATIVE_OPTIMIZATION_FLAGS) -Wno-override-module -x ir -c "$(IMAGE).ll" -o "$(IMAGE).o"
	$(CLANG) -O2 "$(IMAGE).o" $(BUILD)/driver.o $(NATIVE_OBJECTS) $(NATIVE_LINK_FLAGS) -o "$(IMAGE)"

.PHONY: support image
.DELETE_ON_ERROR:
