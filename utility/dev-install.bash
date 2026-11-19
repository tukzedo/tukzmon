#!/usr/bin/env bash

#  shellcheck enable=require-variable-braces  ## Every expansion braced: "${var}", not "$var".

##	Purpose:
##		Builds tukzmon, installs it for this user, and restarts the panel so it
##		loads the new build. XFCE 4.20 reads the .desktop file from
##		~/.local/share/xfce4/panel/plugins and the library from the same path
##		under ~/.local/lib/<multiarch>, so no root is needed.
##	Syntax:
##		dev-install.bash [--no-restart]
##		  --no-restart  install, but leave the running panel alone
##	Exit: 0 installed, 1 failed, 2 bad arguments.
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
fDie() { fEcho "dev-install: $*" >&2; exit 1; }

restart=1
while (($#)); do case "$1" in
	--no-restart) restart=0; shift ;;
	-h|--help)    sed -n '/^##	Purpose:/,/^##	History:/p' "${BASH_SOURCE[0]}" | sed '$d; s/^##\t\{0,1\}//'; exit 0 ;;
	*)            fEcho "dev-install: unknown option: $1 (try --help)" >&2; exit 2 ;;
esac; done

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

## rustup's cargo follows rust-toolchain.toml, but Debian's /bin/cargo doesn't.
## With the same toolchain as cicd.bash, cargo reuses its release build
## rather than rebuilding. rustup looks for that file from the working folder
## rather than the manifest's.
export PATH="${HOME}/.cargo/bin:${PATH}"
cd "${repo}"
pin="$(sed -n 's/^channel *= *"\(.*\)"/\1/p' rust-toolchain.toml)"
have="$(rustc --version 2> /dev/null || true)"
if [[ "${have}" != "rustc ${pin} "* ]]; then
	fEcho "WARNING: building with '${have:-no rustc}' in place of the pinned ${pin}. Install rustup to get it."
fi

## The panel looks for user plugins in a lib dir under the data dir's prefix,
## with the same tail as its own lib dir, such as lib/x86_64-linux-gnu on Debian.
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

## Finds wherever CARGO_TARGET_DIR or a cargo config puts the build.
target="$(cargo metadata --format-version 1 --no-deps --locked 2> /dev/null | sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p' || true)"
[[ -n "${target}" ]] || fDie "cargo metadata didn't give the target dir"
## cicd's release build has the same path maps, so this one is the same bytes.
# shellcheck source=utility/path-maps.bash
source "${repo}/utility/path-maps.bash"
declare -a maps=()
fPathMaps maps "${repo}" "${target}" || fDie "no path maps for the release build"
env "${maps[@]}" cargo build --release --locked -p tukzmon-xfce -p tukzmon-settings || fDie "the release build failed"

mkdir -p "${shareDir}" "${libDir}"
## copy then rename, so a running panel keeps the old file it has open
fPut() {
	local name="${1##*/}"
	cp "${1}" "${2}/.${name}.new" || fDie "couldn't copy ${name} to ${2}"
	mv -f "${2}/.${name}.new" "${2}/${name}" || fDie "couldn't put ${name} in place in ${2}"
}
fPut "${target}/release/libtukzmon.so"   "${libDir}"
fPut "${target}/release/tukzmon-settings" "${libDir}"
fPut "${repo}/xfce/tukzmon.desktop"       "${shareDir}"
fEcho "Installed to ${libDir} and ${shareDir}"

## Earlier versions linked ~/.local/lib/tukzmon into the system folders with
## sudo. The panel reads the user folders first, so those links are shadowed.
old="${HOME}/.local/lib/tukzmon"
for f in "${panelLibDir:-/usr/lib/x86_64-linux-gnu}/xfce4/panel/plugins/libtukzmon.so" "/usr/share/xfce4/panel/plugins/tukzmon.desktop"; do
	if [[ "$(readlink "${f}" || true)" == "${old}/"* ]]; then
		fEcho "Old link, no longer used: ${f}. Remove it with: sudo rm '${f}'"
	fi
done
if [[ -d "${old}" ]]; then fEcho "Old install, no longer used: ${old}"; fi

if ((restart)) && pgrep -x -u "${UID}" xfce4-panel > /dev/null; then
	xfce4-panel --restart
fi
if ! grep -q 'tukzmon$' <<< "$(xfconf-query -c xfce4-panel -lv 2> /dev/null || true)"; then
	fEcho "tukzmon isn't in a panel yet. Add it with the panel's right-click menu, Panel, Add New Items, tukzmon."
fi


##	History:
##		- 2026-10-23: Created, linking ~/.local/lib/tukzmon into the system
##		  plugin folders with sudo.
##		- 2026-10-25: Builds with the pinned toolchain, as cicd.bash does.
##		- 2026-10-29: No sudo. Installs to the user plugin folders, and names
##		  the old links rather than removing them.
##		- 2026-10-30: The helper script layout, MIT, and --help. An unknown
##		  option is an error rather than ignored.
##		- 2026-11-01: Copies from the target dir cargo built into, finds
##		  ldconfig in /usr/sbin, and builds with --locked.
##		- 2026-11-07: ## comments, as in the other scripts.
##		- 2026-11-10: The release build gets cicd's path maps.
