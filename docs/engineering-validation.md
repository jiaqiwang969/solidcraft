# Validation of the truck compatibility candidate

Baseline: SolidCraft 0.27.0, `d11d268597637029055060a8a6555005faaa1f93`.
Compiler: Rust 1.95.0, aarch64-apple-darwin. The published preservation branch
is `experiment/engineering-fixes-0.27.0`; it is not approved for runtime adoption.
The existing source copies, models and runnable engineering programs remain intact.

## Completed checks

The full `cargo xtask ci` run completed on 2026-11-03: **10,117 passed,
10 failed, 69 skipped**. Formatting, repository policy gates, strict Clippy,
wasm and doctests passed. The full gate exited with failure. Neither assertions
nor golden expectations were changed to make the run pass.

Separate vendored-crate tests exercised the mesh/contact and topology changes:
five mesh tests passed; 19 shape-operation tests passed and the existing
`rotated_intersection` failure also reproduced on the unmodified baseline.
The STEP/intersection, undisplaced retry, shoulder-slot, typed inspection and
HLR regressions remain in their existing area test files. Focused passes do not
erase the wider-suite failures below.

The global tightening of curve healing is excluded. It regressed bracket and
bowl mesh tests that pass on the unmodified baseline. The separate precision
experiment and the portable slot/chamfer test preserve that unresolved problem;
the equivalent support test passes on the new SolidCore branch.

## Full-run differences requiring review

| Test | Observed result |
| --- | --- |
| `robustness_goldens::seed_02` | The formerly failing pattern/cut sequence succeeds. The final result and each of the two newly successful cuts were checked in an independent STEP/Boolean process. Both solids were valid; volume differences and extra/missing material met the fixed 0.001 mm³ criterion. This is finite evidence for this seed, not a general qualification. The golden is still unchanged. |
| `robustness_goldens::seed_61` | The failed result remains failed, with additional retry detail in its error text. |
| `robustness_goldens::seed_17` | Edge and face counts differ; geometric equivalence still needs checking. |
| `robustness_goldens::seed_23` | The edge count differs; geometric equivalence still needs checking. |
| `robustness_goldens::seed_36` | A formerly successful sequence refuses intersecting/touching shell boundaries. The cause and the old result's validity need investigation. |
| `robustness_goldens::seed_71` | Topology and reported area differ; independent equivalence still needs checking. |
| `oblique_cuts_through_a_chamfered_ring_are_exact` | One face does not mesh. |
| `a_ring_cut_round_a_counterbored_hole_in_a_chamfered_disc` | The reported removed volume misses the existing criterion. |
| `multiple_profile_intersect_keeps_tool_union_for_solids_and_sheet_groups` | An expected intersection is refused as empty. |
| `s25_autosave_recovery` | The recovered document reports no body where one is expected. |

A clean-baseline comparison ran nine of these tests with the same compiler:
six passed; the counterbored-hole volume test, seed 36 and seed 71 reproduced
the same failures on unmodified `d11d268`. The other differences remain under
review. The autosave case also passed a focused rerun of this candidate with
the original full-workspace feature selection and no retries. Its initial
full-suite failure is retained as an unresolved intermittent observation.

A separate contact-recovery experiment restricted boundary reuse to a fresh
retry. Its eight selected tests still had seven failures; the shoulder-slot
regression passed. This did not resolve the observed differences, and its
changed code is excluded from this preservation commit. None of the failures
is silently classified as fixed or removed from the tests.

One byte-identical replay example was consolidated under `crates/geom/examples`
after the full run; its duplicate under `crates/engine/examples` caused an output
filename collision. This does not change application or test production code.
The original source copies retain both historical files.

## Adoption boundary

This branch preserves the code and regression evidence for review. It does not
establish complete engineering-drawing recovery, manufacturing accuracy, or
independent DWG interoperability. Do not transplant truck vendor patches into
SolidCore. The new integration branch documents the replacement implementation,
retained behavior tests, known upstream failures and remaining migration gates.
