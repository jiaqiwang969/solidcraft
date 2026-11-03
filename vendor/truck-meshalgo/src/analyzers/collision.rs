use super::*;
use array_macro::array;

/// Find collisions between two polygon meshes and extract interference lines.
///
/// # Details
///
/// The algorithm for mesh extraction is as follows.
/// All the triangles are projected toward the axis to form intervals.
/// By looking at the overlap of these intervals, we can narrow down the interfering triangles.
/// The overlap of the intervals is obtained by sorting the endpoints.
///
/// # Remarks
///
/// We see that the surfaces that are in contact are not interfering with each other.
/// Therefore, polygon meshes included in one plane are not interfering.
pub trait Collision {
    /// If `self` and `other` collide, then returns only one interference line.
    /// Otherwise, returns `None`.
    fn collide_with(&self, other: &PolygonMesh) -> Option<(Point3, Point3)>;
    /// Extract all interference lines between `self` and `other`.
    /// # Remarks
    /// The results is not arranged so that included lines make continuous maximal polyline curve.
    fn extract_interference(&self, other: &PolygonMesh) -> Vec<(Point3, Point3)>;
}

impl Collision for PolygonMesh {
    #[inline(always)]
    fn collide_with(&self, other: &PolygonMesh) -> Option<(Point3, Point3)> {
        are_colliding(self, other)
    }
    #[inline(always)]
    fn extract_interference(&self, other: &PolygonMesh) -> Vec<(Point3, Point3)> {
        collision(self, other)
    }
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum EndPointType {
    Front,
    Back,
}

#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
struct EndPoint {
    entity: f64,
    r#type: EndPointType,
    segnum: usize,
    index: usize,
}

impl EndPoint {
    #[inline(always)]
    fn new(entity: f64, r#type: EndPointType, segnum: usize, index: usize) -> EndPoint {
        EndPoint {
            entity,
            r#type,
            segnum,
            index,
        }
    }
    #[inline(always)]
    fn from_seg(seg: (f64, f64), segnum: usize, index: usize) -> Vec<EndPoint> {
        vec![
            EndPoint::new(seg.0, EndPointType::Front, segnum, index),
            EndPoint::new(seg.1, EndPointType::Back, segnum, index),
        ]
    }
}

// A surface evaluation can place a boundary point one ULP away from the
// incident face (for example 159.99999999999997 instead of 160). This bound
// accounts for floating-point arithmetic only, not a modelling tolerance.
fn scalar_roundoff(a: f64, b: f64) -> f64 {
    8.0 * f64::EPSILON * a.abs().max(b.abs()).max(1.0)
}

fn tri_to_seg(tri: [Point3; 3], unit: Vector3, roundoff: bool) -> (f64, f64) {
    let a = tri[0].to_vec().dot(unit);
    let b = tri[1].to_vec().dot(unit);
    let c = tri[2].to_vec().dot(unit);
    let scale = tri.iter().map(|p| p.x.abs()*unit.x.abs() + p.y.abs()*unit.y.abs() + p.z.abs()*unit.z.abs()).fold(1.0_f64, f64::max);
    let pad = if roundoff { 8.0 * f64::EPSILON * scale } else { 0.0 };
    (f64::min(f64::min(a, b), c) - pad, f64::max(f64::max(a, b), c) + pad)
}

#[cfg(any(test, feature = "test-hooks"))]
fn sorted_endpoints<I, J>(iter0: I, iter1: J, unit: Vector3) -> Vec<EndPoint>
where
    I: IntoIterator<Item = [Point3; 3]>,
    J: IntoIterator<Item = [Point3; 3]>, {
    sorted_endpoints_mode(iter0, iter1, unit, false)
}

fn sorted_endpoints_mode<I, J>(iter0: I, iter1: J, unit: Vector3, roundoff: bool) -> Vec<EndPoint>
where
    I: IntoIterator<Item = [Point3; 3]>,
    J: IntoIterator<Item = [Point3; 3]>, {
    let mut res: Vec<EndPoint> = iter0
        .into_iter()
        .enumerate()
        .filter(|(_, tri)| !(tri[1] - tri[0]).cross(tri[2] - tri[0]).so_small())
        .flat_map(|(i, tri)| EndPoint::from_seg(tri_to_seg(tri, unit, roundoff), 0, i))
        .chain(
            iter1
                .into_iter()
                .enumerate()
                .filter(|(_, tri)| !(tri[1] - tri[0]).cross(tri[2] - tri[0]).so_small())
                .flat_map(|(i, tri)| EndPoint::from_seg(tri_to_seg(tri, unit, roundoff), 1, i)),
        )
        .collect();
    res.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Greater));
    res
}

