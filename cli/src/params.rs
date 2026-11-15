//! The parameters each part type takes: their names, types, defaults and
//! meaning, and reading them out of a request.
//!
//! One table serves three readers: `build` (defaults and type checks),
//! `describe` (what PartCAD's side and the documentation are checked against),
//! and the error a misspelt parameter earns, which lists the right names.

use serde_json::{Map, Value, json};

use crate::fail::Fail;

#[derive(Clone, Copy, Debug)]
pub enum Kind {
    Float,
    Int,
    Bool,
    Str,
    Choice(&'static [&'static str]),
}

impl Kind {
    /// The type as PartCAD's `parameters:` spells it.
    pub fn name(self) -> &'static str {
        match self {
            Kind::Float => "float",
            Kind::Int => "int",
            Kind::Bool => "bool",
            Kind::Str | Kind::Choice(_) => "string",
        }
    }
}

pub struct Param {
    pub name: &'static str,
    pub kind: Kind,
    pub default: Value,
    pub desc: &'static str,
}

fn p(name: &'static str, kind: Kind, default: Value, desc: &'static str) -> Param {
    Param {
        name,
        kind,
        default,
        desc,
    }
}

pub const PROFILES: &[&str] = &["involute", "cycloidal"];
pub const SURFACES: &[&str] = &["gyroid", "schwarz_p", "schwarz_d"];
pub const ALIGNMENTS: &[&str] = &["left", "center", "right"];

/// A part type: what it builds, the extension of the file it reads if it reads
/// one, and its parameters.
pub struct PartType {
    pub name: &'static str,
    pub desc: &'static str,
    pub extension: Option<&'static str>,
    pub params: Vec<Param>,
}

pub fn gear_params() -> Vec<Param> {
    vec![
        p(
            "profile",
            Kind::Choice(PROFILES),
            json!("involute"),
            "The tooth form: 'involute' or 'cycloidal'.",
        ),
        p(
            "module",
            Kind::Float,
            json!(1.0),
            "The module in mm: the pitch diameter divided by the number of teeth.",
        ),
        p(
            "teeth",
            Kind::Int,
            json!(20),
            "The number of teeth (at least 4 for an involute gear, 3 for a cycloidal one).",
        ),
        p(
            "pressure_angle",
            Kind::Float,
            json!(20.0),
            "The pressure angle of an involute gear, in degrees.",
        ),
        p(
            "clearance",
            Kind::Float,
            json!(0.25),
            "The root clearance, as a multiple of the module.",
        ),
        p(
            "backlash",
            Kind::Float,
            json!(0.0),
            "The backlash of an involute gear, in mm.",
        ),
        p(
            "generating_radius",
            Kind::Float,
            json!(2.0),
            "The radius of the circle a cycloidal tooth is rolled with, as a multiple of the module (csgrs accepts about 1.5 to 3).",
        ),
        p(
            "thickness",
            Kind::Float,
            json!(5.0),
            "The face width: how far the profile is extruded along Z, in mm.",
        ),
        p(
            "helix_angle",
            Kind::Float,
            json!(0.0),
            "The helix angle of an involute gear, in degrees; 0 is a spur gear.",
        ),
        p(
            "slices",
            Kind::Int,
            json!(16),
            "How many slices a helical gear's twist is built from.",
        ),
        p(
            "bore",
            Kind::Float,
            json!(0.0),
            "The diameter of a hole through the centre, in mm; 0 for none.",
        ),
        p(
            "segments_per_flank",
            Kind::Int,
            json!(8),
            "How many straight segments approximate each tooth flank.",
        ),
    ]
}

pub fn rack_params() -> Vec<Param> {
    vec![
        p(
            "profile",
            Kind::Choice(PROFILES),
            json!("involute"),
            "The tooth form: 'involute' or 'cycloidal'.",
        ),
        p(
            "module",
            Kind::Float,
            json!(1.0),
            "The module in mm, matching the gear the rack meshes with.",
        ),
        p("teeth", Kind::Int, json!(10), "The number of teeth."),
        p(
            "pressure_angle",
            Kind::Float,
            json!(20.0),
            "The pressure angle of an involute rack, in degrees.",
        ),
        p(
            "clearance",
            Kind::Float,
            json!(0.25),
            "The root clearance, as a multiple of the module.",
        ),
        p(
            "backlash",
            Kind::Float,
            json!(0.0),
            "The backlash of an involute rack, in mm.",
        ),
        p(
            "thickness",
            Kind::Float,
            json!(5.0),
            "The face width: how far the profile is extruded along Z, in mm.",
        ),
        p(
            "segments_per_flank",
            Kind::Int,
            json!(12),
            "How many segments approximate each cycloidal flank.",
        ),
    ]
}

