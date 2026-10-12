"""Manual workflow wiring checks; these never build or execute a native image."""
import unittest
from pathlib import Path


class WorkflowTests(unittest.TestCase):
    def setUp(self):
        self.workflow = (Path(__file__).resolve().parents[3] / '.github/workflows/compute-bench.yml').read_text()

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
        self.assertIn('if [ "$RUNNER" = github ]; then diagnostic_options=(--smoke); fi', runner)
        self.assertIn('WHITEFOOT_CHECK_TIMEOUT=300 perl .github/run-check.pl par-demand/measure', runner)
        self.assertNotIn('par-demand/diagnostic', runner)
        self.assertLess(runner.index('mode="${EXPERIMENT#diagnostic-}"'),
                        runner.index('if [ "$EXPERIMENT" = 4 ]'))
        self.assertIn('diagnostic-$mode.json" >> "$GITHUB_STEP_SUMMARY"', runner)

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
