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

## Test ID: dezax
fTest_dev_install_builds_with_rustups_cargo_from_the_repo_wherever_its_run() {
	local t="${work}/dev-install-pin" out='' pin='' repo=''
	repo="$(cd "${cicd}/.." && pwd)"
	pin="$(sed -n 's/^channel *= *"\(.*\)"/\1/p' "${repo}/rust-toolchain.toml")"
	mkdir -p -- "${t}/home/.cargo/bin" "${t}/bin" "${t}/target"
	## rustup's, which says where it ran, and Debian's ahead of it in PATH.
	printf '%s\n' '#!/bin/sh' 'case "$1" in' \
		"metadata) printf '{\"target_directory\":\"%s\"}\\n' '${t}/target' ;;" \
		"build) pwd > '${t}/build-dir'; mkdir -p '${t}/target/release'; for f in libtukzmon.so tukzmon-settings; do echo built > '${t}/target/release/'\"\$f\"; done ;;" \
		'*) exit 1 ;;' 'esac' > "${t}/home/.cargo/bin/cargo"
	printf '#!/bin/sh\necho "rustc %s (stub)"\n' "${pin}" > "${t}/home/.cargo/bin/rustc"
	printf '#!/bin/sh\ntouch "%s"\nexit 1\n' "${t}/debian-ran" > "${t}/bin/cargo"
	printf '#!/bin/sh\necho "rustc 1.95.0 (debian)"\n' > "${t}/bin/rustc"
	printf '%s\n' '#!/bin/sh' 'printf "\tlibxfce4panel-2.0.so.4 (libc6,x86-64) => /usr/lib/x86_64-linux-gnu/libxfce4panel-2.0.so.4\n"' > "${t}/bin/ldconfig"
	printf '#!/bin/sh\n' > "${t}/bin/xfconf-query"
	chmod +x "${t}/bin/"* "${t}/home/.cargo/bin/"*
	out="$(cd / && env -u XDG_DATA_HOME -u CARGO_TARGET_DIR HOME="${t}/home" PATH="${t}/bin:${PATH}" bash "${repo}/utility/dev-install.bash" --no-restart 2>&1)" || { fWhy "failed: ${out}"; return 1; }
	[[ ! -e "${t}/debian-ran" ]] || { fWhy "ran the cargo ahead of ~/.cargo/bin"; return 1; }
	[[ "$(cat -- "${t}/build-dir")" == "${repo}" ]] || { fWhy "built in $(cat -- "${t}/build-dir"), not ${repo}"; return 1; }
	[[ "${out}" != *WARNING* ]] || { fWhy "warned on the pinned ${pin}: ${out}"; return 1; }
	## No rustup rustc, so the build isn't on the pinned version.
	mv -- "${t}/home/.cargo/bin/rustc" "${t}/rustc.off"
	out="$(cd / && env -u XDG_DATA_HOME -u CARGO_TARGET_DIR HOME="${t}/home" PATH="${t}/bin:${PATH}" bash "${repo}/utility/dev-install.bash" --no-restart 2>&1)" || { fWhy "failed off the pin: ${out}"; return 1; }
	[[ "${out}" == *"building with 'rustc 1.95.0 (debian)', not the pinned ${pin}"* ]] || { fWhy "no warning off the pin: ${out}"; return 1; }
}

