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

## Test ID: deqvk
fTest_no_file_a_package_holds_counts_as_docs_for_the_hook() {
	local f='' pat='' n=0 ok=0
	(
		source "${cicd}/config.bash"
		## The literal ones. A ${TUKZMON_*} one is a build output.
		while IFS= read -r f; do
			[[ -n "${f}" && "${f}" != *'${'* ]] || continue
			n=$((n + 1))
			for pat in "${DOCS_PATHS[@]}"; do
				# shellcheck disable=2053  ## The pattern is meant to glob, as in the hook.
				if [[ "${f}" == ${pat} ]]; then fWhy "${f} goes in a package, and matches DOCS_PATHS '${pat}'"; exit 1; fi
			done
		done < <(sed -n 's/^[[:space:]]*-[[:space:]]*src:[[:space:]]*//p' "${cicd}/packaging/nfpm.yaml")
		((n)) || { fWhy "found no src in nfpm.yaml"; exit 1; }
	) && ok=1
	((ok))
}

## Test ID: deqvm
fTest_a_gate_the_hook_started_doesnt_count_the_hook_as_another_run() {
	local t="${work}/other-run" live='' out='' ok=0
	mkdir -p -- "${t}/tukzmon/20261101-000000_001" "${t}/tukzmon/20261101-000000_002" "${t}/tukzmon/20261101-000000_003"
	## _001 is the test's own bash, which starts the scan as the hook starts
	## the gate. _002 is a live run that isn't above it. _003 is the scan's own.
	fPidStamp "$$" > "${t}/tukzmon/20261101-000000_001/run.pid"
	sleep 30 &
	live="$!"
	fPidStamp "${live}" > "${t}/tukzmon/20261101-000000_002/run.pid"
	printf '%s\n' 'set -Eeuo pipefail' 'HOST_TEMP="${2}"; projectName=tukzmon' \
		'source "${1}/utility/include/run-tmp.bash"' \
		"eval \"\$(awk '/^fOtherRun\\(\\)/, /^}/' \"\${1}/cicd.bash\")\"" \
		'fCtrOthers() { :; }' 'runTmp="${HOST_TEMP}/tukzmon/20261101-000000_003"' \
		'fPidStamp "${BASHPID}" > "${runTmp}/run.pid"' 'fOtherRun' > "${t}/scan.bash"
	out="$(bash "${t}/scan.bash" "${cicd}" "${t}" 2>&1)" && ok=1
	((ok)) && [[ "${out}" == 20261101-000000_002 ]] || ok=0
	kill "${live}" 2> /dev/null || true
	wait "${live}" 2> /dev/null || true
	((ok)) || { fWhy "with a live run beside it, got '${out}', not 20261101-000000_002"; return 1; }
	fSafeRm "${t}/tukzmon/20261101-000000_002" || return 1
	out="$(bash "${t}/scan.bash" "${cicd}" "${t}" 2>&1)" || { fWhy "the scan failed: ${out}"; return 1; }
	[[ -z "${out}" ]] || { fWhy "counted '${out}' as another run, with only the one above it live"; return 1; }
}

## Test ID: deqvn
fTest_only_1_skips_the_pre_push_hook() {
	local t="${work}/skip" v='' out=''
	mkdir -p -- "${t}/bin"
	## The hook's first git call comes right after the skip, so a git that
	## leaves a mark says it went on.
	printf '#!/bin/sh\necho went-on >> "%s"\n' "${t}/mark" > "${t}/bin/git"
	chmod +x "${t}/bin/git"
	for v in '' 0 yes 1; do
		: > "${t}/mark"
		out="$(TUKZMON_SKIP_HOOK="${v}" PATH="${t}/bin:${PATH}" bash "${cicd}/hooks/pre-push" < /dev/null 2>&1)" || { fWhy "'${v}' failed: ${out}"; return 1; }
		if [[ "${v}" == 1 ]]; then
			[[ ! -s "${t}/mark" ]] || { fWhy "1 didn't skip"; return 1; }
		else
			[[ -s "${t}/mark" ]] || { fWhy "'${v}' skipped"; return 1; }
		fi
		if [[ "${v}" == yes ]]; then [[ "${out}" == *'not 1, so the gate still runs'* ]] || { fWhy "yes gave no note: ${out}"; return 1; }; fi
	done
}

