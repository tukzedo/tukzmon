#!/usr/bin/env bash

#  shellcheck enable=require-variable-braces  ## Every expansion braced: "${var}", not "$var".

##	Purpose:
##		Test IDs. Every test has one, from when it was first committed: whole
##		minutes since 2000-01-01 00:00 UTC, in lower-case Crockford base 32, so
##		2026-10-24 22:00 UTC is decp8. 2 that come out the same take the next
##		free minute, and an ID never changes, even when its test is renamed or
##		moved. A Rust test carries it on a "// Test ID: decp8" line just above
##		its attributes, and a test of the pipeline in cicd/tests/*.bash on a
##		"## Test ID: decp8" line just above its fTest_ function.
##		Ported from silkterm's cicd/utility/test-id.py, which was base 62 in ms.
##	Syntax:
##		test-ids.bash new [N]           print N fresh IDs for now (default 1)
##		test-ids.bash at WHEN [N]       the same for a past time, as date -d reads it
##		test-ids.bash when ID ...       the time each ID stands for, in UTC
##		test-ids.bash list              every test, as ID and name
##		test-ids.bash check             every test has a sane ID and no 2 share one,
##		                                every Done item in the backlog has a
##		                                Test case row, and every ID one names
##		                                is a test's
##		test-ids.bash annotate          a filter on cargo test's output (stderr
##		                                too), turning each libtest result line
##		                                into "PASS|FAIL|SKIP ID name"
##	Exit: 0 fine, 1 check found a problem, 2 bad arguments.
##	History: At bottom of script.

##	Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)
##	Licensed under The MIT License (MIT). Full text at:
##		https://mit-license.org/
##	SPDX-License-Identifier: MIT


set -Eeuo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
digits="0123456789abcdefghjkmnpqrstvwxyz"
epoch2000=946684800

