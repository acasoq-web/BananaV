# BananaV's website

BananaV's public page: one static page (`index.html`, `assets/style.css`,
`assets/main.js`), no build step, served by GitHub Pages from this folder
at https://bananav-editor.github.io/ — the repository
`bananav-editor/bananav-editor.github.io` (the BananaV organization's site)
holds a copy of this folder, without this README and the `.attribution`
sidecars. `.github/workflows/pages.yml` can publish it from here instead. Dark by default, with a light
theme. Written in English; the language menu translates it (Brazilian
Portuguese today) from `assets/i18n.js`, a table of each English text and its
translation: a text changed in `index.html` needs its entry changed there too.
`?lang=pt-BR` in the address opens it in Portuguese; otherwise the visitor's
last choice, then the browser's language, picks.

- Preview it locally: `python -m http.server 8765 --directory site`, then open
  http://localhost:8765.
- Screenshots (`assets/img/screen-*.png`) come from the screen tests
  (`cargo test -p bv-ui-egui --test telas_inicio -- --ignored`, and `--test
  telas`), cropped; each has its `.attribution` sidecar and a line in
  ATTRIBUTION.md. They show only BananaV and media it generates.
- Donations: the Donate links go to the donate band (`#support`), whose
  buttons are Ko-fi, for donations from any country
  (https://ko-fi.com/bananav), and PIX, for donations from Brazil
  (https://pixgg.com/Acaso). `.github/FUNDING.yml`, the README's badges and
  the welcome screen's Donate tab say the same: keep them in step.
- The page links the public repository's files: its LICENSE,
  CODE_OF_CONDUCT.md, CONTRIBUTING.md and CHANGELOG.md, and the README's
  "Building from source". The downloads point to the latest release's
  packages by name: change them with each release.
- `404.html` uses absolute paths from the site's root, and `index.html`
  names its address for link previews (`og:url`, `og:image`, `canonical`):
  change them if the site moves to its own domain.
- No third-party logos (GitHub, Discord, operating systems): the icons are
  drawn inline as generic shapes (AGENTS.md, assets).
