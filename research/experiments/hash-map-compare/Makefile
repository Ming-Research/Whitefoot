# The memory cell's observed image of bench.wf: the program built by one
# compiler, with its heap calls and its main renamed to peak.c's observer, and
# linked with the runtime of ROOT, the source tree the compilers were built
# from, as tests/performance links its images. compare.py build calls this
# once per arm; the timing images come from the compiler's own driver.
HERE := $(abspath $(dir $(lastword $(MAKEFILE_LIST))))
ROOT ?= $(abspath $(HERE)/../../..)
WFC ?= $(ROOT)/compiler/target/gate/whitefootc
BUILD ?= $(patsubst %/,%,$(if $(TMPDIR),$(TMPDIR),/tmp))/hash-map-compare-peak
CLANG ?= /usr/bin/clang
.DEFAULT_GOAL := peak
.DELETE_ON_ERROR:

include $(ROOT)/compiler/runtime.mk

peak: $(BUILD)/bench-peak

# The grep fails the build when the program reaches the heap through no
# renamed call, which would leave the observer reporting zero.
$(BUILD)/bench-peak.ll: $(HERE)/bench.wf $(WFC) $(HERE)/Makefile
	@mkdir -p "$(dir $@)"
	"$(WFC)" --emit-llvm "$<" -o "$@.raw"
	sed -e 's/@wf__heap_take(/@wf_peak_take(/g' -e 's/@wf__heap_retake(/@wf_peak_retake(/g' \
	    -e 's/@wf__heap_give(/@wf_peak_give(/g' -e 's/@main(/@wf_fixture_main(/g' "$@.raw" > "$@"
	@rm "$@.raw"
	grep -q '@wf_peak_take(' "$@"

$(BUILD)/bench-peak.o: $(BUILD)/bench-peak.ll $(NATIVE_CONFIG)
	$(CLANG) $(NATIVE_OPTIMIZATION_FLAGS) -Wno-override-module -x ir -c "$<" -o "$@"

$(BUILD)/peak.o: $(HERE)/peak.c $(NATIVE_CONFIG) $(HERE)/Makefile
	$(CLANG) -std=c11 -O2 -Wall -Wextra -Werror -Wpedantic -c "$<" -o "$@"

$(BUILD)/bench-peak: $(BUILD)/bench-peak.o $(BUILD)/peak.o $(NATIVE_OBJECTS)
	$(CLANG) -O2 $^ $(NATIVE_LINK_FLAGS) -o "$@"

.PHONY: peak
