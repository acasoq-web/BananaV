#!/usr/bin/env python3
"""Makes BananaV.app and its disk image for macOS (run on a Mac).

    python3 packaging/macos/make_app.py --version 0.3.0 \
        --arch arm64 <bananav built for arm64> <its FFmpeg prefix> \
        --arch x86_64 <bananav built for x86_64> <its FFmpeg prefix> \
        --out dist [--dmg BananaV-0.3.0-macos-universal.dmg] [--docs <folder>]

- One program for both of the Mac's processors (Apple's chips and Intel):
  each processor's build put together (`lipo`); one `--arch` makes a
  program for that processor only.
- The libraries the program uses that are not the system's (FFmpeg's, and
  the libraries they use) go inside, in Contents/Frameworks, found from the
  program (`@rpath`), each one too for both processors.
- Info.plist (packaging/macos/Info.plist, its version filled in), the icon
  and the Brazilian Portuguese texts the system shows.
- Signed: with the Developer ID named by MACOS_SIGN_IDENTITY (the keychain
  holding it), the hardened runtime and packaging/macos/BananaV.entitlements
  — then, with APPLE_ID, APPLE_TEAM_ID and APPLE_APP_PASSWORD, notarized by
  Apple and stapled —; else ad hoc (Apple's chips run nothing unsigned).
- The disk image: BananaV.app, a link to Applications, the README and the
  folder of documents given (`--docs`: licenses and notices).
"""

import argparse
import os
import plistlib
import shutil
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
SYSTEM = ("/usr/lib/", "/System/")


def run(*args, capture=False, shown=None):
    print("+", shown or " ".join(args), flush=True)
    if capture:
        return subprocess.run(args, check=True, capture_output=True, text=True).stdout
    subprocess.run(args, check=True)
    return ""


def lines(*args):
    """A tool's lines, without the headers it writes before each file or
    each processor's part of a universal file ("path (architecture arm64):")."""
    return [l.strip() for l in run(*args, capture=True).splitlines()
            if l.strip() and not l.rstrip().endswith(":")]


def dependencies(path):
    """The libraries `path` loads (`otool -L`), its own id left out."""
    own = set(lines("otool", "-D", path))
    deps = []
    for line in lines("otool", "-L", path):
        name = line.split(" (compatibility")[0].strip()
        if name and name not in deps and name not in own:
            deps.append(name)
    return deps


def resolve(dep, prefix, loader_dir):
    """Where library `dep` (as a program names it) is on this computer."""
    lib = os.path.join(prefix, "lib")
    for start in ("@rpath/", "@loader_path/", "@executable_path/"):
        if dep.startswith(start):
            for d in (lib, loader_dir):
                p = os.path.join(d, dep[len(start):])
                if os.path.exists(p):
                    return p
            return None
    return dep if os.path.exists(dep) else None


def gather(exe, prefix, into):
    """Copies the libraries `exe` needs, and theirs, into `into` (by the
    names they are asked for: libavcodec.61.dylib). Returns {asked: name}."""
    names = {}
    todo = [exe]
    seen = set()
    while todo:
        path = todo.pop()
        if path in seen:
            continue
        seen.add(path)
        for dep in dependencies(path):
            if dep.startswith(SYSTEM):
                continue
            found = resolve(dep, prefix, os.path.dirname(path))
            if found is None:
                sys.exit(f"make_app: {path} needs {dep}, which is not here")
            name = os.path.basename(dep)
            names[dep] = name
            target = os.path.join(into, name)
            if not os.path.exists(target):
                shutil.copy(os.path.realpath(found), target)
                os.chmod(target, 0o755)
                todo.append(target)
    return names