fn collide_seg_triangle(seg: [Point3; 2], tri: [Point3; 3], roundoff: bool) -> Option<Point3> {
    let ab = tri[1] - tri[0];
    let bc = tri[2] - tri[1];
    let ca = tri[0] - tri[2];
    let nor = ab.cross(ca);
    if nor.so_small() {
        return None;
    }
    let ap = seg[0] - tri[0];
    let aq = seg[1] - tri[0];
    if !roundoff {
        let dotapnor = ap.dot(nor);
        let dotaqnor = aq.dot(nor);
        if dotapnor * dotaqnor > 0.0 { return None; }
        let h = seg[0] + dotapnor / (dotapnor - dotaqnor) * (seg[1] - seg[0]);
        return (f64::signum(ab.cross(nor).dot(h - tri[0]) + TOLERANCE2)
            + f64::signum(bc.cross(nor).dot(h - tri[1]) + TOLERANCE2)
            + f64::signum(ca.cross(nor).dot(h - tri[2]) + TOLERANCE2) >= 2.0).then_some(h);
    }
    let scale = tri.iter().chain(seg.iter()).map(|p| p.x.abs().max(p.y.abs()).max(p.z.abs())).fold(1.0_f64, f64::max);
    let bound = 8.0 * f64::EPSILON * scale * (nor.x.abs() + nor.y.abs() + nor.z.abs());
    let (a, b) = (ap.dot(nor), aq.dot(nor));
    if ![a, b, bound].iter().all(|v| v.is_finite()) {
        return None;
    }
    let dotapnor = if a.abs() <= bound { 0.0 } else { a };
    let dotaqnor = if b.abs() <= bound { 0.0 } else { b };
    // Coplanar segments do not define a transverse intersection. Keeping the
    // zero denominator out also prevents NaN points on coincident faces.
    if (dotapnor == 0.0 && dotaqnor == 0.0)
        || (dotapnor > 0.0 && dotaqnor > 0.0)
        || (dotapnor < 0.0 && dotaqnor < 0.0)
    {
        return None;
    }
    let h = seg[0] + dotapnor / (dotapnor - dotaqnor) * (seg[1] - seg[0]);
    let inside_edge = |edge: Vector3, origin: Point3| {
        let side = edge.cross(nor);
        let value = side.dot(h - origin);
        // This signed half-space expression scales with the edge and normal;
        // a fixed epsilon alone loses contacts on long, thin tool walls.
        let rounding = 8.0 * f64::EPSILON * scale * (side.x.abs() + side.y.abs() + side.z.abs());
        value.is_finite() && rounding.is_finite() && value >= -(TOLERANCE2 + rounding)
    };
    if inside_edge(ab, tri[0]) && inside_edge(bc, tri[1]) && inside_edge(ca, tri[2]) {
        Some(h)
    } else {
        None
    }
}

fn collide_triangles(tri0: [Point3; 3], tri1: [Point3; 3]) -> Option<(Point3, Point3)> {
    collide_triangles_mode(tri0, tri1, false)
}

fn collide_triangles_mode(tri0: [Point3; 3], tri1: [Point3; 3], roundoff: bool) -> Option<(Point3, Point3)> {
    let mut tuple = (None, None);
    [
        collide_seg_triangle([tri0[0], tri0[1]], tri1, roundoff),
        collide_seg_triangle([tri0[1], tri0[2]], tri1, roundoff),
        collide_seg_triangle([tri0[2], tri0[0]], tri1, roundoff),
        collide_seg_triangle([tri1[0], tri1[1]], tri0, roundoff),
        collide_seg_triangle([tri1[1], tri1[2]], tri0, roundoff),
        collide_seg_triangle([tri1[2], tri1[0]], tri0, roundoff),
    ]
    .into_iter()
    .for_each(|pt| match tuple {
        (None, _) => tuple.0 = pt,
        (Some(_), None) => tuple.1 = pt,
        (Some(ref mut p), Some(ref mut q)) => {
            if let Some(pt) = pt {
                let dist0 = pt.distance2(*p);
                let dist1 = pt.distance2(*q);
                let dist2 = p.distance2(*q);
                if dist2 < dist0 {
                    *q = pt;
                } else if dist2 < dist1 {
                    *p = pt;
                }
            }
        }
    });
    match tuple {
        (Some(a), Some(b)) => Some((a, b)),
        _ => None,
    }
}