pub fn tpms_params() -> Vec<Param> {
    vec![
        p(
            "surface",
            Kind::Choice(SURFACES),
            json!("gyroid"),
            "The triply periodic minimal surface: 'gyroid', 'schwarz_p' or 'schwarz_d'.",
        ),
        p(
            "size_x",
            Kind::Float,
            json!(20.0),
            "The extent of the block along X, in mm.",
        ),
        p(
            "size_y",
            Kind::Float,
            json!(20.0),
            "The extent of the block along Y, in mm.",
        ),
        p(
            "size_z",
            Kind::Float,
            json!(20.0),
            "The extent of the block along Z, in mm.",
        ),
        p(
            "period",
            Kind::Float,
            json!(10.0),
            "The cell size: the distance the surface repeats over, in mm.",
        ),
        p(
            "thickness",
            Kind::Float,
            json!(1.0),
            "The wall thickness, in the field's units (about mm for small values).",
        ),
        p(
            "iso",
            Kind::Float,
            json!(0.0),
            "The level of the field the wall is centred on; 0 splits space in two equal halves.",
        ),
        p(
            "resolution",
            Kind::Int,
            json!(24),
            "How many samples the field is taken at along each axis; the mesh grows with its square, and the time with its cube.",
        ),
    ]
}

pub fn airfoil_params() -> Vec<Param> {
    vec![
        p(
            "naca",
            Kind::Str,
            json!("2412"),
            "The four-digit NACA designation, such as '0012' or '2412'.",
        ),
        p(
            "chord",
            Kind::Float,
            json!(100.0),
            "The chord length, in mm.",
        ),
        p(
            "span",
            Kind::Float,
            json!(200.0),
            "How far the section is extruded along Z, in mm.",
        ),
        p(
            "samples",
            Kind::Int,
            json!(80),
            "How many points each surface of the section is sampled at (at least 10).",
        ),
    ]
}

pub fn text_params() -> Vec<Param> {
    vec![
        p(
            "text",
            Kind::Str,
            json!("PartCAD"),
            "The text. A newline starts a new line.",
        ),
        p(
            "size",
            Kind::Float,
            json!(10.0),
            "The height of a capital letter, in mm.",
        ),
        p(
            "depth",
            Kind::Float,
            json!(2.0),
            "How far the letters are extruded along Z, in mm.",
        ),
        p(
            "font",
            Kind::Str,
            json!("futural"),
            "The built-in single-stroke Hershey font, by name: such as 'futural', 'futuram', 'rowmans', 'rowmand', \
             'timesr', 'scripts', 'gothiceng' or 'greek'.",
        ),
        p(
            "stroke",
            Kind::Float,
            json!(0.0),
            "The width of the strokes, in mm; 0 for a tenth of 'size'.",
        ),
        p(
            "halign",
            Kind::Choice(ALIGNMENTS),
            json!("left"),
            "Where X = 0 is: the 'left', 'center' or 'right' of the text.",
        ),
    ]
}

pub fn truetype_params() -> Vec<Param> {
    vec![
        p(
            "text",
            Kind::Str,
            json!("PartCAD"),
            "The text. A newline starts a new line.",
        ),
        p(
            "size",
            Kind::Float,
            json!(10.0),
            "The height of a capital letter, in mm (taken as 70% of the font's em; the exact figure is the font's own).",
        ),
        p(
            "depth",
            Kind::Float,
            json!(2.0),
            "How far the letters are extruded along Z, in mm.",
        ),
        p(
            "halign",
            Kind::Choice(ALIGNMENTS),
            json!("left"),
            "Where X = 0 is: the 'left', 'center' or 'right' of the text.",
        ),
    ]
}

pub fn image_params() -> Vec<Param> {
    vec![
        p(
            "threshold",
            Kind::Int,
            json!(128),
            "The grey level (0-255) at or above which a pixel is material.",
        ),
        p(
            "invert",
            Kind::Bool,
            json!(false),
            "Make the dark pixels material instead of the light ones.",
        ),
        p(
            "pixel_size",
            Kind::Float,
            json!(0.1),
            "The size of one pixel, in mm.",
        ),
        p(
            "depth",
            Kind::Float,
            json!(1.0),
            "How far the traced outline is extruded along Z, in mm.",
        ),
    ]
}

