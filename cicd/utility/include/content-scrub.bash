# shellcheck shell=bash

##	Purpose:
##		- fContentScrub, the content scrub the Lint step and the pre-push hook
##		  share. Source it after config.bash. The scrub script isn't in the
##		  repo. It sits at CONTENT_SCRUB beside the main checkout, so a fresh
##		  clone has none, and skips it with one line.
##	History: At bottom of file.

##	Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)
##	Licensed under The MIT License (MIT). Full text at:
##		https://mit-license.org/
##	SPDX-License-Identifier: MIT


## Prints where the scrub script should be for the tree at DIR. A linked
## worktree, such as the pre-push hook's, gets its main checkout's.
## TUKZMON_CONTENT_SCRUB names another, and the image steps get it that way.
fContentScrubPath() {   ## fContentScrubPath DIR
	local common='' path='' dir=''
	if [[ -n "${TUKZMON_CONTENT_SCRUB:-}" ]]; then printf '%s\n' "${TUKZMON_CONTENT_SCRUB}"; return 0; fi
	common="$(git -C "$1" rev-parse --path-format=absolute --git-common-dir 2> /dev/null)" || return 1
	path="$(dirname -- "${common}")/${CONTENT_SCRUB}"
	## Without the .., since docker takes the folder as a mount.
	if dir="$(cd -- "$(dirname -- "${path}")" 2> /dev/null && pwd)"; then path="${dir}/${path##*/}"; fi
	printf '%s\n' "${path}"
}

## Runs the scrub over the tree at DIR. A finding fails it when
## CONTENT_SCRUB_FAILS is 1. Otherwise it gets a count, and the findings go
## to LOG when one is given.
fContentScrub() {   ## fContentScrub DIR [LOG]
	local dir="$1" log="${2:-}" script='' out='' rc=0 n=0
	[[ "${log}" != /dev/null ]] || log=''
	script="$(fContentScrubPath "${dir}")" || script=''
	if [[ -z "${script}" || ! -f "${script}" ]]; then
		printf '%s\n' "content scrub: no script at ${script:-CONTENT_SCRUB}, so it's skipped"
		return 0
	fi
	out="$(bash "${script}" "${dir}" 2>&1)" || rc=$?
	if ((rc == 0)); then printf '%s\n' "content scrub: clean"; return 0; fi
	n="$(grep -c '' <<< "${out}" || true)"
	if [[ "${CONTENT_SCRUB_FAILS:-0}" == 1 ]]; then
		printf '%s\n' "${out}" "content scrub: exit ${rc}, ${n} line(s) of findings"
		return 1
	fi
	if [[ -n "${log}" ]]; then printf '%s\n' "${out}" >> "${log}"; fi
	printf '%s\n' "content scrub: exit ${rc}, ${n} line(s) of findings${log:+, in ${log}}, not failing while CONTENT_SCRUB_FAILS isn't 1"
	return 0
}


##	History:
##		- 2026-11-17: Created.
