#!/usr/bin/env bash

#  shellcheck enable=require-variable-braces  ## Every expansion braced: "${var}", not "$var".

##	Purpose:
##		The Dogfood step's copy to the synced exec folder, so other boxes can
##		install the build with dogfood_tukzmon. Puts libtukzmon.so,
##		tukzmon-settings and tukzmon.desktop in app/linux/tukzmon, with
##		build.txt beside them, which is the version line and a sha256 sum per
##		file. utility/dogfood_tukzmon.bash goes to util/linux/bash as
##		dogfood_tukzmon. The first exec folder that exists wins, and with none
##		it says so and copies nothing.
##	Syntax: cicd/utility/dogfood-sync.bash RELEASE_DIR
##	Exit: 0 copied, already the same, or no synced folder, 1 failed.
##	History: At bottom of script.

##	Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)
##	Licensed under The MIT License (MIT). Full text at:
##		https://mit-license.org/
##	SPDX-License-Identifier: MIT


set -Eeuo pipefail

fDie() { echo "dogfood-sync: $*" >&2; exit 1; }

rel="${1:-}"
[[ -n "${rel}" && -d "${rel}" ]] || fDie "no release folder: '${rel}'"
repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

## HOME is read here, not in config.bash, so a test can point it elsewhere.
execDir=""
for d in "${HOME}/synced/0-0/common/exec" "${HOME}/.synced/Dropbox/0-0/common/exec"; do
	if [[ -d "${d}" ]]; then execDir="${d}"; break; fi
done
if [[ -z "${execDir}" ]]; then echo "Dogfood: no synced exec folder under ${HOME}, so nothing copied there."; exit 0; fi

appDir="${execDir}/app/linux/tukzmon"
utilDir="${execDir}/util/linux/bash"
declare -A from=(
	[libtukzmon.so]="${rel}/libtukzmon.so"
	[tukzmon-settings]="${rel}/tukzmon-settings"
	[tukzmon.desktop]="${repo}/xfce/tukzmon.desktop"
)
files=(libtukzmon.so tukzmon-settings tukzmon.desktop)
for f in "${files[@]}"; do [[ -f "${from[${f}]}" ]] || fDie "${from[${f}]} isn't there"; done

version="$(env -u DISPLAY -u WAYLAND_DISPLAY "${from[tukzmon-settings]}" --version 2> /dev/null | sed -n 1p || true)"
[[ -n "${version}" ]] || fDie "${from[tukzmon-settings]} --version gave nothing"
stamp="${version}"$'\n'
for f in "${files[@]}"; do stamp+="$(sha256sum < "${from[${f}]}" | cut -d ' ' -f 1)  ${f}"$'\n'; done

launcher="${repo}/utility/dogfood_tukzmon.bash"
if [[ "$(cat -- "${appDir}/build.txt" 2> /dev/null; echo .)" == "${stamp}." ]] && cmp -s -- "${launcher}" "${utilDir}/dogfood_tukzmon"; then
	echo "Dogfood: ${appDir} is already ${version}."
	exit 0
fi

mkdir -p -- "${appDir}" "${utilDir}" || fDie "couldn't make ${appDir} or ${utilDir}"
## Beside it, then renamed, so a reader never sees half a file. The stamp
## goes last, though the sync may still bring them over in any order.
fPut() {
	if ! { cp --preserve=mode -- "${1}" "${2}.new.$$" && mv -f -- "${2}.new.$$" "${2}"; }; then
		rm -f -- "${2}.new.$$"; fDie "couldn't copy ${1} to ${2}"
	fi
}
for f in "${files[@]}"; do fPut "${from[${f}]}" "${appDir}/${f}"; done
if ! { printf '%s' "${stamp}" > "${appDir}/build.txt.new.$$" && mv -f -- "${appDir}/build.txt.new.$$" "${appDir}/build.txt"; }; then
	rm -f -- "${appDir}/build.txt.new.$$"; fDie "couldn't write ${appDir}/build.txt"
fi
fPut "${launcher}" "${utilDir}/dogfood_tukzmon"
chmod 755 -- "${utilDir}/dogfood_tukzmon"
echo "Dogfood: ${version} -> ${appDir}, and dogfood_tukzmon -> ${utilDir}"


##	History:
##		- 2026-11-20: Created.
