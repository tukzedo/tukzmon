# shellcheck shell=bash
# shellcheck disable=2034  ## 'appears unused.' mapsOut__ is a nameref, read by the caller.

##	Purpose:
##		- The compilers' path maps for a release build, so its bytes don't
##		  depend on where the checkout, the target dir or the cargo home are.
##		  Panic locations and the C++ __FILE__s would hold them otherwise.
##		  settings/build.rs maps OUT_DIR and the crate for its own C++, but a
##		  crate's Rust and cxx's own C++ can only be mapped from outside.
##		- Sourced by cicd/cicd.bash and utility/dev-install.bash.
##	History: At bottom of file.

##	Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)
##	Licensed under The MIT License (MIT). Full text at:
##		https://mit-license.org/
##	SPDX-License-Identifier: MIT


## Fills ARRAY with RUSTFLAGS, CFLAGS and CXXFLAGS assignments, for env, with
## any the caller already has kept in front. Fails on a path with a space,
## since cc and RUSTFLAGS split on them.
fPathMaps() {   ## fPathMaps ARRAY REPO TARGET
	local -n mapsOut__="$1"
	local repo="$2" target="$3" cargoHome="${CARGO_HOME:-${HOME}/.cargo}" pair rust='' cc=''
	## The last match wins in rustc and gcc, and the target dir and the cargo
	## home can each be inside the one before.
	for pair in "${repo}=/src" "${target}=/target" "${cargoHome}=/cargo"; do
		[[ "${pair}" == /* && "${pair}" != *[[:space:]]* ]] || { printf 'path maps: no absolute path without spaces in "%s"\n' "${pair}" >&2; return 1; }
		rust+=" --remap-path-prefix=${pair}"
		cc+=" -ffile-prefix-map=${pair}"
	done
	mapsOut__=("RUSTFLAGS=${RUSTFLAGS:+${RUSTFLAGS} }${rust# }"
		"CFLAGS=${CFLAGS:+${CFLAGS} }${cc# }" "CXXFLAGS=${CXXFLAGS:+${CXXFLAGS} }${cc# }")
}


##	History:
##		- 2026-11-10: Created.
