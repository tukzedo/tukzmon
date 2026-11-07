<!-- markdownlint-disable MD007 -- Unordered list indentation -->
<!-- markdownlint-disable MD010 -- No hard tabs -->
<!-- markdownlint-disable MD033 -- No inline html -->
<!-- markdownlint-disable MD055 -- Table pipe style [Expected: leading_and_trailing; Actual: leading_only; Missing trailing pipe] -->
<!-- markdownlint-disable MD041 -- First line in a file should be a top-level heading -->

<!-- TOC ignore:true -->
# tukzmon

![Rust 1.96](https://img.shields.io/badge/rust-1.96-orange)

A system monitor that lives in your panel. Its small graphs show CPU, memory, GPU, network, disk, and more, with the last few seconds and the last few weeks in one graph.

![tukzmon in an XFCE panel: a fresh start, days of history on the log time axis, and the settings window](assets/demo.gif)

<!-- Video: [Watch the demo on YouTube](https://www.youtube.com/watch?v=VIDEO_ID) -->

The time axis is logarithmic by default. The last minute gets the most room, and older history is squeezed to the left, so one small graph shows the last few seconds and the last few weeks at once. History is kept across restarts and reboots (by default), with gaps where the computer was off.

Height can go up on a straight, log, or squared scale, for every graph or one at a time. Log lifts a trickle so it shows next to a burst, and squared makes the peaks stand out.

<!-- TOC ignore:true -->
## Table of contents

<!-- TOC -->

- [Status](#status)
- [Graphs](#graphs)
- [Features](#features)
- [Installation](#installation)
	- [NixOS](#nixos)
	- [DIY](#diy)
- [Set up development environment](#set-up-development-environment)
- [Related projects](#related-projects)
- [Background](#background)
- [Copyright and license](#copyright-and-license)

<!-- /TOC -->

## Status

The first version is an XFCE panel plugin. Other desktops will follow. And then after that - Windows, macOS, and the BSDs. Both are deferred items on the [backlog](project/backlog.md).

Tukzmon doesn't need any of the [related projects](#related-projects). It should work on any distro with a supported panel.

There are no release packages yet. For now, install it on NixOS or build it from a checkout.

## Graphs

- CPU, split into I/O wait, system, user, and nice.

- Memory, split into programs, buffers, and cache.

- GPU use and GPU memory, for an NVIDIA or AMD card.

- Network, with local, sent, and received traffic.

- Swap use, and zswap or zram use with its compression ratio.

- Load average.

- The number of processes, split into the system's, other users', and yours.

- Disk reads and writes, for all disks or the ones you pick.

- CPU temperature, with the critical range on top.

- Battery charge and discharge, with the critical level.

- tukzmon's own CPU use, so you can see what it costs. (A boring indicator as it is typically a tiny fraction of a % of one CPU, and usually shows nothing.)

## Features

- Hover over a graph to see its values at that point, and how long ago that was.

- 9 built-in color themes, plus your own saved ones. Randomize rolls a new set of colors that stay easy to tell apart.

- The settings window draws live graphs, and they follow your changes before you apply them.

- Each graph has its own width, sample rate, time axis, and height scale, or follows the shared ones. Drag graphs into any order.

- The network graph can split LAN traffic from internet traffic, without root. It's off by default.

- It works on vertical panels and deskbars too.

- Each panel item has its own settings, so 2 items on one panel can show different graphs.

- Colors come stacked, a color per series, or as one gradient by how high the graph reaches.

- It works under X11 and Wayland on XFCE 4.20.

## Installation

### NixOS

On NixOS, add `tukzmon.url = "github:tukzedo/tukzmon";` to your flake's inputs, put `tukzmon.nixosModules.default` in the system's modules, and set `programs.tukzmon.enable = true;`. After a rebuild, tukzmon is in the panel's Add New Items list. There's also `tukzmon.packages.x86_64-linux.default` and `tukzmon.overlays.default` if you'd rather add the package yourself, but then `environment.pathsToLink` needs `/share/xfce4` and `/lib/xfce4` unless the XFCE desktop is on.

### DIY

To try it from a checkout you need Rust, XFCE 4.20, Qt 6, a C++ compiler, and pkg-config (`qt6-base-dev`, `g++`, and `pkg-config` on Debian). Run `utility/dev-install.bash`. It installs to `~/.local`, with no sudo, and restarts the panel to load the new build.

## Set up development environment

- Rust through rustup. `rust-toolchain.toml` pins the version, and rustup fetches it on the first build. A distro's own `cargo` ignores the pin, so put `~/.cargo/bin` first in `PATH`.

- Qt 6 and a C++ compiler for the settings window (`qt6-base-dev`, `g++` and `pkg-config` on Debian).

- The XFCE 4.20 panel, GTK 3 and cairo. The plugin links their runtime libraries directly, so no `-dev` packages are needed for them.

- Docker, for the pipeline. `cicd/cicd.bash` builds its own image from `cicd/container/` and runs format, build, lint and tests in it, with the other tools pinned there. Without docker it runs on the host, and then it needs those tools installed. `cicd/config.bash` lists them and their versions.

- `cicd/cicd.bash --help` lists the steps and options. The packages step builds a `.deb` and an `.rpm`.

- `git config core.hooksPath cicd/hooks` turns on the pre-push hook, which runs the full merge gate before a push to `main`.

Work goes on `dev`, and `main` takes releases.

## Related projects

- [Tukzedo Linux](https://github.com/tukzedo/tukzedo-linux), an ultra-secure distro with similar design principles.

- [LXQtukz](https://github.com/tukzedo/lxqtukz), a highly configurable desktop with multiple "personas" out-of-the-box. (An LXQt panel version of tukzmon is in the works for it.)

## Background

Inspired by A) The "Non-linear time" scale option in XFCE4's "CPU Graph", and B) The now-abandoned and stale "Multiload-ng" project. (The latter was a revival fork of the defunct "Multiload" project, which was a reimplementation of the original monitors available in the original Gnome desktop panel - and also available in MATE.)

## Copyright and license

> Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)<br>
> Licensed under [GNU LGPL v2.1 or later](https://spdx.org/licenses/LGPL-2.1-or-later.html) license. No warranty. See `license.txt`.
