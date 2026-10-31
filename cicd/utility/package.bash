#!/usr/bin/env bash

#  shellcheck disable=2016  ## 'Expressions don't expand in single quotes.' The ${TUKZMON_*} names are nfpm.yaml's, filled by sed.
#  shellcheck enable=require-variable-braces  ## Every expansion braced: "${var}", not "$var".

##	Purpose:
##		- A .deb and an .rpm through nfpm from the release build in
##		  RELEASE_DIR, named tukzmon-<version>-linux-x86_64.<deb|rpm>, with
##		  tukzmon-<version>-sha256sums.txt beside them in OUT_DIR.
##		- Two builds of one commit give the same bytes: the mtimes inside are
##		  SOURCE_DATE_EPOCH, else HEAD's commit time.
##		- Checks each package holds what nfpm.yaml says, and that the .deb's
##		  depends name every library the binaries link, where dpkg can say
##		  which package owns one.
##		- Writes only into OUT_DIR and WORK_DIR, and removes nothing but the
##		  same-named files it replaces in OUT_DIR.
##	Syntax:
##		package.bash RELEASE_DIR VERSION OUT_DIR WORK_DIR
##		NFPM names the nfpm to run, if it isn't on PATH.
##	History: At bottom of script.

##	Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)
##	Licensed under The MIT License (MIT). Full text at:
##		https://mit-license.org/
##	SPDX-License-Identifier: MIT


set -Eeuo pipefail

usage="usage: package.bash RELEASE_DIR VERSION OUT_DIR WORK_DIR"
relDir="${1:?${usage}}"; ver="${2:?${usage}}"; outDir="${3:?${usage}}"; workDir="${4:?${usage}}"
root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
nfpm="${NFPM:-nfpm}"
fDie() { echo "package: $*" >&2; exit 1; }

[[ "${ver}" =~ ^[0-9]+\.[0-9]+\.[0-9]+([-+][0-9A-Za-z.-]+)?$ ]] || fDie "not a version: '${ver}'"
[[ -d "${outDir}" && -d "${workDir}" ]] || fDie "no such dir: ${outDir} or ${workDir}"
command -v "${nfpm}" > /dev/null 2>&1 || fDie "nfpm isn't installed (pinned in cicd/config.bash), or set NFPM"
bins=("${relDir}/libtukzmon.so" "${relDir}/tukzmon-settings")
for f in "${bins[@]}"; do [[ -f "${f}" ]] || fDie "no release build at ${f}"; done
cd "${root}"

epoch="${SOURCE_DATE_EPOCH:-$(git log -1 --format=%ct 2> /dev/null || true)}"
[[ "${epoch}" =~ ^[0-9]{1,15}$ ]] || fDie "no SOURCE_DATE_EPOCH and no commit time to use"
mtime="$(date -u -d "@${epoch}" +%Y-%m-%dT%H:%M:%SZ)"

glibc=""; needed=""
for f in "${bins[@]}"; do
	glibc+="$(objdump -T "${f}" | { grep -o 'GLIBC_[0-9.]*' || true; } | sed 's/GLIBC_//')"$'\n'
	## Into a variable first: a grep that quits early kills readelf with
	## SIGPIPE under pipefail.
	needed+="$(readelf -d "${f}")"$'\n'
done
glibc="$(sort -uV <<< "${glibc}" | tail -n 1)"
[[ -n "${glibc}" ]] || fDie "no GLIBC_ symbol version in the release build"
mapfile -t sonames < <(sed -n 's/.*(NEEDED).*\[\(.*\)\]$/\1/p' <<< "${needed}" | LC_ALL=C sort -u)
rpmSonames=""
for s in "${sonames[@]}"; do rpmSonames+="\\n      - ${s}()(64bit)"; done

yaml="${workDir}/nfpm.yaml"
sed -e "s|\${TUKZMON_VERSION}|${ver}|g" -e "s|\${TUKZMON_ARCH}|amd64|g" -e "s|\${TUKZMON_MTIME}|${mtime}|g" \
	-e "s|\${TUKZMON_GLIBC}|${glibc}|g" -e "s|\${TUKZMON_RELEASE_DIR}|${relDir}|g" -e "s|\${TUKZMON_RPM_SONAMES}|${rpmSonames}|g" \
	cicd/packaging/nfpm.yaml > "${yaml}"