## Test ID: dezay
fTest_a_step_prints_the_rule_its_start_and_its_pass_or_fail() {
	local t="${work}/step-lines" out='' rc=0 rule='' f='' result=''
	mkdir -p -- "${t}/bin" "${t}/host"
	## Format's tools stand in, exiting STUB_RC, in a temp root of the test's own.
	for f in cargo clang-format black; do printf '#!/bin/sh\nexit "${STUB_RC:-0}"\n' > "${t}/bin/${f}"; done
	chmod +x -- "${t}/bin/"*
	rule="$(printf '%.0s•' {1..79})"
	for result in PASS FAIL; do
		rc=0
		out="$(env -u TUKZMON_IN_IMAGE STUB_RC="$([[ "${result}" == PASS ]] && echo 0 || echo 1)" HOST_TEMP="${t}/host" PATH="${t}/bin:${PATH}" \
			bash "${cicd}/cicd.bash" --backlog-run-quick format --no-container 2>&1)" || rc=$?
		[[ "${result}" == PASS ]] && ((rc != 0)) && { fWhy "a passing Format gave exit ${rc}: ${out}"; return 1; }
		[[ "${result}" == FAIL ]] && ((rc == 0)) && { fWhy "a failing Format passed: ${out}"; return 1; }
		## The rule, then the start line, and the result in the same form.
		grep -A1 -xF -- "${rule}" <<< "${out}" | grep -qE '^\[ [0-9]{8}-[0-9]{6}\.[0-9]{2}: Step 1 of 1; Starting Format \]$' \
			|| { fWhy "${result}: no rule and start line: ${out}"; return 1; }
		grep -qE "^\[ [0-9]{8}-[0-9]{6}\.[0-9]{2}: Step 1 of 1; ${result} Format \]$" <<< "${out}" || { fWhy "no ${result} line: ${out}"; return 1; }
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

## A tree of one crate in a repo of its own, for test-ids.bash check.
fIdsTree() {   ## fIdsTree DIR LIB_RS
	mkdir -p -- "$1/cicd/utility" "$1/a/src" "$1/project"
	cp -- "${cicd}/utility/test-ids.bash" "$1/cicd/utility/"
	printf '[workspace]\nmembers = ["a"]\n' > "$1/Cargo.toml"
	printf '[package]\nname = "a-pkg"\n' > "$1/a/Cargo.toml"
	printf '%b' "$2" > "$1/a/src/lib.rs"
	git -C "$1" init -q . && git -C "$1" add -- Cargo.toml a
}

## Test ID: dezan
fTest_test_ids_check_fails_on_each_kind_of_bad_id() {
	local t="${work}/ids-bad" out='' lib='' want=''
	local -a cases=(
		'mod tests {\n\t#[test]\n\tfn one() {}\n}\n|has no Test ID line'
		'mod tests {\n\t// Test ID: dek00\n\t#[test]\n\tfn one() {}\n\t// Test ID: dek00\n\t#[test]\n\tfn two() {}\n}\n|dek00 is already used at'
		'mod tests {\n\t// Test ID: DEK00\n\t#[test]\n\tfn one() {}\n}\n|DEK00 isn'"'"'t lower-case Crockford base 32'
		'mod tests {\n\t// Test ID: 00000\n\t#[test]\n\tfn one() {}\n}\n|00000 stands for 2000-01-01'
	)
	fIdsTree "${t}/good" 'mod tests {\n\t// Test ID: dek00\n\t#[test]\n\tfn one() {}\n}\n'
	out="$(bash "${t}/good/cicd/utility/test-ids.bash" check 2>&1)" || { fWhy "a good tree failed: ${out}"; return 1; }
	for lib in "${!cases[@]}"; do
		want="${cases[lib]#*|}"
		fIdsTree "${t}/${lib}" "${cases[lib]%%|*}"
		if out="$(bash "${t}/${lib}/cicd/utility/test-ids.bash" check 2>&1)"; then fWhy "passed, not '${want}': ${out}"; return 1; fi
		[[ "${out}" == *"${want}"* ]] || { fWhy "no '${want}' in: ${out}"; return 1; }
	done
	## A tracked test outside src/ and tests/, where nothing reads its ID.
	mkdir -p -- "${t}/good/a/benches"
	printf '// Test ID: dek01\n#[test]\nfn far() {}\n' > "${t}/good/a/benches/b.rs"
	git -C "${t}/good" add -- a/benches/b.rs
	if out="$(bash "${t}/good/cicd/utility/test-ids.bash" check 2>&1)"; then fWhy "passed with a stray test: ${out}"; return 1; fi
	[[ "${out}" == *"a/benches/b.rs:2: a test outside every src/"* ]] || { fWhy "the stray test: ${out}"; return 1; }
}

## Test ID: dezap
fTest_test_ids_check_reads_the_backlogs_test_case_rows() {
	local t="${work}/ids-backlog" out='' row=''
	fIdsTree "${t}" 'mod tests {\n\t// Test ID: dek00\n\t#[test]\n\tfn one() {}\n}\n'
	fItem() { printf -- '- %s\n\t- ID: %s\n\t- Status: %s\n' "$1" "$2" "$3"; [[ -z "$4" ]] || printf '\t- Test case: %s\n' "$4"; }
	## A queued item needs no row, and nothing under the template counts.
	{ fItem Named 1001 Done '`one` (dek00)'; fItem Asked 1002 Done 'none, since it was a question.'; fItem Later 1003 Queued ''
		printf '\n## Template\n\n'; fItem Title 1004 Done ''; } > "${t}/project/backlog.md"
	out="$(bash "${t}/cicd/utility/test-ids.bash" check 2>&1)" || { fWhy "a good backlog failed: ${out}"; return 1; }
	for row in '' dek07; do
		## The last item, with no template under it, so the end of the file closes it.
		{ fItem Named 1001 Done '`one` (dek00)'; fItem Bad 1005 Done "${row:+\`two\` (${row})}"; } > "${t}/project/backlog.md"
		if out="$(bash "${t}/cicd/utility/test-ids.bash" check 2>&1)"; then fWhy "passed with row '${row}': ${out}"; return 1; fi
		if [[ -z "${row}" ]]; then
			[[ "${out}" == *"1005 is Done with no Test case row"* ]] || { fWhy "no row: ${out}"; return 1; }
		else
			[[ "${out}" == *"1005 names dek07, which no test has"* ]] || { fWhy "an unknown ID: ${out}"; return 1; }
		fi
	done
}

## Test ID: dezaq
fTest_jobs_are_half_the_cpus_a_quarter_when_busy_and_never_none() {
	local dir="${work}/jobs" ok=0
	mkdir -p -- "${dir}"
	(
		## As cicd.bash has it, reading a load of this test's.
		eval "$(awk '/^fJobs\(\)/, /^}/' "${cicd}/cicd.bash" | sed "s|/proc/loadavg|${dir}/loadavg|")"
		fCase() {   ## fCase CORES LOAD CAP KIND WANT
			cores="$1"; printf '%s 0.00 0.00 1/100 1\n' "$2" > "${dir}/loadavg"
			CICD_MAX_JOBS="$3" fJobs "$4"
			[[ "${jobs}" == "$5" && "${CARGO_BUILD_JOBS}" == "$5" ]] || { fWhy "${cores} CPUs, load $2, cap '$3' $4: ${jobs} jobs, not $5"; return 1; }
		}
		## Busy is a load over half the CPUs.
		fCase 16 1.00 '' '' 8 && fCase 16 8.00 '' '' 8 && fCase 16 8.01 '' '' 4 \
			&& fCase 16 0.50 '' heavy 4 && fCase 16 12.00 '' heavy 4 \
			&& fCase 3 9.00 '' '' 1 && fCase 1 0.10 '' '' 1 && fCase 1 0.10 '' heavy 1 \
			&& fCase 16 1.00 3 '' 3 && fCase 16 1.00 3 heavy 3 && fCase 16 1.00 0 '' 8 \
			&& fCase 16 1.00 'x[$(touch '"${dir}/ran"')]' '' 8 || exit 1
		[[ ! -e "${dir}/ran" ]] || { fWhy "a \$(...) in CICD_MAX_JOBS ran"; exit 1; }
		fCase 16 8.01 '' '' 4 && [[ "${loadNote}" == "4 of 16 CPUs, load 8.01, busy" ]] || { fWhy "busy note: '${loadNote}'"; exit 1; }
	) && ok=1
	((ok))
}

## Test ID: dezar
fTest_the_shared_flags_are_taken_and_a_wrong_mix_is_refused() {
	local out='' rc=0 flags='' mode='' want=''
	fCicd() { rc=0; out="$(env -u TUKZMON_IN_IMAGE bash "${cicd}/cicd.bash" "$@" 2>&1)" || rc=$?; }
	## Each with the run it names, and --list so nothing runs.
	for flags in ':default' '--quick:quick' '--long:long' '--slow:long' '--leakcheck:default' '--no-leakcheck:default' \
		'--backlog-run-quick format lint:backlog-run-quick' '--gate:backlog-run-quick' '--backlog-run-full:backlog-run-full' \
		'--merge-gate:merge-gate' '--ci:merge-gate' '--no-container --no-remote --no-sync --keep-temp --quiet:default' \
		'--message x:default' '-m x:default' '--msg=x:default'; do
		mode="${flags##*:}"; flags="${flags%:*}"
		# shellcheck disable=2086  ## Split on purpose, into its flags.
		fCicd ${flags} --list
		((rc == 0)) || { fWhy "'${flags} --list' gave exit ${rc}: ${out}"; return 1; }
		[[ "${out}" == *"[ tukzmon cicd: ${mode} ]"* ]] || { fWhy "'${flags}' isn't a ${mode} run: ${out}"; return 1; }
		[[ "${mode}" == backlog-run-* || "${mode}" == merge-gate ]] || continue
		for want in "--${mode} never syncs" "--${mode} never dogfoods"; do
			[[ "${out}" == *"${want}"* ]] || { fWhy "'${flags}' doesn't say '${want}': ${out}"; return 1; }
		done
	done
	for flags in '--bogus:unknown option' '--quick --long:are separate runs' '--leakcheck --no-leakcheck:are opposites' \
		'--merge-gate --message x:takes no --message' '--backlog-run-quick nope:no area' '--message:needs a message'; do
		want="${flags#*:}"; flags="${flags%%:*}"
		# shellcheck disable=2086  ## Split on purpose, into its flags.
		fCicd --list ${flags}
		((rc == 2)) && [[ "${out}" == *"${want}"* ]] || { fWhy "'${flags}' gave exit ${rc}, not 2 with '${want}': ${out}"; return 1; }
	done
	fCicd --help
	((rc == 0)) && [[ "${out}" == *"--backlog-run-quick"* ]] || { fWhy "--help gave exit ${rc}: ${out}"; return 1; }
}

## Test ID: dezas
fTest_the_pipeline_never_prunes_dockers_build_cache() {
	local hits=''
	## That cache is every project's, and docker trims it itself.
	hits="$(find "${cicd}" "${cicd}/../utility" -type f \( -name '*.bash' -o -path '*/hooks/*' \) \
		-exec awk '!/^[ \t]*#/ && /(builder|buildx|system)[ \t]+prune/ { print FILENAME ":" FNR ": " $0 }' {} +)"
	[[ -z "${hits}" ]] || { fWhy "prunes it:"; printf '      %s\n' "${hits}"; return 1; }
}

## Test ID: dezat
fTest_a_run_folder_counts_up_and_the_sweep_takes_only_runs_that_are_gone() {
	local root="${work}/sweep" live='' ok=0
	mkdir -p -- "${root}/${projectName}"
	(
		HOST_TEMP="${root}"
		source "${cicd}/utility/include/run-tmp.bash"
		local base="${HOST_TEMP}/${projectName}" old='' now='' stamp='' d='' n=''
		fRun() {   ## fRun NAME RUN_PID|none [keep]
			mkdir -p -- "${base}/$1"
			[[ "$2" == none ]] || printf '%s' "$2" > "${base}/$1/run.pid"
			[[ -z "${3:-}" ]] || : > "${base}/$1/keep"
		}
		printf -v old '%(%Y%m%d-%H%M%S)T' "$((EPOCHSECONDS - 3 * 86400))"
		printf -v now '%(%Y%m%d-%H%M%S)T' "$((EPOCHSECONDS - 60))"
		fRun "${old}_001" '999999999 1'
		fRun "${old}_002" none
		fRun "${old}_003" '999999999 1' keep
		## Killed, kept by --keep-temp, live, gone but a process runs on in
		## it, and starting, with no pid written yet.
		fRun "${now}_001" '999999999 1'
		fRun "${now}_002" '999999999 1' keep
		fRun "${now}_003" "$(fPidStamp "$$")"
		fRun "${now}_004" '999999999 1'
		fRun "${now}_005" ''
		TMPDIR="${base}/${now}_004" sleep 30 &
		live="$!"
		trap 'kill "${live}" 2> /dev/null || true' EXIT
		for _ in 1 2 3 4 5 6 7 8 9 10; do ! grep -qszF -- "TMPDIR=${base}/${now}_004" "/proc/${live}/environ" || break; sleep 0.1; done
		## A name that's taken gets the next number.
		printf -v stamp '%(%Y%m%d-%H%M%S)T' -1
		mkdir -- "${base}/${stamp}_001"
		fRunTmpMake || { fWhy "made no run folder"; exit 1; }
		[[ "${runTmp}" == "${base}/${stamp}_002" || ( "${runTmp}" != "${base}/${stamp}_"* && "${runTmp}" == *_001 ) ]] || { fWhy "made ${runTmp}"; exit 1; }
		[[ "$(cat -- "${runTmp}/run.pid")" == "$(fPidStamp "$$")" ]] || { fWhy "run.pid is '$(cat -- "${runTmp}/run.pid")'"; exit 1; }
		n="$(fSweepStale)"
		kill "${live}" 2> /dev/null || true
		wait "${live}" 2> /dev/null || true
		for d in "${old}_001" "${old}_003" "${now}_001"; do [[ ! -e "${base}/${d}" ]] || { fWhy "left ${d}"; exit 1; }; done
		for d in "${old}_002" "${now}_002" "${now}_003" "${now}_004" "${now}_005" "${runTmp##*/}"; do [[ -d "${base}/${d}" ]] || { fWhy "took ${d}"; exit 1; }; done
		[[ "${n}" == 3 ]] || { fWhy "counted ${n} gone, not 3"; exit 1; }
	) && ok=1
	((ok))
}

## cicd.bash's container cleanup, with docker a function that logs each call
## to DIR/calls and answers from STUB_PS, the project's containers as
## "id|run|stamp" lines. Run in a subshell.
fCtrRig() {   ## fCtrRig DIR
	dir="$1"
	mkdir -p -- "${dir}/host/${projectName}"
	HOST_TEMP="${dir}/host"
	source "${cicd}/utility/include/run-tmp.bash"
	eval "$(awk '/^(fCtrCleanup|fOtherRun|fCtrOthers|fCtrRunLive|fCtrSweepGone|fCtrImagesBytes)\(\)/, /^}/' "${cicd}/cicd.bash")"
	fEcho_Clean() { printf '%s\n' "$*" >> "${dir}/said"; }
	docker() {
		printf '%s\n' "$*" >> "${dir}/calls"
		case "$*" in
			"ps -aq "*"label=dev.run=${runName}"*) echo c-mine ;;
			"ps -a "*) printf '%s\n' "c-mine|${runName}|$(fPidStamp "$$")" "${STUB_PS:-}" | sed '/^$/d' ;;
			"image ls -q "*dangling=true*) echo sha256:0123456789abcdef ;;
			"image ls ${CTR_IMAGE} "*) printf '%s\n' "${CTR_IMAGE}":{a,b,c,d,e} ;;
			"image inspect "*) printf '%s\n' 100 300 200 '' 50 ;;
		esac
	}
	# shellcheck disable=2034  ## Read by the cleanup, from cicd.bash.
	{ CTR_IMAGE=tukzmon-ci; ctrTag="${CTR_IMAGE}:c"; ctrPassed=1; keepTemp=0; staleGone=0; }
	fRunTmpMake || return 1
	runName="${runTmp##*/}"
	: > "${dir}/calls"; : > "${dir}/said"
}

