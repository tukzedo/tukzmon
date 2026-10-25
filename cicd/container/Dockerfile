# The image cicd.bash builds and tests in: the Rust toolchain from
# rust-toolchain.toml, Qt 6 Widgets for tukzmon-settings, the GTK 3 and panel
# libraries the plugin links by soname, and the lint and audit tools at the
# versions cicd/config.bash pins.
#
# cicd.bash builds it from this dir plus rust-toolchain.toml and the build
# args in cicd/config.bash, and tags it with a hash of them, so an edit to any
# of them means a new image.
# From tukfs-hash's and shcl's cicd/container/Dockerfile.
#
# Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)
# SPDX-License-Identifier: MIT

FROM debian:trixie-slim@sha256:a29215f6a35e51e22adffa17f89e9d2ef06214e64a2bad10d765c46aea49f11f

# Apt reads the snapshot the base image was cut from, so a point release
# can't move Qt or GTK under the build.
ARG APT_SNAP=20261005T000000Z
RUN sed -i -e "s|^URIs: http://deb.debian.org/debian-security|URIs: http://snapshot.debian.org/archive/debian-security/${APT_SNAP}|" \
		-e "s|^URIs: http://deb.debian.org/debian\$|URIs: http://snapshot.debian.org/archive/debian/${APT_SNAP}|" \
		-e 's|^Signed-By:|Check-Valid-Until: no\nSigned-By:|' /etc/apt/sources.list.d/debian.sources \
	&& ! grep -q deb.debian.org /etc/apt/sources.list.d/debian.sources

# g++ for the cxx bridge and qt.cpp. The plugin links GTK, cairo and the panel
# library by soname, so their runtime packages are enough. No -dev for those.
RUN apt-get update \
	&& apt-get install -y --no-install-recommends \
		bash ca-certificates curl git xz-utils \
		gcc g++ libc6-dev pkg-config \
		qt6-base-dev \
		libgtk-3-0t64 libcairo2 libglib2.0-0t64 libxfce4panel-2.0-4 \
	&& rm -rf /var/lib/apt/lists/*

ARG SHELLCHECK_VER=0.11.0
ARG SHELLCHECK_SHA256=8c3be12b05d5c177a04c29e3c78ce89ac86f1595681cab149b65b97c4e227198
# Only here for markdownlint-cli, which wants 22 or later.
ARG NODE_VER=22.23.2
ARG NODE_SHA256=d60acfe00a2932254bb0ad20e01b0d74397a0875595de719654b214f4b03f307
WORKDIR /tmp/dl
RUN curl -fsSLo sc.txz "https://github.com/koalaman/shellcheck/releases/download/v${SHELLCHECK_VER}/shellcheck-v${SHELLCHECK_VER}.linux.x86_64.tar.xz" \
	&& echo "${SHELLCHECK_SHA256}  sc.txz" | sha256sum -c - \
	&& tar -C /usr/local/bin --strip-components=1 -xJf sc.txz "shellcheck-v${SHELLCHECK_VER}/shellcheck" \
	&& curl -fsSLo node.txz "https://nodejs.org/dist/v${NODE_VER}/node-v${NODE_VER}-linux-x64.tar.xz" \
	&& echo "${NODE_SHA256}  node.txz" | sha256sum -c - \
	&& mkdir /opt/node && tar -C /opt/node --strip-components=1 -xJf node.txz \
	&& cd / && rm -rf /tmp/dl
WORKDIR /

# markdownlint-cli and its dependencies from the lock file, each by its
# integrity hash.
COPY package.json package-lock.json /opt/mdl/
RUN cd /opt/mdl && PATH="/opt/node/bin:${PATH}" npm ci --no-fund --no-audit --ignore-scripts \
	&& ln -s /opt/mdl/node_modules/.bin/markdownlint /usr/local/bin/markdownlint

# The toolchain and components rust-toolchain.toml names, so rustup never
# reaches for the network at run time.
ARG RUSTUP_VER=1.29.1
ARG RUSTUP_SHA256=dda7234360b7f578ca8b0ddcb80145646fa61a67c1720a5abc7051b35c9fcb71
ENV RUSTUP_HOME=/opt/rustup
COPY rust-toolchain.toml /opt/rustup-pin/
RUN curl -fsSLo /tmp/rustup-init "https://static.rust-lang.org/rustup/archive/${RUSTUP_VER}/x86_64-unknown-linux-gnu/rustup-init" \
	&& echo "${RUSTUP_SHA256}  /tmp/rustup-init" | sha256sum -c - \
	&& chmod +x /tmp/rustup-init \
	&& export CARGO_HOME=/opt/cargo \
	&& /tmp/rustup-init -y --no-modify-path --profile minimal --default-toolchain none \
	&& cd /opt/rustup-pin \
	&& /opt/cargo/bin/rustup toolchain install \
	&& /opt/cargo/bin/rustup default "$(/opt/cargo/bin/rustup show active-toolchain | cut -d' ' -f1)" \
	&& rm -rf /opt/cargo/registry /tmp/rustup-init

# cargo-deny for the known-advisory scan. Its own layer, so a bump doesn't
# rebuild the ones above.
ARG CARGO_DENY_VER=0.19.9
ARG CARGO_DENY_SHA256=f1f8eedc2a3ac297c540873f93785d4104b102c0079506b2a6b3221b7ec956af
RUN curl -fsSLo /tmp/cd.tgz "https://github.com/EmbarkStudios/cargo-deny/releases/download/${CARGO_DENY_VER}/cargo-deny-${CARGO_DENY_VER}-x86_64-unknown-linux-musl.tar.gz" \
	&& echo "${CARGO_DENY_SHA256}  /tmp/cd.tgz" | sha256sum -c - \
	&& tar -C /usr/local/bin --strip-components=1 -xzf /tmp/cd.tgz "cargo-deny-${CARGO_DENY_VER}-x86_64-unknown-linux-musl/cargo-deny" \
	&& rm -f /tmp/cd.tgz

# The sanitizer step. clang builds the C++ with the sanitizers, since rustc's
# runtime is LLVM's, and llvm-symbolizer puts names in the reports. Then the
# dated nightly cicd/config.bash pins, with rust-src for -Zbuild-std. It
# comes in as a build arg, so that way it's pinned in one place.
RUN apt-get update \
	&& apt-get install -y --no-install-recommends clang-19 llvm-19 \
	&& rm -rf /var/lib/apt/lists/*
ARG RUST_NIGHTLY
RUN test -n "${RUST_NIGHTLY}" \
	&& CARGO_HOME=/opt/cargo /opt/cargo/bin/rustup toolchain install "${RUST_NIGHTLY}" --profile minimal --component rust-src \
	&& rm -rf /opt/cargo/registry

# Runs as the caller's uid, which has no passwd entry here. CARGO_HOME, HOME
# and TMPDIR are set per run to dirs mounted from the host.
ENV PATH=/opt/cargo/bin:/opt/node/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin \
	LANG=C.UTF-8