## Both set the caller's VAR rather than print, so a loop over them doesn't fork.
fEncode() {   ## fEncode MINUTES VAR
	local -i n="$1"; local out=""
	while ((n)); do out="${digits:n % 32:1}${out}"; n=$((n / 32)); done
	printf -v "$2" '%s' "${out:-0}"
}
fDecode() {   ## fDecode ID VAR -> minutes, or fails on a letter that isn't ours
	local id="$1" c head; local -i n=0 i
	for ((i = 0; i < ${#id}; i++)); do
		c="${id:i:1}"; head="${digits%%"${c}"*}"
		[[ "${head}" != "${digits}" ]] || return 1
		n=$((n * 32 + ${#head}))
	done
	printf -v "$2" '%d' "${n}"
}

## Each workspace member as "dir<TAB>crate", where crate is the name libtest's
## binary goes by: the [lib] name when there is one, else the package's, with
## - as _.
fMembers() {
	local m pkg lib; local -a members=()
	read -r -a members < <(awk '/^members *=/{ gsub(/[][",=]|members/, " "); print }' "${root}/Cargo.toml")
	for m in "${members[@]}"; do
		[[ -f "${root}/${m}/Cargo.toml" ]] || continue
		read -r pkg lib < <(awk -F '"' '/^\[/{ sect = $0 } /^name *=/{ if (sect == "[package]") p = $2; if (sect == "[lib]") l = $2 } END { print p, l }' "${root}/${m}/Cargo.toml")
		lib="${lib:-${pkg}}"
		printf '%s\t%s\n' "${m}" "${lib//-/_}"
	done
}

## Every Rust test as "id<TAB>crate<TAB>name<TAB>file:line", with "-" for a
## missing ID. The module path comes from the file's place under src/ and the
## indent of each "mod x {", which rustfmt keeps exact. One awk for every
## file, with crate, base and file set on its command line before each one.
fScanRust() {   ## fScanRust [crate=CRATE base=MODULE_PATH file=REL FILE]...
	(($#)) || return 0
	awk '
		function scan(   depth, n, s, ind, top, id, fn, k, path, d) {
			depth = 0
			for (n = 1; n <= nl; n++) {
				if (match(line[n], /^\t*(pub(\([^)]*\))? +)?mod +[A-Za-z0-9_]+ *\{/)) {
					s = line[n]; ind = s; sub(/[^\t].*/, "", ind)
					sub(/^\t*(pub(\([^)]*\))? +)?mod +/, "", s); sub(/[^A-Za-z0-9_].*/, "", s)
					depth++; mind[depth] = ind; mname[depth] = s
					continue
				}
				if (depth && index(line[n], mind[depth] "}") == 1) { depth--; continue }
				if (line[n] !~ /^[ \t]*#\[test\][ \t]*$/) continue
				top = n
				while (top > 1 && line[top - 1] ~ /^[ \t]*#\[.*\][ \t]*$/) top--
				id = "-"
				if (top > 1 && line[top - 1] ~ /^[ \t]*\/\/[ \t]*Test ID:/) {
					id = line[top - 1]; sub(/^[ \t]*\/\/[ \t]*Test ID:[ \t]*/, "", id); sub(/[ \t]+$/, "", id)
					if (id == "") id = "-"
				}
				fn = "?"
				for (k = n + 1; k <= n + 20 && k <= nl; k++) {
					if (match(line[k], /fn +[A-Za-z0-9_]+/)) { fn = substr(line[k], RSTART, RLENGTH); sub(/fn +/, "", fn); break }
				}
				path = pb
				for (d = 1; d <= depth; d++) path = path mname[d] "::"
				printf "%s\t%s\t%s%s\t%s:%d\n", id, pc, path, fn, pf, n
			}
		}
		FNR == 1 { if (nl) scan(); nl = 0; pc = crate; pb = base; pf = file }
		{ line[++nl] = $0 }
		END { if (nl) scan() }' "$@"
}
fScan() {
	local dir crate f rel mods; local -a args=()
	while IFS=$'\t' read -r dir crate; do
		[[ -d "${root}/${dir}/src" ]] || continue
		while IFS= read -r f; do
			rel="${f#"${root}/${dir}/src/"}"; rel="${rel%.rs}"; rel="${rel%/mod}"
			case "${rel}" in lib|main) mods="" ;; *) mods="${rel//\//::}::" ;; esac
			args+=("crate=${crate}" "base=${mods}" "file=${f#"${root}/"}" "${f}")
		done < <(find "${root}/${dir}/src" -name '*.rs' -type f | LC_ALL=C sort)
		## Each file in tests/ is a test binary of its own, named after it.
		[[ -d "${root}/${dir}/tests" ]] || continue
		while IFS= read -r f; do
			rel="${f##*/}"; args+=("crate=${rel%.rs}" "base=" "file=${f#"${root}/"}" "${f}")
		done < <(find "${root}/${dir}/tests" -maxdepth 1 -name '*.rs' -type f | LC_ALL=C sort)
	done < <(fMembers)
	fScanRust "${args[@]}"
	fScanBash
}

## The pipeline's own tests, in cicd/tests/*.bash: each is an fTest_ function
## with a "## Test ID: decp8" line just above it. The crate is the file's name.
fScanBash() {
	local f; local -a files=()
	for f in "${root}"/cicd/tests/*.bash; do [[ ! -f "${f}" ]] || files+=("${f#"${root}/"}"); done
	((${#files[@]})) || return 0
	(cd "${root}" && awk '
		FNR == 1 { crate = FILENAME; sub(/.*\//, "", crate); sub(/\.bash$/, "", crate); prev = "" }
		/^fTest_[A-Za-z0-9_]+\(\)/ {
			fn = $0; sub(/\(.*/, "", fn); id = "-"
			if (prev ~ /^##[ \t]*Test ID:/) { id = prev; sub(/^##[ \t]*Test ID:[ \t]*/, "", id); sub(/[ \t]+$/, "", id); if (id == "") id = "-" }
			printf "%s\t%s\t%s\t%s:%d\n", id, crate, fn, FILENAME, FNR
		}
		{ prev = $0 }' "${files[@]}")
}

## A #[test] in a tracked .rs file outside any member's src/ and the top of
## its tests/, where fScan doesn't look.
fStrays() {
	local dir f; local -a srcs=() tracked=() strays=()
	while IFS=$'\t' read -r dir _; do srcs+=("${dir}/src/"); done < <(fMembers)
	mapfile -t tracked < <(git -C "${root}" ls-files '*.rs')
	for f in "${tracked[@]}"; do
		for dir in "${srcs[@]}"; do [[ "${f}" != "${dir}"* ]] || continue 2; done
		for dir in "${srcs[@]}"; do
			dir="${dir%src/}tests/"
			[[ "${f}" != "${dir}"* || "${f#"${dir}"}" == */* ]] || continue 2
		done
		strays+=("${f}")
	done
	((${#strays[@]})) || return 0
	(cd "${root}" && awk '/^[ \t]*#\[test\]/{ printf "%s:%d: a test outside every src/, where nothing looks for its ID\n", FILENAME, FNR }' "${strays[@]}")
}

## IDs of tests commented out once a decision made them wrong, on a
## "// // Test ID: dehy3" line. A closed item may still name one, and a new
## test mustn't take it.
fRetired() {
	local dir
	while IFS=$'\t' read -r dir _; do
		[[ -d "${root}/${dir}/src" ]] || continue
		find "${root}/${dir}/src" "${root}/${dir}/tests" -name '*.rs' -type f -exec awk '
			match($0, /^[ \t]*\/\/[ \t]*\/\/[ \t]*Test ID:[ \t]*[0-9a-z]+/) {
				s = substr($0, RSTART, RLENGTH); sub(/.*Test ID:[ \t]*/, "", s); print s
			}' {} + 2>/dev/null || true
	done < <(fMembers)
}

fNew() {   ## fNew MINUTES COUNT
	local -i at="$1" count="$2"; local -A taken=(); local id
	while IFS=$'\t' read -r id _; do taken["${id}"]=1; done < <(fScan)
	while IFS= read -r id; do taken["${id}"]=1; done < <(fRetired)
	while ((count)); do
		fEncode "${at}" id; at+=1
		[[ -z "${taken["${id}"]:-}" ]] || continue
		printf '%s\n' "${id}"; count=$((count - 1))
	done
}

fWhen() { local -i m; fDecode "$1" m || return 1; printf '%s  %s\n' "$1" "$(date -u -d "@$((epoch2000 + m * 60))" '+%Y-%m-%d %H:%M UTC')"; }

## Done items with no Test case row, and IDs a Test case row names that no
## test has. A word counts as an ID when it would be a sane one, so "seven",
## which stands for 2050, doesn't.
fBacklog() {   ## fBacklog FIRST NOW KNOWN-IDS-FILE
	local log="${root}/project/backlog.md"
	[[ -f "${log}" ]] || return 0
	awk -v first="$1" -v now="$2" -v known="$3" -v file="project/backlog.md" -v digits="${digits}" '
		BEGIN { while ((getline k < known) > 0) have[k] = 1 }
		function done_item() { if (status == "Done" && !tc) printf "%s:%d: %s is Done with no Test case row\n", file, at, id }
		/^## Template/ { done_item(); status = ""; exit }
		/^- / { done_item(); at = NR; id = "?"; status = ""; tc = 0 }
		/^\t- ID: / { id = $3 }
		/^\t- Status: / { status = $0; sub(/^\t- Status: */, "", status) }
		/^\t- Test case:/ {
			tc = 1; n = split($0, w, /[^0-9a-z]+/)
			for (i = 1; i <= n; i++) {
				if (length(w[i]) != 5) continue
				m = 0; ok = 1
				for (j = 1; j <= 5; j++) { d = index(digits, substr(w[i], j, 1)); if (!d) { ok = 0; break }; m = m * 32 + d - 1 }
				if (ok && m >= first && m <= now && !(w[i] in have)) printf "%s:%d: %s names %s, which no test has\n", file, NR, id, w[i]
			}
		}
		END { done_item() }
	' "${log}"
}

fCheck() {
	local id crate name at prob; local -i bad=0 count=0 m now first
	local -A seen=()
	now=$(( ($(printf '%(%s)T' -1) - epoch2000) / 60 + 1440 ))
	first=$(( (1767225600 - epoch2000) / 60 ))   ## 2026-01-01, before any test here
	while IFS=$'\t' read -r id crate name at; do
		count+=1; prob=""
		if [[ "${id}" == - ]]; then prob="${at}: ${name} has no Test ID line"
		elif ! fDecode "${id}" m; then prob="${at}: ${id} isn't lower-case Crockford base 32"
		elif ((m < first || m > now)); then prob="${at}: ${id} stands for $(fWhen "${id}" | cut -c 8-)"
		elif [[ -n "${seen["${id}"]:-}" ]]; then prob="${at}: ${id} is already used at ${seen["${id}"]}"
		else seen["${id}"]="${at}"; fi
		[[ -z "${prob}" ]] || { printf 'test-ids: %s\n' "${prob}" >&2; bad+=1; }
	done < <(fScan)
	while IFS= read -r prob; do printf 'test-ids: %s\n' "${prob}" >&2; bad+=1; done < <(fStrays)
	while IFS= read -r prob; do printf 'test-ids: %s\n' "${prob}" >&2; bad+=1; done < <(fBacklog "${first}" "${now}" <(printf '%s\n' "${!seen[@]}"; fRetired))
	if ((bad)); then
		printf 'test-ids: %d problem(s). A new test takes its ID from cicd/utility/test-ids.bash new\n' "${bad}" >&2
		return 1
	fi
	printf 'test-ids: %d tests, every ID unique\n' "${count}"
}

## Reads to the end, so cargo never sees a closed pipe. cargo says which test
## binary is next on stderr, so the caller sends both.
fAnnotate() {
	awk -v map=<(fScan) '
		BEGIN {
			while ((getline row < map) > 0) {
				split(row, f, "\t"); ids[f[2] "\t" f[3]] = f[1]
			}
		}
		/^ *Running / {
			crate = $0; sub(/.*\(/, "", crate); sub(/\)[ \t\r]*$/, "", crate); sub(/.*\//, "", crate); sub(/-[0-9a-f]+$/, "", crate)
		}
		/^ *Doc-tests / { crate = $2 }
		/^test .+ \.\.\. (ok|FAILED|ignored)(, .*)?$/ {
			name = $2; rest = $0; sub(/^test .+ \.\.\. /, "", rest)
			why = ""; if (index(rest, ", ")) { why = substr(rest, index(rest, ", ") + 2); rest = substr(rest, 1, index(rest, ", ") - 1) }
			status = rest == "ok" ? "PASS" : rest == "FAILED" ? "FAIL" : "SKIP"
			id = ids[crate "\t" name]
			if (id == "" || id == "-") { id = "-----"; missing++ }
			printf "%s %s %s%s\n", status, id, name, why == "" ? "" : " (" why ")"
			fflush(); next
		}
		{ print; fflush() }
		END { if (missing) printf "WARNING: %d test(s) above have no ID\n", missing }
	'
}

cmd="${1:-}"; (($#)) && shift
case "${cmd}" in
	new)
		[[ $# -le 1 && "${1:-1}" =~ ^[1-9][0-9]{0,3}$ ]] || { echo "usage: test-ids.bash new [N]" >&2; exit 2; }
		fNew $(( ($(printf '%(%s)T' -1) - epoch2000) / 60 )) "${1:-1}" ;;
	at)
		[[ $# -ge 1 && $# -le 2 && "${2:-1}" =~ ^[1-9][0-9]{0,3}$ ]] || { echo "usage: test-ids.bash at WHEN [N]" >&2; exit 2; }
		secs="$(date -d "$1" +%s)" || exit 2
		fNew $(( (secs - epoch2000) / 60 )) "${2:-1}" ;;
	when)
		(($#)) || { echo "usage: test-ids.bash when ID ..." >&2; exit 2; }
		for id in "$@"; do fWhen "${id}" || { echo "not an ID: ${id}" >&2; exit 2; }; done ;;
	list)     fScan | awk -F '\t' '{ print $1, $3 }' ;;
	check)    fCheck ;;
	annotate) fAnnotate ;;
	-h|--help) sed -n '/^##	Purpose:/,/^##	History:/p' "${BASH_SOURCE[0]}" | sed '$d; s/^##	\{0,1\}//' ;;
	*) echo "test-ids.bash: try --help" >&2; exit 2 ;;
esac


##	History:
##		- 2026-10-25: Created, from silkterm's test-id.py, in Crockford base 32
##		  minutes. check also reads the backlog's Test case rows.
##		- 2026-10-25: Tests in a member's tests/ folder too.
##		- 2026-10-28: A commented-out test's ID still counts for the backlog,
##		  and isn't handed out again.
##		- 2026-10-30: The pipeline's own tests in cicd/tests/*.bash too.
##		- 2026-10-30: No fork per ID or per file: decode and encode set a
##		  variable, and one awk reads every file.
