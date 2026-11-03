//! Bounded, read-only diagnostics for a point on an actual prepared body's edge.
//! The reported classification comes from the production classifier. The additional hit
//! records explain its inputs; they are not a second source of drawing geometry.

use super::{
    Ctx, HlrError, Options, Placement, Prepared, Source, Stats, View, Visibility, classify_point, context, prepare_tracks, scene_centre, slab,
};
use crate::{Vec3, section::CurveAccuracy};
use serde::Serialize;

/// An analytic support recognized from a face's actual geometry. This does not
/// replace its record, change its trims, or certify its manufacturing accuracy.
#[derive(Debug, Clone, Copy, Serialize)]
pub enum SurfaceSupport {
    Plane { normal: Vec3, point: Vec3 },
    Cylinder { origin: Vec3, axis: Vec3, radius: f64 },
    Cone { apex: Vec3, axis: Vec3, half_angle: f64 },
    Sphere { center: Vec3, radius: f64 },
    Torus { center: Vec3, axis: Vec3, major_radius: f64, minor_radius: f64 },
}

/// Reuse the HLR recognizer on a face from a validated native record. The
/// tolerance is relative to the support's size; unrecognized geometry stays
/// unknown. Recognition uses the surface itself, never a feature's requested size.
pub fn surface_support(face: &crate::brep::Face, relative_tolerance: f64) -> Result<Option<SurfaceSupport>, HlrError> {
    use super::surface::{Analytic, S3};
    if !relative_tolerance.is_finite() || !(1e-12..=1e-6).contains(&relative_tolerance) {
        return Err(HlrError::Invalid("surface recognition tolerance"));
    }
    let surface = S3::of_face(face)?;
    if let Some(S3::Plane { o, n, .. }) = surface.flat(relative_tolerance) {
        return Ok(Some(SurfaceSupport::Plane { normal: n, point: o }));
    }
    Ok(match surface.analytic(relative_tolerance) {
        Analytic::Cylinder { p, a, r } => Some(SurfaceSupport::Cylinder { origin: p, axis: a, radius: r }),
        Analytic::Cone { apex, a, half } => Some(SurfaceSupport::Cone { apex, axis: a, half_angle: half }),
        Analytic::Sphere { c, r } => Some(SurfaceSupport::Sphere { center: c, radius: r }),
        Analytic::Torus { c, a, major, minor } => Some(SurfaceSupport::Torus { center: c, axis: a, major_radius: major, minor_radius: minor }),
        Analytic::Plane | Analytic::Other => None,
    })
}

#[derive(Debug, Serialize)]
pub struct TorusCandidate {
    pub parameter: f64,
    pub grazing: bool,
    pub uv: Option<[f64; 2]>,
    pub inverse_residual_mm: Option<f64>,
    pub inverse_acceptance_mm: f64,
    pub in_range: bool,
}

#[derive(Debug, Serialize)]
pub struct FaceQuery {
    pub face: usize,
    pub analytic: String,
    pub range: [f64; 2],
    pub accepted_hits: usize,
    pub torus_candidates: Vec<TorusCandidate>,
    pub contact_sensitive: bool,
    pub potential_contacts: Vec<PotentialContact>,
}

#[derive(Debug, Serialize)]
pub struct PotentialContact {
    pub ray_parameter: f64,
    pub surface_point: Vec3,
    pub ray_point: Vec3,
    pub distance_mm: f64,
    pub margin_mm: f64,
    pub near_boundary: bool,
    pub inside_trim: Option<bool>,
    pub relevant: bool,
}

#[derive(Debug, Serialize)]
pub struct FaceHit {
    pub face: usize,
    pub analytic: String,
    pub own: bool,
    pub ray_range: [f64; 2],
    pub ray_parameter: f64,
    pub uv: [f64; 2],
    pub surface_domain: [[f64; 2]; 2],
    pub trim_bounds: Option<([f64; 2], [f64; 2])>,
    pub surface_point: Vec3,
    pub ray_point: Vec3,
    pub ray_residual_mm: f64,
    pub trim_tolerance_mm: f64,
    pub near_boundary: bool,
    pub inside_trim: Option<bool>,
    pub grazing: bool,
    pub effect: &'static str,
}

