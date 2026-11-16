# csgrs for PartCAD

A [PartCAD](https://github.com/partcad/partcad) plugin package for [csgrs](https://github.com/timschmidt/csgrs), the
exact-arithmetic constructive solid geometry kernel written in Rust, and for **hyperdrc**, the printed circuit
board design-readiness checker built on it.

| What | Declared as | Reads |
|---|---|---|
| An involute (spur or helical) or cycloidal gear | `type: csgrs:gear` | parameters |
| An involute or cycloidal rack | `type: csgrs:rack` | parameters |
| A gyroid, Schwarz P or Schwarz D lattice block | `type: csgrs:tpms` | parameters |
| A NACA four-digit airfoil, extruded | `type: csgrs:airfoil` | parameters |
| Text in one of 33 built-in Hershey single-stroke fonts | `type: csgrs:text` | parameters |
| Text in a TrueType or OpenType font | `type: csgrs:truetype` | the font (`<name>.ttf`) |
| A raster image traced and extruded | `type: csgrs:image` | a PNG, JPEG, BMP or GIF (`<name>.png`) |
| A VRML 2.0 model, such as a KiCad component's | `type: csgrs:vrml` | `<name>.wrl` |
| A CSG tree written in YAML or JSON | `type: csgrs:csg` | `<name>.csgrs` |
| Ready-made parametric parts | `csgrs:spur_gear`, `helical_gear`, `cycloidal_gear`, `rack`, `gyroid`, `schwarz_p`, `schwarz_d`, `naca_airfoil`, `text` | — |
| The same profiles as sketches | `type: csgrs:gear_profile`, `rack_profile`, `airfoil_profile`, `text_profile`, `truetype_profile`, `image_profile`, `csg_profile` | as the part type |
| Ready-made parametric sketches | `csgrs:gear_outline`, `naca_section`, `text_outline` | — |
| hyperdrc's PCB design-readiness checks | `qual: other: <name>: {path: csgrs:hyperdrc}` | a `.kicad_pcb`, or Gerbers |
| Any of the types above, run in this package's image | `type: csgrs:<type>_in_container`, such as `csgrs:gear_in_container` | as the type |

`csgrs:` is the name the workspace imports this package under; use whatever name the dependency is given.
It needs PartCAD 0.10.0 or newer, the first that reads a plugin package's `qual:` section; the sketch types need a
PartCAD with `sketchTypes:` -- 0.10.25 or newer ([partcad/partcad#963](https://github.com/partcad/partcad/pull/963))
-- and an older one builds everything else and says it does not know a sketch's type.

Every one of these has a worked example in
[partcad/partcad-examples-csgrs](https://github.com/partcad/partcad-examples-csgrs) (`//pub/examples/script/csgrs`
in the PartCAD index), with a picture of each.

## Using it

```yaml
dependencies:
  csgrs:
    type: git
    url: https://github.com/partcad/partcad-csgrs
    track: main

parts:
  pinion:
    type: csgrs:gear            # a part type, set by the part's own keys
    teeth: 12
    module: 1.5
    thickness: 6
    bore: 5
  wheel:
    type: csgrs:gear            # ...or by declared parameters, which can be set from outside: 'wheel;teeth=48'
    parameters:
      teeth:
        type: int
        default: 36
  lattice:
    type: csgrs:tpms
    surface: gyroid
    size_x: 40
    period: 8
  label:
    type: csgrs:text
    text: PartCAD
    font: rowmans
  badge:
    type: csgrs:truetype        # reads badge.ttf -- or 'path:', or 'fileFrom: url' and 'fileUrl:'
    text: OK
  logo:
    type: csgrs:image           # reads logo.png
    pixel_size: 0.2
    depth: 1.5

assemblies:
  gearbox:
    type: assy
    # ...and the ready-made parts need nothing declared at all:
    # csgrs:spur_gear;teeth=30,module=2.0
```

Every parameter, its type, default and meaning is what `csgrs-partcad describe` prints, and what `pc info
csgrs:spur_gear` shows for a ready-made part. In short (lengths in mm, angles in degrees):

* **gear** — `profile` (`involute` or `cycloidal`), `module`, `teeth`, `pressure_angle`, `clearance` and
  `generating_radius` (as multiples of the module), `backlash`, `thickness`, `helix_angle` (non-zero: a helical
  gear), `slices`, `bore`, `segments_per_flank`.
* **rack** — `profile`, `module`, `teeth`, `pressure_angle`, `clearance`, `backlash`, `thickness`,
  `segments_per_flank`.
* **tpms** — `surface` (`gyroid`, `schwarz_p`, `schwarz_d`), `size_x`, `size_y`, `size_z`, `period` (the cell
  size), `thickness` (the wall), `iso`, `resolution` (samples per axis; the mesh grows with its square).
* **airfoil** — `naca` (such as `"0012"` or `"2412"`), `chord`, `span`, `samples`.
* **text** — `text`, `size` (cap height), `depth`, `font` (`futural`, `futuram`, `rowmans`, `rowmand`, `timesr`,
  `scripts`, `gothiceng`, `greek` ... 33 in all), `stroke` (width), `halign`.
* **truetype** — `text`, `size`, `depth`, `halign`.
* **image** — `threshold` (the grey level at or above which a pixel is material), `invert`, `pixel_size`, `depth`.
* **vrml** — `units` (what one unit of the file is in mm; KiCad's models are in tenths of an inch: `2.54`).
* **csg** — none of its own: its tree may use any parameter the part declares.

Any part may also say `policy:` — how csgrs decides its geometric predicates: `strict` (certified exactly, or a
failure), `approximate` (csgrs's 512-bit fallback), or the default, `auto`: strict, and approximate only where
strict cannot decide, with a note in PartCAD's debug log (`pc --verbose`) saying so.

### Sketches

Every part type built from a region of the plane has a sketch type for that region: `gear_profile`,
`rack_profile`, `airfoil_profile`, `text_profile`, `truetype_profile`, `image_profile` and `csg_profile`, each
taking its part type's parameters less those only a solid has (`thickness`, `depth`, `span`, `helix_angle`). A
sketch is the region's faces, each with its holes, and every boundary is the curve csgrs kept -- a line, a
circular arc, a Bezier curve -- rather than a polygon standing in for it; a TrueType glyph is its own Bezier
curves. (Where csgrs itself samples a curve -- a gear's involute flanks, a `circle` -- the sketch has those
samples, as the part does.)

```yaml
sketches:
  wheel:
    type: csgrs:gear_profile
    teeth: 24
    bore: 5
  outline:
    type: csgrs:csg_profile     # reads outline.csgrs: a tree whose root is a profile
parts:
  wheel_plate:
    type: extrude               # PartCAD's own extrusion, of csgrs's profile
    sketch: wheel
    depth: 3
```

### CSG trees

A `csg` part reads a tree from its file, `<name>.csgrs`, in YAML (of which JSON is a part). Each node is a mapping
with one *shape* key, saying what it is, and any of the *placement* keys, which move it: `scale`, `rotate`
(degrees; a number turns about Z), `translate` and `center: true` — applied in that order whatever order they are
written in.

```yaml
# bracket.csgrs: a plate with a hub, and a hole through both
difference:
  - union:
      - cuboid: ["${width}", 20, 5]
      - cylinder: {r: 6, h: 5}
        translate: ["${width / 2}", 10, 5]
  - cylinder: {r: "${hole / 2}", h: 20, segments: 48}
    translate: ["${width / 2}", 10, -5]
```

```yaml
parts:
  bracket:
    type: csgrs:csg
    parameters:
      width: {type: float, default: 40.0}
      hole: {type: float, default: 6.0}
```

| Kind | Shapes |
|---|---|
| Solids | `cube: s`, `cuboid: [x, y, z]`, `sphere: r` or `{r, segments, stacks}`, `cylinder: {r, h, segments}`, `cone: {r1, r2, h, segments}`, `torus: {R, r, segments, minor_segments}`, `polyhedron: {points, faces}` |
| Profiles (the XY plane) | `square: s`, `rectangle: [x, y]`, `circle: r` or `{r, segments}`, `ellipse: {width, height, segments}`, `ngon: {sides, r}`, `polygon: [[x, y], ...]`, `star: {points, outer, inner}`, `rounded_rectangle: {width, height, r, segments}` |
| Profile to solid | `extrude: {height, twist, scale, slices, profile}`, `revolve: {angle, segments, profile}` (about Y), `sweep: {path: [[x, y, z], ...], profile}` |
| Profile to profile | `offset: {distance, corner: round \| bevel \| miter, profile}` |
| Either | `union`, `difference`, `intersection`, `xor`: a list of nodes, all solids or all profiles |
| Solids from solids | `hull: [...]`, `minkowski: [a, b]` |
| The part types | `gear`, `rack`, `tpms`, `airfoil`, `text`, `truetype` (with `font:`, a file beside the tree) as solids, and `gear_profile`, `rack_profile`, `airfoil_profile`, `text_profile`, `truetype_profile` as profiles, each with that type's parameters |

A `csg_profile` sketch reads the same kind of tree, whose root is a profile rather than a solid.

A string `"${expression}"` anywhere in the tree is replaced by its value over the part's parameters; `${...}`
inside a longer string (`text: "M${size}"`) by its text. An expression has arithmetic, comparisons, `x if c else
y`, `pi`, and `abs min max round int float str floor ceil sqrt sin cos tan asin acos atan atan2 hypot log exp`
(the trigonometry in degrees) — and nothing else of Python.

### The PCB check

```yaml
parts:
  board:
    type: kicad
    finalized: true             # a kicad part is a board in progress, which 'pc qual test' leaves alone, until it says so
    qual:
      other:
        drc:
          path: csgrs:hyperdrc
          # strict: true        # fail on a warning as well as on an error
          # gerbers: fab/       # check the fabrication package too: a directory or an archive
          # config: rules.json  # hyperdrc's own rule configuration
          # ignore: fiducial-readiness,tooling-hole-readiness
```

`pc qual test --other` (and the Qualification → Other tab of the PartCAD extension) runs it: every error hyperdrc
finds fails the board, every warning is reported, and the whole report — check, severity, message, layers and
location of each finding — is the test's table. The board is `<name>.kicad_pcb` beside the part unless `pcb:` names
another. Given a board without Gerbers, the checks of a fabrication package's file manifest are left out: they would
only report that a package nobody made holds no copper layer.

## How a part is made

csgrs computes with exact rational and computable real numbers and builds **triangle meshes**, not boundary
representations: a curved surface is tessellated at the resolution its parameters ask for (`segments`,
`samples`, `resolution`, `segments_per_flank`). `csgrs-partcad` converts the exact vertices to binary64 once, welds
them, and sorts the triangles into bodies — a solid per closed piece, a cavity added to the solid around it, a
shell per piece that does not close (`cli/src/mesh.rs`). The script (`csgrs_part.py`) makes each triangle a planar
face with the edges between faces shared, so every body is a valid OCCT solid, then merges coplanar neighbours into
one face (`ShapeUpgrade_UnifySameDomain`), so that a gear's cap is one face and its drawings show outlines.

So a csgrs part booleans, measures, renders and exports like any other, and its STEP is faceted: a cylinder is
the polygonal prism its `segments` say. Where a true curved surface matters downstream, model that part in a
B-rep kernel (CadQuery, build123d); where it does not — a gear for printing, a lattice, a label — csgrs is
exact about the geometry it does make.

## Running in the image

Every part and sketch type has a twin that runs in this package's image instead: `gear_in_container`,
`tpms_in_container`, `csg_profile_in_container` and so on, each with the parameters and the file of the type it is
named after. They use PartCAD's `container:` (0.10.24 for part types, 0.10.25 for sketch types), which runs the
script in the image as it is: nothing is installed on the machine and no Python environment is built, because the
image already carries both programs, OpenCASCADE, numpy and PyYAML.

```yaml
parts:
  pinion:
    type: csgrs:gear_in_container
    teeth: 12
```

The plain types prefer the same image too -- it is the package's `dockerImage` -- but only where PartCAD's `docker`
sandbox is in use, and they fall back to a sandbox of their own and a download of the programs anywhere else. The
`_in_container` types do not fall back: they need a container runtime (Docker), and fail without one, saying so.
Choose them where that is what you want: every part built in one known environment, whatever the machine.

A PartCAD older than 0.10.24 is not shown them, and one older than 0.10.25 is not shown the sketch types at all:
`partcad.yaml` is a template, and asks which PartCAD is reading it (`partcad_version_at_least`), so an older one
lists nothing it would fail to build.

## Where the programs come from

Both programs are native: `csgrs-partcad` is built from [`cli/`](cli) and `hyperdrc` from csgrs's own
`crates/hyperdrc`, at the one csgrs commit [`cli/Cargo.toml`](cli/Cargo.toml) pins. Whichever comes first is used:

1. **The pinned release on the `PATH`.** A machine that installed it, or this package's image. A copy of another
   version is not used: the part cache is keyed by the pin.
2. **A download on first use.** The release pinned in [`csgrs_tool.py`](csgrs_tool.py), verified against its
   published SHA-256 before it is unpacked, kept in this package's cache directory (`.cache/` beside this file),
   and reused from then on.

Releases are built by [`.github/workflows/release.yml`](.github/workflows/release.yml) for Linux (x86_64 and
aarch64), macOS (Apple silicon and Intel) and Windows (x86_64). Elsewhere, build them yourself and put them on the
`PATH`:

```shell
cargo build --release --locked --manifest-path cli/Cargo.toml
cargo install hyperdrc --locked --git https://github.com/timschmidt/csgrs --rev "$(grep -m1 -o 'rev = "[0-9a-f]*"' cli/Cargo.toml | cut -d'"' -f2)"
```

## The image

[`Dockerfile`](Dockerfile) builds `ghcr.io/partcad/partcad-csgrs` `FROM` PartCAD's Python sandbox image, adding
the two programs -- installed by `csgrs_tool.py` itself, so the image and a download are the same build -- and,
for the `_in_container` types, OpenCASCADE (build123d 0.11.1, PartCAD's pin), numpy and PyYAML in the image's own
Python; and
[`verify_image.py`](verify_image.py) refuses, at build time, an image whose programs cannot build a hollow cube.
[`.github/workflows/image.yml`](.github/workflows/image.yml) builds it for `amd64` and `arm64` on every pull
request and publishes it from `main`, tagged with a hash of what it is built from ([`image-tag.sh`](image-tag.sh)):
`csgrs_tool.py` is one of those inputs, so a new release is a new tag. `partcad.yaml` pins that tag as the
package's `dockerImage`, which PartCAD's `docker` sandbox uses where it can pull it, and as the `container:` of every
`_in_container` type, through one YAML anchor.

After changing the `Dockerfile`, `verify_image.py` or `csgrs_tool.py`, run `./image-tag.sh` and put what it prints
into `partcad.yaml` in the same commit; the workflow refuses a pin that does not match.

## Tests

[`tests/`](tests) is a PartCAD package of its own: a workspace that imports this plugin through a `local`
dependency and declares one part per part type, and a board with a via drilled into a pad of another net. The empty
`.pcignore` in it keeps it out of this package's tree.

```shell
cargo test --release --manifest-path cli/Cargo.toml
pip install pytest pyyaml "partcad>=0.10.0"
pytest                 # everything that can run here
pytest -m "not slow"   # the scripts' own logic, and what the programs answer
```

The tests that need `csgrs-partcad`, `hyperdrc` or PartCAD skip themselves without them.

## Releasing

A release is two commits to `main`, so that `main` never pins a release that is not out yet:

1. **The program.** Raise `version` in `cli/Cargo.toml`. The release workflow sees a crate version with no
   release, builds every platform, and publishes `v<version>` -- tag, archives and their `SHA256SUMS.txt`. The
   scripts go on running the previous release meanwhile.
2. **The scripts.** Once it is out, raise `VERSION` in `csgrs_tool.py` to it, put each archive's checksum into
   `ASSETS`, run `./image-tag.sh` and put the tag into `partcad.yaml` (see "The image"), and raise
   `CACHE_VERSION` in `csgrs_part.py` (and `csgrs_sketch.py`) if what a part becomes changes.

### Updating csgrs

Change the `rev` of all three csgrs crates in `cli/Cargo.toml` and `CSGRS_REV` in `cli/src/main.rs` (a test holds
them together), run `cargo update --manifest-path cli/Cargo.toml -p csgrs`, adjust to any API that moved, and
release.

## Licences

This package is Apache-2.0 ([`LICENSE.txt`](LICENSE.txt)). csgrs and hyperdrc are MIT, and `hypersdf`, which the lattices are built with, is
Apache-2.0; they are compiled into the released programs, which carry their licences.
