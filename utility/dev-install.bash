#!/usr/bin/env bash
#	Builds tukzmon and puts it in ~/.local/lib/tukzmon, for running from a
#	checkout. XFCE 4.20's panel only looks for plugins in its own system
#	folders, so the first install also needs 2 symlinks made as root. This
#	prints them; it never runs sudo itself.
#	usage: dev-install.bash [--restart]   (--restart restarts xfce4-panel)

##	Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)
##	SPDX-License-Identifier: LGPL-2.1-or-later

set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
dest="${HOME}/.local/lib/tukzmon"

cargo build --release --manifest-path "${repo}/Cargo.toml" -p tukzmon-xfce -p tukzmon-settings

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
missing=0
[[ "$(readlink "${libdir}/libtukzmon.so" || true)" == "${dest}/libtukzmon.so" ]] || missing=1
[[ "$(readlink "${sharedir}/tukzmon.desktop" || true)" == "${dest}/tukzmon.desktop" ]] || missing=1
if [[ "${missing}" == "1" ]]; then
	echo
	echo "One time, as root, so the panel can find it:"
	echo "  sudo ln -sfn '${dest}/libtukzmon.so' '${libdir}/libtukzmon.so'"
	echo "  sudo ln -sfn '${dest}/tukzmon.desktop' '${sharedir}/tukzmon.desktop'"
	echo "Then add it with the panel's right-click menu, Panel, Add New Items, tukzmon."
fi

if [[ "${1:-}" == "--restart" ]]; then
	xfce4-panel --restart
elif [[ "${missing}" == "0" ]]; then
	echo "A plugin already in the panel keeps the old build until: xfce4-panel --restart"
fi
