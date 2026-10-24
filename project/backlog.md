<!-- markdownlint-disable MD007 -- Unordered list indentation -->
<!-- markdownlint-disable MD010 -- No hard tabs -->
<!-- markdownlint-disable MD033 -- No inline html -->
<!-- markdownlint-disable MD055 -- Table pipe style [Expected: leading_and_trailing; Actual: leading_only; Missing trailing pipe] -->
<!-- markdownlint-disable MD041 -- First line in a file should be a top-level heading -->
# Requirements

This is a product backlog just for pre-v1.0.0 release. After that, bugs, features, and enhancements will be managed in Github Issues.

<!-- TOC ignore:true -->
## Table of contents

<!-- TOC -->

- [Issues](#issues)
- [Template](#template)

<!-- /TOC -->

## Issues

Sorted per the notes in [Template](#template).

- Proof of concept: CPU graph in the XFCE panel, dogfooded on b23
	- ID: 2026102319155124
	- Type: Feature
	- Status: Waiting on UAT
	- Priority|Severity: High
	- Opened: 20261023-183946
	- Opened by: t00mietum
	- Related IDs: 2026101916013884, 2026101916013885, 2026101916013886, 2026101916013889
	- Requirements:
		- CPU only, on XFCE 4.
		- Settings are a standalone app that uses a shcl config file, since the graph is meant to be cross-platform.
		- Start with the colors from my customized multiload-ng setup.
		- Use SilkTerm's Settings model, but drawn with Qt: the lightest-weight, fully cross-platform, Rust-compatible, maintained version.
		- If that Qt needs its own file format that goes out to users, use it for user settings too.
	- Progress log:
		- Built: the core library, the plugin, and tukzmon-settings in Qt 6 Widgets. The defaults are the multiload-ng CPU colors.
		- Qt doesn't need a file of its own, so settings stay in shcl, at `~/.config/tukzmon/tukzmon.shcl`.
		- SilkTerm's settings model, knobs, is MIT now under Bubbles, in both projects, and copied in here.
		- The panel only loads plugins from its system folders. `utility/dev-install.bash` installs to `~/.local/lib/tukzmon`, links it there with sudo, and restarts the panel. It's in b23's live panel now.
		- Not done yet: history kept across restarts, a live preview before Apply, and the search box.
	- Branch: dev

- Core library and Linux collectors
	- ID: 2026101916013884
	- Type: Feature
	- Status: Started
	- Priority|Severity: High
	- Opened: 20261019-160138
	- Opened by: t00mietum
	- Prereq IDs: 2026101916013881
	- Requirements:
		- A Rust library with no GUI toolkit in it.
		- Reads every graph's numbers on Linux, from the sources listed in `design.md`.
		- A graph whose source is missing, like a desktop with no battery, is hidden.
	- Progress log:
		- CPU from `/proc/stat` works. The other graphs aren't started.

- History store
	- ID: 2026101916013885
	- Type: Feature
	- Status: Started
	- Priority|Severity: High
	- Opened: 20261019-160138
	- Opened by: t00mietum
	- Prereq IDs: 2026101916013884
	- Requirements:
		- These requirements suggest that all data points should be kept across all time, to continuously recalculate optimal averaging and antialiasing. But that would be impractical for year-long runs. The data will necessarily have to be more temporally compressed as it gets older. But the data will always necessarily need to be at a higher fidelity than the graph, so that:
			- The user can widen the existing graph and not lose resolution.
			- Averaging and antialiasing has enough higher-detailed data to work with.
		- If the graph is closed and then re-opened, it should (optionally) re-load previous data - just with a blank representing the time it was not active. (That doesn't eventually get averaged as values of "0".)
		- If the computer is off or rebooted, the graph should (optionally) re-load previous data, going back indefinitely - just with blank spaces representing the time it was not active. (That doesn't eventually get averaged as values of "0".)
	- Note: the proposed tiers are in `design.md` under History.
	- Progress log:
		- The tiers are in and keep gaps as gaps. Saving and reloading across restarts isn't done yet.

- Graphs with an optional (default) log X axis
	- ID: 2026101916013886
	- Type: Feature
	- Status: Started
	- Priority|Severity: High
	- Opened: 20261019-160138
	- Opened by: t00mietum
	- Prereq IDs: 2026101916013885
	- Related IDs: 2026101916013880
	- Requirements:
		- In log mode, squeezes all data from start to now, across the whole thing, right away.
		- On very long sessions:
			- Four quarters of the graph:
				- The rightmost quarter area covers no more than 1 minute.
				- The second quarter covers no more than the next 59 minutes.
				- The third quarter covers no more than the next 23 hours.
				- The last quarter covers everything else.
			- But if the graph hasn't been running long enough:
				- x <= 1m: The entire graph is an ever-compressing log plot of all available time continuously.
				- 1m < x <= 1h:
					- If rightmost quarter shows <= 1m, then still log plot of all available time.
					- Else [rightmost quarter is therefore > 1m], then (smoothly) adjust the graph so that rightmost covers exactly 1m, and the remaining three quarters plot all of remaining time continuously.
				- 1h < x <= 1d:
					- Rightmost quarter shows <= 1m and second quarter <= 1h, then still log plot of all available time continuously.
					- Rightmost quarter = 1m and second quarter <= 1h, then the leftmost three quarters plot of all remaining time continuously.
					- Rightmost quarter = 1m and second > 1h, then (smoothly) adjust the second quarter to cover only the next 59 minutes, and remaining left two quarters plot of all remaining time continuously.
				- And so on.
			- All adjustments to time divisions should be animated smoothly.
			- The transition from 1m to 1h to 1d to "everything" should be smooth, with no obvious gate or line there behind the scenes.
			- The graph should be averaged and antialiased, so that it doesn't shimmer or look weird while animating.
				- The averaging should be very small and possibly even variable over time - being able to represent single-pixel spikes is important, e.g. when on the farther left, to show for example that the CPU was spiked for a very long time. (But eventually far enough left, it gets averaged into a background level.)
	- Progress log:
		- Log and linear modes work, with the caps and no seams. `design.md` has how.
		- Still needs easing when the width changes or old history is loaded. While running, the boundaries move smoothly by themselves.

- XFCE panel plugin
	- ID: 2026101916013889
	- Type: Feature
	- Status: Started
	- Priority|Severity: High
	- Opened: 20261019-160138
	- Opened by: t00mietum
	- Prereq IDs: 2026101916013881, 2026101916013884
	- Requirements:
		- Start first with an XFCE4 panel plugin.
		- Works on XFCE 4.20, on X11 and on Wayland.
		- One plugin instance can show one or more graphs side by side, picked in its settings dialog.
		- Follows the panel's size and orientation, including a vertical panel.
		- Packaged for Debian, and as a Nix package for tukzedo-linux's NixOS edition.
	- Progress log:
		- One CPU graph in the panel, with a tooltip and Properties opening tukzmon-settings. Tried on X11 only so far.

- The graphs
	- ID: 2026101916013887
	- Type: Feature
	- Status: Queued
	- Priority|Severity: High
	- Opened: 20261019-160138
	- Opened by: t00mietum
	- Prereq IDs: 2026101916013884
	- Related IDs: 2026101916013883
	- Requirements:
		- CPU; with detail stacked from bottom to top: I/O wait, system, user, nice.
		- Memory; with detail stacked from bottom to top: User, Buffer, Cache
		- Network; Local, Sending, Receiving
		- Swap; % used
		- Zswap (or Zram etc.); % used, and a text number overlay for compression ratio, e.g. "2.1x"
		- Load
		- Disk; write, read
			- Auto-adjusts peak level
			- With optional logarithmic Y axis
			- Can choose all disks, or specific drives.
		- CPU Temperature; normal, critical on top of that when normal maxes out
		- Battery; charging, discharging, discharging and critical level

- Settings window: tabs, highlight, number boxes, tips, and buttons
	- ID: 2026102408304546
	- Type: Enhancement
	- Status: Queued
	- Priority|Severity: High
	- Opened: 20261024-082803
	- Opened by: t00mietum
	- Requirements:
		- The tabs are too vertically narrow.
		- The highlight is too bright, and dark-mode text (i.e. white) is hard to read.
			- Make it more readable in either dark or light mode, more like nemo-anywhere (which is a different system, C and GTK).
		- Numeric input boxes:
			- Allow manually typing values that exceed the slider range, but are still sane.
		- Spike emphasis % flyover tip text: Explain what 0% and 100% would do.
		- Buttons:
			- [Cancel] <double space> [Apply] <single space> [Apply and close]

- Update default colors to my current selection
	- ID: 2026102408304559
	- Type: Enhancement
	- Status: Queued
	- Priority|Severity: High
	- Opened: 20261024-082803
	- Opened by: t00mietum
	- Related IDs: 2026102408304612
	- Requirements:
		- Update default colors to use my current selection.

- Option for persistent graph
	- ID: 2026102408304572
	- Type: Enhancement
	- Status: Queued
	- Priority|Severity: High
	- Opened: 20261024-082803
	- Opened by: t00mietum
	- Related IDs: 2026101916013885
	- Requirements:
		- Max age (days); default 365.
		- Note: Don't delete values when disabled, just don't log new ones until reenabled.

- A dropdown for time function
	- ID: 2026102408304586
	- Type: Enhancement
	- Status: Queued
	- Priority|Severity: High
	- Opened: 20261024-082803
	- Opened by: t00mietum
	- Related IDs: 2026101916013886
	- Requirements:
		- Pure log with a fixed 5 s floor (u = ln(1+age/5) / ln(1+total/5), where 5 s is the code's A0).
			- With a better name.
			- Default.
		- Log A with fixed divisions. The existing formula. Show the divisions in its tooltip.
		- Log B with fixed divisions. The existing formula. Show the divisions in its tooltip.
			- Use a different formula: Pure log with a fixed 5 s floor (u = ln(1+age/5) / ln(1+total/5), where 5 s is the code's A0).
				- Or just a less bunched-up log formula.

- Show snapshot time on mouseover
	- ID: 2026102408304599
	- Type: Enhancement
	- Status: Queued
	- Priority|Severity: High
	- Opened: 20261024-082803
	- Opened by: t00mietum
	- Requirements:
		- In the graph, show a thin black vertical line where the mouse is hovering.
		- Mouseover text: How long ago the line is at. Configurable in settings:
			- Date/time (expressed in current user's time).
				- If after midnight, just the time (24-hour or 12-hour depending on user's OS-defined settings).
				- If before previous midnight, date and time (in user's OS-defined short settings).
			- How long ago, e.g.:
				- 5 min ago
				- 7 hrs ago
				- 3 days ago
				- 2 wks ago
				- 9 mos ago
				- 2 yrs ago

- Color themes tab
	- ID: 2026102408304612
	- Type: Feature
	- Status: Queued
	- Priority|Severity: High
	- Opened: 20261024-082803
	- Opened by: t00mietum
	- Related IDs: 2026102408304559
	- Requirements:
		- A tab to define color themes. Similar to silkterm's model.
		- Themes are named starting with whatever color will wind up dominating the monitor, with other colors oriented around that with pleasing color theory. Like "Green standard" (e.g. #00ff00), "Green pastel", etc.
		- Don't use the GTK color dialog. Use something more like silkterm's.
		- The theme tab has a top area that stays, even as sub-tabs change:
			- Name [Dropdown]    [Save] [Save as...] [Rename...] [Delete] [Randomize]
				- The "Randomize" button defines random colors for all monitors, using a few rules:
					- Consistent, pleasing color theory.
					- Doesn't use "Fire" mode.
					- The main regular user-mode usage is the main color, that other colors are based on with color theory.
					- Important stats to watch for getting out of hand (e.g. CPU interrupts) are more alarming colors.
					- Stats like nice, buffers, and cache are more relaxing muted colors.
		- Encode the current theme I've redefined in the settings, as "Default".
			- But update the colors so that they fit into a coordinated color theory, close to the currently defined colors.
		- Each theme:
			- Has per-monitor color settings defined in sub-tabs.
			- Each per-monitor tab:
				- At the top, as a radio button: [ ] Detailed  [ ] Gradient
				- Detailed mode exposes color settings like on silkterm's theme tab.
				- Gradient exposes a simpler set of colors: Low, Medium, High.
					- E.g. a "Fire" theme: dim red, yellow-white, bright light blue. (Like a hot flame.)
					- These get smoothly blended into a gradient, based on how high a pixel reaches into the graph.
		- Global setting to make background transparent, which overrides all background colors.

- Memory graph
	- ID: 2026102408304625
	- Type: Feature
	- Status: Queued
	- Priority|Severity: High
	- Opened: 20261024-082803
	- Opened by: t00mietum
	- Prereq IDs: 2026101916013884
	- Related IDs: 2026101916013887
	- Requirements:
		- Memory, using my current color definitions from multiload-ng.

- Live graph list on the first settings tab
	- ID: 2026102408304639
	- Type: Feature
	- Status: Queued
	- Priority|Severity: High
	- Opened: 20261024-082803
	- Opened by: t00mietum
	- Related IDs: 2026102408304586, 2026102408304612
	- Requirements:
		- Show a list of live graphs in the first tab of settings, arranged in vertical columns (even though the live graphs will be arranged horizontally if in a horizontal bar). With these columns:
			- Checkmarks to enable/disable the monitor.
			- A live graph. The longest graph defines the width of this column, so that the next elements are aligned.
			- Virtual pixel width of the monitor (DPI-aware), text input box.
			- Dropdown to select the time function.
			- Dropdown to select the named theme.
			- Grippers on the right to vertically reorder monitors with the mouse.

- Split network traffic into LAN and internet
	- ID: 2026101916274800
	- Type: Feature
	- Status: Queued
	- Priority|Severity: Low
	- Opened: 20261019-162748
	- Opened by: t00mietum
	- Prereq IDs: 2026101916013887
	- Related IDs: 2026101916013883
	- Requirements:
		- If it can be done easily with low overhead, the network graph shows 5 series: local, LAN sent, LAN received, internet sent, and internet received.
		- Optional. Settings let the user pick the way:
			- Off, the default. 3 series: local, sent, and received.
			- Polling TCP sockets. No root, but UDP and QUIC traffic isn't split.
			- An nftables counter set. Close to no overhead, but needs root to install once.
	- Decisions:
		- Answered 2026-10-19: optional, and the user picks the way. `design.md` has the options under Data sources on Linux.

- Build date after the version, for non-release builds
	- ID: 2026102408304652
	- Type: Enhancement
	- Status: Done
	- Priority|Severity: Avg
	- Opened: 20261024-082803
	- Opened by: t00mietum
	- Requirements:
		- For non-release builds, include " (YYYY-mm-DD)" after the build number.
			- Report the date as the local date of whoever views the version.
	- Progress log:
		- `tukzmon-settings --version` shows "0.1.0 (2026-10-24)". The build time is stored, and the date is worked out in the local time zone of whoever runs it.
		- A build with `TUKZMON_RELEASE=1` leaves the date off. `SOURCE_DATE_EPOCH` is used when set.
	- Branch: build-date
	- Closed: 20261024

- Finish the requirement for moving between time spans
	- ID: 2026101916013880
	- Type: Task
	- Status: Done
	- Priority|Severity: High
	- Opened: 20261019-160138
	- Opened by: t00mietum
	- Related IDs: 2026101916013886
	- Requirements:
		- The log axis item has a sentence that stops partway: "The transition from 1m to 1h to 1d to "everything" should".
	- Progress log:
		- Finished: it should be smooth, with no obvious gate or line behind the scenes. `design.md` has how under Time axis.
	- Closed: 20261019

- Confirm the language split
	- ID: 2026101916013881
	- Type: Task
	- Status: Done
	- Priority|Severity: High
	- Opened: 20261019-160138
	- Opened by: t00mietum
	- Related IDs: 2026101916013889
	- Requirements:
		- OK or change the proposal in `design.md` under Code layout: a Rust core library, and a short C file per panel only where the panel API is C.
	- Progress log:
		- OK'd as proposed. `design.md` now lists it as settled.
	- Closed: 20261019

- What "Local" means on the network graph
	- ID: 2026101916013883
	- Type: Task
	- Status: Done
	- Priority|Severity: Avg
	- Opened: 20261019-160138
	- Opened by: t00mietum
	- Related IDs: 2026101916013887, 2026101916274800
	- Requirements:
		- Loopback traffic, or traffic to the LAN as opposed to the internet?
	- Progress log:
		- Local is 127.0.0.1, the loopback device.
		- A LAN and internet split is its own item, 2026101916274800.
	- Closed: 20261019

**Stop here for a next release**

- Other Linux and BSD panels
	- ID: 2026101916013890
	- Type: Feature
	- Status: Deferred
	- Priority|Severity: Avg
	- Opened: 20261019-160138
	- Opened by: t00mietum
	- Prereq IDs: 2026101916013889
	- Requirements:
		- Supporting all the most popular DE panels.
		- LXQt first, for lxqtukz.
		- Then KDE Plasma, GNOME Shell, Cinnamon, MATE, Budgie, the Wayfire panel, and Waybar. `design.md` has how each one would work.
		- For some platforms, may need to run as a system tray. For others, taskbar-specific plugins.
		- BSD collectors through `sysctl`.

- Windows and macOS
	- ID: 2026101916013892
	- Type: Feature
	- Status: Deferred
	- Priority|Severity: Low
	- Opened: 20261019-160138
	- Opened by: t00mietum
	- Prereq IDs: 2026101916013884
	- Requirements:
		- Cross platform: Linux, Windows, macOS, BSD.
		- Windows 11 has no taskbar toolbars, so it gets a tray icon plus a small window docked over the taskbar.
		- macOS gets a menu bar item, which can be wide.

## Template

- Notes:

	- Only use rows that you actually need or expect will be filled in. Always fill in the title, ID, Type, Status, Opened and Created by.

	- The ID is the local time to the hundredth of a second. Opened is when it was written down, which may differ. (Use a keyboard macro and possibly something like project 'zuid' to generate.)

	- Status values meaning: Testing means the fix is in and checks are running or still to run. Waiting on signoff means automated testing passed. Moot means something else changed that made it irrelevant. Canceled means it still applies but was decided against. Waiting for testing means the fix is in and waits on a long CI run or an outside test host. Can't reproduce means a real attempt to reproduce it failed.
		- An item with nothing left for a person to judge skips Waiting on signoff and goes straight to Done. `Clean up the backlog` in the directives says when.

	- As issues are worked, and statuses change, place them in correct sorting order within the list:
		- First by status: Waiting for answers, Waiting on signoff, Testing, Waiting for testing, Can't reproduce, Stalled, Started, Queued, Done, Deferred, Canceled, Moot
		- Then by severity|priority: Critical, High, Avg, Low
		- Then by type: Bugs, [not bugs together]

	- Rows marked [Bug] are for bugs only, and rows marked [Feature] for features and enhancements. Priority and Severity share one row and one scale. Priority is for a Feature or Enhancement, and Severity for a Bug. Children are not nested. They sit at the top level and point back with Parent ID.

Template:

- Title
	- ID: YYYYmmDDHHMMSSNN
	- Type: [Bug|Feature|Enhancement|Task]
	- Status: [Queued|Waiting for answers|Waiting on signoff|Waiting for testing|Started|Testing|Stalled|Can't reproduce|Moot|Canceled|Deferred|Done]
	- Needs local test suite run?:
	- Needs external testing:
	- Priority [Feature|Enhancement] | Severity [Bug]: [Critical|High|Avg|Low]
	- Opened:
	- Opened by:
	- Assigned to:
	- Parent ID:
	- Prereq IDs:
	- Related IDs:
	- Target OS:
	- Test environment:
	- Version and build:
	- Requirements  [Feature]:
		- Hierarchical bulleted list.
	- Steps to reproduce [Bug]:
		- …
	- Incorrect behavior [Bug]:
	- Expected behavior [Bug]:
	- Reproduced [Bug]: [No, or when, where and how]
	- Possible cause [Bug]:
	- Actual cause [Bug]:
		- …
	- Estimated effort: [High|Avg|Low]
	- Actual effort: [High|Avg|Low]
	- Progress log:
		- …
	- Decisions:
		- …
	- Actual fix [Bug]:
	- Branch:
	- Commit:
	- Test case: [Reason not applicable, or CI test case #]
	- Acceptance signoff:
	- Superseded by ID:
	- Closed:
