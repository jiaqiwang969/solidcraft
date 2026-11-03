use serde_json::{Value, json};

use super::{BodyListParams, DescribeParams, ModelEdges, ModelFaces, ModelThreads, ModelThreadsParams};
use crate::Session;
use crate::schema::tests::{assert_keys, assert_strict_keeps_todays_errors, conforms};
use crate::schema::{ParamCheck, input_schema_for, output_schema_for};

#[test]
fn schemas_and_the_key_check_agree() {
    assert_keys(input_schema_for::<DescribeParams>(), &["command"]);
    assert_keys(input_schema_for::<BodyListParams>(), &["body"]);
    assert_keys(input_schema_for::<ModelThreadsParams>(), &[]);
}

#[test]
fn strict_keeps_todays_errors() {
    assert_strict_keeps_todays_errors(&[
        ("engine.describe", json!({"command": 5})),
        ("engine.describe", json!({"command": null})),
        ("engine.describe", json!({"command": "solid.extrud"})),
        ("model.edges", json!({})),
        ("model.edges", json!({"body": "Nope"})),
        ("model.faces", json!({})),
        ("model.faces", json!({"body": "Nope"})),
    ]);
}

/// #10942: a body's edges and faces and the design's threads reply as their output schemas say,
/// in Warn without warnings for good calls and with them for a misspelt key. A cylindrical face
/// carries `cylinder` and a folded sheet's face `sheet_face`; strict refuses a `body` that isn't
/// text, which the handlers read as no body.
#[test]
fn model_lists_reply_with_their_schemas() {
    let (edges, faces, threads) = (output_schema_for::<ModelEdges>(), output_schema_for::<ModelFaces>(), output_schema_for::<ModelThreads>());
    let mut s = Session { param_check: ParamCheck::Warn, ..Session::default() };
    let run = |s: &mut Session, id: &str, p: Value| s.execute(id, &p).unwrap_or_else(|e| panic!("{id} {p}: {e}"));
    let matches = |schema: &schemars::Schema, r: &Value, warned: bool| {
        let schema = schema.as_value();
        assert!(conforms(schema, schema, r), "{r} doesn't match {schema}");
        assert_eq!(r.get("param_warnings").is_some(), warned, "{r}");
    };
    run(&mut s, "solid.cylinder", json!({"radius": 4, "height": 20, "body_name": "Shaft"}));
    run(&mut s, "solid.thread", json!({"face": [4, 0, 10], "designation": "M8"}));
    run(&mut s, "sheet.manage_rules", json!({"name": "R1", "thickness": "1 mm", "bend_radius": "1 mm", "active": true}));
    run(&mut s, "sketch.create", json!({"plane": "XY", "name": "Base"}));
    run(&mut s, "sketch.rectangle.two_point", json!({"p0": [20, 0], "p1": [60, 30]}));
    run(&mut s, "sketch.finish", json!({}));
    run(&mut s, "sheet.flange", json!({"sketch": "Base"}));
    let sheet = s.model.state().bodies.iter().map(|b| b.name.clone()).find(|n| n != "Shaft").unwrap_or_default();

    let r = run(&mut s, "model.edges", json!({"body": "Shaft"}));
    matches(&edges, &r, false);
    assert!(r["edges"].as_array().is_some_and(|e| !e.is_empty()), "{r}");
    let r = run(&mut s, "model.faces", json!({"body": "Shaft"}));
    matches(&faces, &r, false);
    assert!(r["faces"].as_array().into_iter().flatten().any(|f| f["cylinder"]["radius"].as_f64().is_some_and(|r| (r - 4.0).abs() < 1e-9)), "{r}");
    let r = run(&mut s, "model.faces", json!({"body": sheet}));
    matches(&faces, &r, false);
    assert!(r["faces"].as_array().into_iter().flatten().any(|f| f["sheet_face"].as_str().is_some_and(|n| n.starts_with("sheet:"))), "{r}");
    let r = run(&mut s, "model.threads", json!({}));
    matches(&threads, &r, false);
    assert_eq!((r["threads"][0]["designation"].as_str(), r["threads"][0]["hand"].as_str()), (Some("M8"), Some("right")), "{r}");

    matches(&edges, &run(&mut s, "model.edges", json!({"body": "Shaft", "bdy": 1})), true);
    matches(&faces, &run(&mut s, "model.faces", json!({"body": "Shaft", "bdy": 1})), true);
    matches(&threads, &run(&mut s, "model.threads", json!({"body": "Shaft"})), true);
    let mut strict = Session { param_check: ParamCheck::Strict, ..Session::default() };
    strict.execute("solid.cylinder", &json!({"radius": 4, "height": 20, "body_name": "Shaft"})).unwrap_or_else(|e| panic!("{e}"));
    for id in ["model.edges", "model.faces"] {
        let error = strict.execute(id, &json!({"body": 5})).err().map(|e| e.to_string()).unwrap_or_default();
        assert!(error.contains("`body` must be text"), "{id}: {error}");
    }
}

