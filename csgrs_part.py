#
# PartCAD, 2026
#
# Licensed under Apache License, Version 2.0.
#
"""The partTypes this package declares: a part csgrs builds.

One script for all of them. It works out the part type from the part's
'type:' (the name after the last ':'), gathers the part's parameters, hands
them to 'csgrs-partcad' (see 'csgrs_tool.py' for where that comes from), and
makes the mesh it writes into the OCCT solid PartCAD keeps.

**Parameters.** A part type's parameters are named in 'csgrs-partcad
describe'. A part sets them either as keys of its own declaration --

    parts:
      pinion:
        type: csgrs:gear
        teeth: 12

-- or as declared 'parameters:', which is what makes them settable from
outside ('pinion;teeth=14'); a declared parameter wins over a key of the same
name. A declared parameter that is not one of the type's is an error rather
than something silently left out, except on a 'csg' part, whose tree may use
any parameter it likes.

**A 'csg' part** reads a tree of shapes from its file ('<name>.csgrs', YAML or
JSON; see README.md). A string of the form '${expression}' anywhere in it is
replaced by the expression's value, worked out over the part's parameters with
the arithmetic, comparisons, 'x if c else y' and the functions in FUNCTIONS
below -- nothing else of Python is reachable from it. '${...}' inside a
longer string ('text: "M${size}"') is replaced by the value's text.

**From mesh to part.** csgrs computes exact coordinates; the tool welds them
as binary64 and sorts the triangles into bodies (see 'cli/src/mesh.rs'). Each
triangle becomes a planar face, with the edges between faces shared, and each
body the solid -- or, if it does not close, the shell -- its faces bound, a
cavity added to the solid around it. Coplanar neighbours are then merged into
one face (OCCT's 'ShapeUpgrade_UnifySameDomain'), which is what makes a gear's
cap one face rather than a thousand triangles and its drawings show outlines
rather than a triangulation. A TPMS lattice has no two faces in one plane, so
that step is skipped for it.

Run by PartCAD's partType wrapper with 'request["cache_dir"]' set to where
'csgrs-partcad' is kept, and 'request["path"]' to the part's file for a type
that reads one.
"""

import ast
import json
import math
import os
import re
import subprocess
import sys
import tempfile

# Siblings are imported by bare name, and the sandbox sets PYTHONSAFEPATH.
sys.path.append(os.path.dirname(os.path.abspath(__file__)))

# Raise with any change to what this produces, which includes moving the
# release 'csgrs_tool.py' pins (see PartCAD's 'code_version').
CACHE_VERSION = 1

# The part types whose surface has no two neighbouring faces in one plane,
# for which merging coplanar faces is time spent finding none.
NOTHING_TO_MERGE = ("tpms",)

# The functions an expression in a '.csgrs' file may call.
FUNCTIONS = {
    "abs": abs,
    "min": min,
    "max": max,
    "round": round,
    "int": int,
    "float": float,
    "str": str,
    "floor": math.floor,
    "ceil": math.ceil,
    "sqrt": math.sqrt,
    "sin": lambda degrees: math.sin(math.radians(degrees)),
    "cos": lambda degrees: math.cos(math.radians(degrees)),
    "tan": lambda degrees: math.tan(math.radians(degrees)),
    "asin": lambda x: math.degrees(math.asin(x)),
    "acos": lambda x: math.degrees(math.acos(x)),
    "atan": lambda x: math.degrees(math.atan(x)),
    "atan2": lambda y, x: math.degrees(math.atan2(y, x)),
    "hypot": math.hypot,
    "log": math.log,
    "exp": math.exp,
}
CONSTANTS = {"pi": math.pi, "true": True, "false": False}

EXPRESSION = re.compile(r"\$\{([^}]*)\}")


class ExpressionError(ValueError):
    pass


