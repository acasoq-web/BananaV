# egui-winit 0.36.2, with BananaV's changes

A copy of [egui-winit](https://github.com/emilk/egui/tree/main/crates/egui-winit)
0.36.2 (MIT or Apache-2.0, see `LICENSE-MIT`), used through
`[patch.crates-io]` in the workspace's `Cargo.toml`. BananaV changed:

- `State::on_window_event`: `WindowEvent::Moved` no longer asks for a
  repaint. A window moved shows the same picture, and the next frame reads
  where it is (`update_viewport_info`). BananaV's dialogs are immediate
  viewports, which repaint by repainting their parent: each step of a
  dialog's drag drew the whole application again (the main window and the
  dialog, each waiting for the screen), so dialogs followed the pointer
  late.

- `State::apply_cursor`: a cursor made from a bitmap
  (`PlatformOutput::cursor_image`) is kept for every window of the event
  loop, by its bitmap's address, and set only when it changes. A window
  without the event loop at hand (an immediate viewport: BananaV's
  dialogs) uses one made before, or asks for it: the next frame with the
  event loop makes it, and the window shows the icon until then. Upstream
  dropped those windows' bitmaps, so BananaV's own pointers never showed
  in its dialogs (Pan/Crop, Track Motion…).

- `State::on_mouse_button_input`: a button's release is never dropped.
  It needs a position, and upstream had none once the pointer left the
  window. macOS tells a window the pointer left even while a button
  drags in it, so a drag let go just outside the window lost its release:
  egui went on dragging what was pressed with no button down (the Pan/Crop
  needle following the pointer, a mask drag going on while the needle
  moved, a keyframe at every step). A release goes where the pointer was
  last seen (`last_pointer_pos_in_points`, kept when it leaves).

Everything else is as published. To update it: the changes as a patch are
`diff -ru <the published 0.36.2's src> vendor/egui-winit/src` (one file,
`lib.rs`) — copy the new release's `src/`, `Cargo.toml` and `README.md`
over these and apply that patch (and match the version eframe uses).
