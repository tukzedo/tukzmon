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
##	   2. Container image build the image from cicd/container/ when this recipe
##	                      has none, and fetch the crates the steps need
##	   3. Format          cargo fmt, in place; a check only in the hands-off runs
##	   4. Build (debug)   every crate and test target
##	   5. Lint            clippy -D warnings, shellcheck, markdownlint, and the
##	                      test ID check, with the output also rotated into
##	                      cicd/artifacts/lint/
##	   6. Advisories      cargo deny check advisories: the crates in Cargo.lock
##	                      against RustSec's database, per deny.toml
##	   7. Tests           cargo test, with TMPDIR in the run's temp folder. One
##	                      line per test: PASS, FAIL or SKIP, its ID, its name
##	   8. Release build   the plugin and tukzmon-settings
##	   9. Packages        a .deb and an .rpm of the release build through nfpm,
##	                      with a sha256 sums file, in cicd/artifacts/packages/
##	  10. Sanitizers      a leak check: the tests under ASan and LSan on the
##	                      nightly config.bash pins, std included, then
##	                      tukzmon-settings --snapshot, with its C++ under
##	                      ASan, LSan and UBSan too
##	  11. Miri            a leak check that takes hours: the tests under Miri on
##	                      the same nightly, one Miri per test.
##	                      TUKZMON_MIRI_FILTER=<part of a name> runs only those
##	  12. Soak            a slow leak check: core/tests/soak.rs, the panel's
##	                      work over days of skipped clock, with the heap read at
##	                      round N and 2N. Its report goes to cicd/artifacts/soak/
##	  13. Profiler        the panel's tick, core/examples/profile.rs, under
##	                      pprof's sampler, on this box. The flamegraph goes to
##	                      cicd/artifacts/profiling/, with a report on its hot
##	                      spots from utility/flame-report.py
##	  14. Dogfood         utility/dev-install.bash, which installs the build to
##	                      ~/.local/lib/tukzmon and restarts the panel
##	  A step that's off isn't counted, and the first section says why it's off.
##	  Cleanup comes last, on failure and Ctrl+C too.
##	- Syntax:
##	  cicd/cicd.bash [options]
##	  Options:
##	   (none)              the default run: every step
##	   --quick             the fast steps: the default run but the packages,
##	                       the profiler and the slow leak checks, so no Soak
##	   --long              everything, Miri included, which only it runs. Also
##	                       --slow
##	   --leakcheck         add every leak check to any run
##	   --no-leakcheck      take every leak check out. The advisory scan and
##	                       clippy's leak lints aren't leak check steps, and stay
##	   --backlog-run-quick [AREA...]
##	                       what a backlog worker runs before it commits: the
##	                       format check, build, lint, tests and the fast leak
##	                       checks, or only the areas named:
##	                         format  cargo fmt --check
##	                         build   the debug build
##	                         lint    clippy, shellcheck, markdownlint, and
##	                                 the test ID check
##	                         test    cargo test
##	                         advisories  the advisory scan, only when named
##	                         release the release build, only when named
##	                         package the packages and the release build they
##	                                 come from, only when named
##	                         sanitizers  the Sanitizers leak check
##	                         miri    the Miri leak check, only when named
##	                         soak    the Soak leak check, only when named
##	                         profile the profiler, only when named
##	   --gate [AREA...]    another name for --backlog-run-quick
##	   --backlog-run-full  what a backlog round runs once at its end: every
##	                       check of the default run, so not Miri
##	   --merge-gate        the same checks, before a merge to main. The
##	                       pre-push hook runs it. Also --ci
##	                       None of those 3 syncs, dogfoods, installs, commits,
##	                       pushes, prompts, or rewrites a tracked file.
##	   --no-container      run every step on this box
##	   --no-remote         skip other hosts. There are none yet
##	   --no-sync           skip the remote sync
##	   --keep-temp         keep the run's temp folder and containers, and print
##	                       where they are
##	   --list              print the steps and test IDs the other options
##	                       would run, and run nothing
##	   -q, --quiet         quieter cargo, and no prompt. Step lines still print
##	   -m, --message MSG   kept for the shared flags. There's no publish step,
##	                       so nothing uses it yet. Also --msg MSG and -m=MSG
##	   -h, --help          show this help
##	- Jobs: half the logical CPUs, or a quarter when the 1-minute load is over
##	  half the CPU count, worked out before each step and passed to cargo's -j
##	  and --test-threads. The leak checks get a quarter either way.
##	  CICD_MAX_JOBS, when set, caps them all. The whole run is under nice 19
##	  and ionice idle.
##	- Temp: each run works in <HOST_TEMP>/tukzmon/<YYYYmmDD-HHMMSS>_<NNN>, with
##	  TMPDIR, TEMP and TMP pointed into it. It's removed at the end, along with
##	  anything still running from it. A run folder whose run is gone, such as
##	  after a kill -9, goes at the next run's start once nothing runs from it,
##	  or 2 days on with --keep-temp.
##	- Container: format, build, lint, tests and the release build run in the
##	  image, offline, as the caller's uid, with the repo and the temp folder
##	  at their own paths. It builds into target/container, beside the host's
##	  target/. A run that dogfoods builds the release on the host instead,
##	  since that's what gets installed. With no docker, or an image that won't
##	  build, the run says so in one line and goes on on the host. Containers,
##	  images and build cache are cleaned up at the end, only what has the
##	  dev.project=tukzmon label. Images are left alone while another run is
##	  live. The image step stops containers whose run is gone.
##	- Lint: utility/lint-report.bash reports on the newest lint log, and its
##	  --check prints only what's new since the last look.
##	- Profiler: utility/flame-report.py reports on the newest flamegraph, and
##	  its --check prints only when there's a newer one than at the last look.
##	- Test IDs: utility/test-ids.bash makes, reads and checks them. A new test
##	  takes one from its "new" command.
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
## rust-toolchain.toml. A go-installed nfpm is a fallback for a host run.
export PATH="${HOME}/.cargo/bin:${PATH}"
[[ ! -d "${HOME}/.go/bin" ]] || PATH+=":${HOME}/.go/bin"

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

