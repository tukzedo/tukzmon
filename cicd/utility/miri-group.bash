#!/usr/bin/env bash

#  shellcheck enable=require-variable-braces  ## Every expansion braced: "${var}", not "$var".

##	Purpose:
##		Runs Miri over one module's tests, for cicd.bash's Miri step. A
##		module's tests share one Miri so they parse the settings spec once,
##		which takes minutes under Miri. They run one at a time, in name order,
##		as libtest sorts them.
##		When Miri doesn't finish clean, the log gets lines saying why:
##		- If Miri stops on an error, such as an operation it can't do, the
##		  test that was running fails, and a new Miri runs the tests after it.
##		  So one test that stops Miri doesn't hide the rest of its module.
##		- If the module runs past the time limit, the test that was running
##		  fails, and the ones after it are listed as not run.
##		- An error after every test finished, such as a leak, fails the module,
##		  since Miri can't say which test it came from.
##		A test that fails its asserts already has its own FAILED line. Each
##		Miri's stdout and stderr stay in NUM.PASS.out and NUM.PASS.err, and
##		NUM.log is what the step reads. Its last line, from "## ", gives the
##		module's time.
##	Syntax:
##		miri-group.bash LOGDIR NUM PKG KIND NAME...
##		NIGHTLY, LOCKED and MINUTES come from the environment, with Miri's own.
##		MINUTES is for the whole module, every Miri it takes included.
##	Exit: 0 when Miri ran every test and finished clean, else 1.
##	Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)
##	Licensed under The MIT License (MIT). Full text at:
##		https://mit-license.org/
##	SPDX-License-Identifier: MIT

set -Euo pipefail

logs="$1" num="$2" pkg="$3" kind="$4"; shift 4
log="${logs}/${num}.log" out='' err='' module='' rc=0 bad=0 pass=0 left=0 name='' gone='' finished=''
declare -a rest=("$@") next=()
module="${1%::*}"; [[ "${module}" != "${1}" ]] || module="${pkg} top level"
deadline=$((SECONDS + MINUTES * 60))
: > "${log}"
while ((${#rest[@]})); do
	pass=$((pass + 1)) rc=0
	out="${logs}/${num}.${pass}.out" err="${logs}/${num}.${pass}.err"
	left=$((deadline - SECONDS))
	if ((left > 0)); then
		# shellcheck disable=2086  ## LOCKED is empty or --locked, and goes unquoted.
		timeout -k 30s "${left}s" cargo "+${NIGHTLY}" miri test ${LOCKED} -p "${pkg}" "${kind}" -- --exact --test-threads=1 "${rest[@]}" > "${out}" 2> "${err}" || rc=$?
	else
		: > "${out}"; : > "${err}"; rc=124
	fi
	## stdout and stderr are kept apart, since Miri prints warnings between
	## libtest's "test X ... " and its result. cargo's Running line goes first,
	## since test-ids.bash finds the crate by it, then Miri's own error when
	## there is one.
	{
		((pass > 1)) || sed -n 's/\x1b\[[0-9;]*m//g; /^ *Running /{p;q}' "${err}"
		cat -- "${out}"
		## A test cut off leaves "test X ... " with no newline.
		[[ -z "$(tail -c 1 -- "${out}")" ]] || echo
		((rc == 0)) || sed -n 's/\x1b\[[0-9;]*m//g; /^error/,$p' "${err}"
	} >> "${log}"
	((rc)) || break
	bad=1
	finished="$(sed -n 's/^test \(.*\) \.\.\. \(ok\|FAILED\|ignored\)\(, .*\)\{0,1\}$/\1/p' "${out}")"
	gone='' next=()
	for name in "${rest[@]}"; do
		if [[ -n "${gone}" ]]; then next+=("${name}"); continue; fi
		grep -qxF -- "${name}" <<< "${finished}" || gone="${name}"
	done
	if [[ -z "${gone}" ]]; then
		grep -q '^test .* \.\.\. FAILED$' "${out}" \
			|| printf 'FAIL ----- %s: Miri failed after its tests, exit %s, such as on a leak. TUKZMON_MIRI_FILTER=<name> reruns one\n' "${module}" "${rc}" >> "${log}"
		break
	fi
	if ((rc == 124 || rc == 137)); then
		printf '%s ran past %s min under Miri, with the rest of %s\n' "${gone}" "${MINUTES}" "${module}" >> "${log}"
		printf 'test %s ... FAILED\n' "${gone}" >> "${log}"
		for name in "${next[@]}"; do printf 'test %s ... ignored, not run since %s stopped Miri\n' "${name}" "${gone}" >> "${log}"; done
		break
	fi
	printf '%s stopped Miri, exit %s\n' "${gone}" "${rc}" >> "${log}"
	printf 'test %s ... FAILED\n' "${gone}" >> "${log}"
	((${#next[@]} == 0)) || printf 'a new Miri runs the %d test(s) after it\n' "${#next[@]}" >> "${log}"
	rest=("${next[@]}")
done
## The step leaves "## " lines out of what it prints, and keeps them in the
## log it saves.
printf '## %s: %d test(s), %d Miri(s), %dm %02ds\n' "${module}" "$#" "${pass}" $((SECONDS / 60)) $((SECONDS % 60)) >> "${log}"
exit "${bad}"


##	History:
##		- 2026-11-05: Created, from the one-Miri-per-test runner in cicd.bash.
##		- 2026-11-05: stdout and stderr apart, so a result split by a warning
##		  still reads.
##		- 2026-11-25: A test that stops Miri on an error no longer leaves the
##		  rest of its module unrun. A new Miri runs them, in the same time
##		  limit.
