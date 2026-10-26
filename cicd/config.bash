# shellcheck shell=bash
# shellcheck disable=2034  ## 'appears unused.' Read by cicd.bash and hooks/pre-push, which source this.

##	Purpose:
##		- tukzmon's settings for cicd.bash and the pre-push hook. Plain
##		  assignments only, since the hook sources it too.
##		- Tool pins live here, all in one place. The Rust toolchain is pinned
##		  in rust-toolchain.toml, since rustup reads it from there. Bump a pin
##		  on purpose: change it, run the full pipeline, commit.
##	History: At bottom of file.

##	Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)
##	Licensed under The MIT License (MIT). Full text at:
##		https://mit-license.org/
##	SPDX-License-Identifier: MIT


projectName="tukzmon"

## "name|version|command that prints it". A box with something else gets a
## warning, so a lint result that moved can be traced to the tool.
TOOL_PINS=(
	"shellcheck|0.11.0|shellcheck --version"
	## markdownlint-cli, the `markdownlint` command, not markdownlint-cli2.
	"markdownlint|0.49.1|markdownlint --version"
	## The known-advisory scan. Pinned in the image's Dockerfile too.
	"cargo-deny|0.19.9|cargo deny --version"
	## The .deb and .rpm. Pinned in the image's Dockerfile too. A go-installed
	## one says it's version "dev", so a box with one gets the warning.
	"nfpm|2.47.0|nfpm --version"
)
## The sanitizers and Miri need nightly, so they run on this dated one, beside
## the stable toolchain rust-toolchain.toml pins. The image gets it as a build
## arg, with rust-src and miri.
RUST_NIGHTLY="nightly-2026-10-16"

## Paths that don't change the product. A push to main that changes only
## these skips the merge gate. Bash patterns, matched against the whole path.
DOCS_PATHS=(
	'*.md'
	'project/*'
	'license.txt'
)

## The release build dogfood copies from, and the dogfood itself.
RELEASE_PACKAGES=(-p tukzmon-xfce -p tukzmon-settings)
DOGFOOD_CMD=(utility/dev-install.bash)

## Rules markdownlint skips. Every doc is one line per paragraph, so MD013 is
## always off. The backlog's next-cut marker is bold on purpose.
MARKDOWNLINT_DISABLE=(MD013)
MARKDOWNLINT_DISABLE_FOR=(
	"project/backlog.md|MD036"
)

## The .deb, .rpm and sha256 sums from cicd/packaging/nfpm.yaml, one set per
## version, gitignored.
PACKAGE_DIR="cicd/artifacts/packages"

## The leak checks, "name|step function|length". The length is by measured
## time: fast ones run in every run, slow ones in all but --quick and
## --backlog-run-quick, and hours ones only in --long. --leakcheck adds every
## one to any run, and --no-leakcheck takes every one out. The name, in lower
## case, is its --backlog-run-quick area.
LEAK_CHECKS=(
	## The tests under ASan and LSan, then tukzmon-settings --snapshot with
	## its C++ under UBSan too. 1m 25s from a fresh target dir and about 5 s
	## with nothing to rebuild, at 8 jobs and load 14 (2026-10-25).
	"Sanitizers|fStepSanitizers|fast"
	## The tests under Miri, one Miri per test. About 7 hours of CPU, so
	## nearly an hour at 8 at a time, which would push the default run past
	## its hour (2026-10-25).
	"Miri|fStepMiri|hours"
	## The soak test, core/tests/soak.rs. About 1 minute in a debug build.
	"Soak|fStepSoak|slow"
)
## Steps that get a quarter of the CPUs, busy or not.
HEAVY_STEPS=(fStepSanitizers fStepMiri fStepSoak)
## The test targets Miri runs, as "package target-option". The soak test is
## left out, since it's already ignored and its allocator is dhat's.
MIRI_TARGETS=(
	"tukzmon-core --lib"
	"knobs --lib"
	"tukzmon-settings --bins"
)
## The soak test's reports, and dhat's heap profile when it fails, rotated.
SOAK_DIR="cicd/artifacts/soak"
## LeakSanitizer's suppressions. Prebuilt libraries only, by name.
LSAN_SUPP="cicd/lsan.supp"

## Rotated lint logs, gitignored. utility/lint-report.bash reads them.
LINT_LOG_DIR="cicd/artifacts/lint"

## The build image: CTR_DIR plus CTR_CONTEXT_EXTRA, tagged with a hash of them
## all. The steps in CTR_STEPS run in it, and the rest on the host. A test that
## needs a real panel or display gets a step of its own, left out of the list.
CTR_IMAGE="tukzmon-cicd"
CTR_DIR="cicd/container"
CTR_CONTEXT_EXTRA=(rust-toolchain.toml)
## NAME=value, passed to docker build and part of the tag's hash.
CTR_BUILD_ARGS=("RUST_NIGHTLY=${RUST_NIGHTLY}")
CTR_STEPS=(fStepFormat fStepBuild fStepLint fStepAdvisories fStepTests fStepRelease fStepPackage fStepSanitizers fStepMiri fStepSoak)
## The image steps that get the network. The rest run with none.
CTR_NET_STEPS=(fStepAdvisories)


##	History:
##		- 2026-10-25: Created.
##		- 2026-10-25: The build image's settings.
##		- 2026-10-25: cargo-deny pin, and the advisory step in the image.
##		- 2026-10-25: The nightly pin, the leak checks and their lengths, and
##		  the image's build args.
##		- 2026-10-25: Miri.
##		- 2026-10-25: The soak test.
##		- 2026-10-25: Packages: nfpm's pin, and where they go.
