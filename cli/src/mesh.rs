//! A csgrs mesh as what PartCAD builds a part from: floating-point vertices,
//! welded, and its triangles sorted into bodies.
//!
//! A part is bodies, and a triangle mesh states only triangles. The bodies are
//! found here rather than on PartCAD's side because here the vertices are still
//! the ones csgrs computed, so two triangles that share a corner share it
//! exactly:
//!
//! * a **solid** is a closed piece whose triangles face out (positive volume);
//! * a **cavity** is a closed piece that faces in (negative volume): the
//!   inside wall of a hollow, which belongs to the solid around it, its
//!   `parent`;
//! * a **shell** is a piece that does not close -- a surface, not a body.
//!
//! The file written (`write`) is, little-endian:
//!
//! ```text
//! b"CSGRSPC1"  u64 vertex count  u64 triangle count
//! f64 x, y, z per vertex
//! u32 a, b, c per triangle, sorted by body
//! ```
//!
//! and the bodies, which triangles each one is, go to the JSON report.

use std::collections::HashMap;
use std::io::Write;

use csgrs::TriangleMesh;
use serde_json::{Value, json};

use crate::fail::Fail;

pub const MAGIC: &[u8; 8] = b"CSGRSPC1";

pub struct Body {
    pub kind: &'static str,
    pub first: usize,
    pub count: usize,
    pub volume: f64,
    pub parent: Option<usize>,
}

pub struct Mesh {
    pub positions: Vec<[f64; 3]>,
    pub triangles: Vec<[u32; 3]>,
    pub bodies: Vec<Body>,
}

fn find(parent: &mut [usize], mut i: usize) -> usize {
    while parent[i] != i {
        parent[i] = parent[parent[i]];
        i = parent[i];
    }
    i
}

fn signed_volume(positions: &[[f64; 3]], triangles: &[[u32; 3]]) -> f64 {
    triangles
        .iter()
        .map(|t| {
            let [a, b, c] = t.map(|i| positions[i as usize]);
            a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
                + a[2] * (b[0] * c[1] - b[1] * c[0])
        })
        .sum::<f64>()
        / 6.0
}

/// Whether every edge of `triangles` is met once in each direction: a closed,
/// consistently oriented surface.
fn closes(triangles: &[[u32; 3]]) -> bool {
    let mut edges: HashMap<(u32, u32), i32> = HashMap::with_capacity(triangles.len() * 3);
    for t in triangles {
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            *edges.entry((a, b)).or_insert(0) += 1;
        }
    }
    edges
        .iter()
        .all(|(&(a, b), &n)| n == 1 && edges.get(&(b, a)) == Some(&1))
}

/// Whether `point` is inside the closed surface `triangles`: the parity of the
/// crossings of a ray. The direction is an odd one, so that a ray along an
/// axis-aligned edge -- common in modelled parts -- is unlikely.
fn inside(positions: &[[f64; 3]], triangles: &[[u32; 3]], point: [f64; 3]) -> bool {
    let d = [0.531_7, 0.611_3, 0.586_9];
    let mut crossings = 0;
    for t in triangles {
        let [a, b, c] = t.map(|i| positions[i as usize]);
        let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let e2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let p = [
            d[1] * e2[2] - d[2] * e2[1],
            d[2] * e2[0] - d[0] * e2[2],
            d[0] * e2[1] - d[1] * e2[0],
        ];
        let det = e1[0] * p[0] + e1[1] * p[1] + e1[2] * p[2];
        if det.abs() < 1e-300 {
            continue;
        }
        let s = [point[0] - a[0], point[1] - a[1], point[2] - a[2]];
        let u = (s[0] * p[0] + s[1] * p[1] + s[2] * p[2]) / det;
        if !(0.0..=1.0).contains(&u) {
            continue;
        }
        let q = [
            s[1] * e1[2] - s[2] * e1[1],
            s[2] * e1[0] - s[0] * e1[2],
            s[0] * e1[1] - s[1] * e1[0],
        ];
        let v = (d[0] * q[0] + d[1] * q[1] + d[2] * q[2]) / det;
        if v < 0.0 || u + v > 1.0 {
            continue;
        }
        let distance = (e2[0] * q[0] + e2[1] * q[1] + e2[2] * q[2]) / det;
        if distance > 0.0 {
            crossings += 1;
        }
    }
    crossings % 2 == 1
}