pub fn vrml_params() -> Vec<Param> {
    // Not 'scale': that is a key of every PartCAD part already, which PartCAD
    // applies itself, so a part type's 'scale' would be applied twice.
    vec![p(
        "units",
        Kind::Float,
        json!(1.0),
        "What one unit of the file is in mm. KiCad's VRML models are in tenths of an inch: 2.54.",
    )]
}

pub fn part_types() -> Vec<PartType> {
    vec![
        PartType {
            name: "gear",
            desc: "An involute (spur or helical) or cycloidal gear, extruded from csgrs's exact tooth profile.",
            extension: None,
            params: gear_params(),
        },
        PartType {
            name: "rack",
            desc: "An involute or cycloidal gear rack, extruded.",
            extension: None,
            params: rack_params(),
        },
        PartType {
            name: "tpms",
            desc: "A block of triply periodic minimal surface lattice (gyroid, Schwarz P or Schwarz D) with walls of a given thickness.",
            extension: None,
            params: tpms_params(),
        },
        PartType {
            name: "airfoil",
            desc: "A NACA four-digit airfoil section, extruded along its span.",
            extension: None,
            params: airfoil_params(),
        },
        PartType {
            name: "text",
            desc: "Text in one of the Hershey single-stroke fonts built into csgrs, its strokes given a width and extruded.",
            extension: None,
            params: text_params(),
        },
        PartType {
            name: "truetype",
            desc: "Text in a TrueType or OpenType font -- the part's file is the font -- extruded from the glyphs' outlines.",
            extension: Some("ttf"),
            params: truetype_params(),
        },
        PartType {
            name: "image",
            desc: "A raster image (PNG, JPEG, BMP, GIF) traced at a grey level and extruded: a logo, a stencil, a lithophane's outline.",
            extension: Some("png"),
            params: image_params(),
        },
        PartType {
            name: "vrml",
            desc: "The polygon meshes of a VRML 2.0 scene ('.wrl'), such as a KiCad component model.",
            extension: Some("wrl"),
            params: vrml_params(),
        },
        PartType {
            name: "csg",
            desc: "A constructive solid geometry tree written in YAML or JSON: primitives, profiles, extrusions, booleans and transforms, \
                   with expressions over the part's parameters.",
            extension: Some("csgrs"),
            params: vec![],
        },
    ]
}

pub fn find(name: &str) -> Option<PartType> {
    part_types().into_iter().find(|t| t.name == name)
}

/// The parameters of one request, checked against one part type's table.
pub struct Args {
    values: Map<String, Value>,
    owner: String,
}

impl Args {
    /// The request's values over the table's defaults. A name the table does
    /// not have is an error: it is a misspelling, and building the part
    /// without it would be building a different part than the one asked for.
    pub fn new(
        owner: &str,
        specs: &[Param],
        given: Option<&Map<String, Value>>,
    ) -> Result<Self, Fail> {
        let mut values = Map::new();
        for spec in specs {
            values.insert(spec.name.to_string(), spec.default.clone());
        }
        if let Some(given) = given {
            for (name, value) in given {
                let Some(spec) = specs.iter().find(|s| s.name == name) else {
                    let known: Vec<&str> = specs.iter().map(|s| s.name).collect();
                    return Err(Fail::input(format!(
                        "{owner}: no parameter '{name}'; it takes {}",
                        if known.is_empty() {
                            "none".to_string()
                        } else {
                            known.join(", ")
                        }
                    )));
                };
                if value.is_null() {
                    continue;
                }
                check(owner, spec, value)?;
                values.insert(name.clone(), value.clone());
            }
        }
        Ok(Self {
            values,
            owner: owner.to_string(),
        })
    }

    pub fn f(&self, name: &str) -> Result<f64, Fail> {
        number(&self.owner, name, &self.values[name])
    }

    pub fn int(&self, name: &str) -> Result<usize, Fail> {
        let value = self.f(name)?;
        if value < 0.0 || value.fract() != 0.0 || value > u32::MAX as f64 {
            return Err(Fail::input(format!(
                "{}: '{name}' is {value}, not a whole number of at least 0",
                self.owner
            )));
        }
        Ok(value as usize)
    }

