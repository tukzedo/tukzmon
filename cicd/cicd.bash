#!/usr/bin/env bash

#  shellcheck disable=1091  ## 'source is valid here, but shellcheck doesn't know the path to it.'
#  shellcheck disable=2034  ## 'variable appears unused.' Complains about valid use of variable indirection.
#  shellcheck disable=2153  ## 'Possible misspelling.' False hit on vars assigned in the sourced config.bash.
#  shellcheck disable=2154  ## 'referenced but not assigned.' False hit on trap strings that assign the var they use (rc=$?).
#  shellcheck disable=2317  ## 'Can't reach.' False hits on functions that only run from a trap or by name.
#  shellcheck disable=2329  ## 'This function is never invoked.' The same, for the steps, which run by name from a list.
#  shellcheck enable=require-variable-braces  ## Every expansion braced: "${var}", not "$var".

##	- Purpose: tukzmon's local CI/CD pipeline. Settings are in config.bash.
##	- Steps, fail-fast:
##	   1. Remote sync     fast-forward from upstream before anything is built
##	   2. Format          cargo fmt, in place; a check only in the hands-off runs
##	   3. Build (debug)   every crate and test target
##	   4. Lint            clippy -D warnings, shellcheck, markdownlint, with the
##	                      output also rotated into cicd/artifacts/lint/
##	   5. Tests           cargo test, with TMPDIR in the run's temp folder
##	   6. Release build   the plugin and tukzmon-settings
##	   7. Dogfood         utility/dev-install.bash, which installs the build to
##	                      ~/.local/lib/tukzmon and restarts the panel
##	  A step that's off isn't counted, and the first section says why it's off.
##	  Cleanup comes last, on failure and Ctrl+C too.
##	- Syntax:
##	  cicd/cicd.bash [options]
##	  Options:
##	   (none)              the default run: every step
##	   --quick             the fast steps. Nothing here is slow yet, so the same
##	                       as the default run
##	   --long              everything. Nothing here takes hours yet, so the same
##	                       as the default run. Also --slow
##	   --leakcheck         add every leak check. There are none yet
##	   --no-leakcheck      take every leak check out
##	   --backlog-run-quick [AREA...]
##	                       what a backlog worker runs before it commits: the
##	                       format check, build, lint and tests, or only the
##	                       areas named:
##	                         format  cargo fmt --check
##	                         build   the debug build
##	                         lint    clippy, shellcheck and markdownlint
##	                         test    cargo test
##	                         release the release build, only when named
##	   --gate [AREA...]    another name for --backlog-run-quick
##	   --backlog-run-full  what a backlog round runs once at its end: every
##	                       check of the default run
##	   --merge-gate        the same checks, before a merge to main. The
##	                       pre-push hook runs it. Also --ci
##	                       None of those 3 syncs, dogfoods, installs, commits,
##	                       pushes, prompts, or rewrites a tracked file.
##	   --no-container      run on this box. There's no container yet, so every
##	                       run does
##	   --no-remote         skip other hosts. There are none yet
##	   --no-sync           skip the remote sync
##	   --keep-temp         keep the run's temp folder, and print where it is
##	   --list              print the steps the other options would run, and
##	                       run nothing
##	   -q, --quiet         quieter cargo, and no prompt. Step lines still print
##	   -m, --message MSG   kept for the shared flags. There's no publish step,
##	                       so nothing uses it yet. Also --msg MSG and -m=MSG
##	   -h, --help          show this help
##	- Jobs: half the logical CPUs, or a quarter when the 1-minute load is over
##	  half the CPU count, worked out before each step and passed to cargo's -j
##	  and --test-threads. CICD_MAX_JOBS, when set, caps both. The whole run is
##	  under nice 19 and ionice idle.
##	- Temp: each run works in <HOST_TEMP>/tukzmon/<YYYYmmDD-HHMMSS>_<NNN>, with
##	  TMPDIR, TEMP and TMP pointed into it. It's removed at the end, along with
##	  anything still running from it, and run folders 2 days old whose run is
##	  gone.
##	- Lint: utility/lint-report.bash reports on the newest lint log, and its
##	  --check prints only what's new since the last look.
##	- The pre-push hook, hooks/pre-push, runs --merge-gate on a push to main.
##	  Turn it on per clone with:
##	    git config core.hooksPath cicd/hooks

