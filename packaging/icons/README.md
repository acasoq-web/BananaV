# BananaV's icon and logo

The official logo (the banana and the V), in three versions:

| File | What it is |
|---|---|
| `bananav-icon.svg` | The application icon: the logo on a dark rounded square. |
| `bananav-logo.svg` | The logo on its dark square (square corners). |
| `bananav-logo-transparent.svg` | The logo alone, on a transparent background. The interface draws it (toolbar, welcome screen, Explorer): `crates/ui-egui/src/icons/logo.rs` has its paths. |
| `bananav.png` | `bananav-icon.svg` at 256 × 256 pixels, corners transparent. The AppImage (`.github/workflows/package.yml`) and the main window (`window_icon` in `crates/ui-egui/src/lib.rs`) use it. |

`bananav.png` is the icon's SVG rendered at 256 px (for example by a browser,
on a transparent background): render it again if the icon changes. If the logo
itself changes, update the paths in `icons/logo.rs` as well.
