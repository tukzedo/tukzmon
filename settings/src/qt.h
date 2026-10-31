// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)

#pragma once
#include <cstdint>
#include "rust/cxx.h"

namespace tukzmon {
struct Dialog;
std::int32_t run_dialog(Dialog &d, rust::Str snapshot);
// builds the window off screen and says which rows it greys, for the tests
rust::Vec<rust::String> greyed_rows(Dialog &d);
rust::Vec<std::int32_t> graph_list_places(Dialog &d);
struct TabShown;
rust::Vec<TabShown> tabs_shown(Dialog &d);
rust::Vec<TabShown> tabs_after_move(Dialog &d, rust::Str pick, std::size_t from, std::size_t to);
rust::Vec<rust::String> sections_after_move(Dialog &d, std::size_t from, std::size_t to);
rust::Vec<rust::String> tabs_by_key(Dialog &d, bool back, bool fromPage, bool pageKeys);
rust::Vec<rust::String> section_tops(Dialog &d);
rust::Vec<rust::String> bottom_buttons(Dialog &d);
rust::Vec<bool> apply_while_typing(Dialog &d, bool number);
rust::Vec<rust::String> page_shades(Dialog &d, bool dark);
rust::Vec<rust::String> window_fit(Dialog &d, rust::Str screens);
double empty_box_contrast(bool dark);
} // namespace tukzmon
