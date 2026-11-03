//! Refine an intersection sample against its two original supports and transverse plane.
//! The native search supplies the initial chart coordinates; no leader point is substituted.
use mt::{InnerSpace, MetricSpace, ParametricCurve, ParametricSurface, SquareMatrix};
use truck_modeling as mt;

use super::{Result, invalid};

pub(super) fn point(c: &mt::IntersectionCurve<mt::Leader, mt::Surface>, t: f64, tolerance: f64) -> Result<(mt::Point3, f64, f64)> {
    let (initial, mut u, mut v) = c.search_triple(t).ok_or_else(|| invalid("drawing intersection trace failed"))?;
    let anchor = c.leader().subs(t);
    let normal = c.leader().der(t);
    if !(normal.magnitude2().is_finite() && normal.magnitude2() > 0.) {
        return Err(invalid("drawing transverse plane"));
    }
    let normal = normal.normalize();
    let target = (tolerance * 0.01).max(1e-12);
    for _ in 0..16 {
        let a = c.surface0().subs(u.x, u.y);
        let b = c.surface1().subs(v.x, v.y);
        let gap = a - b;
        let plane = (a - anchor).dot(normal);
        let residual = gap.magnitude().max(plane.abs());
        if !residual.is_finite() {
            return Err(invalid("drawing intersection support residual non-finite"));
        }
        if residual <= target {
            let correction = a.distance(initial);
            if !correction.is_finite() || correction > 8. * tolerance {
                return Err(invalid("drawing intersection refinement changes the native sample excessively"));
            }
            return Ok((a, residual, correction));
        }
        let (au, av) = (c.surface0().uder(u.x, u.y), c.surface0().vder(u.x, u.y));
        let (bu, bv) = (c.surface1().uder(v.x, v.y), c.surface1().vder(v.x, v.y));
        // Four equations: equality of the support points (XYZ), plus the plane through
        // the original leader point perpendicular to its tangent. Solve for four UVs.
        let column = |p: mt::Vector3, w: f64| mt::Vector4::new(p.x, p.y, p.z, w);
        let j = mt::Matrix4::from_cols(column(au, au.dot(normal)), column(av, av.dot(normal)), column(-bu, 0.), column(-bv, 0.));
        let inverse = j.invert().ok_or_else(|| invalid("drawing intersection refinement singular"))?;
        let change = inverse * mt::Vector4::new(gap.x, gap.y, gap.z, plane);
        if [change.x, change.y, change.z, change.w].iter().any(|v| !v.is_finite()) {
            return Err(invalid("drawing intersection refinement non-finite"));
        }
        u.x -= change.x;
        u.y -= change.y;
        v.x -= change.z;
        v.y -= change.w;
    }
    Err(invalid("drawing intersection support refinement did not converge"))
}
