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
)

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

## Rotated lint logs, gitignored. utility/lint-report.bash reads them.
LINT_LOG_DIR="cicd/artifacts/lint"

## The build image: CTR_DIR plus CTR_CONTEXT_EXTRA, tagged with a hash of them
## all. The steps in CTR_STEPS run in it, and the rest on the host. A test that
## needs a real panel or display gets a step of its own, left out of the list.
CTR_IMAGE="tukzmon-cicd"
CTR_DIR="cicd/container"
CTR_CONTEXT_EXTRA=(rust-toolchain.toml)
CTR_STEPS=(fStepFormat fStepBuild fStepLint fStepTests fStepRelease)


##	History:
##		- 2026-10-25: Created.
##		- 2026-10-25: The build image's settings.