def evaluate(expression, names):
    """The value of 'expression' (Python's own syntax, a small part of it) over 'names'."""
    try:
        tree = ast.parse(expression.strip(), mode="eval")
    except SyntaxError as e:
        raise ExpressionError("'%s' is not an expression: %s" % (expression, e.msg)) from None

    def value(node):
        if isinstance(node, ast.Expression):
            return value(node.body)
        if isinstance(node, ast.Constant) and isinstance(node.value, (int, float, str, bool)):
            return node.value
        if isinstance(node, ast.Name):
            if node.id in names:
                return names[node.id]
            if node.id in CONSTANTS:
                return CONSTANTS[node.id]
            raise ExpressionError(
                "'%s' names '%s', which is no parameter of the part (it has %s)"
                % (expression, node.id, ", ".join(sorted(names)) or "none")
            )
        if isinstance(node, ast.UnaryOp) and isinstance(node.op, (ast.USub, ast.UAdd, ast.Not)):
            operand = value(node.operand)
            return {ast.USub: lambda: -operand, ast.UAdd: lambda: +operand, ast.Not: lambda: not operand}[
                type(node.op)
            ]()
        if isinstance(node, ast.BinOp):
            operators = {
                ast.Add: lambda a, b: a + b,
                ast.Sub: lambda a, b: a - b,
                ast.Mult: lambda a, b: a * b,
                ast.Div: lambda a, b: a / b,
                ast.FloorDiv: lambda a, b: a // b,
                ast.Mod: lambda a, b: a % b,
                ast.Pow: lambda a, b: a**b,
            }
            if type(node.op) not in operators:
                raise ExpressionError("'%s' uses an operator expressions here do not have" % expression)
            return operators[type(node.op)](value(node.left), value(node.right))
        if isinstance(node, ast.BoolOp):
            values = [value(v) for v in node.values]
            return all(values) if isinstance(node.op, ast.And) else any(values)
        if isinstance(node, ast.Compare):
            comparisons = {
                ast.Lt: lambda a, b: a < b,
                ast.LtE: lambda a, b: a <= b,
                ast.Gt: lambda a, b: a > b,
                ast.GtE: lambda a, b: a >= b,
                ast.Eq: lambda a, b: a == b,
                ast.NotEq: lambda a, b: a != b,
            }
            left = value(node.left)
            for op, right_node in zip(node.ops, node.comparators):
                if type(op) not in comparisons:
                    raise ExpressionError("'%s' uses a comparison expressions here do not have" % expression)
                right = value(right_node)
                if not comparisons[type(op)](left, right):
                    return False
                left = right
            return True
        if isinstance(node, ast.IfExp):
            return value(node.body) if value(node.test) else value(node.orelse)
        if isinstance(node, ast.Call) and isinstance(node.func, ast.Name) and not node.keywords:
            if node.func.id not in FUNCTIONS:
                raise ExpressionError(
                    "'%s' calls '%s'; the functions are %s" % (expression, node.func.id, ", ".join(sorted(FUNCTIONS)))
                )
            return FUNCTIONS[node.func.id](*[value(a) for a in node.args])
        raise ExpressionError("'%s' is more than an expression here may be" % expression)

    try:
        return value(tree)
    except ExpressionError:
        raise
    except Exception as e:  # a division by zero, a domain error
        raise ExpressionError("'%s': %s" % (expression, e)) from None


def substitute(tree, names):
    """'tree' with every '${...}' replaced by its value over 'names'."""
    if isinstance(tree, dict):
        return {key: substitute(item, names) for key, item in tree.items()}
    if isinstance(tree, list):
        return [substitute(item, names) for item in tree]
    if isinstance(tree, str):
        whole = EXPRESSION.fullmatch(tree.strip())
        if whole:
            return evaluate(whole.group(1), names)
        return EXPRESSION.sub(lambda m: str(evaluate(m.group(1), names)), tree)
    return tree


def read_tree(path):
    """The tree in a '.csgrs' file: YAML, of which JSON is a part."""
    with open(path, encoding="utf-8") as f:
        text = f.read()
    try:
        import yaml
    except ImportError:  # the partType declares PyYAML; a bare test run may lack it
        return json.loads(text)
    return yaml.safe_load(text)


