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

Everything else is as published. To update it, copy the new release's
`src/`, `Cargo.toml` and `README.md` over these and make the change again
(and match the version eframe uses).