#[derive(Debug, Serialize)]
pub struct PointProbe {
    pub parameter: f64,
    pub domain: [f64; 2],
    pub accuracy: CurveAccuracy,
    pub source_point: Vec3,
    pub screen: [f64; 2],
    pub toward_viewer: Vec3,
    pub support_faces: Vec<usize>,
    pub own_epsilon_mm: f64,
    pub other_epsilon_mm: f64,
    pub classification: Visibility,
    pub hit_reduction_matches: bool,
    pub tested_faces: usize,
    pub face_queries: Vec<FaceQuery>,
    pub hits: Vec<FaceHit>,
}

/// Edge identity stays separate from the visibility observation. JSON fields remain flat.
#[derive(Debug, Serialize)]
pub struct EdgeProbe {
    pub edge: usize,
    #[serde(flatten)]
    pub point: PointProbe,
}

struct PointSample<'a> {
    parameter: f64,
    domain: [f64; 2],
    accuracy: CurveAccuracy,
    local_point: Vec3,
    own: &'a [usize],
    own_eps: f64,
}

/// A source-track reference is scoped to one prepared body, view and options snapshot.
#[derive(Clone, Copy, Debug)]
pub struct SilhouetteParameter {
    pub track: usize,
    pub face: usize,
    pub parameter: f64,
}

#[derive(Debug, Serialize)]
pub struct SupportObservation {
    pub face: usize,
    pub analytic: String,
    pub uv: Option<[f64; 2]>,
    pub surface_point: Option<Vec3>,
    pub inverse_residual_mm: Option<f64>,
    pub normal_dot_sight: Option<f64>,
    pub inside_trim: Option<bool>,
    pub near_boundary: Option<bool>,
    pub trim_tolerance_mm: f64,
}

#[derive(Debug, Serialize)]
pub struct SilhouetteProbe {
    pub track: usize,
    pub source: Source,
    pub track_range: [f64; 2],
    /// These are observations on all candidate supports, not a unique ownership claim.
    pub support_observations: Vec<SupportObservation>,
    #[serde(flatten)]
    pub point: PointProbe,
}

/// Inspect actual drawing tracks once per view; no screen geometry is lifted back to 3D.
/// At most 512 samples, 4096 faces, 10000 source tracks and 16384 hits per sample.
pub fn silhouette_visibility(
    def: &Prepared,
    requests: &[SilhouetteParameter],
    view: &View,
    opts: &Options,
    cancel: &dyn Fn() -> bool,
) -> Result<Vec<SilhouetteProbe>, HlrError> {
    if cancel() {
        return Err(HlrError::Cancelled);
    }
    if requests.len() > 512 {
        return Err(HlrError::Budget("diagnostic sample"));
    }
    if def.faces.len() > 4096 {
        return Err(HlrError::Budget("diagnostic face"));
    }
    let defs = std::slice::from_ref(def);
    let unplaced = [Placement { body: 0, rotation: [Vec3::X, Vec3::Y, Vec3::Z], translation: Vec3::ZERO }];
    let origin = scene_centre(defs, &unplaced)?;
    let scene = [Placement { translation: def.offset - origin, ..unplaced[0] }];
    let ctx = context(defs, &scene, view, opts, cancel)?;
    let bounded = Options { max_tracks: opts.max_tracks.min(10_000), ..*opts };
    let tracks = prepare_tracks(&ctx, &bounded, &mut Stats::default(), &mut Vec::new())?;
    let mut result = Vec::with_capacity(requests.len());
    for request in requests {
        if cancel() {
            return Err(HlrError::Cancelled);
        }
        let track = tracks.get(request.track).ok_or(HlrError::Invalid("diagnostic track"))?;
        if !matches!(track.source, Source::Silhouette { face } if face == request.face) {
            return Err(HlrError::Invalid("diagnostic silhouette source"));
        }
        let parameter = request.parameter;
        if !parameter.is_finite() || parameter < track.range[0] || parameter > track.range[1] {
            return Err(HlrError::Invalid("diagnostic parameter"));
        }
        let local_point = track.c3().at(parameter);
        let sample =
            PointSample { parameter, domain: track.c3().domain(), accuracy: track.acc, local_point, own: &track.own, own_eps: track.own_eps };
        let point = point_visibility(def, &ctx, &scene[0], &sample)?;
        let mut supports = Vec::with_capacity(track.own.len());
        for &fi in &track.own {
            if cancel() {
                return Err(HlrError::Cancelled);
            }
            let face = def.faces.get(fi).ok_or(HlrError::Invalid("diagnostic support face"))?;
            let uv = face.surface.inverse(local_point, None);
            let surface_point = uv.map(|uv| face.surface.point(uv));
            let tolerance = 2. * face.trim.chord_error + 1e-7 * (face.hi - face.lo).len();
            supports.push(SupportObservation {
                face: fi,
                analytic: format!("{:?}", face.analytic),
                uv,
                surface_point: surface_point.map(|p| p + def.offset),
                inverse_residual_mm: surface_point.map(|p| p.dist(local_point)),
                normal_dot_sight: uv.and_then(|uv| face.surface.normal(uv)).map(|n| n.dot(view.d)),
                inside_trim: uv.and_then(|uv| face.trim.contains(uv)),
                near_boundary: surface_point.map(|p| face.trim.near(p, tolerance)),
                trim_tolerance_mm: tolerance,
            });
        }
        result.push(SilhouetteProbe { track: request.track, source: track.source, track_range: track.range, support_observations: supports, point });
    }
    Ok(result)
}

