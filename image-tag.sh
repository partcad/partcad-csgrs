#!/bin/sh
#
# The tag this package's image is published and pinned under: a hash of what it
# is built from, not a version.
#
# A content tag is immutable, so a `dockerImage:` pin keeps working when the
# image is next edited, a branch that edits the image publishes a new tag rather
# than replacing the one everything else uses, and merging that branch builds
# the same tag it did.
#
# `csgrs_tool.py` is an input: it is what decides which release the image
# installs. Run this after changing any input, and put what it prints into
# `partcad.yaml`; the workflow runs it too, so a pin that does not match what
# was built fails the build rather than a user's part.
set -eu

cd "$(dirname "$0")"

PYTHON_VERSION="${PYTHON_VERSION:-3.11}"

if command -v sha256sum >/dev/null 2>&1; then
    SUM="sha256sum"
elif command -v shasum >/dev/null 2>&1; then
    SUM="shasum -a 256"
else
    echo "image-tag.sh needs sha256sum or shasum" >&2
    exit 1
fi

{ echo "python ${PYTHON_VERSION}"; cat Dockerfile verify_image.py csgrs_tool.py; } | ${SUM} | cut -c1-12