## Test ID: dezav
fTest_the_cleanup_takes_only_the_projects_own_and_no_image_while_another_run_is_live() {
	local t="${work}/ctr-cleanup" live='' ok=0
	(
		local dir='' calls='' other=''
		fCtrRig "${t}/quiet" || exit 1
		fCtrCleanup
		calls="$(cat -- "${dir}/calls")"
		## Newest first is b, c, a, e, d: c is this run's, a the one before
		## it, b is newer, and d has no stamp, so it's the oldest.
		[[ "$(grep '^rmi' <<< "${calls}")" == "$(printf 'rmi %s\n' sha256:0123456789abcdef tukzmon-ci:e tukzmon-ci:d)" ]] || { fWhy "removed: ${calls}"; exit 1; }
		grep -qx 'stop -t 5 c-mine' <<< "${calls}" && grep -qx 'rm c-mine' <<< "${calls}" || { fWhy "left its own container: ${calls}"; exit 1; }
		if grep -E '^(ps|image ls)' <<< "${calls}" | grep -qvF 'label=dev.project=tukzmon'; then fWhy "looked past the label: ${calls}"; exit 1; fi
		if grep -qE -- ' -f( |$)|prune' <<< "${calls}"; then fWhy "forced or pruned: ${calls}"; exit 1; fi
		## Another run, live in a folder here, then in a container only.
		sleep 30 &
		live="$!"
		trap 'kill "${live}" 2> /dev/null || true' EXIT
		for other in folder container; do
			fCtrRig "${t}/${other}" || exit 1
			if [[ "${other}" == folder ]]; then
				mkdir -- "${HOST_TEMP}/${projectName}/20990101-000000_001"
				fPidStamp "${live}" > "${HOST_TEMP}/${projectName}/20990101-000000_001/run.pid"
			else
				STUB_PS="c-other|20990101-000000_001|$(fPidStamp "${live}")"
			fi
			fCtrCleanup
			calls="$(cat -- "${dir}/calls")"
			if grep -q '^rmi' <<< "${calls}"; then fWhy "another run in a ${other}, and it removed: ${calls}"; exit 1; fi
			grep -qx 'left the images alone, since run 20990101-000000_001 is live' "${dir}/said" || { fWhy "said: $(cat -- "${dir}/said")"; exit 1; }
			unset STUB_PS
		done
		## --keep-temp keeps the run's containers.
		fCtrRig "${t}/kept" || exit 1
		# shellcheck disable=2034  ## As above.
		keepTemp=1; fCtrCleanup
		if grep -qE '^(stop|rm) ' "${dir}/calls"; then fWhy "--keep-temp, and it removed: $(cat -- "${dir}/calls")"; exit 1; fi
	) && ok=1
	((ok))
}

