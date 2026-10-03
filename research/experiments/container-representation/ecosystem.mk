# Shared construction settings for the explicit ecosystem-* family targets.
# Callers provide ECO_FAMILY and may override their two native source paths.
# The family driver owns its oracle, whole-trace ABI, and measurement protocol.
# Retire this include and its allocation helpers when that comparison is retired
# or a maintained experiment runner replaces their five current callers.
ECO_SHARED_DIR := $(dir $(abspath $(lastword $(MAKEFILE_LIST))))
ECO_SHARED_MAKEFILE := $(lastword $(MAKEFILE_LIST))
ECO_CALLER_DEFAULT := $(.DEFAULT_GOAL)
ECO_BUILD ?= $(BUILD)/ecosystem
ECO_SAMPLE_FILE ?= $(ECO_BUILD)/measurements.csv
ECO_ACCOUNT ?= $(ECO_BUILD)/accounting.csv
# Output paths resolve within each family directory. ECO_REPEATS is the numeric
# vector/deque sample count; the other drivers fix their sample counts.
# Work settings belong to direct family commands: vector/deque ECO_WORK budgets
# element rounds (vector suffixes use removed elements), map ECO_WORK budgets
# item rounds, and priority ECO_WORK multiplies its scalar/wide work targets.
# Ordered-map ECO_SCALE multiplies complete traces. There is no common work unit
# or aggregate ECO_WORK override.
ECO_RUST_SOURCE ?= $(ECO_FAMILY)-ecosystem.rs
ECO_CPP_SOURCE ?= $(ECO_FAMILY)-ecosystem.cpp
RUSTC ?= rustc
PKG_CONFIG ?= pkg-config
ifeq ($(origin CXX),default)
CXX := clang++
endif
CXX ?= clang++

ECO_CPPFLAGS ?= -DECOSYSTEM
ECO_CFLAGS ?= -std=c11 -O3 -Wall -Wextra -Werror
ECO_CXXFLAGS ?= -std=c++20 -O3 -DNDEBUG -Wall -Wextra -Werror
ECO_RUSTFLAGS ?= --edition=2024 -C opt-level=3 -C panic=abort -D warnings
ECO_LDFLAGS ?= -O3
# rustc prints its native-static-libs note during construction for verification.
# These are the host's ordinary staticlib dependencies, not a second allocator.
ECO_HOST_OS := $(shell uname -s)
ifeq ($(ECO_HOST_OS),Darwin)
ECO_RUST_LINK_FLAGS ?= -liconv -lSystem -lc -lm
else
ECO_RUST_LINK_FLAGS ?= -ldl -lpthread -lm
endif

ABSEIL_PREFIX ?=
ECO_ABSL_PACKAGES ?=
ECO_PKG_CONFIG_PATH := $(if $(ABSEIL_PREFIX),$(ABSEIL_PREFIX)/lib/pkgconfig:$(ABSEIL_PREFIX)/lib64/pkgconfig:)$(PKG_CONFIG_PATH)
ECO_PKG_CONFIG = PKG_CONFIG_PATH='$(ECO_PKG_CONFIG_PATH)' $(PKG_CONFIG)
ifneq ($(strip $(ECO_ABSL_PACKAGES)),)
ECO_ABSL_CPPFLAGS := $(shell $(ECO_PKG_CONFIG) --cflags $(ECO_ABSL_PACKAGES) 2>/dev/null)
ECO_ABSL_LDFLAGS := $(shell $(ECO_PKG_CONFIG) --static --libs $(ECO_ABSL_PACKAGES) 2>/dev/null)
ifeq ($(ECO_HOST_OS),Darwin)
# Abseil 20260817.0's exported absl::time_zone CMake target requires this
# framework on Darwin, but absl_time_zone.pc omits it from the static link.
ECO_ABSL_LDFLAGS += -framework CoreFoundation
endif
else
ECO_ABSL_CPPFLAGS :=
ECO_ABSL_LDFLAGS :=
endif

ECO_CONFIG := $(ECO_BUILD)/configuration.txt
.PHONY: ecosystem-configuration-force
ecosystem-configuration-force:

$(ECO_BUILD):
	mkdir -p $@

# A caller can reuse its build directory after changing a compiler, dependency,
# or flags. Refresh the identity only when its contents change.
$(ECO_CONFIG): ecosystem-configuration-force $(ECO_SHARED_MAKEFILE) | $(ECO_BUILD)
	@set -eu; \
	  if test -n '$(ECO_ABSL_PACKAGES)'; then $(ECO_PKG_CONFIG) --exists $(ECO_ABSL_PACKAGES); fi; \
	  { printf '%s\n' '$(ECO_FAMILY)' '$(WHITEFOOTC)' '$(CLANG)' '$(CXX)' '$(RUSTC)' \
	      '$(ECO_RUST_SOURCE)' '$(ECO_CPP_SOURCE)' \
	      '$(ECO_CPPFLAGS)' '$(ECO_CFLAGS)' '$(ECO_CXXFLAGS)' '$(ECO_RUSTFLAGS)' \
	      '$(ECO_LDFLAGS)' '$(ECO_RUST_LINK_FLAGS)' '$(ABSEIL_PREFIX)' \
	      '$(ECO_ABSL_PACKAGES)' '$(ECO_ABSL_CPPFLAGS)' '$(ECO_ABSL_LDFLAGS)'; \
	    $(CLANG) --version; $(CXX) --version; $(RUSTC) --version --verbose; \
	    if test -n '$(ECO_ABSL_PACKAGES)'; then $(ECO_PKG_CONFIG) --modversion $(ECO_ABSL_PACKAGES); fi; \
	  } > $@.new; \
	  cmp -s $@.new $@ || mv -f $@.new $@; \
	  rm -f $@.new

$(ECO_BUILD)/rust-%.a: $(ECO_RUST_SOURCE) $(ECO_SHARED_DIR)ecosystem-allocator.rs $(ECO_CONFIG) $(ECO_SHARED_MAKEFILE)
	@set -eu; case '$*' in timed) account= ;; account) account='--cfg account_only' ;; *) exit 1 ;; esac; \
	  $(RUSTC) $(ECO_RUSTFLAGS) --crate-type staticlib --crate-name $(ECO_FAMILY)_ecosystem \
	    --print native-static-libs $$account $< -o $@

$(ECO_BUILD)/cpp-%.o: $(ECO_CPP_SOURCE) $(ECO_SHARED_DIR)ecosystem-allocator.hpp $(ECO_CONFIG) $(ECO_SHARED_MAKEFILE)
	@set -eu; case '$*' in timed) account= ;; account) account=-DACCOUNT_ONLY ;; *) exit 1 ;; esac; \
	  $(CXX) $(ECO_CPPFLAGS) $(ECO_CXXFLAGS) $(ECO_ABSL_CPPFLAGS) $$account -c $< -o $@

# Shared rules must not change a family's historical default target.
.DEFAULT_GOAL := $(ECO_CALLER_DEFAULT)