#[test]
fn describe_returns_both_schemas() {
    let mut s = Session::default();
    let d = s.execute("engine.describe", &json!({"command": "solid.box"})).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(d["id"], "solid.box");
    assert!(d["input"]["properties"].get("length").is_some());
    assert!(d["output"]["properties"].get("created_bodies").is_some());
    // Its own output schema lists the warnings warn mode can add.
    let own = s.execute("engine.describe", &json!({"command": "engine.describe"})).unwrap_or_else(|e| panic!("{e}"));
    assert!(own["output"]["properties"].get("param_warnings").is_some(), "{own}");
    let d = s.execute("engine.describe", &json!({"command": "solid.revolve"})).unwrap_or_else(|e| panic!("{e}"));
    assert!(d["input"].is_null() && d["output"].is_null(), "unconverted commands have no schemas yet");
    let e = s.execute("engine.describe", &json!({"command": "solid.extrud"})).err().map(|e| e.to_string());
    assert!(e.as_deref().is_some_and(|e| e.contains("did you mean `solid.extrude`")), "{e:?}");
}

#[test]
fn face_support_surfaces_survive_edge_treatments_and_parameter_changes() {
    use super::{AnalyticSurface, ModelEdges as ModelEdgesResult, ModelFaces as ModelFacesResult};
    let mut s = Session::default();
    for (command, params) in [
        ("parameters.add", json!({"name":"C", "expression":"1 mm"})),
        ("parameters.add", json!({"name":"R", "expression":"2 mm"})),
        ("solid.cylinder", json!({"diameter":20, "height":30, "axis":"X", "base":[5,0,0], "body_name":"Probe"})),
        ("solid.chamfer", json!({"body":"Probe", "edges":[[5,7.071067811865475,7.071067811865475]], "distance":"C"})),
        ("solid.fillet", json!({"body":"Probe", "edges":[[35,7.071067811865475,7.071067811865475]], "radius":"R", "fit":"exact"})),
    ] {
        s.execute(command, &params).unwrap_or_else(|e| panic!("{command}: {e}"));
    }
    s.param_check = crate::schema::ParamCheck::Strict;
    for (chamfer, radius) in [(1.0, 2.0), (1.5, 3.0)] {
        s.execute("parameters.change", &json!({"name":"C", "expression":format!("{chamfer} mm")})).unwrap();
        s.execute("parameters.change", &json!({"name":"R", "expression":format!("{radius} mm")})).unwrap();
        let raw = s.execute("model.faces", &json!({"body":"Probe"})).unwrap();
        let reply: ModelFacesResult = serde_json::from_value(raw).unwrap();
        assert!(reply.faces.iter().any(|f| matches!(f.analytic_surface,
            Some(AnalyticSurface::Cone { apex, axis, half_angle_rad }) if (apex[0] - (chamfer - 5.0)).abs() < 1e-6
            && axis[0] > 0.999999 && (half_angle_rad - std::f64::consts::FRAC_PI_4).abs() < 1e-6)));
        assert!(
            reply.faces.iter().any(|f| matches!(f.analytic_surface,
            Some(AnalyticSurface::Torus { center, major_radius, minor_radius, .. }) if (center[0] - (35.0 - radius)).abs() < 1e-6
            && (major_radius - (10.0 - radius)).abs() < 1e-6 && (minor_radius - radius).abs() < 1e-6)),
            "{reply:?}"
        );
        assert!(reply.faces.iter().any(|f| f.cylinder.as_ref().is_some_and(|c| (c.radius - 10.0).abs() < 1e-6)));
        assert!(reply.faces.iter().all(|f| f.analytic_surface.is_some() && f.analytic_tolerance_mm.is_some_and(|t| t > 0.0)));
        let edges = s.execute("model.edges", &json!({"body":"Probe"})).unwrap();
        let edge_count = edges["edges"].as_array().unwrap().len();
        let typed_edges: ModelEdgesResult = serde_json::from_value(edges).unwrap();
        assert!(typed_edges.edges.iter().any(|e| {
            e.circle.as_ref().is_some_and(|c| (c.radius - radius).abs() < 1e-6 && (c.angle_rad - std::f64::consts::FRAC_PI_2).abs() < 1e-6)
        }));
        assert!(reply.faces.iter().all(|f| !f.boundary_edges.is_empty() && f.boundary_edges.iter().all(|e| *e < edge_count)));
        let state = s.world_state().unwrap();
        let native = solidcraft_kernel::brep_record(&state.body("Probe").unwrap().body).unwrap();
        for (edge, record) in native.edges.iter().enumerate() {
            // A seam has two uses in one face; two distinct neighboring faces are not required.
            let expected: std::collections::BTreeSet<_> = record.uses.iter().map(|u| u.face as usize).collect();
            let observed = reply.faces.iter().filter(|f| f.boundary_edges.contains(&edge)).map(|f| f.index).collect();
            assert_eq!(expected, observed);
            assert_eq!(record.uses.len(), 2);
        }
    }
    let d = s.execute("engine.describe", &json!({"command":"model.faces"})).unwrap();
    assert!(d["input"]["properties"].get("body").is_some());
    assert!(d["output"]["properties"].get("faces").is_some());
}