impl Mesh {
    pub fn from_csgrs(mesh: &TriangleMesh) -> Result<Self, Fail> {
        // Welded by value: csgrs shares a vertex between the triangles that
        // meet at it, but a mesh assembled from several (a merge, a file)
        // may repeat one.
        let mut index: HashMap<[u64; 3], u32> = HashMap::with_capacity(mesh.positions.len());
        let mut positions = Vec::with_capacity(mesh.positions.len());
        let mut remap = Vec::with_capacity(mesh.positions.len());
        for point in mesh.positions.iter() {
            let coordinate = |v: &csgrs::Real| {
                v.to_f64_lossy()
                    .filter(|f| f.is_finite())
                    .ok_or_else(|| Fail::input("a vertex is not a finite number"))
            };
            // +0.0 and -0.0 are one place.
            let p = [
                coordinate(&point.x)? + 0.0,
                coordinate(&point.y)? + 0.0,
                coordinate(&point.z)? + 0.0,
            ];
            let key = p.map(f64::to_bits);
            let next = positions.len() as u32;
            let at = *index.entry(key).or_insert_with(|| {
                positions.push(p);
                next
            });
            remap.push(at);
        }
        let mut triangles: Vec<[u32; 3]> = Vec::with_capacity(mesh.triangles.len());
        for t in mesh.triangles.iter() {
            let tri = [remap[t.v0], remap[t.v1], remap[t.v2]];
            // A triangle two of whose corners became one place bounds nothing.
            if tri[0] != tri[1] && tri[1] != tri[2] && tri[0] != tri[2] {
                triangles.push(tri);
            }
        }
        Ok(Self::sorted(positions, triangles))
    }

    /// The triangles sorted into bodies.
    fn sorted(positions: Vec<[f64; 3]>, triangles: Vec<[u32; 3]>) -> Self {
        let mut parent: Vec<usize> = (0..positions.len()).collect();
        for t in &triangles {
            let a = find(&mut parent, t[0] as usize);
            for &v in &t[1..] {
                let b = find(&mut parent, v as usize);
                parent[b] = a;
            }
        }
        let mut groups: HashMap<usize, Vec<[u32; 3]>> = HashMap::new();
        let mut order = Vec::new();
        for t in &triangles {
            let root = find(&mut parent, t[0] as usize);
            groups
                .entry(root)
                .or_insert_with(|| {
                    order.push(root);
                    Vec::new()
                })
                .push(*t);
        }
        // Largest first: the order bodies are reported in, and the order a
        // cavity looks for its solid in.
        let mut pieces: Vec<Vec<[u32; 3]>> = order
            .into_iter()
            .map(|root| groups.remove(&root).unwrap())
            .collect();
        pieces.sort_by(|a, b| {
            signed_volume(&positions, b)
                .abs()
                .total_cmp(&signed_volume(&positions, a).abs())
                .then(b.len().cmp(&a.len()))
        });

        let mut bodies = Vec::with_capacity(pieces.len());
        let mut sorted = Vec::with_capacity(triangles.len());
        for piece in &pieces {
            let volume = signed_volume(&positions, piece);
            let kind = if !closes(piece) {
                "shell"
            } else if volume < 0.0 {
                "cavity"
            } else {
                "solid"
            };
            bodies.push(Body {
                kind,
                first: sorted.len(),
                count: piece.len(),
                volume,
                parent: None,
            });
            sorted.extend_from_slice(piece);
        }
        // A cavity belongs to the smallest solid around it.
        for i in 0..bodies.len() {
            if bodies[i].kind != "cavity" {
                continue;
            }
            let probe = positions[pieces[i][0][0] as usize];
            let owner = (0..bodies.len())
                .filter(|&j| bodies[j].kind == "solid" && inside(&positions, &pieces[j], probe))
                .min_by(|&a, &b| bodies[a].volume.total_cmp(&bodies[b].volume));
            bodies[i].parent = owner;
            if owner.is_none() {
                // Inside nothing: a solid turned inside out, which is a
                // surface rather than a body.
                bodies[i].kind = "shell";
            }
        }
        Self {
            positions,
            triangles: sorted,
            bodies,
        }
    }

