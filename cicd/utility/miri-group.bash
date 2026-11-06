#!/usr/bin/env bash

#  shellcheck enable=require-variable-braces  ## Every expansion braced: "${var}", not "$var".

##	Purpose:
##		One Miri over one module's tests, for cicd.bash's Miri step. A module's
##		tests share one Miri so they parse the settings spec once, which takes
##		over 30 minutes under Miri. They run one at a time, in name order, as
##		libtest sorts them.
##		When Miri doesn't finish clean, the log gets lines saying why:
##		- past the time limit, or stopped by an error, the test that was running
##		  fails and the ones after it are listed as not run
##		- an error after every test finished, such as a leak, fails the module,
##		  since Miri can't say which test it came from
##		A test that fails its asserts already has its own FAILED line. stdout and
##		stderr stay in NUM.out and NUM.err, and NUM.log is what the step reads.
##	Syntax:
##		miri-group.bash LOGDIR NUM PKG KIND NAME...
##		NIGHTLY, LOCKED and MINUTES come from the environment, with Miri's own.
##	Exit: 0 when Miri ran every test and finished clean, else 1.
##	Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)
##	Licensed under The MIT License (MIT). Full text at:
##		https://mit-license.org/
##	SPDX-License-Identifier: MIT

set -Euo pipefail

logs="$1" num="$2" pkg="$3" kind="$4"; shift 4
out="${logs}/${num}.out" err="${logs}/${num}.err" log="${logs}/${num}.log" module='' rc=0 name='' gone='' finished=''
module="${1%::*}"; [[ "${module}" != "${1}" ]] || module="${pkg} top level"

# shellcheck disable=2086  ## LOCKED is empty or --locked, and goes unquoted.
timeout -k 30s "${MINUTES}m" cargo "+${NIGHTLY}" miri test ${LOCKED} -p "${pkg}" "${kind}" -- --exact --test-threads=1 "$@" > "${out}" 2> "${err}" || rc=$?
## Apart, since Miri's warnings land between libtest's "test X ... " and its
## result. cargo's Running line goes first, as test-ids.bash finds the crate
## by it, then Miri's own error when there is one.
{
	sed -n 's/\x1b\[[0-9;]*m//g; /^ *Running /{p;q}' "${err}"
	cat -- "${out}"
	((rc == 0)) || sed -n 's/\x1b\[[0-9;]*m//g; /^error/,$p' "${err}"
} > "${log}"
((rc)) || exit 0

finished="$(sed -n 's/^test \(.*\) \.\.\. \(ok\|FAILED\|ignored\)\(, .*\)\{0,1\}$/\1/p' "${log}")"
for name in "$@"; do
	grep -qxF -- "${name}" <<< "${finished}" && continue
	if [[ -z "${gone}" ]]; then
		gone="${name}"
		if ((rc == 124 || rc == 137)); then
			printf '%s ran past %s min under Miri, with the rest of %s\n' "${name}" "${MINUTES}" "${module}" >> "${log}"
		else
			printf '%s stopped Miri, exit %s\n' "${name}" "${rc}" >> "${log}"
		fi
		printf 'test %s ... FAILED\n' "${name}" >> "${log}"
	else
		printf 'test %s ... ignored, not run since %s stopped Miri\n' "${name}" "${gone}" >> "${log}"
	fi
done
if [[ -z "${gone}" ]] && ! grep -q '^test .* \.\.\. FAILED$' "${log}"; then
	printf 'FAIL ----- %s: Miri failed after its tests, exit %s, such as on a leak. TUKZMON_MIRI_FILTER=<name> reruns one\n' "${module}" "${rc}" >> "${log}"
fi
exit 1


##	History:
##		- 2026-11-05: Created, from the one-Miri-per-test runner in cicd.bash.
##		- 2026-11-05: stdout and stderr apart, so a result split by a warning
##		  still reads.
