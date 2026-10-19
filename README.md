<!-- markdownlint-disable MD007 -- Unordered list indentation -->
<!-- markdownlint-disable MD010 -- No hard tabs -->
<!-- markdownlint-disable MD033 -- No inline html -->
<!-- markdownlint-disable MD055 -- Table pipe style [Expected: leading_and_trailing; Actual: leading_only; Missing trailing pipe] -->
<!-- markdownlint-disable MD041 -- First line in a file should be a top-level heading -->
# tukzmon

A system monitor that lives in your panel. It draws small graphs for CPU, memory, network, disk, swap, temperature, battery, and more, right in the taskbar.

The time axis is logarithmic by default. The last minute gets the most room, and older history is squeezed to the left, so one small graph shows the last few seconds and the last few weeks at once. History is kept across restarts and reboots, with gaps where the computer was off.

## Status

Early. There's no code yet and nothing to install. The plan is in [project/design.md](project/design.md) and the work list in [project/backlog.md](project/backlog.md).

The first version is an XFCE panel plugin. Other desktops come after that, and later Windows, macOS, and the BSDs.

## Graphs

- CPU, split into I/O wait, system, user, and nice.

- Memory, split into user, buffers, and cache.

- Network, with local, sent, and received traffic.

- Swap use, and zswap or zram use with its compression ratio.

- Load average.

- Disk reads and writes, for all disks or the ones you pick, with an optional log scale.

- CPU temperature, with the critical range on top.

- Battery charge and discharge, with the critical level.

## Related projects

- [tukzedo-linux](https://github.com/tukzedo/tukzedo-linux) is the distro.

- [lxqtukz](https://github.com/tukzedo/lxqtukz) is the desktop. An LXQt panel version of tukzmon is planned for it.

tukzmon doesn't need either one. It should work on any distro with a supported panel.

## Copyright and license

> Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)<br>
> Licensed under [GNU LGPL v2.1 or later](https://spdx.org/licenses/LGPL-2.1-or-later.html) license. No warranty. See `license.txt`.
