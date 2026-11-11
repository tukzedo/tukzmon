# shellcheck shell=bash

##	Purpose:
##		- fLintMarkdown, the markdownlint the Lint step and the pre-push hook
##		  share. Source it after config.bash, and define fLintRun, which runs
##		  each markdownlint call. Paths are relative to the repo root, which
##		  is where it runs from.
##	History: At bottom of file.

##	Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)
##	Licensed under The MIT License (MIT). Full text at:
##		https://mit-license.org/
##	SPDX-License-Identifier: MIT


## One markdownlint per set of rules it skips, MARKDOWNLINT_DISABLE plus a
## file's own in MARKDOWNLINT_DISABLE_FOR, since it starts slow.
fLintMarkdown() {   ## fLintMarkdown FILE...
	local f entry off i rc=0
	local -a files=("$@") offOf=() sets=() group=() rules=()
	local -A known=()
	for f in "${files[@]}"; do
		off="${MARKDOWNLINT_DISABLE[*]}"
		for entry in "${MARKDOWNLINT_DISABLE_FOR[@]}"; do [[ "${entry%%|*}" != "${f}" ]] || off+=" ${entry#*|}"; done
		offOf+=("${off}")
		[[ -n "${known[=${off}]:-}" ]] || { known["=${off}"]=1; sets+=("${off}"); }
	done
	for off in "${sets[@]}"; do
		group=()
		for i in "${!files[@]}"; do [[ "${offOf[i]}" != "${off}" ]] || group+=("${files[i]}"); done
		read -r -a rules <<< "${off}"
		fLintRun markdownlint --disable "${rules[@]}" -- "${group[@]}" || rc=1
	done
	return "${rc}"
}


##	History:
##		- 2026-11-11: Moved out of cicd.bash, so the pre-push hook lints a
##		  docs-only push the same way.
