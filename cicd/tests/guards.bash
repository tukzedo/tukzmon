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

## Test ID: den97
fTest_test_ids_reads_each_file_on_its_own() {
	local t="${work}/ids" out='' want=''
	mkdir -p -- "${t}/cicd/utility" "${t}/cicd/tests" "${t}/a/src" "${t}/a/tests"
	cp -- "${cicd}/utility/test-ids.bash" "${t}/cicd/utility/"
	printf '[workspace]\nmembers = ["a"]\n' > "${t}/Cargo.toml"
	printf '[package]\nname = "a-pkg"\n' > "${t}/a/Cargo.toml"
	printf 'mod tests {\n\t// Test ID: dek00\n\t#[test]\n\tfn one() {}\n}\n' > "${t}/a/src/lib.rs"
	## Ends inside its mod, with a fn past where the next file's lines stop.
	printf 'mod deep {\n\t// Test ID: dek01\n\t#[test]\n\tfn two() {}\n\tfn stale() {}\n' > "${t}/a/src/m.rs"
	: > "${t}/a/src/n.rs"
	printf '#[test]\n' > "${t}/a/src/o.rs"
	printf '// Test ID: dek02\n#[test]\nfn three() {}\n' > "${t}/a/tests/t.rs"
	printf 'fTest_x() { :; }\n## Test ID: dek03\n' > "${t}/cicd/tests/w.bash"
	printf 'fTest_four() { :; }\n' > "${t}/cicd/tests/x.bash"
	want="$(printf '%s\n' 'dek00 tests::one' 'dek01 m::deep::two' '- o::?' 'dek02 three' '- fTest_x' '- fTest_four')"
	out="$(bash "${t}/cicd/utility/test-ids.bash" list 2>&1)" || { fWhy "list failed: ${out}"; return 1; }
	[[ "${out}" == "${want}" ]] || { fWhy "got:"; printf '%s\n' "${out}" | sed 's/^/      /'; return 1; }
}

## Test ID: deqrb
fTest_run_pids_skips_a_process_gone_since_the_grep() {
	local dir="${work}/pids" live='' out='' ok=0
	mkdir -p -- "${dir}"
	TMPDIR="${dir}" sleep 30 &
	live="$!"
	## Its environ is the fork's until sleep starts.
	for _ in 1 2 3 4 5 6 7 8 9 10; do ! grep -qszF -- "TMPDIR=${dir}" "/proc/${live}/environ" || break; sleep 0.1; done
	## A bash of its own, since the if that runs each test turns off errexit
	## and the ERR trap all the way down. It runs as cicd.bash does, and its
	## grep names a process that's gone, then one that's live.
	printf '%s\n' 'set -Eeuo pipefail' "trap 'echo ABORTED; exit 1' ERR" 'projectName=tukzmon' \
		'source "${1}/utility/include/run-tmp.bash"' \
		'live="${3}"' 'grep() { printf "%s\n" /proc/999999999/environ "/proc/${live}/environ"; }' 'fRunPids "${2}"' > "${dir}/scan.bash"
	out="$(bash "${dir}/scan.bash" "${cicd}" "${dir}" "${live}" 2>&1)" && ok=1
	kill "${live}" 2> /dev/null || true
	wait "${live}" 2> /dev/null || true
	((ok)) || { fWhy "the scan failed: ${out}"; return 1; }
	[[ "${out}" == "${live}" ]] || { fWhy "got '${out}', not the live pid ${live}"; return 1; }
}

## Test ID: deqrc
fTest_lists_of_files_fail_when_git_does_or_a_kind_has_none() {
	local t="${work}/tree" ok=0
	mkdir -p -- "${t}/none" "${t}/repo"
	(
		## As cicd.bash has them, since sourcing it runs it.
		eval "$(awk '/^(fTreeFiles|fSomeOf)\(\)/, /^}/' "${cicd}/cicd.bash")"
		fEcho_Clean() { :; }
		local -a got=()
		## The folder above isn't a repo, as one that git refuses isn't.
		cd "${t}/none" && export GIT_CEILING_DIRECTORIES="${t}"
		if fTreeFiles got 2> /dev/null; then fWhy "listed ${#got[@]} file(s) with no repo"; exit 1; fi
		cd "${t}/repo" && git init -q . && printf 'x\n' > a.md && printf 'y\n' > b.py && git add a.md
		fTreeFiles got '*.md' '*.py' || { fWhy "failed in a repo"; exit 1; }
		[[ "$(printf '%s\n' "${got[@]}" | LC_ALL=C sort | paste -sd ' ')" == "a.md b.py" ]] || { fWhy "listed '${got[*]}'"; exit 1; }
		fSomeOf Python 1 || { fWhy "refused a kind with 1 file"; exit 1; }
		if fSomeOf Python 0; then fWhy "took a kind with none"; exit 1; fi
	) && ok=1
	((ok))
}

