#
# PartCAD, 2026
#
# Licensed under Apache License, Version 2.0.
#
"""The sketchTypes this package declares: a profile csgrs builds, as a sketch.

One script for all of them, the sketch counterpart of 'csgrs_part.py': it
works out the sketch type from the sketch's 'type:', gathers its parameters
the same way (its own keys, under its declared 'parameters:'), and asks
'csgrs-partcad profile' for the region. What comes back is the region's
boundary as csgrs keeps it -- lines, circular arcs, quadratic and cubic
Béziers, in binary64 -- and it is made into OCCT faces here, one per piece of
material, each with its holes, every curve the curve csgrs had rather than a
polygon standing in for it (see 'cli/src/profile.rs').

Run by PartCAD's sketchType wrapper with run name '__partcad_sketch__',
'request["cache_dir"]' set to where 'csgrs-partcad' is kept, and
'request["path"]' to the sketch's file for a type that reads one.
"""

import json
import math
import os
import sys
import tempfile

# Siblings are imported by bare name, and the sandbox sets PYTHONSAFEPATH.
sys.path.append(os.path.dirname(os.path.abspath(__file__)))

import csgrs_part  # noqa: E402 - after the path that finds it

# Raise with any change to what this produces, which includes moving the
# release 'csgrs_tool.py' pins (see PartCAD's 'code_version').
CACHE_VERSION = 1


def _point(xy):
    from OCP.gp import gp_Pnt

    return gp_Pnt(float(xy[0]), float(xy[1]), 0.0)


def _arc_middle(arc):
    """The point halfway along an arc, which with its ends fixes it."""
    cx, cy = arc["center"]
    sx, sy = arc["start"]
    ex, ey = arc["end"]
    radius = math.hypot(sx - cx, sy - cy)
    a0 = math.atan2(sy - cy, sx - cx)
    a1 = math.atan2(ey - cy, ex - cx)
    sweep = a1 - a0
    if arc["clockwise"]:
        while sweep >= 0:
            sweep -= 2 * math.pi
    else:
        while sweep <= 0:
            sweep += 2 * math.pi
    middle = a0 + sweep / 2
    return [cx + radius * math.cos(middle), cy + radius * math.sin(middle)]


def edges_of(segment):
    """The OCCT edges one segment of a boundary is."""
    from OCP.BRepBuilderAPI import BRepBuilderAPI_MakeEdge
    from OCP.GC import GC_MakeArcOfCircle
    from OCP.Geom import Geom_BezierCurve
    from OCP.TColgp import TColgp_Array1OfPnt

    if "line" in segment:
        start, end = segment["line"]
        return [BRepBuilderAPI_MakeEdge(_point(start), _point(end)).Edge()]
    if "arc" in segment:
        arc = segment["arc"]
        curve = GC_MakeArcOfCircle(_point(arc["start"]), _point(_arc_middle(arc)), _point(arc["end"])).Value()
        return [BRepBuilderAPI_MakeEdge(curve).Edge()]
    if "bezier" in segment:
        poles = TColgp_Array1OfPnt(1, len(segment["bezier"]))
        for index, xy in enumerate(segment["bezier"], start=1):
            poles.SetValue(index, _point(xy))
        return [BRepBuilderAPI_MakeEdge(Geom_BezierCurve(poles)).Edge()]
    if "polyline" in segment:
        points = segment["polyline"]
        return [
            BRepBuilderAPI_MakeEdge(_point(a), _point(b)).Edge()
            for a, b in zip(points, points[1:])
            if (a[0], a[1]) != (b[0], b[1])
        ]
    raise ValueError("csgrs: a boundary segment of a kind this script does not know: %s" % sorted(segment))


def wire_of(segments):
    """One closed boundary as an OCCT wire."""
    from OCP.BRepBuilderAPI import BRepBuilderAPI_MakeWire

    maker = BRepBuilderAPI_MakeWire()
    for segment in segments:
        for edge in edges_of(segment):
            maker.Add(edge)
    if not maker.IsDone():
        raise RuntimeError("csgrs: a boundary of the profile does not join up into a wire")
    return maker.Wire()


def faces_of(profile):
    """The faces of a profile 'csgrs-partcad profile' wrote: each outer boundary with its holes."""
    from OCP.BRepBuilderAPI import BRepBuilderAPI_MakeFace
    from OCP.ShapeFix import ShapeFix_Face

    faces = []
    for face in profile["faces"]:
        maker = BRepBuilderAPI_MakeFace(wire_of(face["outer"]), True)
        for hole in face["holes"]:
            # A hole runs clockwise, which is the way OCCT wants an inner
            # boundary of a face whose normal is +Z.
            maker.Add(wire_of(hole))
        if not maker.IsDone():
            raise RuntimeError("csgrs: OCCT could not make a face of the profile")
        fix = ShapeFix_Face(maker.Face())
        fix.Perform()
        faces.append(fix.Face())
    return faces


def build(request):
    """The sketch 'request' asks for, as a list of OCCT faces."""
    import csgrs_tool

    cli = csgrs_tool.executable("csgrs-partcad", request.get("cache_dir"))
    name = csgrs_part.type_name(request)
    types = json.loads(csgrs_part.run([cli, "describe"]))["sketch_types"]
    if name not in types:
        raise ValueError("csgrs builds no sketch type '%s'; it builds %s" % (name, ", ".join(sorted(types))))
    asked = csgrs_part.build_request(name, types[name]["parameters"], request, tree_type="csg_profile")

    with tempfile.TemporaryDirectory(prefix="partcad-csgrs-") as tmp:
        request_file = os.path.join(tmp, "request.json")
        profile_file = os.path.join(tmp, "profile.json")
        with open(request_file, "w", encoding="utf-8") as f:
            json.dump(asked, f)
        report = json.loads(csgrs_part.run([cli, "profile", request_file, profile_file]))
        with open(profile_file, encoding="utf-8") as f:
            profile = json.load(f)

    for warning in report.get("warnings") or []:
        print("csgrs: %s" % warning, file=sys.stderr)
    return faces_of(profile)


if __name__ == "__partcad_sketch__":
    try:
        faces = build(request)  # noqa: F821 - injected by the sketchType wrapper
        output = {"shape": faces[0]} if len(faces) == 1 else {"shapes": faces}
    except Exception as e:
        output = {"exception": str(e)}
