#!/usr/bin/env bash
#  shellcheck enable=require-variable-braces  ## Every expansion braced: "${var}", not "$var".

##	- Purpose: the Nix step's check, run by cicd.bash in a nixos/nix container
##	  as its root, with the repo's tracked files at /src, read-only, and the
##	  store in a docker volume kept between runs. It builds the flake's package,
##	  which runs the workspace tests, checks every library the plugin links
##	  resolves, runs tukzmon-settings --version, builds a NixOS system with the
##	  module, then has nixpkgs' xfce4-panel load and embed the plugin from that
##	  system's path, on Xvfb.
##	- Env: TUKZMON_JOBS, the build's cores. TUKZMON_NIX_CAP_MB, the store size
##	  past which a garbage collection runs at the end, keeping what this run
##	  built and what built it.
##	History: At bottom of file.

##	Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)
##	Licensed under The MIT License (MIT). Full text at:
##		https://mit-license.org/
##	SPDX-License-Identifier: MIT


set -Eeuo pipefail

src="/src"
work="/tmp/check"
## Links here are the store's roots, so a collection keeps the last run's
## build and, with keep-outputs, what it was built from.
roots="/nix/var/nix/gcroots/tukzmon"
jobs="${TUKZMON_JOBS:-1}"
capMb="${TUKZMON_NIX_CAP_MB:-12000}"
## Digits only, since bash arithmetic runs a $(...) inside an array subscript.
[[ "${jobs}" =~ ^[1-9][0-9]{0,3}$ ]] || { printf "nix check: TUKZMON_JOBS isn't a job count: '%s'\n" "${jobs}" >&2; exit 1; }
[[ "${capMb}" =~ ^[1-9][0-9]{0,8}$ ]] || { printf "nix check: TUKZMON_NIX_CAP_MB isn't a size in MB: '%s'\n" "${capMb}" >&2; exit 1; }
## No sandbox, since the container has no user namespaces to give it.
NIX_CONFIG="$(printf '%s\n' "experimental-features = nix-command flakes" "sandbox = false" \
	"keep-outputs = true" "keep-derivations = true" "max-jobs = 1" "cores = ${jobs}")"
export NIX_CONFIG

fSay() { printf '%s\n' "$*"; }
fFail() { printf 'nix check: %s\n' "$*" >&2; exit 1; }
## A package from the nixpkgs the flake locks, so the check runs on what the
## package was built with.
fPkg() {
	nix build --print-out-paths --inputs-from "${src}" "nixpkgs#$1^out" -o "${roots}/$1" 2> "${work}/pkg-$1.log" | tail -n 1 \
		|| { tail -n 20 -- "${work}/pkg-$1.log" >&2; fFail "nixpkgs#$1 didn't build"; }
}

mkdir -p -- "${work}" "${roots}"

fSay "nix build .#tukzmon, with the workspace tests"
nix build -L --inputs-from "${src}" "path:${src}#tukzmon" -o "${roots}/tukzmon" > "${work}/build.log" 2>&1 \
	|| { tail -n 40 -- "${work}/build.log"; fFail "the package didn't build"; }
## A build from the store's cache runs no tests, so none is fine then.
n="$(grep -c -E '^tukzmon> test result: ok\.' "${work}/build.log" || true)"
if grep -q -E '^tukzmon> ' "${work}/build.log"; then
	((n > 0)) || fFail "the package built but ran no tests"
	fSay "${n} test binaries passed"
else
	fSay "the package was already built, with its tests, in the kept store"
fi
pkg="$(readlink -f -- "${roots}/tukzmon")"
lib="${pkg}/lib/xfce4/panel/plugins/libtukzmon.so"
for f in "${lib}" "${pkg}/share/xfce4/panel/plugins/tukzmon.desktop" "${pkg}/bin/tukzmon-settings"; do
	[[ -e "${f}" ]] || fFail "the package has no ${f#"${pkg}/"}"
done

