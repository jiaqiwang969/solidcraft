//! Read-only adaptation for orthographic drawing, distinct from lossless persistence.
//! Procedural curves are traced on their supports, with sampled accuracy only.
use std::collections::{HashMap, HashSet};

use mt::{BoundedCurve, InnerSpace};
use serde::Serialize;
use solidcraft_geom::Vec3;
use solidcraft_geom::brep::{Brep, Curve, Surface};
use solidcraft_geom::section::CurveAccuracy;
use truck_modeling as mt;

use super::{Body, Result, adapter, guard, invalid};

const EVALUATION_BUDGET: usize = 2_097_152;

/// One extracted edge's link to the native body, separate from an engineering feature ID.
#[derive(Debug, Serialize)]
pub struct DrawingEdgeSource {
    pub kernel_edge: usize,
    pub accuracy: CurveAccuracy,
    /// For a sampled intersection: native parameters at the polyline vertices. Drawing
    /// parameter k denotes vertex k, not native parameter k. Empty for untouched curves.
    pub native_parameters: Vec<f64>,
    /// Observed support residual and displacement from the native solver, for traced edges.
    /// Neither value is a certified global curve error bound.
    pub intersection_sampling: Option<IntersectionSampling>,
}

#[derive(Debug, Serialize)]
pub struct IntersectionSampling {
    pub max_support_residual_mm: f64,
    pub max_native_correction_mm: f64,
}

/// A derived drawing representation. The original portable records remain unchanged.
#[derive(Debug, Serialize)]
pub struct DrawingBody {
    pub brep: Brep,
    /// Flat portable edge order -> native edge index and approximation declaration.
    pub edges: Vec<DrawingEdgeSource>,
    /// Flat portable face order -> native face index.
    pub faces: Vec<usize>,
    pub requested_sample_deviation_mm: f64,
    pub evaluations: usize,
}

/// Adapt the evaluated body's own records, never dimensions, mesh edges or an exchange file.
/// Sampling checks interior points but supplies no bound between samples. Proper rigid
/// placements of revolutions are supported; affine scales and reflections are refused.
pub fn drawing_body(body: &Body, tolerance: f64) -> Result<DrawingBody> {
    body.require_brep("drawing source")?;
    if !tolerance.is_finite() || !(1e-9..=1e-2).contains(&tolerance) {
        return Err(invalid("drawing sample tolerance must be 1e-9..1e-2 mm"));
    }
    guard("drawing source", || {
        let mut brep = adapter::encode(&body.solid)?;
        let native = crate::body::edge_order(body.solid.face_iter()).0;
        let indices: HashMap<_, _> = native.iter().enumerate().map(|(i, e)| (e.id(), i)).collect();
        let mut edges = Vec::new();
        let mut faces = Vec::new();
        let mut remaining = EVALUATION_BUDGET;
        for (source, target) in body.solid.boundaries().iter().zip(&mut brep.shells) {
            // compress() traverses absolute wires; Body::edges traverses oriented wires.
            // Their positions can differ. Join by topology identity, never by position.
            let mut seen = HashSet::new();
            let absolute: Vec<_> = source.iter().flat_map(|f| f.absolute_boundaries()).flatten().filter(|e| seen.insert(e.id())).collect();
            if absolute.len() != target.edges.len() || source.len() != target.faces.len() {
                return Err(invalid("drawing topology correspondence"));
            }
            for (source, target) in absolute.into_iter().zip(&mut target.edges) {
                let kernel_edge = *indices.get(&source.id()).ok_or_else(|| invalid("drawing edge identity"))?;
                let (accuracy, native_parameters, intersection_sampling) = if let mt::Curve::IntersectionCurve(c) = source.curve() {
                    let samples = trace(&c, tolerance, &mut remaining).map_err(|e| invalid(format!("native edge {kernel_edge}: {e}")))?;
                    target.curve = Curve::Polyline { points: samples.points };
                    (CurveAccuracy::Sampled { max_sample_deviation_mm: samples.deviation }, samples.parameters, Some(samples.observed))
                } else {
                    (CurveAccuracy::Analytic, Vec::new(), None)
                };
                edges.push(DrawingEdgeSource { kernel_edge, accuracy, native_parameters, intersection_sampling });
            }
            for f in &mut target.faces {
                rigid_revolution(&mut f.surface)?;
                faces.push(faces.len());
            }
        }
        brep.validate().map_err(invalid)?;
        Ok(DrawingBody { brep, edges, faces, requested_sample_deviation_mm: tolerance, evaluations: EVALUATION_BUDGET - remaining })
    })
}

struct Trace {
    parameters: Vec<f64>,
    points: Vec<[f64; 3]>,
    deviation: f64,
    observed: IntersectionSampling,
}

