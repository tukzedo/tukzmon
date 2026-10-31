#!/usr/bin/env bash
#  shellcheck disable=1091  ## 'source is valid here, but shellcheck doesn't know the path to it.'
#  shellcheck disable=2016  ## 'Expressions don't expand in single quotes.' The $(...) given to check.bash mustn't expand here.
#  shellcheck disable=2154  ## 'referenced but not assigned.' runTmp is set by fRunTmpMake in the sourced run-tmp.bash.
#  shellcheck disable=2329  ## 'This function is never invoked.' The tests run by name, from this file's own list.
#  shellcheck enable=require-variable-braces  ## Every expansion braced: "${var}", not "$var".

##	Purpose:
##		Tests of the pipeline's own guards. Each fTest_ function is a test,
##		with its ID on the line above, and prints "PASS|FAIL ID name". Works
##		in a folder of its own under the run's temp folder, or under
##		<HOST_TEMP>/tukzmon when run by hand.
##	Syntax: cicd/tests/guards.bash
##	Exit: 0 every test passed, 1 one failed.
##	History: At bottom of file.

##	Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)
##	Licensed under The MIT License (MIT). Full text at:
##		https://mit-license.org/
##	SPDX-License-Identifier: MIT


set -Eeuo pipefail

cicd="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
projectName="tukzmon"
source "${cicd}/utility/include/run-tmp.bash"
fRunTmpMake || { echo "guards: could not make a temp folder under ${HOST_TEMP}/${projectName}" >&2; exit 1; }
work="${runTmp}"
trap 'fSafeRm "${work}" || true' EXIT

fWhy() { printf '    %s\n' "$*"; }

## Test ID: den8d
fTest_safe_rm_takes_a_run_folder_but_not_the_temp_root_or_a_project_folder() {
	local root="${work}/host" run='' p='' ok=0
	run="${root}/${projectName}/20261030-000000_001"
	mkdir -p -- "${run}/sub"
	## A subshell, so the guard sees this test's temp root and not the run's.
	(
		HOST_TEMP="${root}"
		source "${cicd}/utility/include/run-tmp.bash"
		for p in "${HOST_TEMP}" "${HOST_TEMP}/" "${HOST_TEMP}/${projectName}" "${HOST_TEMP}/${projectName}/"; do
			if fSafeRm "${p}" 2> /dev/null; then fWhy "took ${p}"; exit 1; fi
			[[ -d "${run}" ]] || { fWhy "the run folder went with ${p}"; exit 1; }
		done
		fSafeRm "${run}" || { fWhy "refused the run folder ${run}"; exit 1; }
		[[ ! -e "${run}" ]] || { fWhy "the run folder is still there"; exit 1; }
		[[ -d "${HOST_TEMP}/${projectName}" ]] || { fWhy "the project folder went with the run folder"; exit 1; }
	) && ok=1
	((ok))
}

## Test ID: den8e
fTest_nix_check_refuses_env_values_that_arent_numbers() {
	local mark="${work}/ran" out='' bad=''
	for bad in 'x[$(touch '"${mark}"')]' '8 9' '0'; do
		## A refused value stops it before it makes a folder or calls nix.
		if out="$(TUKZMON_JOBS=1 TUKZMON_NIX_CAP_MB="${bad}" bash "${cicd}/nix/check.bash" 2>&1)"; then fWhy "took cap '${bad}'"; return 1; fi
		[[ "${out}" == *"TUKZMON_NIX_CAP_MB isn't a size in MB"* ]] || { fWhy "cap '${bad}': ${out}"; return 1; }
		if out="$(TUKZMON_JOBS="${bad}" TUKZMON_NIX_CAP_MB=100 bash "${cicd}/nix/check.bash" 2>&1)"; then fWhy "took jobs '${bad}'"; return 1; fi
		[[ "${out}" == *"TUKZMON_JOBS isn't a job count"* ]] || { fWhy "jobs '${bad}': ${out}"; return 1; }
	done
	[[ ! -e "${mark}" ]] || { fWhy "a \$(...) in a value ran"; return 1; }
}

failed=0
while read -r id fn; do
	if "${fn}"; then echo "PASS ${id} ${fn#fTest_}"; else echo "FAIL ${id} ${fn#fTest_}"; failed=1; fi
done < <(awk '/^fTest_[A-Za-z0-9_]+\(\)/ { fn = $0; sub(/\(.*/, "", fn); id = "-----"; if (prev ~ /^## Test ID:/) id = prev; sub(/^## Test ID:[ \t]*/, "", id); print id, fn } { prev = $0 }' "${BASH_SOURCE[0]}")
exit "${failed}"


##	History:
##		- 2026-10-30: Created, with the safe removal guard and the Nix check's
##		  env values.
