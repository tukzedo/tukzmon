#!/usr/bin/env bash

#  shellcheck enable=require-variable-braces  ## Every expansion braced: "${var}", not "$var".

##	Purpose:
##		Installs the newest dogfood build of tukzmon for this user, from the
##		synced app/linux/tukzmon folder that cicd.bash's Dogfood step fills, and
##		restarts the panel so it loads it. The same user plugin folders as
##		utility/dev-install.bash, so no root is needed. Does nothing when the
##		installed build is already that one.
##		- cicd.bash puts it in util/linux/bash as dogfood_tukzmon.
##	Syntax:
##		dogfood_tukzmon [--no-restart]
##		  --no-restart  install, but leave the running panel alone
##	Exit: 0 installed or already the same, 1 failed, 2 bad arguments.
##	History: At bottom of script.

##	Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)
##	Licensed under The MIT License (MIT). Full text at:
##		https://mit-license.org/
##	SPDX-License-Identifier: MIT


set -Eeuo pipefail

declare -i __wasLastEchoBlank=0
fEcho_Clean() {
	if [[ -n "${1:-}" ]]; then printf '%s\n' "$*"; __wasLastEchoBlank=0
	elif ((! __wasLastEchoBlank)); then echo; __wasLastEchoBlank=1; fi
}
fEcho() { if [[ -n "$*" ]]; then fEcho_Clean "[ $* ]"; else fEcho_Clean ""; fi; }
fDie() { fEcho "dogfood_tukzmon: $*" >&2; exit 1; }

restart=1
while (($#)); do case "$1" in
	--no-restart) restart=0; shift ;;
	-h|--help)    sed -n '/^##	Purpose:/,/^##	History:/p' "${BASH_SOURCE[0]}" | sed '$d; s/^##\t\{0,1\}//'; exit 0 ;;
	*)            fEcho "dogfood_tukzmon: unknown option: $1 (try --help)" >&2; exit 2 ;;
esac; done

files=(libtukzmon.so tukzmon-settings tukzmon.desktop)
stampName="build.txt"

## util/linux/bash sits 3 down from the exec folder that has app/linux. The
## home spellings are for a copy run from somewhere else.
self="$(readlink -f -- "${BASH_SOURCE[0]}" || true)"
src=""
for dir in ${self:+"${self%/*}/../../../app/linux/tukzmon"} \
	"${HOME}/synced/0-0/common/exec/app/linux/tukzmon" \
	"${HOME}/.synced/Dropbox/0-0/common/exec/app/linux/tukzmon"; do
	if [[ -f "${dir}/${stampName}" ]]; then src="$(cd -P -- "${dir}" && pwd)"; break; fi
done
[[ -n "${src}" ]] || fDie "no dogfood build found, in app/linux/tukzmon beside this script or under ~/synced"

## The stamp is the version line, then a sha256 sum per file. The sync can
## bring the files in any order, so a sum that doesn't match is a build still
## on its way.
build="$(head -n 1 -- "${src}/${stampName}")"
(cd -- "${src}" && tail -n +2 -- "${stampName}" | sha256sum --check --status --strict 2> /dev/null) \
	|| fDie "${src} doesn't match its ${stampName} yet, so it may still be syncing. Try again in a while."
for f in "${files[@]}"; do
	grep -qE "^[0-9a-f]{64}  ${f}\$" -- "${src}/${stampName}" || fDie "${src}/${stampName} has no sum for ${f}"
done

## The panel looks for user plugins in a lib dir under the data dir's prefix,
## with the same tail as its own lib dir. The same as dev-install.bash.
dataHome="${XDG_DATA_HOME:-${HOME}/.local/share}"
if [[ "${dataHome}" != */share ]]; then
	fDie "XDG_DATA_HOME is '${dataHome}', and the panel doesn't read any user plugins unless it ends in /share."
fi
## ldconfig is in /usr/sbin, which isn't on a Debian user's PATH.
panelLib="$(grep -m1 -F 'libxfce4panel-2.0.so.4 ' <<< "$(PATH="${PATH}:/usr/sbin:/sbin" ldconfig -p 2> /dev/null || true)" | sed 's/.*=> //' || true)"
if [[ -n "${panelLib}" ]]; then
	panelLibDir="${panelLib%/*}"
	libTail="lib${panelLibDir##*/lib}"
else
	libTail="lib/$(uname -m)-linux-gnu"
	fEcho "WARNING: libxfce4panel-2.0.so.4 not found, so assuming the panel's lib dir is ${libTail}."
fi
shareDir="${dataHome}/xfce4/panel/plugins"
libDir="${dataHome%/share}/${libTail}/xfce4/panel/plugins"
fDest() { if [[ "$1" == *.desktop ]]; then echo "${shareDir}"; else echo "${libDir}"; fi; }

same=1
for f in "${files[@]}"; do cmp -s -- "${src}/${f}" "$(fDest "${f}")/${f}" || { same=0; break; }; done
if ((same)); then
	fEcho "Already installed: ${build}"
	exit 0
fi

mkdir -p -- "${shareDir}" "${libDir}"
## copy then rename, so a running panel keeps the old file it has open
fPut() {
	cp -- "${src}/${1}" "${2}/.${1}.new" || fDie "couldn't copy ${1} to ${2}"
	mv -f -- "${2}/.${1}.new" "${2}/${1}" || fDie "couldn't put ${1} in place in ${2}"
}
for f in "${files[@]}"; do fPut "${f}" "$(fDest "${f}")"; done
fEcho "Installed ${build} to ${libDir} and ${shareDir}"

if ((restart)) && pgrep -x -u "${UID}" xfce4-panel > /dev/null; then
	fEcho "Restarting the panel"
	xfce4-panel --restart
fi
if ! grep -q 'tukzmon$' <<< "$(xfconf-query -c xfce4-panel -lv 2> /dev/null || true)"; then
	fEcho "tukzmon isn't in a panel yet. Add it with the panel's right-click menu, Panel, Add New Items, tukzmon."
fi


##	History:
##		- 2026-11-20: Created, from dev-install.bash's install half.
