//! The `csg` part type: a constructive solid geometry tree, written as data.
//!
//! A node is a mapping with exactly one *shape* key, which says what the node
//! is, and any of the *placement* keys, which move it:
//!
//! ```yaml
//! difference:
//!   - cuboid: [40, 20, 5]
//!   - cylinder: {r: 3, h: 7}
//!     translate: [20, 10, -1]
//! ```
//!
//! Placement is applied in a fixed order whatever order the keys are written
//! in: `scale`, then `rotate`, then `translate` -- the order a part is placed
//! in everywhere else. A node is either a solid or a profile (a filled region
//! of the XY plane); profiles become solids through `extrude`, `revolve` and
//! `sweep`, and a boolean's operands are all one or all the other.
//!
//! By the time a tree reaches this file every value is a plain number: the
//! expressions over a part's parameters a `.csgrs` file may hold are worked
//! out by the script that reads it (`csgrs_part.py`).

use csgrs::{Csg, TriangleMesh, curve, solid};
use hypercurve::{BooleanOp, CurveRegion2, OffsetCornerStyle2};
use hyperlattice::Point3;
use serde_json::{Map, Value};

use crate::fail::{Fail, other};
use crate::geometry::{self, r};
use crate::params::number;

pub enum Geom {
    Solid(TriangleMesh),
    Profile(CurveRegion2),
}

const PLACEMENT: &[&str] = &["translate", "rotate", "scale", "center"];

const SHAPES: &[&str] = &[
    // solids
    "cube",
    "cuboid",
    "sphere",
    "cylinder",
    "cone",
    "torus",
    "polyhedron",
    "extrude",
    "revolve",
    "sweep",
    "hull",
    "minkowski",
    "gear",
    "rack",
    "tpms",
    "airfoil",
    "text",
    "truetype",
    // profiles
    "square",
    "rectangle",
    "circle",
    "ellipse",
    "ngon",
    "polygon",
    "star",
    "rounded_rectangle",
    "offset",
    "gear_profile",
    "rack_profile",
    "airfoil_profile",
    "text_profile",
    "truetype_profile",
    // either
    "union",
    "difference",
    "intersection",
    "xor",
];

pub struct Tree<'a> {
    pub m: &'a Csg,
    pub base: Option<&'a std::path::Path>,
    pub warnings: Vec<String>,
}

fn where_(path: &str) -> String {
    if path.is_empty() {
        "the tree".to_string()
    } else {
        path.to_string()
    }
}

fn f(path: &str, name: &str, v: &Value) -> Result<f64, Fail> {
    number(&where_(path), name, v)
}

fn count(path: &str, name: &str, v: &Value) -> Result<usize, Fail> {
    let n = f(path, name, v)?;
    if n < 0.0 || n.fract() != 0.0 {
        return Err(Fail::input(format!(
            "{}: '{name}' is {n}, not a whole number",
            where_(path)
        )));
    }
    Ok(n as usize)
}

fn numbers(path: &str, name: &str, v: &Value, len: usize) -> Result<Vec<f64>, Fail> {
    match v {
        Value::Array(items) if items.len() == len => {
            items.iter().map(|i| f(path, name, i)).collect()
        }
        Value::Number(_) | Value::String(_) => Ok(vec![f(path, name, v)?; len]),
        _ => Err(Fail::input(format!(
            "{}: '{name}' is {v}; it has to be {len} numbers",
            where_(path)
        ))),
    }
}

/// The value of `key` in a mapping of keyword arguments, or `default`.
fn kw<'v>(
    path: &str,
    args: &'v Map<String, Value>,
    key: &str,
    default: Option<&'v Value>,
) -> Result<&'v Value, Fail> {
    args.get(key)
        .or(default)
        .ok_or_else(|| Fail::input(format!("{}: '{key}' is missing", where_(path))))
}

fn mapping<'v>(path: &str, shape: &str, v: &'v Value) -> Result<&'v Map<String, Value>, Fail> {
    v.as_object().ok_or_else(|| {
        Fail::input(format!(
            "{}: '{shape}' takes a mapping of its parameters, not {v}",
            where_(path)
        ))
    })
}