## Each container step runs this script again in the image, with these.
cliArgs=("$@")
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
allAreas=(format build lint test advisories release package profile)
for entry in "${LEAK_CHECKS[@]}"; do entry="${entry%%|*}"; allAreas+=("${entry,,}"); done
namedAreas="${#areas[@]}"
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
if ((noContainer)); then offLines+=("Container: --no-container, so this runs on the host")
else fPlan "Container image" fStepImage ""; fi
fPlan "Format" fStepFormat format
fPlan "Build (debug)" fStepBuild build
fPlan "Lint" fStepLint lint
## It needs the network, and finds what a dependency change brings in, so a
## worker's quick run takes it only when it's named.
if [[ "${mode}" == backlog-run-quick ]] && [[ " ${areas[*]} " != *" advisories "* ]]; then offLines+=("Advisories: --backlog-run-quick runs it only when the advisories area is named")
else fPlan "Advisories" fStepAdvisories ""; fi
fPlan "Tests" fStepTests test
## The packages are of the release build, so naming package brings it too.
if [[ "${mode}" == backlog-run-quick ]] && [[ " ${areas[*]} " != *" release "* && " ${areas[*]} " != *" package "* ]]; then offLines+=("Release build: --backlog-run-quick runs it only when the release or package area is named")
else fPlan "Release build" fStepRelease ""; fi
if [[ "${mode}" == backlog-run-quick ]] && [[ " ${areas[*]} " != *" package "* ]]; then offLines+=("Packages: --backlog-run-quick runs it only when the package area is named")
elif [[ "${mode}" == quick ]]; then offLines+=("Packages: --quick")
else fPlan "Packages" fStepPackage ""; fi
## A leak check's length says which runs take it, per LEAK_CHECKS. Prints why
## when it's off.
fLeakWhyOff() {   ## fLeakWhyOff NAME LENGTH
	local area="${1,,}"
	[[ "${leakOpt}" != --no-leakcheck ]] || { echo "--no-leakcheck"; return 0; }
	[[ "${leakOpt}" != --leakcheck ]] || return 0
	if [[ "${mode}" == backlog-run-quick ]]; then
		[[ " ${areas[*]} " != *" ${area} "* ]] || return 0
		[[ "${2}" != fast ]] || ((namedAreas)) || return 0
		echo "--backlog-run-quick runs it only when the ${area} area is named, or with --leakcheck"; return 0
	fi
	case "${2}" in
		fast) ;;
		slow) [[ "${mode}" != quick ]] || echo "a slow leak check, so not in --quick without --leakcheck" ;;
		hours) [[ "${mode}" == long ]] || echo "takes hours, so only in --long or with --leakcheck" ;;
	esac
}
for entry in "${LEAK_CHECKS[@]}"; do
	IFS='|' read -r leakName leakFn leakLength <<< "${entry}"
	[[ " fast slow hours " == *" ${leakLength} "* ]] || fDie "LEAK_CHECKS: ${leakName} has no length fast, slow or hours"
	leakWhy="$(fLeakWhyOff "${leakName}" "${leakLength}")"
	if [[ -n "${leakWhy}" ]]; then offLines+=("${leakName}: ${leakWhy}"); else fPlan "${leakName}" "${leakFn}" ""; fi
done
if [[ "${mode}" == backlog-run-quick ]] && [[ " ${areas[*]} " != *" profile "* ]]; then offLines+=("Profiler: --backlog-run-quick runs it only when the profile area is named")
elif [[ "${mode}" == quick ]]; then offLines+=("Profiler: --quick")
else fPlan "Profiler" fStepProfile ""; fi
if ((handsOff)); then offLines+=("Dogfood: --${mode} never dogfoods")
else fPlan "Dogfood" fStepDogfood ""; fi
## Off by name, until they're built.
if ((noRemote)); then offLines+=("Other hosts: --no-remote")
else offLines+=("Other hosts: none, since tukzmon is Linux only for now"); fi
offLines+=("Publish: not part of this pipeline${cliMessage:+, so --message goes unused}")

## Which steps go in the image. The release build is what dogfood installs on
## this box, so a run that dogfoods builds it here.
dogfoodOn=0; [[ " ${planFns[*]} " != *" fStepDogfood "* ]] || dogfoodOn=1
fInImage() {
	[[ " ${CTR_STEPS[*]} " == *" $1 "* ]] || return 1
	[[ "$1" != fStepRelease ]] || ((! dogfoodOn))
}

fShowPlan() {
	local i where=''
	fSection "${projectName} cicd: ${mode}"
	[[ -z "${aliasNote}" ]] || fEcho_Clean "${aliasNote}"
	fEcho_Clean "Steps:"
	for i in "${!planNames[@]}"; do
		where=''
		if ((! noContainer)) && [[ "${planFns[i]}" != fStepImage ]] && ! fInImage "${planFns[i]}"; then where=" (host)"; fi
		fEcho_Clean "	$((i + 1)). ${planNames[i]}${where}"
	done
	fEcho_Clean "Off:"
	for i in "${offLines[@]}"; do fEcho_Clean "	${i}"; done
}
if ((listOnly)); then
	fShowPlan
	if [[ " ${planFns[*]} " == *" fStepTests "* ]]; then
		fEcho_Clean "Test IDs:"
		"${here}/utility/test-ids.bash" list | sed 's/^/	/'
	fi
	fEcho_Clean; exit 0
