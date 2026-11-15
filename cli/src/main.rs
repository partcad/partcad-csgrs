//! `csgrs-partcad`: the command line tool PartCAD's csgrs plugin runs.
//!
//! ```text
//! csgrs-partcad --version
//! csgrs-partcad describe                       the part types and their parameters, as JSON
//! csgrs-partcad build <request.json> <out>     build one part; the report goes to stdout as JSON
//! csgrs-partcad profile <request.json> <out>   build one sketch's profile, as JSON (see `profile.rs`)
//! ```
//!
//! A request is
//!
//! ```json
//! {"type": "gear", "parameters": {"teeth": 24}, "path": null, "tree": null,
//!  "base": "/the/part's/package", "policy": "auto"}
//! ```
//!
//! where `path` is the file a file-backed type reads, `tree` the CSG tree of a
//! `csg` part, `base` the directory a relative file a parameter names (a
//! font) is found in, and `policy` how csgrs decides its predicates: `strict`
//! (certified or a failure), `approximate` (csgrs's 512-bit terminal), or
//! `auto` -- strict, and approximate only where strict cannot decide, with a
//! warning saying so.
//!
//! The mesh is written to `<out>` (see `mesh.rs`); the report says how many
//! vertices, triangles and bodies it holds, its bounding box, whether csgrs
//! certified it, and any warnings. A failure is exit status 1 with the reason
//! on stderr; a misused command line, 2.

mod csg;
mod fail;
mod geometry;
mod mesh;
mod params;
mod profile;

use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use csgrs::{Csg, GeometryCertainty, TriangleMesh};
use serde_json::{Map, Value, json};

use crate::fail::Fail;

/// The csgrs commit this tool is built from: the `rev` in Cargo.toml.
pub const CSGRS_REV: &str = "1671894b4a896b091416503910e3720ecf233c82";

const USAGE: &str = "usage: csgrs-partcad --version | describe | build <request.json> <out.mesh> | profile <request.json> <out.json>";

fn build_once(
    m: &Csg,
    request: &Map<String, Value>,
    warnings: &mut Vec<String>,
) -> Result<TriangleMesh, Fail> {
    let kind = request
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| Fail::input("the request names no 'type'"))?;
    let spec = params::find(kind).ok_or_else(|| {
        let known: Vec<&str> = params::part_types().iter().map(|t| t.name).collect();
        Fail::input(format!(
            "no part type '{kind}'; there are {}",
            known.join(", ")
        ))
    })?;
    let given = match request.get("parameters") {
        None | Some(Value::Null) => None,
        Some(Value::Object(map)) => Some(map),
        Some(other) => {
            return Err(Fail::input(format!(
                "'parameters' is {other}, not a mapping"
            )));
        }
    };
    let args = params::Args::new(kind, &spec.params, given)?;
    let base = request
        .get("base")
        .and_then(Value::as_str)
        .map(PathBuf::from);
    let file = || -> Result<PathBuf, Fail> {
        request
            .get("path")
            .and_then(Value::as_str)
            .map(PathBuf::from)
            .ok_or_else(|| {
                Fail::input(format!(
                    "a '{kind}' part reads a file, and the request names none ('path')"
                ))
            })
    };
    match kind {
        "gear" => geometry::gear(m, &args),
        "rack" => geometry::rack(m, &args),
        "tpms" => geometry::tpms(m, &args, warnings),
        "airfoil" => geometry::airfoil(m, &args),
        "text" => geometry::text(m, &args, geometry::Glyphs::Hershey),
        "truetype" => geometry::text(m, &args, geometry::Glyphs::File(&file()?)),
        "image" => geometry::image(m, &args, &file()?),
        "vrml" => geometry::vrml(m, &args, &file()?, warnings),
        "csg" => {
            let tree = request.get("tree").ok_or_else(|| {
                Fail::input(
                    "a 'csg' part is built from a tree, and the request carries none ('tree')",
                )
            })?;
            let mut tree_ = csg::Tree {
                m,
                base: base.as_deref(),
                warnings: Vec::new(),
            };
            let built = tree_.solid(tree);
            warnings.append(&mut tree_.warnings);
            built
        }
        _ => unreachable!("params::find() knows only the types above"),
    }
}

fn profile_once(
    m: &Csg,
    request: &Map<String, Value>,
    _warnings: &mut Vec<String>,
) -> Result<hypercurve::CurveRegion2, Fail> {
    let kind = request
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| Fail::input("the request names no 'type'"))?;
    let spec = params::find_sketch(kind).ok_or_else(|| {
        let known: Vec<&str> = params::sketch_types().iter().map(|t| t.name).collect();
        Fail::input(format!(
            "no sketch type '{kind}'; there are {}",
            known.join(", ")
        ))
    })?;
    let given = match request.get("parameters") {
        None | Some(Value::Null) => None,
        Some(Value::Object(map)) => Some(map),
        Some(other) => {
            return Err(Fail::input(format!(
                "'parameters' is {other}, not a mapping"
            )));
        }
    };
    let args = params::Args::new(kind, &spec.params, given)?;
    let file = || -> Result<PathBuf, Fail> {
        request
            .get("path")
            .and_then(Value::as_str)
            .map(PathBuf::from)
            .ok_or_else(|| {
                Fail::input(format!(
                    "a '{kind}' sketch reads a file, and the request names none ('path')"
                ))
            })
    };
    match kind {
        "gear_profile" => geometry::gear_region(m, &args),
        "rack_profile" => geometry::rack_region(m, &args),
        "airfoil_profile" => geometry::airfoil_region(m, &args),
        "text_profile" => geometry::text_region(m, &args, geometry::Glyphs::Hershey),
        "truetype_profile" => geometry::text_region(m, &args, geometry::Glyphs::File(&file()?)),
        "image_profile" => geometry::image_region(&args, &file()?),
        "csg_profile" => {
            let tree = request.get("tree").ok_or_else(|| {
                Fail::input("a 'csg_profile' sketch is built from a tree, and the request carries none ('tree')")
            })?;
            let base = request
                .get("base")
                .and_then(Value::as_str)
                .map(PathBuf::from);
            let mut tree_ = csg::Tree {
                m,
                base: base.as_deref(),
                warnings: Vec::new(),
            };
            tree_.profile(tree)
        }
        _ => unreachable!("params::find_sketch() knows only the types above"),
    }
}

/// Runs `once` under the policy the request asks for -- `strict`,
/// `approximate`, or `auto` (strict, and approximate only where strict cannot
/// decide) -- and says whether csgrs certified what it made.
fn under_policy<T>(
    request: &Map<String, Value>,
    warnings: &mut Vec<String>,
    once: impl Fn(&Csg, &Map<String, Value>, &mut Vec<String>) -> Result<T, Fail>,
) -> Result<(T, &'static str), Fail> {
    let policy = request
        .get("policy")
        .and_then(Value::as_str)
        .unwrap_or("auto");
    let certainty = |m: &Csg| match m.certainty() {
        GeometryCertainty::Certified => "certified",
        _ => "approximate",
    };
    match policy {
        "strict" => {
            let m = Csg::strict();
            let built = once(&m, request, warnings)?;
            Ok((built, certainty(&m)))
        }
        "approximate" => {
            let m = Csg::approximate_512();
            let built = once(&m, request, warnings)?;
            Ok((built, certainty(&m)))
        }
        "auto" => {
            let m = Csg::strict();
            match once(&m, request, warnings) {
                Ok(built) => Ok((built, certainty(&m))),
                Err(Fail::Undecided(why)) => {
                    warnings.clear();
                    warnings.push(format!(
                        "csgrs could not certify a step exactly ({why}); built with its approximate 512-bit policy instead"
                    ));
                    let m = Csg::approximate_512();
                    let built = once(&m, request, warnings)?;
                    Ok((built, "approximate"))
                }
                Err(e) => Err(e),
            }
        }
        other => Err(Fail::input(format!(
            "'policy' is '{other}'; it is one of auto, strict, approximate"
        ))),
    }
}

fn read_request(path: &Path) -> Result<Map<String, Value>, Fail> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| Fail::input(format!("{}: {e}", path.display())))?;
    let request: Value = serde_json::from_str(&text)
        .map_err(|e| Fail::input(format!("{}: not JSON: {e}", path.display())))?;
    match request {
        Value::Object(request) => Ok(request),
        _ => Err(Fail::input("the request is not a JSON object")),
    }
}

fn run_build(request_path: &Path, out_path: &Path) -> Result<Value, Fail> {
    let request = read_request(request_path)?;
    let mut warnings = Vec::new();
    let (built, certainty) = under_policy(&request, &mut warnings, build_once)?;
    let mesh = mesh::Mesh::from_csgrs(&built)?;
    if mesh.triangles.is_empty() {
        return Err(Fail::input("the part is empty: csgrs built no triangles"));
    }
    let file = std::fs::File::create(out_path)
        .map_err(|e| Fail::input(format!("{}: {e}", out_path.display())))?;
    let mut out = BufWriter::new(file);
    mesh.write(&mut out)
        .and_then(|()| out.flush())
        .map_err(|e| Fail::input(format!("{}: {e}", out_path.display())))?;
    let mut report = mesh.report();
    report["certainty"] = json!(certainty);
    report["warnings"] = json!(warnings);
    Ok(report)
}

/// The bounds of every point a profile's JSON names.
fn json_bounds(value: &Value, bounds: &mut [f64; 4]) {
    match value {
        Value::Array(items) => {
            if let [Value::Number(x), Value::Number(y)] = items.as_slice()
                && let (Some(x), Some(y)) = (x.as_f64(), y.as_f64())
            {
                bounds[0] = bounds[0].min(x);
                bounds[1] = bounds[1].min(y);
                bounds[2] = bounds[2].max(x);
                bounds[3] = bounds[3].max(y);
                return;
            }
            items.iter().for_each(|item| json_bounds(item, bounds));
        }
        Value::Object(map) => map.values().for_each(|item| json_bounds(item, bounds)),
        _ => {}
    }
}

fn run_profile(request_path: &Path, out_path: &Path) -> Result<Value, Fail> {
    let request = read_request(request_path)?;
    let mut warnings = Vec::new();
    let (region, certainty) = under_policy(&request, &mut warnings, profile_once)?;
    let context = if certainty == "certified" {
        csgrs::GeometryContext::STRICT
    } else {
        csgrs::GeometryContext::APPROXIMATE_512
    };
    let faces = profile::region_json(&region, context)?;
    let count = faces["faces"].as_array().map_or(0, Vec::len);
    if count == 0 {
        return Err(Fail::input("the sketch is empty: csgrs built no region"));
    }
    // The bounds of what was written, rather than csgrs's exact bounding box,
    // which a profile of many sampled curves can leave undecided.
    let mut bounds = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    json_bounds(&faces, &mut bounds);
    std::fs::write(out_path, faces.to_string())
        .map_err(|e| Fail::input(format!("{}: {e}", out_path.display())))?;
    Ok(json!({
        "faces": count,
        "bbox": bounds,
        "certainty": certainty,
        "warnings": warnings,
    }))
}

/// Prints what `run` reports, or says why it could not.
fn answer(result: Result<Value, Fail>) -> ExitCode {
    match result {
        Ok(report) => {
            println!("{report}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::from(1)
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["--version"] | ["-V"] => {
            println!(
                "{} {} (csgrs {})",
                env!("CARGO_PKG_NAME"),
                env!("CARGO_PKG_VERSION"),
                &CSGRS_REV[..12]
            );
            ExitCode::SUCCESS
        }
        ["describe"] => {
            println!(
                "{}",
                serde_json::to_string_pretty(&params::describe()).expect("JSON")
            );
            ExitCode::SUCCESS
        }
        ["build", request, out] => answer(run_build(Path::new(request), Path::new(out))),
        ["profile", request, out] => answer(run_profile(Path::new(request), Path::new(out))),
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}