fn only_keys(
    path: &str,
    shape: &str,
    args: &Map<String, Value>,
    allowed: &[&str],
) -> Result<(), Fail> {
    for key in args.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(Fail::input(format!(
                "{}: '{shape}' takes no '{key}'; it takes {}",
                where_(path),
                allowed.join(", ")
            )));
        }
    }
    Ok(())
}

impl Tree<'_> {
    pub fn solid(&mut self, node: &Value) -> Result<TriangleMesh, Fail> {
        match self.node(node, "")? {
            Geom::Solid(mesh) => Ok(mesh),
            Geom::Profile(region) => {
                // A profile on its own is a sketch, not a part: say how to make
                // it one rather than guessing a thickness.
                let _ = region;
                Err(Fail::input(
                    "the tree is a profile, not a solid; put it under 'extrude' or 'revolve'",
                ))
            }
        }
    }

    fn node(&mut self, node: &Value, path: &str) -> Result<Geom, Fail> {
        let Some(map) = node.as_object() else {
            return Err(Fail::input(format!(
                "{}: a node is a mapping, not {node}",
                where_(path)
            )));
        };
        let shapes: Vec<&String> = map
            .keys()
            .filter(|k| !PLACEMENT.contains(&k.as_str()))
            .collect();
        let shape = match shapes.as_slice() {
            [one] => one.as_str(),
            [] => {
                return Err(Fail::input(format!(
                    "{}: the node says what it is with one of {}",
                    where_(path),
                    SHAPES.join(", ")
                )));
            }
            many => {
                return Err(Fail::input(format!(
                    "{}: a node is one shape, and this one says {}",
                    where_(path),
                    many.iter()
                        .map(|s| format!("'{s}'"))
                        .collect::<Vec<_>>()
                        .join(" and ")
                )));
            }
        };
        let here = if path.is_empty() {
            shape.to_string()
        } else {
            format!("{path} > {shape}")
        };
        // A failure csgrs reports is said about the node it happened in; one
        // of this file's own already names it.
        let geom = self.shape(shape, &map[shape], &here).map_err(|e| {
            if e.to_string().starts_with(&here) || e.to_string().contains(" > ") {
                e
            } else {
                e.context(&here)
            }
        })?;
        self.place(geom, map, &here)
    }

    fn children(&mut self, v: &Value, path: &str) -> Result<Vec<Geom>, Fail> {
        let Some(items) = v.as_array() else {
            return Err(Fail::input(format!(
                "{path}: takes a list of nodes, not {v}"
            )));
        };
        if items.is_empty() {
            return Err(Fail::input(format!("{path}: the list of nodes is empty")));
        }
        items
            .iter()
            .enumerate()
            .map(|(i, item)| self.node(item, &format!("{path}[{i}]")))
            .collect()
    }

    fn of_profile(&mut self, args: &Map<String, Value>, path: &str) -> Result<CurveRegion2, Fail> {
        let profile = kw(path, args, "profile", None)?;
        match self.node(profile, &format!("{path} > profile"))? {
            Geom::Profile(region) => Ok(region),
            Geom::Solid(_) => Err(Fail::input(format!(
                "{path}: 'profile' is a solid; it has to be a profile"
            ))),
        }
    }

    fn part_type(
        &mut self,
        name: &str,
        v: &Value,
        path: &str,
    ) -> Result<crate::params::Args, Fail> {
        let given = mapping(path, name, v)?;
        geometry::args_for(name, Some(given)).map_err(|e| e.context(path))
    }

    /// A 'truetype' node's parameters, and its 'font': the font file, relative
    /// to the tree's own file.
    fn font_file(
        &mut self,
        v: &Value,
        path: &str,
    ) -> Result<(crate::params::Args, std::path::PathBuf), Fail> {
        let mut given = mapping(path, "truetype", v)?.clone();
        let font = given
            .remove("font")
            .and_then(|f| f.as_str().map(str::to_string))
            .ok_or_else(|| {
                Fail::input(format!(
                    "{path}: 'font' names the font file, and it is missing"
                ))
            })?;
        let file = match self.base {
            Some(base) if std::path::Path::new(&font).is_relative() => base.join(&font),
            _ => std::path::PathBuf::from(&font),
        };
        let a = geometry::args_for("truetype", Some(&given)).map_err(|e| e.context(path))?;
        Ok((a, file))
    }

    fn shape(&mut self, shape: &str, v: &Value, path: &str) -> Result<Geom, Fail> {
        let m = self.m;
        let solid_of = |mesh: TriangleMesh| Ok(Geom::Solid(mesh));
        let profile_of = |region: CurveRegion2| Ok(Geom::Profile(region));
        match shape {
            "cube" => solid_of(m.cube(r(f(path, "cube", v)?)?)?),
            "cuboid" => {
                let s = numbers(path, "cuboid", v, 3)?;
                solid_of(m.cuboid(r(s[0])?, r(s[1])?, r(s[2])?)?)
            }
            "sphere" => {
                let (radius, segments, stacks) = match v {
                    Value::Object(a) => {
                        only_keys(path, shape, a, &["r", "segments", "stacks"])?;
                        (
                            f(path, "r", kw(path, a, "r", None)?)?,
                            count(
                                path,
                                "segments",
                                kw(path, a, "segments", Some(&Value::from(32)))?,
                            )?,
                            count(
                                path,
                                "stacks",
                                kw(path, a, "stacks", Some(&Value::from(16)))?,
                            )?,
                        )
                    }
                    _ => (f(path, "sphere", v)?, 32, 16),
                };
                solid_of(m.sphere(r(radius)?, segments, stacks)?)
            }
            "cylinder" | "cone" => {
                let a = mapping(path, shape, v)?;
                only_keys(path, shape, a, &["r", "r1", "r2", "h", "segments"])?;
                let h = f(path, "h", kw(path, a, "h", None)?)?;
                let segments = count(
                    path,
                    "segments",
                    kw(path, a, "segments", Some(&Value::from(32)))?,
                )?;
                let (r1, r2) = match a.get("r") {
                    Some(rv) => (f(path, "r", rv)?, f(path, "r", rv)?),
                    None => (
                        f(path, "r1", kw(path, a, "r1", None)?)?,
                        f(path, "r2", kw(path, a, "r2", None)?)?,
                    ),
                };
                solid_of(solid::frustum(r(r1)?, r(r2)?, r(h)?, segments)?)
            }
            "torus" => {
                let a = mapping(path, shape, v)?;
                only_keys(path, shape, a, &["R", "r", "segments", "minor_segments"])?;
                solid_of(solid::torus(
                    r(f(path, "R", kw(path, a, "R", None)?)?)?,
                    r(f(path, "r", kw(path, a, "r", None)?)?)?,
                    count(
                        path,
                        "segments",
                        kw(path, a, "segments", Some(&Value::from(48)))?,
                    )?,
                    count(
                        path,
                        "minor_segments",
                        kw(path, a, "minor_segments", Some(&Value::from(16)))?,
                    )?,
                )?)
            }
            "polyhedron" => {
                let a = mapping(path, shape, v)?;
                only_keys(path, shape, a, &["points", "faces"])?;
                let points = kw(path, a, "points", None)?
                    .as_array()
                    .ok_or_else(|| Fail::input(format!("{path}: 'points' is a list of [x, y, z]")))?
                    .iter()
                    .map(|p| {
                        let c = numbers(path, "points", p, 3)?;
                        Ok([r(c[0])?, r(c[1])?, r(c[2])?])
                    })
                    .collect::<Result<Vec<_>, Fail>>()?;
                let faces = kw(path, a, "faces", None)?
                    .as_array()
                    .ok_or_else(|| {
                        Fail::input(format!(
                            "{path}: 'faces' is a list of lists of point indices"
                        ))
                    })?
                    .iter()
                    .map(|face| {
                        face.as_array()
                            .ok_or_else(|| {
                                Fail::input(format!("{path}: a face is a list of point indices"))
                            })?
                            .iter()
                            .map(|i| count(path, "faces", i))
                            .collect::<Result<Vec<usize>, Fail>>()
                    })
                    .collect::<Result<Vec<_>, Fail>>()?;
                let faces: Vec<&[usize]> = faces.iter().map(|f| f.as_slice()).collect();
                solid_of(m.run(|context| solid::polyhedron_with_context(&points, &faces, context))?)
            }
            "extrude" => {
                let a = mapping(path, shape, v)?;
                only_keys(
                    path,
                    shape,
                    a,
                    &["height", "twist", "scale", "slices", "profile"],
                )?;
                let region = self.of_profile(a, path)?;
                let height = r(f(path, "height", kw(path, a, "height", None)?)?)?;
                let twist = a
                    .get("twist")
                    .map(|t| f(path, "twist", t))
                    .transpose()?
                    .unwrap_or(0.0);
                let scale = a
                    .get("scale")
                    .map(|s| numbers(path, "scale", s, 2))
                    .transpose()?;
                if twist == 0.0 && scale.is_none() {
                    return solid_of(m.extrude(&region, height)?);
                }
                let scale = scale.unwrap_or(vec![1.0, 1.0]);
                let slices = count(
                    path,
                    "slices",
                    kw(path, a, "slices", Some(&Value::from(16)))?,
                )?;
                let (twist, end) = (r(twist)?, [r(scale[0])?, r(scale[1])?]);
                solid_of(m.run(|context| {
                    curve::extrude_twisted(&region, height, twist, end, slices, context)
                })?)
            }
            "revolve" => {
                let a = mapping(path, shape, v)?;
                only_keys(path, shape, a, &["angle", "segments", "profile"])?;
                let region = self.of_profile(a, path)?;
                let angle = r(f(
                    path,
                    "angle",
                    kw(path, a, "angle", Some(&Value::from(360)))?,
                )?)?;
                let segments = count(
                    path,
                    "segments",
                    kw(path, a, "segments", Some(&Value::from(48)))?,
                )?;
                solid_of(m.run(|context| curve::revolve_solid(&region, angle, segments, context))?)
            }
            "sweep" => {
                let a = mapping(path, shape, v)?;
                only_keys(path, shape, a, &["path", "profile"])?;
                let region = self.of_profile(a, path)?;
                let points = kw(path, a, "path", None)?
                    .as_array()
                    .ok_or_else(|| Fail::input(format!("{path}: 'path' is a list of [x, y, z]")))?
                    .iter()
                    .map(|p| {
                        let c = numbers(path, "path", p, 3)?;
                        Ok(Point3::new(r(c[0])?, r(c[1])?, r(c[2])?))
                    })
                    .collect::<Result<Vec<_>, Fail>>()?;
                solid_of(m.run(|context| curve::try_sweep(&region, &points, context))?)
            }
            "hull" => {
                let mut meshes = Vec::new();
                for child in self.children(v, path)? {
                    match child {
                        Geom::Solid(mesh) => meshes.push(mesh),
                        Geom::Profile(_) => {
                            return Err(Fail::input(format!("{path}: takes solids only")));
                        }
                    }
                }
                let merged = solid::merge(&meshes)?;
                solid_of(
                    m.run(|context| solid::convex_hull_with_context(&merged, context))
                        .map_err(other)?,
                )
            }
            "minkowski" => {
                let children = self.children(v, path)?;
                let [Geom::Solid(a), Geom::Solid(b)] = children.as_slice() else {
                    return Err(Fail::input(format!("{path}: takes exactly two solids")));
                };
                solid_of(
                    m.run(|context| solid::minkowski_sum_with_context(a, b, context))
                        .map_err(other)?,
                )
            }
            "gear" => {
                let a = self.part_type("gear", v, path)?;
                solid_of(geometry::gear(m, &a)?)
            }
            "rack" => {
                let a = self.part_type("rack", v, path)?;
                solid_of(geometry::rack(m, &a)?)
            }
            "tpms" => {
                let a = self.part_type("tpms", v, path)?;
                solid_of(geometry::tpms(m, &a, &mut self.warnings)?)
            }
            "airfoil" => {
                let a = self.part_type("airfoil", v, path)?;
                solid_of(geometry::airfoil(m, &a)?)
            }
            "text" => {
                let a = self.part_type("text", v, path)?;
                solid_of(geometry::text(m, &a, geometry::Glyphs::Hershey)?)
            }
            "truetype" => {
                let (a, file) = self.font_file(v, path)?;
                solid_of(geometry::text(m, &a, geometry::Glyphs::File(&file))?)
            }
            "gear_profile" => {
                let a = self.part_type("gear", v, path)?;
                profile_of(geometry::gear_region(m, &a)?)
            }
            "rack_profile" => {
                let a = self.part_type("rack", v, path)?;
                profile_of(geometry::rack_region(m, &a)?)
            }
            "airfoil_profile" => {
                let a = self.part_type("airfoil", v, path)?;
                profile_of(geometry::airfoil_region(m, &a)?)
            }
            "text_profile" => {
                let a = self.part_type("text", v, path)?;
                profile_of(geometry::text_region(m, &a, geometry::Glyphs::Hershey)?)
            }
            "truetype_profile" => {
                let (a, file) = self.font_file(v, path)?;
                profile_of(geometry::text_region(m, &a, geometry::Glyphs::File(&file))?)
            }
            "square" => profile_of(m.square(r(f(path, "square", v)?)?)?),
            "rectangle" => {
                let s = numbers(path, "rectangle", v, 2)?;
                profile_of(m.rectangle(r(s[0])?, r(s[1])?)?)
            }
            "circle" => {
                let (radius, segments) = match v {
                    Value::Object(a) => {
                        only_keys(path, shape, a, &["r", "segments"])?;
                        (
                            f(path, "r", kw(path, a, "r", None)?)?,
                            count(
                                path,
                                "segments",
                                kw(path, a, "segments", Some(&Value::from(64)))?,
                            )?,
                        )
                    }
                    _ => (f(path, "circle", v)?, 64),
                };
                profile_of(m.circle(r(radius)?, segments)?)
            }
            "ellipse" => {
                let a = mapping(path, shape, v)?;
                only_keys(path, shape, a, &["width", "height", "segments"])?;
                let (w, h) = (
                    r(f(path, "width", kw(path, a, "width", None)?)?)?,
                    r(f(path, "height", kw(path, a, "height", None)?)?)?,
                );
                let segments = count(
                    path,
                    "segments",
                    kw(path, a, "segments", Some(&Value::from(64)))?,
                )?;
                profile_of(m.run(|context| curve::ellipse_with_context(w, h, segments, context))?)
            }
            "ngon" => {
                let a = mapping(path, shape, v)?;
                only_keys(path, shape, a, &["sides", "r"])?;
                profile_of(curve::regular_ngon(
                    count(path, "sides", kw(path, a, "sides", None)?)?,
                    r(f(path, "r", kw(path, a, "r", None)?)?)?,
                )?)
            }
            "polygon" => {
                let points = v
                    .as_array()
                    .ok_or_else(|| Fail::input(format!("{path}: 'polygon' is a list of [x, y]")))?
                    .iter()
                    .map(|p| {
                        let c = numbers(path, "polygon", p, 2)?;
                        Ok([r(c[0])?, r(c[1])?])
                    })
                    .collect::<Result<Vec<_>, Fail>>()?;
                profile_of(curve::polygon(&points)?)
            }
            "star" => {
                let a = mapping(path, shape, v)?;
                only_keys(path, shape, a, &["points", "outer", "inner"])?;
                profile_of(curve::star(
                    count(path, "points", kw(path, a, "points", None)?)?,
                    r(f(path, "outer", kw(path, a, "outer", None)?)?)?,
                    r(f(path, "inner", kw(path, a, "inner", None)?)?)?,
                )?)
            }
            "rounded_rectangle" => {
                let a = mapping(path, shape, v)?;
                only_keys(path, shape, a, &["width", "height", "r", "segments"])?;
                profile_of(curve::rounded_rectangle(
                    r(f(path, "width", kw(path, a, "width", None)?)?)?,
                    r(f(path, "height", kw(path, a, "height", None)?)?)?,
                    r(f(path, "r", kw(path, a, "r", None)?)?)?,
                    count(
                        path,
                        "segments",
                        kw(path, a, "segments", Some(&Value::from(8)))?,
                    )?,
                )?)
            }
            "offset" => {
                let a = mapping(path, shape, v)?;
                only_keys(path, shape, a, &["distance", "corner", "profile"])?;
                let region = self.of_profile(a, path)?;
                let distance = r(f(path, "distance", kw(path, a, "distance", None)?)?)?;
                let corner = match kw(path, a, "corner", Some(&Value::from("round")))?.as_str() {
                    Some("round") => OffsetCornerStyle2::Round,
                    Some("bevel") => OffsetCornerStyle2::Bevel,
                    Some("miter") => OffsetCornerStyle2::Miter { limit: r(4.0)? },
                    other_ => {
                        return Err(Fail::input(format!(
                            "{path}: 'corner' is {other_:?}; it is one of round, bevel, miter"
                        )));
                    }
                };
                profile_of(region.offset(distance, &corner).map_err(other)?)
            }
            "union" | "difference" | "intersection" | "xor" => {
                let children = self.children(v, path)?;
                self.boolean(shape, children, path)
            }
            unknown => Err(Fail::input(format!(
                "{}: no shape '{unknown}'; a node is one of {}",
                where_(path),
                SHAPES.join(", ")
            ))),
        }
    }

    fn boolean(&mut self, op: &str, children: Vec<Geom>, path: &str) -> Result<Geom, Fail> {
        let mut children = children.into_iter();
        let first = children.next().expect("children() refuses an empty list");
        match first {
            Geom::Solid(mut acc) => {
                for child in children {
                    let Geom::Solid(next) = child else {
                        return Err(Fail::input(format!("{path}: mixes solids and profiles")));
                    };
                    acc = match op {
                        "union" => self.m.union(&acc, &next)?,
                        "difference" => self.m.difference(&acc, &next)?,
                        "intersection" => self.m.intersection(&acc, &next)?,
                        _ => self.m.xor(&acc, &next)?,
                    };
                }
                Ok(Geom::Solid(acc))
            }
            Geom::Profile(mut acc) => {
                let operation = match op {
                    "union" => BooleanOp::Union,
                    "difference" => BooleanOp::Difference,
                    "intersection" => BooleanOp::Intersection,
                    _ => BooleanOp::Xor,
                };
                for child in children {
                    let Geom::Profile(next) = child else {
                        return Err(Fail::input(format!("{path}: mixes profiles and solids")));
                    };
                    acc = acc.boolean_region(&next, operation).map_err(other)?;
                }
                Ok(Geom::Profile(acc))
            }
        }
    }

    fn place(&mut self, geom: Geom, map: &Map<String, Value>, path: &str) -> Result<Geom, Fail> {
        let m = self.m;
        match geom {
            Geom::Solid(mut mesh) => {
                if let Some(v) = map.get("center")
                    && v.as_bool() == Some(true)
                {
                    mesh = m.run(|context| solid::center_with_context(&mesh, context))?;
                }
                if let Some(v) = map.get("scale") {
                    let s = numbers(path, "scale", v, 3)?;
                    mesh = m.scale(&mesh, r(s[0])?, r(s[1])?, r(s[2])?)?;
                }
                if let Some(v) = map.get("rotate") {
                    let a = match v {
                        Value::Array(_) => numbers(path, "rotate", v, 3)?,
                        _ => vec![0.0, 0.0, f(path, "rotate", v)?],
                    };
                    mesh = m.rotate(&mesh, r(a[0])?, r(a[1])?, r(a[2])?)?;
                }
                if let Some(v) = map.get("translate") {
                    let t = numbers(path, "translate", v, 3)?;
                    mesh = m.translate(&mesh, r(t[0])?, r(t[1])?, r(t[2])?)?;
                }
                Ok(Geom::Solid(mesh))
            }
            Geom::Profile(mut region) => {
                if let Some(v) = map.get("center")
                    && v.as_bool() == Some(true)
                    && let Some([x0, y0, x1, y1]) = geometry::region_bounds(&region)?
                {
                    region =
                        curve::translated(&region, r(-(x0 + x1) / 2.0)?, r(-(y0 + y1) / 2.0)?)?;
                }
                if let Some(v) = map.get("scale") {
                    let s = numbers(path, "scale", v, 2)?;
                    region = curve::scaled(&region, r(s[0])?, r(s[1])?)?;
                }
                if let Some(v) = map.get("rotate") {
                    region = curve::rotated(&region, r(f(path, "rotate", v)?)?)?;
                }
                if let Some(v) = map.get("translate") {
                    let t = numbers(path, "translate", v, 2)?;
                    region = curve::translated(&region, r(t[0])?, r(t[1])?)?;
                }
                Ok(Geom::Profile(region))
            }
        }
    }
}
