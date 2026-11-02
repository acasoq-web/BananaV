# lensfun's lens database

BananaV's Lens Correction (`crates/effects/src/lens_correction.rs`) offers
lenses whose distortion it corrects:
`crates/effects/src/lens_correction/lenses.rs`, an adaptation of the lens
database of the lensfun project (<https://lensfun.github.io/>,
<https://github.com/lensfun/lensfun>), version 0.3.95 (its `data/db`
folder), by the lensfun project and its contributors, licensed under the
Creative Commons Attribution-ShareAlike 3.0 Unported license
(<https://creativecommons.org/licenses/by-sa/3.0/>).

What was changed: of the database's lenses, the 636 Lens Correction offers
were taken, each with the PanoTools coefficients a, b and c of the first
distortion calibration the database lists for it (a one-coefficient
`poly3` calibration's k1 as b, with a and c 0), and written as a Rust table
in the order of their names' bytes. "GoPro Hero3 black & compatibles" is
the database's "GoPro Hero3+ black & compatibles".

As section 4(b) of the 3.0 license allows, this adaptation is licensed
under the Creative Commons Attribution-ShareAlike 4.0 International license
(<https://creativecommons.org/licenses/by-sa/4.0/>), which Creative Commons
declares compatible with the GNU General Public License, version 3, under
which BananaV's code is.

The adaptation is provided as is, without warranties of any kind.
