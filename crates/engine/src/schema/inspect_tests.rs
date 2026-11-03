use serde_json::json;

use super::{AnalyticSurface, DescribeParams, ModelEdgesResult, ModelFacesParams, ModelFacesResult};
use crate::Session;
use crate::schema::input_schema_for;
use crate::schema::tests::{assert_keys, assert_strict_keeps_todays_errors};

#[test]
fn schemas_and_the_key_check_agree() {
    assert_keys(input_schema_for::<DescribeParams>(), &["command"]);
    assert_keys(input_schema_for::<ModelFacesParams>(), &["body"]);
}

#[test]
fn strict_keeps_todays_errors() {
    assert_strict_keeps_todays_errors(&[
        ("engine.describe", json!({"command": 5})),
        ("engine.describe", json!({"command": null})),
        ("engine.describe", json!({"command": "solid.extrud"})),
        ("model.faces", json!({"body": 5})),
        ("model.faces", json!({})),
        ("model.faces", json!({"body": null})),
        ("model.faces", json!({"body": "missing"})),
        ("model.edges", json!({"body": 5})),
        ("model.edges", json!({})),
        ("model.edges", json!({"body": null})),
        ("model.edges", json!({"body": "missing"})),
    ]);
}

#[test]
fn face_support_surfaces_survive_edge_treatments_and_parameter_changes() {
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
        assert!(reply.faces.iter().any(|f| matches!(f.analytic_surface,
            Some(AnalyticSurface::Torus { center, major_radius, minor_radius, .. }) if (center[0] - (35.0 - radius)).abs() < 1e-6
            && (major_radius - (10.0 - radius)).abs() < 1e-6 && (minor_radius - radius).abs() < 1e-6)));
        assert!(reply.faces.iter().any(|f| f.cylinder.as_ref().is_some_and(|c| (c.radius - 10.0).abs() < 1e-6)));
        assert!(reply.faces.iter().all(|f| f.analytic_surface.is_some() && f.analytic_tolerance_mm.is_some_and(|t| t > 0.0)));
        let edges = s.execute("model.edges", &json!({"body":"Probe"})).unwrap();
        let edge_count = edges["edges"].as_array().unwrap().len();
        let typed_edges: ModelEdgesResult = serde_json::from_value(edges).unwrap();
        assert!(typed_edges.edges.iter().any(|e| {
            e.circle.as_ref().is_some_and(|c| (c.radius - radius).abs() < 1e-6 && (c.angle_rad - std::f64::consts::FRAC_PI_2).abs() < 1e-6)
        }));
        assert!(reply.faces.iter().all(|f| !f.boundary_edges.is_empty() && f.boundary_edges.iter().all(|e| *e < edge_count)));
        for edge in 0..edge_count {
            assert_eq!(reply.faces.iter().filter(|f| f.boundary_edges.contains(&edge)).count(), 2);
        }
    }
    let d = s.execute("engine.describe", &json!({"command":"model.faces"})).unwrap();
    assert!(d["input"]["properties"].get("body").is_some());
    assert!(d["output"]["properties"].get("faces").is_some());
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