/// Inspect one native edge parameter in a single, unplaced prepared body. Work is limited
/// to 4096 faces and 16384 hits, and observes cancellation before each surface query.
pub fn edge_visibility(
    def: &Prepared,
    edge: usize,
    parameter: f64,
    view: &View,
    opts: &Options,
    cancel: &dyn Fn() -> bool,
) -> Result<EdgeProbe, HlrError> {
    if cancel() {
        return Err(HlrError::Cancelled);
    }
    if def.faces.len() > 4096 {
        return Err(HlrError::Budget("diagnostic face"));
    }
    let e = def.edges.get(edge).ok_or(HlrError::Invalid("diagnostic edge"))?;
    let domain = e.curve.domain();
    if !parameter.is_finite() || parameter < domain[0] || parameter > domain[1] {
        return Err(HlrError::Invalid("diagnostic parameter"));
    }
    let defs = std::slice::from_ref(def);
    let unplaced = [Placement { body: 0, rotation: [Vec3::X, Vec3::Y, Vec3::Z], translation: Vec3::ZERO }];
    let origin = scene_centre(defs, &unplaced)?;
    let scene = [Placement { translation: def.offset - origin, ..unplaced[0] }];
    let ctx = context(defs, &scene, view, opts, cancel)?;
    let local_point = e.curve.at(parameter);
    let far = 1e-14 * def.offset.x.abs().max(def.offset.y.abs()).max(def.offset.z.abs());
    let size = e.faces.iter().filter_map(|f| def.faces.get(*f)).map(|f| (f.hi - f.lo).len()).fold(0., f64::max);
    let base = (1e-9 * ctx.scale).max(1e-6 * size).max(far);
    let own_eps = match e.acc {
        CurveAccuracy::Bounded { max_deviation_mm: tol } => base.max(10. * tol),
        CurveAccuracy::Sampled { max_sample_deviation_mm: tol } => base.max(10. * tol),
        _ => base,
    };
    let sample = PointSample { parameter, domain, accuracy: e.acc, local_point, own: &e.faces, own_eps };
    Ok(EdgeProbe { edge, point: point_visibility(def, &ctx, &scene[0], &sample)? })
}

