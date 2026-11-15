# The runtime this package's scripts run best in: PartCAD's Python sandbox
# image with `csgrs-partcad` and `hyperdrc` on the PATH.
#
# It carries what pip cannot: the two native programs, at the release
# `csgrs_tool.py` pins, installed by that same file so that the image and a
# download can never be two different builds. Nothing else is here, because
# nothing else is needed: the scripts use the standard library, numpy, PyYAML
# and the OpenCASCADE that PartCAD installs into every sandbox.
#
# It does NOT carry this package's code. PartCAD mounts the package at run time,
# so a user who fetches a newer version gets that version rather than whatever
# was baked in months ago.
#
# It is an alternative, not a requirement. `partcad.yaml` names it as the
# package's `dockerImage`, which PartCAD uses where its `docker` sandbox is in
# use and the image can be pulled; anywhere else the scripts download the same
# release themselves (see `csgrs_tool.py`).
ARG PYTHON_VERSION=3.11
ARG ARCH=amd64
FROM ghcr.io/partcad/partcad-container-python:py${PYTHON_VERSION}-${ARCH}

# The moving tag, deliberately. What this image publishes is immutable and is
# what the package pins; what it is *built from* moves, so that a base image
# which breaks this plugin fails this repository's own build rather than a
# user's part.

LABEL partcad.image="1"
LABEL org.opencontainers.image.source="https://github.com/partcad/partcad-csgrs"

USER root

# The pinned release, verified against its checksum, into /usr/local/bin,
# which a sandbox that rebuilds PATH still searches.
COPY csgrs_tool.py /tmp/csgrs_tool.py
RUN python3 /tmp/csgrs_tool.py /usr/local/bin && rm /tmp/csgrs_tool.py

# What a caller may run, extending the base image's allowlist rather than
# replacing it: the interpreter stays reachable, and so do both programs.
ENV PC_CONTAINER_ALLOWED_COMMANDS='{"python": "/usr/local/bin/python3", "csgrs-partcad": "/usr/local/bin/csgrs-partcad", "hyperdrc": "/usr/local/bin/hyperdrc"}'

# Two checks, and neither is redundant. The first is the base image's own: it
# says whether anything above broke the contract PartCAD talks to this image
# through. The second is this package's, and proves the programs work here.
COPY verify_image.py /tmp/verify_image.py
RUN python3 /pc/verify.py \
  && python3 /tmp/verify_image.py \
  && rm /tmp/verify_image.py

USER pc