# --------------------------------------------------------------------------- #
# The request                                                                 #
# --------------------------------------------------------------------------- #


def type_name(request):
    """The part type this part is: the name after the last ':' of its 'type:'."""
    declared = str((request.get("config") or {}).get("type") or "")
    return declared.rsplit(":", 1)[-1]


def parameters(name, spec, request):
    """The values the part sets for the parameters 'spec' names, its declared parameters over its own keys."""
    config = request.get("config") or {}
    declared = request.get("parameters") or {}
    names = [p["name"] for p in spec]
    values = {key: config[key] for key in names if key in config and config[key] is not None}
    unknown = sorted(set(declared) - set(names))
    if unknown and name != "csg":
        raise ValueError(
            "a '%s' part has no parameter %s; it has %s"
            % (name, ", ".join("'%s'" % u for u in unknown), ", ".join(names) or "none")
        )
    values.update({key: value for key, value in declared.items() if key in names and value is not None})
    return values


def build_request(name, spec, request):
    """What 'csgrs-partcad build' is asked."""
    asked = {"type": name, "parameters": parameters(name, spec, request)}
    config = request.get("config") or {}
    policy = config.get("policy")
    if policy is not None:
        asked["policy"] = policy
    if request.get("path"):
        asked["path"] = os.path.abspath(request["path"])
        asked["base"] = os.path.dirname(asked["path"])
    if name == "csg":
        if not request.get("path"):
            raise ValueError("a 'csg' part reads its tree from a file, and PartCAD handed it none")
        names = dict(request.get("parameters") or {})
        asked["tree"] = substitute(read_tree(request["path"]), names)
    return asked


# --------------------------------------------------------------------------- #
# The tool                                                                    #
# --------------------------------------------------------------------------- #


def run(command):
    """Run 'csgrs-partcad', returning what it printed and raising with what it said when it fails."""
    result = subprocess.run(command, capture_output=True, text=True, check=False)
    if result.returncode != 0:
        detail = (result.stderr or result.stdout or "").strip().splitlines()
        raise RuntimeError("csgrs: %s" % (detail[-1] if detail else "exit %d" % result.returncode))
    return result.stdout


def describe(cli):
    """Every part type 'csgrs-partcad' builds, and its parameters."""
    return json.loads(run([cli, "describe"]))["types"]


def read_mesh(path):
    """'(points, triangles)' of a mesh 'csgrs-partcad build' wrote: float64 (n, 3) and uint32 (m, 3)."""
    import numpy as np

    with open(path, "rb") as f:
        data = f.read()
    if data[:8] != b"CSGRSPC1":
        raise RuntimeError("csgrs: %s is not a mesh csgrs-partcad wrote" % path)
    vertices, triangles = (int(n) for n in np.frombuffer(data, dtype="<u8", count=2, offset=8))
    points_end = 24 + vertices * 24
    size = points_end + triangles * 12
    if size != len(data):
        raise RuntimeError("csgrs: %s is %d bytes, not the %d its header says" % (path, len(data), size))
    points = np.frombuffer(data, dtype="<f8", count=vertices * 3, offset=24).reshape(-1, 3)
    faces = np.frombuffer(data, dtype="<u4", count=triangles * 3, offset=points_end).reshape(-1, 3)
    if len(faces) and int(faces.max()) >= vertices:
        raise RuntimeError("csgrs: %s names a vertex it does not hold" % path)
    return points, faces


# --------------------------------------------------------------------------- #
# The mesh as a part                                                          #
# --------------------------------------------------------------------------- #


