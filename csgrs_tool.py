#
# PartCAD, 2026
#
# Licensed under Apache License, Version 2.0.
#
"""Find this package's programs, or fetch them: 'csgrs-partcad' and 'hyperdrc'.

Both are native programs built from csgrs (https://github.com/timschmidt/csgrs,
MIT) rather than Python packages, so pip cannot install them into a sandbox:

  * 'csgrs-partcad', built from this repository's 'cli/', is what every part
    type here runs: a JSON request in, a triangle mesh out;
  * 'hyperdrc', built from csgrs's own 'crates/hyperdrc' at the same commit,
    is what the 'hyperdrc' qualification test runs.

Both ship in one archive per platform, published as a release of this
repository. In order of preference, the copy of a program used is:

  1. one on the PATH, *if* it is the release this module pins -- this
     package's image (see 'Dockerfile') carries that one, and so may a machine
     that installed it;
  2. the release this module pins, downloaded once into the package's cache
     directory and verified against the checksum below before it is unpacked.

The PATH copy is held to the pin because the pin is what the parts are keyed
by: 'csgrs_part.py' raises its CACHE_VERSION with it, and another build on the
PATH would make the same parameters into other geometry under the same key.
(This is cadmpeg's rule in PartCAD's own 'cadmpeg_tool.py', for its reason.)

The same file, run as a program, puts the pinned release into a directory:

    python3 csgrs_tool.py /usr/local/bin

which is how the image comes to carry it.

Runs inside the sandbox, so it uses nothing but the standard library.
"""

import hashlib
import os
import platform
import shutil
import subprocess
import sys
import tarfile
import tempfile
import time
import urllib.error
import urllib.request
import zipfile

# The release of this repository the programs come from. Moving it is moving
# both programs; see README.md, "Releasing".
VERSION = "0.1.0"
RELEASE_URL = "https://github.com/partcad/partcad-csgrs/releases/download/v%s/" % VERSION

# What each program says its version is ('<name> <version> ...' on '--version'),
# which is how a copy on the PATH is told to be the pinned one.
PROGRAMS = {
    "csgrs-partcad": VERSION,
    "hyperdrc": "0.3.0",
}

# (operating system, machine) -> (asset, SHA-256), from the release's
# 'SHA256SUMS.txt'. A platform missing here has no build to download.
ASSETS = {
    ("linux", "x86_64"): (
        "csgrs-partcad-%s-x86_64-unknown-linux-gnu.tar.gz" % VERSION,
        "aee71a5b689cd7183f2cbdb40209db3a22cbe600e0bf69270e880eef6227690c",
    ),
    ("linux", "aarch64"): (
        "csgrs-partcad-%s-aarch64-unknown-linux-gnu.tar.gz" % VERSION,
        "e71a8de4bcb5e2c73c74c75badb35f78a53d9498baf2601dc369a7837af79099",
    ),
    ("darwin", "x86_64"): (
        "csgrs-partcad-%s-x86_64-apple-darwin.tar.gz" % VERSION,
        "1c637895d9747ea945c3427f5b828899cefa8bb04ed11425df897623fe7bd11b",
    ),
    ("darwin", "aarch64"): (
        "csgrs-partcad-%s-aarch64-apple-darwin.tar.gz" % VERSION,
        "b15f56fa5eb1119c6b25051dbb8013a8ded3102876f4fdd15a5265b7c7c814b6",
    ),
    ("windows", "x86_64"): (
        "csgrs-partcad-%s-x86_64-pc-windows-msvc.zip" % VERSION,
        "997fd26484570acaac5950a53d49cdffcbb558dd4c94eaa06d01e3bfb6f35112",
    ),
}

MACHINES = {"amd64": "x86_64", "x86_64": "x86_64", "arm64": "aarch64", "aarch64": "aarch64"}

# How many times a download is tried, waiting twice as long after each failure.
ATTEMPTS = 4
FIRST_WAIT = 2


def host():
    """This machine as ASSETS spells it: ('linux', 'x86_64') and so on."""
    return platform.system().lower(), MACHINES.get(platform.machine().lower(), platform.machine().lower())


def default_cache_dir():
    """This package's own directory, for a PartCAD that hands scripts no 'cache_dir'."""
    return os.path.join(os.path.dirname(os.path.abspath(__file__)), ".cache")


def _transient(error):
    """Whether trying again could give a different answer: a server error or a dropped connection."""
    if isinstance(error, urllib.error.HTTPError):
        return error.code >= 500 or error.code == 429
    return isinstance(error, (urllib.error.URLError, ConnectionError, TimeoutError))