def sign(path, identity, entitlements=None):
    args = ["codesign", "--force", "--sign", identity]
    if identity != "-":
        args += ["--options", "runtime", "--timestamp"]
    if entitlements:
        args += ["--entitlements", entitlements]
    run(*args, path)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--version", required=True)
    ap.add_argument("--arch", nargs=3, action="append", required=True,
                    metavar=("NAME", "PROGRAM", "PREFIX"))
    ap.add_argument("--out", required=True)
    ap.add_argument("--dmg")
    ap.add_argument("--docs")
    a = ap.parse_args()

    identity = os.environ.get("MACOS_SIGN_IDENTITY") or "-"
    os.makedirs(a.out, exist_ok=True)
    app = os.path.join(a.out, "BananaV.app")
    shutil.rmtree(app, ignore_errors=True)
    contents = os.path.join(app, "Contents")
    macos = os.path.join(contents, "MacOS")
    frameworks = os.path.join(contents, "Frameworks")
    resources = os.path.join(contents, "Resources")
    for d in (macos, frameworks, resources):
        os.makedirs(d)

    # Each processor's libraries apart, then put together.
    work = tempfile.mkdtemp()
    asked = {}
    per_arch = []
    for name, program, prefix in a.arch:
        into = os.path.join(work, name)
        os.makedirs(into)
        asked.update(gather(program, os.path.abspath(prefix), into))
        per_arch.append((name, program, into))
    exe = os.path.join(macos, "BananaV")
    run("lipo", "-create", *[p for _, p, _ in per_arch], "-output", exe)
    libs = sorted({n for _, _, d in per_arch for n in os.listdir(d)})
    for lib in libs:
        have = [os.path.join(d, lib) for _, _, d in per_arch if os.path.exists(os.path.join(d, lib))]
        if len(have) != len(per_arch):
            sys.exit(f"make_app: {lib} is not built for every processor")
        run("lipo", "-create", *have, "-output", os.path.join(frameworks, lib))

    # Found from the program: @rpath, and the program's rpath its
    # Frameworks.
    targets = [exe] + [os.path.join(frameworks, lib) for lib in libs]
    for t in targets:
        changes = []
        for dep in dependencies(t):
            if dep in asked:
                changes += ["-change", dep, "@rpath/" + asked[dep]]
        if t != exe:
            changes += ["-id", "@rpath/" + os.path.basename(t)]
        if changes:
            run("install_name_tool", *changes, t)
    rpath = "@executable_path/../Frameworks"
    if rpath not in run("otool", "-l", exe, capture=True):
        run("install_name_tool", "-add_rpath", rpath, exe)
    for t in targets:
        for dep in dependencies(t):
            if not dep.startswith(SYSTEM) and not dep.startswith("@rpath/"):
                sys.exit(f"make_app: {t} still loads {dep}")

    with open(os.path.join(HERE, "Info.plist"), encoding="utf-8") as f:
        info = f.read().replace("@VERSION@", a.version)
    with open(os.path.join(contents, "Info.plist"), "w", encoding="utf-8") as f:
        f.write(info)
    plistlib.loads(info.encode("utf-8"))
    with open(os.path.join(contents, "PkgInfo"), "w", encoding="ascii") as f:
        f.write("APPL????")
    shutil.copy(os.path.join(HERE, "BananaV.icns"), resources)
    shutil.copytree(os.path.join(HERE, "pt-BR.lproj"), os.path.join(resources, "pt-BR.lproj"))

    # Signed from the inside out.
    for lib in libs:
        sign(os.path.join(frameworks, lib), identity)
    entitlements = os.path.join(HERE, "BananaV.entitlements")
    sign(exe, identity, entitlements)
    sign(app, identity, entitlements)
    run("codesign", "--verify", "--deep", "--strict", "--verbose=2", app)
    print(run("lipo", "-archs", exe, capture=True).strip())

    if a.dmg:
        stage = os.path.join(work, "dmg")
        os.makedirs(stage)
        run("ditto", app, os.path.join(stage, "BananaV.app"))
        os.symlink("/Applications", os.path.join(stage, "Applications"))
        shutil.copy(os.path.join(HERE, "README.txt"), stage)
        if a.docs:
            shutil.copytree(a.docs, os.path.join(stage, "Licenses and notices"))
        dmg = os.path.join(a.out, a.dmg)
        if os.path.exists(dmg):
            os.remove(dmg)
        run("hdiutil", "create", "-volname", f"BananaV {a.version}", "-srcfolder", stage,
            "-ov", "-fs", "HFS+", "-format", "UDZO", dmg)
        if identity != "-":
            run("codesign", "--force", "--sign", identity, "--timestamp", dmg)
            apple_id = os.environ.get("APPLE_ID")
            team = os.environ.get("APPLE_TEAM_ID")
            password = os.environ.get("APPLE_APP_PASSWORD")
            if apple_id and team and password:
                run("xcrun", "notarytool", "submit", dmg, "--apple-id", apple_id,
                    "--team-id", team, "--password", password, "--wait",
                    shown="xcrun notarytool submit (the disk image) --wait")
                run("xcrun", "stapler", "staple", dmg)
        run("hdiutil", "verify", dmg)
    shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    main()