def faces_on(points, triangles):
    """A planar face per triangle, in order, with shared edges: OCCT's 'BRepBuilderAPI_MakeShapeOnMesh'."""
    from OCP.BRepBuilderAPI import BRepBuilderAPI_MakeShapeOnMesh
    from OCP.gp import gp_Pnt
    from OCP.Poly import Poly_Triangle, Poly_Triangulation
    from OCP.TopAbs import TopAbs_FACE
    from OCP.TopExp import TopExp
    from OCP.TopTools import TopTools_IndexedMapOfShape

    mesh = Poly_Triangulation(len(points), len(triangles), False)
    for index, (x, y, z) in enumerate(points.tolist(), start=1):
        mesh.SetNode(index, gp_Pnt(x, y, z))
    for index, (a, b, c) in enumerate(triangles.tolist(), start=1):
        mesh.SetTriangle(index, Poly_Triangle(a + 1, b + 1, c + 1))  # OCCT counts from 1
    maker = BRepBuilderAPI_MakeShapeOnMesh(mesh)
    maker.Build()
    if not maker.IsDone():
        raise RuntimeError("OCCT could not make faces of the mesh")
    found = TopTools_IndexedMapOfShape()
    TopExp.MapShapes_s(maker.Shape(), TopAbs_FACE, found)
    if found.Extent() != len(triangles):
        raise RuntimeError("OCCT made %d faces of %d triangles" % (found.Extent(), len(triangles)))
    return [found.FindKey(index) for index in range(1, found.Extent() + 1)]


def merged(shape):
    """'shape' with coplanar neighbouring faces made one, and collinear edges likewise."""
    from OCP.ShapeUpgrade import ShapeUpgrade_UnifySameDomain

    unify = ShapeUpgrade_UnifySameDomain(shape, True, True, False)
    unify.Build()
    result = unify.Shape()
    return shape if result is None or result.IsNull() else result


def shapes_of(points, triangles, bodies, merge=True):
    """The bodies of a mesh, as the solids and shells PartCAD keeps."""
    from OCP.BRep import BRep_Builder
    from OCP.TopoDS import TopoDS_Shell, TopoDS_Solid

    builder = BRep_Builder()
    faces = faces_on(points, triangles)
    shells = []
    for body in bodies:
        shell = TopoDS_Shell()
        builder.MakeShell(shell)
        for face in faces[body["first"] : body["first"] + body["count"]]:
            builder.Add(shell, face)
        if body["kind"] != "shell":
            shell.Closed(True)
        shells.append(shell)

    solids = {}
    for index, body in enumerate(bodies):
        if body["kind"] == "solid":
            solid = TopoDS_Solid()
            builder.MakeSolid(solid)
            builder.Add(solid, shells[index])
            solids[index] = solid
    for index, body in enumerate(bodies):
        # The inside wall of a hollow, already facing into it.
        if body["kind"] == "cavity":
            builder.Add(solids[body["parent"]], shells[index])

    result = [solids[i] for i in sorted(solids)]
    result += [shells[i] for i, body in enumerate(bodies) if body["kind"] == "shell"]
    if merge:
        result = [merged(shape) for shape in result]
    return result


def build(request):
    """The part 'request' asks for, as a list of OCCT shapes."""
    import csgrs_tool

    cli = csgrs_tool.executable("csgrs-partcad", request.get("cache_dir"))
    name = type_name(request)
    types = describe(cli)
    if name not in types:
        raise ValueError("csgrs builds no part type '%s'; it builds %s" % (name, ", ".join(sorted(types))))
    asked = build_request(name, types[name]["parameters"], request)

    with tempfile.TemporaryDirectory(prefix="partcad-csgrs-") as tmp:
        request_file = os.path.join(tmp, "request.json")
        mesh_file = os.path.join(tmp, "part.mesh")
        with open(request_file, "w", encoding="utf-8") as f:
            json.dump(asked, f)
        report = json.loads(run([cli, "build", request_file, mesh_file]))
        points, triangles = read_mesh(mesh_file)

    for warning in report.get("warnings") or []:
        print("csgrs: %s" % warning, file=sys.stderr)
    return shapes_of(points, triangles, report["bodies"], merge=name not in NOTHING_TO_MERGE)


if __name__ == "__partcad_part__":
    try:
        shapes = build(request)  # noqa: F821 - injected by the partType wrapper
        output = {"shape": shapes[0]} if len(shapes) == 1 else {"shapes": shapes}
    except Exception as e:
        output = {"exception": str(e)}