## Test ID: deqvr
fTest_dev_install_copies_what_cargo_built_and_finds_the_panel_library() {
	local t="${work}/dev-install" out='' f='' lib=''
	mkdir -p -- "${t}/home" "${t}/bin" "${t}/target"
	## A cargo whose target dir is elsewhere, and an ldconfig on an arm64 box.
	printf '%s\n' '#!/bin/sh' 'case "$1" in' \
		"metadata) printf '{\"target_directory\":\"%s\"}\\n' '${t}/target' ;;" \
		"build) echo \"\$*\" > '${t}/build-args'; mkdir -p '${t}/target/release'; for f in libtukzmon.so tukzmon-settings; do echo built > '${t}/target/release/'\"\$f\"; done ;;" \
		'*) exit 1 ;;' 'esac' > "${t}/bin/cargo"
	printf '%s\n' '#!/bin/sh' 'printf "\tlibxfce4panel-2.0.so.4 (libc6,AArch64) => /usr/lib/aarch64-linux-gnu/libxfce4panel-2.0.so.4\n"' > "${t}/bin/ldconfig"
	printf '#!/bin/sh\n' > "${t}/bin/xfconf-query"
	chmod +x "${t}/bin/"*
	out="$(env -u XDG_DATA_HOME -u CARGO_TARGET_DIR HOME="${t}/home" PATH="${t}/bin:${PATH}" bash "${cicd}/../utility/dev-install.bash" --no-restart 2>&1)" || { fWhy "failed: ${out}"; return 1; }
	lib="${t}/home/.local/lib/aarch64-linux-gnu/xfce4/panel/plugins"
	for f in "${lib}/libtukzmon.so" "${lib}/tukzmon-settings"; do
		cmp -s -- "${f}" <(echo built) || { fWhy "${f} isn't what cargo built: ${out}"; return 1; }
	done
	[[ -s "${t}/home/.local/share/xfce4/panel/plugins/tukzmon.desktop" ]] || { fWhy "no .desktop file"; return 1; }
	[[ " $(cat -- "${t}/build-args") " == *' --locked '* ]] || { fWhy "built without --locked: $(cat -- "${t}/build-args")"; return 1; }
	## With a Debian user's PATH and the box's own ldconfig, when it knows
	## the panel's library.
	mv -- "${t}/bin/ldconfig" "${t}/ldconfig.off"
	for f in /usr/sbin/ldconfig /sbin/ldconfig; do
		if [[ ! -x "${f}" ]] || ! grep -qF 'libxfce4panel-2.0.so.4 ' <<< "$("${f}" -p 2> /dev/null || true)"; then continue; fi
		out="$(env -u XDG_DATA_HOME HOME="${t}/home" PATH="${t}/bin:/usr/bin:/bin" bash "${cicd}/../utility/dev-install.bash" --no-restart 2>&1)" || { fWhy "failed with a user PATH: ${out}"; return 1; }
		[[ "${out}" != *'libxfce4panel-2.0.so.4 not found'* ]] || { fWhy "didn't find ldconfig with a user PATH: ${out}"; return 1; }
		break
	done
}

## Test ID: der4k
fTest_a_run_with_every_check_off_fails() {
	local out='' rc=0
	## It stops while planning, before it makes a run folder or calls docker.
	out="$(env -u TUKZMON_IN_IMAGE bash "${cicd}/cicd.bash" --backlog-run-quick miri --no-leakcheck 2>&1)" || rc=$?
	((rc == 2)) || { fWhy "exit ${rc}, not 2: ${out}"; return 1; }
	[[ "${out}" == *"every check is off"* ]] || { fWhy "got: ${out}"; return 1; }
	out="$(env -u TUKZMON_IN_IMAGE bash "${cicd}/cicd.bash" --backlog-run-quick miri --no-leakcheck --list 2>&1)" || { fWhy "--list failed: ${out}"; return 1; }
	[[ "${out}" == *"Fails: every check is off"* ]] || { fWhy "--list didn't say it fails: ${out}"; return 1; }
	out="$(env -u TUKZMON_IN_IMAGE bash "${cicd}/cicd.bash" --backlog-run-quick miri --list 2>&1)" || { fWhy "--list with Miri on failed: ${out}"; return 1; }
	[[ "${out}" != *"Fails:"* ]] || { fWhy "--list with Miri on says it fails: ${out}"; return 1; }
}

