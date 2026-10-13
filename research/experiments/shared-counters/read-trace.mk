# Diagnostic W4rd only. Reuse the native inventory/flags; the caller supplies
# the scratch NATIVE_ROOT created by read-trace.py and unchanged emitted LLVM.
# Retire with the whole-read attribution observer.
ROOT ?= $(abspath $(dir $(lastword $(MAKEFILE_LIST)))/../../..)
.DEFAULT_GOAL := image
include $(ROOT)/compiler/runtime.mk
NATIVE_OPTIMIZATION_FLAGS += -flto=full

$(BUILD)/program.o: $(LLVM) $(NATIVE_CONFIG)
	$(CLANG) $(NATIVE_OPTIMIZATION_FLAGS) -Wno-override-module -x ir -c $(LLVM) -o $@

image: $(BUILD)/program.o $(NATIVE_OBJECTS)
	$(CLANG) $(NATIVE_OPTIMIZATION_FLAGS) -fuse-ld=lld $(BUILD)/program.o $(NATIVE_OBJECTS) $(NATIVE_LINK_FLAGS) -o $(IMAGE)

.PHONY: image
.DELETE_ON_ERROR:
