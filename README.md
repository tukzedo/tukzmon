<!-- markdownlint-disable MD007 -- Unordered list indentation -->
<!-- markdownlint-disable MD010 -- No hard tabs -->
<!-- markdownlint-disable MD033 -- No inline html -->
<!-- markdownlint-disable MD055 -- Table pipe style [Expected: leading_and_trailing; Actual: leading_only; Missing trailing pipe] -->
<!-- markdownlint-disable MD041 -- First line in a file should be a top-level heading -->
# tukzmon

A system monitor that lives in your panel. It draws small graphs for CPU, memory, GPU, network, disk, swap, temperature, battery, and more, right in the taskbar.

Inspired by A) The "Non-linear time" scale option in XFCE4's "CPU Graph", and B) The now-abandoned and stale "Multimon-NG" project. (The latter was a revival fork of the defunct "Multimon" project, which was a reimplimentation of the original monitors available in the original Gnome desktop panel' - and also available in MATE.)

The time axis is logarithmic by default. The last minute gets the most room, and older history is squeezed to the left, so one small graph shows the last few seconds and the last few weeks at once. History is kept across restarts and reboots (by default), with gaps where the computer was off.

## Status

To try it from a checkout you need Rust, XFCE 4.20, and Qt 6 (`qt6-base-dev` on Debian). Run `utility/dev-install.bash`. It installs to `~/.local`, with no sudo, and restarts the panel to load the new build.

On NixOS, add `tukzmon.url = "github:tukzedo/tukzmon";` to your flake's inputs, put `tukzmon.nixosModules.default` in the system's modules, and set `programs.tukzmon.enable = true;`. After a rebuild, tukzmon is in the panel's Add New Items list. There's also `tukzmon.packages.x86_64-linux.default` and `tukzmon.overlays.default` if you'd rather add the package yourself, but then `environment.pathsToLink` needs `/share/xfce4` and `/lib/xfce4` unless the XFCE desktop is on.

The first version is an XFCE panel plugin. Other desktops will follow. And then after that - Windows, macOS, and the BSDs.

## Graphs

- CPU, split into I/O wait, system, user, and nice.

- Memory, split into user, buffers, and cache.

- GPU use and GPU memory, for an NVIDIA or AMD card.

- Network, with local, sent, and received traffic.

- Swap use, and zswap or zram use with its compression ratio.

- Load average.

- The number of processes, split into the system's, other users', and yours.

- Disk reads and writes, for all disks or the ones you pick, with an optional log scale.

- CPU temperature, with the critical range on top.

- Battery charge and discharge, with the critical level.

- tukzmon's own CPU use, so you can see what it costs. (A boring indicator as it is typically a tiny fraction of a % of one CPU, and usually shows nothing.)

## Related projects

- [Tukzedo Linux](https://github.com/tukzedo/tukzedo-linux) an altra-secure distro with similar design principles.

- [LXQtukz](https://github.com/tukzedo/lxqtukz) a highly-configurable desktop with multiple "personnas" out-of-the-box. (An LXQt panel version of tukzmon is in the works for it.)

Tukzmon doesn't need either one. It should work on any distro with a supported panel.

## Copyright and license

> Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)<br>
> Licensed under [GNU LGPL v2.1 or later](https://spdx.org/licenses/LGPL-2.1-or-later.html) license. No warranty. See `license.txt`.