def fetch(url, path):
    """Download 'url' to 'path', trying again while the failure is one that passes."""
    for attempt in range(ATTEMPTS):
        try:
            with urllib.request.urlopen(url, timeout=600) as response, open(path, "wb") as f:
                shutil.copyfileobj(response, f)
            return
        except Exception as e:
            if attempt == ATTEMPTS - 1 or not _transient(e):
                raise
            time.sleep(FIRST_WAIT * 2**attempt)


def version_of(path, name):
    """The version the program at 'path' says it is, or None if it says nothing this module reads."""
    try:
        result = subprocess.run([path, "--version"], capture_output=True, text=True, timeout=60, check=False)
    except (OSError, subprocess.SubprocessError):
        return None
    words = (result.stdout or "").split()
    return words[1] if result.returncode == 0 and len(words) >= 2 and words[0] == name else None


def executable(name="csgrs-partcad", cache_dir=None):
    """The path of 'name' to run: the pinned one on the PATH, or the pinned release, downloaded if need be."""
    if name not in PROGRAMS:
        raise ValueError("this package ships no program '%s'" % name)
    found = shutil.which(name)
    if found and version_of(found, name) == PROGRAMS[name]:
        return found
    return os.path.join(download(cache_dir or default_cache_dir()), name + (".exe" if host()[0] == "windows" else ""))


def download(cache_dir):
    """The pinned release unpacked into 'cache_dir' once, verified, and the directory holding its programs."""
    system, machine = host()
    if (system, machine) not in ASSETS:
        raise RuntimeError(
            "partcad-csgrs %s publishes no build for %s/%s; build 'cli/' with cargo and put csgrs-partcad on the PATH"
            % (VERSION, system, machine)
        )
    asset, sha256 = ASSETS[(system, machine)]
    suffix = ".exe" if system == "windows" else ""
    target = os.path.join(cache_dir, "csgrs", VERSION, "%s-%s" % (system, machine))
    if all(os.path.isfile(os.path.join(target, name + suffix)) for name in PROGRAMS):
        return target

    os.makedirs(os.path.dirname(target), exist_ok=True)
    # Unpacked beside the target and moved into place in one rename, so that
    # two parts built at once never see half a download: the loser of the
    # race finds the winner's copy and uses that.
    staging = tempfile.mkdtemp(prefix=".download-", dir=os.path.dirname(target))
    try:
        archive = os.path.join(staging, asset)
        fetch(RELEASE_URL + asset, archive)
        digest = hashlib.sha256()
        with open(archive, "rb") as f:
            for chunk in iter(lambda: f.read(1 << 20), b""):
                digest.update(chunk)
        if digest.hexdigest() != sha256:
            raise RuntimeError("%s does not match its published checksum; refusing to run it" % asset)

        unpacked = os.path.join(staging, "unpacked")
        if asset.endswith(".zip"):
            with zipfile.ZipFile(archive) as z:
                z.extractall(unpacked)
        else:
            with tarfile.open(archive) as t:
                if hasattr(tarfile, "data_filter"):
                    t.extractall(unpacked, filter="data")
                else:
                    t.extractall(unpacked)

        payload = os.path.join(staging, "payload")
        os.makedirs(payload)
        for name in PROGRAMS:
            for root, _dirs, files in os.walk(unpacked):
                if name + suffix in files:
                    shutil.move(os.path.join(root, name + suffix), os.path.join(payload, name + suffix))
                    break
            else:
                raise RuntimeError("%s holds no %s" % (asset, name + suffix))
            os.chmod(os.path.join(payload, name + suffix), 0o755)

        try:
            os.replace(payload, target)
        except OSError:
            if not all(os.path.isfile(os.path.join(target, name + suffix)) for name in PROGRAMS):
                raise
    finally:
        shutil.rmtree(staging, ignore_errors=True)
    return target


def install(directory):
    """Put the pinned release, verified, into 'directory', whatever the PATH holds, and return the paths."""
    with tempfile.TemporaryDirectory(prefix="csgrs-") as cache:
        source = download(cache)
        os.makedirs(directory, exist_ok=True)
        installed = []
        for name in sorted(os.listdir(source)):
            target = os.path.join(directory, name)
            shutil.copyfile(os.path.join(source, name), target)
            os.chmod(target, 0o755)
            installed.append(target)
    return installed


if __name__ == "__main__":
    if len(sys.argv) != 2:
        sys.exit("usage: python3 csgrs_tool.py <directory>")
    for path in install(sys.argv[1]):
        print(path)