## Test ID: dezaw
fTest_containers_whose_run_is_gone_are_stopped_but_not_a_kept_or_live_ones() {
	local t="${work}/ctr-gone" live='' ok=0
	(
		local dir='' base=''
		fCtrRig "${t}" || exit 1
		base="${HOST_TEMP}/${projectName}"
		sleep 30 &
		live="$!"
		trap 'kill "${live}" 2> /dev/null || true' EXIT
		mkdir -- "${base}/20990101-000000_003" "${base}/20990101-000000_004"
		: > "${base}/20990101-000000_003/keep"
		fPidStamp "${live}" > "${base}/20990101-000000_004/run.pid"
		## Gone, live, a kept run's, one with no labels, and one from before
		## dev.pid whose run folder is live.
		STUB_PS="$(printf '%s\n' 'c-gone|20990101-000000_001|999999999 1' "c-live|20990101-000000_002|$(fPidStamp "${live}")" \
			'c-kept|20990101-000000_003|999999999 1' 'c-bare||' 'c-older|20990101-000000_004|')"
		fCtrSweepGone
		[[ "$(grep -E '^(stop|rm) ' "${dir}/calls")" == "$(printf '%s\n' 'stop -t 5 c-gone c-bare' 'rm c-gone c-bare')" ]] || { fWhy "calls: $(cat -- "${dir}/calls")"; exit 1; }
		[[ -d "${runTmp}" ]] || { fWhy "swept this run's folder"; exit 1; }
	) && ok=1
	((ok))
}

