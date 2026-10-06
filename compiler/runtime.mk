# Reuse production runtime objects for formal native callers and experiments.
# Keep the unit list aligned with src/bin/whitefootc.rs::runtime_units.
# Consumers supply ROOT, BUILD and CLANG. Retire this include when the compiler
# provides a reusable linked-library artifact for these callers.
NATIVE_ROOT := $(ROOT)/compiler/src/backend
NATIVE_C := ordinary_values.c sched/core.c sched/entry.c \
            completion/runtime.c completion/file_adapter.c completion/bridge.c
ifeq ($(OS),Windows_NT)
NATIVE_C += wf_floor_windows.c windows_runtime.c sched/prim_windows.c \
            completion/wait_windows.c completion/file_windows.c completion/windows_iocp.c
NATIVE_COMPILE_FLAGS :=
NATIVE_LINK_FLAGS := -lws2_32 -lshell32
else
NATIVE_C += wf_floor.c sched/prim_host.c completion/wait_host.c \
            completion/file_posix.c completion/linux_io_uring.c
NATIVE_COMPILE_FLAGS := -pthread
NATIVE_LINK_FLAGS := -pthread -lm
endif
# The driver's HOST_OPTIMIZATION_ARGUMENTS (src/driver.rs), which every
# runtime unit compiles with; keep the two equal.
NATIVE_OPTIMIZATION_FLAGS := -O2 -falign-functions=64
NATIVE_HEADERS := $(wildcard $(NATIVE_ROOT)/*.h $(NATIVE_ROOT)/sched/*.h $(NATIVE_ROOT)/completion/*.h)
NATIVE_OBJECTS := $(addprefix $(BUILD)/native/,$(NATIVE_C:.c=.o)) $(BUILD)/native/ordinary_values_ir.o
.SECONDARY: $(NATIVE_OBJECTS)
NATIVE_BUILD_RULES := $(lastword $(MAKEFILE_LIST))
NATIVE_CONFIG := $(BUILD)/native/configuration.txt
NATIVE_CALLER_DEFAULT := $(.DEFAULT_GOAL)

.PHONY: native-configuration-force
native-configuration-force:

# A shared scratch directory may outlive a checkout or compiler/flag change.
# Compare construction identity, not a cached assertion verdict.
$(NATIVE_CONFIG): native-configuration-force
	@mkdir -p $(dir $@)
	@{ printf '%s\n' "$(NATIVE_ROOT)" "$(CLANG)" "$(NATIVE_OPTIMIZATION_FLAGS)" "$(NATIVE_COMPILE_FLAGS)" "$(NATIVE_LINK_FLAGS)"; $(CLANG) --version; } > $@.new
	@cmp -s $@.new $@ || mv -f $@.new $@
	@rm -f $@.new

$(BUILD)/native/%.o: $(NATIVE_ROOT)/%.c $(NATIVE_HEADERS) $(NATIVE_CONFIG) $(NATIVE_BUILD_RULES)
	mkdir -p $(dir $@)
	$(CLANG) -std=c11 $(NATIVE_OPTIMIZATION_FLAGS) $(NATIVE_COMPILE_FLAGS) -I$(NATIVE_ROOT) -I$(NATIVE_ROOT)/completion -c $< -o $@

$(BUILD)/native/%_ir.o: $(NATIVE_ROOT)/%.ll $(NATIVE_CONFIG) $(NATIVE_BUILD_RULES)
	mkdir -p $(dir $@)
	$(CLANG) $(NATIVE_OPTIMIZATION_FLAGS) -Wno-override-module -x ir -c $< -o $@

# This include supplies construction rules, not its caller's default command.
.DEFAULT_GOAL := $(NATIVE_CALLER_DEFAULT)
