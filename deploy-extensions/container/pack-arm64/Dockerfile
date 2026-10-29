# No `# syntax=` directive on purpose: it would pull a floating frontend image (docker/dockerfile:1) that this file
# could not pin; the builder's built-in Dockerfile frontend is used (same rule as scripts/laya_train/container/Dockerfile.train).
#
# Phase 14 plan 14-07: the linux/arm64 pack-and-verify image (D-14, Fargate on Graviton). One program runs in it:
# run_step.py pack (ENTRYPOINT + CMD), which downloads a finished training run, re-hashes it, runs `pack_laya pack` then
# `pack_laya verify` and uploads the .apr only when verify accepted it. Pack and verify run on arm64 ONLY: every parity
# and pack-tolerance bar was measured on aarch64 and the .apr is served there. No pip, no Python dependency: run_step.py
# is standard library only, and the policy lives in /src/contracts (read by pack_laya, never an argument).
#
# BUILD CONTEXT: exactly the paths in deploy-extensions/container/pack-arm64/context-paths.txt, assembled from the git
# INDEX into a scratch directory by smoke.sh here and by the image recipe in plan 14-09, then
#     docker build --platform linux/arm64 -f deploy-extensions/container/pack-arm64/Dockerfile <scratch dir>
# (this Dockerfile is outside the context, which is allowed with a directory context). The repository root is NEVER the
# context: no .planning/, docs/ or models/ (a 26 GB tree) ever enters a pack build, and a dirty one never blocks it.
# Add a path to the list only when the smoke proves the build needs it.
#
# WHAT IS PINNED, AND WHAT IS NOT (the run's identity is the recorded image DIGEST, not a byte-identical rebuild):
#   pinned by digest or hash   the rust and debian base images (index @sha256, below), the workspace build (`--locked`,
#                              Cargo.lock; rustc 1.93.0 exactly as rust-toolchain.toml pins), the aws CLI v2 aarch64 zip
#                              (version + sha256, verified in the RUN), the contracts and scripts (COPY)
#   NOT pinned                 the apt packages (python3, ca-certificates, curl, unzip): they resolve from Debian
#                              bookworm's archive at build time. The crates.io registry is reached by cargo, which checks
#                              every crate against Cargo.lock's checksums.
#   never in the image         pip installs, weights or any base snapshot (the base tar is downloaded at run time and
#                              verified against base-stage.json before it is extracted), any AWS credential.
#
# rust:1.93.0-bookworm index digest, resolved 2026-10-29 with `docker buildx imagetools inspect rust:1.93.0-bookworm`
# (linux/arm64 child chosen by --platform at build time).
# RUSTUP_TOOLCHAIN=1.93.0 pins the same compiler rust-toolchain.toml names, without rustup trying to fetch the
# rustfmt/clippy components that file lists (the image ships the minimal profile; a build needs neither).
FROM rust:1.93.0-bookworm@sha256:d0a4aa3ca2e1088ac0c81690914a0d810f2eee188197034edf366ed010a2b382 AS build

ENV RUSTUP_TOOLCHAIN=1.93.0 \
    CARGO_TARGET_DIR=/src/target \
    CARGO_TERM_COLOR=never

WORKDIR /src
COPY Cargo.toml Cargo.lock rust-toolchain.toml /src/
COPY src/ /src/src/
COPY crates/ /src/crates/
COPY contracts/ /src/contracts/

# An EXAMPLE, not a bin: no promotion of pack_laya to a binary target, so FALSIFY-MONO-011 is untouched. `--locked`
# fails the build if Cargo.lock would change.
RUN cargo build --release --locked -p aprender-decide --example pack_laya

# debian:bookworm-slim index digest, resolved 2026-10-29 with `docker buildx imagetools inspect debian:bookworm-slim`.
FROM debian:bookworm-slim@sha256:99c0b25c46d0214d5a2091092e7908907dcaecd6a962cde0ac700acee222b6f3

ENV DEBIAN_FRONTEND=noninteractive \
    LANG=C.UTF-8

SHELL ["/bin/bash", "-o", "pipefail", "-c"]

RUN apt-get update \
 && apt-get install -y --no-install-recommends python3 ca-certificates curl unzip \
 && rm -rf /var/lib/apt/lists/*

# aws CLI v2 (aarch64) from the official zip, pinned by version and sha256 (the hash was taken from awscli.amazonaws.com
# over TLS on 2026-10-29 and is checked here; run_step.py transfers through `aws s3api get-object` / `put-object`).
ARG AWSCLI_VERSION=2.39.1
ARG AWSCLI_SHA256=5adf0b5052843e542b55a846d49ba2a2b3f3fb4960f35c11df9ac7c395cffaaf
RUN curl -fsSL -o /tmp/awscliv2.zip "https://awscli.amazonaws.com/awscli-exe-linux-aarch64-${AWSCLI_VERSION}.zip" \
 && echo "${AWSCLI_SHA256}  /tmp/awscliv2.zip" | sha256sum -c - \
 && unzip -q /tmp/awscliv2.zip -d /tmp \
 && /tmp/aws/install \
 && rm -rf /tmp/aws /tmp/awscliv2.zip \
 && aws --version

# pack_laya resolves its policy at RUN time from env!("CARGO_MANIFEST_DIR")/../../contracts, a path baked in at COMPILE
# time (crates/aprender-decide/examples/pack_laya.rs, fn contract): the build stage compiled it at
# /src/crates/aprender-decide, so at run time it reads /src/crates/aprender-decide/../../contracts = /src/contracts.
# That directory only has to exist for the path to resolve (`..` of a missing directory fails on Linux), and
# /src/contracts must hold the files pack_laya reads: the laya gate, the laya parity and the decide-apr contracts.
# THE POLICY IS NOT AN ARGUMENT: nothing here, and nothing in run_step.py, lets a caller point it elsewhere.
COPY --from=build /src/target/release/examples/pack_laya /src/target/release/examples/pack_laya
COPY contracts/laya-finetune-gate-v1.yaml contracts/laya-parity-v1.yaml contracts/decide-apr-v1.yaml /src/contracts/
RUN mkdir -p /src/crates/aprender-decide /work

# run_step.py's ROOT is parents[3] of the script = /opt/laya. Its selftest and its contract hashing need the golden
# fixtures and the very contract files pack_laya reads, so both are present at ROOT's paths (the contracts by symlink:
# one copy, no second set of bytes to drift).
COPY scripts/laya_train/container/run_step.py scripts/laya_train/container/stage_base.py scripts/laya_train/container/base-stage.json /opt/laya/scripts/laya_train/container/
COPY crates/aprender-mcp-laya-train/fixtures/ /opt/laya/crates/aprender-mcp-laya-train/fixtures/
RUN ln -s /src/contracts /opt/laya/contracts

ARG GIT_COMMIT=unknown
LABEL org.opencontainers.image.revision=$GIT_COMMIT

ENV PYTHONDONTWRITEBYTECODE=1 \
    PYTHONUNBUFFERED=1

WORKDIR /work
ENTRYPOINT ["python3", "/opt/laya/scripts/laya_train/container/run_step.py"]
CMD ["pack"]