## Test ID: dezf4
fTest_the_stale_sweep_scans_the_processes_once() {
	local root="${work}/sweep-once" live='' ok=0
	mkdir -p -- "${root}/${projectName}"
	(
		HOST_TEMP="${root}"
		source "${cicd}/utility/include/run-tmp.bash"
		local base="${HOST_TEMP}/${projectName}" d='' n=''
		for d in 20990101-000000_00{1..4}; do mkdir -- "${base}/${d}"; printf '999999999 1' > "${base}/${d}/run.pid"; done
		TMPDIR="${base}/20990101-000000_003/sub" sleep 30 &
		live="$!"
		trap 'kill "${live}" 2> /dev/null || true' EXIT
		for _ in 1 2 3 4 5 6 7 8 9 10; do ! grep -qszF -- "TMPDIR=${base}/20990101-000000_003" "/proc/${live}/environ" || break; sleep 0.1; done
		grep() { echo x >> "${root}/greps"; command grep "$@"; }
		n="$(fSweepStale)"
		[[ "$(wc -l < "${root}/greps")" == 1 ]] || { fWhy "scanned $(wc -l < "${root}/greps") times"; exit 1; }
		[[ "${n}" == 3 && -d "${base}/20990101-000000_003" ]] || { fWhy "took ${n}: $(ls -- "${base}")"; exit 1; }
	) && ok=1
	((ok))
}

