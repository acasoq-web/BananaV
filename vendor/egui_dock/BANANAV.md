# egui_dock 0.21.1, with BananaV's changes

A copy of [egui_dock](https://github.com/anhosh/egui_dock) 0.21.1 (MIT,
see `LICENSE`), used through `[patch.crates-io]` in the workspace's
`Cargo.toml`. BananaV changed:

- `TabBarStyle::at_bottom`: the tab bars below the tabs' contents (VEGAS's
  docked windows have their tabs at the bottom unless Options › Preferences
  › Display › "Position tabs at top of docked windows" is checked).
- `TabStyle::{active_strong, active_line, close_on_active_only}`: the open
  tab's title in bold with a line on its contents' side, and a close button
  on it only, as VEGAS's docked windows show their tabs.
- `TabBarStyle::shrink_titles_on_overflow`: when a window's tabs do not
  fit in its tab bar, the longest titles are shortened with "…" (the whole
  title in a tooltip) so every tab stays visible, instead of the bar
  scrolling under a scroll bar.
- `DockArea::floating_tabs_dock`: whether a tab dragged out of a floating
  window may dock into another place; when not, the window only moves
  (Options › Preferences › Display › "Allow floating windows to dock (hold
  Ctrl to override)").
- `TabViewer::fixed`: a window fixed in its place, as VEGAS's timeline:
  its pane has no tab bar (nothing to drag it by or close it with) and a
  tab dropped on it docks around it, never in its pane
  (`HoverData::fixed`: no Append, the middle splits too).
- A floating window dragged by its only tab moves with the pointer from
  the first move, as an OS window by its title (`State::window_move`, set
  in `tabs`, placed in `show_window_surface` before it is drawn): no
  outline of where it would go, and let go it stays where it is unless it
  docks (what lies under it is where it would dock: its own leaves give no
  hover). egui_dock waited for egui's drag (six points) and its own (30
  across), drew an outline and moved the window there when let go.

- `TabViewer::{leaf_buttons_width, leaf_buttons}`: room at the right end
  of every tab bar for the viewer's own buttons (VEGAS's docked windows'
  Maximize and Restore), drawn for the window's tabs.

Everything else is as published. To update it, copy the new release over
`src/` and make these changes again.
