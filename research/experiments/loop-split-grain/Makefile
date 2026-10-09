# The loop-split-grain arms: one source, one compiler and one runtime, the
# overlapped world's split sites rewritten by arms.py. The question, arms and
# criteria are in research/investigations/loop-split-grain/DESIGN.md.
#   make -C research/experiments/loop-split-grain BUILD=<scratch> [CALLS=n]
HERE := $(abspath $(dir $(lastword $(MAKEFILE_LIST))))
ROOT ?= $(abspath $(HERE)/../../..)
BUILD ?= $(patsubst %/,%,$(if $(TMPDIR),$(TMPDIR),/tmp))/loop-split-grain
WFC ?= $(ROOT)/compiler/target/gate/whitefootc
CLANG ?= /usr/bin/clang
PYTHON ?= python3
CALLS ?= 200000000
ARMS := emitted direct zero plain
.DEFAULT_GOAL := build
.DELETE_ON_ERROR:

include $(ROOT)/compiler/runtime.mk

build: $(addprefix $(BUILD)/,$(ARMS) twin)

$(BUILD)/small_split.wf: $(HERE)/small_split.wf Makefile
	@mkdir -p "$(dir $@)"
	sed 's/20000000_u64/$(CALLS)_u64/' "$<" > "$@"

$(BUILD)/emitted.ll: $(BUILD)/small_split.wf $(WFC)
	"$(WFC)" --par --emit-llvm "$<" -o "$@"

$(BUILD)/plain.ll: $(BUILD)/small_split.wf $(WFC)
	"$(WFC)" --emit-llvm "$<" -o "$@"

$(BUILD)/direct.ll $(BUILD)/zero.ll: $(BUILD)/%.ll: $(BUILD)/emitted.ll $(HERE)/arms.py
	$(PYTHON) -I "$(HERE)/arms.py" $* "$<" "$@"

$(BUILD)/%.o: $(BUILD)/%.ll $(NATIVE_CONFIG)
	$(CLANG) $(NATIVE_OPTIMIZATION_FLAGS) -Wno-override-module -x ir -c "$<" -o "$@"

$(BUILD)/%: $(BUILD)/%.o $(NATIVE_OBJECTS)
	$(CLANG) -O2 $^ $(NATIVE_LINK_FLAGS) -o "$@"

# The identical image under a second name: the noise control.
$(BUILD)/twin: $(BUILD)/emitted
	cp "$<" "$@"

.SECONDARY: $(addprefix $(BUILD)/,$(addsuffix .ll,$(ARMS)) $(addsuffix .o,$(ARMS)) small_split.wf)
.PHONY: build
