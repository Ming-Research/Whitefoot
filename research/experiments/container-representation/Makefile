ROOT := $(abspath ../../..)
CARGO ?= cargo
EXE := $(if $(filter Windows_NT,$(OS)),.exe,)
WHITEFOOTC ?= $(ROOT)/compiler/target/gate/whitefootc$(EXE)
.DEFAULT_GOAL := x1-observe

.PHONY: check measure compiler x1-observe slab-deque-check slab-deque-measure map-native-check map-check map-measure \
        ecosystem-build ecosystem-check ecosystem-account ecosystem-measure ecosystem-summarize ecosystem-targets

# Explicit research commands, wrapped externally by the shared run-check guard.
# Recursive calls are sequential even under an outer parallel make. Callers
# provide the frozen WHITEFOOTC and pinned ABSEIL_PREFIX; no compiler rebuild or
# dependency download is hidden in these experiment targets.
ECOSYSTEM_FAMILIES := vector-library deque-library map-library priority-library ordered-library
ABSEIL_PREFIX ?=

ecosystem-build ecosystem-check ecosystem-account ecosystem-measure:
	@set -eu; \
	  if test '$(origin ECO_WORK)' != undefined; then \
	    echo 'ECO_WORK has family-specific units; set it on a direct family command.' >&2; exit 2; \
	  fi; \
	  for family in $(ECOSYSTEM_FAMILIES); do \
	  $(MAKE) -C $$family $@ WHITEFOOTC="$(WHITEFOOTC)" ABSEIL_PREFIX="$(ABSEIL_PREFIX)"; \
	done

# Summary validation and reduction are explicit research operations too. This
# reads the default per-family sample files; alternate runs can be supplied
# directly as family=path arguments to the same reducer.
ecosystem-summarize:
	@mkdir -p .build
	perl summarize-ecosystem.pl --self-test
	perl summarize-ecosystem.pl --complete $(foreach family,$(ECOSYSTEM_FAMILIES),$(patsubst %-library,%,$(family))=$(family)/.build/ecosystem/measurements.csv) > .build/ecosystem-summary.csv
	@echo 'Grouped medians and matched ratios: .build/ecosystem-summary.csv'

# Target status uses the final preserved work settings, without rerunning timing.
# Override the whole family=path list for a candidate's complete family matrix.
ECO_TARGET_INPUTS ?= vector=vector-library/ecosystem-samples.csv \
                    deque=deque-library/ecosystem-samples.csv \
                    map=map-library/ecosystem-replay-samples.csv \
                    priority=priority-library/ecosystem-replay-samples.csv \
                    ordered=ordered-library/ecosystem-replay-samples.csv
ecosystem-targets:
	@mkdir -p .build
	perl summarize-ecosystem.pl --self-test
	perl summarize-ecosystem.pl --targets $(ECO_TARGET_INPUTS) > .build/ecosystem-targets.csv
	@echo 'Per-cell standard-library target status: .build/ecosystem-targets.csv'

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