fn make_pos_tri(poly: &PolygonMesh, face: [StandardVertex; 3]) -> [Point3; 3] {
    array![i => poly.positions()[face[i].pos]; 3]
}

/// Triangles of a mesh with their bounding boxes, each computed once.
fn boxed_triangles(poly: &PolygonMesh) -> Vec<([Point3; 3], BoundingBox<Point3>)> {
    poly.faces()
        .triangle_iter()
        .map(|face| {
            let tri = make_pos_tri(poly, face);
            (tri, tri.iter().collect())
        })
        .collect()
}

/// Whether two triangles' bounding boxes miss each other (touching boxes do not miss).
#[inline(always)]
#[cfg(any(test, feature = "test-hooks"))]
fn disjoint_boxes(bdb0: &BoundingBox<Point3>, bdb1: &BoundingBox<Point3>) -> bool {
    disjoint_boxes_mode(bdb0, bdb1, false)
}

fn disjoint_boxes_mode(bdb0: &BoundingBox<Point3>, bdb1: &BoundingBox<Point3>, roundoff: bool) -> bool {
    if !roundoff {
        return bdb0.max()[0] < bdb1.min()[0] || bdb1.max()[0] < bdb0.min()[0]
            || bdb0.max()[1] < bdb1.min()[1] || bdb1.max()[1] < bdb0.min()[1]
            || bdb0.max()[2] < bdb1.min()[2] || bdb1.max()[2] < bdb0.min()[2];
    }
    (0..3).any(|i| {
        bdb1.min()[i] - bdb0.max()[i] > scalar_roundoff(bdb1.min()[i], bdb0.max()[i])
            || bdb0.min()[i] - bdb1.max()[i] > scalar_roundoff(bdb0.min()[i], bdb1.max()[i])
    })
}

/// Pairs of triangles (one from each mesh) whose bounding boxes overlap, in the order the
/// sweep along `unit` meets them.
///
/// The sweep keeps the triangles whose interval along `unit` is open. When a triangle opens,
/// it pairs with every open triangle of the other mesh whose box it overlaps. The pairs and
/// their order are the same as the earlier version, which cloned the open list for every
/// triangle and rejected disjoint boxes afterwards (rebuilding both boxes for each pair).
fn candidate_pairs(
    tris0: &[([Point3; 3], BoundingBox<Point3>)],
    tris1: &[([Point3; 3], BoundingBox<Point3>)],
    unit: Vector3,
) -> Vec<(usize, usize)> {
    candidate_pairs_mode(tris0, tris1, unit, false)
}

fn candidate_pairs_mode(
    tris0: &[([Point3; 3], BoundingBox<Point3>)],
    tris1: &[([Point3; 3], BoundingBox<Point3>)],
    unit: Vector3,
    roundoff: bool,
) -> Vec<(usize, usize)> {
    let endpoints = sorted_endpoints_mode(
        tris0.iter().map(|(t, _)| *t),
        tris1.iter().map(|(t, _)| *t),
        unit,
        roundoff,
    );
    let boxes = [tris0, tris1];
    let mut open = [Vec::<usize>::new(), Vec::<usize>::new()];
    let mut pairs = Vec::new();
    for EndPoint {
        r#type,
        segnum,
        index,
        ..
    } in endpoints
    {
        match r#type {
            EndPointType::Front => {
                let mine = &boxes[segnum][index].1;
                for &other in &open[1 - segnum] {
                    if !disjoint_boxes_mode(mine, &boxes[1 - segnum][other].1, roundoff) {
                        pairs.push(if segnum == 0 {
                            (index, other)
                        } else {
                            (other, index)
                        });
                    }
                }
                open[segnum].push(index);
            }
            EndPointType::Back => {
                // Every interval opens before it closes unless a NaN broke the sort. Then the
                // interval stays open: more box tests, never a panic (the web build aborts on any).
                if let Some(i) = open[segnum].iter().position(|idx| *idx == index) {
                    open[segnum].swap_remove(i);
                }
            }
        }
    }
    pairs
}

