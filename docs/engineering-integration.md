# Engineering drawing integration on SolidCore

This fork integration starts at upstream **SolidCraft 0.40.0-rc.1**, commit
`4bdffef06180fb3a2e8fbd4b15c7d3f60e55bd11`. The fork's main was
`2aa7e3e51c94c7f47fef3fc0b25523a18a5fd6bf` when the integration began; the
additional upstream commit changes the release workflow. SolidCore stays pinned
to `8ddce901671f1cebd9214a31d21895d725e886c8` by the upstream manifest and lock.

The kernel switch is upstream work, commit
`ea772a0438b145d7d876eb724b744060fc8c59b6` (PR #11351). This branch ports the
applicable drawing behavior from our truck-based engineering work; it does not
restore truck or claim that a new kernel automatically fixes every old defect.

## What carries over

| Concern | This branch | truck compatibility branch |
| --- | --- | --- |
| STEP close knots and intersection support precision | Synthetic regressions exercise the new STEP writer and SolidCore geometry. No old writer is copied. | Retains the truck writer and native curve refinement fixes. |
| Cutting-plane and slot topology | Synthetic revolve/slot regression checks the actual cutting plane, closure and history. | Retains shared-boundary, endpoint-contact and wire-orientation repairs. |
| Boolean retry ordering | SolidCore owns the replacement operation. Old injected truck retry tests do not prove its behavior. | Retains bounded, undisplaced recovery and its injected-failure regressions. |
| HLR and silhouettes | Ports source-accuracy propagation, support-face records, contact/crossing handling, near-start rays and close/tangent torus roots to schema 4. | Same behavior uses the older portable record and sampled intersection adapter. |
| Inspection contracts | Keeps upstream typed commands; adds boundary indices, circle data and analytic support with its recognition basis. | Carries the original typed inspection extension. |
| Native drawing capture | Version 2 reads the exact canonical schema-4 record. Refuses a timeline with errors or warnings. | Version 1 retains native intersection parameters and observed sampling residuals; also refuses incomplete timelines. |

The compatibility branch is `integrate/engineering-fixes-0.27.0`, based on
`d11d268597637029055060a8a6555005faaa1f93`. Its truck implementation is not a
patch series to apply to this branch. Public tests are synthetic; private model
files, saved-state diagnostics and downstream engineering state are excluded.

## Interface and geometry meaning

`model.faces` returns native boundary edge indices, including seams. These are
indices within the current evaluated body, not stable engineering object IDs.
One closed edge may have two uses on the same periodic face. Native surface
labels and supports recognized from actual surface geometry are distinguished
by `analytic_basis`; recognition uses the HLR recognizer, not requested feature
sizes. An unrecognized surface stays unknown.

`analytic_tolerance_mm` and `curve_tolerance_mm` describe source/recognition
conditions. They are not manufacturing tolerances or continuous error bounds.
The HLR diagnostics retain uncertainty and reuse the production drawing tracks
and classifier. They do not infer a source edge by lifting a screen polyline.

The `native_engineering_projection` example emits profile
`solidcraft-engineering-projection/2` and snapshot
`solidcraft.native-drawing-snapshot/2`. Schema-4 indices are canonical; no truck
sampling or native-parameter arrays are invented. A consumer must explicitly
support this profile instead of treating it as a version-1 snapshot.

## Build and qualification

`rust-toolchain.toml` pins Rust **1.99.1**, with Clippy, rustfmt and the wasm
target. The pinned SolidCore fails to compile with Rust 1.95 at an array-to-slice
coercion in `blend_corner/mixed.rs`; the same source compiles with 1.99.1. The
dependency and its cache have not been modified. This branch does not claim
that the upstream manifest's declared minimum compiler was verified.

Use a separate target directory for another baseline or compiler. Bound both
test-process and Rayon concurrency, for example:

```sh
CARGO_BUILD_JOBS=2 RAYON_NUM_THREADS=2 SOLIDCRAFT_TEST_JOBS=2 \
  NEXTEST_TEST_THREADS=2 ACTION_MATRIX_JOBS=2 SOLIDCRAFT_TEST_RUNNER=nextest \
  cargo xtask ci
cargo test --locked --profile ci -p solidcraft-engine --example native_engineering_projection
```

Actual gate results and limits are recorded in [engineering-validation.md](engineering-validation.md).
Opening an old design may expose obsolete topology references even when a body
remains. The capture writes its inspection evidence and refuses that incomplete
timeline. Such a design needs explicit reference migration, geometry checks,
parameter-change checks and fresh-process reopening before a runtime switch.
Passing these software tests alone does not qualify a complete drawing or an
automatic inverse reconstruction.
