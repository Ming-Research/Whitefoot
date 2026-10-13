"""Manual workflow wiring checks; these never build or execute a native image."""
import re
import unittest
from pathlib import Path


class WorkflowTests(unittest.TestCase):
    def setUp(self):
        self.workflow = (Path(__file__).resolve().parents[3] / '.github/workflows/compute-bench.yml').read_text()

    def test_cell_selection_only_reaches_decisive_driver_after_full_construction_and_inspection(self):
        self.assertIn('par_demand_cells:', self.workflow)
        timing = self.workflow.index('name: Interleave arms, widths and rounds under the selected protocol')
        before, driver = self.workflow[:timing], self.workflow[timing:]
        self.assertIn('build verify idle-policy-identity ablation-identity', before)
        self.assertIn('name: Require build-bound manual inspection before qualified timing', before)
        self.assertNotIn('inputs.par_demand_cells }}', before)
        self.assertNotIn('--cells', before)
        self.assertIn('CELLS: ${{ inputs.par_demand_cells }}', driver)
        self.assertIn('if [ -n "$CELLS" ]; then cells=(--cells "$CELLS"); fi', driver)
        self.assertIn('--experiment "$EXPERIMENT" "${cells[@]}"', driver)
        self.assertIn('par_demand_cells requires demand experiment 6 or 6b.', driver)
        makefile = (Path(__file__).parent / 'Makefile').read_text()
        self.assertIn('CELLS ?=', makefile)
        self.assertIn('--cells "$(CELLS)"', makefile)
        verify = makefile[makefile.index('verify: build'):makefile.index('measure: build')]
        self.assertNotIn('CELLS', verify)

    def test_diagnostics_build_from_six_and_require_hosted_smoke_before_transfer(self):
        for mode in ('spine', 'fir', 'twin'):
            self.assertIn(f'diagnostic-{mode}', self.workflow)
        self.assertIn('diagnostic-*) base_experiment=6', self.workflow)
        self.assertIn('EXPERIMENT="$base_experiment"', self.workflow)
        self.assertIn('EXPERIMENT=6 DIAGNOSTIC_MODE="${EXPERIMENT#diagnostic-}"', self.workflow)
        smoke = self.workflow.index('name: Require bounded diagnostic smoke before artifact transfer')
        transfer = self.workflow.index('name: Retain hosted construction evidence')
        self.assertLess(smoke, transfer)
        self.assertIn('--mode "${EXPERIMENT#diagnostic-}" --build "$RUNNER_TEMP/par-demand" --smoke',
                      self.workflow[smoke:transfer])

    def test_github_diagnostics_are_smoke_only_and_budget_label_is_reused(self):
        runner = self.workflow[self.workflow.index('  par-demand:\n'):]
        self.assertIn('diagnostic-*|1) ;;', runner)
        labels = re.findall(r'\.github/run-check\.pl\s+([^\s\\]+)', runner)
        self.assertTrue(labels)
        self.assertEqual(set(labels), {'par-demand/measure'})
        diagnostic = re.search(r'(?ms)^\s*diagnostic-\*\)\n(.*?)^\s*;;\s*$', runner)
        self.assertIsNotNone(diagnostic)
        branch = diagnostic.group(1)
        self.assertIn('diagnostic_options=()', branch)
        self.assertIn('if [ "$RUNNER" = github ]; then diagnostic_options=(--smoke); fi', branch)
        self.assertIn('WHITEFOOT_CHECK_TIMEOUT=300 perl .github/run-check.pl par-demand/measure', branch)
        self.assertIn('python3 research/experiments/par-demand/diagnostics.py', branch)
        self.assertIn('--mode "$mode" --build "$RUNNER_TEMP/par-demand" "${diagnostic_options[@]}"', branch)
        self.assertIn('diagnostic-$mode.json" >> "$GITHUB_STEP_SUMMARY"', branch)
        self.assertNotIn('measure.py', branch)
        self.assertNotIn('summarize.py', branch)
        self.assertRegex(branch, r'\bexit 0\s*$')
        self.assertLess(diagnostic.end(), runner.index('sizing=()'))

    def test_inspection_is_explicit_bound_to_head_and_before_qualified_timing(self):
        self.assertIn('par_demand_inspection:', self.workflow)
        gate = self.workflow.index('name: Require build-bound manual inspection before qualified timing')
        timing = self.workflow.index('name: Interleave arms, widths and rounds under the selected protocol')
        self.assertLess(gate, timing)
        text = self.workflow[gate:timing]
        self.assertIn("inputs.placement_runner == '14900k' && !startsWith(inputs.par_demand_experiment, 'diagnostic-')", text)
        self.assertIn('if [ -n "$INSPECTION" ]; then', text)
        self.assertIn('realpath -e -- "$INSPECTION"', text)
        self.assertIn('cp -- "$inspection_source" "$RUNNER_TEMP/par-demand/inspection.json"', text)
        self.assertIn('--revision "$(git rev-parse HEAD)"', text)
        self.assertNotIn('inspection-380', text)
