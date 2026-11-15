//! The part types: each one's parameters made into a csgrs profile or solid.
//!
//! Profiles (`*_region`) are kept apart from the solids built from them, so
//! that a CSG tree can use a gear's or a text's outline as a profile of its
//! own (see `csg.rs`), and so that a sketch type can hand the outline back
//! as it is.

use std::path::Path;

use csgrs::{Csg, Real, TriangleMesh, curve, solid};
use hypercurve::{BooleanOp, CurveRegion2, OffsetCap, OffsetCornerStyle2};

use crate::fail::{Fail, other};
use crate::params::{self, Args};

/// The exact decimal value of a number as it was written, rather than the
/// binary value nearest it: 0.1 here is one tenth. (Rust prints an f64 as the
/// shortest decimal that reads back as the same f64, and never in exponent
/// form, which is what `csgrs::decimal` parses.)
pub fn r(value: f64) -> Result<Real, Fail> {
    if !value.is_finite() {
        return Err(Fail::input(format!("{value} is not a finite number")));
    }
    Ok(csgrs::decimal(&format!("{value}"))?)
}

/// A length, which has to be more than zero.
fn positive(owner: &str, name: &str, value: f64) -> Result<f64, Fail> {
    if value > 0.0 {
        Ok(value)
    } else {
        Err(Fail::input(format!(
            "{owner}: '{name}' is {value}; it has to be more than 0"
        )))
    }
}

pub fn region_bounds(region: &CurveRegion2) -> Result<Option<[f64; 4]>, Fail> {
    let Some(bounds) = curve::bounding_box(region)? else {
        return Ok(None);
    };
    let f = |v: &Real| {
        v.to_f64_lossy()
            .ok_or_else(|| Fail::input("a bound is not a finite number"))
    };
    Ok(Some([
        f(&bounds.mins.x)?,
        f(&bounds.mins.y)?,
        f(&bounds.maxs.x)?,
        f(&bounds.maxs.y)?,
    ]))
}

/// A profile less a hole through the middle.
fn bored(m: &Csg, region: CurveRegion2, diameter: f64) -> Result<CurveRegion2, Fail> {
    if diameter <= 0.0 {
        return Ok(region);
    }
    let hole = m.circle(r(diameter / 2.0)?, 64)?;
    region
        .boolean_region(&hole, BooleanOp::Difference)
        .map_err(other)
}

// ----------------------------------------------------------------- gears ---

pub fn gear_region(m: &Csg, a: &Args) -> Result<CurveRegion2, Fail> {
    let module = positive("gear", "module", a.f("module")?)?;
    let teeth = a.int("teeth")?;
    let clearance = r(a.f("clearance")? * module)?;
    let segments = a.int("segments_per_flank")?;
    let region = match a.string("profile")?.as_str() {
        "cycloidal" => {
            let generating = r(a.f("generating_radius")? * module)?;
            let module = r(module)?;
            m.run(|context| {
                curve::cycloidal_gear_with_context(
                    module, teeth, generating, clearance, segments, context,
                )
            })?
        }
        _ => {
            let pressure = r(a.f("pressure_angle")?)?;
            let backlash = r(a.f("backlash")?)?;
            let module = r(module)?;
            m.run(|context| {
                curve::involute_gear_with_context(
                    module, teeth, pressure, clearance, backlash, segments, context,
                )
            })?
        }
    };
    bored(m, region, a.f("bore")?)
}

pub fn gear(m: &Csg, a: &Args) -> Result<TriangleMesh, Fail> {
    let region = gear_region(m, a)?;
    let thickness = positive("gear", "thickness", a.f("thickness")?)?;
    let helix = a.f("helix_angle")?;
    if helix == 0.0 {
        return Ok(m.extrude(&region, r(thickness)?)?);
    }
    if a.string("profile")? != "involute" {
        return Err(Fail::input(
            "gear: only an involute gear can be helical; set 'helix_angle' to 0",
        ));
    }
    if helix.abs() >= 90.0 {
        return Err(Fail::input(format!(
            "gear: 'helix_angle' is {helix}; it has to be between -90 and 90"
        )));
    }
    // The profile turns by the angle the helix travels around the pitch
    // circle over the face width.
    let pitch_radius = a.f("module")? * a.int("teeth")? as f64 / 2.0;
    let twist = (thickness * helix.to_radians().tan() / pitch_radius).to_degrees();
    let slices = a.int("slices")?.max(1);
    let (height, twist) = (r(thickness)?, r(twist)?);
    Ok(m.run(|context| {
        curve::extrude_twisted(
            &region,
            height,
            twist,
            [Real::one(), Real::one()],
            slices,
            context,
        )
    })?)
}

pub fn rack_region(m: &Csg, a: &Args) -> Result<CurveRegion2, Fail> {
    let module = positive("rack", "module", a.f("module")?)?;
    let teeth = a.int("teeth")?;
    let clearance = r(a.f("clearance")? * module)?;
    let region = match a.string("profile")?.as_str() {
        "cycloidal" => {
            let segments = a.int("segments_per_flank")?;
            let module = r(module)?;
            m.run(|context| {
                curve::cycloidal_rack_with_context(module, teeth, clearance, segments, context)
            })?
        }
        _ => {
            let pressure = r(a.f("pressure_angle")?)?;
            let backlash = r(a.f("backlash")?)?;
            let module = r(module)?;
            m.run(|context| {
                curve::involute_rack_with_context(
                    module, teeth, pressure, clearance, backlash, context,
                )
            })?
        }
    };
    Ok(region)
}

pub fn rack(m: &Csg, a: &Args) -> Result<TriangleMesh, Fail> {
    let region = rack_region(m, a)?;
    let thickness = positive("rack", "thickness", a.f("thickness")?)?;
    Ok(m.extrude(&region, r(thickness)?)?)
}

// ------------------------------------------------------------------ tpms ---

pub fn tpms(_m: &Csg, a: &Args, warnings: &mut Vec<String>) -> Result<TriangleMesh, Fail> {
    let size = [
        positive("tpms", "size_x", a.f("size_x")?)?,
        positive("tpms", "size_y", a.f("size_y")?)?,
        positive("tpms", "size_z", a.f("size_z")?)?,
    ];
    let period = r(positive("tpms", "period", a.f("period")?)?)?;
    let thickness = r(positive("tpms", "thickness", a.f("thickness")?)?)?;
    let iso = r(a.f("iso")?)?;
    let asked = a.int("resolution")?;
    if !(2..=400).contains(&asked) {
        return Err(Fail::input(format!(
            "tpms: 'resolution' is {asked}; it has to be from 2 to 400"
        )));
    }
    let bounds = solid::cuboid(r(size[0])?, r(size[1])?, r(size[2])?)?;
    let surface = a.string("surface")?;
    // csgrs's surface nets can emit a degenerate triangle -- one whose corners
    // fall on a line -- at some resolutions and not at the next, and then
    // refuses its own mesh. A sample more is a field sampled a little more
    // finely, which is a fair answer to "this resolution, or about it".
    let mut last = None;
    for resolution in asked..asked + 4 {
        let built = match surface.as_str() {
            "schwarz_p" => solid::schwarz_p_solid(
                &bounds,
                resolution,
                period.clone(),
                iso.clone(),
                thickness.clone(),
            ),
            "schwarz_d" => solid::schwarz_d_solid(
                &bounds,
                resolution,
                period.clone(),
                iso.clone(),
                thickness.clone(),
            ),
            _ => solid::gyroid_solid(
                &bounds,
                resolution,
                period.clone(),
                iso.clone(),
                thickness.clone(),
            ),
        };
        match built {
            Ok(mesh) => {
                if resolution != asked {
                    warnings.push(format!(
                        "csgrs meshed the field at resolution {asked} with a degenerate triangle; built at {resolution} instead"
                    ));
                }
                return Ok(mesh);
            }
            Err(e) if e.to_string().contains("degenerate") => last = Some(e),
            Err(e) => return Err(e.into()),
        }
    }
    Err(last.expect("the loop runs at least once").into())
}

// --------------------------------------------------------------- airfoil ---

pub fn airfoil_region(m: &Csg, a: &Args) -> Result<CurveRegion2, Fail> {
    let naca = a.string("naca")?;
    let digits: Vec<u32> = naca.trim().chars().filter_map(|c| c.to_digit(10)).collect();
    if digits.len() != 4 || naca.trim().len() != 4 {
        return Err(Fail::input(format!(
            "airfoil: 'naca' is '{naca}'; it has to be four digits, such as '2412'"
        )));
    }
    let camber = r(digits[0] as f64)?;
    let position = r(digits[1] as f64)?;
    let thickness = r((digits[2] * 10 + digits[3]) as f64)?;
    let chord = r(positive("airfoil", "chord", a.f("chord")?)?)?;
    let samples = a.int("samples")?;
    Ok(m.run(|context| {
        curve::airfoil_naca4_with_context(camber, position, thickness, chord, samples, context)
    })?)
}

