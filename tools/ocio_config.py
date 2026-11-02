"""Writes BananaV's ACES 1.3 OpenColorIO configuration,
`crates/ocio/configs/bananav-aces-1.3.ocio`: its color spaces' matrices
worked out here (RGB from primaries; Bradford to ACES's white for the
standard gamuts, CAT02 for the camera makers', as their published
matrices), the same math as bv-ocio's math.rs. Change this script, not
the file, and run it from the repository's root (it needs numpy):

    python tools/ocio_config.py crates/ocio/configs/bananav-aces-1.3.ocio
"""
import sys

import numpy as np

D65 = (0.3127, 0.3290)
ACES_W = (0.32168, 0.33767)
P = {
    'AP0': ((0.7347, 0.2653), (0.0, 1.0), (0.0001, -0.077), ACES_W),
    'AP1': ((0.713, 0.293), (0.165, 0.830), (0.128, 0.044), ACES_W),
    'REC709': ((0.64, 0.33), (0.30, 0.60), (0.15, 0.06), D65),
    'REC2020': ((0.708, 0.292), (0.170, 0.797), (0.131, 0.046), D65),
    'P3D65': ((0.680, 0.320), (0.265, 0.690), (0.150, 0.060), D65),
    'BMD_WG5': ((0.7177215, 0.3171181), (0.2280410, 0.8615690), (0.1005841, -0.0820452), D65),
    'DAVINCI_WG': ((0.8000, 0.3130), (0.1682, 0.9877), (0.0790, -0.1155), D65),
    # The camera makers' gamuts, as bv-ocio's math.rs has them.
    'ARRI_WG3': ((0.684, 0.313), (0.221, 0.848), (0.0861, -0.102), D65),
    'ARRI_WG4': ((0.7347, 0.2653), (0.1424, 0.8576), (0.0991, -0.0308), D65),
    'SGAMUT3': ((0.730, 0.280), (0.140, 0.855), (0.100, -0.050), D65),
    'SGAMUT3_CINE': ((0.766, 0.275), (0.225, 0.800), (0.089, -0.087), D65),
    'VENICE_SGAMUT3': ((0.740464264304292, 0.279364374750660), (0.089241145423286, 0.893809528608105),
                       (0.110488236673827, -0.052579333080476), D65),
    'VENICE_SGAMUT3_CINE': ((0.775901871567345, 0.274502392854799), (0.188682902773355, 0.828684937020288),
                            (0.101337382499301, -0.089187517306263), D65),
    'CANON_CGAMUT': ((0.74, 0.27), (0.17, 1.14), (0.08, -0.10), D65),
    'RED_WG': ((0.780308, 0.304253), (0.121595, 1.493994), (0.095612, -0.084589), D65),
    'VGAMUT': ((0.730, 0.280), (0.165, 0.840), (0.100, -0.030), D65),
}
BRADFORD = np.array([[0.8951, 0.2664, -0.1614], [-0.7502, 1.7135, 0.0367], [0.0389, -0.0685, 1.0296]])
CAT02 = np.array([[0.7328, 0.4296, -0.1624], [-0.7036, 1.6975, 0.0061], [0.0030, 0.0136, 0.9834]])


def xyz(xy):
    return np.array([xy[0] / xy[1], 1.0, (1 - xy[0] - xy[1]) / xy[1]])


def rgb_to_xyz(p):
    m = np.stack([xyz(p[0]), xyz(p[1]), xyz(p[2])], 1)
    s = np.linalg.solve(m, xyz(p[3]))
    return m * s


def adapt(cat, a, b):
    if a == b:
        return np.eye(3)
    s, d = cat @ xyz(a), cat @ xyz(b)
    return np.linalg.inv(cat) @ np.diag(d / s) @ cat


def rgb_to_rgb(a, b, cat=BRADFORD):
    pa, pb = P[a], P[b]
    return np.linalg.inv(rgb_to_xyz(pb)) @ adapt(cat, pa[3], pb[3]) @ rgb_to_xyz(pa)


def matrix(m):
    rows = []
    for i in range(3):
        rows.append(', '.join('%.15g' % v for v in m[i]) + ', 0')
    return '[' + ', '.join(rows) + ', 0, 0, 0, 1]'


def to_aces(space, cat=BRADFORD):
    return '!<MatrixTransform> {matrix: %s}' % matrix(rgb_to_rgb(space, 'AP0', cat))


