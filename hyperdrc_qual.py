#
# PartCAD, 2026
#
# Licensed under Apache License, Version 2.0.
#
"""hyperdrc as a PartCAD qualification test of the 'other' subtype.

hyperdrc (csgrs's 'crates/hyperdrc', MIT) checks a printed circuit board for
design readiness: copper clearance and width, annular rings, drill spacing,
solder mask and paste, silkscreen, board outline, mounting holes, fiducials,
controlled impedance, return paths, high-current and high-voltage spacing --
some 160 checks, read from a KiCad board ('.kicad_pcb') or a fabrication
package of Gerber and Excellon files.

Run by PartCAD's 'wrapper_qual.py' with 'request' and 'path' as its globals.
The board is the object's own file unless the test names another:

    pcb       a '.kicad_pcb', relative to the object's package; by default
              '<object name>.kicad_pcb', which is where a 'kicad' part's board
              is unless the part says otherwise
    gerbers   a directory of Gerber and drill files, or an archive of them
              (.zip, .tar, .tar.gz, .tgz), checked as the package a fab house
              would be sent -- instead of, or as well as, the board
    config    a hyperdrc rule configuration (JSON), relative to the package
    ignore    checks to leave out, separated by commas
    strict    whether a warning fails the test as an error does

hyperdrc's verdict and words are carried across as they are:

    passed    no finding of severity 'error' (nor of 'warning', if strict);
              None when there is no board to check, which is a skip
    findings  one per check that found something: how many, and what the
              first one says and where
    report    every finding, one row each, in hyperdrc's own columns

Given a board and no Gerbers, the checks of a fabrication package's file
manifest ('file-manifest-readiness') are left out: they report that the
package holds no copper layer, which is true of a package nobody made.
"""

import json
import os
import subprocess
import sys

# Siblings are imported by bare name, and the sandbox sets PYTHONSAFEPATH.
sys.path.append(os.path.dirname(os.path.abspath(__file__)))

COLUMNS = [
    {"key": "severity", "title": "Severity"},
    {"key": "check", "title": "Check"},
    {"key": "message", "title": "Message"},
    {"key": "layers", "title": "Layers"},
    {"key": "location", "title": "Location (mm)"},
]

# What a board on its own is not: a fabrication package.
PACKAGE_CHECKS = ("file-manifest-readiness",)

ARCHIVES = (".zip", ".tar", ".tar.gz", ".tgz")


def _flag(value) -> bool:
    if isinstance(value, str):
        return value.strip().lower() in ("1", "true", "yes", "on")
    return bool(value)


def _names(value):
    """A list of names, however it was written: 'a,b', '[a, b]' or a list."""
    if value is None:
        return []
    if isinstance(value, str):
        return [name.strip() for name in value.strip("[]").split(",") if name.strip()]
    return [str(name).strip() for name in value if str(name).strip()]


def _in_package(request, name):
    """'name' as an absolute path, relative to the object's package."""
    base = request.get("package_dir") or os.getcwd()
    return name if os.path.isabs(name) else os.path.normpath(os.path.join(base, name))


def object_name(request) -> str:
    """The object's own name: 'board' of '//pkg:board;layers=4'."""
    subject = str(request.get("subject") or "")
    return subject.rsplit(":", 1)[-1].split(";", 1)[0]


def inputs(request):
    """'(command line arguments, whether a package is checked, why nothing can be)'."""
    arguments = []
    pcb = request.get("pcb") or ("%s.kicad_pcb" % object_name(request) if object_name(request) else "")
    gerbers = request.get("gerbers")
    board = _in_package(request, pcb) if pcb else None
    if board and os.path.isfile(board):
        arguments += ["--kicad-pcb", board]
    elif request.get("pcb"):
        return None, False, "there is no board at %s" % board
    else:
        board = None

    if gerbers:
        package = _in_package(request, gerbers)
        if os.path.isdir(package):
            arguments += ["--gerber-dir", package]
        elif os.path.isfile(package) and package.lower().endswith(ARCHIVES):
            arguments += ["--package-archive", package]
        else:
            return None, False, "'gerbers' names %s, which is neither a directory nor an archive of Gerbers" % package

    if not arguments:
        return (
            None,
            False,
            "there is no board to check: no %s beside the object, and the test names no 'pcb' or 'gerbers'"
            % os.path.basename(pcb or "<name>.kicad_pcb"),
        )
    if request.get("config"):
        arguments += ["--config", _in_package(request, request["config"])]
    return arguments, bool(gerbers), None


def _location(violation):
    locations = violation.get("locations") or []
    if not locations:
        return ""
    x, y = locations[0][:2]
    more = " (and %d more)" % (len(locations) - 1) if len(locations) > 1 else ""
    return "%.3f, %.3f%s" % (x, y, more)


def _layers(violation):
    return ", ".join(layer.rsplit(":", 1)[-1] for layer in violation.get("layers") or [])


def verdict(document, strict=False, ignore=()):
    """What PartCAD reads of the test, from what 'hyperdrc --format json' printed."""
    violations = [v for v in document.get("violations") or [] if v.get("check") not in ignore]
    failing = ("error", "warning") if strict else ("error",)

    by_check = {}
    for violation in violations:
        by_check.setdefault((violation.get("check"), violation.get("severity")), []).append(violation)

    findings = []
    for (check, severity), found in sorted(
        by_check.items(), key=lambda item: (item[0][1] != "error", item[0][1] != "warning", item[0][0] or "")
    ):
        first = found[0]
        text = "%s: %d %s%s -- %s" % (
            check,
            len(found),
            severity or "finding",
            "" if len(found) == 1 else "s",
            first.get("message") or "no message",
        )
        where = _location(first)
        if where:
            text += " (first at %s%s)" % (where, " on " + _layers(first) if first.get("layers") else "")
        level = "error" if severity in failing else ("warning" if severity in ("error", "warning") else "info")
        findings.append({"level": level, "text": text})

    rows = [
        {
            "severity": v.get("severity") or "",
            "check": v.get("check") or "",
            "message": v.get("message") or "",
            "layers": _layers(v),
            "location": _location(v),
        }
        for v in violations
    ]
    passed = not any(v.get("severity") in failing for v in violations)
    return {
        "success": True,
        "passed": passed,
        "findings": findings,
        "report": {"columns": COLUMNS, "rows": rows},
    }


def process(path, request):
    import csgrs_tool

    arguments, package, why_not = inputs(request)
    if arguments is None:
        return {"success": True, "passed": None, "skipped": why_not, "findings": []}

    hyperdrc = csgrs_tool.executable("hyperdrc", request.get("cache_dir"))
    report = os.path.join(path, "hyperdrc.json")
    command = [hyperdrc, *arguments, "--format", "json", "--allow-findings"]
    result = subprocess.run(command, capture_output=True, text=True, check=False)
    if result.returncode != 0:
        detail = [line for line in (result.stderr or "").splitlines() if not line.startswith("hyperdrc: ")]
        return {
            "success": False,
            "exception": "hyperdrc failed (exit %d): %s"
            % (result.returncode, (detail[-1] if detail else result.stdout.strip()[-500:]) or "no reason given"),
        }
    with open(report, "w", encoding="utf-8") as f:
        f.write(result.stdout)
    document = json.loads(result.stdout)

    ignore = set(_names(request.get("ignore")))
    if not package:
        ignore.update(PACKAGE_CHECKS)
    return verdict(document, strict=_flag(request.get("strict")), ignore=ignore)