fn trace(c: &mt::IntersectionCurve<mt::Leader, mt::Surface>, tolerance: f64, budget: &mut usize) -> Result<Trace> {
    let (lo, hi) = c.range_tuple();
    let mut breaks = vec![lo, hi];
    match c.leader() {
        mt::Leader::Polyline(p) => breaks.extend((0..p.0.len()).map(|i| i as f64).filter(|t| *t > lo && *t < hi)),
        mt::Leader::BSpline(s) => breaks.extend(s.knot_vec().iter().copied().filter(|t| *t > lo && *t < hi)),
    }
    for i in 1..8 {
        breaks.push(lo + (hi - lo) * i as f64 / 8.);
    }
    breaks.sort_by(f64::total_cmp);
    breaks.dedup();
    if breaks.len() > 65_536 {
        return Err(invalid("drawing intersection interval budget"));
    }
    let mut max_support_residual = 0f64;
    let mut max_native_correction = 0f64;
    let mut at = |t: f64, budget: &mut usize| -> Result<mt::Point3> {
        *budget = budget.checked_sub(1).ok_or_else(|| invalid("drawing intersection evaluation budget"))?;
        let (p, residual, correction) = super::refine::point(c, t, tolerance)?;
        max_support_residual = max_support_residual.max(residual);
        max_native_correction = max_native_correction.max(correction);
        if [p.x, p.y, p.z].iter().any(|x| !x.is_finite()) {
            return Err(invalid("drawing intersection non-finite point"));
        }
        Ok(p)
    };
    let mut out = Trace {
        parameters: vec![lo],
        points: Vec::new(),
        deviation: 0.,
        observed: IntersectionSampling { max_support_residual_mm: 0., max_native_correction_mm: 0. },
    };
    let mut a = at(lo, budget)?;
    out.points.push([a.x, a.y, a.z]);
    for pair in breaks.windows(2) {
        let (Some(&t0), Some(&t1)) = (pair.first(), pair.get(1)) else {
            return Err(invalid("drawing interval"));
        };
        let b = at(t1, budget)?;
        let mut pending = vec![(t0, a, t1, b, 0usize)];
        while let Some((t0, a, t1, b, depth)) = pending.pop() {
            let delta = b - a;
            let length2 = delta.magnitude2();
            let mut deviation = 0f64;
            let mut middle = a;
            // Eighths include quarter/midpoint tests and interleaved holdout positions.
            for i in 1..8 {
                let p = at(t0 + (t1 - t0) * i as f64 / 8., budget)?;
                let w = if length2 > 0. { ((p - a).dot(delta) / length2).clamp(0., 1.) } else { 0. };
                deviation = deviation.max((p - (a + delta * w)).magnitude());
                if i == 4 {
                    middle = p;
                }
            }
            if deviation > tolerance * 0.98 {
                let mid = 0.5 * (t0 + t1);
                if depth >= 24 || !(mid > t0 && mid < t1) {
                    return Err(invalid("drawing intersection sampling did not converge"));
                }
                pending.push((mid, middle, t1, b, depth + 1));
                pending.push((t0, a, mid, middle, depth + 1));
            } else {
                if out.points.len() >= 65_536 {
                    return Err(invalid("drawing intersection point budget"));
                }
                out.parameters.push(t1);
                out.points.push([b.x, b.y, b.z]);
                out.deviation = out.deviation.max(deviation);
            }
        }
        a = b;
    }
    // Preserve the observed support solve error alongside the chord measurements. This
    // remains sampled accuracy: finite probes do not certify the curve between probes.
    out.deviation += 2. * max_support_residual;
    out.observed = IntersectionSampling { max_support_residual_mm: max_support_residual, max_native_correction_mm: max_native_correction };
    Ok(out)
}

fn rigid_revolution(surface: &mut Surface) -> Result<()> {
    let Surface::PlacedRevolution { profile, origin, axis, matrix, .. } = surface else { return Ok(()) };
    let [cx, cy, cz, ct] = *matrix;
    let v = |p: [f64; 4]| Vec3::new(p[0], p[1], p[2]);
    let (x, y, z, t) = (v(cx), v(cy), v(cz), v(ct));
    if [cx[3], cy[3], cz[3]].iter().any(|w| w.abs() > 1e-12)
        || (ct[3] - 1.).abs() > 1e-12
        || [x, y, z].iter().any(|u| (u.len() - 1.).abs() > 1e-12)
        || x.dot(y).abs() > 1e-12
        || x.dot(z).abs() > 1e-12
        || y.dot(z).abs() > 1e-12
        || (x.cross(y).dot(z) - 1.).abs() > 1e-12
    {
        return Err(invalid("drawing requires a proper rigid revolution placement"));
    }
    let vector = |p: [f64; 3]| x * p[0] + y * p[1] + z * p[2];
    let array = |p: Vec3| [p.x, p.y, p.z];
    let point = |p: [f64; 3]| array(vector(p) + t);
    let mut profile = profile.as_ref().clone();
    match &mut profile {
        Curve::Line { start, end } => {
            *start = point(*start);
            *end = point(*end);
        }
        Curve::CircleArc { frame, .. } | Curve::EllipseArc { frame, .. } => {
            frame.origin = point(frame.origin);
            frame.x_axis = array(vector(frame.x_axis));
            frame.y_axis = array(vector(frame.y_axis));
        }
        Curve::Polyline { points } | Curve::Spline { points, .. } | Curve::Intersection { points, .. } => {
            for p in points {
                *p = point(*p);
            }
        }
        Curve::RationalSpline { homogeneous, .. } => {
            for h in homogeneous {
                let q = vector([h[0], h[1], h[2]]) + t * h[3];
                h[0] = q.x;
                h[1] = q.y;
                h[2] = q.z;
            }
        }
        Curve::SurfaceIntersection { .. } => return Err(invalid("drawing procedural revolution profile unsupported")),
    }
    *surface =
        Surface::Revolution { profile: Box::new(profile), origin: point(*origin), axis: array(vector(*axis)), angle: [0., std::f64::consts::TAU] };
    Ok(())
}
