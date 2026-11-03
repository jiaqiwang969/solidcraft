//! A face whose edge is an intersection curve that does not evaluate (#1252): meshing it neither
//! panics nor gives non-finite points.

use solidcraft_geom::brep::{self as b, Curve, Curve2, Surface};

/// A plane at height `z` with its parameters along x and y.
fn plane(z: f64) -> Surface {
    Surface::Plane { origin: [0.0, 0.0, z], u_axis: [1.0, 0.0, 0.0], v_axis: [0.0, 1.0, 0.0] }
}

/// A triangular sheet at z = 0.5 whose first edge is the intersection of two planes: the parallel
/// planes `z = 0` and `z = 1` when `support_gap` is `1.0` (the projection onto both never
/// converges), the same plane twice when it is `0.0` (the leader lies on both and always
/// evaluates).
fn triangle_with_intersection_edge(support_gap: f64) -> b::Brep {
    let corners = [[0.0, 0.0, 0.5], [1.0, 0.0, 0.5], [0.5, 1.0, 0.5]];
    let uv = [[0.0, 0.0], [1.0, 0.0], [0.5, 1.0]];
    let traced = Curve::SurfaceIntersection {
        surfaces: [Box::new(plane(0.5 - support_gap / 2.0)), Box::new(plane(0.5 + support_gap / 2.0))],
        leader: Box::new(Curve::Polyline { points: vec![corners[0], corners[1]] }),
        tolerance: 0.01,
    };
    let curve = |k: usize| if k == 0 { traced.clone() } else { Curve::Line { start: corners[k], end: corners[(k + 1) % 3] } };
    let edges = (0..3)
        .map(|k| b::Edge {
            nurbs: None,
            curve: curve(k),
            interval: [0.0, 1.0],
            vertices: [k as u32, ((k + 1) % 3) as u32],
            tolerance: 1e-6,
            uses: Vec::new(),
        })
        .collect();
    let uses = (0..3)
        .map(|k| b::OrientedEdge {
            edge: k as u32,
            forward: true,
            pcurve: Curve2::Spline { knots: vec![0.0, 0.0, 1.0, 1.0], points: vec![uv[k], uv[(k + 1) % 3]] },
        })
        .collect();
    let face = b::Face { nurbs: None, surface: plane(0.5), domain: [[0.0, 1.0], [0.0, 1.0]], loops: vec![b::Loop::Edges { edges: uses }] };
    let loose = b::Brep {
        version: b::VERSION,
        tolerance: 1e-6,
        regions: vec![b::Region { shells: vec![0], void: false }],
        shells: vec![b::Shell::Faces { faces: vec![b::OrientedFace { face: 0, forward: true }], wires: Vec::new() }],
        faces: vec![face],
        edges,
        vertices: corners.iter().map(|p| b::Vertex { point: *p, tolerance: 1e-6 }).collect(),
    };
    loose.canonical().unwrap().brep
}

/// Meshing an edge whose intersection curve does not evaluate (#1252): the two supports here are
/// parallel planes, so the projection never converges. Nothing may panic (wasm aborts on one, and
/// the native guard prints the message first), and a mesh that comes back is finite.
#[test]
fn meshing_an_edge_that_does_not_evaluate_neither_panics_nor_goes_non_finite() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    for gap in [1.0, 0.0] {
        let me = std::thread::current().id();
        let count = Arc::new(AtomicUsize::new(0));
        let c2 = count.clone();
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |_| {
            if std::thread::current().id() == me {
                c2.fetch_add(1, Ordering::SeqCst);
            }
        }));
        let meshed = std::panic::catch_unwind(|| {
            crate::Body::from_record(triangle_with_intersection_edge(gap)).and_then(|body| body.mesh(&crate::MeshTolerance::chord(0.01)))
        });
        std::panic::set_hook(prev);
        assert_eq!(count.load(Ordering::SeqCst), 0, "gap {gap}: panics while meshing");
        if let Ok(Ok(mesh)) = meshed {
            assert!(mesh.positions.iter().all(|p| p.is_finite()), "gap {gap}");
        }
    }
}