fn collision(poly0: &PolygonMesh, poly1: &PolygonMesh) -> Vec<(Point3, Point3)> {
    collision_mode(poly0, poly1, false)
}

/// Re-evaluate a failed solid construction with arithmetic roundoff accounted
/// for in contact predicates. Coordinates, surfaces and model tolerances stay
/// unchanged. The ordinary collision query retains its previous behaviour.
pub fn extract_roundoff_interference(poly0: &PolygonMesh, poly1: &PolygonMesh) -> Vec<(Point3, Point3)> {
    collision_mode(poly0, poly1, true)
}

fn collision_mode(poly0: &PolygonMesh, poly1: &PolygonMesh, roundoff: bool) -> Vec<(Point3, Point3)> {
    let unit = match poly0.positions().is_empty() {
        true => return Vec::new(),
        false => hash::take_one_unit(poly0.positions()[0]),
    };
    let (tris0, tris1) = (boxed_triangles(poly0), boxed_triangles(poly1));
    let pairs = candidate_pairs_mode(&tris0, &tris1, unit, roundoff);
    let test = |&(i, j): &(usize, usize)| collide_triangles_mode(tris0[i].0, tris1[j].0, roundoff);
    #[cfg(not(target_arch = "wasm32"))]
    {
        use rayon::prelude::*;
        // Indexed parallel collect: results in the same order as the sequential loop.
        pairs.par_iter().filter_map(test).collect()
    }
    #[cfg(target_arch = "wasm32")]
    {
        pairs.iter().filter_map(test).collect()
    }
}

fn are_colliding(poly0: &PolygonMesh, poly1: &PolygonMesh) -> Option<(Point3, Point3)> {
    let unit = match poly0.positions().is_empty() {
        true => return None,
        false => hash::take_one_unit(poly0.positions()[0]),
    };
    let (tris0, tris1) = (boxed_triangles(poly0), boxed_triangles(poly1));
    let pairs = candidate_pairs(&tris0, &tris1, unit);
    let test = |&(i, j): &(usize, usize)| collide_triangles(tris0[i].0, tris1[j].0);
    #[cfg(not(target_arch = "wasm32"))]
    {
        use rayon::prelude::*;
        // The first collision in sweep order, as the sequential search returns.
        pairs.par_iter().find_map_first(test)
    }
    #[cfg(target_arch = "wasm32")]
    {
        pairs.iter().find_map(test)
    }
}

#[test]
fn collide_triangles_test() {
    let tri0 = [
        Point3::origin(),
        Point3::new(1.0, 0.0, 0.0),
        Point3::new(0.0, 1.0, 0.0),
    ];
    let tri1 = [
        Point3::new(0.0, 0.0, -1.0),
        Point3::new(-1.0, -1.0, 1.0),
        Point3::new(1.0, 1.0, 1.0),
    ];
    assert!(collide_triangles(tri0, tri1).is_some());
    let tri0 = [
        Point3::new(0.0, 0.0, 0.0),
        Point3::new(1.0, 0.0, 0.0),
        Point3::new(0.0, 1.0, 0.0),
    ];
    let tri1 = [
        Point3::new(0.0, 0.0, 0.5),
        Point3::new(1.0, 0.0, 1.0),
        Point3::new(1.0, 1.0, 1.0),
    ];
    assert!(collide_triangles(tri0, tri1).is_none());
}

#[test]
fn boundary_contact_survives_one_ulp_of_mesh_roundoff() {
    for x in [1.0_f64, 64.0, 160.0, 4096.0] {
        let face = [Point3::new(x, -2., -2.), Point3::new(x, 2., -2.), Point3::new(x, 0., 2.)];
        let wall = [Point3::new(x.next_down(), -0.5, 0.), Point3::new(x.next_down(), 0.5, 0.), Point3::new(x-4., 0., 0.)];
        for (a, b) in [(face, wall), (wall, face)] {
            let ba = vec![(a, a.iter().collect())];
            let bb = vec![(b, b.iter().collect())];
            for direction in [Vector3::unit_x(), Vector3::new(1., 2., 3.).normalize()] {
                assert_eq!(candidate_pairs_mode(&ba, &bb, direction, true), vec![(0, 0)], "candidate x={x}");
            }
            let (p, q) = collide_triangles_mode(a, b, true).expect("transverse contact line survives roundoff");
            assert!((p.x-x).abs() <= 2e-12 && (q.x-x).abs() <= 2e-12, "x={x}: {p:?} {q:?}");
            assert!(p.z.abs() < 1e-12 && q.z.abs() < 1e-12);
            assert!((p.distance(q)-1.).abs() < 1e-12, "x={x}: {p:?} {q:?}");
        }
    }
}