## Test ID: deqrd
fTest_test_ids_check_fails_when_git_cant_list_the_rs_files() {
	local t="${work}/ids-nogit" out=''
	mkdir -p -- "${t}/cicd/utility" "${t}/a/src"
	cp -- "${cicd}/utility/test-ids.bash" "${t}/cicd/utility/"
	printf '[workspace]\nmembers = ["a"]\n' > "${t}/Cargo.toml"
	printf '[package]\nname = "a-pkg"\n' > "${t}/a/Cargo.toml"
	printf 'mod tests {\n\t// Test ID: dek00\n\t#[test]\n\tfn one() {}\n}\n' > "${t}/a/src/lib.rs"
	if out="$(GIT_CEILING_DIRECTORIES="${work}" bash "${t}/cicd/utility/test-ids.bash" check 2>&1)"; then fWhy "passed with no repo: ${out}"; return 1; fi
	[[ "${out}" == *"so none was checked for a stray test"* ]] || { fWhy "got: ${out}"; return 1; }
}

## Test ID: deqre
fTest_nix_step_removes_only_its_own_stores_that_no_checkout_pins() {
	local t="${work}/nix-stores" ok=0
	mkdir -p -- "${t}/a/cicd" "${t}/b/cicd"
	printf 'NIX_IMAGE="nixos/nix:1.0"\n' > "${t}/a/cicd/config.bash"
	printf '## a note\nNIX_IMAGE="nixos/nix:2.0"\n' > "${t}/b/cicd/config.bash"
	(
		eval "$(awk '/^(fNixStore|fNixStoresPinned|fNixStoresUnused)\(\)/, /^}/' "${cicd}/cicd.bash")"
		## 2 checkouts, and one that's gone.
		git() {
			if [[ "${1}" != worktree ]]; then command git "$@"; return; fi
			((! gitFails)) || return 128
			printf 'worktree %s\nHEAD 0\n\n' "${t}/a" "${t}/b" "${t}/gone"
		}
		local gitFails=0 keep='' one='' two='' old='' would=''
		one="$(fNixStore nixos/nix:1.0)"; two="$(fNixStore nixos/nix:2.0)"; old="$(fNixStore nixos/nix:0.9)"
		keep="$(fNixStoresPinned)" || { fWhy "the pins failed"; exit 1; }
		[[ "${keep}" == "${one}"$'\n'"${two}" ]] || { fWhy "pinned: ${keep//$'\n'/ }"; exit 1; }
		would="$(fNixStoresUnused "${keep//$'\n'/ }" "${one}" "${old}" tukzmon-cache "${two}" tukzmon-nix-notahash)"
		[[ "${would}" == "${old}" ]] || { fWhy "would remove: ${would//$'\n'/ }"; exit 1; }
		gitFails=1
		if fNixStoresPinned > /dev/null; then fWhy "listed pins when git failed"; exit 1; fi
	) && ok=1
	((ok))
}

failed=0
while read -r id fn; do
	if "${fn}"; then echo "PASS ${id} ${fn#fTest_}"; else echo "FAIL ${id} ${fn#fTest_}"; failed=1; fi
done < <(awk '/^fTest_[A-Za-z0-9_]+\(\)/ { fn = $0; sub(/\(.*/, "", fn); id = "-----"; if (prev ~ /^## Test ID:/) id = prev; sub(/^## Test ID:[ \t]*/, "", id); print id, fn } { prev = $0 }' "${BASH_SOURCE[0]}")
exit "${failed}"


##	History:
##		- 2026-10-30: Created, with the safe removal guard and the Nix check's
##		  env values.
##		- 2026-10-30: test-ids.bash keeps each file's modules and lines to
##		  itself.
##		- 2026-11-01: fRunPids skips a process that exits while it scans.
##		- 2026-11-01: The lists of files fail when git can't list them.
##		- 2026-11-01: The Nix step keeps the stores other checkouts pin.