## Test ID: dezf5
fTest_markdownlint_runs_once_per_set_of_rules() {
	local t="${work}/mdlint" ok=0
	mkdir -p -- "${t}"
	(
		eval "$(awk '/^fLintMarkdown\(\)/, /^}/' "${cicd}/cicd.bash")"
		fLintRun() { "$@"; }
		markdownlint() { printf '%s\n' "$*" >> "${t}/calls"; [[ " $* " != *" ${bad:-} "* ]]; }
		# shellcheck disable=2034  ## Read by fLintMarkdown, from cicd.bash.
		{ MARKDOWNLINT_DISABLE=(MD013); MARKDOWNLINT_DISABLE_FOR=("b.md|MD036" "c.md|MD001 MD002" "c.md|MD003"); }
		local want='' bad=''
		want="$(printf '%s\n' '--disable MD013 -- a.md d.md' '--disable MD013 MD036 -- b.md' '--disable MD013 MD001 MD002 MD003 -- c.md')"
		fLintMarkdown a.md b.md c.md d.md || { fWhy "failed with nothing wrong"; exit 1; }
		[[ "$(cat -- "${t}/calls")" == "${want}" ]] || { fWhy "calls: $(cat -- "${t}/calls")"; exit 1; }
		: > "${t}/calls"; bad=b.md
		if fLintMarkdown a.md b.md c.md d.md; then fWhy "passed with b.md failing"; exit 1; fi
		[[ "$(cat -- "${t}/calls")" == "${want}" ]] || { fWhy "stopped at the failure: $(cat -- "${t}/calls")"; exit 1; }
	) && ok=1
	((ok))
}