if grep -q '^[^#]*\${TUKZMON_' "${yaml}"; then fDie "nfpm.yaml has a name package.bash doesn't fill"; fi

## Every linked library's owner has to be among the .deb's depends.
mapfile -t debDeps < <(sed -n '/^  deb:/,/^  [a-z]/s/^      - \([^ ]*\).*/\1/p' "${yaml}"; sed -n '/^  deb:/,/^  [a-z]/s/^      - .* | \([^ ]*\).*/\1/p' "${yaml}")
unowned=0
if command -v dpkg-query > /dev/null 2>&1; then
	## One query for them all. Each line is "pkg[:arch][, pkg...]: path", and
	## the first one for a soname wins.
	declare -A owners=()
	if ((${#sonames[@]})); then
		while IFS= read -r line; do
			s="${line##*/}"
			[[ -n "${owners["${s}"]:-}" ]] || owners["${s}"]="${line%%[:,]*}"
		done < <(dpkg-query -S "${sonames[@]/#/*/x86_64-linux-gnu/}" 2> /dev/null || true)
	fi
	for s in "${sonames[@]}"; do
		owner="${owners["${s}"]:-}"
		if [[ -z "${owner}" ]]; then unowned=$((unowned + 1)); continue; fi
		[[ " ${debDeps[*]} " == *" ${owner} "* ]] || fDie "${s} is from ${owner}, which the .deb's depends in nfpm.yaml don't name"
	done
	((unowned == 0)) || echo "WARNING: dpkg owns no file for ${unowned} of the linked libraries, so those went unchecked"
else
	echo "WARNING: no dpkg here, so the .deb's depends went unchecked against the libraries"
fi

base="tukzmon-${ver}-linux-x86_64"
sums="tukzmon-${ver}-sha256sums.txt"
for fmt in deb rpm; do
	"${nfpm}" package -f "${yaml}" -p "${fmt}" -t "${workDir}/${base}.${fmt}" > /dev/null || fDie "nfpm failed on ${base}.${fmt}"
done

## What went in, by path, against what nfpm.yaml asks for.
want="$(printf '%s\n' /usr/bin/tukzmon-settings /usr/share/xfce4/panel/plugins/tukzmon.desktop)"
got="$(dpkg-deb -c "${workDir}/${base}.deb" 2> /dev/null | awk '$1 !~ /^d/ { sub(/^\./, "", $6); print $6 }' | LC_ALL=C sort)"
[[ "${got}" == "$(printf '%s\n' "${want}" /usr/lib/x86_64-linux-gnu/xfce4/panel/plugins/libtukzmon.so /usr/share/doc/tukzmon/copyright | LC_ALL=C sort)" ]] \
	|| fDie "the .deb holds something else:"$'\n'"${got}"
if command -v rpm > /dev/null 2>&1; then
	got="$(rpm -qlp "${workDir}/${base}.rpm" 2> /dev/null | LC_ALL=C sort)"
	[[ "${got}" == "$(printf '%s\n' "${want}" /usr/lib64/xfce4/panel/plugins/libtukzmon.so /usr/share/licenses/tukzmon/license.txt | LC_ALL=C sort)" ]] \
		|| fDie "the .rpm holds something else:"$'\n'"${got}"
else
	echo "WARNING: no rpm here, so the .rpm's contents went unchecked"
fi

(cd "${workDir}" && sha256sum -- "${base}.deb" "${base}.rpm" > "${sums}")
for f in "${base}.deb" "${base}.rpm" "${sums}"; do
	cp -- "${workDir}/${f}" "${outDir}/.${f}.new"
	mv -f -- "${outDir}/.${f}.new" "${outDir}/${f}"
	echo "${f} ($(stat -c %s -- "${outDir}/${f}") bytes)"
done
echo "depends: glibc ${glibc}, ${#sonames[@]} linked libraries"


##	History:
##		- 2026-10-25: Created from tukfs-hash's package.bash: the plugin, its
##		  .desktop file and tukzmon-settings, with the depends checked.
##		- 2026-10-30: One dpkg-query for every linked library.