pub fn airfoil(m: &Csg, a: &Args) -> Result<TriangleMesh, Fail> {
    let region = airfoil_region(m, a)?;
    let span = positive("airfoil", "span", a.f("span")?)?;
    Ok(m.extrude(&region, r(span)?)?)
}

// ------------------------------------------------------------------ text ---

/// How many Hershey units a capital letter is tall in the Roman fonts, which
/// is what 'size' is measured against.
const HERSHEY_CAP_HEIGHT: f64 = 21.0;

/// How tall a capital letter is, as a fraction of a TrueType font's point size.
/// Close enough for most Latin fonts; the exact figure is the font's own.
const TRUETYPE_CAP_HEIGHT: f64 = 0.7;

fn hershey_font(name: &str) -> Option<hypercurve::hershey::Font<'static>> {
    hypercurve::hershey::fonts::ALL
        .iter()
        .find(|font| font.name().eq_ignore_ascii_case(name))
        .copied()
}

pub fn hershey_font_names() -> Vec<&'static str> {
    hypercurve::hershey::fonts::ALL
        .iter()
        .map(|font| font.name())
        .collect()
}

/// Where a text's glyphs come from.
pub enum Glyphs<'p> {
    /// A Hershey font built into csgrs, named by the 'font' parameter.
    Hershey,
    /// The TrueType or OpenType font in a file.
    File(&'p Path),
}

pub fn text_region(m: &Csg, a: &Args, glyphs: Glyphs) -> Result<CurveRegion2, Fail> {
    let text = a.string("text")?;
    let size = positive("text", "size", a.f("size")?)?;
    let region = match glyphs {
        Glyphs::Hershey => {
            let font = a.string("font")?;
            let hershey = hershey_font(&font).ok_or_else(|| {
                Fail::input(format!(
                    "text: no built-in font '{font}'; there are {}. For a font file, use the 'truetype' part type",
                    hershey_font_names().join(", ")
                ))
            })?;
            let stroke = match a.f("stroke")? {
                w if w > 0.0 => w,
                _ => size / 10.0,
            };
            let scale = r(size / HERSHEY_CAP_HEIGHT)?;
            let strings = m.run(|context| {
                curve::hershey_strings_with_context(&text, &hershey, scale, context)
            })?;
            let half = r(stroke / 2.0)?;
            let mut letters: Option<CurveRegion2> = None;
            for string in strings {
                let band = CurveRegion2::stroke_path(
                    &curve_path(&string)?,
                    half.clone(),
                    &OffsetCornerStyle2::Round,
                    OffsetCap::Round,
                )
                .map_err(other)?;
                letters = Some(match letters {
                    None => band,
                    Some(so_far) => so_far
                        .boolean_region(&band, BooleanOp::Union)
                        .map_err(other)?,
                });
            }
            // Hershey glyphs are drawn about their centre line, with the
            // baseline 9 units below it; a part's text stands on Y = 0.
            let letters = letters.unwrap_or_else(curve::empty);
            if region_bounds(&letters)?.is_none() {
                letters
            } else {
                curve::translated(&letters, Real::zero(), r(9.0 * size / HERSHEY_CAP_HEIGHT)?)?
            }
        }
        Glyphs::File(path) => {
            let data = std::fs::read(path)
                .map_err(|e| Fail::input(format!("truetype: {}: {e}", path.display())))?;
            // csgrs measures a font in points of 127/360 mm.
            let points = r(size / TRUETYPE_CAP_HEIGHT * 360.0 / 127.0)?;
            m.run(|context| curve::truetype_text_with_context(&text, &data, points, context))?
        }
    };
    align(region, &a.string("halign")?)
}

/// An open run of a Hershey glyph as the path a stroke is drawn along.
fn curve_path(string: &hypercurve::CurveString2) -> Result<hypercurve::CurvePath2, Fail> {
    let curves = string
        .segments()
        .iter()
        .map(|segment| match segment {
            hypercurve::Segment2::Line(line) => hypercurve::Curve2::from(line.clone()),
            hypercurve::Segment2::Arc(arc) => hypercurve::Curve2::from(arc.clone()),
        })
        .collect();
    hypercurve::CurvePath2::try_new(curves).map_err(other)
}