    pub fn write(&self, out: &mut impl Write) -> std::io::Result<()> {
        out.write_all(MAGIC)?;
        out.write_all(&(self.positions.len() as u64).to_le_bytes())?;
        out.write_all(&(self.triangles.len() as u64).to_le_bytes())?;
        for p in &self.positions {
            for c in p {
                out.write_all(&c.to_le_bytes())?;
            }
        }
        for t in &self.triangles {
            for i in t {
                out.write_all(&i.to_le_bytes())?;
            }
        }
        Ok(())
    }

    pub fn report(&self) -> Value {
        let bodies: Vec<Value> = self
            .bodies
            .iter()
            .map(|b| {
                json!({
                    "kind": b.kind,
                    "first": b.first,
                    "count": b.count,
                    "volume": b.volume,
                    "parent": b.parent,
                })
            })
            .collect();
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        for p in &self.positions {
            for i in 0..3 {
                lo[i] = lo[i].min(p[i]);
                hi[i] = hi[i].max(p[i]);
            }
        }
        json!({
            "vertices": self.positions.len(),
            "triangles": self.triangles.len(),
            "bodies": bodies,
            "bbox": if self.positions.is_empty() { Value::Null } else { json!([lo[0], lo[1], lo[2], hi[0], hi[1], hi[2]]) },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn built(tree: serde_json::Value) -> Mesh {
        let m = csgrs::Csg::strict();
        let mesh = crate::csg::Tree {
            m: &m,
            base: None,
            warnings: Vec::new(),
        }
        .solid(&tree)
        .expect("the tree builds");
        Mesh::from_csgrs(&mesh).expect("the mesh converts")
    }

    #[test]
    fn a_cube_is_one_closed_solid() {
        let mesh = built(json!({"cube": 2}));
        assert_eq!(mesh.bodies.len(), 1);
        assert_eq!(mesh.bodies[0].kind, "solid");
        assert!((mesh.bodies[0].volume - 8.0).abs() < 1e-12);
        assert_eq!(mesh.positions.len(), 8);
    }

    #[test]
    fn a_hollow_is_a_cavity_of_the_solid_around_it() {
        let mesh =
            built(json!({"difference": [{"cube": 10}, {"cube": 4, "translate": [3, 3, 3]}]}));
        let kinds: Vec<(&str, Option<usize>)> =
            mesh.bodies.iter().map(|b| (b.kind, b.parent)).collect();
        assert_eq!(kinds, vec![("solid", None), ("cavity", Some(0))]);
    }

    #[test]
    fn separate_pieces_are_separate_bodies_largest_first() {
        let mesh = built(json!({"union": [{"cube": 1}, {"cube": 2, "translate": [5, 0, 0]}]}));
        let volumes: Vec<f64> = mesh.bodies.iter().map(|b| b.volume).collect();
        assert_eq!(volumes.len(), 2);
        assert!((volumes[0] - 8.0).abs() < 1e-12 && (volumes[1] - 1.0).abs() < 1e-12);
        assert_eq!(mesh.bodies[1].first, mesh.bodies[0].count);
    }

    #[test]
    fn the_file_is_what_the_header_says() {
        let mesh = built(json!({"cube": 1}));
        let mut bytes = Vec::new();
        mesh.write(&mut bytes).unwrap();
        assert_eq!(&bytes[..8], MAGIC);
        assert_eq!(
            bytes.len(),
            24 + mesh.positions.len() * 24 + mesh.triangles.len() * 12
        );
    }
}
