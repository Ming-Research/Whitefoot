SHELL := /bin/sh
WHITEFOOTC ?= ../../../../compiler/target/gate/whitefootc
BUILD := build

.PHONY: check clean
check:
	@test -x "$(WHITEFOOTC)" || { echo "Build whitefootc or set WHITEFOOTC to the current compiler."; exit 1; }
	@mkdir -p "$(BUILD)"
	@set -eu; \
	  printf 'fixture\tcurrent_outcome\tclassification\n' > "$(BUILD)/outcomes.tsv"; \
	  for name in pool_known_capacity pool_conservation pool_static_capacity pool_boxed_helper pool_boxed_capacity linear_failure_cleanup linear_pop_empty linear_failure_run ring_indexed; do \
	    case "$$name" in pool_static_capacity|pool_boxed_helper|pool_boxed_capacity) helpers= ;; pool_*) helpers=pool_helpers.wf ;; linear_*) helpers=linear_helpers.wf ;; ring_*) helpers=ring_helpers.wf ;; esac; \
	    if ! "$(WHITEFOOTC)" --no-overlap $$helpers "$$name.wf" -o "$(BUILD)/$$name" > "$(BUILD)/$$name.log" 2>&1; then cat "$(BUILD)/$$name.log"; exit 1; fi; \
	    expected=0; test "$$name" = linear_failure_run && expected=7; \
	    status=0; "$(BUILD)/$$name" || status=$$?; \
	    if test "$$status" -ne "$$expected"; then echo "Execution failed: $$name (exit $$status, expected $$expected)"; exit 1; fi; \
	    printf '%s\taccepted-and-executed\tsupported\n' "$$name" >> "$(BUILD)/outcomes.tsv"; \
	  done; \
	  for name in pool_boundary_capacity pool_field_result pool_false_conservation pool_static_length linear_failure_leak ring_contiguous_view; do \
	    source="$$name.wf"; \
	    case "$$name" in \
	      pool_boundary_capacity) helpers=pool_helpers.wf; rule='[INV-1]'; detail=UndischargedLocalInvariant; class=specified-proof-limitation ;; \
	      pool_field_result) helpers=; rule='[FN-9]'; detail=InvalidPostconditionSelector; class=specified-contract-limitation ;; \
	      pool_false_conservation) helpers=; rule='[FN-9]'; detail=Refuted; class=invalid-contract ;; \
	      pool_static_length) helpers=; rule='[INV-1]'; detail='name: initialized'; class=specified-proof-limitation; source="$(BUILD)/$$name.wf"; sed 's/invariant capacity: cap_of(block.bytes) >= 4_u64;/invariant initialized: len_of(block.bytes) >= 4_u64;/' pool_static_capacity.wf > "$$source" ;; \
	      linear_failure_leak) helpers=linear_helpers.wf; rule='[PROV-6]'; detail='binding: first'; class=invalid-linear-leak ;; \
	      ring_contiguous_view) helpers=ring_helpers.wf; rule='[BLK-0]'; detail='head_of(run) <= room_of(run)'; class=invalid-contiguous-view ;; \
	    esac; \
	    rm -f "$(BUILD)/$$name.ll"; \
	    status=0; \
	    "$(WHITEFOOTC)" --no-overlap --emit-llvm $$helpers "$$source" -o "$(BUILD)/$$name.ll" > "$(BUILD)/$$name.log" 2>&1 || status=$$?; \
	    if test "$$status" -ne 1 || test -f "$(BUILD)/$$name.ll" || ! grep -F "$$rule" "$(BUILD)/$$name.log" > /dev/null || ! grep -F "$$detail" "$(BUILD)/$$name.log" > /dev/null; then \
	      echo "Unexpected current outcome: $$name (exit $$status)"; cat "$(BUILD)/$$name.log"; exit 1; \
	    fi; \
	    printf '%s\trejected:%s\t%s\n' "$$name" "$$rule" "$$class" >> "$(BUILD)/outcomes.tsv"; \
	  done; \
	  cat "$(BUILD)/outcomes.tsv"

clean:
	rm -rf "$(BUILD)"
