//! A csgrs profile as what PartCAD builds a sketch from: its faces, each an
//! outer boundary and its holes, every boundary a loop of the curves csgrs
//! kept -- lines, circular arcs, quadratic and cubic Béziers -- in binary64.
//!
//! ```json
//! {"faces": [{"outer": [segment, ...], "holes": [[segment, ...], ...]}, ...]}
//! ```
//!
//! where a segment is one of
//!
//! ```json
//! {"line": [[x, y], [x, y]]}
//! {"arc": {"start": [x, y], "end": [x, y], "center": [x, y], "clockwise": false}}
//! {"bezier": [[x, y], [x, y], [x, y]]}            (quadratic: three points; cubic: four)
//! {"polyline": [[x, y], ...]}                     (a curve kept in any other form, sampled)
//! ```
//!
//! Every boundary has material on its left, so an outer boundary runs
//! counter-clockwise and a hole clockwise, and each segment starts where the
//! one before it ended.

use csgrs::Real;
use hypercurve::{CurveGeometry2, CurvePath2, CurveRegion2, FiniteProjectionOptions, Point2};
use serde_json::{Value, json};

use crate::fail::{Fail, other};

/// How far a sampled curve may stray from the curve it samples, in mm: a
/// curve in a form PartCAD has no exact counterpart for (a rational Bézier, a
/// B-spline) is the only thing sampled.
const CHORD_ERROR: f64 = 0.001;

fn coordinate(value: &Real) -> Result<f64, Fail> {
    value
        .to_f64_lossy()
        .filter(|f| f.is_finite())
        .map(|f| f + 0.0)
        .ok_or_else(|| Fail::input("a profile's point is not a finite number"))
}

fn point(p: &Point2) -> Result<Value, Fail> {
    Ok(json!([coordinate(p.x())?, coordinate(p.y())?]))
}

fn sampled(curve: &hypercurve::Curve2) -> Result<Value, Fail> {
    let path = CurvePath2::try_new(vec![curve.clone()]).map_err(other)?;
    let options = FiniteProjectionOptions::try_new(CHORD_ERROR).map_err(other)?;
    let polyline = path.project_to_finite_polyline(&options).map_err(other)?;
    let points: Vec<Value> = polyline
        .points()
        .iter()
        .map(|p| json!([p[0] + 0.0, p[1] + 0.0]))
        .collect();
    Ok(json!({ "polyline": points }))
}

fn path_json(path: &CurvePath2) -> Result<Value, Fail> {
    let mut segments = Vec::with_capacity(path.curves().len());
    for curve in path.curves() {
        segments.push(match curve.geometry() {
            Some(CurveGeometry2::Line(line)) => {
                json!({"line": [point(line.start())?, point(line.end())?]})
            }
            Some(CurveGeometry2::CircularArc(arc)) => json!({"arc": {
                "start": point(arc.start())?,
                "end": point(arc.end())?,
                "center": point(arc.center())?,
                "clockwise": arc.is_clockwise(),
            }}),
            Some(CurveGeometry2::QuadraticBezier(b)) => {
                json!({"bezier": [point(b.start())?, point(b.control())?, point(b.end())?]})
            }
            Some(CurveGeometry2::CubicBezier(b)) => json!({"bezier": [
                point(b.start())?,
                point(b.control1())?,
                point(b.control2())?,
                point(b.end())?,
            ]}),
            _ => sampled(curve)?,
        });
    }
    Ok(Value::Array(segments))
}

/// The faces of `region`: one per piece of material, with its holes.
///
/// csgrs groups the boundaries exactly, keeping its curves, where it can
/// decide every comparison that takes (`material_components`). Where it
/// cannot -- a profile of many sampled curves can leave csgrs's strict policy
/// undecided about which loop lies inside which -- the region is taken as
/// csgrs projects it for extrusion and interchange (`try_finite_profiles`):
/// the same faces and holes, every boundary a polyline within csgrs's
/// sampled-profile chord error. For a profile csgrs samples anyway -- a
/// gear's flanks, a circle -- that loses nothing.
pub fn region_json(region: &CurveRegion2, context: csgrs::GeometryContext) -> Result<Value, Fail> {
    match exactly_grouped(region) {
        Ok(faces) => Ok(json!({ "faces": faces })),
        Err(_) => Ok(json!({ "faces": projected(region, context)? })),
    }
}

fn exactly_grouped(region: &CurveRegion2) -> Result<Vec<Value>, Fail> {
    let mut faces = Vec::new();
    for component in region.material_components().map_err(other)? {
        let paths = component.boundary_paths().map_err(other)?;
        let Some((outer, holes)) = paths.split_first() else {
            continue;
        };
        let holes: Vec<Value> = holes.iter().map(path_json).collect::<Result<_, _>>()?;
        faces.push(json!({"outer": path_json(outer)?, "holes": holes}));
    }
    Ok(faces)
}

/// A closed ring as the one polyline segment its boundary is.
fn ring_json(points: &[[f64; 2]]) -> Value {
    let mut ring: Vec<Value> = points
        .iter()
        .map(|p| json!([p[0] + 0.0, p[1] + 0.0]))
        .collect();
    if points.first() != points.last()
        && let Some(first) = ring.first().cloned()
    {
        ring.push(first);
    }
    json!([{ "polyline": ring }])
}

fn projected(region: &CurveRegion2, context: csgrs::GeometryContext) -> Result<Vec<Value>, Fail> {
    let profiles = csgrs::curve::try_finite_profiles(region, context)?.into_value();
    Ok(profiles
        .iter()
        .map(|profile| {
            let holes: Vec<Value> = profile
                .holes()
                .iter()
                .map(|h| ring_json(h.points()))
                .collect();
            json!({"outer": ring_json(profile.material().points()), "holes": holes})
        })
        .collect())
}
