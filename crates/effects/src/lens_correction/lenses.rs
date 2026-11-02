//! Lens Correction's lenses: an adaptation of the lensfun project's lens
//! database (version 0.3.95, <https://lensfun.github.io/>, CC BY-SA 3.0),
//! licensed as CC BY-SA 4.0 — the authors and the changes are in
//! `crates/effects/licenses/lensfun.md`. Each lens: its name and the
//! PanoTools coefficients a, b and c of the first distortion calibration
//! the database lists for it (a one-coefficient calibration's k1 as b), in
//! the order of their names' bytes.

/// The lenses: a name and its coefficients a, b and c.
pub(super) const LENSES: &[(&str, [f64; 3])] = &[
    ("1 Nikkor 10mm f/2.8", [0.06176, -0.15886, 0.10734]),
    ("1 Nikkor 32mm f/1.2", [-0.002, 0.004, -0.003]),
    ("1 Nikkor AW 10mm f/2.8", [0.0120353, -0.035894, 0.0124636]),
    (
        "1 Nikkor AW 11-27.5mm f/3.5-5.6",
        [0.0108, -0.04366, 0.01672],
    ),
    (
        "1 Nikkor VR 10-30mm f/3.5-5.6",
        [0.00625, -0.02807, 0.01012],
    ),
    ("28.0mm f/1.7", [0.00098, -0.043, 0.01683]),
    (
        "30mm f/1.4 DC DN | Contemporary 016",
        [-0.00399, -0.00701, -0.00069],
    ),
    ("AEE SD19 & compatibles", [0.05109, -0.12092, 0.10955]),
    (
        "AF 18-200mm f/3.5-6.3 Di II VC",
        [0.0298292, -0.0940382, 0.0506931],
    ),
    ("Auto Petri 1:2.8 f=28mm", [0.00434, -0.0172, 0.00123]),
    (
        "C2040Z & compatibles (Standard)",
        [0.018282, -0.052125, 0.0],
    ),
    (
        "C2040Z & compatibles, with WCON-08B",
        [0.027465, -0.07362, 0.0],
    ),
    (
        "C4000Z & compatibles (Standard)",
        [0.021642, -0.049919, 0.0],
    ),
    (
        "C4000Z & compatibles, with A-28 iS/L converter",
        [0.033, -0.072996, 0.0],
    ),
    (
        "C5060WZ & compatibles (Standard)",
        [0.0, -0.004736, -0.025632],
    ),
    (
        "C7000Z & compatibles (Standard)",
        [0.008728, -0.032692, 0.0],
    ),
    ("C700UZ & compatibles (Standard)", [0.02035, -0.054079, 0.0]),
    ("C750UZ & compatibles (Standard)", [0.014396, -0.03198, 0.0]),
    (
        "C8080WZ & compatibles (Standard)",
        [0.013182, -0.036019, 0.0],
    ),
    ("C860L & compatibles (Standard)", [0.01115, -0.029245, 0.0]),
    (
        "Canon DIGITAL IXUS 400 & compatibles (Standard)",
        [0.009324, -0.03174, 0.0],
    ),
    (
        "Canon DIGITAL IXUS 400 & compatibles, with Tiffen MegaPlus 0.56 converter",
        [0.020376, -0.071126, 0.0],
    ),
    (
        "Canon DIGITAL IXUS 80 IS & compatibles (Standard)",
        [0.01576, -0.05512, 0.01877],
    ),
    (
        "Canon DIGITAL IXUS i & compatibles (Standard)",
        [0.0, -0.007998, -0.009906],
    ),
    (
        "Canon EF 100-400mm f/4.5-5.6L IS USM",
        [0.001543, -0.003547, 0.0],
    ),
    ("Canon EF 100mm f/2.8 Macro", [-0.001558, 0.004225, 0.0]),
    (
        "Canon EF 100mm f/2.8 Macro USM",
        [-0.00126, 0.00429, -0.00887],
    ),
    (
        "Canon EF 100mm f/2.8L Macro IS USM",
        [0.00406, -0.01754, 0.02944],
    ),
    ("Canon EF 135mm f/2.0L USM", [-0.007, 0.025, -0.019]),
    ("Canon EF 135mm f/2.8 Soft Focus", [0.001266, 0.001782, 0.0]),
    ("Canon EF 14mm f/2.8L II USM", [0.00829, -0.02079, 0.00394]),
    (
        "Canon EF 16-35mm f/2.8L II USM",
        [0.0065, -0.01669, -0.01549],
    ),
    (
        "Canon EF 16-35mm f/2.8L III USM",
        [0.0104208, -0.0275753, -0.00353756],
    ),
    (
        "Canon EF 16-35mm f/2.8L USM",
        [0.01687424, -0.03208409, -0.028794159],
    ),
    ("Canon EF 16-35mm f/4L IS USM", [0.00938, -0.03862, 0.01807]),
    ("Canon EF 17-35mm f/2.8L USM", [0.020618, -0.051946, 0.0]),
    ("Canon EF 17-40mm f/4L USM", [0.020104, -0.047886, 0.0]),
    ("Canon EF 200mm f/2.8L USM", [0.00082, -0.00169, 0.01111]),
    ("Canon EF 20mm f/2.8 USM", [0.02466, -0.05795, 0.0166]),
    (
        "Canon EF 24-105mm f/4L IS II USM",
        [0.00188, -0.00884, -0.00325],
    ),
    ("Canon EF 24-105mm f/4L IS USM", [0.017263, -0.049244, 0.0]),
    (
        "Canon EF 24-70mm f/2.8L II USM",
        [0.02964, -0.07853, 0.02943],
    ),
    ("Canon EF 24-70mm f/2.8L USM", [0.015298, -0.039413, 0.0]),
    (
        "Canon EF 24-70mm f/4L IS USM",
        [0.01231, -0.02141, -0.02607],
    ),
    (
        "Canon EF 24-85mm f/3.5-4.5 USM",
        [0.031698, -0.089701, 0.043822],
    ),
    ("Canon EF 24mm f/1.4L USM", [0.013378, -0.028367, 0.0]),
    ("Canon EF 24mm f/2.8", [0.017412, -0.039315, 0.0]),
    (
        "Canon EF 28-105mm f/3.5-4.5 II USM",
        [0.013305, -0.03973, 0.0],
    ),
    (
        "Canon EF 28-135mm f/3.5-5.6 IS USM",
        [0.015307, -0.042304, 0.0],
    ),
    (
        "Canon EF 28-300mm f/3.5-5.6L IS USM",
        [0.013512, -0.044163, 0.0],
    ),
    ("Canon EF 28-70mm f/2.8L USM", [-0.001744, 0.000657, 0.0]),
    ("Canon EF 28-80mm f/3.5-5.6 USM IV", [0.0, -0.00843, 0.0]),
    ("Canon EF 28mm f/1.8", [0.001175, -0.006534, 0.0]),
    ("Canon EF 28mm f/1.8 USM", [0.018, -0.048, 0.019]),
    ("Canon EF 28mm f/2.8", [0.003003, -0.011213, 0.0]),
    ("Canon EF 300mm f/4L IS USM", [-0.00271, 0.00945, -0.00645]),
    ("Canon EF 35-105mm f/3.5-4.5", [0.003118, -0.013401, 0.0]),
    ("Canon EF 35-105mm f/4.5-5.6", [-0.00202, 0.00115, -0.00877]),
    ("Canon EF 35-135mm f/4-5.6 USM", [0.001862, -0.007763, 0.0]),
    ("Canon EF 35-80mm f/4-5.6 III", [0.003148, -0.013071, 0.0]),
    ("Canon EF 35mm f/1.4L USM", [0.01105, -0.02944, 0.00633]),
    ("Canon EF 35mm f/2", [0.007768, -0.018685, 0.0]),
    ("Canon EF 35mm f/2 IS USM", [0.0100042, -0.031314, 0.019113]),
    ("Canon EF 400mm f/5.6L USM", [0.00429, -0.00425, -0.00493]),
    (
        "Canon EF 400mm f/5.6L USM + 1.4x extender",
        [-0.00472, 0.01271, -0.00812],
    ),
    ("Canon EF 40mm f/2.8 STM", [0.01087, -0.02951, 0.0162]),
    ("Canon EF 50mm f/1.2L USM", [0.00476, -0.01688, 0.00748]),
    ("Canon EF 50mm f/1.4 USM", [0.00225, -0.01104, 0.00161]),
    ("Canon EF 50mm f/1.8", [-0.00016, -0.003951, 0.0]),
    ("Canon EF 50mm f/1.8 MkII", [0.00163, -0.01449, 0.01568]),
    (
        "Canon EF 50mm f/1.8 STM",
        [0.0061844, -0.0313122, 0.0314815],
    ),
    ("Canon EF 50mm f/2.5 Compact Macro", [-0.001, 0.005, -0.005]),
    ("Canon EF 55-200mm f/4.5-5.6", [-0.002904, 0.003678, 0.0]),
    (
        "Canon EF 70-200mm f/2.8L IS II USM",
        [0.00109, -0.01393, 0.00917],
    ),
    ("Canon EF 70-200mm f/2.8L USM", [0.00685, -0.02857, 0.01482]),
    ("Canon EF 70-200mm f/4L IS USM", [-0.012, 0.037, -0.046]),
    (
        "Canon EF 70-210mm f/3.5-4.5 USM",
        [0.002876, -0.015288, 0.013076],
    ),
    (
        "Canon EF 70-300mm f/4-5.6 IS USM",
        [-0.00012, 0.00556, -0.00398],
    ),
    (
        "Canon EF 70-300mm f/4-5.6L IS USM",
        [-0.00045, -0.00205, -0.00015],
    ),
    (
        "Canon EF 8-15mm f/4L Fisheye USM",
        [0.3872, -0.71595, 0.4026],
    ),
    ("Canon EF 85mm f/1.2L II USM", [0.00109, -0.00481, 0.00232]),
    ("Canon EF 85mm f/1.8 USM", [-0.000989, -0.002335, 0.0]),
    ("Canon EF 90-300mm f/4.5-5.6", [-0.006, 0.022, -0.02]),
    (
        "Canon EF-M 15-45mm f/3.5-6.3 IS STM",
        [0.035139, -0.115139, 0.078461],
    ),
    (
        "Canon EF-M 18-55mm f/3.5-5.6 IS STM",
        [0.0208, -0.06707, 0.02864],
    ),
    (
        "Canon EF-M 55-200mm f/4.5-6.3 IS STM",
        [0.00329, -0.01435, 0.00377],
    ),
    (
        "Canon EF-S 10-18mm f/4.5-5.6 IS STM",
        [0.00949, -0.05893, 0.07285],
    ),
    (
        "Canon EF-S 10-22mm f/3.5-4.5 USM",
        [0.01986, -0.06874, 0.05166],
    ),
    (
        "Canon EF-S 15-85mm f/3.5-5.6 IS USM",
        [0.0184, -0.03224, -0.03581],
    ),
    (
        "Canon EF-S 17-55mm f/2.8 IS USM",
        [0.0160544, -0.0422842, 0.00805163],
    ),
    (
        "Canon EF-S 17-85mm f/4-5.6 IS USM",
        [0.021181, -0.055581, 0.0],
    ),
    (
        "Canon EF-S 18-135mm f/3.5-5.6 IS",
        [0.04464, -0.15487, 0.13016],
    ),
    (
        "Canon EF-S 18-135mm f/3.5-5.6 IS STM",
        [0.01902, -0.05009, -0.0067],
    ),
    (
        "Canon EF-S 18-135mm f/3.5-5.6 IS USM",
        [0.03582, -0.10962, 0.05876],
    ),
    (
        "Canon EF-S 18-200mm f/3.5-5.6 IS",
        [0.02543, -0.07968, 0.03623],
    ),
    ("Canon EF-S 18-55mm f/3.5-5.6", [0.0, 0.003658, -0.04063]),
    (
        "Canon EF-S 18-55mm f/3.5-5.6 III",
        [0.00029, -0.00065, -0.04054],
    ),
    (
        "Canon EF-S 18-55mm f/3.5-5.6 IS",
        [0.00644, -0.01245, -0.0348],
    ),
    (
        "Canon EF-S 18-55mm f/3.5-5.6 IS II",
        [0.02504, -0.06883, 0.01502],
    ),
    (
        "Canon EF-S 18-55mm f/3.5-5.6 IS STM",
        [0.01786, -0.05378, 0.01216],
    ),
    (
        "Canon EF-S 55-250mm f/4-5.6 IS",
        [-0.000303056, -0.00581537, -0.00237143],
    ),
    (
        "Canon EF-S 55-250mm f/4-5.6 IS II",
        [-0.000303056, -0.00581537, -0.00237143],
    ),
    (
        "Canon EF-S 55-250mm f/4-5.6 IS STM",
        [0.00157, -0.01287, 0.00756],
    ),
    (
        "Canon FDn 100mm 1:2.8",
        [-0.000626176, 0.00321735, -0.0016503],
    ),
    ("Canon FDn 135mm 1:2.8", [0.0, 0.0, 0.002]),
    ("Canon FDn 200mm 1:4", [-0.002, 0.013, -0.015]),
    ("Canon FDn 24mm 1:2.8", [0.00827, -0.02833, 0.01235]),
    (
        "Canon FDn 50mm 1:1.4",
        [-0.000919817, 0.00112022, -0.00289404],
    ),
    ("Canon G7 X & compatibles", [0.03872, -0.15563, 0.02544]),
    (
        "Canon IXUS 125 HS & compatibles",
        [0.0414, -0.15308, 0.04158],
    ),
    (
        "Canon IXUS 220 HS & compatibles, with CHDK's DNG",
        [0.02693, -0.08681, -0.06074],
    ),
    (
        "Canon PowerShot A4000 IS & compatibles, with CHDK's DNG",
        [0.01814, -0.07704, -0.04865],
    ),
    (
        "Canon PowerShot A510 & compatibles (Standard)",
        [0.008099, -0.03097, 0.0],
    ),
    (
        "Canon PowerShot A610 & compatibles (Standard)",
        [0.013142, -0.038236, 0.0],
    ),
    (
        "Canon PowerShot A610 & compatibles, with CHDK's DNG",
        [0.006931, -0.02547, 0.006226],
    ),
    (
        "Canon PowerShot A640 & compatibles",
        [0.02127, -0.05684, 0.0111],
    ),
    (
        "Canon PowerShot A640 & compatibles, with WC-DC58N",
        [0.0204767, -0.0650704, -0.0334725],
    ),
    (
        "Canon PowerShot A650 IS & compatibles (Standard)",
        [0.02, -0.06, 0.021],
    ),
    (
        "Canon PowerShot A70 & compatibles (Standard)",
        [0.007133, -0.026098, 0.0],
    ),
    (
        "Canon PowerShot A720 IS & compatibles (Standard)",
        [0.02218, -0.07779, 0.05524],
    ),
    (
        "Canon PowerShot A80 & compatibles (Standard)",
        [0.004902, -0.021392, 0.0],
    ),
    (
        "Canon PowerShot A80 & compatibles, with Tiffen 0.56x converter",
        [0.025867, -0.072267, 0.0],
    ),
    (
        "Canon PowerShot G1 & compatibles (Standard)",
        [0.013759, -0.037638, 0.0],
    ),
    (
        "Canon PowerShot G11 & compatibles (Standard)",
        [0.01606, -0.0393, -0.0167],
    ),
    (
        "Canon PowerShot G15 & compatibles (Standard)",
        [0.022578, -0.081031, 0.011077],
    ),
    (
        "Canon PowerShot G16 & compatibles",
        [0.02662, -0.08597, 0.00168],
    ),
    (
        "Canon PowerShot G1X & compatibles (Standard)",
        [0.02381, -0.08242, -0.01669],
    ),
    (
        "Canon PowerShot G1X Mark II & compatibles",
        [0.05797, -0.1878, -0.00495],
    ),
    (
        "Canon PowerShot G2 & compatibles (Standard)",
        [0.01966, -0.051543, 0.0],
    ),
    (
        "Canon PowerShot G2 & compatibles, with Geobartic 0.5x converter",
        [0.0, -0.030599, -0.037374],
    ),
    (
        "Canon PowerShot G3 & compatibles (Standard)",
        [0.013088, -0.036387, 0.0],
    ),
    (
        "Canon PowerShot G3 & compatibles, with WC-DC58N",
        [0.016762, -0.065439, 0.0],
    ),
    (
        "Canon PowerShot G7 & compatibles (Standard)",
        [0.01524, -0.04187, 0.0],
    ),
    (
        "Canon PowerShot Pro1 & compatibles (Standard)",
        [0.01839, -0.04535, 0.0],
    ),
    (
        "Canon PowerShot Pro90 IS & compatibles (Standard)",
        [0.014317, -0.036991, 0.0],
    ),
    (
        "Canon PowerShot S1 IS & compatibles (Standard)",
        [0.020538, -0.050896, 0.0],
    ),
    (
        "Canon PowerShot S100 & compatibles",
        [0.02214, -0.10656, -0.01315],
    ),
    (
        "Canon PowerShot S110 & compatibles",
        [0.02413, -0.13243, 0.04812],
    ),
    (
        "Canon PowerShot S120 & compatibles",
        [0.03458, -0.14652, -0.00567],
    ),
    (
        "Canon PowerShot S2 IS & compatibles (Standard)",
        [0.019347, -0.049194, 0.0],
    ),
    (
        "Canon PowerShot S30 & compatibles (Standard)",
        [0.007715, -0.026731, 0.0],
    ),
    (
        "Canon PowerShot S5 IS & compatibles (Standard)",
        [0.019347, -0.049194, 0.0],
    ),
    (
        "Canon PowerShot S70 & compatibles (Standard)",
        [0.018497, -0.049501, 0.0],
    ),
    (
        "Canon PowerShot S90 & compatibles (Standard)",
        [0.01801, -0.09667, 0.02041],
    ),
    (
        "Canon PowerShot S95 & compatibles",
        [0.01786, -0.08833, 0.00128],
    ),
    (
        "Canon PowerShot SD200 & compatibles (Standard)",
        [0.009753, -0.025366, 0.0],
    ),
    (
        "Canon PowerShot SD500 & compatibles (Standard)",
        [0.006629, -0.024613, 0.0],
    ),
    ("Canon PowerShot SX10 IS", [0.02144, -0.0514, 0.02812]),
    (
        "Canon PowerShot SX150 IS & compatibles (Standard)",
        [0.00489, -0.0203, 0.00662],
    ),
    (
        "Canon PowerShot SX150 IS & compatibles, with CHDK's DNG",
        [0.02028, -0.08888, -0.01226],
    ),
    (
        "Canon PowerShot SX160 IS & compatibles, with CHDK's DNG",
        [0.0255, -0.1025, -0.00421],
    ),
    (
        "Canon PowerShot SX220 HS & compatibles, with CHDK's DNG",
        [0.02746, -0.1202, 0.01599],
    ),
    (
        "Canon PowerShot SX260 HS & compatibles, with CHDK's DNG",
        [0.06245, -0.20795, 0.06699],
    ),
    (
        "Canon PowerShot SX30 IS & compatibles, with CHDK's DNG",
        [0.08567, -0.28856, 0.18737],
    ),
    (
        "Canon PowerShot SX510 HS & compatibles, with CHDK's DNG",
        [0.09431, -0.30906, 0.16325],
    ),
    (
        "Canon Powershot A495 & compatibles, with CHDK's DNG",
        [0.00718, -0.033603, 0.028263],
    ),
    (
        "Canon Powershot G9 X & compatibles",
        [0.0263809, -0.139829, 0.0743743],
    ),
    ("Canon TS-E 24mm f/3.5L", [0.006127, -0.018983, 0.0]),
    ("Canon TS-E 45mm f/2.8", [0.00211, -0.007388, 0.0]),
    (
        "Caplio GX & compatibles (Standard)",
        [0.010201, -0.035572, 0.0],
    ),
    (
        "Caplio GX & compatibles, with DW-4",
        [0.016596, -0.050843, 0.0],
    ),
    (
        "Carl Zeiss Distagon T* 2,8/21 ZE",
        [0.03083, -0.0633, 0.01093],
    ),
    (
        "Carl Zeiss Distagon T* 2,8/21 ZF.2",
        [0.03083, -0.0633, 0.01093],
    ),
    (
        "Carl Zeiss Distagon T* 2/35 ZF.2",
        [0.00786, -0.02862, 0.01911],
    ),
    (
        "Carl Zeiss Vario-Sonnar T* 16-35mm f/2.8 ZA SSM II (SAL1635Z2)",
        [0.02199, -0.0563, 0.01126],
    ),
    (
        "Carl Zeiss Vario-Sonnar T* 24-70mm f/2.8 ZA SSM II (SAL2470Z2)",
        [0.03099, -0.085, 0.04233],
    ),
    ("Coolpix A & compatibles", [0.00611, -0.01026, -0.01844]),
    (
        "Coolpix P330 & compatibles (Standard)",
        [0.0176, -0.07831, -0.03885],
    ),
    (
        "Coolpix P60 & compatibles (Standard)",
        [0.01144, -0.05486, 0.02864],
    ),
    ("Coolpix P7000 & compatibles", [0.01246, -0.02848, -0.02244]),
    ("Coolpix P7800 & compatibles", [-0.00644, -0.01027, 0.01753]),
    (
        "Cosina 19-35mm f/3.5-4.5 MC",
        [0.046286, -0.11248, 0.030287],
    ),
    ("D-Xenon 1:3.5-5.6 18-55mm AL", [0.004593, -0.021008, 0.0]),
    (
        "DMC-FX7 & compatibles (Standard)",
        [0.004918, -0.016726, 0.0],
    ),
    (
        "DMC-FZ10 & compatibles (Standard)",
        [0.015495, -0.046197, 0.0],
    ),
    (
        "DMC-FZ200 & compatibles (Standard)",
        [0.02263, -0.08126, -0.02068],
    ),
    (
        "DMC-FZ28 & compatibles (Standard)",
        [0.0151051047088923, -0.0590418104126213, 0.0522404604411257],
    ),
    (
        "DMC-FZ3 & compatibles (Standard)",
        [0.016092, -0.047846, 0.0],
    ),
    (
        "DMC-FZ30 & compatibles (Standard)",
        [0.014174, -0.039301, 0.0],
    ),
    (
        "DMC-FZ45 & compatibles (Standard)",
        [0.00326892, -0.0150012, -0.0488601],
    ),
    (
        "DMC-FZ5 & compatibles (Standard)",
        [0.007805, -0.026346, 0.0],
    ),
    ("DMC-LF1 & compatibles", [0.0290756, -0.105241, -0.00975908]),
    (
        "DMC-LX1 & compatibles (Standard)",
        [0.007252, -0.024439, 0.0],
    ),
    (
        "DMC-LX10 & compatibles (Standard)",
        [0.010942, -0.029589, -0.127751],
    ),
    ("DMC-LX100 & compatibles", [0.02706, -0.09683, -0.02057]),
    (
        "DMC-LX3 & compatibles (Standard)",
        [-0.003959, 0.004736, -0.081302],
    ),
    (
        "DMC-LX5 & compatibles (Standard)",
        [0.03955, -0.16355, 0.11864],
    ),
    (
        "DMC-LX7 & compatibles (Standard)",
        [0.02076, -0.11853, 0.05898],
    ),
    (
        "DMC-LZ2 & compatibles (Standard)",
        [0.018339, -0.048901, 0.0],
    ),
    (
        "DMC-TZ100 & compatibles",
        [0.00708902, -0.040959, -0.0636536],
    ),
    ("DMC-TZ71 & compatibles", [0.10898, -0.3138, 0.1171]),
    (
        "DSC-F707 & compatibles (Standard)",
        [0.013439, -0.038147, 0.0],
    ),
    (
        "DSC-F707 & compatibles, macro at 1 inch lens-to-subject",
        [0.006106, -0.039021, 0.0],
    ),
    (
        "DSC-F707 & compatibles, macro at 12 inches lens-to-subject",
        [0.005441, -0.026284, 0.0],
    ),
    (
        "DSC-F707 & compatibles, macro at 16 inches lens-to-subject",
        [0.003516, -0.019643, 0.0],
    ),
    (
        "DSC-F707 & compatibles, macro at 2 inches lens-to-subject",
        [0.006526, -0.03487, 0.0],
    ),
    (
        "DSC-F707 & compatibles, macro at 20 inches lens-to-subject",
        [0.007797, -0.028559, 0.0],
    ),
    (
        "DSC-F707 & compatibles, macro at 24 inches lens-to-subject",
        [0.010658, -0.033833, 0.0],
    ),
    (
        "DSC-F707 & compatibles, macro at 32 inches lens-to-subject",
        [0.011238, -0.03276, 0.0],
    ),
    (
        "DSC-F707 & compatibles, macro at 4 inches lens-to-subject",
        [0.011414, -0.041732, 0.0],
    ),
    (
        "DSC-F707 & compatibles, macro at 6 inches lens-to-subject",
        [0.010005, -0.036568, 0.0],
    ),
    (
        "DSC-F707 & compatibles, macro at 8 inches lens-to-subject",
        [0.008828, -0.033454, 0.0],
    ),
    (
        "DSC-F707 & compatibles, with Sakar 1858W",
        [-0.002331, 0.001991, -0.087683],
    ),
    (
        "DSC-F707 & compatibles, with VCL-HGD0758 wide angle",
        [0.016774, -0.047703, 0.0],
    ),
    (
        "DSC-F828 & compatibles (Standard)",
        [0.018095, -0.048162, 0.0],
    ),
    (
        "DSC-F828 & compatibles, at 2 feet lens-to-subject",
        [0.031747, -0.075086, 0.0],
    ),
    (
        "DSC-H1 & compatibles (Standard)",
        [0.021497, -0.054037, 0.0],
    ),
    (
        "DSC-H1 & compatibles, with VCL-DH0758 0.7x converter",
        [0.026726, -0.080352, 0.0],
    ),
    ("DSC-R1 & compatibles (Standard)", [0.016048, -0.04251, 0.0]),
    (
        "DSC-RX100 & compatibles (Standard)",
        [0.01545, -0.08481, 0.01241],
    ),
    (
        "DSC-RX100 II & compatibles",
        [0.011046, -0.066845, -0.010623],
    ),
    ("DSC-RX100 III & compatibles", [0.02266, -0.09581, 0.0019]),
    ("DSC-RX1RM2 & compatibles", [0.01397, -0.05582, 0.04619]),
    (
        "DSC-S60 & compatibles (Standard)",
        [0.003091, -0.022634, 0.0],
    ),
    (
        "DSC-S85 & compatibles (Standard)",
        [0.014946, -0.03766, 0.0],
    ),
    (
        "DSC-S85 & compatibles, with VCL-MHG07a converter",
        [0.03951, -0.094976, 0.0],
    ),
    ("DSC-T1 & compatibles (Standard)", [0.0, -0.014873, 0.0]),
    (
        "DSC-V1 & compatibles (Standard)",
        [0.020149, -0.051282, 0.0],
    ),
    (
        "DSC-V1 & compatibles, with VCL-DEH07V converter",
        [0.045731, -0.098626, 0.0],
    ),
    (
        "DSC-W1 & compatibles (Standard)",
        [-0.008279, 0.002007, 0.0],
    ),
    ("DiMAGE 7 & compatibles (Standard)", [0.025, -0.065, 0.0]),
    (
        "DiMAGE 7 & compatibles, with ACW-100 converter",
        [0.0, 0.02892, -0.106517],
    ),
    (
        "DiMAGE G400 & compatibles (Standard)",
        [0.006939, -0.026729, 0.0],
    ),
    (
        "DiMAGE X & compatibles (Standard)",
        [0.006514, -0.029243, 0.0],
    ),
    (
        "DiMAGE Z1 & compatibles (Standard)",
        [0.025087, -0.065158, 0.0],
    ),
    (
        "DiMAGE Z1 & compatibles, with ZCW-100",
        [0.030707, -0.10549, 0.0],
    ),
    (
        "DiMAGE Z10 & compatibles (Standard)",
        [0.012869, -0.041112, 0.0],
    ),
    (
        "DiMAGE Z10 & compatibles, with ZCW-200",
        [0.024186, -0.107425, 0.0],
    ),
    (
        "DiMAGE Z2 & compatibles (Standard)",
        [0.024434, -0.049454, 0.0],
    ),
    (
        "DiMAGE Z2 & compatibles, with ZCW-100",
        [0.020234, -0.07549, 0.0],
    ),
    (
        "DiMAGE Z3 & compatibles (Standard)",
        [0.008857, -0.033348, 0.0],
    ),
    (
        "Digilux 2 & compatibles (Standard)",
        [0.0, 0.009873, -0.057763],
    ),
    ("E 10-18mm f/4 OSS", [0.03262, -0.08127, 0.0223]),
    ("E 16-70mm f/4 ZA OSS", [0.02953, -0.08038, 0.02902]),
    ("E 16mm f/2.8", [0.01701, -0.02563, -0.0052]),
    ("E 18-200mm f/3.5-6.3 OSS", [0.03037, -0.09891, 0.05057]),
    ("E 18-200mm f/3.5-6.3 OSS LE", [0.03823, -0.12894, 0.09134]),
    ("E 18-55mm f/3.5-5.6 OSS", [0.02897, -0.0892, 0.03513]),
    ("E 20mm f/2", [0.0236801, -0.0528733, 0.0242738]),
    ("E 20mm f/2.8", [0.01609, -0.05179, 0.01925]),
    ("E 21mm F2.8", [0.02059, -0.05856, 0.0302]),
    ("E 24mm f/1.8 ZA", [0.00641, -0.0083, 0.00228]),
    ("E 25mm f/2", [0.01403, -0.03257, 0.00722]),
    ("E 30mm f/3.5 Macro", [0.00105, -0.00352, 0.00057]),
    ("E 35mm f/1.8 OSS", [-0.01085, 0.03631, -0.04468]),
    ("E 50mm F2", [-0.00093, -0.00216, -0.0024]),
    ("E 55-210mm f/4.5-6.3 OSS", [0.00219, 0.00538, -0.01084]),
    ("E 85mm F1.8", [-0.000588, -0.00021, 0.024475]),
    ("E PZ 16-50mm f/3.5-5.6 OSS", [0.0219, -0.10642, 0.06233]),
    ("E PZ 18-105mm f/4 G OSS", [0.058, -0.14747, 0.10212]),
    ("E4800 & compatibles (Standard)", [0.010112, -0.033866, 0.0]),
    (
        "E5000 & compatibles (Standard)",
        [-0.009406, 0.022592, -0.044771],
    ),
    (
        "E5000 & compatibles, with WC-E68 (full wide)",
        [0.026854, -0.077694, 0.0],
    ),
    ("E5400 & compatibles (Standard)", [0.0, 0.009612, -0.058281]),
    (
        "E5400 & compatibles, with WC-E80",
        [0.02609, -0.086283, 0.0],
    ),
    ("E5700 & compatibles (Standard)", [0.014425, -0.044782, 0.0]),
    (
        "E5700 & compatibles, with TC-E15ED (full tele)",
        [-0.000659, 0.004458, 0.0],
    ),
    (
        "E5700 & compatibles, with WC-E80 (full wide)",
        [0.016692, -0.060767, 0.0],
    ),
    ("E7900 & compatibles (Standard)", [0.006962, -0.036546, 0.0]),
    ("E8400 & compatibles, with WC-E75", [0.0165, -0.057649, 0.0]),
    ("E8800 & compatibles (Standard)", [0.013454, -0.041694, 0.0]),
    (
        "E8800 & compatibles, with WM-E80",
        [0.015468, -0.07614, 0.0],
    ),
    ("E990 & compatibles (Standard)", [0.009323, -0.036014, 0.0]),
    (
        "E990 & compatibles, with WC-E63",
        [0.027148, -0.073181, 0.0],
    ),
    ("E995 & compatibles, with WC-E63", [0.017724, -0.0578, 0.0]),
    ("EF-S18-55mm f/4-5.6 IS STM", [0.02902, -0.09036, 0.05046]),
    (
        "EX-P600 & compatibles (Standard)",
        [0.020742, -0.051562, 0.0],
    ),
    (
        "EX-P600 & compatibles, with WC-DC58A",
        [0.020587, -0.07417, 0.0],
    ),
    ("EX-Z4 & compatibles (Standard)", [0.016943, -0.048353, 0.0]),
    ("Elmarit-M 1:2.8/28", [0.00349, -0.00508, -0.01721]),
    ("Elmarit-M 1:2.8/90", [-0.00423, 0.01406, -0.00692]),
    (
        "FC6310 & compatibles",
        [0.00961599, -0.0582928, -0.00551691],
    ),
    ("FE 16-35mm f/4 ZA OSS", [0.00561, -0.00446, -0.04348]),
    ("FE 21mm f/2.8", [0.01519, -0.07683, 0.00078]),
    ("FE 24-240mm f/3.5-6.3 OSS", [0.03845, -0.13772, 0.07834]),
    ("FE 24-70mm f/2.8 GM", [0.02824, -0.07818, 0.03129]),
    ("FE 24-70mm f/4 ZA OSS", [0.01086, -0.05129, 0.0454]),
    (
        "FE 28-70mm f/3.5-5.6 OSS",
        [0.00900771, -0.0310341, 0.00110802],
    ),
    ("FE 55mm f/1.8 ZA", [-0.01142, 0.02682, -0.01628]),
    ("FE 70-200mm f/4 G OSS", [-0.00998, 0.04602, -0.08171]),
    ("FE 85mm f/1.8", [-0.011, 0.029, -0.022]),
    ("FZ1000 & compatibles", [0.01858, -0.05809, -0.06581]),
    ("FZ150 & compatibles", [0.0109406, -0.042017, -0.0287289]),
    (
        "FinePix 2800 ZOOM & compatibles (Standard)",
        [0.0, 0.0, -0.035],
    ),
    (
        "FinePix F200EXR & compatibles (Standard)",
        [0.01564, -0.04677, 0.01961],
    ),
    (
        "FinePix F770EXR & compatibles (Standard)",
        [0.01272, -0.05766, -0.00271],
    ),
    (
        "FinePix F810 & compatibles (Standard)",
        [0.013263, -0.038383, 0.0],
    ),
    (
        "FinePix F810 & compatibles, with WL-FXE01 (full wide)",
        [0.023137, -0.073048, 0.0],
    ),
    (
        "FinePix HS20EXR & compatibles (Standard)",
        [0.01557, -0.05066, -0.04858],
    ),
    (
        "FinePix S5500 & compatibles (Standard)",
        [0.011881, -0.03472, 0.0],
    ),
    (
        "FinePix S602 ZOOM & compatibles (Standard)",
        [0.00554, -0.018984, 0.0],
    ),
    (
        "FinePix S9000 & compatibles (Standard)",
        [0.022449, -0.05488, 0.0],
    ),
    ("Fotasy M3517 35mm f/1.7", [0.02256, -0.05856, 0.07779]),
    ("Fujifilm FinePix A370", [0.01077, -0.03642, 0.0168]),
    ("Fujifilm X-S1", [0.04173, -0.1255, 0.03337]),
    ("Fujifilm XQ1", [0.01267, -0.07641, 0.00242]),
    (
        "GR Digital & compatibles (Standard)",
        [0.008807, -0.022132, 0.0],
    ),
    (
        "GoPro Hero3 black & compatibles",
        [0.01049, 0.01663, -0.40901],
    ),
    (
        "HD Pentax-D FA 15-30mm f/2.8 ED SDM WR",
        [0.00365, -0.0373, 0.04147],
    ),
    (
        "HD Pentax-D FA 24-70mm f/2.8 ED SDM WR",
        [0.03519, -0.10395, 0.06092],
    ),
    (
        "HD Pentax-D FA 28-105mm f/3.5-5.6 ED DC WR",
        [0.0165845, -0.0455463, -0.000358805],
    ),
    (
        "HD Pentax-DA 16-85mm f/3.5-5.6 ED DC WR",
        [0.0128194, -0.0316035, -0.0188219],
    ),
    (
        "HD Pentax-DA 18-50mm f/4-5.6 DC WR RE",
        [0.009, -0.038, 0.012],
    ),
    (
        "HD Pentax-DA 20-40mm f/2.8-4 ED Limited DC WR",
        [0.00595, -0.02518, 0.00497],
    ),
    (
        "HD Pentax-DA 55-300mm f/4-5.8 ED WR",
        [0.00917, -0.03664, 0.02573],
    ),
    ("HD2 & compatibles", [-0.07664, 0.21934, -0.10671]),
    ("HERO4", [0.03183, -0.11592, 0.0]),
    ("Irix 15mm f/2.4", [-0.00136, -0.00782, 0.00534]),
    ("KM 20mm f/2.8", [0.005548, -0.022552, 0.0]),
    ("KM 24-105mm f/3.5-4.5 AF D", [0.004504, -0.01756, 0.0]),
    ("KM 28-100mm f/3.5-5.6 AF D", [0.005155, -0.020044, 0.0]),
    ("Kodak CX6330 & compatibles", [0.02078, -0.06962, 0.04229]),
    (
        "Leica D Vario-Elmar 14-150mm f/3.5-5.6 Asph. OIS",
        [0.01426, -0.01566, -0.05658],
    ),
    (
        "Leica DG Summilux 15mm f/1.7 Asph.",
        [0.01153, -0.05273, -0.00197],
    ),
    (
        "Leica DG Summilux 25mm f/1.4 Asph.",
        [-0.00305, -0.01088, -0.00658],
    ),
    ("Lumia 1020", [-0.01804, 0.06109, -0.05264]),
    ("Lumia 950", [-0.01625, 0.04667, -0.03155]),
    (
        "Lumix G 14mm f/2.5 Asph.",
        [0.0115556, -0.0589829, 0.000154452],
    ),
    (
        "Lumix G 14mm f/2.5 Asph. + GWC1 0.79x",
        [0.000389003, -0.0125969, -0.00230827],
    ),
    ("Lumix G 14mm f/2.5 II", [0.00401, -0.02455, -0.04437]),
    (
        "Lumix G 20mm f/1.7 Asph.",
        [0.00670602, -0.0381172, 0.0065352],
    ),
    ("Lumix G 20mm f/1.7 II Asph.", [0.01169, -0.0533, 0.02618]),
    ("Lumix G 25mm f/1.7", [0.006901, -0.043711, 0.032678]),
    ("Lumix G 42.5mm f/1.7", [0.00306, -0.02178, 0.01278]),
    (
        "Lumix G Vario 100-300mm f/4.0-5.6 Mega OIS",
        [-0.000115873, 0.00367061, 0.000486072],
    ),
    (
        "Lumix G Vario 12-32mm f/3.5-5.6 Asph. Mega OIS",
        [0.02222, -0.06354, -0.05077],
    ),
    (
        "Lumix G Vario 12-60mm f/3.5-5.6 Asph. Power OIS",
        [0.0287212, -0.0853854, -0.0106763],
    ),
    (
        "Lumix G Vario 14-140mm f/3.5-5.6",
        [0.02559, -0.09222, 0.02687],
    ),
    (
        "Lumix G Vario 14-42mm f/3.5-5.6 II Asph. Mega OIS",
        [0.0225278, -0.0802366, 0.00964907],
    ),
    (
        "Lumix G Vario 35-100mm f/4.0-5.6 Asph. Mega OIS",
        [0.00663258, -0.0185251, 0.0134508],
    ),
    (
        "Lumix G Vario 45-150mm f/4.0-5.6 Asph. Mega OIS",
        [-0.01139, 0.04017, -0.04716],
    ),
    ("Lumix G Vario 45-200mm f/4.0-5.6 Mega OIS", [0.0, 0.0, 0.0]),
    (
        "Lumix G Vario 7-14mm f/4.0 Asph.",
        [0.02686, -0.10328, 0.05484],
    ),
    (
        "Lumix G Vario HD 14-140mm f/4.0-5.8 Asph. Mega OIS",
        [0.00966207, -0.0543377, -0.00740716],
    ),
    (
        "Lumix G X Vario 12-35mm f/2.8 Asph. Power OIS",
        [0.03626, -0.10638, 0.01966],
    ),
    (
        "Lumix G X Vario PZ 14-42mm f/3.5-5.6 Asph. Power OIS",
        [0.02565, -0.08842, 0.01461],
    ),
    (
        "Lumix G X Vario PZ 14-42mm f/3.5-5.6 Asph. Power OIS + GWC1 0.79x",
        [0.0179152, -0.045476, -0.00473275],
    ),
    (
        "Lumix G X Vario PZ 45-175mm f/4.0-5.6 Asph. Power OIS",
        [-0.00276, 0.01275, -0.01961],
    ),
    (
        "MC Soligor C/D Wide-Auto 1:2.8 f=24mm",
        [0.00094, -0.00031, -0.02187],
    ),
    ("MC Volna-3 80mm f/2.8", [0.0, 0.00056, 0.0]),
    ("MIR-1B 37mm f/2.8", [0.0, -0.00255, 0.0]),
    ("Mamiya 35mm f/3.5", [0.025471, -0.05106, 0.0]),
    ("Mamiya 55-110mm f/4.5", [-0.001633, -0.002243, 0.0]),
    ("Mamiya 80mm f/2.8", [-0.008173, 0.015478, 0.0]),
    ("Mavic Pro FC220 & compatibles", [0.0, -0.014, 0.033]),
    ("Meike 35mm f/1.7", [-0.00688, 0.00111, -0.00744]),
    (
        "Minolta AF 100-300mm f/4.5-5.6 APO (D)",
        [0.01066, -0.02087, 0.01828],
    ),
    (
        "Minolta MC Rokkor-PG 50mm 1:1.4",
        [-0.000767655, -0.000599186, -0.00129564],
    ),
    ("Minolta MD 35mm 1/2.8", [0.01424, -0.05599, 0.06278]),
    ("Minolta/Sony AF 100mm F2.8 Macro (D)", [0.0, 0.00185, 0.0]),
    (
        "Minolta/Sony AF 500mm F8 Reflex",
        [0.00847, -0.02444, 0.03796],
    ),
    (
        "Minolta/Sony AF DT 18-70mm f/3.5-5.6 (D)",
        [0.02852, -0.07655, 0.0228],
    ),
    ("Miranda 28mm f/2.8 MC", [0.01614, -0.05197, 0.02005]),
    ("Mitakon wide MC f=24mm 1:2.8", [0.00966, -0.0318, 0.01539]),
    (
        "Nikon AF DC-Nikkor 105mm f/2D",
        [-0.00457, 0.01891, -0.02932],
    ),
    ("Nikon AF DC-Nikkor 135mm f/2D", [0.00165, 0.0009, -0.01172]),
    (
        "Nikon AF DX Fisheye-Nikkor 10.5mm f/2.8G ED",
        [0.03347, -0.0964, 0.08113],
    ),
    ("Nikon AF Nikkor 14mm f/2.8D ED", [0.013209, -0.036529, 0.0]),
    ("Nikon AF Nikkor 20mm f/2.8D", [0.0063, -0.021, 0.0]),
    ("Nikon AF Nikkor 24mm f/2.8D", [0.00112, -0.00753, -0.00379]),
    (
        "Nikon AF Nikkor 28mm f/1.4D",
        [0.00913976, -0.0245588, 0.000542579],
    ),
    ("Nikon AF Nikkor 28mm f/2.8D", [0.0, 0.025773, -0.085777]),
    (
        "Nikon AF Nikkor 35-70mm f/2.8",
        [0.01027, -0.03727, 0.02147],
    ),
    ("Nikon AF Nikkor 35mm f/2.0D", [0.001968, -0.009475, 0.0]),
    ("Nikon AF Nikkor 50mm f/1.8D", [-0.00255, 0.00841, -0.01107]),
    (
        "Nikon AF Zoom-Nikkor 18-35mm f/3.5-4.5D IF-ED",
        [0.005981, -0.024888, 0.0],
    ),
    (
        "Nikon AF Zoom-Nikkor 20-35mm f/2.8D IF",
        [0.01479, -0.03259, 0.0026],
    ),
    (
        "Nikon AF Zoom-Nikkor 24-85mm f/2.8-4D IF",
        [0.07, -0.19609, 0.12989],
    ),
    (
        "Nikon AF Zoom-Nikkor 28-200mm f/3.5-5.6G IF-ED",
        [0.0, -0.005311, 0.0],
    ),
    (
        "Nikon AF Zoom-Nikkor 28-70mm f/3.5-4.5D",
        [0.01986, -0.06207, 0.02114],
    ),
    (
        "Nikon AF Zoom-Nikkor 28-80mm f/3.3-5.6G",
        [0.0075, -0.02022, -0.02162],
    ),
    (
        "Nikon AF Zoom-Nikkor 70-210mm f/4",
        [-0.002443, 0.001936, 0.0],
    ),
    (
        "Nikon AF-P DX Nikkor 10-20mm f/4.5-5.6 VR 168",
        [0.01265, -0.08749, 0.1144],
    ),
    (
        "Nikon AF-P DX Nikkor 18-55mm f/3.5-5.6G VR",
        [0.01368, -0.06635, 0.05634],
    ),
    (
        "Nikon AF-P DX Nikkor 70-300mm f/4.5-6.3G ED VR",
        [0.00027, -0.00374, -0.00068],
    ),
    (
        "Nikon AF-S DX Nikkor 10-24mm f/3.5-4.5G ED",
        [0.0054, -0.04682, 0.05708],
    ),
    (
        "Nikon AF-S DX Nikkor 16-80mm f/2.8-4E ED VR",
        [0.01368, -0.04363, 0.00904],
    ),
    (
        "Nikon AF-S DX Nikkor 18-140mm f/3.5-5.6G ED VR",
        [0.02, -0.06, 0.006],
    ),
    (
        "Nikon AF-S DX Nikkor 18-300mm f/3.5-5.6G ED VR",
        [0.02888, -0.09293, 0.04919],
    ),
    (
        "Nikon AF-S DX Nikkor 18-300mm f/3.5-6.3G ED VR",
        [0.0325, -0.10956, 0.07226],
    ),
    ("Nikon AF-S DX Nikkor 35mm f/1.8G", [0.003, -0.017, 0.005]),
    (
        "Nikon AF-S DX VR Nikkor 18-55mm f/3.5-5.6G II",
        [-0.00033, -0.007745, -0.015154],
    ),
    (
        "Nikon AF-S DX VR Zoom-Nikkor 18-200mm f/3.5-5.6G IF-ED",
        [0.03144, -0.10111, 0.07201],
    ),
    (
        "Nikon AF-S DX VR Zoom-Nikkor 18-200mm f/3.5-5.6G IF-ED II",
        [0.03144, -0.10111, 0.07201],
    ),
    (
        "Nikon AF-S DX Zoom-Nikkor 12-24mm f/4G IF-ED",
        [-0.016006, 0.024041, 0.0],
    ),
    (
        "Nikon AF-S DX Zoom-Nikkor 16-85mm f/3.5-5.6G ED VR",
        [0.0199837, -0.0681903, 0.0388181],
    ),
    (
        "Nikon AF-S DX Zoom-Nikkor 18-105mm f/3.5-5.6G ED VR",
        [0.02437, -0.06486, 0.00154],
    ),
    (
        "Nikon AF-S DX Zoom-Nikkor 18-135mm f/3.5-5.6G IF-ED",
        [0.010516, -0.033415, 0.0],
    ),
    (
        "Nikon AF-S DX Zoom-Nikkor 18-55mm f/3.5-5.6G ED",
        [0.003409, -0.019128, 0.0],
    ),
    (
        "Nikon AF-S DX Zoom-Nikkor 18-55mm f/3.5-5.6G VR",
        [0.000658776, -0.0150048, -0.00123339],
    ),
    (
        "Nikon AF-S DX Zoom-Nikkor 18-70mm f/3.5-4.5G IF-ED",
        [0.0266119, -0.0709405, 0.0182763],
    ),
    (
        "Nikon AF-S Nikkor 20mm f/1.8G ED",
        [0.0108146, -0.0341733, 0.0157005],
    ),
    (
        "Nikon AF-S Nikkor 24-120mm f/4G ED VR",
        [0.01497, -0.04816, 0.01104],
    ),
    (
        "Nikon AF-S Nikkor 24-70mm f/2.8E ED VR",
        [0.00178, -0.01599, -0.00487],
    ),
    (
        "Nikon AF-S Nikkor 24-85 mm f/3.5-4.5G ED VR",
        [0.00771, -0.03274, 0.01406],
    ),
    (
        "Nikon AF-S Nikkor 28mm f/1.8G",
        [0.00961, -0.02915, 0.01024],
    ),
    ("Nikon AF-S Nikkor 300mm f/4D IF-ED", [0.0, 0.00357, 0.0]),
    (
        "Nikon AF-S Nikkor 35mm f/1.8G ED",
        [0.02498, -0.07793, 0.05642],
    ),
    ("Nikon AF-S Nikkor 50mm f/1.4G", [0.0, -0.00466, 0.0]),
    (
        "Nikon AF-S Nikkor 50mm f/1.8G",
        [-0.00299, 0.00708, -0.00949],
    ),
    (
        "Nikon AF-S Nikkor 600mm f/4G ED VR",
        [-0.00089, 0.00493, -0.00382],
    ),
    (
        "Nikon AF-S Nikkor 70-200mm f/2.8G ED VR II",
        [-0.00191, 0.00438, -0.0077],
    ),
    (
        "Nikon AF-S Nikkor 800mm f/5.6E FL ED VR",
        [0.0015, -0.00178, 0.00484],
    ),
    (
        "Nikon AF-S Nikkor 85mm f/1.8G",
        [-0.00079, 0.00341, -0.00179],
    ),
    (
        "Nikon AF-S VR Zoom-Nikkor 24-120mm f/3.5-5.6G IF-ED",
        [0.004393, -0.01801, 0.0],
    ),
    (
        "Nikon AF-S Zoom-Nikkor 16-35mm f/4G ED VR",
        [0.01589, -0.08067, 0.06712],
    ),
    (
        "Nikon AF-S Zoom-Nikkor 17-35mm f/2.8D IF-ED",
        [0.005, -0.02, 0.0],
    ),
    (
        "Nikon AF-S Zoom-Nikkor 24-70mm f/2.8G ED",
        [0.01436, -0.06611, 0.06436],
    ),
    (
        "Nikon AF-S Zoom-Nikkor 24-85mm f/3.5-4.5G IF-ED",
        [0.023939, -0.061462, 0.0],
    ),
    (
        "Nikon AF-S Zoom-Nikkor 28-300mm f/3.5-5.6G ED VR",
        [0.02437, -0.07944, 0.03221],
    ),
    ("Nikon AI Nikkor 15mm f/3.5", [0.007, -0.021, 0.0]),
    (
        "Nikon AI-S Fisheye-Nikkor 6mm f/2.8",
        [0.011705, -0.01815, -0.03936],
    ),
    (
        "Nikon AI-S Nikkor 20mm f/2.8",
        [0.0346145503714303, -0.0771531442478898, 0.0142974496031017],
    ),
    ("Nikon AI-S Nikkor 24mm f/2.8", [0.0202, -0.05926, 0.03781]),
    ("Nikon AI-S Nikkor 28mm f/2.8", [0.00929, -0.02155, 0.0021]),
    (
        "Nikon AI-S Nikkor 28mm f/3.5 PC (unshifted)",
        [0.007377, -0.021769, 0.0],
    ),
    ("Nikon AI-S Nikkor 35mm f/2.0", [0.01447, -0.03949, 0.0153]),
    (
        "Nikon AI-S Nikkor 400mm f/3.5",
        [0.00965, -0.03022, 0.03424],
    ),
    ("Nikon AI-S Nikkor 50mm f/1.2", [0.0042, -0.01715, 0.00585]),
    ("Nikon AI-S Nikkor 50mm f/1.4", [0.0, -0.0025, 0.0]),
    (
        "Nikon Lens Series E 28mm f/2.8",
        [0.01493, -0.05112, 0.02965],
    ),
    ("Nikon Nikkor AI 20mm f/3.5", [0.02362, -0.04833, -0.00643]),
    (
        "Olympus E-10 & compatibles, with B-300 (full tele)",
        [0.001885, 0.002595, 0.0],
    ),
    (
        "Olympus E-10 & compatibles, with WCON-08B",
        [0.015434, -0.044944, 0.0],
    ),
    (
        "Olympus M.Zuiko Digital 14-42mm f/3.5-5.6 II",
        [0.018, -0.074, 0.014],
    ),
    (
        "Olympus M.Zuiko Digital 17mm f/1.8",
        [0.01989, -0.09761, 0.07461],
    ),
    (
        "Olympus M.Zuiko Digital 17mm f/2.8 Pancake",
        [0.00835, -0.04149, -0.0022],
    ),
    (
        "Olympus M.Zuiko Digital 25mm f/1.8",
        [0.00454, -0.0141, 0.00283],
    ),
    (
        "Olympus M.Zuiko Digital 45mm f/1.8",
        [0.00149954, -0.0023693, 0.00382496],
    ),
    (
        "Olympus M.Zuiko Digital ED 12-100mm f/4.0 IS Pro",
        [0.0302425, -0.103301, 0.0149435],
    ),
    (
        "Olympus M.Zuiko Digital ED 12-40mm f/2.8 Pro",
        [0.03659, -0.11322, 0.0294],
    ),
    (
        "Olympus M.Zuiko Digital ED 12-50mm f/3.5-6.3 EZ",
        [0.00331, -0.03343, -0.00683],
    ),
    (
        "Olympus M.Zuiko Digital ED 12mm f/2.0",
        [0.0, -0.028892, 0.0],
    ),
    (
        "Olympus M.Zuiko Digital ED 14-150mm f/4.0-5.6",
        [0.0188691, -0.0734975, 0.0114623],
    ),
    (
        "Olympus M.Zuiko Digital ED 14-42mm f/3.5-5.6 EZ",
        [0.01468, -0.06545, -0.00437],
    ),
    (
        "Olympus M.Zuiko Digital ED 14-42mm f/3.5-5.6 II R",
        [0.018, -0.074, 0.014],
    ),
    (
        "Olympus M.Zuiko Digital ED 14-42mm f/3.5-5.6 L",
        [0.10558, -0.3175, 0.19816],
    ),
    (
        "Olympus M.Zuiko Digital ED 40-150mm f/2.8 Pro",
        [-0.000686648, -0.00940753, -0.00546391],
    ),
    (
        "Olympus M.Zuiko Digital ED 40-150mm f/2.8 Pro + 1.4x converter",
        [-0.00554012, 0.00335391, -0.00497355],
    ),
    (
        "Olympus M.Zuiko Digital ED 7-14mm f/2.8 Pro",
        [0.04439, -0.17864, 0.13663],
    ),
    (
        "Olympus M.Zuiko Digital ED 9-18mm f/4.0-5.6",
        [-0.00243, -0.05248, 0.0576],
    ),
    (
        "Olympus Zuiko Digital 14-45mm f/3.5-5.6",
        [0.015865, -0.045846, 0.0],
    ),
    (
        "Olympus Zuiko Digital 14-54mm f/2.8-3.5",
        [0.019325, -0.040094, 0.0],
    ),
    (
        "Olympus Zuiko Digital 25mm f/2.8",
        [0.0011362, -0.011804, 0.00114701],
    ),
    (
        "Olympus Zuiko Digital 35mm f/3.5 Macro",
        [-0.00226197, 0.00228625, -0.00756547],
    ),
    (
        "Olympus Zuiko Digital ED 12-60mm f/2.8-4.0 SWD",
        [0.0378606, -0.0903769, 0.0209704],
    ),
    (
        "Olympus Zuiko Digital ED 14-35mm F2.0 SWD",
        [0.02286, -0.06336, 0.03264],
    ),
    (
        "Olympus Zuiko Digital ED 14-42mm f/3.5-5.6",
        [0.0100665, -0.0270472, 0.0022372],
    ),
    (
        "Olympus Zuiko Digital ED 40-150mm f/4.0-5.6",
        [-0.0069552, 0.0128954, -0.0201501],
    ),
    (
        "Olympus Zuiko Digital ED 50mm f/2.0 Macro",
        [0.001878, -0.005529, 0.0],
    ),
    (
        "Olympus Zuiko Digital ED 7-14mm f/4.0",
        [-0.013136, 0.015797, 0.0],
    ),
    (
        "Pentax Optio 230GS & compatibles (Standard)",
        [0.021182, -0.054588, 0.0],
    ),
    (
        "Pentax Optio 430 & compatibles (Standard)",
        [0.00125, -0.015475, 0.0],
    ),
    (
        "Pentax Optio 43WR & compatibles (Standard)",
        [0.00051, -0.015327, 0.0],
    ),
    (
        "Pentax Optio 750Z & compatibles (Standard)",
        [0.017912, -0.044145, 0.0],
    ),
    (
        "Pentax-F 28-80mm f/3.5-4.5",
        [0.00103457, -0.0104385, -0.00251646],
    ),
    (
        "Phantom Vision FC200 & compatibles",
        [0.02707, -0.10021, -0.09787],
    ),
    (
        "QV-3500EX & compatibles (Standard)",
        [0.012733, -0.03693, 0.0],
    ),
    (
        "QV-3500EX & compatibles, with Raynox 0.66x",
        [0.0, 0.02562, -0.106136],
    ),
    ("Ricoh GR & compatibles", [0.01134, -0.03066, 0.01059]),
    (
        "Samsung NX 10mm f/3.5 Fisheye",
        [-0.0459, 0.13431, -0.10789],
    ),
    (
        "Samsung NX 16-50mm f/3.5-5.6 PZ ED OIS",
        [0.01687, -0.04326, -0.03389],
    ),
    (
        "Samsung NX 16mm f/2.4 Pancake",
        [0.00013, -0.00589, -0.04838],
    ),
    (
        "Samsung NX 18-55mm f/3.5-5.6 OIS",
        [0.01917, -0.0495, -0.00128],
    ),
    (
        "Samsung NX 20-50mm f/3.5-5.6 ED",
        [0.01347, -0.05952, 0.01561],
    ),
    ("Samsung NX 20mm f/2.8 Pancake", [0.0065, -0.03633, 0.00495]),
    (
        "Samsung S7 wide angle lens cover",
        [0.03904, -0.15646, 0.13025],
    ),
    ("Samyang 12mm f/2.0 NCS CS", [0.0097, -0.04108, 0.03623]),
    (
        "Samyang 12mm f/2.8 Fish-Eye ED AS NCS",
        [-0.000331804, -0.00911417, -0.044237],
    ),
    ("Samyang 135mm f/2 ED UMC", [0.00072, -0.001, 0.00215]),
    (
        "Samyang 14mm f/2.8 AE ED AS IF UMC",
        [0.04015, -0.07043, -0.05466],
    ),
    (
        "Samyang 20mm f/1.8 ED AS UMC",
        [0.00825994, -0.022059, -0.00358632],
    ),
    ("Samyang 35mm f/1.4 AS UMC", [0.00331, -0.0171, 0.00274]),
    (
        "Samyang 7.5mm f/3.5 UMC Fish-eye MFT",
        [-0.0133116, 0.00822383, -0.0180619],
    ),
    (
        "Samyang 85mm f/1.4 IF UMC Aspherical",
        [0.00027, -0.00193, 0.00014],
    ),
    (
        "Samyang 8mm f/2.8 UMC Fish-eye",
        [0.02036, -0.08028, 0.01446],
    ),
    (
        "Samyang 8mm f/3.5 Fish-Eye CS",
        [0.04098, -0.15413, 0.11977],
    ),
    ("Samyang T-S 24mm f/3.5 ED AS UMC", [-0.004, 0.007, -0.02]),
    ("Schneider 28mm Digitar f/2.8", [0.027509, -0.054383, 0.0]),
    ("Schneider 28mm f/2.8 PC", [0.009514, -0.027901, 0.0]),
    (
        "Schneider Retina-Curtagon 1:4/28mm",
        [0.00591, -0.01953, 0.00935],
    ),
    ("Sigma 10-20mm f/3.5 EX DC HSM", [0.02278, -0.0794, 0.05484]),
    (
        "Sigma 10-20mm f/4-5.6 EX DC HSM",
        [-0.0284207, 0.0674528, -0.0484386],
    ),
    (
        "Sigma 100-300mm f/4 APO EX DG HSM",
        [7e-05, -0.0008, -1e-05],
    ),
    (
        "Sigma 100-300mm f/4 APO EX DG HSM + Kenko Teleplus PRO 300 AF 1.4x DGX extender",
        [-0.00597, 0.00974, -0.00848],
    ),
    (
        "Sigma 10mm f/2.8 EX DC Fisheye HSM",
        [0.01165, -0.04879, 0.09652],
    ),
    (
        "Sigma 12-24mm f/4.5-5.6 EX DG HSM",
        [0.004893, -0.019407, 0.0],
    ),
    ("Sigma 14mm f/2.8 EX", [0.031106, -0.059086, 0.0]),
    ("Sigma 14mm f/3.5 EX", [0.013209, -0.036529, 0.0]),
    (
        "Sigma 15-30mm f/3.5-4.5 EX DG Aspherical",
        [0.02055, -0.04027, -0.02834],
    ),
    (
        "Sigma 150-500mm f/5-6.3 APO DG OS HSM",
        [0.06903, -0.2138, 0.22959],
    ),
    (
        "Sigma 150-600mm f/5-6.3 DG OS HSM | C",
        [-0.002, 0.016, -0.029],
    ),
    (
        "Sigma 150mm f/2.8 EX DG APO HSM Macro",
        [-0.00472839, 0.0149475, -0.0142996],
    ),
    ("Sigma 17-35mm f/2.8-4 EX DG", [0.007263, -0.02454, 0.0]),
    (
        "Sigma 17-50mm f/2.8 EX DC OS HSM",
        [0.03046, -0.085, 0.04461],
    ),
    (
        "Sigma 17-70mm f/2.8-4 DC MACRO OS HSM Contemporary",
        [0.040576, -0.127959, 0.10121],
    ),
    (
        "Sigma 17-70mm f/2.8-4 DC Macro OS HSM",
        [0.01587, -0.03928, -0.00282],
    ),
    (
        "Sigma 17-70mm f/2.8-4.5 DC Macro",
        [0.019088, -0.048894, 0.0],
    ),
    ("Sigma 18-125mm f/3.5-5.6 DC", [0.028249, -0.061326, 0.0]),
    (
        "Sigma 18-200mm f/3.5-6.3 DC",
        [0.0228732, -0.0665251, 0.0172122],
    ),
    (
        "Sigma 18-200mm f/3.5-6.3 DC Macro OS HSM",
        [0.0339, -0.09259, 0.0422],
    ),
    (
        "Sigma 18-200mm f/3.5-6.3 II DC OS HSM",
        [0.00427, -0.0189, 0.0199],
    ),
    (
        "Sigma 18-250mm f/3.5-6.3 DC OS Macro HSM",
        [0.03419, -0.09116, 0.04125],
    ),
    (
        "Sigma 18-35mm f/1.8 DC HSM [A]",
        [0.01397, -0.03966, 0.01675],
    ),
    ("Sigma 18-50mm f/2.8 EX DC", [0.012979, -0.036394, 0.0]),
    ("Sigma 18-50mm f/3.5-5.6 DC", [0.018244, -0.048901, 0.0]),
    (
        "Sigma 180mm f/2.8 EX DG OS HSM APO Macro",
        [-0.00333, 0.00918, -0.00596],
    ),
    ("Sigma 19mm f/2.8 DN", [0.015, -0.046, 0.024]),
    ("Sigma 19mm f/2.8 EX DN", [0.00475, -0.01706, 0.00298]),
    (
        "Sigma 20mm f/1.4 DG HSM | Art 015",
        [0.007, -0.03037, 0.03089],
    ),
    ("Sigma 20mm f/1.8 EX DG", [0.034347, -0.066549, 0.0]),
    (
        "Sigma 24-105mm f/4.0 DG OS HSM [A]",
        [0.03625, -0.10075, 0.0477],
    ),
    (
        "Sigma 24-60mm f/2.8 EX DG",
        [0.00598105, -0.0230051, 0.00638311],
    ),
    (
        "Sigma 24-70mm f/2.8 IF EX DG HSM",
        [-0.00125, -0.00574, -0.00426],
    ),
    ("Sigma 24mm f/2.8 Super Wide II", [0.0, -0.0085, 0.0]),
    (
        "Sigma 28-300mm f/3.5-6.3 Macro ASP IF",
        [0.0, -0.009837, -0.008234],
    ),
    ("Sigma 28-70mm f/2.8 AF", [0.0146, -0.0486, 0.005]),
    ("Sigma 28-70mm f/2.8 EX DG", [0.01508, -0.036446, 0.0]),
    ("Sigma 28mm f/1.8 EX DG", [0.002854, -0.010067, 0.0]),
    ("Sigma 30mm f/1.4 EX DC HSM", [0.003926, -0.013819, 0.0]),
    (
        "Sigma 4.5mm f/2.8 EX DC HSM circular fisheye",
        [-0.21693, -0.44076, -0.47357],
    ),
    (
        "Sigma 50-150mm f/2.8 APO EX DC HSM II",
        [0.00143, -0.0113, 0.00788],
    ),
    ("Sigma 50mm f/1.4 EX DG HSM", [0.00772, -0.02395, 0.00956]),
    ("Sigma 60mm f/2.8 DN", [0.0, 0.00275, 0.0]),
    (
        "Sigma 70-200mm f/2.8 EX DG APO OS HSM",
        [0.00236, -0.00978, -0.00264],
    ),
    (
        "Sigma 70-300mm f/4-5.6 APO Macro Super II",
        [0.002522, -0.005913, 0.0],
    ),
    (
        "Sigma 70-300mm f/4-5.6 DG Macro",
        [-0.00361, -0.00079, 0.00665],
    ),
    (
        "Sigma 70-300mm f/4-5.6 DG OS",
        [-0.00291, 0.00834, -0.00971],
    ),
    (
        "Sigma 70-300mm f/4-5.6 DL Macro",
        [-0.00291, 0.00834, -0.00971],
    ),
    (
        "Sigma 8-16mm f/4.5-5.6 DC HSM",
        [0.02364, -0.13856, 0.19092],
    ),
    (
        "Sigma 8mm f/3.5 EX DG Circular",
        [-0.18485, 0.5218, -0.42534],
    ),
    (
        "Sigma DP2 & compatibles (Standard)",
        [0.00558, -0.01995, 0.00605],
    ),
    (
        "Sigma DT 18-300mm f/3.5-6.3 DC Macro",
        [0.04288, -0.11689, 0.06181],
    ),
    (
        "Sony 70-300mm f/4.5-5.6 G SSM II (SAL70300G2)",
        [0.00376, -0.00221, 0.00606],
    ),
    ("Sony AF DT 16-105mm f/3.5-5.6", [0.03, -0.08, 0.026]),
    (
        "Sony AF DT 18-250mm f/3.5-6.3 (SAL18250)",
        [0.01887, -0.05832, 0.00708],
    ),
    (
        "Sony AF DT 30mm f/2.8 SAM Macro (SAL30M28)",
        [0.00472, -0.02284, 0.02137],
    ),
    (
        "Sony DT 16-50mm f/2.8 SSM (SAL1650)",
        [0.031395, -0.081281, 0.0164699],
    ),
    (
        "Sony DT 18-135mm f/3.5-5.6 SAM",
        [0.03774, -0.1046, 0.03514],
    ),
    ("Sony RX10 & compatibles", [0.0376, -0.14223, 0.1059]),
    ("Sony RX10II & compatibles", [0.0376, -0.14223, 0.1059]),
    ("Sony RX10III & compatibles", [0.0241, -0.0957892, 0.036098]),
    ("Sony Xperia Z3 & compatibles", [0.0132, -0.07765, 0.14683]),
    ("Stylus 1 & compatibles", [-0.00245, -0.00755, -0.06886]),
    (
        "Stylus Epic & compatibles (Standard)",
        [-0.01247, 0.027098, 0.0],
    ),
    ("Super-Takumar 55mm f/1.8", [0.0, -0.00012, 0.0]),
    (
        "Tamron 16-300mm f/3.5-6.3 Di II VC PZD B016 Macro",
        [0.02076, -0.07067, 0.03778],
    ),
    (
        "Tamron 18-200mm f/3.5-6.3 Di III VC",
        [0.02016, -0.04597, -0.03359],
    ),
    (
        "Tamron 28-300mm f/3.5-6.3 Di VC PZD A010",
        [0.0324178, -0.0946795, 0.0297184],
    ),
    ("Tamron 35-70mm f/3.5 CF Macro", [0.0, -0.00768, 0.0]),
    (
        "Tamron AF 17-50mm f/2.8 XR Di-II LD (Model A16)",
        [0.02667, -0.07161, 0.02012],
    ),
    (
        "Tamron AF 18-200mm f/3.5-6.3 XR Di II LD Aspherical (IF) Macro",
        [0.01757, -0.052885, 0.0],
    ),
    (
        "Tamron AF 18-200mm f/3.5-6.3 XR Di II LD Aspherical (IF) Macro A14",
        [0.01364, -0.042157, 0.0],
    ),
    (
        "Tamron AF 18-250mm f/3.5-6.3 Di II LD Aspherical (IF) Macro",
        [0.02036, -0.05905, 0.0],
    ),
    (
        "Tamron AF 18-270mm F/3.5-6.3 Di II VC LD Aspherical (IF) Macro",
        [0.02445, -0.07818, 0.03054],
    ),
    (
        "Tamron AF 18-270mm f/3.5-6.3 Di II VC PZD",
        [0.01861, -0.05313, -0.00391],
    ),
    ("Tamron AF 19-35mm f/3.5-4.5", [0.013926, -0.041094, 0.0]),
    (
        "Tamron AF 28-300mm f/3.5-6.3 XR Di LD Aspherical (IF)",
        [-0.00171, -0.003088, 0.0],
    ),
    (
        "Tamron AF 70-300mm f/4-5.6 LD Macro 1:2",
        [0.00168, -0.00649, 0.0021],
    ),
    (
        "Tamron SP 15-30mm f/2.8 Di VC USD (A012)",
        [0.00365, -0.0373, 0.04147],
    ),
    (
        "Tamron SP 24-70mm f/2.8 Di VC USD",
        [0.03519, -0.10395, 0.06092],
    ),
    (
        "Tamron SP 70-300mm f/4.0-5.6 Di VC USD (A005)",
        [0.0, -0.006, 0.002],
    ),
    (
        "Tamron SP AF 11-18mm f/4.5-5.6 Di-II LD Aspherical (IF)",
        [-0.01251, 0.06318, -0.12889],
    ),
    (
        "Tamron SP AF 17-35mm f/2.8-4 Di LD Aspherical (IF)",
        [0.023029, -0.056393, 0.0],
    ),
    (
        "Tamron SP AF 17-50mm f/2.8 XR Di II LD Aspherical (IF)",
        [0.02667, -0.07161, 0.02012],
    ),
    (
        "Tamron SP AF 17-50mm f/2.8 XR Di II VC LD Aspherical (IF)",
        [0.01062, -0.02795, -0.01852],
    ),
    (
        "Tamron SP AF 24-135mm f/3.5-5.6 AD Aspherical (IF) Macro",
        [0.004951, -0.019509, 0.0],
    ),
    (
        "Tamron SP AF 28-105mm f/2.8 LD Aspherical IF",
        [0.020983, -0.054749, 0.0],
    ),
    (
        "Tamron SP AF 28-75mm f/2.8 XR Di LD Aspherical (IF)",
        [0.01469, -0.034774, 0.0],
    ),
    (
        "Tamron SP AF 60mm f/2 Di II LD (IF) Macro 1:1",
        [0.01029, -0.0342, 0.02936],
    ),
    (
        "Tamron SP AF 90mm f/2.8 Di Macro 1:1",
        [-0.00126953, 0.00404399, -0.0027782],
    ),
    (
        "Tokina 11-16mm f/2.8 AT-X 116 AF Pro DX",
        [-0.024, -0.002, 0.005],
    ),
    (
        "Tokina 12-24mm f/4 AT-X 124 AF Pro DX",
        [-0.005227, 0.0018, 0.0],
    ),
    ("Tokina 17mm f/3.5 AT-X 17 AF Pro", [0.0, -0.012125, 0.0]),
    (
        "Tokina 19-35mm f/3.5-4.5 AF 193",
        [0.0, -0.005476, -0.022162],
    ),
    (
        "Tokina 80-200mm f/4.5-5.6 SZ-X",
        [-0.01761, 0.06433, -0.07566],
    ),
    (
        "Tokina AF 100mm f/2.8 AT-X Pro D M100 Macro",
        [0.0, -0.0008, 0.0],
    ),
    (
        "Tokina AF 11-20mm f/2.8 AT-X Pro DX",
        [0.0208245, -0.102661, 0.119784],
    ),
    (
        "Tokina AF 12-28mm f/4 AT-X Pro DX",
        [-0.0155, 0.02702, -0.01478],
    ),
    (
        "Tokina AF 28-80mm f/2.8 AT-X 280 Pro",
        [0.01909, -0.06299, 0.03342],
    ),
    (
        "Tokina AF 80-200mm f/2.8 AT-X 828 Pro",
        [0.00084, -0.00992, 0.00919],
    ),
    ("VCL-ECF1 fisheye converter", [-0.03545, 0.08332, -0.03802]),
    (
        "VCL-ECU1 ultra wide converter",
        [0.04359, -0.10483, 0.04408],
    ),
    (
        "Voigtlander Ultron 40mm f/2 SLII Aspherical",
        [0.00914, -0.03392, 0.02683],
    ),
    (
        "WB2000 & compatibles (Standard)",
        [0.0252892, -0.097418, 0.0155762],
    ),
    ("X10 & compatibles (Standard)", [0.02972, -0.09564, 0.0155]),
    ("X100 & compatibles (Standard)", [0.02159, -0.0618, 0.04364]),
    (
        "X100 & compatibles (Standard) + TCL-X100",
        [0.02986, -0.05994, 0.05518],
    ),
    (
        "X70 & compatibles (Standard)",
        [0.0246309, -0.0712065, 0.0634822],
    ),
    ("XC16-50mmF3.5-5.6 OIS", [0.02487, -0.09617, 0.03106]),
    ("XC16-50mmF3.5-5.6 OIS II", [0.02234, -0.0861, 0.01689]),
    ("XF10-24mmF4 R OIS", [0.01619, -0.07655, 0.05816]),
    (
        "XF100-400mmF4.5-5.6 R LM OIS WR + 1.4x converter",
        [-0.00910152, 0.0158002, -0.00149072],
    ),
    ("XF14mmF2.8 R", [0.003995, -0.009534, 0.002875]),
    ("XF16-55mmF2.8 R LM WR", [0.02551, -0.08802, 0.02986]),
    ("XF16mmF1.4 R WR", [0.00071, -0.00085, -0.005909]),
    ("XF18-135mmF3.5-5.6R LM OIS WR", [0.01941, -0.06675, 0.0182]),
    ("XF18-55mmF2.8-4 R LM OIS", [0.02808, -0.10604, 0.07041]),
    ("XF18mmF2 R", [-0.01002, 0.01206, -0.03947]),
    ("XF23mmF1.4 R", [0.00331, -0.0094, 0.00784]),
    ("XF23mmF2 R WR", [0.0289722, -0.0763625, 0.0493558]),
    ("XF35mmF1.4 R", [0.00729, -0.02422, 0.02113]),
    ("XF35mmF2 R WR", [0.00956, -0.07057, 0.07334]),
    ("XF56mmF1.2 R", [0.00239, -0.00767, 0.00358]),
    ("XF56mmF1.2 R APD", [0.000434, -0.006427, 0.018063]),
    ("XZ-1 & compatibles (Standard)", [0.01604, -0.05047, 0.0]),
    ("Yongnuo YN 35mm f/2", [-0.00119, 0.00329, -0.01831]),
    ("Zeiss 21mm f/2.8 Distagon", [0.021925, -0.043445, 0.0]),
    ("Zeiss 28mm f/2.8 Distagon", [0.007448, -0.019182, 0.0]),
    ("Zeiss Batis 2.8/18", [0.03666, -0.0936845, 0.0413531]),
    ("Zuiko Auto-S 50mm f/1.8", [4e-05, 0.00389, -0.01321]),
    ("smc PENTAX-FA 77mm F1.8 Limited", [0.0, 0.00174, 0.0]),
    ("smc Pentax-A 28mm 1:2.8", [0.00207, -0.0073, -0.00479]),
    ("smc Pentax-A 50mm f/1.7", [0.0, -0.00348, 0.0]),
    (
        "smc Pentax-D FA Macro 100mm f/2.8 WR",
        [0.00355, -0.01675, 0.02729],
    ),
    (
        "smc Pentax-DA 15mm f/4 ED AL Limited",
        [0.00305, 0.00418, -0.01276],
    ),
    (
        "smc Pentax-DA 16-45mm f/4 ED AL",
        [0.012341, -0.035947, 0.0],
    ),
    (
        "smc Pentax-DA 17-70mm f/4 AL [IF] SDM",
        [-0.01067, 0.03945, -0.07039],
    ),
    (
        "smc Pentax-DA 18-135mm f/3.5-5.6 ED AL IF DC WR",
        [0.02393, -0.05763, 0.001],
    ),
    (
        "smc Pentax-DA 18-250mm f/3.5-6.3 ED AL [IF]",
        [0.02036, -0.05905, 0.0],
    ),
    (
        "smc Pentax-DA 18-55mm f/3.5-5.6 AL",
        [0.00606571, -0.0251409, -0.00216764],
    ),
    (
        "smc Pentax-DA 18-55mm f/3.5-5.6 AL II L WR",
        [0.01335, -0.04723, 0.02086],
    ),
    ("smc Pentax-DA 35mm f/2.4 AL", [0.0, -0.0038, 0.0]),
    (
        "smc Pentax-DA 35mm f/2.8 Macro Limited",
        [0.01494, -0.04536, 0.03628],
    ),
    ("smc Pentax-DA 40mm f/2.8 XS", [0.00084, 0.0026, -0.01446]),
    (
        "smc Pentax-DA 55-300mm f/4-5.8 ED",
        [-0.00518, 0.01937, -0.02824],
    ),
    (
        "smc Pentax-DA Fish-Eye 10-17mm f/3.5-4.5 ED IF",
        [-0.03518, 0.12586, -0.09198],
    ),
    (
        "smc Pentax-DA L 18-50mm f/4-5.6 DC WR RE",
        [0.00982, -0.03805, 0.01695],
    ),
    (
        "smc Pentax-DA L 55-300mm f/4-5.8 ED",
        [-0.00518, 0.01937, -0.02824],
    ),
    (
        "smc Pentax-DA* 16-50mm f/2.8 ED AL IF SDM",
        [0.02994, -0.08352, 0.03371],
    ),
    ("smc Pentax-FA 28mm f/2.8 AL", [0.0, -0.00843, 0.0]),
    (
        "smc Pentax-FA 31mm f/1.8 AL Limited",
        [-0.000413, -0.000458, -0.007123],
    ),
    (
        "smc Pentax-FA 43mm f/1.9 Limited",
        [-0.00469, 0.01317, -0.0],
    ),
    ("smc Pentax-FA 50mm f/1.4", [0.0, -0.00203, 0.0]),
    ("smc Pentax-M 28mm 1:3.5", [0.01008, -0.03516, 0.02694]),
    (
        "smc Pentax-M 50mm f/1.7",
        [0.00266415, -0.0099024, 0.0120246],
    ),
];