#[test]
fn shoulder_slot_preserves_the_analytic_cutting_planes() {
    use solidcraft_geom::{Loop2, Plane as ProfilePlane, Region2, Seg2, Vec2};
    let points = [
        (0., 0.),
        (0., 12.5),
        (25., 12.5),
        (25., 15.),
        (45., 15.),
        (55., 20.),
        (100., 20.),
        (100., 24.),
        (125., 24.),
        (125., 21.),
        (180., 21.),
        (180., 0.),
    ];
    let region = Region2 { outer: Loop2::polygon(&points.map(|(x, y)| Vec2::new(x, y))), holes: vec![] };
    let shaft = crate::revolve(&ProfilePlane::XY, &[region], Vec2::ZERO, Vec2::X, std::f64::consts::TAU).unwrap().remove(0);
    let a = Vec2::new(-10., -4.);
    let b = Vec2::new(100., -4.);
    let c = Vec2::new(100., 4.);
    let d = Vec2::new(-10., 4.);
    let slot = Region2 {
        outer: Loop2 {
            segs: vec![
                Seg2::Line { a, b },
                Seg2::Arc { center: Vec2::new(100., 0.), radius: 4., start: -std::f64::consts::FRAC_PI_2, sweep: std::f64::consts::PI },
                Seg2::Line { a: c, b: d },
                Seg2::Line { a: d, b: a },
            ],
        },
        holes: vec![],
    };
    let tool = crate::extrude_planar_regions(&ProfilePlane::XZ, &[slot], 16., 56.).unwrap();
    let cut = crate::boolean(&shaft, &tool, crate::BoolOp::Cut);
    let cut = cut.unwrap().unwrap();
    assert_eq!(cut.shell_count(), 1);
    assert!(cut.history().is_some());
    let floors: Vec<_> = (0..cut.face_count()).filter_map(|i| cut.planar_face(i)).filter(|p| p.normal().y.abs() > 0.99).collect();
    assert!(!floors.is_empty());
    for p in floors {
        assert!((p.origin.y - 16.).abs() < 1e-8, "{p:?}");
        assert!(p.normal().x.abs() < 1e-9 && p.normal().z.abs() < 1e-9, "{p:?}");
    }
}

/// The support-precision regression retained from the truck healing experiment:
/// extending the shaft must not straighten the short slot/chamfer intersection.
#[test]
fn a_slot_through_a_chamfer_stays_on_both_supports() {
    use solidcraft_geom::{Vec3, brep::eval::CurveEval};
    for length in [80.0, 240.0] {
        let shaft = crate::cylinder(Vec3::ZERO, Vec3::X, 12.0, length).unwrap();
        let shaft = crate::chamfer(&shaft, &[Vec3::new(0.0, 12.0, 0.0)], 1.0).unwrap();
        let tool = crate::box_solid(Vec3::new(-2.0, -4.0, 9.0), Vec3::new(20.0, 4.0, 30.0)).unwrap();
        let body = crate::boolean(&shaft, &tool, crate::BoolOp::Cut).unwrap().unwrap();
        let record = crate::brep_record(&body).unwrap();
        record.check_geometry().unwrap();
        let mut count = 0;
        let mut worst = 0.0_f64;
        for edge in &record.edges {
            let curve = CurveEval::new(edge.parameters()).unwrap();
            let [lo, hi] = edge.interval;
            for k in 1..200 {
                let p = curve.point(lo + (hi - lo) * f64::from(k) / 200.0).unwrap();
                if (1e-5..0.99999).contains(&p.x) && (p.y.abs() - 4.0).abs() < 1e-5 && p.z > 9.0 {
                    worst = worst.max((p.y.hypot(p.z) - (11.0 + p.x)).abs());
                    count += 1;
                }
            }
        }
        assert!(count >= 100, "the slot/chamfer intersection must be present");
        assert!(worst < 2e-5, "length {length}: curve recognition displaced the intersection by {worst} mm");
    }
}