#[test]
fn roundoff_contact_keeps_real_gaps_and_coplanar_faces_separate() {
    for x in [1.0_f64, 64.0, 160.0, 4096.0] {
        let face = [Point3::new(x, -2., -2.), Point3::new(x, 2., -2.), Point3::new(x, 0., 2.)];
        for gap in [1e-9, 1e-7, 1e-4] {
            let wall = [Point3::new(x-gap, -0.5, 0.), Point3::new(x-gap, 0.5, 0.), Point3::new(x-4., 0., 0.)];
            let ba = vec![(face, face.iter().collect())];
            let bb = vec![(wall, wall.iter().collect())];
            assert!(candidate_pairs(&ba, &bb, Vector3::unit_x()).is_empty());
            assert!(collide_triangles(face, wall).is_none());
            assert!(candidate_pairs_mode(&ba, &bb, Vector3::unit_x(), true).is_empty());
            assert!(collide_triangles_mode(face, wall, true).is_none());
        }
        assert!(collide_triangles(face, face).is_none());
        assert!(collide_triangles_mode(face, face, true).is_none());
    }
}

#[test]
fn long_wall_contact_keeps_the_clipped_intersection_interval() {
    for x in [1.0_f64, 64.0, 160.0, 4096.0] {
        let face = [Point3::new(x, -2., -2.), Point3::new(x, 2., -2.), Point3::new(x, 0., 2.)];
        // Both ends of the wall's contact edge lie outside the face: the
        // interval must come from clipping the face's own edges against it.
        let wall = [Point3::new(x.next_down(), -40., 0.), Point3::new(x.next_down(), 40., 0.), Point3::new(x-160., 0., 0.)];
        for (a, b) in [(face, wall), (wall, face)] {
            let (p, q) = collide_triangles_mode(a, b, true).expect("roundoff must not remove a clipped contact line");
            assert!((p.x-x).abs() <= 2e-12 && (q.x-x).abs() <= 2e-12, "x={x}: {p:?} {q:?}");
            assert!(p.z.abs() < 1e-12 && q.z.abs() < 1e-12);
            assert!((p.distance(q)-2.).abs() < 1e-12, "x={x}: {p:?} {q:?}");
        }
        for gap in [1e-9, 1e-7] {
            let separated = [Point3::new(x-gap, -40., 0.), Point3::new(x-gap, 40., 0.), Point3::new(x-160., 0., 0.)];
            for roundoff in [false, true] {
                assert!(collide_triangles_mode(face, separated, roundoff).is_none(), "real gap {gap} at x={x}");
                assert!(collide_triangles_mode(separated, face, roundoff).is_none(), "reversed real gap {gap} at x={x}");
            }
        }
    }
}

/// The candidate search before boxes were computed once and the open list stopped being
/// cloned: kept to check the new search finds the same collisions in the same order.
///
/// Not part of the API. Built only for tests, or with the `test-hooks` feature, so the
/// SolidCraft kernel tests can run these checks in CI (this crate's own tests do not run there).
#[cfg(any(test, feature = "test-hooks"))]
#[doc(hidden)]
pub mod test_hooks {
    use super::*;