def q(text):
    assert '"' not in text and '\\' not in text
    return '"%s"' % text


def cs(name, family, description, categories, encoding, to=None, aliases=(), isdata=False):
    out = ['  - !<ColorSpace>', '    name: %s' % name]
    if aliases:
        out.append('    aliases: [%s]' % ', '.join(aliases))
    out += [
        '    family: %s' % family,
        '    equalitygroup: ""',
        '    bitdepth: 32f',
        '    description: %s' % q(description),
        '    isdata: %s' % ('true' if isdata else 'false'),
        '    categories: [%s]' % ', '.join(categories),
        '    encoding: %s' % encoding,
    ]
    if to:
        out.append('    to_scene_reference: %s' % to)
    return '\n'.join(out)


def group(*children):
    return '!<GroupTransform>\n      children:\n' + '\n'.join('        - %s' % c for c in children)


def dcs(name, description, style, aliases):
    return '\n'.join([
        '  - !<ColorSpace>',
        '    name: %s' % name,
        '    aliases: [%s]' % ', '.join(aliases),
        '    family: Display',
        '    equalitygroup: ""',
        '    bitdepth: 32f',
        '    description: %s' % q(description),
        '    isdata: false',
        '    categories: [file-io]',
        '    encoding: sdr-video',
        '    from_display_reference: !<BuiltinTransform> {style: %s}' % style,
    ])


REC709_OETF = '!<ExponentWithLinearTransform> {gamma: 2.22222222222222, offset: 0.099}'
SRGB = '!<ExponentWithLinearTransform> {gamma: 2.4, offset: 0.055}'

# Each display (its color space named as it is) and the ACES output its
# ACES view uses; the studio configuration's names stay as aliases.
DISPLAYS = [
    ('sRGB', 'A computer monitor, sRGB curve and Rec. 709 primaries.',
     'DISPLAY - CIE-XYZ-D65_to_sRGB', 'ACES 1.0 - SDR Video', ['sRGB - Display', 'srgb_display']),
    ('Rec.709', 'A video monitor, BT.1886 (gamma 2.4) and Rec. 709 primaries.',
     'DISPLAY - CIE-XYZ-D65_to_REC.1886-REC.709', 'ACES 1.0 - SDR Video',
     ['Rec.1886 Rec.709 - Display', 'rec1886_rec709_display']),
    ('Rec.2020', 'A video monitor, BT.1886 (gamma 2.4) and Rec. 2020 primaries.',
     'DISPLAY - CIE-XYZ-D65_to_REC.1886-REC.2020', 'ACES 1.0 - SDR Video',
     ['Rec.1886 Rec.2020 - Display', 'rec1886_rec2020_display']),
    ('Display P3', 'Display P3, sRGB curve with P3 primaries and a D65 white.',
     'DISPLAY - CIE-XYZ-D65_to_DisplayP3', 'ACES 1.0 - SDR Video', ['Display P3 - Display', 'displayp3_display']),
    ('P3-D65', 'A cinema projector, gamma 2.6 with P3 primaries and a D65 white.',
     'DISPLAY - CIE-XYZ-D65_to_G2.6-P3-D65', 'ACES 1.0 - SDR Cinema', ['P3-D65 - Display', 'g26_p3d65_display']),
    ('P3-DCI', 'A cinema projector, gamma 2.6 with P3 primaries and the DCI white.',
     'DISPLAY - CIE-XYZ-D65_to_G2.6-P3-DCI-BFD', 'ACES 1.0 - SDR Cinema', ['P3-DCI - Display', 'g26_p3dci_display']),
]

