<!-- markdownlint-disable MD007 -- Unordered list indentation -->
<!-- markdownlint-disable MD010 -- No hard tabs -->
<!-- markdownlint-disable MD033 -- No inline html -->
<!-- markdownlint-disable MD055 -- Table pipe style [Expected: leading_and_trailing; Actual: leading_only; Missing trailing pipe] -->
<!-- markdownlint-disable MD041 -- First line in a file should be a top-level heading -->
<!-- TOC ignore:true -->
# Design

This is a draft. The XFCE plugin and its settings window are built, but the other panels are still to come. The [backlog](backlog.md) goes with it.

<!-- TOC ignore:true -->
## Table of contents

<!-- TOC -->

- [Summary](#summary)
- [Code layout](#code-layout)
	- [XFCE plugin](#xfce-plugin)
	- [Settings window](#settings-window)
	- [Theme](#theme)
- [Panels and trays](#panels-and-trays)
- [Data sources on Linux](#data-sources-on-linux)
- [History](#history)
- [Time axis](#time-axis)
- [Drawing](#drawing)

<!-- /TOC -->

## Summary

tukzmon draws small system graphs in a desktop panel. Most of the work is the same on every panel, so it's all in one Rust library. That library reads the numbers, keeps years of history in a small file, maps time onto a log axis, and draws. Each panel gets a thin front end that hands the library a place to draw.

XFCE comes first. The other panels and platforms are in the backlog.

## Code layout

The code is a Rust core, with C only where a panel's API needs it. That was settled on 2026-10-19.

- `tukzmon-core` is a Rust library without any GUI toolkit in it.
	- It has collectors for each platform, behind one trait.
	- It has the history store.
	- It does the time axis mapping.
	- It draws into a plain pixel buffer. Stacked columns are filled by hand, and the top pixel of each series is shaded by how much of it the series fills. [tiny-skia](https://crates.io/crates/tiny-skia) comes in once there are lines or curves to draw.
	- It has the settings spec in `core/src/settings.shcl`, which the plugin and the settings window both compile in.

- `knobs` is SilkTerm's settings model. Settings are declared in shcl, and knobs works out what a change to one does to the others. It's MIT, copyright Bubbles, and the copy here is free to change on its own.

- `tukzmon-settings` is the settings window, a program of its own. See [Settings window](#settings-window).

- Each front end is as small as it can be. It creates the panel widget, runs a timer, and copies the library's pixel buffer onto the screen. A pixel buffer is the one thing every host can show, as a cairo image surface on GTK, a `QImage` on Qt, a DIB on Windows, and a `CGImage` on macOS.

### XFCE plugin

- XFCE 4.20 is still GTK3, and its plugin API is C macros around `libxfce4panel-2.0`.

- The plugin is a Rust `cdylib`. The few dozen GLib, GTK, cairo, and panel calls it makes are declared by hand in `xfce/src/sys.rs`, and it links the runtime libraries by soname. So one `cargo build` gives the `.so`, without any `-dev` packages.
	- The gtk3-rs bindings were archived in 2024.
	- Every widget is an opaque pointer, so no C struct layouts are copied, and a header change can't break it quietly.
	- The other way, a short C file built against the headers, needs `libxfce4panel-2.0-dev`. On a box whose panel is newer than the distro's, that package would pull the panel library back to the distro's version.
	- The panel runs external plugins in a separate wrapper process, so a crash takes down the plugin adn not the panel.
	- That's on X11. On Wayland, XFCE 4.20 loads the plugin into the panel's own process, since XEmbed is X11 only, so an abort in the plugin takes down the whole panel. Every instance shares that process too. That's safe, since the plugin's only globals are caches set once, such as the settings spec, the CPU count, the start time, and the NVML library's calls, a per-thread cache of `/proc/meminfo`, the per-thread time the NVIDIA readers rest until, and its panic hook. It doesn't start any threads.
	- So the release build unwinds, and every handler the panel calls runs through `guarded` in `xfce/src/lib.rs`, which catches a panic and stops only that item. A stopped item drops its timers and file watches, stops drawing, and doesn't save any more history, and its tip says it stopped and why. The panic is one line, `tukzmon: stopped by a bug at <file>:<line>: <text>`. Test `dekpx` checks every handler for the guard, and `dekpv` checks that the release profile unwinds.
		- Cargo can't set `panic` per package, so tukzmon-settings unwinds too. Unwinding made each of them about 7% bigger.
	- The plugin's messages go through `complain` in `xfce/src/lib.rs`, since `eprintln!` panics when stderr can't be written, such as to a full disk or a closed terminal. Test `defyn` keeps the print macros out of all the code the panel loads.
	- A panel that `xfce4-panel --restart` started has stderr on `/dev/null`, so `complain` also sends each line to syslog, which the journal reads, unless stderr is the journal already. A failed history read, save, or delete also shows under the graphs in the tip, until a save works. A settings file that's there but can't be read is logged once, and the item keeps the look it has, or the defaults at its start. The tip says so until the file reads again, which each tick tries. A missing file means the defaults.
	- Nothing else in the plugin assumes X11. The drawing, the tip, the hover line, the right-click menu, and Properties work the same on Wayland, and tukzmon-settings opens as a Wayland window.

- The plugin draws its graphs in one drawing area, side by side, in the saved order. Each graph has its margins clear around it, in logical pixels, so the panel shows through. The margins are Top and bottom px and Left and right px on the Outline sub-tab, 2 px each by default. So there are 4 px between 2 graphs side by side, which is both their margins. The item's 2 ends along the panel don't get any, since the panel spaces its items. On a vertical panel or a deskbar they're stacked. Graphs are always drawn upright, so there top and bottom are between graphs, and left and right either side of them. There a graph's width is its height, as multiload-ng's Size is, and it's as wide as the panel. Time still runs from left to right. The graph under the pointer gets the hover line and the tip. The 2 px edges count as the graph beside them, so a pointer pushed against the screen's edge still finds it, and a gap between 2 graphs finds neither. `core/src/graphs.rs` has the layout.
	- The Graphs tab is the graph list, a row per graph in the saved order. Each row has a checkbox at `<graph>.show`, the live graph, a width at `<graph>.width`, a sample rate, a time axis, a vertical scale, a revert arrow, and a gripper. `<graph>` is the graph's name in `graph.order`, below, such as `cpu.show` or `mem.width`. A width is a percentage of the Width px default setting, so one setting still resizes them all. The defaults are 100% for CPU, memory, and battery, 50% for GPU, GPU memory, network, disk, and temperature, 25% for swap, zswap, load, and processes, and 12% for tukzmon. Widths come out in the panel's own pixels, so they scale with it.
		- A hidden graph's width, sample rate, time axis, and vertical scale aren't greyed, so a graph can be set up before it's turned on.
	- Each row draws its graph live, at its own width, with the window's settings, so a change shows there before Apply. The window reads the graphs itself from when it opens, so a row starts empty. A hidden graph is drawn too. The widest graph sets the column, so the boxes after it line up, and one wider than 512 px is squeezed to that.
	- The row under the pointer gets the hover line and the tip, the same as in the panel, through the panel's own `graphs::point`. On a squeezed row the line goes at the pointer's place in the whole graph. A tip left up follows each sample.
	- The sample rate is at `<graph>.sample`, as a percentage of Sample ms default at `graph.sample_ms`, the way a width is a percentage of Width px default. It's how long between reads, so 200% reads half as often. Sample ms default is 1000. CPU and memory default to 50%, so they read twice a second. Processes, swap, zswap, and load default to 200%, tukzmon to 400%, and the rest to 100%. Under 50 ms counts as 50 ms, the finest history tier.
		- An earlier build kept a graph's own rate in ms at `<name>.sample_ms`. No release had it, so that line is ignored and the graph reads at 100%.
	- The time axis is at `<graph>.time`. See [Time axis](#time-axis).
	- A graph gets a row once `core/src/settings.shcl` has its `<name>.show`, `.width`, `.sample`, `.time`, and `.vertical` settings.
	- A box where `/proc/meminfo` can't be read never shows the memory graph.
	- CPU, memory, GPU memory, disk, temperature, and swap are on by default, and the rest are off. A graph is never shown on a box that doesn't have what it reads, such as swap, zswap or zram, a disk, a CPU sensor tukzmon knows, a GPU it can read, or a battery.
	- Under Misc, last on the General tab, LAN and internet at `net.split` tells the LAN from the internet. It's `off` by default, or `sockets`. See [Data sources on Linux](#data-sources-on-linux). It greys out while the network graph is off, as the disk rows do.
	- Under Misc too, Disk drives at `disk.drives` picks what the disk graph counts. It's `all` by default, or `/proc/diskstats` names such as `sda nvme0n1`, split the same way as `graph.order`. A new pick takes effect at Apply and keeps the graph's history.
	- Under Vertical axis on the General tab, Scale default at `graph.vertical` sets how height goes up every graph. It's `linear` by default, or `log10` or `squared`. The window labels the last 2 with log and chi symbols. Each graph's row in the graph list on the Graphs tab has its own Vertical scale, at `cpu.vertical`, `mem.vertical` and so on, right of its time axis. Its Global default, `same`, follows Scale default. It isn't part of the theme, so picking a theme leaves it alone. See [Drawing](#drawing).
		- Disk log scale, at `disk.log`, was the disk graph's alone. A file with it on loads with the disk graph's own Vertical scale at `log10`, and the next save drops the old line.
	- With every other graph off, CPU shows anyway, so the item always has some width, and its right-click menu can still be opened.
	- A hidden graph isn't read. Its history is kept, and shows a gap for the time it was hidden.
	- Dragging a row's gripper, or Up and Down on it, sets the order. It's saved at `graph.order`, which names the graphs first to last, left to right or top to bottom. The names are `cpu`, `mem`, `gpu`, `vram`, `net`, `swap`, `zswap`, `load`, `procs`, `disk`, `temp`, `battery`, and `self`, the same as their history section names. The default is `cpu mem procs gpu vram net disk temp swap zswap load self battery`, so the busiest graphs come first, processes follows memory, the GPU's 2 come next, zswap is beside swap, and the battery is last. A saved order from before a graph was added puts it last.
		- Spaces, commas, or a list such as `[mem, cpu]` all work. shcl 3 refuses a comma with a space after it outside quotes or brackets, so a hand edit of `order: mem, cpu` is dropped.
		- A name tukzmon doesn't know is skipped. A graph the order doesn't name goes at the end, in the order tukzmon lists its graphs, so that way a graph added later, such as network, shows up last in a saved order.
		- A hidden graph keeps its place for when it's shown again.
		- Tab follows the order, through the graph list's rows and the Themes/Colors sections, from the moment a graph is moved.
	- A name the list doesn't know stays in `graph.order` after a drag, at the end, so a file from a newer tukzmon keeps it.
	- The row's revert arrow puts its checkbox, width, sample rate, time axis, and vertical scale back. The order doesn't have an arrow, since dragging a graph back undoes a move.

- The drawing area has a window of its own, so it has to take button events and be an action widget, or a right-click never opens the panel's menu.

- The plugin watches the settings file, and reads it again when its modified time or size changes. That's how a save in the settings window shows up, at once rather than at the next graph's read, which can be a minute away. Each timer tick checks it too, in case the watch couldn't be set up.

- Each graph reads at its own rate, on one timer. A graph reads on whole multiples of its rate, counted on one clock every graph shares, so graphs whose rates divide each other read on the same tick and the panel draws once for them. The timer is one-shot, set again each tick for the next graph due, so a mix of rates doesn't wake the panel any more than it has to. `Graph::tick` in `core/src/lib.rs` has the beats.
	- A read that would come less than half its rate after the one before is skipped, such as right after the rate changes. A very short span is mostly noise.
	- The settings window's graph list reads its graphs the same way.

- The tip belongs to the drawing area, through `query-tooltip`, so it can say what's under the pointer. GTK shows a tip once the pointer has been still for 500 ms, and asking again restarts that wait. So the timer asks again only after the pointer has been still for a second, which keeps a still pointer's time current.

- A `.desktop` file tells the panel where the plugin is. XFCE 4.20 pairs each data dir with a lib dir under the same prefix. It pairs `/usr/share` with the system lib dir, each `XDG_DATA_DIRS` entry `<p>/share` with `<p>/lib`, and `~/.local/share` with `~/.local/lib/<multiarch>`, such as `~/.local/lib/x86_64-linux-gnu` on Debian. It reads the `.desktop` file from `xfce4/panel/plugins` under the data dir, and the library from `xfce4/panel/plugins` under the lib dir. Debian's panel 4.20.7 finds, loads, and embeds the plugin with only `~/.local/share/xfce4/panel/plugins/tukzmon.desktop` and `~/.local/lib/x86_64-linux-gnu/xfce4/panel/plugins/libtukzmon.so`, so a build for one user doesn't need root. `utility/dev-install.bash` installs there. It copies the `.desktop` file to `~/.local/share/xfce4/panel/plugins`, and the library and `tukzmon-settings` to `~/.local/lib/<multiarch>/xfce4/panel/plugins`, where `<multiarch>` is the tail of the panel library's own lib dir. It then restarts the panel. A user folder is read before the system ones, so the user copy wins over a system one of the same module.

- The Nix package is for tukzedo-linux's NixOS edition. `flake.nix` has it, an overlay, and a NixOS module, `programs.tukzmon.enable`. `nix/package.nix` builds it.
	- XFCE 4.20 built with its own prefix, as Nix builds it, pairs each `XDG_DATA_DIRS` entry `<p>/share` with `<p>/lib`. It reads `.desktop` files from `<p>/share/xfce4/panel/plugins`, and the library from `<p>/lib/xfce4/panel/plugins`. So the package puts them there, and NixOS links both into `/run/current-system/sw`. Its XFCE desktop module does that already, and the tukzmon module does it too, for a panel run under some other desktop.
	- The plugin links its libraries by soname, which Nix's linker wrapper doesn't add an rpath for, so the package passes the rpath itself.
	- A link to `tukzmon-settings` is beside the library, so the plugin runs the one from its own package rather than whatever is first on PATH.
	- The build number is the flake's commit time, since Nix's default `SOURCE_DATE_EPOCH` is 1980.
	- `cicd/cicd.bash --long`, or its `nix` area, builds the package in a nixos/nix container, then has nixpkgs' xfce4-panel load and embed the plugin from a NixOS system with the module. `cicd/nix/check.bash` is the check. Its store is a docker volume kept between runs, since the first build fetches about 5 GB. It's left out of the other runs for that reason.

- If Rust can't be used for some reason, the fallback is the same split, with the C file growing into the whole plugin.

### Settings window

- Settings are a separate program, `tukzmon-settings`, so every panel and platform shares one window. A panel only has to start it with `--config FILE`.

- It's Qt 6 Widgets. Widgets is the lightest Qt that's cross-platform and maintained, and Qt Quick would add QML and a scene graph to what's still a form. Rust calls it through the [cxx](https://cxx.rs) crate, and `settings/src/qt.cpp` only builds widgets and copies state onto them. The model, the rules, and the file are all in Rust.

- Qt doesn't need a file format of its own for this, so settings stay in shcl, under `~/.config/tukzmon/`.
	- Each panel item has its own file, `plugin-<id>.shcl` by the panel's unique id, named like its history file. Its Properties opens that file, so 2 items on a panel can show different graphs.
	- `tukzmon.shcl` is the shared file. An item without a file of its own yet starts from a copy of it, so a panel set up before items had their own keeps its look. The shared file is never changed by an item, and `tukzmon-settings` without `--config` edits it, which sets what a new item starts from.
	- Without a shared file either, a new item starts at the defaults, and it doesn't write a file until a save.
	- A file that's there but can't be read, such as one without read permission, isn't taken for a missing one. `tukzmon-settings` says so in a message box and on stderr, and quits without changing anything, rather than opening at the defaults for a save to write over it.
	- With no display for Qt to open, `tukzmon-settings` prints Qt's lines and quits with 1, for the box and for the window, rather than aborting with a core dump.
	- Removing an item from the panel deletes its file and its history file, as the panel's Remove dialog warns, and closes its Properties window, even one opened before a panel restart, which it finds by its `--config` in `/proc`. So a later item that's given the same id starts from the shared file, without any history.
		- The panel can also drop an item without telling it, such as Remove after the item crashed twice, or yes to "could not be loaded". Then its files stay. But the panel deletes its own `xfce4/panel/tukzmon-<id>.rc` when it makes a new item on an id. So each item writes that file at start, and then a `plugin-<id>.mark` beside its settings file. An item that finds the mark but not the panel's file deletes the old settings and history files before it starts. An item from before the marks has neither, and keeps its files.
	- Properties doesn't start a second window when the item's window is open already. It sends that window SIGUSR1, and the window un-minimizes, raises, and activates itself. On X11 it also asks the window manager the way a task list does, so one on another workspace comes up too. A window from an older build, which doesn't catch SIGUSR1, is left alone, since the signal would end it, and the tip says it's open. The tip says so on Wayland too, where the compositor may not let it come forward. A window that can't start, or quits with an error, is in the tip too, as well as the journal, until the next window opens or one closes cleanly. Cancel, Esc and the close button quit with 0, as OK does, since closing without saving isn't a failure.

- It follows SilkTerm's settings dialog, on top of knobs:
	- There are tabs down the left, groups with a rule under the title, and a sub-group stepped in under the switch it depends on. A row is greyed while that switch is off, but its revert arrow still works.
	- Every row has a revert arrow, which is live when the value isn't its default.
	- The buttons are Cancel, Apply, and OK, with Cancel set a little apart. Enter is OK and Esc is Cancel, even from inside a field. Enter on a focused button presses that button.
		- Apply turns on as soon as a box is typed in, before it's left. Apply and OK take what's typed, Alt+A included, which leaves the focus where it was.
		- A value put back to its default is no change, so Apply goes off again, and the file gets no line for it. A line that only names a default, as builds before 2026-11-09 wrote one, is left out at the next save, so the setting follows a later change to the default. Theme colors and the theme pick are the exception, since Custom's colors are a set and the file is read by its pick.
	- A question that can't be undone, such as Delete theme or Reset history, starts on No, so Enter on it doesn't change anything.
	- An empty checkbox or radio button gets an edge in the text color, a little faded, over what the style draws. Some dark themes draw that edge close to the window color, so it almost vanished. `BoxStyle` in `qt.cpp` does it.
	- Every button with text has at least 12 px across and 8 px over and under, from its text to the inside of its frame, in the window and in its message boxes. The style may give more. So every button is one height, the bottom ones included. `BoxStyle` does it too.
	- A slider's range can be shorter than what its number box takes. The box takes any sane value, and `slider_range` in `core/src/settings.shcl` sets how far the slider goes.
		- A click anywhere on a slider but its handle puts the handle there, and holding the button on drags it from there. A press on the handle drags it from where it was taken, so a grab doesn't nudge it onto the next detent. `TrackSlider` in `qt.cpp` does it.
		- A slider is as tall as the number box beside it, and its handle as tall as the slider. `BoxStyle` does that.
	- The selected tab is a tint of the theme's highlight under the normal text color, so it reads in a dark theme too.
	- A tab with rows of its own and tabs under it, which is Themes, keeps those rows on top of whichever of its sub-tabs is picked. The sub-tabs are listed under it in the tab list, stepped in, and picking Themes itself shows the first one. The sub-tab shown and Themes over it are both lit, the one picked in the full tint and the other dimmer, so Themes stays lit while any of its sub-tabs is picked. Ctrl+Tab and Ctrl+Shift+Tab go page by page, as a tab bar does, so Themes itself isn't a stop, since it only shows Colors.
		- Themes has the Name dropdown with Save, Save as, Rename, Delete, and Randomize, over 2 sub-tabs. Colors has every graph's colors, and Outline has the margins and the colors that graphs share. Like every other change, the theme buttons take effect at Apply.
		- Colors has a section per graph, under the graph's name, in the graph order. A section moves when its graph is moved in the graph list, before Apply. Its mode and main colors are on the left, and its Background on the right. The mode is on its own line, and Background starts level with what the mode shows under it, such as From the bottom up or By height. Each revert arrow is right after the box it resets, and the mode's after its last choice. The page scrolls in the height the other tabs set, so it never makes the window taller.
		- `graph: name` on a heading in `core/src/settings.shcl` starts that graph's section, and `beside: true` on a heading puts it and the rest of its section on the right. Both keys are tukzmon's own, and knobs ignores them.
	- `shown_as` and `shown_when` in `core/src/settings.shcl` are tukzmon's keys too. `shown_as: radio` draws a dropdown as radio buttons, and `shown_when: "id = key"` shows a row only while that dropdown is on that key. knobs' gates only take a checkbox.
		- `same_line: true` is tukzmon's too. It puts a row to the right of the row above it, on that row's line, as Max age in days beside Keep across restarts.
	- The window opens as wide as its widest tab needs, so the graph list's dropdowns aren't squeezed.
	- The window opens as tall as its tallest tab needs, Colors left out, and never taller than the screen less its panels, with the title bar and border included. The window manager's frame isn't known until the window is up, so the first move after that takes it off the size and keeps the window on screen. `fitFrame` in `qt.cpp` does it.
	- The tabs are General, Graphs, and Themes. Graphs is a list with a row per graph rather than a row per setting. See [XFCE plugin](#xfce-plugin). General has what every graph shares. Its groups are Size, with Width px default and Border px, then Time axis, Vertical axis, Timespan, and Misc. Max age in days is to the right of Keep across restarts, on its line.
	- The page that scrolls and the tab list are set in, a shade darker than the rest of the window. So the page reads apart from what stays put, which is the theme's Name row over the Themes sub-tabs and the buttons at the bottom. No rule is drawn between the two, since the shade marks the edge. The two shades span the step from the theme's window color 1/16 of the way to its text color. The parts set in take the darker end, and the rest stops 3/4 of the way to the lighter one. So a dark theme keeps its own window color for the parts set in, and a light theme for neither. `shadeOf` in `qt.cpp` does it.
	- A color chip opens a picker, and the hex value can be typed beside it. The picker is SilkTerm's rather than the toolkit's. It has a saturation and brightness square, a hue strip, and boxes for red, green, blue, brightness, saturation, and hex. The row behind it follows every change, and Cancel puts back all of it, the theme's "*" included. The color model is in `settings/src/pick.rs`.
	- A screen reader names each control by its row's label, with the graph's name in front on Colors, as "CPU Low". A revert arrow is "Revert" and that name, a color chip "Pick a color for" it, and the color picker's title is the name too. In the graph list each control names its graph. A row's label is its control's buddy, since Qt on Linux names a dropdown by its text and finds its label that way.
	- A tip is broken into lines of at most 70 characters, the width GTK wraps its own tips at, so the panel's and the window's read alike. Qt would show a plain tip as one line. `wrap_tip` in `settings/src/bridge.rs` does it, and an event filter in `qt.cpp` sends every tip in the window through it.
	- The file gets only what was changed in the window, applied to the file as it is at save time, so a hand edit that was made meanwhile isn't lost.
	- A line it can't read, or a setting it doesn't know, such as one an older build wrote, is dropped at the next save rather than refused, since most users never edit the file by hand.
	- Every file is read by shcl 3 rules, with or without a Format line. tukzmon doesn't read or convert a file written for shcl 2, since there aren't any in use. Every save writes the Format line under the header, so a later format change can tell the version.
	- Comments a hand edit put in the file stay, as long as the line or section they're on does. A changed value keeps its line's comment. `keep_notes` in `settings/src/dialog.rs` sets the new values on the file as shcl loaded it. If that wouldn't read back as the values meant, the file is written plain, without the comments.
	- What Apply does with a change made elsewhere, in another window or by hand, while the window was open. The theme pick and the colors changed on top of it, the "*" ones, go together, since a "*" color means nothing on another theme. `carry`, `carry_theme` and `merge_themes` in `settings/src/dialog.rs` do it.

	| Made elsewhere                        | Changed here                                       | At Apply
	| :---                                  | :---                                               | :---
	| Anything                              | Something else, outside the theme                  | The file's stays, and this window's goes on top.
	| A setting                             | The same setting                                   | This window's.
	| The theme pick, or a "*" color        | Nothing on the theme                               | The file's pick and its "*" colors stay.
	| A "*" color on the theme picked here  | Another "*" color, on the same theme               | Both.
	| The theme pick                        | The pick, a "*" color, or a Custom color on Custom | This window's pick, with its own "*" colors. The other's "*" colors go, as a pick drops them.
	| A Custom color                        | Another Custom color                               | Both, whichever theme is picked.
	| A saved theme's name or a color in it | Another of its name and colors                     | Both. Each name and color is the last save's.
	| A new saved theme                     | A new one with the same name or key                | Both. This window's moves to "Name 2".
	| A saved theme deleted                 | Changed, or picked here and changed on             | It comes back as this window has it.
	| A saved theme deleted                 | Nothing on it                                      | It stays deleted.

- Reset history is on the left of the bottom strip, under the pages, and starts every graph over at once, in the window and in every panel item. It also deletes every panel item's saved history, whether its Keep across restarts is on or off. When any saved history file exists, it asks first. It's an action rather than a setting, so it doesn't wait for Apply, and Cancel doesn't take it back.
	- It writes `reset` in the history folder. First it deletes every `plugin-<id>.hist` there, so an item that isn't running doesn't bring old history back. A running item that keeps history deletes its own again, since it may have saved in between.
	- A running plugin watches that file, as it does the settings file, and checks its modified time at each sample too. It drops what it has in memory, and when history is kept it deletes its own file again, since it may have saved in between.
	- With Keep across restarts off, the file is left as it is, the same as Off always does.

- Still to come are a live preview in the panel before Apply, and the search box.

### Theme

- Graph colors are meant to start from the panel theme, which is the GTK style context on XFCE and the Qt palette on LXQt. That's how the graphs pick up a tukzedo personality's theme. For now the defaults are a hand-picked set instead, which started as a customized multiload-ng setup.

- Each color can be overridden in the settings window.

- Every color is in one knobs preset group, `theme`, picked with the Name dropdown on the Themes tab, at `theme.name`, and so is each graph's mode. The built-in themes in `core/src/settings.shcl` are Default, Primary, Pastel, Neon, Earth, Fire, Terminal, Vintage, and Frost.
	- Every graph is colored by one theme, so a graph doesn't pick its own. A theme has a value for every graph's colors and mode, and picking one changes every section.
	- A theme's name doesn't say anything about color. Each graph has a lead color of its own in every theme but the one-color ones, so graphs side by side are told apart. A graph's quiet series and its gradient's Low are its hue, muted. Each graph's border and background are neutral, or lean to the theme's mood.
	- A graph's lead is the color it reads as from across the room. That's its main series in Detailed, such as user for CPU or received for the network, or its Medium in Gradient. `config::LEADS` lists them.
		- In every built-in but the one-color ones, any 2 graphs' leads are 10 or more apart as CIEDE2000, but for one pair in Default, below. In every built-in, every lead is 10 or more from I/O wait and system. At 10 or more, 2 colors read apart at a glance.
		- Hue angle alone doesn't do it. An equal step in hue looks far smaller among the greens than among the blues, and 13 graphs share what's left of the circle off red and orange. So leads also differ by lightness and saturation.
		- The measure is in `core/src/color.rs`, checked against the test pairs Sharma, Wu, and Dalal published with the formula. Randomize uses the same one.
	- I/O wait and system stay red and orange in every theme but Default, so trouble reads the same whichever is picked. No graph's own hue is red or orange, except in Fire, Earth, and Vintage, where they're warm but not the alarms', and in Default.
	- Default is that hand-picked set, which is the earlier multiload-ng colors with the changes made to them since. Where those break a rule the other built-ins keep, Default follows the colors picked.
		- System is pink, `#fa72f0`, and the battery's on battery is copper, `#c46e43`, rather than system's color.
		- GPU, GPU memory, swap, zswap, load, and temperature are drawn as gradients. Load's is dark red, and temperature's runs from light teal through a dull yellow to red.
		- Swap's and zswap's gradients run through the colors picked for them in Detailed, amber `#a45f00` and green `#4a8e54`, and temperature's normal is its Medium. So a graph switched to the other mode keeps its color.
		- The network's local and sent, and the disk's written, are colors of their own rather than shades of the lead.
		- The border and the top are gray, `#8b8b8b` and `#313131`.
		- Programs on the memory graph moved from `#ffff80` to `#ffffb8`, the least change that reads apart from temperature's yellow and processes' yellow.
		- Temperature's Medium and processes' Yours are both picked, and under 10 apart. They stay as picked, the one pair of leads under 10 in Default.
	- Terminal, Vintage, and Frost are one-color themes, like a green phosphor screen, an amber one, and pale ice. Every graph has the same colors in them, so graphs are told apart by place and by the gap between them, as on a one-color multiload-ng setup.
		- There's no way to space 13 graphs 10 apart by lightness alone, so these themes are let off the check on leads. A theme whose graphs all lead with one color is one of them, and only that lets a theme off.
		- I/O wait and system stay red and orange, 10 or more from the lead and from each other, so trouble still reads. Each graph's own colors are 10 or more apart, and from the top.
	- Each color row's default is Default's color, so Custom starts from it. `config::tests::the_default_theme_is_the_row_defaults` ties the two together.
	- A color changed on a theme is kept apart from it, and the dropdown says "Name *", the way SilkTerm's does. Picking another theme drops those changes.
	- knobs reads a color line at its own path as the Custom set. A file with colors and no theme line reads as Custom, so a hand edit or an older file isn't hidden under Default. A save names the theme whenever Custom colors are stored, even Default, or the file would read back as Custom.

- Each graph's section starts with its mode, Detailed or Gradient, at `<graph>.mode`, such as `cpu.mode`.
	- Detailed stacks a color per series, as before.
	- Gradient draws the column in one blend of Low, Medium, and High. A pixel takes the blend at its height in the plot, Low at the bottom, Medium halfway and High at the top, so a busier column goes further up the ramp and the colors stay put as it grows. The column's height is the same as the stack's.
	- Only the rows for the mode picked are shown. A mode changed by hand is a change on the theme, like a color.
	- Fire is a Gradient theme, and each graph burns a flame of its own. CPU is dim red, yellow-white, then light blue, like a hot flame. Memory is a violet flame, swap a blue gas flame, the network a pink lithium one, zswap a cyan one, load a yellow-green barium one, the disk an indigo one, temperature a lilac potassium one, the battery a green copper one, tukzmon a bright green boron one, the GPU a sky blue one, GPU memory a cornflower one, and processes a pale salmon calcium one. The other built-ins are Detailed, but for Default's gradients above. In them each graph's gradient runs from its muted color through its own lead to a pale tint of it.

- The memory graph's colors are in the Memory section: Programs, Buffers, and Cache, at `mem.colors.user`, `.buffers`, and `.cache`, plus its own Low, Medium, and High.
	- Default's are multiload-ng's memory colors, `#804080` and `#408060`, with programs a little paler at `#ffffb8`.
	- In the other built-ins, programs take memory's own hue, cache is that hue muted, and buffers a muted color a step round from it. Fire draws memory as a gradient too.
	- multiload-ng's memory border and background aren't Default's, so every graph keeps one look there. They can be set in the Memory section.

- The swap graph's colors are in the Swap section: In use, at `swap.colors.used`, plus its own Low, Medium, and High. Every built-in gives swap a hue and gradient of its own. Default's is amber, `#a45f00`, and Default and Fire draw swap as a gradient.

- The network graph's colors are in the Network section: Local, LAN sent, LAN received, Sent, and Received, at `net.colors.local`, `.lan_sent`, `.lan_received`, `.sent`, and `.received`, plus its own Low, Medium, and High. Received is the network's own hue, local is that hue muted, the same as Low, and sent is a pale tint of it, the same as High. Default's received is light blue, `#6ad4ff`, with local yellow, `#ffed00`, and sent a light red, `#ff7777`.
	- With the split on, sent and received are the internet's. In the built-ins LAN received is between Low and Medium, and LAN sent between Medium and High, each moved as little as it takes to read 10 or more from the other 4.
	- The tip gives the 3 rates in bytes a second, in decimal units, one to a line, such as "Received 1.5 MB/s", "Sent 30 kB/s", and "Local 2.0 kB/s". With the split on it gives "Internet received", "Internet sent", "LAN received", and "LAN sent" lines.

- The load graph's colors are in the Load section: Load, at `load.colors.load`, plus its own Low, Medium, and High. Default's is a dark red gradient, `#553030`, `#924444`, and `#eb4a4a`. Its tip reads like "Load 1.25".

- The disk graph's colors are in the Disk section: Written and Read, at `disk.colors.write` and `.read`, plus its own Low, Medium, and High. Read is the disk's own hue, and written a pale tint of it, the same as High. Default's read is light blue, `#6bb3ff`, and written red, `#c60000`.
	- The tip gives both rates the way the network's does, such as "Read 1.5 MB/s" over "Write 30 kB/s".

- The temperature graph's colors are in the Temperature section: Normal and Critical, at `temp.colors.normal` and `.critical`, plus its own Low, Medium, and High. Normal is a hue of its own, and critical is the theme's I/O wait color, so going over reads as an alarm, as the battery's critical does. Default draws it as a gradient, from teal `#60dae2` through yellow `#d9da62` to red.
	- The tip gives the reading and the level it turns critical at, in whole degrees C.

- The zswap graph's colors are in the Zswap section: In use, at `zswap.colors.used`, plus its own Low, Medium, and High. Every built-in gives zswap a hue and gradient of its own. Default's is green, `#4a8e54`, as a gradient.
	- The graph is zram's on a box with zram and no zswap, and its tip says which, such as "Zram 40% used".
	- The tip gives the compression ratio from the latest read on a second line, such as "Compressed 4.7x now". The ratio isn't kept in the history, so it's the latest one wherever the pointer is, and the line says so. An empty pool doesn't have a ratio, so the line is left out.
	- Nothing is written over the graph. The ratio was, until backlog 2026102814061810 moved it to the tip.

- The battery graph's colors are in the Battery section: Plugged in, On battery, and Critical, at `battery.colors.charging`, `.discharging`, and `.critical`, plus its own Low, Medium, and High. In every built-in, plugged in is a hue of the battery's own, and on battery and critical are the theme's system and I/O wait colors, except Default's on battery, so on battery reads as a warning and critical as an alarm. The gradient is the battery's own. Default's plugged in is lavender, `#b080ff`.
	- The tip says plugged in rather than charging, since a full battery with the power in counts as charging.

- The tukzmon graph's colors are in the tukzmon section: CPU use, at `self.colors.used`, plus its own Low, Medium, and High. Default's is `#00ccb8`.
	- The tip gives the share of all CPUs, then of one CPU, as top shows it, such as "tukzmon 0.014% of all CPUs" over "0.46% of one CPU". The share is often under 0.1%, so it's shown to 2 figures or so rather than in whole percents.

- The processes graph's colors are in the Processes section: System, Other users, and Yours, at `procs.colors.system`, `.users`, and `.me`, stacked in that order from the bottom, plus its own Low, Medium, and High. Yours is the lead, and System is its Low, as memory's cache is. Default's are gray `#707070`, blue `#6bc9ff`, and yellow `#fff02a`, and its Low and Medium are System's and Yours.
	- The tip gives the total, then each owner's count, such as "Processes 612", "Yours 112", "Other users 20", and "System 480", one to a line.

- The GPU graph's colors are in the GPU section: Busy, at `gpu.colors.busy`, plus its own Low, Medium, and High. GPU memory's are in the GPU memory section: In use, at `vram.colors.used`, plus the same 3. Default's are blue `#3b6fd9` and violet `#843bd9`, and also the one-color themes give both tukzmon's color.

- Each graph's section has its Background on the right, with Border, Top, and Bottom, at `cpu.colors.border`, `.top`, and `.bottom` for the CPU graph, and the same under each graph's own section. They're in the `theme` group, so each theme gives every graph its own. The plot blends from Top down to Bottom, inside the border.
	- In every built-in, each graph has the colors the theme had for all of them, so a built-in looks as it did.
	- The Outline sub-tab has one Border, Top, and Bottom too, at `graph.overrides.colors.border`, `.top`, and `.bottom`. The One border for every graph checkbox, at `graph.overrides.border`, puts its Border over every graph's own. The One background for every graph checkbox, at `graph.overrides.background`, does the same for Top and Bottom. Both are on in Default and off in the other built-ins, and in the `theme` group, so a theme sets them and a saved one keeps them.
		- Their colors are greyed while the box over them is off, since they draw nothing then. Picking one of them ticks its box.
		- Randomize rolls the colors and leaves the 2 boxes as they were.
		- `Look::from_model` is the one place they apply, so each graph's `Outline` is already the one to draw.
	- A file or a saved theme from before each graph had its own colors may have set the shared ones. Then every graph that doesn't have its own gets the shared ones, so the file keeps its look. The next save writes them per graph.
		- A file from then is recognized by the old paths, `graph.colors.border`, `.top`, and `.bottom`, and a load moves them to the new ones. A save leaves out a graph's colors when they're its theme's, so a missing one can't tell an old file from a new one. A saved theme is written whole, so a missing one there does.

- Saved themes are kept whole in the config file, under `themes.<key>`, with a `name` and every color and mode by its setting id, laid out the same way as a built-in in settings.shcl. The dropdown lists them after the built-ins, and before Custom.
	- They join the `theme` group as presets on a copy of the spec, through knobs' `Spec::finish`. The plugin reads them the same way, so knobs is unchanged.
	- The key comes from the name when the theme is made, and stays through a rename. A key that is a built-in's is left out, and a color line that can't be read is dropped at the next save, like any other. A missing color is Default's.
	- Save puts the "*" changes into the saved theme in force. Save as makes a new theme from the colors showing and picks it. Delete picks Default. Built-in themes can't be saved over, renamed or deleted, and no two themes share a name.
	- A save writes every saved theme. A theme this window didn't touch is written as the file has it, so a theme added by hand meanwhile isn't lost. One it changed gets only the name and colors changed here, so another window's edits to it stay.
	- A theme made in this window whose key or name another window saved meanwhile moves to a free one at save, such as "Mine 2" at `mine_2`, and stays picked. So both are kept.

- The margins, on the Outline sub-tab at `graph.margins.top_bottom` and `.left_right`, are in the `theme` group, so a theme sets them, and a saved one keeps them. Every built-in has 2 and 2. Randomize leaves them as they are, since a roll is colors. A saved theme writes them as numbers, the override boxes as `true` or `false`, and the rest as text.

- Transparent, on the Outline sub-tab at `graph.transparent`, leaves out every graph's Top and Bottom colors so the panel shows through, overrides or not. It's one setting for every theme, outside the `theme` group, so a theme pick, a roll, or Save as doesn't touch it. The border, the series, and the hover line stay opaque. Pixels are premultiplied, so a row a column only half fills is half clear.

- Randomize rolls colors for every graph and sets them as changes on the theme in force, so the dropdown says "Name *". Save as keeps a roll, and picking a theme drops it. The rules are in `settings/src/dice.rs`.
	- Each graph leads with a hue of its own, the user share for CPU, at any hue from yellow-green round to magenta, which keeps it off red and orange. The 13 lead hues are at least 15 degrees apart, in a random order. A graph's quiet colors and its gradient's Low are its hue muted, or one up to 30 degrees from it. Each graph's border and background are its lead hue or its quiet one, darker and grayer, and the Outline sub-tab's are CPU's. The top is dark, at about 15% brightness, so a quiet color shows on a background of its own hue. At 30%, almost half the graphs rolled had a quiet color under 10 from their top.
	- I/O wait stays red and system orange, as in the built-ins, since those are the ones to watch. Every color softens together, from as bright as Primary to as pale as Pastel.
	- Every graph leads at one saturation and brightness, so a roll reads as one set.
		- Where 2 leads would still look alike, or a lead would look like I/O wait or system, one of them steps darker or paler, until every 2 are 10 or more apart as the built-ins are. With 13 graphs a set of hues often doesn't have any such steps, so up to 256 sets are drawn. At 16 degrees apart some seeds ran out of sets, so the gap is 15. About 1 roll in 7,500 still runs out, all near the pale end, and then the saturation, brightness and alarms are drawn again, so every roll reads apart. After 64 of those a roll would start over from seed 0, whose first draw reads apart. CPU's lead never steps, since the rest fit round it. A graph that steps paler takes its quiet colors and tint along, and one that steps darker keeps its quiet colors clear of the background.
		- The draws come from the seed, so one seed always gives the same roll.
	- The network's LAN received is between its quiet color and its lead, and LAN sent between its lead and tint, both on the buffers' side of the lead. Each steps in hue, saturation, and brightness till it's 11 or more from the network's other colors. Without that, about a third of rolls had a pair under 10.
	- A roll is always Detailed. Each graph's gradient runs from its quiet color through its lead to a pale tint, like the built-ins', so it's ready if Gradient is picked. The hover line stays black.
	- Buffers has a role of its own. It's muted like cache, but on the other side of the lead hue and at least 30 degrees from cache, so that way the 2 are told apart.
	- Every theme member needs a role in `dice::ROLES`. A new graph's members fail `dice::tests::every_theme_member_has_a_role` until they get one, such as buffers and cache as muted ones.

## Panels and trays

A tray icon mostly can't be wide, but a panel plugin can.

| Host                        | Wide graph | How
| :---                        | :---       | :---
| XFCE panel                  | Yes        | Native plugin, any width.
| LXQt panel                  | Yes        | Native plugin in C++ and Qt.
| KDE Plasma                  | Yes        | A plasmoid in QML, with a C++ part that calls the library.
| GNOME Shell                 | Yes        | An extension in JavaScript. It can't load the library, so it reads from a small helper process.
| Cinnamon                    | Yes        | An applet in JavaScript, with the same helper as GNOME.
| MATE                        | Yes        | An applet in C.
| Budgie                      | Yes        | An applet in C or Vala.
| Wayfire panel               | Yes        | A widget in C++.
| Waybar                      | Yes        | A CFFI module in C.
| Tray icons on Linux and BSD | Mostly no  | StatusNotifierItem hosts put each icon in a square slot. Some show a short text label beside it, which fits a number but not a graph.
| Windows 11 tray             | No         | Small square icons only. Windows 11 dropped taskbar toolbars, so a wide graph has to be a small window placed over the taskbar.
| macOS menu bar              | Yes        | A status item can be any length.

On panels without a native plugin, the plan is a tray icon with one small graph, where a click opens a full window.

## Data sources on Linux

| Graph       | Source
| :---        | :---
| CPU         | `/proc/stat`. The I/O wait, system, user, and nice columns.
| Memory      | `/proc/meminfo`. User is total minus free, buffers, and cache. Cache is `Cached` plus `SReclaimable`, as `free` counts it and as multiload-ng does with `graph-mem-procps-compliant` on. Slab that can't be reclaimed is in user. Where ZFS is loaded, cache also takes in the ARC's `size` over its `c_min` from `/proc/spl/kstat/zfs/arcstats`, as htop counts it, since `Cached` leaves the ARC out and the ARC won't shrink below `c_min`.
| Network     | `/proc/net/dev`, in bytes per second. Local is what the loopback device, `lo`, received, since it receives all it sends. Sent and received add up the interfaces on hardware, the ones with a `device` link in `/sys/class/net`. Bridges, VPN tunnels and VM and container links are left out, since what they carry either crosses a hardware one too or never leaves the box. Counting them made a VM's download show about 80 times over. A box with no hardware interface, such as a container, counts every one but `lo`. A counted interface coming or going, or a counter that went down, makes that sample a gap.
| Swap        | `/proc/meminfo`. Used is `SwapTotal` less `SwapFree`, against `SwapTotal`. A box without swap at start doesn't get a Swap graph, and swap turned off later is a gap.
| Zswap       | `Zswap` and `Zswapped` in `/proc/meminfo`, on kernels new enough to have them. Used is the pool, `Zswap`, against the cap zswap stops taking pages at, `max_pool_percent` of `MemTotal`. The ratio is `Zswapped / Zswap`. `/sys/kernel/debug/zswap` needs root, so it isn't used.
| Zram        | `/sys/block/zram*/mm_stat`, summed over every device there at start, when zswap is off. Used is `orig_data_size` against `disksize`, and the ratio is `orig_data_size / compr_data_size`. A box with neither, or with zram devices that don't have a size, doesn't get a Zswap graph.
| Load        | `/proc/loadavg`, its first column, the 1 minute load.
| Disk        | Each counted drive's `stat` under `/sys/class/block`, as 512 byte sectors read and written. All disks means each device with a `device` link under `/sys/block`. That leaves out partitions and what's stacked on a disk, such as LVM, LUKS, RAID, loop, and zram, so nothing is counted twice. A chosen list counts only the names on it, partitions too. The drives are listed again every 10 s, so one plugged in is counted from then. A counted drive coming or going, or a counter that went down, makes a gap.
| Temperature | `hwmon` under `/sys/class/hwmon`, picking the CPU package sensor: `Package id` on `coretemp`, `Tdie` or else `Tctl` on `k10temp` and `zenpower`, and the first sensor on an Arm board's `cpu_thermal`, `cpu0_thermal` or `soc_thermal`. With more than one socket it's the hottest. `temp*_crit` gives the critical level when the sensor has one, the lowest of them with more than one, or else it's 95 degrees C, Zen's own limit. A reading comes as 2 series that add up to it: normal, up to that level, and critical, what's over it. Other drivers, such as `acpitz`, `nvme`, and the board's, aren't the CPU's, so a box without any of these doesn't get a Temperature graph.
| tukzmon     | The CPU time of tukzmon's own process, from `clock_gettime` on `CLOCK_PROCESS_CPUTIME_ID`, against the time between reads times the CPUs it can run on. It's one call, without a file to read. On X11 the process is the panel's wrapper, which runs only this plugin, and its main thread has nearly all of that time. On Wayland it's the whole panel, other plugins included, so the name in the tip is Panel and a line says so. In the settings window's graph list it's the window's own use, named Settings window.
| GPU         | The first GPU. On NVIDIA that's NVML, `libnvidia-ml.so.1`, which comes with the driver and is loaded at run time, so a box without it still starts. Busy is `nvmlDeviceGetUtilizationRates`' `gpu`, the share of the driver's last sample period that ran work. Without NVML it's the first `/sys/class/drm/card*` with amdgpu's `device/gpu_busy_percent`. Neither needs root. Intel cards don't give a busy share without root, so a box with only those doesn't get GPU graphs. NVML is open only while the GPU or GPU memory graph is there, and is shut down when both are hidden. An NVML call can wake a card the kernel has put to sleep, and a call each second can keep it up. So where the first nvidia-bound display card under `/sys/bus/pci/devices` has `power/control` at `auto`, NVML stays shut while its `power/runtime_status` is `suspended`, and both graphs read 0. Once the card has read idle for 5 s, whatever the sample time, both readers shut NVML together and leave it shut for the card's `power/autosuspend_delay_ms` plus 5 s, so it can sleep. That rest ends as soon as the card is `suspended`, so work that wakes it reads at once. Otherwise they open it again when the rest is up, if it's still awake. Work that starts while the card is still awake in the rest reads 0 until the rest is up, since only NVML can tell, and asking it would keep the card up. Reading those files doesn't wake it. A card that's been asleep since start doesn't have a total yet, so the GPU memory tip gives only its share. On a laptop with an NVIDIA GPU and another one, NVML's first device is the NVIDIA one, since NVML doesn't list any other kind.
| GPU memory  | The same card as the GPU graph. On NVIDIA it's `nvmlDeviceGetMemoryInfo_v2`'s used against its total, which leaves out what the driver keeps back, as `nvidia-smi` does. A driver older than 510 has only the first call, which counts that part as used. On AMD it's `device/mem_info_vram_used` against `mem_info_vram_total`. The tip gives both in GiB.
| Processes   | The numbered folders under `/proc`, one per process, kernel threads included, by the owner of each folder, which is the user the process runs as. That's one `stat` per process, as htop reads it, rather than a status file each. The `/proc` folder is kept open and listed again from the start each read, and each process is a `fstatat` by name off it, without allocating anything. A read costs around a millisecond on a busy desktop, nearly all of it in the kernel, so the profiler runs with processes on. User IDs under 1000, and `nobody`, are the system's. The user tukzmon runs as is yours, and anyone else is another user's. A process that turned off its core dumps, such as `ssh-agent`, shows as root's. With `/proc` mounted `hidepid`, other users' processes can't be seen, so they don't count.
| Battery     | `/sys/class/power_supply/BAT*`. A mouse's or a headset's battery has another name, so it isn't counted, and neither is a `BAT*` whose `type` isn't `Battery`, or one with `present` at 0. The charge is `energy_now / energy_full`, or else `charge_now / charge_full`, or else `capacity`. More than one battery adds up as one, or gives the mean of their shares when they count in different units. The charge goes in one series by state. It's discharging when any battery's `status` says so, and Full, `Not charging` and `Unknown` count as charging. Critical is discharging with every battery's `capacity_level` at `Critical`, or at 4% or less when a battery has no level, as multiload-ng does. A box without a battery in doesn't get a Battery graph.

A file that's read every tick is kept open and read again from the start, since the open costs more than the read. A file that stops reading, as when its device goes away, is opened again at the next read. Memory, swap, and zswap share one read of `/proc/meminfo` a tick.

Splitting network traffic into LAN and internet is optional, backlog item 2026101916274800. It's off by default, and the user picks the way in settings. `/proc/net/dev` counts per interface rather than per destination, so the split needs one of these:

- An nftables counter set that matches private address ranges. Its overhead is close to zero, but it needs root to install once.

- Polling every TCP socket through `sock_diag` and summing bytes by remote address. It needs no root, but it misses UDP, which includes QUIC, and sockets that close between polls.

- eBPF is exact, but it needs root too. It won't be offered unless the other two fall short.

Either way the LAN is what's counted, and the internet is the rest of what the interfaces carried. So UDP and QUIC to the LAN show as internet with socket polling. The LAN can't be more than the interfaces carried.

The way is picked at `net.split`, a dropdown with `off` and `sockets`. The nftables set is deferred, 2026102914561248, and joins it as a third value when it's done, so a saved pick still reads.

- The network graph always draws 5 series, which are local, LAN sent, LAN received, sent, and received. With the split off the LAN's 2 are 0, so sent and received are everything, as before. `net::Traffic` has it.

- A failed LAN poll is a gap in the whole graph, so the LAN's bytes never show as internet.

- A history saved with 3 series doesn't read as 5, so the network graph starts over once after the update.

With socket polling, `core/src/lan.rs` asks `sock_diag` for every TCP socket's `tcp_info`. A socket is on the LAN when its far end is a private IPv4 address in 10/8, 172.16/12, or 192.168/16, a link-local one, an IPv6 unique local or link-local address, or in this end's own IPv6 /64. Loopback and the box's own addresses go through `lo`, so they stay local. A socket whose own address is on an interface the network graph doesn't count, such as a VM's NAT bridge, a container's link or a VPN tunnel, isn't the LAN's either, since its bytes never cross a counted one as themselves. A bridge, bond or VLAN stacked on a hardware interface, by its `upper_` links in `/sys/class/net`, still counts as the LAN's way out. On such a bridge, a far end behind one of its virtual ports, such as a VM's tap, isn't the LAN's. The neighbor table gives the far end's link address, and that address is in the bridge's `brforward` on that port. The addresses and the far ends behind virtual ports are read again every 10 s and when the interfaces change, so a copy to a VM can count as LAN for up to 10 s at its start. Its bytes are `tcpi_bytes_acked` and `tcpi_bytes_received`, plus 66 bytes of headers for each segment on IPv4 and 86 on IPv6, since `/proc/net/dev` counts those. The far end's address picks which, so an IPv4 client on a dual-stack socket counts as IPv4. A socket that's new since the last poll counts all its bytes as moved, and a failed poll is a gap. A socket that isn't the LAN's only because of the addresses above is kept between polls too, so when it becomes the LAN's, as when an idle VM ages out of the bridge's table, only what it moved since the last poll counts. A message from an earlier dump, left over when one failed partway, is skipped by its sequence number. The netlink socket of a failed dump is closed and a new one opened at the next poll, since the kernel refuses a new dump on a socket while the old one is still going, so a failed dump is one gap.

The BSDs use `sysctl`, Windows uses PDH counters, and macOS uses `host_statistics64` and IOKit. Those come later.

## History

The graph needs data at a finer grain than its own pixels, so it can be widened without going blocky, and so averaging has something to work with. Keeping every sample forever is too much for a box that runs for a year. So older data is kept at coarser steps.

- Samples go into tiers. Each tier keeps fixed-size buckets for a set time, and buckets roll up into the next tier as they age out.

	| Tier | Bucket | Kept for
	| :--- | :---   | :---
	| 0    | 50 ms  | 256 seconds
	| 1    | 250 ms | 2048 seconds, about 34 minutes
	| 2    | 2 s    | 3 hours
	| 3    | 30 s   | 3 days
	| 4    | 5 min  | 90 days
	| 5    | 1 hour | Forever

- Every tier gets every sample, so there's no roll-up step.

- Tiers 0 and 1 are each kept for 1024 times the next tier's bucket. The Width slider's top is 1024 px, so a linear graph that wide keeps one sample a pixel at any rate down to 50 ms. Past that width, or on a 2x screen at a slow rate, the left end can reach a coarser tier.

- A sample is a share of the time since the read before it, so it's added over that whole stretch, to every bucket it covers. Half-second reads on quarter-second buckets would otherwise leave every other bucket empty.
	- A stretch longer than a tier keeps only makes the buckets that tier keeps.

- Each bucket keeps the min, max, and mean of each series, how many seconds of data it has, and where in the bucket that data starts and ends. Max is kept so a short spike can still be drawn once it's old. The start and end let the newest bucket, still filling, read as full where it has data.

- Each tier also keeps a total for every block of 8 buckets, and for every block of 64. A total has the weighted sums, the seconds of data, and the max. A wide column reads the blocks of 64 it covers whole, then blocks of 8 at either end, and only the last few buckets one at a time. Before these, a column on a log axis could walk thousands of 50 ms buckets each draw. With blocks of 32 alone, the buckets at a column's ends took a quarter of a panel's tick. The totals are only in memory, and are worked out again when history loads. A block that's lost buckets off its old end is never read.

- A bucket without any samples is a gap. Gaps are never counted as zero. A gap is drawn as blank, and it stays out of every average.

- Rough size at tier 5: 8,760 buckets a year, times about 28 series, times 12 bytes, so that way it comes to about 2.9 MB a year. Each bucket adds 20 bytes for its seconds and its start and end.

- History is saved under `$XDG_DATA_HOME/tukzmon/`, as `plugin-<id>.hist`, one file per plugin instance. On start the file is read back, and the time since the last sample becomes a gap.
	- It's written every 15 minutes and when the panel quits, to a temp file renamed over the old one. A crash loses at most the 15 minutes.
		- The temp file's name has the process id, since 2 sessions can each run an item with one id. A failed write removes it, and Reset history removes any a crash left.
		- A file that doesn't read, such as one from a newer build, isn't saved over, so going back a version doesn't lose it. The tip says so until it reads, or Reset history deletes it.
	- The file has a named section per graph. A section that no graph shown owns, such as a hidden graph's, is kept at each save, so showing the graph again brings its history back. A file from other tiers, or one that doesn't read whole, is skipped. A file from before tier 0 went in, which has one tier less, still reads, with tier 0 empty. The graph starts fresh, and the next save replaces it.
	- Since file version 2 each section starts with its graph's series count, and a section with another count is skipped. A version 1 file still reads. Its sections don't have a count, so only their layout can refuse one, and a hidden graph's is saved back as it was.
	- Times are Unix seconds, so runs line up. A clock set back to before the saved history is moved up to its end, so new samples aren't dropped as old.
		- Only up to 14 hours, the most a clock can be behind by a time zone, such as a dual boot that keeps local time in the hardware clock. A section that ends further ahead starts over, since that's a broken file or a clock set back for good, and moving up to it would keep the graph that far off.
	- A section whose times are out of order, or whose buckets lie outside them, is skipped. A bucket with a value that isn't finite reads as a gap, and so does a column that adds up to one while drawing.
		- While running, a graph's time follows the wall clock forward. A suspend, or the clock set on by more than a sample's worth, is a gap.
		- A clock set back while running doesn't take the graph's time back with it. Its time runs on from where it was, for the same reason as at a load.
	- "Keep across restarts" turns it off. Off stops saving and loading, and leaves the file as it is, unless Reset history deletes it. Turned back on, the file is loaded again, so what was seen while it was off is a gap.
	- History older than the max age, 28 days by default, is dropped at each save and load.
	- Reset history in the settings window drops all of it, and deletes the file whether Keep across restarts is on or off. See [Settings window](#settings-window).
	- Removing the item from the panel deletes its file, and it isn't saved after that.

## Time axis

- In log mode, the newest sample is at the right edge. Every pixel to the left covers more time than the one to its right, and the left edge is the oldest sample. Until the caps below kick in, the whole run is squeezed into the graph from the first second.

- There are 3 log curves, picked by Scale default under Time axis in settings, beside Linear. Smooth log is the default.
	- Smooth log is one curve over the whole run, `u = ln(1 + age / 5 s) / ln(1 + T / 5 s)`, without any caps. A year in, the right quarter covers about 4 minutes.
	- Log A and Log B put caps on 4 quarters, as below. They differ in the free curve that places a boundary before its cap is reached.
	- Log A is the first curve tukzmon had. Its free curve is scaled to the run, so the caps fill early.
	- Log B's free curve is Smooth log, so it stays the same as Smooth log until a quarter fills its cap. The right quarter fills at about 1.7 days, the third at about 36 days, and the second at about 128 days.

- With caps, the graph is split into 4 quarters. From the right, they cover at most 1 minute, the next 59 minutes, the next 23 hours, and then everything older.

- Each quarter boundary is the smaller of where pure log mode would put it and its cap. Early on, the quarters stay under their caps, and the graph is pure log. Once the right quarter would cover more than a minute, it's pinned at 1 minute and the other 3 quarters share the rest in log mode. The same happens at 1 hour and 1 day.
	- For Log A, pure log is scaled to the run. The age at `u` of the width, from the right, is `T * ((1 + K)^u - 1) / K`, with `K` at 50. So the right quarter covers about 3% of the run, and the caps pin at about 30 minutes, 18 hours, and 7 days.
	- For Log B, it's the fixed log `ln(1 + age / 5 s)` of Smooth log, which pins much later, as above.

- Within a quarter, time is still log scaled.

- While running, the boundaries move smoothly by themselves, since each is a smooth function of the run's length. A jump eases in over half a second instead, from what was on screen to where the axis should be.
	- A jump is anything other than time going on, such as history loaded from disk, the first sample after a gap, a new curve or time axis, or a linear graph's span changing with its width or sample rate. A log axis is in fractions of the width, so a width change only stretches it.
	- Dropping history past the max age is time going on, unless it takes more than 1% off the oldest end in `ln(1 + age / 5 s)`, as a lower max age does. A 15 minute trim of a day takes 0.2%.
	- The two are mixed in `ln(1 + age / 5 s)`, so time still only runs older to the left. A jump part way through an ease starts from what's on screen.
	- While an axis eases, the panel and the settings window's graph list draw about 30 times a second, then go back to once a sample.

- There's no visible seam where one quarter meets the next. The curve between boundaries is a monotone cubic through them, in `ln(1 + age / 5 s)`, so a pixel covers the same time on both sides of a boundary. The move from 1 minute to 1 hour to 1 day to everything older looks like one smooth curve. Tests check both that and that time only ever runs older to the left.

- A linear mode is there too, for anyone who wants a plain scrolling graph. A pixel is one sample there.

- Each graph can pick its own time axis on the Graphs tab: Linear, or one of the 3 log curves. Its Default follows Scale default under Time axis on the General tab, at `graph.time`, so one setting still changes every graph that doesn't have a pick of its own.
	- A graph's pick at `<graph>.time` is `same`, `linear`, `smooth`, `fixed_a`, or `fixed_b`. `same` is the one shown as Default.
	- Scale default, first named Time axis default, replaced a Log time axis checkbox and a Log curve dropdown, `graph.log_axis` and `graph.log_curve`, before any release. A file with either line loses it at the next save from the settings window, like any setting it doesn't know.
	- Spike emphasis applies to any graph on a log curve, so it's never greyed.

## Drawing

- Each pixel column covers a span of time. Its value is the average of every bucket in that span, weighted by how much of each bucket falls in it. Buckets at the edges count in part. That's what keeps the graph from shimmering while boundaries move and columns slide across bucket edges.

- To keep a single-pixel spike visible, a column is drawn from its mean, plus part of the way toward its max. The part gets smaller as the column covers more time. A spike that lasted a long time stays visible far to the left, and a short one fades into the background level once its column covers enough time.
	- The part is the Spike emphasis setting times `1 / (1 + seconds / T)`. T is 20 seconds times 1000 to the power of the setting, from 0 to 1. That halves the part at about 10 minutes at 50%, an hour at 75%, and 5.5 hours at 100%. A stack that comes out over 100% is scaled down to fit.
		- T used to be a fixed 20 seconds. Even at 100% that lost nearly every spike past the first minute of a log graph.

- A column with data for less than half its time is drawn as a gap.

- With the pointer over the graph, a line one pixel wide is drawn under it, black unless set otherwise, over the plot rather than the border. The tip gives the time under the line, either as "7 hrs ago" or as the local time, with the date for anything before today. The time and date follow the locale's short formats, so 12 or 24 hour is its call too. Under that are that graph's shares across the line's column, each name and value on a line of its own, or "No data" for a gap.

- A still pointer's tip keeps its age and moves its clock time. The right edge is always now and the graph scrolls left under the pointer, so each column is a fixed age, while the time in that column advances a sample each tick. Both describe the column under the line. The line and tip follow the column rather than the moment that was first pointed at.

- Stacked graphs, like CPU and memory, draw each series on top of the one below, in the order each graph lists its series.

- Disk and network scale to their recent peak, and the peak is eased too.
	- The network graph's top is the tallest stack in view, spikes included, as drawn. It jumps up at once, so nothing is cut off, and eases down over about 2 seconds once the peak scrolls away or squeezes into a wider column. `render::Peak` has it, and every graph without a fixed top uses it.
	- The top never goes under 10 kB/s, so the odd packet on a quiet line doesn't fill the graph.
	- There aren't any labels on the graph, so the tip gives the rates.
	- The disk graph does the same, with a floor of 100 kB/s, so a log file's flush doesn't fill it.
	- The load graph's floor is the number of CPUs, so it fills only once there's more work queued than CPUs to run it.
	- The processes graph's floor is 100. A box runs more than that, so its top is the highest count in view, and a dip shows as processes end.
	- The tukzmon graph's floor is every CPU busy, as the CPU graph's top is, so it reads as a share of all cores. Its usual use is a few hundredths of a percent, so the graph is close to empty and the tip has the number.
	- The temperature graph's floor is the sensor's critical level, or 95 degrees C when it doesn't have one, as with `k10temp`. Up to that it's normal, and it fills the graph at that level. What's over it is critical, drawn on top, and the top rises with it, the way multiload-ng splits it.

- Every graph can go up on a log10 or squared scale, as well as a straight one. The scale applies to the whole stack, and each series keeps its share of that height. `render::Vertical` has it.
	- On log10, a column's height is `ln(1 + v/u) / ln(1 + top/u)`, with `u` a thousandth of the floor, or of the top for a graph with a fixed one, such as CPU. So the disk graph's `u` is 100 B/s, and 1 kB/s under a 4 MB/s top is about a fifth of the height, where a straight scale gives it 1/4000. On the CPU graph 1% is a third of the height.
	- On squared, a column's height is `(v / top)^2`. It's the opposite of log10, since it makes big values stand out and pushes small ones down. So half the top is drawn a quarter of the way up.
	- A graph without a fixed top fits its peak first, then applies the scale under that peak.
