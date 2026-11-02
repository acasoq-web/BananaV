<!--
Thank you for contributing! Please read AGENTS.md and CONTRIBUTING.md first.
One subject per pull request; for a large change, open an issue first.
-->

## What and why

<!-- What this pull request changes, and why. Link the issue it closes: "Closes #123". -->

## How it was tested

<!-- The tests you added or ran, and what you checked by hand (captures of the interface are welcome). -->

## Checklist

- [ ] `cargo xtask ci` is green on my computer (rustfmt, layers, clippy, tests, wasm).
- [ ] Tests cover the change (a bug fix has a test that failed before it).
- [ ] Every new or changed interface text goes through `tr`/`trf!`/`trn!`/`trc` and has its pt-BR entry.
- [ ] No asset without its `.attribution` sidecar and its line in `ATTRIBUTION.md` (or this pull request adds no assets).
- [ ] Nothing taken from proprietary programs: no code, assets, screenshots or reference material of theirs (AGENTS.md §2 and §3).
- [ ] New dependencies, if any, pass `cargo deny check licenses`.
- [ ] Every commit is signed off (`Signed-off-by:`, `git commit -s`) by the person responsible for it; AI agents never sign off.
