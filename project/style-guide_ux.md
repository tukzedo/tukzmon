<!-- markdownlint-disable MD007 -- Unordered list indentation -->
<!-- markdownlint-disable MD010 -- No hard tabs -->
<!-- markdownlint-disable MD033 -- No inline html -->
<!-- markdownlint-disable MD055 -- Table pipe style [Expected: leading_and_trailing; Actual: leading_only; Missing trailing pipe] -->
<!-- markdownlint-disable MD041 -- First line in a file should be a top-level heading -->
<!-- TOC ignore:true -->
# UX style guide

Rules for the settings window, taken from how it already behaves and adjusted where it didn't agree with itself. The window is built from `core/src/settings.shcl` and `settings/src/qt.cpp`. When the window and this guide disagree, one of them is a bug. Where the window doesn't follow a rule yet, the [backlog](backlog.md) has an item. Layout and behavior are described in [design.md](design.md).

<!-- TOC ignore:true -->
## Table of contents

<!-- TOC -->

- [Window layout](#window-layout)
- [Groups and headings](#groups-and-headings)
- [Rows](#rows)
- [Choosing a control](#choosing-a-control)
- [Labels](#labels)
- [Tips](#tips)
- [Colors and themes](#colors-and-themes)
- [Buttons](#buttons)
- [Message boxes](#message-boxes)
- [Keyboard](#keyboard)
- [Wording](#wording)

<!-- /TOC -->

## Window layout

- The window has a tab list on the left, a page on the right, and a strip of buttons along the bottom.

- Tabs are General, Graphs, and Themes. Themes has 2 sub-tabs, Colors and Outline, which are indented under it in the tab list.
	- Each tab has one kind of setting. General has what every graph shares, Graphs has the list of graphs, and Themes has the looks.
	- Tab names are one word.

- The page and the tab list are a shade darker than the rest of the window. Whatever stays put while a page scrolls is the lighter shade, so that way the page reads as set in, and no rule is drawn between the two. The lighter shade includes the theme Name row with its buttons over the sub-tabs, and the bottom strip.

- A page scrolls instead of growing. The window is as tall as its tallest tab needs, but never taller than the screen.

- Settings take effect at Apply or OK. An action such as Reset history takes effect when it's pressed, and Cancel doesn't undo it.

## Groups and headings

- Related rows go under a heading. The heading is bold text with a rule under it.

- On a page that has headings, every row is under one. A group of one row is fine.

- A heading doesn't repeat its tab's name.

- A heading is a noun or a short phrase in sentence case, without a colon. Examples are Size, Time axis, and Timespan.

- Misc is the last group on General, for a row that doesn't fit under any other heading, such as Disk drives or LAN and internet.

- Border px is under Size on General, next to Width px default. It isn't on Outline with the margins, because it's one width for every theme rather than a theme value.

- Groups are 22 px apart. The first group on a page doesn't have a gap above it.

- A row that only matters while a switch is on goes under that switch, and it's indented 16 px per level.
	- The switch comes first.
	- A switch with one short row may have it to its right, on the same line, through `same_line: true`, such as Max age in days beside Keep across restarts.
	- While the switch is off, its rows are greyed. Their revert arrows stay live.

- A row that only makes sense in one mode, such as the Gradient colors, is hidden rather than greyed while another mode is picked.

## Rows

- A row is a label, a control, and a revert arrow, left to right.

- The labels in a group line up. All number boxes share one width, so sliders end in one column.

- A slider is at least 180 px wide, as tall as the number box beside it, and its handle is as tall as the slider.

- The revert arrow is live only when the value differs from its default.
	- Its tip is "Back to the default." for one row, and "Back to the defaults." where it resets several.

- A row's default in `settings.shcl` is Default's value. The built-in themes may differ from it, but Default may not.

## Choosing a control

- A checkbox is for on or off. Its label names the thing, such as "Transparent" or "Keep across restarts", rather than a command.

- A dropdown is for picking one of several named options. Each graph's mode is radio buttons instead, through `shown_as: radio`, since Detailed and Gradient are both worth seeing at once.

- A slider with a number box is for a size or a time that is judged by eye, such as a width, a margin, or a sample time. The box takes any sane value, and `slider_range` sets how far the slider goes.
	- A range that spans decades uses `scale: log`, with detents at the values people pick.

- A number box alone is for a count that a slider would only get in the way of, such as days of history, or a value in a tight list such as the percentages on the Graphs tab.

- A text box is for a list typed by hand, like the graph order or the disk drives. It always has a tip that says the format.

## Labels

- Labels are in sentence case, without a colon or a period. Names such as CPU, GPU, I/O wait, and LAN keep their case. The graph for tukzmon itself is "tukzmon", in lower case.

- Graph names are CPU, Memory, GPU, GPU memory, Network, Swap, Zswap, Load, Disk, Temperature, Battery, Processes, and tukzmon. A per-graph row is the graph name and then the setting, such as "Memory width %".

- Say "graph" in any text that people see.

- Units follow the thing they measure, in short form, such as px, ms, and %. A unit that is a plain word follows "in", as "Max age in days".
	- A General row that each graph can take a share of says "default" after the unit, such as "Width px default" and "Sample ms default". Scale default follows the same rule, under both Time axis and Vertical axis.

- Write "and" instead of an ampersand.

## Tips

- Every control has a tip, and so does every color chip. Headings don't.

- A tip is made of whole sentences that end in a period. The first says what the setting does. Later ones give the range, a side effect, or an example.

- A tip is plain text. The window breaks it into lines of at most 70 characters, so don't add line breaks, apart from a blank line between paragraphs.

- Tips use contractions and digits, and call the thing by the same name the label does.

## Colors and themes

- Colors are in one section per graph, in the graph order. The section's heading is the graph name.

- Each section starts with its mode, Detailed or Gradient, on a line of its own above both columns.
	- The left column has the mode's colors. Detailed has a group named for how the series stack, such as "From the bottom up", "By owner", or "By state". Gradient has "By height" with Low, Medium, and High.
	- A graph with one series doesn't have a Detailed heading.
	- The right column is Background, with Border, Top, and Bottom.
	- Both columns start level with what the mode shows first.

- A color row is a label, a chip that opens the picker, and the hex value, then the revert arrow.
	- The revert arrow is right after the hex value, rather than at the column's edge, so it's plain which color it resets. The mode's revert arrow is after its last choice.
	- A color is labeled with the name of the series, as the hover tip shows it.
	- Low, Medium, High, Border, Top, and Bottom mean the same on every graph.

- A graph has a lead color, the one it reads as from across the room. Any 2 leads are at least 10 apart by CIEDE2000, except in the one-color themes and the one Default pair that design.md names.

- A theme is picked in the Name dropdown. When a color is changed after that, the name gets a "*" after it.
	- The theme buttons are Save, Save as..., Rename..., Delete, and Randomize, in that order.
	- Outline has the settings that all graphs share. Those are the margins, transparency, and the 2 checkboxes that put one border or one background over every graph's own.

## Buttons

- A button's text is in sentence case, one or two words.

- A button ends in "..." when it asks for more before it acts, such as Save as... and Rename.... A button that only asks for a confirmation, like Delete, doesn't get one.

- Every button with text is at least 12 px from its text to the frame across and 8 px over and under, so all buttons are one height.

- The bottom strip has Reset history on the left, under the pages. Cancel, Apply, and OK are on the right, with Cancel set a little apart from the other two.

- OK saves and closes. Apply saves and stays. Cancel discards and closes.
	- Enter is OK and Esc is Cancel, even from inside a field.
	- Apply turns on as soon as a box is typed in.
	- Apply has the accelerator Alt+A. The other two don't have one.

- A button has a tip that says what it does in one sentence.

## Message boxes

- A question box's title is the action, such as "Delete theme" or "Reset history". Its text asks the question, naming the thing.

- A question that can't be undone has No as its default button.

- A failure box's title says what didn't happen, as "Settings weren't saved" or "History wasn't reset". The text gives the reason. If the failure left everything as it was, the last line says so.

- A box that asks for text has the action as its title and "Name:" as its label.

## Keyboard

- Tab goes through the controls in the order they're drawn, including after a graph or a section is moved.

- Ctrl+Tab and Ctrl+Shift+Tab, and Ctrl+PgDn and Ctrl+PgUp, step through the pages. Themes itself isn't a stop, since it only shows Colors.

- In the graph list, Up and Down on a graph's grip move it.

## Wording

- Text is ASCII. The ellipsis is three dots.

- Text uses contractions where a person would use them, digits for numbers, and the Oxford comma.

- Say what a thing does rather than how it's built. "Panel", "graph", "item", and "history" are the terms, and each means one thing.
