# shellcheck shell=bash
# shellcheck disable=2034  ## 'appears unused.' runTmp is read by whatever sources this.

##	Purpose:
##		- A run's temp folder, and the only rm -rf the pipeline does. Source it
##		  after projectName is set, then call fRunTmpMake.
##		- The folder is <HOST_TEMP>/<project>/<YYYYmmDD-HHMMSS>_<NNN>, with a
##		  run.pid in it, and a keep file under --keep-temp. fSweepStale takes
##		  the ones whose run is gone and that nothing runs from any more: at
##		  once, or after 2 days for a kept one.
##		- fStopLeftovers stops what a run left running, such as a session bus
##		  a test started and what the bus started in turn. They're found by the
##		  run folder in their TMPDIR, which they inherit.
##	History: At bottom of file.

##	Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)
##	Licensed under The MIT License (MIT). Full text at:
##		https://mit-license.org/
##	SPDX-License-Identifier: MIT


## Resolved once, since a temp root behind a link would fail fSafeRm's check.
HOST_TEMP="$(realpath -e -- "${HOST_TEMP:-${TMPDIR:-/tmp}}" 2> /dev/null || realpath -e /tmp)"

## The shared guard, per the CICD process. Also takes a path under our own
## temp root, whose name needn't be tmp.
fSafeRm() {
	local p="${1:-}" real='' own="${HOST_TEMP:?}/${projectName:?}"
	if [[ "${#}" -ne 1 || -z "${p}" || "${p}" == *..* || "${p}" == *//* || "${p}" == *$'\n'* ]]; then
		printf 'safe-rm: refused, bad path: "%s"\n' "${p}" >&2; return 1
	fi
	p="${p%/}"
	if ! [[ "${p}" =~ ^/(.+/)?tmp/[^/]+(/[^/]+)*$ || "${p}" == "${own}"/?* ]]; then
		printf 'safe-rm: refused, not under a tmp folder: %s\n' "${p}" >&2; return 1
	fi
	## Other runs and sessions keep theirs in the temp root and each project's
	## folder there.
	if [[ "${p}" == "${HOST_TEMP}" || ( "${p}" == "${HOST_TEMP}"/* && "${p#"${HOST_TEMP}"/}" != */* ) ]]; then
		printf 'safe-rm: refused, a whole project temp folder: %s\n' "${p}" >&2; return 1
	fi
	if [[ -L "${p}" ]]; then printf 'safe-rm: refused, a link: %s\n' "${p}" >&2; return 1; fi
	real="$(realpath -e -- "${p}" 2> /dev/null)" || { printf 'safe-rm: refused, not there: %s\n' "${p}" >&2; return 1; }
	if [[ "${real}" != "${p}" ]]; then printf 'safe-rm: refused, goes through a link: %s\n' "${p}" >&2; return 1; fi
	rm -rf --one-file-system -- "${p}"
}

## "<pid> <start time>", so a reused pid doesn't read as the run still going.
fPidStamp() {
	local stat=''
	local -a fields=()
	{ read -r stat < "/proc/${1}/stat"; } 2> /dev/null || return 1
	read -r -a fields <<< "${stat##*) }"
	printf '%s %s\n' "${1}" "${fields[19]}"
}

## True when STAMP, from fPidStamp, is still the same process.
fStampLive() { [[ "${1:-}" =~ ^[0-9]+\ [0-9]+$ ]] && [[ "$(fPidStamp "${1% *}" || true)" == "${1}" ]]; }

## True while the run that made run folder DIR is going.
fRunLive() {
	local got=''
	{ read -r got < "${1}/run.pid"; } 2> /dev/null || return 1
	fStampLive "${got}"
}

## The pids of ours with DIR, or a folder in it, as their TMPDIR, which every
## child of a run inherits. One grep over every process for a rough match,
## then an exact look at the few it names.
fRunPids() {   ## fRunPids DIR
	local dir="${1:?}" p='' e=''
	while IFS= read -r p; do
		p="${p#/proc/}"; p="${p%/environ}"
		[[ "${p}" =~ ^[0-9]+$ ]] || continue
		while IFS= read -r -d '' e; do
			if [[ "${e}" == "TMPDIR=${dir}" || "${e}" == "TMPDIR=${dir}/"* ]]; then echo "${p}"; break; fi
		done 2> /dev/null < "/proc/${p}/environ"
	done < <(LC_ALL=C grep -lszF -- "TMPDIR=${dir}" /proc/[0-9]*/environ 2> /dev/null || true)
}

## Sets runTmp. A plain mkdir, counting up while the name is taken.
runTmp=''
fRunTmpMake() {
	local base="${HOST_TEMP}/${projectName}" stamp='' n=1 d=''
	printf -v stamp '%(%Y%m%d-%H%M%S)T' -1
	mkdir -p -- "${base}" || return 1
	while ((n < 1000)); do
		printf -v d '%s/%s_%03d' "${base}" "${stamp}" "${n}"
		if mkdir -- "${d}" 2> /dev/null; then
			fPidStamp "$$" > "${d}/run.pid" || return 1
			runTmp="${d}"
			return 0
		fi
		n=$((n + 1))
	done
	return 1
}

## Removes our own run folders whose run is gone and that nothing runs from,
## such as one a kill -9 left. A kept one waits 2 days. Only folders with a
## run.pid, so a temp folder made by hand is left alone. Prints how many went.
fSweepStale() {
	local d='' name='' got='' old=0 epoch='' cutoff='' n=0
	printf -v epoch '%(%s)T' -1
	printf -v cutoff '%(%Y%m%d-%H%M%S)T' "$((epoch - 172800))"
	for d in "${HOST_TEMP}/${projectName}"/[0-9][0-9][0-9][0-9][0-9][0-9][0-9][0-9]-[0-9][0-9][0-9][0-9][0-9][0-9]_[0-9][0-9][0-9]; do
		[[ -d "${d}" && ! -L "${d}" && -f "${d}/run.pid" ]] || continue
		name="${d##*/}"
		old=0; [[ ! "${name:0:15}" < "${cutoff}" ]] || old=1
		if [[ -e "${d}/keep" ]] && ((! old)); then continue; fi
		## A run.pid not yet written may be a run just starting.
		got=''; { read -r got < "${d}/run.pid"; } 2> /dev/null || true
		if [[ ! "${got}" =~ ^[0-9]+\ [0-9]+$ ]] && ((! old)); then continue; fi
		! fStampLive "${got}" || continue
		[[ -z "$(fRunPids "${d}")" ]] || continue
		if fSafeRm "${d}"; then n=$((n + 1)); fi
	done
	echo "${n}"
}

## TERM, then KILL, every process of ours with this run's folder in its
## TMPDIR, other than this shell and what started it. Prints how many.
fStopLeftovers() {
	local dir="${1:?}" p='' stat='' pid='' n=0 alive=0
	local -a found=() fields=()
	local -A spare=()
	p="${BASHPID}"
	while [[ "${p}" =~ ^[0-9]+$ ]] && ((p > 1)); do
		spare["${p}"]=1
		stat=''; { read -r stat < "/proc/${p}/stat"; } 2> /dev/null || break
		read -r -a fields <<< "${stat##*) }"
		p="${fields[1]}"
	done
	spare["$$"]=1
	while IFS= read -r pid; do
		[[ -n "${spare[${pid}]:-}" ]] || found+=("${pid}")
	done < <(fRunPids "${dir}")
	((${#found[@]})) || { echo 0; return 0; }
	kill -TERM "${found[@]}" 2> /dev/null || true
	for n in 1 2 3 4 5 6 7 8 9 10; do
		alive=0
		for pid in "${found[@]}"; do [[ -d "/proc/${pid}" ]] && alive=1; done
		((alive)) || break
		sleep 0.2
	done
	kill -KILL "${found[@]}" 2> /dev/null || true
	echo "${#found[@]}"
}


##	History:
##		- 2026-10-25: Created, with fSafeRm from the CICD process.
##		- 2026-10-25: A folder whose run is gone goes at once, unless kept.
##		- 2026-10-30: fSafeRm refuses the temp root and a project's folder in it,
##		  as the CICD process's copy does.