    fn old_pairs(sort_endpoints: Vec<EndPoint>) -> Vec<(usize, usize)> {
        let mut current = [Vec::<usize>::new(), Vec::<usize>::new()];
        let mut out = Vec::new();
        for EndPoint { r#type, segnum, index, .. } in sort_endpoints {
            match r#type {
                EndPointType::Front => {
                    current[segnum].push(index);
                    for i in current[1 - segnum].clone() {
                        out.push(if segnum == 0 { (index, i) } else { (i, index) });
                    }
                }
                EndPointType::Back => {
                    if let Some(i) = current[segnum].iter().position(|idx| *idx == index) {
                        current[segnum].swap_remove(i);
                    }
                }
            }
        }
        out
    }

    fn old_disjoint(tri0: [Point3; 3], tri1: [Point3; 3]) -> bool {
        let b0: BoundingBox<Point3> = tri0.iter().collect();
        let b1: BoundingBox<Point3> = tri1.iter().collect();
        disjoint_boxes(&b0, &b1)
    }

    /// Small seeded generator (no extra dependency).
    struct Lcg(u64);
    impl Lcg {
        fn unit(&mut self) -> f64 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (self.0 >> 11) as f64 / (1u64 << 53) as f64
        }
        /// On a coarse grid half the time, so shared vertices, touching boxes and
        /// degenerate triangles come up.
        fn coord(&mut self) -> f64 {
            if self.unit() < 0.5 { (self.unit() * 8.0).floor() * 0.5 } else { self.unit() * 4.0 }
        }
        fn tri(&mut self) -> [Point3; 3] {
            array![_ => Point3::new(self.coord(), self.coord(), self.coord()); 3]
        }
    }

    /// A NaN coordinate breaks the endpoint sort: the search must go on without a panic.
    pub fn nan_triangles_do_not_panic() -> Result<(), String> {
        let good = [Point3::new(0.0, 0.0, 0.0), Point3::new(1.0, 0.0, 0.0), Point3::new(0.0, 1.0, 0.0)];
        let nan = [Point3::new(f64::NAN, 0.0, 0.0), Point3::new(1.0, f64::NAN, 0.0), Point3::new(0.0, 1.0, 0.5)];
        let crossing = [Point3::new(0.2, 0.2, -1.0), Point3::new(0.2, 0.2, 1.0), Point3::new(0.6, 0.2, 0.0)];
        for (a, b) in [(vec![good, nan], vec![crossing, nan]), (vec![nan, nan], vec![nan]), (vec![nan], vec![good])] {
            let ba: Vec<_> = a.iter().map(|t| (*t, t.iter().collect())).collect();
            let bb: Vec<_> = b.iter().map(|t| (*t, t.iter().collect())).collect();
            let r = std::panic::catch_unwind(|| candidate_pairs(&ba, &bb, Vector3::new(1.0, 0.0, 0.0)));
            if r.is_err() {
                return Err(format!("panicked on {a:?} / {b:?}"));
            }
        }
        Ok(())
    }

    /// The new candidate search finds the same collisions, in the same order, as the old one
    /// on random triangle sets. Returns how many collisions were compared.
    pub fn same_collisions_in_the_same_order(seeds: std::ops::Range<u64>) -> Result<usize, String> {
        let mut compared = 0;
        for seed in seeds {
            let mut r = Lcg(seed * 7919 + 1);
            let n0 = 1 + (r.unit() * 60.0) as usize;
            let n1 = 1 + (r.unit() * 60.0) as usize;
            let t0: Vec<[Point3; 3]> = (0..n0).map(|_| r.tri()).collect();
            let t1: Vec<[Point3; 3]> = (0..n1).map(|_| r.tri()).collect();
            let unit = hash::take_one_unit(t0[0][0]);

            let old: Vec<(Point3, Point3)> = old_pairs(sorted_endpoints(t0.iter().copied(), t1.iter().copied(), unit))
                .into_iter()
                .filter(|&(i, j)| !old_disjoint(t0[i], t1[j]))
                .filter_map(|(i, j)| collide_triangles(t0[i], t1[j]))
                .collect();

            let b0: Vec<_> = t0.iter().map(|t| (*t, t.iter().collect())).collect();
            let b1: Vec<_> = t1.iter().map(|t| (*t, t.iter().collect())).collect();
            let new: Vec<(Point3, Point3)> = candidate_pairs(&b0, &b1, unit)
                .into_iter()
                .filter_map(|(i, j)| collide_triangles(t0[i], t1[j]))
                .collect();

            if old.len() != new.len() {
                return Err(format!("seed {seed}: {} collisions before, {} now", old.len(), new.len()));
            }
            for (k, (a, b)) in old.iter().zip(&new).enumerate() {
                if !(a.0 == b.0 && a.1 == b.1) {
                    return Err(format!("seed {seed}: collision {k} differs: {a:?} vs {b:?}"));
                }
            }
            compared += old.len();
        }
        Ok(compared)
    }

    #[cfg(test)]
    #[test]
    fn hooks_pass() {
        assert_eq!(nan_triangles_do_not_panic(), Ok(()));
        let n = same_collisions_in_the_same_order(0..200);
        assert!(matches!(n, Ok(n) if n > 0), "{n:?}");
    }
}
