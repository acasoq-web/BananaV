# OpenFX headers

The C headers of [OpenFX](https://github.com/AcademySoftwareFoundation/openfx)
1.5 (BSD-3-Clause, see `LICENSE.md`), from commit
`c3f1c2dcf4f51762b794c4fc21181b4a501be1bf`, unchanged.

BananaV does not compile them: `crates/ofx-sys` declares the same
structures in Rust, and `tools/ofx_names.py` writes every name they
define into `crates/ofx-sys/src/names.rs`. They stay here as the
reference the Rust code follows, line for line.