# ACES 1.x configurations' names, kept as aliases so projects and other
# hosts that use them find the same spaces.
OLD = {
    'ACES2065-1': ['ACES - ACES2065-1'],
    'ACEScc': ['ACES - ACEScc'],
    'ACEScct': ['ACES - ACEScct'],
    'ACEScg': ['ACES - ACEScg'],
    'ACESproxy': ['ACES - ACESproxy'],
    'Linear Rec.709 (sRGB)': ['Utility - Linear - sRGB', 'Utility - Linear - Rec.709'],
    'Linear Rec.2020': ['Utility - Linear - Rec.2020'],
    'Linear P3-D65': ['Utility - Linear - P3-D65'],
    'sRGB - Texture': ['Utility - sRGB - Texture'],
    'Camera Rec.709': ['Utility - Rec.709 - Camera'],
    'ARRI LogC3 (EI800)': ['Input - ARRI - V3 LogC (EI800) - Wide Gamut'],
    'S-Log3 S-Gamut3': ['Input - Sony - S-Log3 - S-Gamut3'],
    'S-Log3 S-Gamut3.Cine': ['Input - Sony - S-Log3 - S-Gamut3.Cine'],
    'V-Log V-Gamut': ['Input - Panasonic - V-Log - V-Gamut'],
    'Log3G10 REDWideGamutRGB': ['Input - RED - Log3G10 - REDWideGamutRGB'],
    'Linear ARRI Wide Gamut 3': ['Input - ARRI - Linear - ALEXA Wide Gamut'],
    'CanonLog2 CinemaGamut D55': ['Input - Canon - Canon-Log2 - Cinema Gamut Daylight'],
    'CanonLog3 CinemaGamut D55': ['Input - Canon - Canon-Log3 - Cinema Gamut Daylight'],
    'Linear CinemaGamut D55': ['Input - Canon - Linear - Canon Cinema Gamut Daylight'],
    'Linear V-Gamut': ['Input - Panasonic - Linear - V-Gamut'],
    'Linear REDWideGamutRGB': ['Input - RED - Linear - REDWideGamutRGB'],
    'Linear S-Gamut3': ['Input - Sony - Linear - S-Gamut3'],
    'Linear S-Gamut3.Cine': ['Input - Sony - Linear - S-Gamut3.Cine'],
    'Linear Venice S-Gamut3': ['Input - Sony - Linear - Venice S-Gamut3'],
    'Linear Venice S-Gamut3.Cine': ['Input - Sony - Linear - Venice S-Gamut3.Cine'],
    'Raw': ['Utility - Raw'],
}


def main(out):
    lines = [
        '# Written by tools/ocio_config.py: change the script, not this file.',
        'ocio_profile_version: 2.1',
        '',
        'environment:',
        '  {}',
        'search_path: ""',
        'strictparsing: true',
        'luma: [0.2126, 0.7152, 0.0722]',
        'name: bananav-aces-1.3',
        'description: |',
        '  BananaV\'s color management: the Academy Color Encoding System 1.3.',
        '  ACES2065-1 is the scene reference, CIE XYZ (D65) the displays\'.',
        '  Written for BananaV; its transforms are OpenColorIO\'s builtins and standard',
        '  transforms, so other OpenColorIO hosts read it too.',
        '',
        'roles:',
        '  aces_interchange: ACES2065-1',
        '  cie_xyz_d65_interchange: CIE-XYZ-D65',
        '  color_picking: sRGB - Texture',
        '  color_timing: ACEScct',
        '  compositing_log: ACEScct',
        '  data: Raw',
        '  default: ACES2065-1',
        '  matte_paint: sRGB - Texture',
        '  scene_linear: ACEScg',
        '  texture_paint: ACEScct',
        '',
        'file_rules:',
        '  - !<Rule> {name: Default, colorspace: ACES2065-1}',
        '',
        'shared_views:',
        '  - !<View> {name: Un-tone-mapped, view_transform: Un-tone-mapped, display_colorspace: <USE_DISPLAY_NAME>}',
        '',
        'displays:',
    ]
    for name, _, _, aces, _ in DISPLAYS:
        lines += [
            '  %s:' % name,
            '    - !<View> {name: ACES, view_transform: %s, display_colorspace: %s}' % (aces, name),
            '    - !<Views> [Un-tone-mapped]',
            '    - !<View> {name: Raw, colorspace: Raw}',
        ]
    lines += [
        '',
        'active_displays: [%s]' % ', '.join(d[0] for d in DISPLAYS),
        'active_views: [ACES, Un-tone-mapped, Raw]',
        'inactive_colorspaces: [CIE-XYZ-D65]',
        '',
        'looks:',
        '  - !<Look>',
        '    name: ACES 1.3 Reference Gamut Compression',
        '    process_space: ACES2065-1',
        '    description: %s' % q('Brings the colors outside AP1, the strongest of cameras and lights, inside it.'),
        '    transform: !<BuiltinTransform> {style: ACES-LMT - ACES 1.3 Reference Gamut Compression}',
        '  - !<Look>',
        '    name: Blue Light Artifact Fix',
        '    process_space: ACES2065-1',
        '    description: %s' % q('Desaturates the bright blues of sirens and LEDs, which would clip or shift hue.'),
        '    transform: !<BuiltinTransform> {style: ACES-LMT - BLUE_LIGHT_ARTIFACT_FIX}',
        '',
        'default_view_transform: Un-tone-mapped',
        '',
        'view_transforms:',
        '  - !<ViewTransform>',
        '    name: ACES 1.0 - SDR Video',
        '    description: %s' % q('The ACES reference rendering and output for video monitors in a dim surround.'),
        '    from_scene_reference: !<BuiltinTransform> {style: ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-VIDEO_1.0}',
        '  - !<ViewTransform>',
        '    name: ACES 1.0 - SDR Cinema',
        '    description: %s' % q('The ACES reference rendering and output for cinema projectors in a dark surround.'),
        '    from_scene_reference: !<BuiltinTransform> {style: ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-CINEMA_1.0}',
        '  - !<ViewTransform>',
        '    name: Un-tone-mapped',
        '    description: %s' % q('Scene light shown as it is, without a rendering.'),
        '    from_scene_reference: !<BuiltinTransform> {style: UTILITY - ACES-AP0_to_CIE-XYZ-D65_BFD}',
        '',
        'display_colorspaces:',
        '\n'.join([
            '  - !<ColorSpace>',
            '    name: CIE-XYZ-D65',
            '    aliases: [cie_xyz_d65]',
            '    family: ""',
            '    equalitygroup: ""',
            '    bitdepth: 32f',
            '    description: %s' % q('The displays\' connection space, CIE XYZ under D65, 1 the display\'s white.'),
            '    isdata: false',
            '    categories: [file-io]',
            '    encoding: display-linear',
        ]),
    ]
    for name, description, style, _, aliases in DISPLAYS:
        lines.append('')
        lines.append(dcs(name, description, style, aliases))
    lines += ['', 'colorspaces:']

    def space(name, family, description, categories, encoding, to=None, aliases=(), isdata=False):
        return cs(name, family, description, categories, encoding, to, list(aliases) + OLD.get(name, []), isdata)

    spaces = [
        space('ACES2065-1', 'ACES', 'The Academy Color Encoding Specification, SMPTE ST 2065-1: AP0 primaries, scene light.',
              ['file-io'], 'scene-linear', aliases=['aces2065_1', 'lin_ap0']),
        space('ACEScc', 'ACES', 'The ACES log encoding for grading, AP1 primaries.', ['file-io'], 'log',
              to='!<BuiltinTransform> {style: ACEScc_to_ACES2065-1}', aliases=['acescc_ap1']),
        space('ACEScct', 'ACES', 'ACEScc with a toe, as the logs of film scans, AP1 primaries.',
              ['file-io', 'working-space'], 'log',
              to='!<BuiltinTransform> {style: ACEScct_to_ACES2065-1}', aliases=['acescct_ap1']),
        space('ACEScg', 'ACES', 'The ACES working space for rendering and compositing: AP1 primaries, scene light.',
              ['file-io', 'working-space'], 'scene-linear',
              to='!<BuiltinTransform> {style: ACEScg_to_ACES2065-1}', aliases=['lin_ap1']),
        space('ACESproxy', 'ACES', 'The ACES integer log encoding for previews on set, 10-bit, AP1 primaries.',
              ['file-io'], 'log',
              to='!<BuiltinTransform> {style: ACESproxy10i_to_ACES2065-1}', aliases=['acesproxy10i_ap1']),
        space('Linear Rec.709 (sRGB)', 'Utility', 'Scene light with Rec. 709 (sRGB) primaries.',
              ['file-io', 'working-space'], 'scene-linear', to=to_aces('REC709'),
              aliases=['lin_rec709_srgb', 'lin_srgb', 'lin_rec709']),
        space('Linear Rec.2020', 'Utility', 'Scene light with Rec. 2020 primaries.', ['file-io', 'working-space'],
              'scene-linear', to=to_aces('REC2020'), aliases=['lin_rec2020']),
        space('Linear P3-D65', 'Utility', 'Scene light with P3 primaries and a D65 white.',
              ['file-io', 'working-space'], 'scene-linear', to=to_aces('P3D65'), aliases=['lin_p3d65']),
        space('sRGB - Texture', 'Utility', 'The sRGB curve and Rec. 709 primaries, read as scene light: pictures, graphics.',
              ['file-io', 'texture'], 'sdr-video',
              to=group(SRGB, to_aces('REC709')), aliases=['srgb_tx', 'srgb_texture']),
        space('Camera Rec.709', 'Utility', 'The Rec. 709 camera curve (BT.709 OETF) and primaries, read as scene light.',
              ['file-io'], 'sdr-video', to=group(REC709_OETF, to_aces('REC709')), aliases=['rec709_camera']),
        space('ARRI LogC3 (EI800)', 'Camera', 'LogC3 at EI 800 with ARRI Wide Gamut 3.', ['file-io'], 'log',
              to='!<BuiltinTransform> {style: ARRI_ALEXA-LOGC-EI800-AWG_to_ACES2065-1}',
              aliases=['arri_logc3_ei800']),
        space('ARRI LogC4', 'Camera', 'LogC4 with ARRI Wide Gamut 4.', ['file-io'], 'log',
              to='!<BuiltinTransform> {style: ARRI_LOGC4_to_ACES2065-1}', aliases=['arri_logc4']),
        space('S-Log3 S-Gamut3', 'Camera', 'S-Log3 with S-Gamut3.', ['file-io'], 'log',
              to='!<BuiltinTransform> {style: SONY_SLOG3-SGAMUT3_to_ACES2065-1}', aliases=['slog3_sgamut3']),
        space('S-Log3 S-Gamut3.Cine', 'Camera', 'S-Log3 with S-Gamut3.Cine.', ['file-io'], 'log',
              to='!<BuiltinTransform> {style: SONY_SLOG3-SGAMUT3.CINE_to_ACES2065-1}',
              aliases=['slog3_sgamutcine']),
        space('S-Log3 Venice S-Gamut3', 'Camera', 'S-Log3 with the VENICE camera\'s S-Gamut3.', ['file-io'], 'log',
              to='!<BuiltinTransform> {style: SONY_SLOG3-SGAMUT3-VENICE_to_ACES2065-1}',
              aliases=['slog3_venice_sgamut3']),
        space('S-Log3 Venice S-Gamut3.Cine', 'Camera', 'S-Log3 with the VENICE camera\'s S-Gamut3.Cine.',
              ['file-io'], 'log',
              to='!<BuiltinTransform> {style: SONY_SLOG3-SGAMUT3.CINE-VENICE_to_ACES2065-1}',
              aliases=['slog3_venice_sgamutcine']),
        space('V-Log V-Gamut', 'Camera', 'V-Log with V-Gamut.', ['file-io'], 'log',
              to='!<BuiltinTransform> {style: PANASONIC_VLOG-VGAMUT_to_ACES2065-1}', aliases=['vlog_vgamut']),
        space('Log3G10 REDWideGamutRGB', 'Camera', 'Log3G10 with REDWideGamutRGB.', ['file-io'], 'log',
              to='!<BuiltinTransform> {style: RED_LOG3G10-RWG_to_ACES2065-1}', aliases=['log3g10_rwg']),
        space('Apple Log', 'Camera', 'Apple Log, Rec. 2020 primaries.', ['file-io'], 'log',
              to='!<BuiltinTransform> {style: APPLE_LOG_to_ACES2065-1}', aliases=['apple_log']),
        space('BMDFilm WideGamut Gen5', 'Camera', 'Blackmagic Film Gen 5 with its Wide Gamut.',
              ['file-io'], 'log',
              to=group('!<LogCameraTransform> {base: 2.71828182845905, log_side_slope: 0.0869287606549122, '
                       'log_side_offset: 0.530013339229194, lin_side_slope: 1, lin_side_offset: 0.00549407243225781, '
                       'lin_side_break: 0.005, linear_slope: 8.28360593240249, direction: inverse}',
                       to_aces('BMD_WG5', CAT02)), aliases=['bmdfilm_widegamut_gen5']),
        space('DaVinci Intermediate WideGamut', 'Camera', 'DaVinci Intermediate with DaVinci Wide Gamut.',
              ['file-io'], 'log',
              to=group('!<LogCameraTransform> {base: 2, log_side_slope: 0.07329248, log_side_offset: 0.51304736, '
                       'lin_side_slope: 1, lin_side_offset: 0.0075, lin_side_break: 0.00262409, '
                       'linear_slope: 10.44426855, direction: inverse}',
                       to_aces('DAVINCI_WG', CAT02)), aliases=['davinci_intermediate_widegamut']),
        # The cameras' gamuts in scene light, and Canon's logs (the ACES
        # studio configuration's camera spaces, as VEGAS lists them).
        space('Linear ARRI Wide Gamut 3', 'Camera', 'Scene light with ARRI Wide Gamut 3.', ['file-io'],
              'scene-linear', to=to_aces('ARRI_WG3', CAT02), aliases=['lin_arri_wide_gamut_3']),
        space('Linear ARRI Wide Gamut 4', 'Camera', 'Scene light with ARRI Wide Gamut 4.', ['file-io'],
              'scene-linear', to=to_aces('ARRI_WG4', CAT02), aliases=['lin_arri_wide_gamut_4']),
        space('Linear BMD WideGamut Gen5', 'Camera', 'Scene light with Blackmagic Wide Gamut Gen 5.', ['file-io'],
              'scene-linear', to=to_aces('BMD_WG5', CAT02), aliases=['lin_bmd_widegamut_gen5']),
        space('Linear DaVinci WideGamut', 'Camera', 'Scene light with DaVinci Wide Gamut.', ['file-io'],
              'scene-linear', to=to_aces('DAVINCI_WG', CAT02), aliases=['lin_davinci_widegamut']),
        space('CanonLog2 CinemaGamut D55', 'Camera', 'Canon Log 2 with Cinema Gamut (daylight).', ['file-io'], 'log',
              to='!<BuiltinTransform> {style: CANON_CLOG2-CGAMUT_to_ACES2065-1}',
              aliases=['canonlog2_cinemagamut_d55']),
        space('CanonLog3 CinemaGamut D55', 'Camera', 'Canon Log 3 with Cinema Gamut (daylight).', ['file-io'], 'log',
              to='!<BuiltinTransform> {style: CANON_CLOG3-CGAMUT_to_ACES2065-1}',
              aliases=['canonlog3_cinemagamut_d55']),
        space('Linear CinemaGamut D55', 'Camera', 'Scene light with Canon Cinema Gamut (daylight).', ['file-io'],
              'scene-linear', to=to_aces('CANON_CGAMUT', CAT02), aliases=['lin_cinemagamut_d55']),
        space('Linear V-Gamut', 'Camera', 'Scene light with Panasonic V-Gamut.', ['file-io'], 'scene-linear',
              to=to_aces('VGAMUT', CAT02), aliases=['lin_vgamut']),
        space('Linear REDWideGamutRGB', 'Camera', 'Scene light with REDWideGamutRGB.', ['file-io'], 'scene-linear',
              to=to_aces('RED_WG'), aliases=['lin_redwidegamutrgb']),
        space('Linear S-Gamut3', 'Camera', 'Scene light with S-Gamut3.', ['file-io'], 'scene-linear',
              to=to_aces('SGAMUT3', CAT02), aliases=['lin_sgamut3']),
        space('Linear S-Gamut3.Cine', 'Camera', 'Scene light with S-Gamut3.Cine.', ['file-io'], 'scene-linear',
              to=to_aces('SGAMUT3_CINE', CAT02), aliases=['lin_sgamut3cine']),
        space('Linear Venice S-Gamut3', 'Camera', "Scene light with the VENICE camera's S-Gamut3.", ['file-io'],
              'scene-linear', to=to_aces('VENICE_SGAMUT3', CAT02), aliases=['lin_venice_sgamut3']),
        space('Linear Venice S-Gamut3.Cine', 'Camera', "Scene light with the VENICE camera's S-Gamut3.Cine.",
              ['file-io'], 'scene-linear', to=to_aces('VENICE_SGAMUT3_CINE', CAT02),
              aliases=['lin_venice_sgamut3cine']),
        space('Raw', 'Utility', 'Values that are not colors, which nothing converts.', ['file-io'], 'data',
              aliases=['raw'], isdata=True),
    ]
    lines.append('\n\n'.join(spaces))
    text = '\n'.join(lines) + '\n'
    open(out, 'w', encoding='utf-8', newline='\n').write(text)
    print('wrote', out, len(text), 'bytes')


if __name__ == '__main__':
    main(sys.argv[1])
