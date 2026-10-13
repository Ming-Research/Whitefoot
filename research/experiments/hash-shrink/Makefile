# Invoked only by run.py inside the temporary manual hosted experiment.
SHELL := /bin/sh
ROOT ?= $(abspath ../../..)
BUILD ?= .build
SOURCES ?= $(BUILD)/generated
WFC ?= $(ROOT)/compiler/target/gate/whitefootc
CLANG ?= /usr/bin/clang
.DEFAULT_GOAL := images
.DELETE_ON_ERROR:
include $(ROOT)/compiler/runtime.mk

SHAPES := scalar bytes wide scalar-par bytes-par wide-par
.PHONY: images
images: $(addprefix $(BUILD)/,$(SHAPES)) $(addprefix $(BUILD)/,$(addsuffix .opt.ll,$(SHAPES)))

$(BUILD)/%.ll: $(SOURCES)/%.wf $(WFC)
	mkdir -p $(dir $@)
	$(WFC) $(if $(filter %-par,$*),--par --par-call-grain off,) --emit-llvm $< -o $@

$(BUILD)/%.opt.ll: $(BUILD)/%.ll
	$(CLANG) -O2 -falign-functions=64 -Wno-override-module -x ir -S -emit-llvm $< -o $@

$(BUILD)/%: $(BUILD)/%.ll $(NATIVE_OBJECTS)
	$(CLANG) -O2 -falign-functions=64 -Wno-override-module $< $(NATIVE_OBJECTS) $(NATIVE_LINK_FLAGS) -o $@

# Keep emitted source and both forms of LLVM available for path inspection.
.SECONDARY:
