ROOT := $(abspath ../../..)
CARGO ?= cargo
EXE := $(if $(filter Windows_NT,$(OS)),.exe,)
WHITEFOOTC ?= $(ROOT)/compiler/target/gate/whitefootc$(EXE)

.PHONY: check measure compiler x1-observe slab-deque-check slab-deque-measure map-native-check map-check map-measure \
        ecosystem-build ecosystem-check ecosystem-account ecosystem-measure

# Explicit research commands, wrapped externally by the shared run-check guard.
# Recursive calls are sequential even under an outer parallel make. Callers
# provide the frozen WHITEFOOTC and pinned ABSEIL_PREFIX; no compiler rebuild or
# dependency download is hidden in these experiment targets.
ECOSYSTEM_FAMILIES := vector-library deque-library map-library priority-library ordered-library
ABSEIL_PREFIX ?=

ecosystem-build ecosystem-check ecosystem-account ecosystem-measure:
	@set -eu; for family in $(ECOSYSTEM_FAMILIES); do \
	  $(MAKE) -C $$family $@ WHITEFOOTC="$(WHITEFOOTC)" ABSEIL_PREFIX="$(ABSEIL_PREFIX)"; \
	done

x1-observe:
	$(MAKE) -C x1 observe WHITEFOOTC="$(WHITEFOOTC)"

slab-deque-check: compiler
	$(MAKE) -C slab-library check WHITEFOOTC="$(WHITEFOOTC)"
	$(MAKE) -C deque-library check WHITEFOOTC="$(WHITEFOOTC)"

slab-deque-measure: slab-deque-check
	$(MAKE) -C slab-library measure WHITEFOOTC="$(WHITEFOOTC)"
	$(MAKE) -C deque-library measure WHITEFOOTC="$(WHITEFOOTC)"

map-native-check:
	$(MAKE) -C map-library native-check

map-check:
	$(MAKE) -C map-library check

map-measure:
	$(MAKE) -C map-library measure

compiler:
	$(CARGO) build --manifest-path $(ROOT)/compiler/Cargo.toml --profile gate --bin whitefootc --locked --offline

check: compiler
	$(MAKE) -C authority check
	$(MAKE) -C foundation check
	$(MAKE) -C lifecycle check WHITEFOOTC="$(WHITEFOOTC)"
	$(MAKE) -C dense check WHITEFOOTC="$(WHITEFOOTC)"
	$(MAKE) -C costs check
	$(MAKE) -C families check WHITEFOOTC="$(WHITEFOOTC)"
	$(MAKE) -C vector-library check WHITEFOOTC="$(WHITEFOOTC)"
	$(MAKE) -C slab-library check WHITEFOOTC="$(WHITEFOOTC)"
	$(MAKE) -C deque-library check WHITEFOOTC="$(WHITEFOOTC)"

measure: check
	$(MAKE) -C foundation measure
	$(MAKE) -C dense measure WHITEFOOTC="$(WHITEFOOTC)"
	$(MAKE) -C costs measure
	$(MAKE) -C families measure WHITEFOOTC="$(WHITEFOOTC)"
	$(MAKE) -C vector-library measure WHITEFOOTC="$(WHITEFOOTC)"
	$(MAKE) -C slab-library measure WHITEFOOTC="$(WHITEFOOTC)"
	$(MAKE) -C deque-library measure WHITEFOOTC="$(WHITEFOOTC)"