##	History: At bottom of script.

##	Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)
##	Licensed under The MIT License (MIT). Full text at:
##		https://mit-license.org/
##	SPDX-License-Identifier: MIT


set -Eeuo pipefail

## One compound command, which bash reads to its end before any of it runs, so
## an edit to this file during a run can't be read at the old offset.
{

## GIT_DIR and friends, from a hook run in a linked worktree, would point every
## git call here at the wrong repo.
for gitVar in $(git rev-parse --local-env-vars 2> /dev/null || true); do unset "${gitVar}"; done

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "${here}/.." && pwd)"

## rustup's cargo has to win over Debian's /bin/cargo, which ignores
## rust-toolchain.toml.
export PATH="${HOME}/.cargo/bin:${PATH}"

source "${here}/config.bash"
source "${here}/utility/include/gfs-rotate.bash"
cd "${root}"
started="${SECONDS}"


#•••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••
## Output

declare -i __wasLastEchoBlank=0
fEcho_ResetBlankCounter() { __wasLastEchoBlank=0; }
fEcho_Clean() {
	if [[ -n "${1:-}" ]]; then printf '%s\n' "$*"; __wasLastEchoBlank=0
	elif ((! __wasLastEchoBlank)); then echo; __wasLastEchoBlank=1; fi
}
fEcho() { if [[ -n "$*" ]]; then fEcho_Clean "[ $* ]"; else fEcho_Clean ""; fi; }
fEcho_Force() { fEcho_ResetBlankCounter; fEcho "$*"; }
__letterbox="•••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••"
fSection() { fEcho_Clean; fEcho_Clean "${__letterbox}"; fEcho "$*"; }
fDie() { { fEcho_Force "FAILED: $*"; echo; } >&2; exit 1; }
fUsage() { fEcho_Clean "$* (try --help)" >&2; exit 2; }
## Local time to the hundredth. The locale can make the point a comma.
fNow() { local t="${EPOCHREALTIME/,/.}" u=''; u="${t#*.}"; printf '%(%Y%m%d-%H%M%S)T.%s' "${t%.*}" "${u:0:2}"; }
fMinSec() { printf '%dm %02ds' $(($1 / 60)) $(($1 % 60)); }

## Git that talks to the remote goes through gitsby where it's installed, so it
## uses the account the repo belongs to.
fGitRemote() { if command -v gitsby > /dev/null 2>&1; then gitsby raw git "$@"; else git "$@"; fi; }


#•••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••
## Options

mode="default"; areas=(); aliasNote=""; quiet=0; keepTemp=0; listOnly=0; syncOn=1; leakOpt=""; noRemote=0; noContainer=0; cliMessage=""
fMode() { [[ "${mode}" == default || "${mode}" == "$1" ]] || fUsage "${mode/#/--} and --$1 are separate runs; give one"; mode="$1"; }
while (($#)); do case "$1" in
	--quick)              fMode quick; shift ;;
	--long|--slow)        fMode long; shift ;;
	--backlog-run-quick|--gate)
		[[ "$1" == --gate ]] && aliasNote="--gate is another name for --backlog-run-quick"
		fMode backlog-run-quick; shift
		while (($#)) && [[ "$1" != -* ]]; do areas+=("$1"); shift; done ;;
	--backlog-run-full)   fMode backlog-run-full; shift ;;
	--merge-gate|--ci)
		[[ "$1" == --ci ]] && aliasNote="--ci is another name for --merge-gate"
		fMode merge-gate; shift ;;
	--leakcheck|--no-leakcheck)
		[[ -z "${leakOpt}" || "${leakOpt}" == "$1" ]] || fUsage "--leakcheck and --no-leakcheck are opposites; give one"
		leakOpt="$1"; shift ;;
	--no-container)       noContainer=1; shift ;;
	--no-remote)          noRemote=1; shift ;;
	--no-sync)            syncOn=0; shift ;;
	--keep-temp)          keepTemp=1; shift ;;
	--list)               listOnly=1; shift ;;
	-q|--quiet)           quiet=1; shift ;;
	--message=*|--msg=*|-m=*) cliMessage="${1#*=}"; shift ;;
	-m|--message|--msg)   (($# >= 2)) || fUsage "$1 needs a message"; cliMessage="$2"; shift 2 ;;
	-h|--help)            sed -n '/^##	- Purpose:/,/^##	History:/p' "${BASH_SOURCE[0]}" | sed '$d; s/^##	\{0,1\}//'; exit 0 ;;
	*) fUsage "unknown option: $1" ;;
