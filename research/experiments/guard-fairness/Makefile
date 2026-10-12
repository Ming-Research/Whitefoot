# Research-only native arms; retire with the temporary fairness experiment.
# Reuse the runtime's native source list and optimization flags rather than
# adding a compiler option or copying the production runtime build rules.
HERE := $(abspath $(dir $(lastword $(MAKEFILE_LIST))))
ROOT := $(abspath $(HERE)/../../..)
BUILD ?= /tmp/guard-fairness-build
CLANG ?= /usr/bin/clang
MODE ?= 3
ELIGIBLE ?= 0
COUNTERS ?= 0
.DEFAULT_GOAL := image
include $(ROOT)/compiler/runtime.mk
NATIVE_OPTIMIZATION_FLAGS += -flto=full
NATIVE_COMPILE_FLAGS += -DWF_GUARD_MODE=$(MODE) -DWF_GUARD_WAKE_ELIGIBLE=$(ELIGIBLE) -DWF_GUARD_COUNTERS=$(COUNTERS)

# LLVM is emitted once by the same compiler for every arm. Its main is the
# ordinary admitted runner; the experiment adds no source-specific entry.
$(BUILD)/program.o: $(LLVM) $(NATIVE_CONFIG)
	$(CLANG) $(NATIVE_OPTIMIZATION_FLAGS) -Wno-override-module -x ir -c $(LLVM) -o $@

image: $(BUILD)/program.o $(NATIVE_OBJECTS)
	$(CLANG) $(NATIVE_OPTIMIZATION_FLAGS) -fuse-ld=lld $(BUILD)/program.o $(NATIVE_OBJECTS) $(NATIVE_LINK_FLAGS) -o $(IMAGE)

.PHONY: image
.DELETE_ON_ERROR:
