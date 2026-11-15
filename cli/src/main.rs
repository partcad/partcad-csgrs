//! `csgrs-partcad`: the command line tool PartCAD's csgrs plugin runs.
//!
//! ```text
//! csgrs-partcad --version
//! csgrs-partcad describe                       the part types and their parameters, as JSON
//! csgrs-partcad build <request.json> <out>     build one part; the report goes to stdout as JSON
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

use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use csgrs::{Csg, GeometryCertainty, TriangleMesh};
use serde_json::{Map, Value, json};

use crate::fail::Fail;

/// The csgrs commit this tool is built from: the `rev` in Cargo.toml.
pub const CSGRS_REV: &str = "1671894b4a896b091416503910e3720ecf233c82";

const USAGE: &str = "usage: csgrs-partcad --version | describe | build <request.json> <out.mesh>";

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

/// The part, under the policy the request asks for, and whether csgrs
/// certified it.
fn build(
    request: &Map<String, Value>,
    warnings: &mut Vec<String>,
) -> Result<(TriangleMesh, &'static str), Fail> {
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
            let mesh = build_once(&m, request, warnings)?;
            Ok((mesh, certainty(&m)))
        }
        "approximate" => {
            let m = Csg::approximate_512();
            let mesh = build_once(&m, request, warnings)?;
            Ok((mesh, certainty(&m)))
        }
        "auto" => {
            let m = Csg::strict();
            match build_once(&m, request, warnings) {
                Ok(mesh) => Ok((mesh, certainty(&m))),
                Err(Fail::Undecided(why)) => {
                    warnings.clear();
                    warnings.push(format!(
                        "csgrs could not certify a step exactly ({why}); built with its approximate 512-bit policy instead"
                    ));
                    let m = Csg::approximate_512();
                    let mesh = build_once(&m, request, warnings)?;
                    Ok((mesh, "approximate"))
                }
                Err(e) => Err(e),
            }
        }
        other => Err(Fail::input(format!(
            "'policy' is '{other}'; it is one of auto, strict, approximate"
        ))),
    }
}

fn run_build(request_path: &Path, out_path: &Path) -> Result<Value, Fail> {
    let text = std::fs::read_to_string(request_path)
        .map_err(|e| Fail::input(format!("{}: {e}", request_path.display())))?;
    let request: Value = serde_json::from_str(&text)
        .map_err(|e| Fail::input(format!("{}: not JSON: {e}", request_path.display())))?;
    let Value::Object(request) = request else {
        return Err(Fail::input("the request is not a JSON object"));
    };
    let mut warnings = Vec::new();
    let (built, certainty) = build(&request, &mut warnings)?;
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
        ["build", request, out] => match run_build(Path::new(request), Path::new(out)) {
            Ok(report) => {
                println!("{report}");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("{e}");
                ExitCode::from(1)
            }
        },
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}
