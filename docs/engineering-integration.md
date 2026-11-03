# Engineering fixes on the truck baseline

This compatibility candidate is based on **SolidCraft 0.27.0**, commit
`d11d268597637029055060a8a6555005faaa1f93`. It consolidates the selected fixes
from the engineering source copies; the original application and private
engineering evidence remain separate. `rust-toolchain.toml` pins Rust 1.95.0.

The remote preservation branch is `experiment/engineering-fixes-0.27.0`.
The local integration worktree is `integrate/engineering-fixes-0.27.0`.
The full run found behavior changes that still need qualification; this is not
a release or a replacement for the selected engineering runtime. Applicable
behavior has separately been ported to the SolidCore integration branch.

## Retained changes

- Preserve close, distinct STEP spline knots and their multiplicities. Refine
  native intersection curves on their support surfaces before export; preserve
  the cutting planes instead of projecting them onto a moved cutter.
- Recover one-ULP mesh boundary contacts when tracing Boolean intersections,
  while keeping real gaps and coplanar faces separate. Reuse an existing shared
  boundary only when its interior curve agrees, preserving vertices and wire
  orientation. Empty/coarse failed attempts allow the later undisplaced exact
  and refined strategies before displaced retries.
- Expose native drawing records with edge/face correspondence. Sampled
  intersection traces retain native parameters, observed support residuals and
  corrections, with bounded work. These measurements are **not continuous error
  certificates** and do not change the native persistence representation.
- Carry source accuracy through HLR, retain support faces, distinguish contact
  from crossing, and keep close/tangent torus and near-start cylinder/cone
  intersections. Visibility and silhouette diagnostics reuse the drawn tracks;
  unresolved contacts remain uncertain.
- Add typed `model.faces` / `model.edges` contracts and analytic support,
  boundary-edge and circle data. Recognition tolerances are not design or
  manufacturing tolerances. Read-only native capture/replay examples support
  downstream engineering adapters.

The existing area test files retain portable synthetic regressions. Private
saved-state diagnostics and real drawings are excluded. Older unsuccessful
candidates are not substituted for the selected source. Equivalent Boolean,
iterator and match-guard rewrites satisfy the pinned compiler's strict lints;
no lint gates or assertions are disabled.

## Isolated precision experiment

The global tightening of curve healing from the engineering source copies is
not in this compatibility branch. Its slot/chamfer support regression remains
on `experiment/truck-support-precision`, together with a support-aware candidate
and endpoint diagnostics. The original tightening regressed the existing bracket
and bowl mesh tests; the support-aware candidate repairs the bracket, but the
bowl still has seven open mesh edges. The unmodified baseline passes both tests.
The bowl previously closed by accepting a circular fit about 7e-5 mm off a
support. That is not a substitute for repairing the mesher on the true curve.
The original engineering source and runnable programs are preserved separately.
The applicable support checks continue on the new SolidCore branch.

## New-kernel integration

The current upstream 0.40.0-rc.1 uses SolidCore, pinned to
`8ddce901671f1cebd9214a31d21895d725e886c8`. Its Boolean/STEP implementations and
schema-4 records replace the truck implementation. The independent branch
`integrate/engineering-fixes-0.40.0-rc.1` ports applicable HLR/interface behavior
and tests equivalent geometry conditions. Removal of the old implementation
alone is not evidence that a failure is fixed. Do not merge this branch's
`vendor/truck-*` files over the new kernel.

## Reproduction

Use a separate target directory when testing another kernel or toolchain.
Run `cargo xtask ci` for the repository gate. The geometry regressions also
exercise the vendored truck crates, which are outside the application workspace;
run their existing analyzer/transversal/output test suites independently.
Detailed outcomes and unresolved changes are recorded in
[engineering-validation.md](engineering-validation.md).

These tests establish the listed software behavior. They do not certify a
complete mechanical drawing, a reconstruction, manufacturing tolerance, or
independent CAD interoperability.
