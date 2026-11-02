# egui-winit 0.36.2, with BananaV's change

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

Everything else is as published. To update it, copy the new release's
`src/`, `Cargo.toml` and `README.md` over these and make the change again
(and match the version eframe uses).