## glibc's loader lists what the library needs and where it found each, as ldd
## would, so a library missing from the rpath shows as "not found".
glibc="$(fPkg glibc)"
deps="$("${glibc}/lib/ld-linux-x86-64.so.2" --list "${lib}")" || fFail "the loader couldn't read ${lib#"${pkg}/"}"
if grep -q 'not found' <<< "${deps}"; then grep 'not found' <<< "${deps}"; fFail "the plugin links a library its rpath doesn't reach"; fi
fSay "the plugin's $(grep -c '=> /nix/store/' <<< "${deps}") libraries all resolve"
env -u DISPLAY "${pkg}/bin/tukzmon-settings" --version | head -n 1 || fFail "tukzmon-settings --version failed"

## A NixOS system with the module, with and without the XFCE desktop.
cp -r -- "${src}/cicd/nix/system" "${work}/system"
for shape in plain xfce; do
	got="$(nix eval --raw "path:${work}/system#${shape}" 2> "${work}/eval.log")" || { tail -n 20 -- "${work}/eval.log"; fFail "the ${shape} system didn't evaluate"; }
	[[ "${got}" == "package links" ]] || fFail "the ${shape} system has '${got}', not the package and both links"
done
fSay "a system with the module has the package and both links, with the XFCE desktop and without"
sys="$(nix build --print-out-paths "path:${work}/system#path" -o "${roots}/system" 2> "${work}/system.log" | tail -n 1)" \
	|| { tail -n 20 -- "${work}/system.log"; fFail "the system path didn't build"; }
[[ -e "${sys}/lib/xfce4/panel/plugins/libtukzmon.so" ]] || fFail "the system path has no libtukzmon.so"

## The panel, given the system's share in XDG_DATA_DIRS, has to find the
## plugin there and embed it.
panel="$(fPkg xfce4-panel)"; xfconf="$(fPkg xfconf)"; dbus="$(fPkg dbus)"; xvfb="$(fPkg xvfb)"
home="${work}/home"
mkdir -p -- "${home}/.config/xfce4/xfconf/xfce-perchannel-xml" "${home}/run"
chmod 700 -- "${home}/run"
cp -- "${src}/cicd/nix/xfce4-panel.xml" "${home}/.config/xfce4/xfconf/xfce-perchannel-xml/"
"${xvfb}/bin/Xvfb" :98 -screen 0 1280x800x24 > "${work}/xvfb.log" 2>&1 &
xvfbPid=$!
sleep 2
# shellcheck disable=2016  ## The inner bash expands it.
env XDG_DATA_DIRS="${sys}/share:${xfconf}/share:${panel}/share" HOME="${home}" XDG_CONFIG_HOME="${home}/.config" \
	XDG_DATA_HOME="${home}/.local/share" XDG_RUNTIME_DIR="${home}/run" DISPLAY=:98 PATH="${sys}/bin:${panel}/bin:${PATH}" \
	"${dbus}/bin/dbus-run-session" --config-file="${dbus}/share/dbus-1/session.conf" -- bash -c '
		PANEL_DEBUG=all xfce4-panel > "$1" 2>&1 &
		sleep 10
		xfce4-panel --quit || true
		sleep 1' _ "${work}/panel.log" 2> "${work}/session.log" || true
kill "${xvfbPid}" 2> /dev/null || true
want="new module tukzmon, filename=${sys}/lib/xfce4/panel/plugins/libtukzmon.so"
if ! grep -qF -- "${want}" "${work}/panel.log" || ! grep -qE 'tukzmon-1: child is embedded' "${work}/panel.log"; then
	grep -iE 'tukzmon|wrapper|plugin-1|error|crit' "${work}/panel.log" | head -n 20 || true
	fFail "the panel didn't load and embed the plugin from the system path"
fi
name="${panel##*/}"
fSay "xfce4-panel ${name#*-xfce4-panel-} loaded the plugin from the system path and embedded it"

mb="$(du -sm /nix/store | cut -f1)"
if ((mb > capMb)); then
	fSay "the store is ${mb} MB, over ${capMb}, so it's collected down to what this run used"
	nix-collect-garbage > "${work}/gc.log" 2>&1 || { tail -n 5 -- "${work}/gc.log"; fFail "nix-collect-garbage failed"; }
	mb="$(du -sm /nix/store | cut -f1)"
fi
fSay "store: ${mb} MB"


##	History:
##		- 2026-10-28: Created, from the rig that first checked the package.
##		- 2026-10-30: TUKZMON_JOBS and TUKZMON_NIX_CAP_MB are checked for digits.
