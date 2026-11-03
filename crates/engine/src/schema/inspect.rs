//! Contracts and result types for the commands in `cmd/inspect.rs`.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Which command to describe.
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct DescribeParams {
    /// The command's id, such as `solid.extrude`.
    #[serde(deserialize_with = "command_id")]
    pub command: String,
}

/// A command id, with the `engine.describe` handler's message for anything else.
fn command_id<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    match Value::deserialize(d)? {
        Value::String(s) => Ok(s),
        _ => Err(serde::de::Error::custom("must be a command id")),
    }
}

/// What `engine.describe` returns.
#[derive(Debug, Serialize, JsonSchema)]
pub struct DescribeResult {
    pub id: String,
    pub label: String,
    /// The command's parameter documentation, as prose.
    pub params: String,
    /// The parameters' JSON Schema, or `null` when the command has no contract yet.
    pub input: Option<Value>,
    /// The successful result's JSON Schema, or `null` when not described yet.
    pub output: Option<Value>,
    /// In the warn parameter-check mode: what the call's parameters got wrong.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub param_warnings: Vec<String>,
}

/// List a body's face names, approximate measurements and recognized analytic support surfaces.
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct ModelFacesParams {
    /// The body name in the current world state.
    #[serde(deserialize_with = "body_name")]
    pub body: String,
}

fn body_name<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    match Value::deserialize(d)? {
        Value::String(s) => Ok(s),
        _ => Err(serde::de::Error::custom(super::check::whole("`body` is required"))),
    }
}

/// A support surface recognized by the existing kernel, expressed in world coordinates.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AnalyticSurface {
    Plane {
        normal: [f64; 3],
        point: [f64; 3],
    },
    Cylinder {
        origin: [f64; 3],
        axis: [f64; 3],
        radius: f64,
    },
    /// Axis points from the apex into the cone; half-angle is in radians.
    Cone {
        apex: [f64; 3],
        axis: [f64; 3],
        half_angle_rad: f64,
    },
    Sphere {
        center: [f64; 3],
        radius: f64,
    },
    /// The signed major radius also preserves the inner sheet of a spindle torus.
    Torus {
        center: [f64; 3],
        axis: [f64; 3],
        major_radius: f64,
        minor_radius: f64,
    },
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct CylinderSurface {
    pub origin: [f64; 3],
    pub axis: [f64; 3],
    pub radius: f64,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ModelFaceResult {
    pub index: usize,
    pub name: Option<String>,
    /// Whether the history-derived face name is exact; not an accuracy claim for measurements.
    pub exact: bool,
    /// Approximate triangulated area, in square millimetres.
    pub area: f64,
    /// Approximate area centroid; need not lie on the trimmed face.
    pub centroid: [f64; 3],
    pub plane_normal: Option<[f64; 3]>,
    pub point: Option<[f64; 3]>,
    /// Boundary edge indices in model.edges, including seams, from native topology adjacency.
    pub boundary_edges: Vec<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cylinder: Option<CylinderSurface>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub analytic_surface: Option<AnalyticSurface>,
    /// Kernel surface recognition tolerance in mm; not a design or manufacturing tolerance.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub analytic_tolerance_mm: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sheet_face: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ModelFacesResult {
    pub body: String,
    pub faces: Vec<ModelFaceResult>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub param_warnings: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct EdgeCircleResult {
    pub center: [f64; 3],
    pub axis: [f64; 3],
    pub radius: f64,
    pub start: [f64; 3],
    pub end: [f64; 3],
    /// Edge runs counter-clockwise about axis; radians, 2π for a whole circle.
    pub angle_rad: f64,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ModelEdgeResult {
    pub index: usize,
    pub name: Option<String>,
    /// Approximate polyline midpoint, in mm.
    pub mid: [f64; 3],
    /// Approximate polyline length, in mm.
    pub length: f64,
    pub start: Option<[f64; 3]>,
    pub end: Option<[f64; 3]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub circle: Option<EdgeCircleResult>,
    /// Existing kernel circle-recognition tolerance, in mm; not a manufacturing tolerance.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub curve_tolerance_mm: Option<f64>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ModelEdgesResult {
    pub body: String,
    pub edges: Vec<ModelEdgeResult>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub param_warnings: Vec<String>,
}

#[cfg(test)]
#[path = "inspect_tests.rs"]
mod tests;
