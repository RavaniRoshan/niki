# NIKI sandbox image.
#
# Pre-bundles the minimal, purpose-specific toolchain the agent pipeline needs
# (git, python3, node, npm) so that runtime sandbox startup performs NO package
# installation. Build once and reference it from niki.toml's [docker].base_image.
#
#   docker build -t niki-sandbox:24.04 -f docker/Dockerfile .
#
FROM ubuntu:24.04

ENV DEBIAN_FRONTEND=noninteractive

COPY scripts/sandbox-bootstrap.sh /tmp/sandbox-bootstrap.sh
RUN chmod +x /tmp/sandbox-bootstrap.sh && /tmp/sandbox-bootstrap.sh

# niki starts the container with `tail -f /dev/null`; this is just a safe default.
CMD ["tail", "-f", "/dev/null"]