## Test ID: df2sr
fTest_the_memory_cap_is_a_third_of_the_ram_or_a_size_or_none() {
	local dir="${work}/memcap" ok=0
	mkdir -p -- "${dir}"
	(
		## As cicd.bash has it, reading a meminfo of this test's.
		eval "$(awk '/^fMemCap\(\)/, /^}/' "${cicd}/cicd.bash" | sed "s|/proc/meminfo|${dir}/meminfo|")"
		fCase() {   ## fCase MEMTOTAL_KB CAP WANT
			if [[ -n "$1" ]]; then printf 'MemTotal:       %s kB\nMemFree:  1 kB\n' "$1" > "${dir}/meminfo"; else printf 'MemFree:  1 kB\n' > "${dir}/meminfo"; fi
			memCap="unset"
			CICD_MEM_CAP="$2" fMemCap || { fWhy "${1:-no} kB, cap '$2': refused"; return 1; }
			[[ "${memCap}" == "$3" ]] || { fWhy "${1:-no} kB, cap '$2': '${memCap}', not '$3'"; return 1; }
		}
		## 125, 48 and 8 GiB, and no MemTotal at all.
		fCase 131072000 '' 32g && fCase 50331648 '' 16g && fCase 8388608 '' 4g && fCase '' '' 4g \
			&& fCase 50331648 7g 7g && fCase 50331648 7G 7g && fCase 50331648 512m 512m && fCase 50331648 none '' || exit 1
		local bad=''
		for bad in 'x[$(touch '"${dir}/ran"')]' '0' '07g' '7gb' '7 g' '-1' 'None' '1.5g'; do
			if CICD_MEM_CAP="${bad}" fMemCap; then fWhy "took '${bad}' as '${memCap}'"; exit 1; fi
		done
		[[ ! -e "${dir}/ran" ]] || { fWhy "a \$(...) in CICD_MEM_CAP ran"; exit 1; }
	) && ok=1
	((ok))
}

## Test ID: df2ss
fTest_every_container_gets_the_memory_cap_with_no_swap() {
	local dir="${work}/ctrmem" ok=0 hits=''
	mkdir -p -- "${dir}"
	printf 'MemTotal:       50331648 kB\n' > "${dir}/meminfo"
	(
		## fMemCap and the lines that make ctrMem from it, as cicd.bash has them.
		eval "$(awk '/^fMemCap\(\)/, /^}/' "${cicd}/cicd.bash" | sed "s|/proc/meminfo|${dir}/meminfo|")"
		eval "$(awk '/^fCtrRun\(\)/, /^}/' "${cicd}/cicd.bash")"
		local ctrMemLines=''
		ctrMemLines="$(awk '/^memCap=""$/, /ctrMem=\(--memory/' "${cicd}/cicd.bash")"
		[[ -n "${ctrMemLines}" ]] || { fWhy "cicd.bash makes no ctrMem"; exit 1; }
		fDie() { printf 'DIE %s\n' "$*" >> "${dir}/calls"; return 1; }
		docker() { printf '%s\n' "$*" >> "${dir}/calls"; }
		# shellcheck disable=2034  ## Read by fCtrRun, from cicd.bash.
		{ keepTemp=0; runName=run; ctrLabels=(--label dev.project=tukzmon); jobs=2; }
		fCase() {   ## fCase CAP WANT
			: > "${dir}/calls"
			# shellcheck disable=2034  ## Read by fMemCap, from cicd.bash.
			CICD_MEM_CAP="$1"
			eval "${ctrMemLines}"
			fCtrRun build img true
			if [[ -n "$2" ]]; then
				[[ " $(cat -- "${dir}/calls") " == *" --memory $2 --memory-swap $2 --oom-score-adj 1000 "* ]] || { fWhy "cap '$1': $(cat -- "${dir}/calls")"; return 1; }
			else
				[[ " $(cat -- "${dir}/calls") " != *" --memory"* && " $(cat -- "${dir}/calls") " != *"--oom-score-adj"* ]] || { fWhy "cap '$1' is capped: $(cat -- "${dir}/calls")"; return 1; }
			fi
		}
		fCase '' 16g && fCase 7g 7g && fCase none '' || exit 1
	) && ok=1
	((ok)) || return 1
	## Every docker or podman run and create in cicd.bash, continuation lines
	## joined, takes ctrMem, Nix's too.
	hits="$(awk '!/^[ \t]*#/ { line = line $0; if (/\\$/) { sub(/\\$/, "", line); next }
		if (line ~ /(docker|podman)[ \t]+(run|create)[ \t]/ && index(line, "${ctrMem[@]}") == 0) print FNR ": " line; line = "" }' "${cicd}/cicd.bash")"
	[[ -z "${hits}" ]] || { fWhy "uncapped:"; printf '      %s\n' "${hits}"; return 1; }
}

