# Validation of the SolidCore integration

Baseline: `4bdffef06180fb3a2e8fbd4b15c7d3f60e55bd11`, SolidCraft 0.40.0-rc.1.
Kernel dependency: SolidCore `8ddce901671f1cebd9214a31d21895d725e886c8`.
Compiler: Rust 1.99.1 on aarch64-apple-darwin. Work is isolated from the existing
truck runtime. The original model and runtime have not been overwritten.

## Focused software checks

| Check | Result |
| --- | --- |
| HLR before porting | 74 upstream tests passed. |
| HLR with source accuracy, support faces and contact corrections | 84 tests passed. |
| Surface recognition from native rational geometry | Exact torus support recognized; perturbed control point and invalid tolerance refused. |
| Typed inspection contracts, circles, boundaries, fillet/chamfer edits | 5 tests passed, including the new support and topology regression. |
| Close distinct STEP knots | Passed; distinct knots and multiplicities retained. |
| Cross-drilled intersection, native and STEP round trip | Passed at both specimen lengths with the unchanged 2e-5 mm support-error criterion. |
| Shoulder slot | Passed; analytic cutting plane, shell closure and history retained. |
| Slot through a chamfer, at two shaft lengths | Passed the unchanged 2e-5 mm support-error criterion. The portable regression is retained without the truck healing implementation. |
| Incomplete drawing source | Unit test passed. A native design with obsolete references is refused, retaining inspection evidence and emitting no projection. |
| Repository gate, completed 2026-11-03 | Formatting, assets/licences, repository policy checks, Clippy `-D warnings`, wasm and doctests passed. The full test run had 11,696 passes, 35 failures and 58 skips; the gate exited with failure. |

## Full-suite failure comparison

The 31 failures listed in the upstream `tests/KNOWN_FAILING_TESTS` remain
visible. Four more failures were reproduced on an unmodified checkout of the
same upstream commit, with the same compiler and bounded concurrency:

| Test | Unmodified upstream and integration result |
| --- | --- |
| `edge_only_box_join_is_refused_in_both_orders` | An edge-only box join is accepted instead of refused. |
| `translation_invariant` | The minimal sphere/box case translated by 100,000 mm fails the kernel surface check. |
| `a_cylinder_side_section_is_its_circle` | The section reports twice the expected circumference. |
| `a_long_expression_opens_wide_while_edited_and_enter_applies_it` | Simulated Enter does not apply the edited expression. |

The upstream failure list, golden expectations and assertions were not changed.
This is an integration branch with recorded upstream limitations, not a green
release. The slot/chamfer test was added after the full run and passed separately;
production code did not change after that run.

The old truck retry-injection implementation is retained on the separate
compatibility branch. It is not installed over SolidCore and is not a test of
SolidCore's internal retry policy. The old STEP writer and vendor topology code
are likewise replaced implementations, not mechanically cherry-picked files.

## Native migration evaluation

A private representative design was reopened without changing its original
bytes. Four stored blend references no longer resolved after the kernel switch.
Explicit `timeline.redefine` calls reused the stored picks and parameter
expressions. Feature IDs and all parameter definitions stayed intact; the
upstream format migration also made the old join-grouping behavior explicit.

The migrated model has no feature errors or warnings. The nominal state, three
parameter changes, restoration and a fresh-process reopening passed **60/60**
checks from the existing independent geometry checker, with its original
thresholds. Features and parameters after the walk and reopening equal the
migrated nominal document. The native version-2 capture produces three views
and retains unresolved visibility classifications.

A proposed STEP writer change based on equal 3D-curve and PCURVE parameters is
excluded. ISO 10303-42 `surface_curve` permits different parameterizations;
point sets and directions must agree. A separate STEP reader checked 41 points
per oriented use against the bounded 3D curve: 268 uses, no missing projection,
and a maximum sampled distance of 4.29e-6 mm. This finite, one-way observation
does not establish a continuous error bound or independently verify orientation.
The original equal-parameter discrepancy is not used as evidence of a defect.

There is an additional unresolved measurement result: the independent
two-direction volume observer does not meet its original numerical convergence
budget, although the fixed ten checks pass. Its failures are retained and the
budget has not been relaxed. This is **not complete runtime qualification**.
Private designs and reference geometry are not part of the repository and must
not become inputs to automatic drawing reconstruction.

## Remaining qualification

Resolve or explicitly qualify the additional volume measurement and the
recorded upstream failures. The downstream typed bridge accepts profile 2 and
resolves the model's axial anchors. Actual drawing creation still refuses a
near-zero projected line segment; source membership must be preserved when
handling that degeneracy. Projection, associative annotations and independent
DWG editing need further checks before switching the production engineering
entry point. The existing runtime remains available throughout that process.
