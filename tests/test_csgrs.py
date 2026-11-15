#
# PartCAD, 2026
#
# Licensed under Apache License, Version 2.0.
#
"""The csgrs plugin, as a workspace uses it.

Three kinds of test, and the split is what a contributor can run where:

* The scripts' own logic -- which parameters a part hands csgrs, what an
  expression in a '.csgrs' file is worth, which release is fetched for which
  machine, that a download whose checksum does not match is refused -- needs
  nothing but the standard library and pytest. It is checked offline.
* What 'csgrs-partcad' itself answers needs the program: on the PATH, or built
  from 'cli/' (`cargo build --release`). Those tests skip without it.
* Everything else needs PartCAD: the package beside this file imports the
  plugin and declares one part per part type. Those tests are skipped where
  PartCAD is not installed, and the ones that build a part are marked 'slow':
  the first of them builds PartCAD's sandbox.

Run them with `pytest`; `pytest -m "not slow"` for the quick ones only.
"""

import asyncio
import hashlib
import importlib.util
import io
import json
import os
import re
import shutil
import subprocess
import sys
import tarfile

import pytest

HERE = os.path.dirname(os.path.abspath(__file__))
PLUGIN = os.path.dirname(HERE)


def _module(name):
    spec = importlib.util.spec_from_file_location("csgrs_plugin_" + name, os.path.join(PLUGIN, name + ".py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


@pytest.fixture
def part():
    return _module("csgrs_part")


@pytest.fixture
def tool():
    return _module("csgrs_tool")


def _yaml(path):
    yaml = pytest.importorskip("yaml")
    with open(path, encoding="utf-8") as f:
        return yaml.safe_load(f)


# --------------------------------------------------------------------------- #
# The scripts, without PartCAD                                                #
# --------------------------------------------------------------------------- #

GEAR_SPEC = [{"name": n} for n in ("module", "teeth", "thickness", "profile")]


def test_a_part_type_is_the_name_after_the_last_colon(part):
    assert part.type_name({"config": {"type": "//pub/feature/cad/csgrs:gear"}}) == "gear"
    assert part.type_name({"config": {"type": "csgrs:tpms"}}) == "tpms"


def test_declared_parameters_win_over_the_part_s_own_keys(part):
    request = {
        "config": {"type": "csgrs:gear", "teeth": 12, "module": 1.5, "desc": "not a parameter", "name": "pinion"},
        "parameters": {"teeth": 30},
    }
    assert part.parameters("gear", GEAR_SPEC, request) == {"teeth": 30, "module": 1.5}


def test_a_parameter_the_type_does_not_have_is_an_error(part):
    request = {"config": {"type": "csgrs:gear"}, "parameters": {"teth": 30}}
    with pytest.raises(ValueError, match="no parameter 'teth'"):
        part.parameters("gear", GEAR_SPEC, request)


def test_a_csg_part_may_declare_parameters_for_its_tree(part):
    request = {"config": {"type": "csgrs:csg"}, "parameters": {"width": 40}}
    assert part.parameters("csg", [], request) == {}


def test_expressions_are_worked_out_over_the_parameters(part):
    tree = {
        "difference": [
            {"cuboid": ["${width}", 20, "${height * 2}"]},
            {"cylinder": {"r": "${hole / 2}", "h": 10}, "translate": ["${width / 2}", 10, -1]},
            {"text": {"text": "W${int(width)}", "size": "${max(3, height)}"}},
        ]
    }
    assert part.substitute(tree, {"width": 40.0, "height": 2.5, "hole": 6}) == {
        "difference": [
            {"cuboid": [40.0, 20, 5.0]},
            {"cylinder": {"r": 3.0, "h": 10}, "translate": [20.0, 10, -1]},
            {"text": {"text": "W40", "size": 3}},
        ]
    }


@pytest.mark.parametrize(
    "expression, value",
    [
        ("sin(30)", 0.5),
        ("atan2(1, 1)", 45.0),
        ("2 ** 3 + 1", 9),
        ("10 if n > 3 else 20", 10),
        ("pi", 3.141592653589793),
        ("-n % 3", 2),
    ],
)
def test_an_expression_has_arithmetic_and_degrees(part, expression, value):
    assert part.evaluate(expression, {"n": 4}) == pytest.approx(value)


@pytest.mark.parametrize(
    "expression, complaint",
    [
        ("__import__('os')", "calls '__import__'"),
        ("n.real", "more than an expression"),
        ("open('x')", "calls 'open'"),
        ("[1, 2]", "more than an expression"),
        ("missing + 1", "names 'missing', which is no parameter"),
        ("1 / 0", "division by zero"),
        ("1 +", "not an expression"),
    ],
)
def test_an_expression_reaches_nothing_but_arithmetic(part, expression, complaint):
    with pytest.raises(part.ExpressionError, match=re.escape(complaint)):
        part.evaluate(expression, {"n": 4})


def test_a_csg_request_carries_its_tree(part, tmp_path):
    source = tmp_path / "plate.csgrs"
    source.write_text('{"cuboid": ["${w}", 2, 3]}', encoding="utf-8")
    request = {"config": {"type": "csgrs:csg"}, "parameters": {"w": 7}, "path": str(source)}
    asked = part.build_request("csg", [], request)
    assert asked["tree"] == {"cuboid": [7, 2, 3]}
    assert asked["base"] == str(tmp_path)


def test_a_mesh_file_is_read_as_written(part, tmp_path):
    np = pytest.importorskip("numpy")
    points = np.array([[0, 0, 0], [1, 0, 0], [0, 1, 0], [0, 0, 1]], dtype="<f8")
    triangles = np.array([[0, 2, 1], [0, 1, 3], [1, 2, 3], [0, 3, 2]], dtype="<u4")
    path = tmp_path / "part.mesh"
    path.write_bytes(
        b"CSGRSPC1"
        + np.array([len(points), len(triangles)], dtype="<u8").tobytes()
        + points.tobytes()
        + triangles.tobytes()
    )
    read_points, read_triangles = part.read_mesh(str(path))
    assert (read_points == points).all() and (read_triangles == triangles).all()

    path.write_bytes(path.read_bytes()[:-4])
    with pytest.raises(RuntimeError, match="not the"):
        part.read_mesh(str(path))


def test_every_platform_names_an_asset_of_the_pinned_release(tool):
    for (system, machine), (asset, sha256) in tool.ASSETS.items():
        assert tool.VERSION in asset, (system, machine)
        assert re.fullmatch(r"[0-9a-f]{64}", sha256), (system, machine)


def _crate_version():
    with open(os.path.join(PLUGIN, "cli", "Cargo.toml"), encoding="utf-8") as f:
        return re.search(r'^version = "([^"]+)"$', f.read(), re.M).group(1)


def _version(text):
    return tuple(int(part) for part in text.split("."))


def test_the_tool_pins_a_release_of_the_crate(tool):
    """The scripts run a release: the crate's own version, or one before it while that one is being released.

    A new version of the program is released from main before the scripts move
    to it (see README.md, "Releasing"), so for one commit the crate is ahead.
    """
    assert _version(tool.VERSION) <= _version(_crate_version())
    assert tool.PROGRAMS["csgrs-partcad"] == tool.VERSION


def test_one_csgrs_commit_everywhere():
    """The commit csgrs is pinned at: in every dependency of the crate, and in what the tool reports."""
    with open(os.path.join(PLUGIN, "cli", "Cargo.toml"), encoding="utf-8") as f:
        revs = set(re.findall(r'rev = "([0-9a-f]{40})"', f.read()))
    assert len(revs) == 1
    with open(os.path.join(PLUGIN, "cli", "src", "main.rs"), encoding="utf-8") as f:
        assert 'CSGRS_REV: &str = "%s"' % revs.pop() in f.read()


def test_the_image_pinned_is_the_one_its_inputs_make():
    """'dockerImage' names the tag 'image-tag.sh' computes from the Dockerfile and what it installs."""
    if not shutil.which("sh") or not (shutil.which("sha256sum") or shutil.which("shasum")):
        pytest.skip("no sh and sha256sum here to compute the tag with")
    tag = subprocess.run(["sh", os.path.join(PLUGIN, "image-tag.sh")], capture_output=True, text=True, check=True)
    assert _yaml(os.path.join(PLUGIN, "partcad.yaml"))["dockerImage"] == (
        "ghcr.io/partcad/partcad-csgrs:" + tag.stdout.strip()
    )


def test_the_image_installs_what_a_download_would():
    """The image runs this very file to install the programs, rather than pinning a release of its own."""
    with open(os.path.join(PLUGIN, "Dockerfile"), encoding="utf-8") as f:
        dockerfile = f.read()
    assert "RUN python3 /tmp/csgrs_tool.py /usr/local/bin" in dockerfile


def test_one_on_the_path_is_used_if_it_is_the_pinned_release(tool, monkeypatch, tmp_path):
    monkeypatch.setattr(tool.shutil, "which", lambda name: "/opt/csgrs/bin/" + name)
    monkeypatch.setattr(tool, "version_of", lambda path, name: tool.PROGRAMS[name])
    assert tool.executable("csgrs-partcad", str(tmp_path)) == "/opt/csgrs/bin/csgrs-partcad"
    assert list(tmp_path.iterdir()) == []


def _serve(tool, monkeypatch, archive: bytes, sha256: str):
    """Make the pinned release for this machine be 'archive', checked against 'sha256'."""
    monkeypatch.setattr(tool.shutil, "which", lambda _name: None)
    monkeypatch.setattr(tool, "host", lambda: ("linux", "x86_64"))
    monkeypatch.setitem(tool.ASSETS, ("linux", "x86_64"), ("csgrs-partcad-test.tar.gz", sha256))
    fetched = []

    def urlopen(url, timeout=None):
        fetched.append(url)
        return io.BytesIO(archive)

    monkeypatch.setattr(tool.urllib.request, "urlopen", urlopen)
    return fetched


def _release():
    """A release archive laid out as this repository's are."""
    out = io.BytesIO()
    payloads = {}
    with tarfile.open(fileobj=out, mode="w:gz") as archive:
        for name in ("csgrs-partcad", "hyperdrc"):
            payload = b"#!/bin/sh\necho %s\n" % name.encode()
            payloads[name] = payload
            info = tarfile.TarInfo("csgrs-partcad-0.0.0-x86_64-unknown-linux-gnu/" + name)
            info.size = len(payload)
            archive.addfile(info, io.BytesIO(payload))
    return out.getvalue(), payloads


def test_a_release_is_downloaded_once_into_the_cache(tool, monkeypatch, tmp_path):
    archive, payloads = _release()
    fetched = _serve(tool, monkeypatch, archive, hashlib.sha256(archive).hexdigest())

    exe = tool.executable("csgrs-partcad", str(tmp_path))
    target = tmp_path / "csgrs" / tool.VERSION / "linux-x86_64"
    assert exe == str(target / "csgrs-partcad")
    for name, payload in payloads.items():
        assert (target / name).read_bytes() == payload
        assert os.access(target / name, os.X_OK)

    # The other program, and a second part, find it there.
    assert tool.executable("hyperdrc", str(tmp_path)) == str(target / "hyperdrc")
    assert len(fetched) == 1
    assert sorted(os.listdir(tmp_path / "csgrs" / tool.VERSION)) == ["linux-x86_64"]


def test_a_release_that_does_not_match_its_checksum_is_refused(tool, monkeypatch, tmp_path):
    archive, _payloads = _release()
    _serve(tool, monkeypatch, archive, "0" * 64)
    with pytest.raises(RuntimeError, match="checksum"):
        tool.executable("csgrs-partcad", str(tmp_path))
    assert not (tmp_path / "csgrs" / tool.VERSION / "linux-x86_64").exists()


def test_a_machine_with_no_release_is_told_what_to_do(tool, monkeypatch, tmp_path):
    monkeypatch.setattr(tool.shutil, "which", lambda _name: None)
    monkeypatch.setattr(tool, "host", lambda: ("sunos", "sparc"))
    with pytest.raises(RuntimeError, match="PATH"):
        tool.executable("csgrs-partcad", str(tmp_path))


def test_only_this_package_s_programs_are_run(tool):
    with pytest.raises(ValueError):
        tool.executable("sh")


def test_without_a_cache_directory_the_package_s_own_is_used(tool):
    assert tool.default_cache_dir() == os.path.join(PLUGIN, ".cache")


# --------------------------------------------------------------------------- #
# The program                                                                 #
# --------------------------------------------------------------------------- #


def _cli():
    """'csgrs-partcad' as 'cli/' builds it now: built there, or on the PATH at the crate's version."""
    tool = _module("csgrs_tool")
    for candidate in (
        os.path.join(PLUGIN, "cli", "target", "release", "csgrs-partcad"),
        shutil.which("csgrs-partcad"),
    ):
        if candidate and os.path.isfile(candidate) and tool.version_of(candidate, "csgrs-partcad") == _crate_version():
            return candidate
    pytest.skip("no csgrs-partcad %s here; build it with `cargo build --release` in cli/" % _crate_version())


@pytest.fixture(scope="module")
def described():
    return json.loads(subprocess.run([_cli(), "describe"], capture_output=True, text=True, check=True).stdout)


def test_the_package_declares_every_type_the_program_builds(described):
    declared = _yaml(os.path.join(PLUGIN, "partcad.yaml"))["partTypes"]
    assert sorted(declared) == sorted(described["types"])
    for name, spec in described["types"].items():
        assert declared[name]["path"] == "csgrs_part.py"
        assert declared[name].get("extension") == spec["extension"], name
        # The description lists the parameters, so that `pc info` says them.
        for parameter in spec["parameters"]:
            assert parameter["name"] in declared[name]["desc"], (name, parameter["name"])


def test_the_ready_made_parts_declare_the_types_own_parameters(described):
    """A ready-made part's parameters are its type's, with the type's defaults unless it says why not."""
    package = _yaml(os.path.join(PLUGIN, "partcad.yaml"))
    differs = {("helical_gear", "helix_angle")}
    for name, declared in package["parts"].items():
        spec = {p["name"]: p for p in described["types"][declared["type"].lstrip(":")]["parameters"]}
        for parameter, value in declared["parameters"].items():
            assert parameter in spec, (name, parameter)
            assert value["type"] == spec[parameter]["type"], (name, parameter)
            assert value.get("enum") == spec[parameter].get("choices"), (name, parameter)
            if (name, parameter) not in differs:
                assert value["default"] == spec[parameter]["default"], (name, parameter)


def test_the_program_refuses_a_misspelt_parameter(tmp_path):
    request = tmp_path / "request.json"
    request.write_text(json.dumps({"type": "gear", "parameters": {"teth": 3}}))
    result = subprocess.run([_cli(), "build", str(request), str(tmp_path / "out")], capture_output=True, text=True)
    assert result.returncode == 1
    assert "no parameter 'teth'" in result.stderr and "teeth" in result.stderr


def test_the_program_builds_a_gear_s_bodies(part, tmp_path):
    request = tmp_path / "request.json"
    request.write_text(json.dumps({"type": "gear", "parameters": {"teeth": 12, "module": 2, "bore": 4}}))
    mesh = tmp_path / "gear.mesh"
    result = subprocess.run([_cli(), "build", str(request), str(mesh)], capture_output=True, text=True, check=True)
    report = json.loads(result.stdout)
    assert report["certainty"] in ("certified", "approximate")
    assert [b["kind"] for b in report["bodies"]] == ["solid"]
    # The tip circle: the pitch diameter, 24, and an addendum of one module (2)
    # either side -- reached on both axes, since 12 teeth put a tip on each.
    assert report["bbox"] == pytest.approx([-14, -14, 0, 14, 14, 5], abs=1e-9)
    points, triangles = part.read_mesh(str(mesh))
    assert len(points) == report["vertices"] and len(triangles) == report["triangles"]


def test_a_hollow_is_a_cavity_of_the_solid_around_it(tmp_path):
    tree = {"difference": [{"cube": 10}, {"cube": 4, "translate": [3, 3, 3]}]}
    request = tmp_path / "request.json"
    request.write_text(json.dumps({"type": "csg", "tree": tree}))
    result = subprocess.run(
        [_cli(), "build", str(request), str(tmp_path / "out.mesh")], capture_output=True, text=True, check=True
    )
    bodies = json.loads(result.stdout)["bodies"]
    assert [(b["kind"], b["parent"]) for b in bodies] == [("solid", None), ("cavity", 0)]
    assert sum(b["volume"] for b in bodies) == pytest.approx(1000 - 64)


# --------------------------------------------------------------------------- #
# The package, through PartCAD                                               #
# --------------------------------------------------------------------------- #


def _partcad():
    """PartCAD, if it is installed and new enough to read 'qual:'."""
    pc = pytest.importorskip("partcad")
    if not hasattr(pc.output, "package_cache_dir"):
        pytest.skip("this PartCAD's partTypes have no cache directory; it is older than this plugin")
    return pc


@pytest.fixture(scope="module")
def ctx():
    pc = _partcad()
    return pc.Context(os.path.join(HERE, "partcad.yaml"))


def test_the_tests_are_not_part_of_the_plugin():
    pc = _partcad()
    pytest.importorskip("partcad_utils.pcignore")
    plugin = pc.Context(PLUGIN)
    names = [p["name"] if isinstance(p, dict) else p for p in plugin.get_all_packages(has_stuff=False)]
    assert not any("tests" in name for name in names)
    assert pc.Context(HERE).root_path == HERE


def test_the_plugin_declares_its_part_types_and_parts(ctx):
    plugin = ctx.get_project("//csgrs")
    assert plugin is not None
    for name in ("gear", "rack", "tpms", "airfoil", "text", "truetype", "image", "vrml", "csg"):
        assert plugin.get_part_type_config(name)["path"] == "csgrs_part.py"
    assert plugin.get_part_type_config("image")["extension"] == "png"
    assert {"spur_gear", "gyroid", "naca_airfoil", "text"} <= set(plugin.config_obj["parts"])


@pytest.mark.parametrize("name, file", [("logo", "logo.png"), ("model", "model.wrl"), ("bracket", "bracket.csgrs")])
def test_the_file_is_the_workspace_s(ctx, name, file):
    part = ctx.get_part(":" + name)
    assert os.path.abspath(part.path) == os.path.join(HERE, file)
    assert os.path.abspath(part.path) in [os.path.abspath(p) for p in part.cache_dependencies]


def _measured(ctx, name):
    from partcad import shape_envelope

    part = ctx.get_part(name)
    envelope = asyncio.run(part.get_wrapped(ctx))
    assert envelope is not None, part.errors
    return envelope[shape_envelope.KEY_METADATA][shape_envelope.METADATA_MEASUREMENTS]


@pytest.mark.slow
def test_a_gear_is_the_size_its_parameters_say(ctx):
    # 12 teeth of module 1.5: a pitch diameter of 18, and the tips a module beyond it.
    assert _measured(ctx, ":pinion")["bbox"] == pytest.approx([-10.5, -10.5, 0, 10.5, 10.5, 4], abs=1e-6)


@pytest.mark.slow
def test_a_declared_parameter_rebuilds_the_part(ctx):
    plain = _measured(ctx, ":helical")
    bored = _measured(ctx, ":helical;bore=10.0")
    assert bored["volume"] < plain["volume"]
    assert bored["volume"] == pytest.approx(plain["volume"] - 3.14159265 * (25 - 6.25) * 5, rel=1e-2)


@pytest.mark.slow
def test_a_ready_made_part_takes_its_parameters_from_the_reference(ctx):
    # 30 teeth of module 2: tips on a 64 mm circle, one of them on the X axis.
    bbox = _measured(ctx, "//csgrs:spur_gear;teeth=30,module=2.0")["bbox"]
    assert [bbox[0], bbox[3], bbox[5]] == pytest.approx([-32, 32, 5], abs=1e-6)


@pytest.mark.slow
@pytest.mark.parametrize("name", ["bar", "lattice", "wing", "label", "logo"])
def test_every_part_type_builds_a_solid(ctx, name):
    measured = _measured(ctx, ":" + name)
    assert measured["volume"] > 0


@pytest.mark.slow
def test_a_vrml_model_is_scaled_to_mm(ctx):
    assert _measured(ctx, ":model")["bbox"] == pytest.approx([0, 0, 0, 5.08, 2.54, 1.27], abs=1e-9)


@pytest.mark.slow
def test_a_csg_tree_takes_the_part_s_parameters(ctx):
    plate = _measured(ctx, ":bracket")
    wider = _measured(ctx, ":bracket;width=60.0")
    assert plate["bbox"][3] == pytest.approx(40) and wider["bbox"][3] == pytest.approx(60)
    assert wider["volume"] == pytest.approx(plate["volume"] + 20 * 20 * 5, rel=1e-6)


# --------------------------------------------------------------------------- #
# The hyperdrc test                                                           #
# --------------------------------------------------------------------------- #


@pytest.fixture
def drc():
    return _module("hyperdrc_qual")


DOCUMENT = {
    "violations": [
        {
            "check": "drill-to-copper-clearance",
            "severity": "error",
            "message": "too close",
            "layers": ["b.kicad_pcb:F.Cu"],
            "locations": [[1.0, 2.0], [3.0, 4.0]],
        },
        {"check": "drill-to-copper-clearance", "severity": "error", "message": "too close again", "layers": ["F.Cu"]},
        {"check": "fiducial-readiness", "severity": "warning", "message": "no fiducials"},
        {"check": "file-manifest-readiness", "severity": "error", "message": "no copper layer in the package"},
    ]
}


def test_an_error_fails_the_board_and_a_warning_does_not(drc):
    answer = drc.verdict(DOCUMENT, ignore={"file-manifest-readiness"})
    assert answer["passed"] is False
    assert answer["findings"] == [
        {
            "level": "error",
            "text": "drill-to-copper-clearance: 2 errors -- too close (first at 1.000, 2.000 (and 1 more) on F.Cu)",
        },
        {"level": "warning", "text": "fiducial-readiness: 1 warning -- no fiducials"},
    ]
    assert [row["check"] for row in answer["report"]["rows"]] == ["drill-to-copper-clearance"] * 2 + [
        "fiducial-readiness"
    ]
    assert [c["key"] for c in answer["report"]["columns"]] == ["severity", "check", "message", "layers", "location"]


def test_strict_fails_a_board_on_a_warning(drc):
    document = {"violations": [DOCUMENT["violations"][2]]}
    assert drc.verdict(document)["passed"] is True
    strict = drc.verdict(document, strict=True)
    assert strict["passed"] is False and strict["findings"][0]["level"] == "error"


def test_the_board_is_the_object_s_own_by_default(drc, tmp_path):
    (tmp_path / "board.kicad_pcb").write_text("(kicad_pcb)")
    request = {"subject": "//pkg:board;layers=4", "package_dir": str(tmp_path)}
    arguments, package, why_not = drc.inputs(request)
    assert arguments == ["--kicad-pcb", str(tmp_path / "board.kicad_pcb")] and not package and why_not is None


def test_gerbers_are_checked_as_a_package(drc, tmp_path):
    (tmp_path / "fab").mkdir()
    (tmp_path / "fab.zip").write_bytes(b"PK")
    request = {"subject": "//pkg:board", "package_dir": str(tmp_path), "gerbers": "fab", "config": "rules.json"}
    assert drc.inputs(request) == (
        ["--gerber-dir", str(tmp_path / "fab"), "--config", str(tmp_path / "rules.json")],
        True,
        None,
    )
    request["gerbers"] = "fab.zip"
    assert drc.inputs(request)[0][:2] == ["--package-archive", str(tmp_path / "fab.zip")]


def test_no_board_is_a_skip_that_says_where_it_looked(drc, tmp_path):
    arguments, _package, why_not = drc.inputs({"subject": "//pkg:board", "package_dir": str(tmp_path)})
    assert arguments is None and "board.kicad_pcb" in why_not
    answer = drc.process(str(tmp_path), {"subject": "//pkg:board", "package_dir": str(tmp_path)})
    assert answer["passed"] is None and answer["skipped"] == why_not


def _hyperdrc():
    tool = _module("csgrs_tool")
    found = shutil.which("hyperdrc")
    if not found or tool.version_of(found, "hyperdrc") != tool.PROGRAMS["hyperdrc"]:
        pytest.skip("no hyperdrc %s on the PATH" % tool.PROGRAMS["hyperdrc"])
    return found


def test_hyperdrc_finds_the_via_drilled_into_a_pad(drc, tmp_path):
    _hyperdrc()
    answer = drc.process(str(tmp_path), {"subject": "//tests:board", "package_dir": HERE})
    assert answer["passed"] is False
    errors = {f["text"].split(":", 1)[0] for f in answer["findings"] if f["level"] == "error"}
    assert {"drill-to-copper-clearance", "different-net-short-readiness", "different-net-spacing"} <= errors
    # A board alone is not a fabrication package, and is not reported as an empty one.
    assert not any(row["check"] == "file-manifest-readiness" for row in answer["report"]["rows"])
    assert (tmp_path / "hyperdrc.json").is_file()


@pytest.mark.slow
def test_pc_qual_test_fails_the_board(ctx):
    _hyperdrc()
    # The 'pc' beside this interpreter: the PartCAD the other tests import.
    pc = os.path.join(os.path.dirname(sys.executable), "pc")
    if not os.path.isfile(pc):
        pytest.skip("no 'pc' command here")
    result = subprocess.run(
        [pc, "--no-ansi", "qual", "test", "--other", ":board"], cwd=HERE, capture_output=True, text=True
    )
    log = result.stdout + result.stderr
    assert result.returncode != 0, log
    assert "Qualification failed: //:board: drc: drill-to-copper-clearance" in log


# --------------------------------------------------------------------------- #
# The profiles, which the sketch types are made of                            #
# --------------------------------------------------------------------------- #


def test_a_profile_keeps_its_holes_and_its_curves(tmp_path):
    tree = {"difference": [{"circle": {"r": 10, "segments": 64}}, {"square": 4, "center": True}]}
    request = tmp_path / "request.json"
    request.write_text(json.dumps({"type": "csg_profile", "tree": tree}))
    out = tmp_path / "profile.json"
    report = json.loads(
        subprocess.run([_cli(), "profile", str(request), str(out)], capture_output=True, text=True, check=True).stdout
    )
    assert report["faces"] == 1 and report["bbox"] == pytest.approx([-10, -10, 10, 10])
    faces = json.loads(out.read_text())["faces"]
    assert len(faces) == 1 and len(faces[0]["holes"]) == 1
    assert {next(iter(segment)) for segment in faces[0]["holes"][0]} == {"line"}