## Test ID: df2st
fTest_a_bad_memory_cap_starts_nothing_and_a_good_one_reruns_the_run_in_a_capped_scope() {
	local t="${work}/memscope" out='' rc=0 f='' want=''
	mkdir -p -- "${t}/bin" "${t}/home" "${t}/host/${projectName}"
	## Format's tools stand in, and systemd-run logs its arguments and exits
	## STUB_SR_RC without running anything.
	for f in cargo clang-format black choom; do printf '#!/bin/sh\nexit 0\n' > "${t}/bin/${f}"; done
	printf '#!/bin/sh\nprintf "%%s\\n" "$*" >> "%s"\nexit "${STUB_SR_RC:-0}"\n' "${t}/calls" > "${t}/bin/systemd-run"
	chmod +x -- "${t}/bin/"*
	fRun() {   ## fRun CAP [ENV...]
		local cap="$1"; shift
		rc=0; : > "${t}/calls"
		out="$(env -u TUKZMON_IN_IMAGE -u TUKZMON_MEM_SCOPE CICD_MEM_CAP="${cap}" HOME="${t}/home" HOST_TEMP="${t}/host" PATH="${t}/bin:${PATH}" "$@" \
			bash "${cicd}/cicd.bash" --backlog-run-quick format --no-container 2>&1)" || rc=$?
	}
	fRun 7g
	want="--user --scope --quiet -p MemoryMax=7G -p MemorySwapMax=0 -p OOMPolicy=continue choom -n 1000 -- bash ${cicd}/cicd.bash --backlog-run-quick format --no-container"
	((rc == 0)) && [[ "$(cat -- "${t}/calls")" == "$(printf '%s\n' '--user --scope --quiet true' "${want}")" ]] \
		|| { fWhy "7g, exit ${rc}, systemd-run got: $(cat -- "${t}/calls")"; return 1; }
	[[ "${out}" != *"Starting Format"* ]] || { fWhy "7g ran Format outside the scope: ${out}"; return 1; }
	for f in 'x[$(touch '"${t}/ran"')]' 0 24gb; do
		fRun "${f}"
		((rc != 0)) && [[ "${out}" == *"CICD_MEM_CAP '${f}' isn't a size like 24g, or none"* ]] || { fWhy "'${f}' gave exit ${rc}: ${out}"; return 1; }
		[[ ! -s "${t}/calls" && "${out}" != *"Starting"* ]] || { fWhy "'${f}' started something: ${out}"; return 1; }
		[[ -z "$(ls -A -- "${t}/host/${projectName}")" ]] || { fWhy "'${f}' made a run folder"; return 1; }
	done
	[[ ! -e "${t}/ran" ]] || { fWhy "a \$(...) in CICD_MEM_CAP ran"; return 1; }
	## none, a run already in its scope, and a box with no user systemd run
	## Format here, with no scope.
	for f in none 'scope' 'no-systemd'; do
		case "${f}" in
			none) fRun none ;;
			scope) fRun 7g TUKZMON_MEM_SCOPE=7g ;;
			no-systemd) fRun 7g STUB_SR_RC=1 ;;
		esac
		((rc == 0)) && [[ "${out}" == *"PASS Format"* ]] || { fWhy "${f} gave exit ${rc}: ${out}"; return 1; }
		[[ "$(cat -- "${t}/calls")" != *MemoryMax* ]] || { fWhy "${f} reran in a scope: $(cat -- "${t}/calls")"; return 1; }
	done
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
##		- 2026-11-06: The test ID check's faults and backlog rows, the job
##		  counts, the shared flags, no build cache prune, the stale sweep, the
##		  container cleanup, dev-install's cargo, and the step lines.
##		- 2026-11-07: One process scan per stale sweep, and one markdownlint
##		  per set of skipped rules.
##		- 2026-11-09: The memory cap: its default and values, on every
##		  container, and the run's rerun in a capped scope.