fi


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

	fEcho_Clean "clippy -D warnings -j ${jobs}, the profiler's workload too"
	if CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-${root}/target}/clippy" fLintRun cargo clippy --workspace --all-targets --features tukzmon-core/profiling "${locked[@]}" -j "${jobs}" -- -D warnings; then
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
	fLintRun "${here}/utility/test-ids.bash" check || failed+=("test IDs")
	if [[ "${lintLog}" != /dev/null ]]; then
		printf '[ lint: done, %d failed ]\n' "${#failed[@]}" >> "${lintLog}"
		gfs_rotate "${root}/${LINT_LOG_DIR}" lint log > /dev/null 2>&1 || true
	fi
	lintLog="/dev/null"
	((${#failed[@]} == 0)) || { fEcho_Clean "lint failed: ${failed[*]}"; return 1; }
}

fStepTests() {
	fEcho_Clean "cargo test --workspace -j ${jobs} -- --test-threads ${jobs}  (TMPDIR=${TMPDIR})"
	## No test may reach the screen in use. A quiet cargo would quiet libtest
	## too, and its per-test lines are what get the IDs.
	env -u DISPLAY -u WAYLAND_DISPLAY -u CARGO_TERM_QUIET cargo test --workspace "${locked[@]}" -j "${jobs}" -- --test-threads "${jobs}" 2>&1 \
		| "${here}/utility/test-ids.bash" annotate || return 1
}

## The fetch and the check are apart, so a database that can't be had isn't
## read as a finding. With none at all the step warns and passes, like a sync
## that can't reach the remote. cargo-deny's fetch passes with no network when
## it has a copy, so the age of the copy's newest commit is what tells a stale
## one. RustSec commits most days.
fStepAdvisories() {
	local out='' newest='' days=0
	local -a dbs=()
	if ! command -v cargo-deny > /dev/null 2>&1; then
		fEcho_Clean "cargo-deny isn't installed (pinned in config.bash)"
		((! handsOff)) || return 1
		return 0
	fi
	fEcho_Clean "cargo deny fetch db"
	out="$(cargo deny fetch db 2>&1)" || fEcho_Clean "${out}"
	dbs=("${CARGO_HOME:-${HOME}/.cargo}"/advisory-dbs/advisory-db-*/)
	newest="$(git -C "${dbs[0]}" log -1 --format=%ct 2> /dev/null || true)"
	if ! [[ "${newest}" =~ ^[0-9]+$ ]]; then fEcho_Clean "WARNING: no advisory database (offline?), so nothing was scanned"; return 0; fi
	days=$(( (EPOCHSECONDS - newest) / 86400 ))
	fEcho_Clean "advisory database as of $(printf '%(%Y-%m-%d)T' "${newest}")"
	((days <= 7)) || fEcho_Clean "WARNING: the advisory database is ${days} days old (offline?)"
	fEcho_Clean "cargo deny check --disable-fetch advisories"
	cargo deny --locked check --disable-fetch advisories || return 1
}

fStepRelease() {
	fEcho_Clean "cargo build --release ${RELEASE_PACKAGES[*]} -j ${jobs}"
	cargo build --release "${locked[@]}" "${RELEASE_PACKAGES[@]}" -j "${jobs}" || return 1
}

## A .deb and an .rpm of whichever release build the run made, which is the
## host's in a run that dogfoods. The version is the workspace's, per the Rust
## pipeline notes. Each version's set replaces the last one of that version.
fStepPackage() {
	local ver='' rel="${TUKZMON_RELEASE_DIR:-${CARGO_TARGET_DIR:-${root}/target}/release}" work="${TMPDIR}/packages"
	ver="$(sed -n '/^\[workspace\.package\]/,/^\[/s/^version *= *"\(.*\)"/\1/p' "${root}/Cargo.toml")"
	[[ -n "${ver}" ]] || { fEcho_Clean "no version under [workspace.package] in Cargo.toml"; return 1; }
	[[ "${rel}" == /* ]] || rel="${root}/${rel}"
	mkdir -p -- "${work}" "${root}/${PACKAGE_DIR}" || return 1
	fEcho_Clean "cicd/utility/package.bash, v${ver}, from ${rel#"${root}/"}"
	"${here}/utility/package.bash" "${rel}" "${ver}" "${root}/${PACKAGE_DIR}" "${work}" || return 1
	fEcho_Clean "in ${PACKAGE_DIR}/"
}

## The tests under ASan and LSan on the pinned nightly, with std built too and
## the target named, so the flags don't reach build scripts. clang builds the
## C++ with UBSan and the libstdc++ assertions as well. No test reaches
## qt.cpp, so tukzmon-settings --snapshot then runs it offscreen. LSan reports
## at exit, so a leak fails a whole test binary: rerun that one with
## --test-threads=1 and a name filter to find the test.
fStepSanitizers() {
	local dir="${CARGO_TARGET_DIR:-${root}/target}/asan" cxx='' gccDir='' sym='' flags='' lib='' syms='' snap="${TMPDIR}/snapshot" pics=0
	local -a sanEnv=() runEnv=() target=(-Zbuild-std --target x86_64-unknown-linux-gnu)
	## Asking cargo through rustup would install a missing toolchain then and
	## there, so the list says whether it's here.
	if [[ "$(rustup toolchain list 2> /dev/null || true)" != *"${RUST_NIGHTLY}-"* ]]; then
		fEcho_Clean "${RUST_NIGHTLY} isn't here: rustup toolchain install ${RUST_NIGHTLY} --profile minimal --component rust-src"
		((! handsOff)) || return 1
		fEcho_Clean "WARNING: so nothing was sanitized"; return 0
	fi
	## rustc's ASan runtime has no UBSan C++ part, so the vptr check won't link.
	flags="-fsanitize=address,undefined -fno-sanitize=vptr -fno-sanitize-recover=all -fno-omit-frame-pointer -D_GLIBCXX_ASSERTIONS -g"
	for cxx in clang++-{30..14} clang++; do command -v "${cxx}" > /dev/null 2>&1 && break; done
	## clang takes the newest GCC it finds, which can lack the C++ headers, so
	## then it's pointed at g++'s.
	if ! printf '#include <algorithm>\n' | "${cxx}" -x c++ -fsyntax-only - > /dev/null 2>&1; then
		gccDir="$(g++ -print-libgcc-file-name 2> /dev/null || true)"
		gccDir="--gcc-install-dir=${gccDir%/*}"
		if ! printf '#include <algorithm>\n' | "${cxx}" "${gccDir}" -x c++ -fsyntax-only - > /dev/null 2>&1; then
			fEcho_Clean "no clang++ here that builds C++ (tried ${cxx})"
			((! handsOff)) || return 1
			fEcho_Clean "WARNING: so nothing was sanitized"; return 0
		fi
		flags+=" ${gccDir}"
	fi
	for sym in llvm-symbolizer-{30..14} llvm-symbolizer; do command -v "${sym}" > /dev/null 2>&1 && break; done
	sym="$(command -v "${sym}" 2> /dev/null || true)"
	sanEnv=(RUSTFLAGS="-Zsanitizer=address -Cforce-frame-pointers=yes"
		"CC_x86_64_unknown_linux_gnu=${cxx/clang++/clang}" "CXX_x86_64_unknown_linux_gnu=${cxx}"
		"CFLAGS_x86_64_unknown_linux_gnu=${flags}" "CXXFLAGS_x86_64_unknown_linux_gnu=${flags}")
	## exitcode 77 is no test's own, so a report can't pass for a failed test.
	## The suppressions load for the tests too, so a planted leak shows they
	## hide nothing of ours.
	runEnv=(ASAN_OPTIONS="detect_leaks=1:exitcode=77:strict_string_checks=1:check_initialization_order=1${sym:+:external_symbolizer_path=${sym}}"
		LSAN_OPTIONS="exitcode=77:suppressions=${root}/${LSAN_SUPP}:print_suppressions=0"
		UBSAN_OPTIONS="print_stacktrace=1:halt_on_error=1" G_SLICE=always-malloc G_DEBUG=gc-friendly)
	fEcho_Clean "cargo +${RUST_NIGHTLY} test --workspace ${target[*]} -j ${jobs} -- --test-threads ${jobs}, in ${dir#"${root}/"}"
	## The tests step showed the passes, so only the rest shows here.
	env -u DISPLAY -u WAYLAND_DISPLAY -u CARGO_TERM_QUIET "${sanEnv[@]}" "${runEnv[@]}" cargo "+${RUST_NIGHTLY}" test --workspace "${locked[@]}" "${target[@]}" \
		--target-dir "${dir}" -j "${jobs}" -- --test-threads "${jobs}" 2>&1 \
		| "${here}/utility/test-ids.bash" annotate | awk '/^PASS / { n++; next } { print } END { printf "%d test(s) passed under ASan\n", n }' || return 1

	fEcho_Clean "tukzmon-settings --snapshot, offscreen"
	env "${sanEnv[@]}" cargo "+${RUST_NIGHTLY}" build -p tukzmon-settings "${locked[@]}" "${target[@]}" --target-dir "${dir}" -j "${jobs}" || return 1
	## Its C++ has to show the instrumentation, or it went unchecked.
	lib="$(find "${dir}/x86_64-unknown-linux-gnu/debug/build" -name libtukzmon-settings-qt.a -printf '%T@ %p\n' 2> /dev/null | sort -n | tail -n 1)"
	lib="${lib#* }"
	syms="$(nm -u -- "${lib}" 2> /dev/null || true)"
	if [[ "${syms}" != *__asan_report_* || "${syms}" != *__ubsan_handle_* ]]; then
		fEcho_Clean "${lib:-libtukzmon-settings-qt.a} has no ASan or UBSan calls, so qt.cpp would go unchecked"; return 1
	fi
	mkdir -p -- "${snap}/config" "${snap}/data" || return 1
	env -u DISPLAY -u WAYLAND_DISPLAY "${runEnv[@]}" QT_QPA_PLATFORM=offscreen XDG_CONFIG_HOME="${snap}/config" XDG_DATA_HOME="${snap}/data" \
		"${dir}/x86_64-unknown-linux-gnu/debug/tukzmon-settings" --snapshot "${snap}/tab.png" 2>&1 || return 1
	pics="$(find "${snap}" -maxdepth 1 -name 'tab-*.png' | wc -l)"
	((pics > 0)) || { fEcho_Clean "--snapshot drew nothing, so qt.cpp went unchecked"; return 1; }
	fEcho_Clean "sanitizers: nothing reported, and ${pics} snapshot(s) drawn"
}

## The tests under Miri, on the same nightly, with the host's files and clock
## in reach, since the collectors and the history file are what's tested.
## Miri runs a whole test binary on one core, so each test gets a Miri of its
## own, the step's jobs at a time. A test Miri can't run, such as one that
## calls C, carries cfg_attr(miri, ignore) with the reason.
## TUKZMON_MIRI_FILTER, a part of a test's name, runs only the tests that have
## it, such as to rerun one that failed.
fStepMiri() {
	local dir="${CARGO_TARGET_DIR:-${root}/target}" logs="${TMPDIR}/miri" sel pkg kind name n=0 rc=0
	local -a miriEnv=()
	if [[ "$(rustup toolchain list 2> /dev/null || true)" != *"${RUST_NIGHTLY}-"* ]] \
		|| [[ "$(rustup component list --installed --toolchain "${RUST_NIGHTLY}" 2> /dev/null || true)" != *miri* ]]; then
		fEcho_Clean "${RUST_NIGHTLY} with miri isn't here: rustup toolchain install ${RUST_NIGHTLY} --profile minimal --component rust-src,miri"
		((! handsOff)) || return 1
		fEcho_Clean "WARNING: so nothing ran under Miri"; return 0
	fi
	## Its own sysroot, kept with the build, so a run doesn't rebuild it. Miri
	## rounds ln and friends a little differently each call, so the same axis
	## worked out twice wouldn't compare equal, as it does on the box.
	miriEnv=("MIRI_SYSROOT=${dir}/miri-sysroot" "MIRIFLAGS=-Zmiri-disable-isolation -Zmiri-deterministic-floats")
	mkdir -p -- "${logs}" || return 1
	fEcho_Clean "cargo +${RUST_NIGHTLY} miri setup, in ${dir#"${root}/"}/miri-sysroot"
	env "${miriEnv[@]}" cargo "+${RUST_NIGHTLY}" miri setup > "${logs}/setup.log" 2>&1 || { cat -- "${logs}/setup.log"; return 1; }
	fEcho_Clean "cargo +${RUST_NIGHTLY} miri test --no-run -j ${jobs}"
	env -u DISPLAY -u WAYLAND_DISPLAY "${miriEnv[@]}" cargo "+${RUST_NIGHTLY}" miri test --workspace "${locked[@]}" --no-run -j "${jobs}" || return 1
	: > "${logs}/list"
	for sel in "${MIRI_TARGETS[@]}"; do
		read -r pkg kind <<< "${sel}"
		while IFS= read -r name; do
			n=$((n + 1)); printf '%04d %s %s %s\n' "${n}" "${pkg}" "${kind}" "${name}" >> "${logs}/list"
		done < <(env -u DISPLAY -u WAYLAND_DISPLAY "${miriEnv[@]}" cargo "+${RUST_NIGHTLY}" miri test -q "${locked[@]}" -p "${pkg}" "${kind}" -- --list --format terse 2> /dev/null \
			| sed -n 's/: test$//p' | grep -F -- "${TUKZMON_MIRI_FILTER:-}" || true)
	done
	[[ -z "${TUKZMON_MIRI_FILTER:-}" ]] || fEcho_Clean "only the tests with '${TUKZMON_MIRI_FILTER}' in their name"
	((n > 0)) || { fEcho_Clean "Miri found no tests"; return 1; }
	fEcho_Clean "${n} test(s), one Miri each, ${jobs} at a time"
	## A leak is found after the test's own ok line, so a Miri that failed has
	## that line swapped for a FAILED one. The order's kept, so the IDs go on
	## the right lines.
	# shellcheck disable=2016  ## The inner bash expands them.
	env -u DISPLAY -u WAYLAND_DISPLAY -u CARGO_TERM_QUIET "${miriEnv[@]}" MIRI_LOGS="${logs}" NIGHTLY="${RUST_NIGHTLY}" LOCKED="${locked[*]}" \
		xargs -P "${jobs}" -L 1 bash -c 'log="${MIRI_LOGS}/$1.log"
			cargo "+${NIGHTLY}" miri test ${LOCKED} -p "$2" "$3" -- --exact "$4" > "${log}" 2>&1 && exit 0
			sed -i "/^test .* \.\.\. ok\$/d" "${log}"
			grep -q "^test .* \.\.\. FAILED\$" "${log}" || printf "test %s ... FAILED\n" "$4" >> "${log}"; exit 1' _ < "${logs}/list" || rc=1
	cat -- "${logs}"/[0-9]*.log | "${here}/utility/test-ids.bash" annotate | awk '
		/^PASS / { n++; next }
		/^ *(Running|Finished|Blocking waiting for file lock) |^running 1 test$|^test result: ok\.|has been running for over 60 seconds$|^$/ { next }
		/^FAIL / { bad++ }
		{ print }
		END { printf "%d test(s) passed under Miri%s\n", n, bad ? ", " bad " failed" : "" }'
	return "${rc}"
}

## The soak test, core/tests/soak.rs: the panel's work over days of skipped
## clock, failing when the heap at round 2N is past round N's by more than its
## margin. A plain debug build, since ASan's allocator would hide the heap from
## dhat. Its report, and dhat's profile when it fails, are kept in SOAK_DIR.
fStepSoak() {
	local out="${TMPDIR}/soak" art="${root}/${SOAK_DIR}" stamp='' rc=0
	mkdir -p -- "${out}" || return 1
	fEcho_Clean "cargo test -p tukzmon-core --test soak -j ${jobs} -- --ignored"
	env -u DISPLAY -u WAYLAND_DISPLAY -u CARGO_TERM_QUIET TUKZMON_SOAK_DIR="${out}" cargo test -p tukzmon-core --test soak "${locked[@]}" -j "${jobs}" \
		-- --ignored --test-threads 1 2>&1 | "${here}/utility/test-ids.bash" annotate || rc=1
	[[ -f "${out}/soak-heap.txt" ]] || { fEcho_Clean "the soak test wrote no report"; return 1; }
	tail -n 1 -- "${out}/soak-heap.txt"
	stamp="$(printf '%(%Y%m%d-%H%M%S)T' -1)"
	mkdir -p -- "${art}" || return 1
	cp -- "${out}/soak-heap.txt" "${art}/soak_${stamp}.log" || return 1
	## The rotation may add a tag to the name, such as _first.
	fEcho_Clean "report: ${SOAK_DIR}/soak_${stamp}*.log"
	if [[ -f "${out}/soak-heap.json" ]]; then
		cp -- "${out}/soak-heap.json" "${art}/soak_${stamp}.json" || return 1
		fEcho_Clean "dhat's profile: ${SOAK_DIR}/soak_${stamp}*.json, for dh_view.html, sorted by At t-end"
	fi
	gfs_rotate "${art}" soak log > /dev/null 2>&1 || true
	gfs_rotate "${art}" soak json > /dev/null 2>&1 || true
	return "${rc}"
}

## The panel's tick under pprof's sampler, in a release build with the names
## kept. On this box, not in the image, since the tick reads this box's /proc
## and the report needs python3. The report is a summary, so it can't fail the
## step. flame-report.py's --check marker is left alone.
fStepProfile() {
	local out="${TMPDIR}/profile" art="${root}/${PROFILE_DIR}" stamp=''
	mkdir -p -- "${out}" "${art}" || return 1
	fEcho_Clean "cargo build --profile profiling --features profiling -p tukzmon-core --example profile -j ${jobs}"
	cargo build --profile profiling --features profiling -p tukzmon-core --example profile "${locked[@]}" -j "${jobs}" || return 1
	fEcho_Clean "${PROFILE_ROUNDS} ticks under the sampler"
	env -u DISPLAY -u WAYLAND_DISPLAY "${hostTarget}/profiling/examples/profile" "${out}/flame.svg" "${PROFILE_ROUNDS}" || return 1
	stamp="$(printf '%(%Y%m%d-%H%M%S)T' -1)"
	cp -- "${out}/flame.svg" "${art}/flame_${stamp}.svg" || return 1
	gfs_rotate "${art}" flame svg > /dev/null 2>&1 || true
	## The rotation may add a tag to the name, such as _first.
	fEcho_Clean "flamegraph: ${PROFILE_DIR}/flame_${stamp}*.svg"
	fEcho_Clean
	if ! command -v python3 > /dev/null 2>&1; then fEcho_Clean "WARNING: no python3, so no report on the flamegraph"; return 0; fi
	python3 -I "${here}/utility/flame-report.py" --dir "${art}" || fEcho_Clean "WARNING: flame-report.py couldn't read the flamegraph"
}

fStepDogfood() {
	fEcho_Clean "${DOGFOOD_CMD[*]}"
	"${root}/${DOGFOOD_CMD[0]}" "${DOGFOOD_CMD[@]:1}" || return 1
}


#•••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••
## In the image. fCtrStep starts this script again in a container, with the
## step to run in TUKZMON_IN_IMAGE. The temp folder, step lines and cleanup
## stay with the run outside.

if [[ -n "${TUKZMON_IN_IMAGE:-}" ]]; then
	[[ " ${CTR_STEPS[*]} " == *" ${TUKZMON_IN_IMAGE} "* ]] || fDie "no container step '${TUKZMON_IN_IMAGE}'"
	[[ "${TUKZMON_JOBS:-}" =~ ^[1-9][0-9]{0,3}$ ]] || fDie "TUKZMON_JOBS isn't a job count: '${TUKZMON_JOBS:-}'"
	jobs="${TUKZMON_JOBS}"
	export CARGO_BUILD_JOBS="${jobs}" RUST_TEST_THREADS="${jobs}"
	if "${TUKZMON_IN_IMAGE}"; then exit 0; fi
	exit 1
fi


#•••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••
## Machine use

## Children inherit both.
renice -n 19 -p "$$" > /dev/null 2>&1 || true
if command -v ionice > /dev/null 2>&1; then ionice -c 3 -p "$$" > /dev/null 2>&1 || true; fi

## Half the CPUs, a quarter when the box is busy, and a quarter for the heavy
## steps either way. Never under 1. Sets jobs, heavyJobs and loadNote, and
## jobs to heavyJobs when given "heavy".
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
	[[ "${1:-}" != heavy ]] || jobs="${heavyJobs}"
	printf -v loadNote '%s of %s CPUs, load %d.%02d' "${jobs}" "${cores}" $((10#${load} / 100)) $((10#${load} % 100))
	if ((busy)); then loadNote+=", busy"; fi
	if [[ "${1:-}" == heavy ]]; then loadNote+=", a heavy step"; fi
	export CARGO_BUILD_JOBS="${jobs}" RUST_TEST_THREADS="${jobs}"
}


#•••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••
## Temp folder and cleanup

source "${here}/utility/include/run-tmp.bash"
staleGone="$(fSweepStale)"
fRunTmpMake || fDie "could not make a temp folder under ${HOST_TEMP}/${projectName}"
export TMPDIR="${runTmp}" TEMP="${runTmp}" TMP="${runTmp}"
## Marked at the start, so a kept run that was killed is kept too.
((! keepTemp)) || : > "${runTmp}/keep"
runStamp="$(fPidStamp "$$")"

runSaid=0; cleaned=0; ctrTouched=0
fCleanup() {
	local stopped=0 kb=''
	((cleaned)) && return 0
	cleaned=1
	trap '' INT TERM HUP
	((runSaid)) || printf '\n[ CICD ABORTED (stopped) ]\n' >&2
	stopped="$(fStopLeftovers "${runTmp}")"
	fSection "$(fNow): Cleanup"
	((stopped == 0)) || fEcho_Clean "stopped ${stopped} process(es) still running from this run's temp folder"
	((! ctrTouched)) || fCtrCleanup
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
## Container. The image is built from CTR_DIR, and the CTR_STEPS run in it as
## this script again. Everything here carries the project's and the run's
## labels, and cleanup touches nothing without them.

runName="${runTmp##*/}"
ctrOn=0; ctrPassed=0; ctrTag=""
## Its own target and cargo home. The paths inside differ from the host's, so
## sharing target/ would rebuild everything each time a run switched sides.
## docker reads a relative -v source as a named volume.
hostTarget="${CARGO_TARGET_DIR:-${root}/target}"
[[ "${hostTarget}" == /* ]] || hostTarget="${root}/${hostTarget}"
ctrBase="${hostTarget}/container"
## dev.pid is the run's "<pid> <start time>", so a later run can tell it's gone.
ctrLabels=(--label "dev.project=${projectName}" --label "dev.run=${runName}" --label "dev.pid=${runStamp}")

## Why docker can't be used, or nothing. One with no daemon behind it counts as
## missing. The timeout is for a DOCKER_HOST that never replies.
fCtrWhyNot() {
	local -a limit=()
	if [[ "$(uname -m)" != x86_64 ]]; then echo "the image is x86_64 only"; return 0; fi
	command -v docker > /dev/null 2>&1 || { echo "there's no docker here"; return 0; }
	if command -v timeout > /dev/null 2>&1; then limit=(timeout 30); fi
	"${limit[@]}" docker info > /dev/null 2>&1 || echo "docker doesn't answer"
	return 0
}

## True while the run behind a container is going: by its dev.pid label, or
## for one made before that label, by its run folder.
fCtrRunLive() {   ## fCtrRunLive RUN STAMP
	if [[ -n "${2}" ]]; then fStampLive "${2}"; return; fi
	[[ -n "${1}" ]] && fRunLive "${HOST_TEMP}/${projectName}/${1}"
}

## The project's containers, one "id|run|stamp" a line, this run's left out.
fCtrOthers() {
	docker ps -a --filter "label=dev.project=${projectName}" --format '{{.ID}}|{{.Label "dev.run"}}|{{.Label "dev.pid"}}' 2> /dev/null \
		| awk -F'|' -v me="${runName}" '$2 != me' || true
}

## Stops and removes containers whose run is gone, such as after a kill -9,
## other than those of a kept run folder, then sweeps run folders again, since
## a container, or the gone run's docker client, can keep a folder from the
## sweep at the start.
fCtrSweepGone() {
	local id='' run='' stamp='' n=0 left=0
	local -a gone=() runs=()
	while IFS='|' read -r id run stamp; do
		[[ -n "${id}" ]] || continue
		! fCtrRunLive "${run}" "${stamp}" || continue
		[[ -z "${run}" || ! -e "${HOST_TEMP}/${projectName}/${run}/keep" ]] || continue
		gone+=("${id}"); [[ -z "${run}" ]] || runs+=("${run}")
	done < <(fCtrOthers)
	if ((${#gone[@]})); then
		docker stop -t 5 "${gone[@]}" > /dev/null 2>&1 || true
		docker rm "${gone[@]}" > /dev/null 2>&1 || true
		fEcho_Clean "stopped and removed ${#gone[@]} container(s) whose run is gone"
	fi
	## The gone run's docker client outlives its container by a moment.
	for n in {1..25}; do
		left=0
		for run in "${runs[@]}"; do [[ -z "$(fRunPids "${HOST_TEMP}/${projectName}/${run}")" ]] || left=1; done
		((left)) || break
		sleep 0.2
	done
	n="$(fSweepStale)"
	staleGone=$((staleGone + n))
}

## The name of another live run of the project, or nothing: one with a run
## folder here, or with a container, which covers one with another temp root.
fOtherRun() {
	local d='' id='' run='' stamp=''
	for d in "${HOST_TEMP}/${projectName}"/*_[0-9][0-9][0-9]; do
		[[ "${d}" != "${runTmp}" && -d "${d}" ]] || continue
		if fRunLive "${d}"; then echo "${d##*/}"; return 0; fi
	done
	while IFS='|' read -r id run stamp; do
		[[ -n "${id}" ]] || continue
		if fCtrRunLive "${run}" "${stamp}"; then echo "${run:-${id}}"; return 0; fi
	done < <(fCtrOthers)
}

## Copies the recipe to DIR and sets ctrTag from a hash of it and the build
## args, so an edit to any of them means a new image.
fCtrContext() {   ## fCtrContext DIR
	local ctx="$1" f
	local -a ctxFiles=()
	mkdir -p -- "${ctx}" || return 1
	cp -p -- "${root}/${CTR_DIR}"/* "${ctx}/" || return 1
	for f in "${CTR_CONTEXT_EXTRA[@]}"; do cp -p -- "${root}/${f}" "${ctx}/" || return 1; done
	mapfile -t ctxFiles < <(cd "${ctx}" && find . -type f | LC_ALL=C sort)
	ctrTag="${CTR_IMAGE}:$({ (cd "${ctx}" && for f in "${ctxFiles[@]}"; do echo "${f}"; cat "${f}"; done); printf 'build-arg %s\n' "${CTR_BUILD_ARGS[@]}"; } | git hash-object --stdin | cut -c1-12)"
}

## Every container goes through here. nice on the client doesn't reach a
## container, so each gets the run's job count at a low share, and the ones
## that build run nice inside too. The init passes a Ctrl+C to the whole
## group inside, or cargo would run on to the end.
fCtrRun() {   ## fCtrRun STEP DOCKER_RUN_ARG...
	local step="$1"
	local -a rm=(--rm)
	shift
	((! keepTemp)) || rm=()
	docker run "${rm[@]}" --init -e TINI_KILL_PROCESS_GROUP=1 --name "${projectName}-${runName}-${step}" "${ctrLabels[@]}" \
		--cpus "${jobs}" --cpu-shares 256 --user "$(id -u):$(id -g)" "$@"
}

## Builds the image when this recipe has none, checks it answers, and fills
## its cargo home for the offline steps. Anything that goes wrong is one line,
## and the run goes on on the host.
fStepImage() {
	local why='' have='' want='' got='' ch="${ctrBase}/cargo-home" i arg
	local -a inside=() buildArgs=()
	why="$(fCtrWhyNot)"
	if [[ -n "${why}" ]]; then fEcho_Clean "${why}, so this run is on the host"; return 0; fi
	ctrTouched=1
	fCtrSweepGone
	fCtrContext "${runTmp}/ctx" || { fEcho_Clean "could not copy the image's recipe, so this run is on the host"; return 0; }
	if docker image inspect "${ctrTag}" > /dev/null 2>&1; then
		fEcho_Clean "${ctrTag} is here"
	else
		fEcho_Clean "building ${ctrTag}, which takes several minutes the first time"
		## dev.built orders the tags for cleanup. Created can't, since a build
		## from cache gives every tag the same one.
		buildArgs=(); for arg in "${CTR_BUILD_ARGS[@]}"; do buildArgs+=(--build-arg "${arg}"); done
		if ! docker build --progress=plain "${ctrLabels[@]}" --label "dev.built=${EPOCHSECONDS}" "${buildArgs[@]}" -t "${ctrTag}" "${runTmp}/ctx" > "${runTmp}/image-build.log" 2>&1; then
			tail -n 25 "${runTmp}/image-build.log"
			fEcho_Clean "could not build ${ctrTag}, so this run is on the host"
			return 0
		fi
		fEcho_Clean "built ${ctrTag}"
	fi
	want="$(sed -n 's/^channel *= *"\(.*\)"/\1/p' "${root}/rust-toolchain.toml")"
	have="$(fCtrRun check --network none -e HOME=/tmp "${ctrTag}" rustc --version 2>&1 || true)"
	if [[ "${have}" != "rustc ${want} "* ]]; then
		fEcho_Clean "${ctrTag} says '${have}', not rustc ${want}, so this run is on the host"
		return 0
	fi
	ctrPassed=1
	mkdir -p -- "${ch}" "${ctrBase}/target" "${runTmp}/home" || return 1
	want="$(sha256sum < "${root}/Cargo.lock" | cut -c1-16) ${ctrTag}"
	{ read -r got < "${ch}/.fetched"; } 2> /dev/null || true
	if [[ "${got}" != "${want}" ]]; then
		## -Zbuild-std needs std's own crates too, from its Cargo.lock in rust-src.
		fEcho_Clean "cargo fetch --locked, for the steps, which run offline, and for std's build on ${RUST_NIGHTLY}"
		if fCtrRun fetch -v "${root}:${root}:ro" -v "${ch}:${ch}" -w "${root}" -e "HOME=${ch}" -e "CARGO_HOME=${ch}" \
			"${ctrTag}" nice -n 19 cargo fetch --locked --quiet \
			&& fCtrRun fetch-std -v "${ch}:${ch}" -e "HOME=${ch}" -e "CARGO_HOME=${ch}" "${ctrTag}" nice -n 19 cargo "+${RUST_NIGHTLY}" fetch --locked --quiet \
				--manifest-path "/opt/rustup/toolchains/${RUST_NIGHTLY}-x86_64-unknown-linux-gnu/lib/rustlib/src/rust/library/Cargo.toml"; then
			printf '%s\n' "${want}" > "${ch}/.fetched"
		else
			fEcho_Clean "WARNING: could not fetch crates (offline?); going on with what's cached"
		fi
	fi
	ctrOn=1
	for i in "${!planFns[@]}"; do fInImage "${planFns[i]}" && inside+=("${planNames[i]}"); done
	fEcho_Clean "in ${ctrTag}: ${inside[*]:-none of the steps}"
}

## One step in the image, as this script with the run's own options. The
## repo, the run's temp folder and the image's target dir are mounted at their
## own paths, so every path reads the same inside.
fCtrStep() {   ## fCtrStep FUNCTION
	local fn="$1" step='' common=''
	local -a mounts=() env=() net=()
	step="${fn#fStep}"; step="${step,,}"
	mounts=(-v "${root}:${root}" -v "${runTmp}:${runTmp}" -v "${ctrBase}:${ctrBase}")
	## A linked worktree's git dir is outside it, and lint lists files with git.
	common="$(git rev-parse --path-format=absolute --git-common-dir 2> /dev/null || true)"
	if [[ -n "${common}" && "${common}" != "${root}"/* ]]; then mounts+=(-v "${common}:${common}:ro"); fi
	env=(-e "TUKZMON_IN_IMAGE=${fn}" -e "TUKZMON_JOBS=${jobs}" -e "HOME=${runTmp}/home"
		-e "TMPDIR=${runTmp}" -e "TEMP=${runTmp}" -e "TMP=${runTmp}"
		-e "CARGO_HOME=${ctrBase}/cargo-home" -e "CARGO_TARGET_DIR=${ctrBase}/target" -e CARGO_NET_OFFLINE=true
		-e CARGO_TERM_QUIET -e SOURCE_DATE_EPOCH -e TUKZMON_RELEASE -e TUKZMON_MIRI_FILTER)
	[[ ! -t 1 ]] || env+=(-e CARGO_TERM_COLOR=always)
	## A run that dogfoods built the release here, so the packages come from
	## that one.
	if [[ "${fn}" == fStepPackage ]] && ! fInImage fStepRelease; then
		env+=(-e "TUKZMON_RELEASE_DIR=${hostTarget}/release")
		[[ "${hostTarget}" == "${root}"/* ]] || mounts+=(-v "${hostTarget}/release:${hostTarget}/release:ro")
	fi
	net=(--network none)
	if [[ " ${CTR_NET_STEPS[*]} " == *" ${fn} "* ]]; then net=(); fi
	fEcho_Clean "in ${projectName}-${runName}-${step}"
	fCtrRun "${step}" "${net[@]}" "${mounts[@]}" -w "${root}" "${env[@]}" \
		"${ctrTag}" nice -n 19 ionice -c 3 bash "${root}/cicd/cicd.bash" "${cliArgs[@]}"
}

## The images' total from docker system df, in bytes. Layers are shared, so
## the sizes of what was removed would overstate what it freed.
fCtrImagesBytes() {
	docker system df --format '{{.Type}}|{{.Size}}' 2> /dev/null | awk -F'|' '$1 == "Images" {
		n = $2 + 0; u = $2; sub(/^[0-9.]+/, "", u)
		m = (u == "kB") ? 1e3 : (u == "MB") ? 1e6 : (u == "GB") ? 1e9 : (u == "TB") ? 1e12 : 1
		printf "%.0f\n", n * m }' || true
}

## The run's containers, the project's dangling images, and its older tags
## once this run's image has answered, keeping it and the one before. Then a
## week of build cache is kept for every project. No image goes while another
## run is live, since its half-built image has no tag yet, and the tag it
## runs from may be an older one.
fCtrCleanup() {
	local tag='' seen=0 kept=0 before=0 after=0 freed='' pruned='' other='' i
	local -a ids=() doomed=() gone=() tags=() built=() order=()
	mapfile -t ids < <(docker ps -aq --filter "label=dev.project=${projectName}" --filter "label=dev.run=${runName}" 2> /dev/null || true)
	if ((keepTemp)); then
		((${#ids[@]} == 0)) || fEcho_Clean "kept ${#ids[@]} container(s): docker ps -a --filter label=dev.run=${runName}"
	elif ((${#ids[@]})); then
		## The --rm ones go away on the stop, so rm can find them gone.
		docker stop -t 5 "${ids[@]}" > /dev/null 2>&1 || true
		docker rm "${ids[@]}" > /dev/null 2>&1 || true
		fEcho_Clean "stopped and removed ${#ids[@]} container(s) left by this run"
	fi
	other="$(fOtherRun)"
	if [[ -n "${other}" ]]; then
		fEcho_Clean "left the images alone, since run ${other} is live"
	else
		mapfile -t doomed < <(docker image ls -q --no-trunc --filter dangling=true --filter "label=dev.project=${projectName}" 2> /dev/null | LC_ALL=C sort -u || true)
	fi
	if ((ctrPassed)) && [[ -z "${other}" ]]; then
		mapfile -t tags < <(docker image ls "${CTR_IMAGE}" --filter "label=dev.project=${projectName}" --format '{{.Repository}}:{{.Tag}}' 2> /dev/null || true)
		((${#tags[@]} == 0)) || mapfile -t built < <(docker image inspect --format '{{index .Config.Labels "dev.built"}}' "${tags[@]}" 2> /dev/null || true)
		## Newest first. One with no stamp counts as oldest.
		for i in "${!tags[@]}"; do
			[[ "${built[i]:-}" =~ ^[0-9]+$ ]] || built[i]=0
			order+=("${built[i]}|${tags[i]}")
		done
		while IFS='|' read -r _ tag; do
			if [[ "${tag}" == "${ctrTag}" ]]; then seen=1; continue; fi
			((seen)) || continue
			if ((! kept)); then kept=1; continue; fi
			doomed+=("${tag}")
		done < <(((${#order[@]} == 0)) || printf '%s\n' "${order[@]}" | sort -t '|' -k1,1nr)
	fi
	if ((${#doomed[@]})); then
		before="$(fCtrImagesBytes)"
		## Not -f, so an image a container still uses stays.
		for tag in "${doomed[@]}"; do
			if ! docker rmi "${tag}" > /dev/null 2>&1; then continue; fi
			if [[ "${tag}" == sha256:* ]]; then gone+=("<none> ${tag:7:12}"); else gone+=("${tag}"); fi
		done
		after="$(fCtrImagesBytes)"
		## Other projects build meanwhile, so it's only roughly ours.
		if ((${before:-0} > ${after:-0})); then freed=$((before - after)); else freed=0; fi
		freed=", about $((freed / 1000000)) MB"
	fi
	pruned="$(docker builder prune -f --filter until=168h 2> /dev/null | sed -n 's/^Total\( reclaimed space\)\{0,1\}:[[:space:]]*//p' || true)"
	fEcho_Clean "docker: removed ${gone[*]:-no image}${freed}, and ${pruned:-0B} of build cache"
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
	if [[ " ${HEAVY_STEPS[*]} " == *" ${planFns[i]} "* ]]; then fJobs heavy; else fJobs; fi
	fEcho_Clean; fEcho_Clean "${__letterbox}"
	fEcho "$(fNow): Step ${n} of ${stepCount}; Starting ${name}"
	if [[ "${loadNote}" != "${lastNote}" ]]; then fEcho_Clean "jobs: ${loadNote}"; lastNote="${loadNote}"; fi
	stepRun=("${planFns[i]}")
	if ((ctrOn)) && fInImage "${planFns[i]}"; then stepRun=(fCtrStep "${planFns[i]}"); fi
	if "${stepRun[@]}"; then
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
##		- 2026-10-25: Format, build, lint, tests and release in the image from
##		  cicd/container/, with labeled containers and their cleanup.
##		- 2026-10-25: Test IDs on each test's line, checked with lint, and in
##		  --list.
##		- 2026-10-25: Containers and run folders a killed run left are cleaned
##		  up by the next run, and no image goes while another run is live.
##		- 2026-10-25: The known-advisory scan, in the image with the network.
##		- 2026-10-25: The Sanitizers leak check on a pinned nightly, leak check
##		  lengths, and a quarter of the CPUs for heavy steps.
##		- 2026-10-25: The Miri leak check.
##		- 2026-10-25: The Soak leak check.
##		- 2026-10-25: .deb and .rpm packages, with sha256 sums.
##		- 2026-10-25: The profiler, with a report on each flamegraph.
