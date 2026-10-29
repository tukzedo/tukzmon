#!/usr/bin/env bash
#	Builds tukzmon, installs it for this user, and restarts the panel so it
#	loads the new build. XFCE 4.20 reads the .desktop file from
#	~/.local/share/xfce4/panel/plugins and the library from the same path under
#	~/.local/lib/<multiarch>, so no root is needed.
#	usage: dev-install.bash [--no-restart]

##	Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)
##	SPDX-License-Identifier: LGPL-2.1-or-later

set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# rustup's cargo follows rust-toolchain.toml and Debian's /bin/cargo doesn't.
# The same toolchain as cicd.bash means the same target/release, not a rebuild.
# rustup looks for that file from the working folder, not the manifest.
export PATH="${HOME}/.cargo/bin:${PATH}"
cd "${repo}"
pin="$(sed -n 's/^channel *= *"\(.*\)"/\1/p' rust-toolchain.toml)"
have="$(rustc --version 2> /dev/null || true)"
if [[ "${have}" != "rustc ${pin} "* ]]; then
	echo "WARNING: building with '${have:-no rustc}', not the pinned ${pin}. Install rustup to get it."
fi

# The panel pairs its user data dir with a lib dir under the same prefix,
# with its own lib dir's tail on it, such as lib/x86_64-linux-gnu on Debian.
dataHome="${XDG_DATA_HOME:-${HOME}/.local/share}"
if [[ "${dataHome}" != */share ]]; then
	echo "ERROR: the panel reads no user plugins when XDG_DATA_HOME ('${dataHome}') doesn't end in /share." >&2
	exit 1
fi
panelLib="$(grep -m1 -F 'libxfce4panel-2.0.so.4 ' <<< "$(ldconfig -p 2> /dev/null || true)" | sed 's/.*=> //' || true)"
if [[ -n "${panelLib}" ]]; then
	panelLibDir="$(dirname "${panelLib}")"
	libTail="lib${panelLibDir##*/lib}"
else
	libTail="lib/x86_64-linux-gnu"
	echo "WARNING: libxfce4panel-2.0.so.4 not found, so assuming the panel's lib dir is ${libTail}."
fi
shareDir="${dataHome}/xfce4/panel/plugins"
libDir="${dataHome%/share}/${libTail}/xfce4/panel/plugins"

cargo build --release -p tukzmon-xfce -p tukzmon-settings

mkdir -p "${shareDir}" "${libDir}"
# copy then rename, so a running panel keeps the old file it has open
fPut() {
	cp "${1}" "${2}/.$(basename "${1}").new"
	mv -f "${2}/.$(basename "${1}").new" "${2}/$(basename "${1}")"
}
fPut "${repo}/target/release/libtukzmon.so"   "${libDir}"
fPut "${repo}/target/release/tukzmon-settings" "${libDir}"
fPut "${repo}/xfce/tukzmon.desktop"            "${shareDir}"
echo "Installed to ${libDir} and ${shareDir}"

# Earlier versions linked ~/.local/lib/tukzmon into the system folders with
# sudo. The panel reads the user folders first, so those links are shadowed.
old="${HOME}/.local/lib/tukzmon"
for f in "${panelLibDir:-/usr/lib/x86_64-linux-gnu}/xfce4/panel/plugins/libtukzmon.so" "/usr/share/xfce4/panel/plugins/tukzmon.desktop"; do
	if [[ "$(readlink "${f}" || true)" == "${old}/"* ]]; then
		echo "Old link, no longer used: ${f}. Remove it with: sudo rm '${f}'"
	fi
done
if [[ -d "${old}" ]]; then echo "Old install, no longer used: ${old}"; fi

if [[ "${1:-}" != "--no-restart" ]] && pgrep -x -u "${UID}" xfce4-panel > /dev/null; then
	xfce4-panel --restart
fi
if ! grep -q 'tukzmon$' <<< "$(xfconf-query -c xfce4-panel -lv 2> /dev/null || true)"; then
	echo "Not in a panel yet. Add it with the panel's right-click menu, Panel, Add New Items, tukzmon."
fi