    pub fn string(&self, name: &str) -> Result<String, Fail> {
        match &self.values[name] {
            Value::String(s) => Ok(s.clone()),
            Value::Number(n) => Ok(n.to_string()),
            other => Err(Fail::input(format!(
                "{}: '{name}' is {other}, not text",
                self.owner
            ))),
        }
    }

    pub fn boolean(&self, name: &str) -> Result<bool, Fail> {
        match &self.values[name] {
            Value::Bool(b) => Ok(*b),
            Value::String(s)
                if matches!(s.to_ascii_lowercase().as_str(), "true" | "yes" | "on" | "1") =>
            {
                Ok(true)
            }
            Value::String(s)
                if matches!(
                    s.to_ascii_lowercase().as_str(),
                    "false" | "no" | "off" | "0"
                ) =>
            {
                Ok(false)
            }
            other => Err(Fail::input(format!(
                "{}: '{name}' is {other}, not true or false",
                self.owner
            ))),
        }
    }
}

/// A number from JSON, where PartCAD may hand a numeric parameter over as
/// text ("12.5") as readily as a number.
pub fn number(owner: &str, name: &str, value: &Value) -> Result<f64, Fail> {
    let found = match value {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse::<f64>().ok(),
        _ => None,
    };
    match found {
        Some(v) if v.is_finite() => Ok(v),
        _ => Err(Fail::input(format!(
            "{owner}: '{name}' is {value}, not a number"
        ))),
    }
}

fn check(owner: &str, spec: &Param, value: &Value) -> Result<(), Fail> {
    match spec.kind {
        Kind::Float | Kind::Int => number(owner, spec.name, value).map(|_| ()),
        Kind::Bool => match value {
            Value::Bool(_) | Value::String(_) => Ok(()),
            _ => Err(Fail::input(format!(
                "{owner}: '{}' is {value}, not true or false",
                spec.name
            ))),
        },
        Kind::Str => match value {
            Value::String(_) | Value::Number(_) => Ok(()),
            _ => Err(Fail::input(format!(
                "{owner}: '{}' is {value}, not text",
                spec.name
            ))),
        },
        Kind::Choice(choices) => match value {
            Value::String(s) if choices.contains(&s.as_str()) => Ok(()),
            _ => Err(Fail::input(format!(
                "{owner}: '{}' is {value}; it is one of {}",
                spec.name,
                choices.join(", ")
            ))),
        },
    }
}

/// Everything `describe` prints: the part types and their parameters.
pub fn describe() -> Value {
    let mut types = Map::new();
    for t in part_types() {
        let params: Vec<Value> = t
            .params
            .iter()
            .map(|s| {
                let mut entry = json!({
                    "name": s.name,
                    "type": s.kind.name(),
                    "default": s.default,
                    "desc": s.desc,
                });
                if let Kind::Choice(choices) = s.kind {
                    entry["choices"] = json!(choices);
                }
                entry
            })
            .collect();
        types.insert(
            t.name.to_string(),
            json!({"desc": t.desc, "extension": t.extension, "parameters": params}),
        );
    }
    json!({
        "name": env!("CARGO_PKG_NAME"),
        "version": env!("CARGO_PKG_VERSION"),
        "csgrs": crate::CSGRS_REV,
        "types": types,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_default_is_of_its_declared_type() {
        for t in part_types() {
            for spec in &t.params {
                check(t.name, spec, &spec.default)
                    .unwrap_or_else(|e| panic!("{}.{}: {e}", t.name, spec.name));
            }
        }
    }

    #[test]
    fn a_misspelt_parameter_is_refused_with_the_right_names() {
        let given = json!({"teth": 3});
        let error = Args::new("gear", &gear_params(), given.as_object())
            .err()
            .unwrap()
            .to_string();
        assert!(
            error.contains("no parameter 'teth'") && error.contains("teeth"),
            "{error}"
        );
    }

    #[test]
    fn a_number_may_be_written_as_text() {
        let given = json!({"teeth": "24", "module": "1.5"});
        let args = Args::new("gear", &gear_params(), given.as_object()).unwrap();
        assert_eq!(args.int("teeth").unwrap(), 24);
        assert_eq!(args.f("module").unwrap(), 1.5);
    }

    #[test]
    fn a_choice_outside_its_list_is_refused() {
        let given = json!({"profile": "helical"});
        assert!(Args::new("gear", &gear_params(), given.as_object()).is_err());
    }
}
