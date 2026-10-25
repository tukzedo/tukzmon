#!/usr/bin/env bash
#	Builds tukzmon, puts it in ~/.local/lib/tukzmon, and restarts the panel so
#	it loads the new build. XFCE 4.20's panel only looks for plugins in its own
#	system folders, so a missing link there is made with sudo.
#	usage: dev-install.bash [--no-restart]

##	Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)
##	SPDX-License-Identifier: LGPL-2.1-or-later

set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
dest="${HOME}/.local/lib/tukzmon"

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

cargo build --release -p tukzmon-xfce -p tukzmon-settings

mkdir -p "${dest}"
# copy then rename, so a running panel keeps the old file it has open
for f in "${repo}/target/release/libtukzmon.so" "${repo}/target/release/tukzmon-settings" "${repo}/xfce/tukzmon.desktop"; do
	cp "${f}" "${dest}/.$(basename "${f}").new"
	mv -f "${dest}/.$(basename "${f}").new" "${dest}/$(basename "${f}")"
done
echo "Installed to ${dest}"

panel_lib="$(grep -m1 -F 'libxfce4panel-2.0.so.4 ' <<< "$(ldconfig -p)" | sed 's/.*=> //' || true)"
libdir="$(dirname "${panel_lib:-/usr/lib/x86_64-linux-gnu/x}")/xfce4/panel/plugins"
sharedir="/usr/share/xfce4/panel/plugins"
fLink() {
	[[ "$(readlink "${2}" || true)" == "${1}" ]] && return 0
	echo "Linking ${2} (sudo)"
	sudo ln -sfn "${1}" "${2}"
}
fLink "${dest}/libtukzmon.so" "${libdir}/libtukzmon.so"
fLink "${dest}/tukzmon.desktop" "${sharedir}/tukzmon.desktop"

if [[ "${1:-}" != "--no-restart" ]] && pgrep -x -u "${UID}" xfce4-panel > /dev/null; then
	xfce4-panel --restart
fi
if ! grep -q 'tukzmon$' <<< "$(xfconf-query -c xfce4-panel -lv 2> /dev/null || true)"; then
	echo "Not in a panel yet. Add it with the panel's right-click menu, Panel, Add New Items, tukzmon."
fi