## Test ID: dexn3
fTest_a_miri_module_names_the_test_that_stopped_it() {
	local dir="${work}/miri-group" log='' rc=0 c=''
	mkdir -p -- "${dir}/bin"
	## cargo stands in for Miri: it prints STUB_OUT, with a warning on stderr
	## where the first one says, and exits STUB_RC.
	printf '#!/usr/bin/env bash\nprintf "     Running unittests (x/pkg-0a1b)\\n" >&2\nprintf "%%b" "${STUB_OUT%%%%|*}"\nprintf "warning: files in /proc\\nerror: memory leaked\\n" >&2\nprintf "%%b" "${STUB_OUT#*|}"\nexit "${STUB_RC}"\n' > "${dir}/bin/cargo"
	chmod +x -- "${dir}/bin/cargo"
	fRun() {   ## fRun NUM OUT RC -> the group's exit in rc, its log in log
		rc=0; log="${dir}/$1.log"
		PATH="${dir}/bin:${PATH}" STUB_OUT="$2" STUB_RC="$3" NIGHTLY=x LOCKED='' MINUTES=1 \
			bash "${cicd}/utility/miri-group.bash" "${dir}" "$1" pkg --lib m::a m::b m::c || rc=$?
	}
	fRun 0001 'test m::a ... |ok\ntest m::b ... ok\ntest m::c ... ignored\n' 0
	((rc == 0)) || { fWhy "a clean run failed: $(cat -- "${log}")"; return 1; }
	## a warning between a test and its result doesn't split it, and the
	## crate's line comes first, with no error text on a clean run
	[[ "$(cat -- "${log}")" == "     Running unittests (x/pkg-0a1b)"$'\n'"test m::a ... ok"$'\n'* ]] || { fWhy "a clean run's log: $(cat -- "${log}")"; return 1; }
	grep -q '^error' "${log}" && { fWhy "a clean run has error text"; return 1; }
	## Past the limit: b was running, and c never ran.
	fRun 0002 'test m::a ... ok\ntest m::b ... |' 124
	((rc == 1)) || { fWhy "past the limit gave ${rc}"; return 1; }
	for c in 'm::b ran past 1 min under Miri, with the rest of m' 'test m::b ... FAILED' 'test m::c ... ignored, not run since m::b stopped Miri'; do
		grep -qxF -- "${c}" "${log}" || { fWhy "past the limit, no '${c}' in: $(cat -- "${log}")"; return 1; }
	done
	grep -qF 'test m::a ... FAILED' "${log}" && { fWhy "a, which finished, failed too"; return 1; }
	## b was cut off mid-line, and what follows still starts a line of its own
	grep -qx 'error: memory leaked' "${log}" || { fWhy "b's cut-off line ran on: $(cat -- "${log}")"; return 1; }
	## A leak comes after every test, so the module fails.
	fRun 0003 'test m::a ... ok\ntest m::b ... ok\ntest m::c ... ok\n|' 1
	((rc == 1)) || { fWhy "a leak gave ${rc}"; return 1; }
	grep -qx 'error: memory leaked' "${log}" || { fWhy "a leak's error isn't in: $(cat -- "${log}")"; return 1; }
	grep -qF 'FAIL ----- m: Miri failed after its tests, exit 1' "${log}" || { fWhy "a leak: $(cat -- "${log}")"; return 1; }
	## A failed assert has its own line, and nothing is added.
	fRun 0004 'test m::a ... FAILED\ntest m::b ... ok\ntest m::c ... ok\n|' 101
	((rc == 1)) || { fWhy "a failed test gave ${rc}"; return 1; }
	## the Running line, 3 results and the stand-in's error line
	(($(wc -l < "${log}") == 5)) || { fWhy "a failed test got more lines: $(cat -- "${log}")"; return 1; }
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
##		- 2026-11-01: No file in a package counts as docs for the hook.
##		- 2026-11-01: The gate doesn't count the hook that started it as another
##		  run, and only 1 skips the hook.
##		- 2026-11-01: dev-install.bash copies what cargo built and finds the
##		  panel's library.
##		- 2026-11-01: A run with every check off fails.
##		- 2026-11-05: A Miri module names the test that stopped it.