esac; done

handsOff=0
case "${mode}" in backlog-run-quick|backlog-run-full|merge-gate) handsOff=1 ;; esac
allAreas=(format build lint test release)
if [[ "${mode}" == backlog-run-quick ]]; then
	for a in "${areas[@]}"; do [[ " ${allAreas[*]} " == *" ${a} "* ]] || fUsage "no area '${a}'. Areas: ${allAreas[*]}"; done
	((${#areas[@]})) || areas=(format build lint test)
fi
((handsOff)) && [[ -n "${cliMessage}" ]] && fUsage "--${mode} never publishes, so it takes no --message"
((quiet)) && export CARGO_TERM_QUIET=true
## The hands-off runs may not rewrite Cargo.lock.
locked=(); ((handsOff)) && locked=(--locked)

## What turns each step on, and why one is off. Read by fPlan.
fAreaOn() { [[ "${mode}" != backlog-run-quick || " ${areas[*]} " == *" $1 "* ]]; }
declare -a planNames=() planFns=() offLines=()
fPlan() {   ## fPlan NAME FUNCTION AREA
	local name="$1" fn="$2" area="$3"
	if [[ -n "${area}" ]] && ! fAreaOn "${area}"; then offLines+=("${name}: not among the areas named"); return 0; fi
	planNames+=("${name}"); planFns+=("${fn}")
}
if ((handsOff)); then offLines+=("Remote sync: --${mode} never syncs")
elif ((! syncOn)); then offLines+=("Remote sync: --no-sync")
else fPlan "Remote sync" fStepSync ""; fi
fPlan "Format" fStepFormat format
fPlan "Build (debug)" fStepBuild build
fPlan "Lint" fStepLint lint
fPlan "Tests" fStepTests test
if [[ "${mode}" == backlog-run-quick ]] && [[ " ${areas[*]} " != *" release "* ]]; then offLines+=("Release build: --backlog-run-quick runs it only when the release area is named")
else fPlan "Release build" fStepRelease ""; fi
if ((handsOff)); then offLines+=("Dogfood: --${mode} never dogfoods")
else fPlan "Dogfood" fStepDogfood ""; fi
## Off by name, until they're built.
if [[ "${leakOpt}" == --no-leakcheck ]]; then offLines+=("Leak checks: --no-leakcheck")
else offLines+=("Leak checks: not built yet (backlog 2026102506591278)"); fi
offLines+=("Packages: not built yet (backlog 2026102506591283)")
offLines+=("Profiler: not built yet (backlog 2026102506591288)")
if ((noContainer)); then offLines+=("Container: --no-container")
else offLines+=("Container: not built yet (backlog 2026102416252889), so this runs on the host"); fi
if ((noRemote)); then offLines+=("Other hosts: --no-remote")
else offLines+=("Other hosts: none, since tukzmon is Linux only for now"); fi
offLines+=("Publish: not part of this pipeline${cliMessage:+, so --message goes unused}")

fShowPlan() {
	local i
	fSection "${projectName} cicd: ${mode}"
	[[ -z "${aliasNote}" ]] || fEcho_Clean "${aliasNote}"
	fEcho_Clean "Steps:"
	for i in "${!planNames[@]}"; do fEcho_Clean "	$((i + 1)). ${planNames[i]}"; done
	fEcho_Clean "Off:"
	for i in "${offLines[@]}"; do fEcho_Clean "	${i}"; done
}
if ((listOnly)); then fShowPlan; fEcho_Clean; exit 0; fi


#•••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••
## Machine use

## Children inherit both.
renice -n 19 -p "$$" > /dev/null 2>&1 || true
if command -v ionice > /dev/null 2>&1; then ionice -c 3 -p "$$" > /dev/null 2>&1 || true; fi

## Half the CPUs, a quarter when the box is busy, and a quarter for the heavy
## steps either way. Never under 1. Sets jobs, heavyJobs and loadNote.
cores="$(nproc 2> /dev/null || echo 2)"
jobs=1; heavyJobs=1; loadNote=""
fJobs() {
	local load='' busy=0
	read -r load _ < /proc/loadavg || load=0
	## Load against half the CPUs, both in hundredths, with no float math.
	load="${load%%.*}${load#*.}"
	if (( 10#${load} > cores * 50 )); then busy=1; fi
	jobs=$((busy ? cores / 4 : cores / 2)); ((jobs >= 1)) || jobs=1
	heavyJobs=$((cores / 4)); ((heavyJobs >= 1)) || heavyJobs=1
	## A caller sharing the box can set a lower cap. Digits only, since it goes
	## into arithmetic.
	if [[ "${CICD_MAX_JOBS:-}" =~ ^[1-9][0-9]{0,3}$ ]]; then
		((jobs <= CICD_MAX_JOBS)) || jobs="${CICD_MAX_JOBS}"
		((heavyJobs <= CICD_MAX_JOBS)) || heavyJobs="${CICD_MAX_JOBS}"
	fi
	printf -v loadNote '%s of %s CPUs, load %d.%02d' "${jobs}" "${cores}" $((10#${load} / 100)) $((10#${load} % 100))
	if ((busy)); then loadNote+=", busy"; fi
	export CARGO_BUILD_JOBS="${jobs}" RUST_TEST_THREADS="${jobs}"
}


#•••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••
## Temp folder and cleanup

source "${here}/utility/include/run-tmp.bash"
staleGone="$(fSweepStale)"
fRunTmpMake || fDie "could not make a temp folder under ${HOST_TEMP}/${projectName}"
export TMPDIR="${runTmp}" TEMP="${runTmp}" TMP="${runTmp}"

runSaid=0; cleaned=0
fCleanup() {
	local stopped=0 kb=''
	((cleaned)) && return 0
	cleaned=1
	trap '' INT TERM HUP
	((runSaid)) || printf '\n[ CICD ABORTED (stopped) ]\n' >&2
	stopped="$(fStopLeftovers "${runTmp}")"
	fSection "$(fNow): Cleanup"
	((stopped == 0)) || fEcho_Clean "stopped ${stopped} process(es) still running from this run's temp folder"
	((staleGone == 0)) || fEcho_Clean "removed ${staleGone} stale run folder(s)"
	if ((keepTemp)); then
		fEcho_Clean "kept the temp folder: ${runTmp}"
	else
		kb="$(du -sk -- "${runTmp}" 2> /dev/null | cut -f1 || true)"
		if fSafeRm "${runTmp}"; then fEcho_Clean "removed this run's temp folder, about $(( (${kb:-0} + 512) / 1024 )) MB"; fi
	fi
}
trap 'fCleanup; fEcho_Clean' EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
trap 'exit 129' HUP
trap 'rc=$?; runSaid=1; printf "\n[ CICD ABORTED (exit %s) at line %s: %s ]\n" "${rc}" "${LINENO}" "${BASH_COMMAND}" >&2; exit "${rc}"' ERR


#•••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••
## Steps. Each returns non-zero to fail, and runs inside an if, so set -e is
## off in it: every command that can fail says so with || return 1.

fStepSync() {
	local branch='' upstream='' out='' ahead=0 behind=0 dirty=0 ff=0
	branch="$(git symbolic-ref --quiet --short HEAD || true)"
	upstream="$(git rev-parse --abbrev-ref --symbolic-full-name '@{upstream}' 2> /dev/null || true)"
	if [[ -z "${branch}" ]]; then fEcho_Clean "WARNING: detached HEAD; nothing to sync"; return 0; fi
	if [[ -z "${upstream}" ]]; then fEcho_Clean "WARNING: ${branch} tracks no upstream; nothing to sync"; return 0; fi
	if ! out="$(fGitRemote fetch --quiet 2>&1)"; then
		fEcho_Clean "${out}"
		fEcho_Clean "WARNING: fetch failed (offline?); going on with the local tree"
		return 0
	fi
	read -r ahead behind < <(git rev-list --left-right --count "HEAD...${upstream}")
	if ((ahead && behind)); then
		fEcho_Clean "${branch} and ${upstream} have diverged (${ahead} local, ${behind} remote); reconcile by hand"
		return 1
	fi
	if ((! behind)); then
		if ((ahead)); then fEcho_Clean "${ahead} commit(s) ahead of ${upstream}, nothing to take"; else fEcho_Clean "level with ${upstream}"; fi
		return 0
	fi
	## A fast-forward refuses to touch a modified file, so stash around it.
	if ! git diff --quiet || ! git diff --cached --quiet || [[ -n "$(git ls-files --others --exclude-standard)" ]]; then dirty=1; fi
	if ((dirty)); then git stash push --quiet --include-untracked --message "cicd sync $(fNow)" || return 1; fi
	if git merge --ff-only --quiet "${upstream}"; then ff=1; fi
	## git keeps the entry when a pop conflicts.
	if ((dirty)) && ! git stash pop --quiet; then
		fEcho_Clean "the local changes conflict with ${upstream}; they're safe in the stash. Resolve, then 'git stash drop'"
		return 1
	fi
	((ff)) || { fEcho_Clean "fast-forward from ${upstream} failed"; return 1; }
	fEcho_Clean "fast-forwarded ${behind} commit(s) from ${upstream}"
}

fStepFormat() {
	if ((handsOff)); then
		fEcho_Clean "cargo fmt --all --check"
		cargo fmt --all --check || { fEcho_Clean "run cargo fmt --all and commit"; return 1; }
	else
		fEcho_Clean "cargo fmt --all"
		cargo fmt --all || return 1
	fi
}

fStepBuild() {
	fEcho_Clean "cargo build --workspace --all-targets -j ${jobs}"
	cargo build --workspace --all-targets "${locked[@]}" -j "${jobs}" || return 1
}

## One lint tool, its output into the lint log too.
lintLog="/dev/null"
fLintRun() { local rc=0; "$@" 2>&1 | tee -a "${lintLog}" || rc=$?; return "${rc}"; }

fStepLint() {
	local f entry pin pinName pinWant pinCmd have
	local -a failed=() files=() shFiles=() mdFiles=() mdOff=() rules=() pinArgv=()
	if mkdir -p -- "${root}/${LINT_LOG_DIR}" 2> /dev/null; then
		lintLog="${root}/${LINT_LOG_DIR}/lint_$(printf '%(%Y%m%d-%H%M%S)T' -1).log"
		: > "${lintLog}"
	fi
	for pin in "${TOOL_PINS[@]}"; do
		pinName="${pin%%|*}"; pinWant="${pin#*|}"; pinCmd="${pinWant#*|}"; pinWant="${pinWant%%|*}"
		read -r -a pinArgv <<< "${pinCmd}"
		have="$("${pinArgv[@]}" 2> /dev/null || true)"
		if [[ -z "${have}" ]]; then fLintRun echo "WARNING: ${pinName} isn't installed (pinned ${pinWant})"
		elif [[ "${have}" != *"${pinWant}"* ]]; then fLintRun echo "WARNING: ${pinName} drifted from pin ${pinWant}: ${have//$'\n'/ }"; fi
	done
	## Untracked files too, since they go out with the next commit.
	mapfile -t files < <(git ls-files --cached --others --exclude-standard)
	for f in "${files[@]}"; do
		[[ -f "${f}" ]] || continue
		case "${f}" in
			*.bash|cicd/hooks/*) shFiles+=("${f}") ;;
			*.md) mdFiles+=("${f}") ;;
		esac
	done

	fEcho_Clean "clippy -D warnings -j ${jobs}"
	if CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-${root}/target}/clippy" fLintRun cargo clippy --workspace --all-targets "${locked[@]}" -j "${jobs}" -- -D warnings; then
		fEcho_Clean "clippy: clean"
	else failed+=(clippy); fi
	if ! command -v shellcheck > /dev/null 2>&1; then
		((handsOff)) && failed+=("shellcheck (not installed)")
	elif ((${#shFiles[@]})); then
		fEcho_Clean "shellcheck: ${#shFiles[@]} file(s)"
		if fLintRun shellcheck -x "${shFiles[@]}"; then fEcho_Clean "shellcheck: clean"; else failed+=(shellcheck); fi
	fi
	if ! command -v markdownlint > /dev/null 2>&1; then
		((handsOff)) && failed+=("markdownlint (not installed)")
	else
		fEcho_Clean "markdownlint: ${#mdFiles[@]} file(s)"
		for f in "${mdFiles[@]}"; do
			mdOff=("${MARKDOWNLINT_DISABLE[@]}")
			for entry in "${MARKDOWNLINT_DISABLE_FOR[@]}"; do
				[[ "${entry%%|*}" == "${f}" ]] || continue
				read -r -a rules <<< "${entry#*|}"
				mdOff+=("${rules[@]}")
			done
			fLintRun markdownlint --disable "${mdOff[@]}" -- "${f}" || failed+=("markdownlint ${f}")
		done
		[[ " ${failed[*]} " == *" markdownlint "* ]] || fEcho_Clean "markdownlint: clean"
	fi
	if [[ "${lintLog}" != /dev/null ]]; then
		printf '[ lint: done, %d failed ]\n' "${#failed[@]}" >> "${lintLog}"
		gfs_rotate "${root}/${LINT_LOG_DIR}" lint log > /dev/null 2>&1 || true
	fi
	lintLog="/dev/null"
	((${#failed[@]} == 0)) || { fEcho_Clean "lint failed: ${failed[*]}"; return 1; }
}

fStepTests() {
	fEcho_Clean "cargo test --workspace -j ${jobs} -- --test-threads ${jobs}  (TMPDIR=${TMPDIR})"
	## No test may reach the screen in use.
	env -u DISPLAY -u WAYLAND_DISPLAY cargo test --workspace "${locked[@]}" -j "${jobs}" -- --test-threads "${jobs}" || return 1
}

fStepRelease() {
	fEcho_Clean "cargo build --release ${RELEASE_PACKAGES[*]} -j ${jobs}"
	cargo build --release "${locked[@]}" "${RELEASE_PACKAGES[@]}" -j "${jobs}" || return 1
}

fStepDogfood() {
	fEcho_Clean "${DOGFOOD_CMD[*]}"
	"${root}/${DOGFOOD_CMD[0]}" "${DOGFOOD_CMD[@]:1}" || return 1
}


#•••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••
## Run

fJobs
fShowPlan
fEcho_Clean "Jobs:	${loadNote}"
fEcho_Clean "Temp:	${runTmp}"
fEcho_Clean

stepCount="${#planNames[@]}"; lastNote=""
for i in "${!planNames[@]}"; do
	n=$((i + 1)); name="${planNames[i]}"
	fJobs
	fEcho_Clean; fEcho_Clean "${__letterbox}"
	fEcho "$(fNow): Step ${n} of ${stepCount}; Starting ${name}"
	if [[ "${loadNote}" != "${lastNote}" ]]; then fEcho_Clean "jobs: ${loadNote}"; lastNote="${loadNote}"; fi
	if "${planFns[i]}"; then
		fEcho_ResetBlankCounter
		fEcho "$(fNow): Step ${n} of ${stepCount}; PASS ${name}"
		fEcho_Clean
	else
		fEcho_ResetBlankCounter
		fEcho "$(fNow): Step ${n} of ${stepCount}; FAIL ${name}"
		fEcho_Clean
		runSaid=1
		exit 1
	fi
done

runSaid=1
fCleanup
fSection "${projectName} CI/CD: done. ($(fMinSec $((SECONDS - started))))"
exit 0
}


##	History:
##		- 2026-10-25: Created from tukfs-hash's engine and gitsby's flags: the
##		  shared flags, step lines, CPU caps under nice and ionice, a temp
##		  folder per run behind fSafeRm, lint logs, and the pre-push hook.
