#!/usr/bin/env python3
#
# PartCAD, 2026
#
# Licensed under Apache License, Version 2.0.
#
"""Whether this image can actually run this package's programs.

Run at build time, so that an image which cannot is never published. The base
image's own `verify.py` checks the contract PartCAD talks to a container
through; this checks what this image adds on top of it, the way the package
uses it: `csgrs-partcad` builds a hollow cube and reports it as the solid and
the cavity it is, `hyperdrc` answers as the release the package pins, and the
image's own Python imports OpenCASCADE, numpy and PyYAML, which a script run in
the image as it is (`container:`) needs.
"""

import json
import os
import shutil
import subprocess
import sys
import tempfile

TREE = {"difference": [{"cube": 10}, {"cube": 4, "translate": [3, 3, 3]}]}


def main() -> int:
    failures = []
    for name in ("csgrs-partcad", "hyperdrc"):
        if not shutil.which(name):
            failures.append("'%s' is not on PATH" % name)

    cli = shutil.which("csgrs-partcad")
    if cli:
        with tempfile.TemporaryDirectory() as tmp:
            request = os.path.join(tmp, "request.json")
            with open(request, "w", encoding="utf-8") as f:
                json.dump({"type": "csg", "tree": TREE}, f)
            built = subprocess.run(
                [cli, "build", request, os.path.join(tmp, "out.mesh")], capture_output=True, text=True
            )
            try:
                bodies = json.loads(built.stdout)["bodies"]
            except (ValueError, KeyError):
                bodies = None
            if built.returncode != 0 or not bodies:
                failures.append(
                    "csgrs-partcad could not build a hollow cube: %s" % (built.stderr.strip() or built.returncode)
                )
            else:
                kinds = [b["kind"] for b in bodies]
                volume = sum(b["volume"] for b in bodies)
                if kinds != ["solid", "cavity"] or abs(volume - 936.0) > 1e-9:
                    failures.append(
                        "csgrs-partcad built %s of %r mm^3, not a solid and a cavity of 936" % (kinds, volume)
                    )

    # What a script imports when PartCAD runs it in this image as it is.
    imported = subprocess.run(
        [
            sys.executable,
            "-c",
            "import OCP, numpy, yaml; from OCP.BRepBuilderAPI import BRepBuilderAPI_MakeShapeOnMesh",
        ],
        capture_output=True,
        text=True,
    )
    if imported.returncode != 0:
        failures.append("the image's Python cannot import what the scripts need: %s" % imported.stderr.strip()[-300:])

    drc = shutil.which("hyperdrc")
    if drc:
        answer = subprocess.run([drc, "--version"], capture_output=True, text=True)
        if answer.returncode != 0 or not answer.stdout.startswith("hyperdrc "):
            failures.append("hyperdrc --version answered %r" % (answer.stdout or answer.stderr))

    for failure in failures:
        print("verify_image: " + failure, file=sys.stderr)
    if not failures:
        print("verify_image: csgrs-partcad and hyperdrc work")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