fn point_visibility(def: &Prepared, ctx: &Ctx<'_>, placement: &Placement, sample: &PointSample<'_>) -> Result<PointProbe, HlrError> {
    let view = &ctx.view;
    let (parameter, domain, local_point, own_eps) = (sample.parameter, sample.domain, sample.local_point, sample.own_eps);
    let world = placement.apply(local_point);
    if !world.is_finite() {
        return Err(HlrError::Invalid("diagnostic point"));
    }
    let source_error = super::accuracy_error(sample.accuracy);
    let classification = classify_point(ctx, 0, sample.own, own_eps, source_error, world);
    let toward = -view.d;
    // Use the production classifier's world-to-local round trip. Reusing the raw curve
    // point instead can differ by an ulp, which changes a boundary-margin observation.
    let p0 = placement.local(world);
    let dl = placement.local_dir(toward);
    let mut records = Vec::new();
    let mut hits = Vec::new();
    let mut tested_faces = 0;
    let mut face_queries = Vec::new();
    let mut hidden = false;
    let mut uncertain = false;
    for (fi, f) in def.faces.iter().enumerate() {
        if (ctx.cancel)() {
            return Err(HlrError::Cancelled);
        }
        let is_own = sample.own.contains(&fi);
        let pad = Vec3::new(source_error, source_error, source_error);
        let Some([l0, l1]) = slab(p0, dl, f.lo - pad, f.hi + pad) else { continue };
        let eps = if is_own { own_eps } else { ctx.eps_other };
        let range = [l0.max(eps), l1];
        if range[1] < range[0] {
            continue;
        }
        tested_faces += 1;
        hits.clear();
        f.surface.ray_as(&f.analytic, p0, dl, range, &mut hits);
        let torus_candidates = if let super::surface::Analytic::Torus { c, a, major, minor } = f.analytic {
            super::surface::torus_roots(p0 - c, dl, a, major, minor, range)
                .into_iter()
                .map(|(parameter, grazing)| {
                    let p = p0 + dl * parameter;
                    let uv = f.surface.inverse(p, None);
                    TorusCandidate {
                        parameter,
                        grazing,
                        uv,
                        inverse_residual_mm: uv.map(|x| f.surface.point(x).dist(p)),
                        inverse_acceptance_mm: 1e-7 * (1. + p.len()),
                        in_range: parameter >= range[0] && parameter <= range[1],
                    }
                })
                .collect()
        } else {
            Vec::new()
        };
        let potential_contacts: Vec<_> = f
            .surface
            .near_contacts(&f.analytic, p0, dl, range, source_error)
            .iter()
            .map(|h| {
                let p = f.surface.point(h.uv);
                let tol = 2. * f.trim.chord_error + 1e-7 * (f.hi - f.lo).len();
                let near = f.trim.near(p, tol);
                let inside = f.trim.contains(h.uv);
                PotentialContact {
                    ray_parameter: h.lam,
                    surface_point: p + def.offset,
                    ray_point: p0 + dl * h.lam + def.offset,
                    distance_mm: h.distance_mm,
                    margin_mm: h.margin_mm,
                    near_boundary: near,
                    inside_trim: inside,
                    relevant: near || inside != Some(false),
                }
            })
            .collect();
        let sensitive = potential_contacts.iter().any(|h| h.relevant);
        uncertain |= sensitive;
        face_queries.push(FaceQuery {
            face: fi,
            analytic: format!("{:?}", f.analytic),
            range,
            accepted_hits: hits.len(),
            torus_candidates,
            contact_sensitive: sensitive,
            potential_contacts,
        });
        if records.len().saturating_add(hits.len()) > 16384 {
            return Err(HlrError::Budget("diagnostic hit"));
        }
        for h in &hits {
            let p = f.surface.point(h.uv);
            let ray_point = p0 + dl * h.lam;
            let tol = 2. * f.trim.chord_error + 1e-7 * (f.hi - f.lo).len();
            let near = f.trim.near(p, tol);
            let inside = f.trim.contains(h.uv);
            let effect = if near {
                uncertain = true;
                "boundary_margin"
            } else {
                match inside {
                    None => {
                        uncertain = true;
                        "unsupported_trim"
                    }
                    Some(false) => "outside_trim",
                    Some(true) if (f.freeform && !ctx.freeform) || h.grazing || sensitive || (!is_own && h.lam < ctx.eps_own) => {
                        uncertain = true;
                        "uncertain_hit"
                    }
                    Some(true) => {
                        hidden = true;
                        "occludes"
                    }
                }
            };
            records.push(FaceHit {
                face: fi,
                analytic: format!("{:?}", f.analytic),
                own: is_own,
                ray_range: range,
                ray_parameter: h.lam,
                uv: h.uv,
                surface_domain: f.surface.domain(),
                trim_bounds: f.trim.uv_bounds(),
                surface_point: p + def.offset,
                ray_point: ray_point + def.offset,
                ray_residual_mm: p.dist(ray_point),
                trim_tolerance_mm: tol,
                near_boundary: near,
                inside_trim: inside,
                grazing: h.grazing,
                effect,
            });
        }
    }
    let reduced = if hidden {
        Visibility::Hidden
    } else if uncertain {
        Visibility::Uncertain
    } else {
        Visibility::Visible
    };
    let source_point = local_point + def.offset;
    Ok(PointProbe {
        parameter,
        domain,
        accuracy: sample.accuracy,
        source_point,
        screen: view.screen(source_point),
        toward_viewer: toward,
        support_faces: sample.own.to_vec(),
        own_epsilon_mm: own_eps,
        other_epsilon_mm: ctx.eps_other,
        classification,
        hit_reduction_matches: reduced == classification,
        tested_faces,
        face_queries,
        hits: records,
    })
}