fn align(region: CurveRegion2, halign: &str) -> Result<CurveRegion2, Fail> {
    let Some([x0, _, x1, _]) = region_bounds(&region)? else {
        return Ok(region);
    };
    let dx = match halign {
        "center" => -(x0 + x1) / 2.0,
        "right" => -x1,
        _ => -x0,
    };
    if dx == 0.0 {
        return Ok(region);
    }
    Ok(curve::translated(&region, r(dx)?, Real::zero())?)
}

pub fn text(m: &Csg, a: &Args, glyphs: Glyphs) -> Result<TriangleMesh, Fail> {
    let region = text_region(m, a, glyphs)?;
    if region_bounds(&region)?.is_none() {
        return Err(Fail::input(
            "text: there is nothing to draw; 'text' is empty or only spaces",
        ));
    }
    let depth = positive("text", "depth", a.f("depth")?)?;
    Ok(m.extrude(&region, r(depth)?)?)
}

// ----------------------------------------------------------------- image ---

pub fn image_region(a: &Args, path: &Path) -> Result<CurveRegion2, Fail> {
    let picture =
        image::open(path).map_err(|e| Fail::input(format!("image: {}: {e}", path.display())))?;
    let mut gray = picture.to_luma8();
    if a.boolean("invert")? {
        for pixel in gray.pixels_mut() {
            pixel.0[0] = 255 - pixel.0[0];
        }
    }
    let threshold = a.int("threshold")?.min(255) as u8;
    let traced = curve::from_image(&gray, threshold).map_err(other)?;
    if region_bounds(&traced)?.is_none() {
        return Err(Fail::input(format!(
            "image: no pixel of {} is at or above the threshold {threshold}{}",
            path.display(),
            if a.boolean("invert")? {
                " once inverted"
            } else {
                "; set 'invert' for dark material"
            }
        )));
    }
    // Pixels count down from the top row; a part's Y counts up.
    let size = positive("image", "pixel_size", a.f("pixel_size")?)?;
    let height = gray.height() as f64;
    let flipped = curve::scaled(&traced, r(size)?, r(-size)?)?;
    Ok(curve::translated(
        &flipped,
        Real::zero(),
        r(height * size)?,
    )?)
}

pub fn image(m: &Csg, a: &Args, path: &Path) -> Result<TriangleMesh, Fail> {
    let region = image_region(a, path)?;
    let depth = positive("image", "depth", a.f("depth")?)?;
    Ok(m.extrude(&region, r(depth)?)?)
}

// ------------------------------------------------------------------ vrml ---

pub fn vrml(
    m: &Csg,
    a: &Args,
    path: &Path,
    warnings: &mut Vec<String>,
) -> Result<TriangleMesh, Fail> {
    let bytes =
        std::fs::read(path).map_err(|e| Fail::input(format!("vrml: {}: {e}", path.display())))?;
    let read = csgrs::io::vrml::from_vrml_with_context(&bytes, m.context())
        .map_err(|e| other(e).context(&format!("vrml: {}", path.display())))?;
    let read = m.record(read);
    if read.ignored_non_mesh_geometry_count > 0 {
        warnings.push(format!(
            "{} line or point node(s) carry no surface and were left out",
            read.ignored_non_mesh_geometry_count
        ));
    }
    let degenerate = read.ignored_degenerate_polygon_count + read.ignored_degenerate_triangle_count;
    if degenerate > 0 {
        warnings.push(format!(
            "{degenerate} degenerate polygon(s) or triangle(s) were left out"
        ));
    }
    let scale = positive("vrml", "units", a.f("units")?)?;
    if scale == 1.0 {
        return Ok(read.mesh);
    }
    let s = r(scale)?;
    Ok(m.scale(&read.mesh, s.clone(), s.clone(), s)?)
}

pub fn args_for(
    name: &str,
    given: Option<&serde_json::Map<String, serde_json::Value>>,
) -> Result<Args, Fail> {
    let spec = params::find(name).ok_or_else(|| Fail::input(format!("no part type '{name}'")))?;
    Args::new(name, &spec.params, given)
}
