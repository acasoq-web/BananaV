"""The third-party notices that travel with BananaV's packages: every crate
linked into the program for a target (normal dependencies of `bananav`,
followed from Cargo's resolve) other than BananaV's own, with its version,
authors, license expression and the license and notice files it ships.

    python tools/third_party_notices.py <target triple> > THIRD-PARTY-NOTICES.txt

Run it after the build (Cargo has the crates' sources then). The licenses
MIT, BSD, ISC, Zlib and Apache-2.0 ask for their notice beside the binary;
this file is that notice.
"""

import json
import os
import subprocess
import sys

LICENSE_FILES = ("LICENSE", "LICENCE", "COPYING", "NOTICE", "UNLICENSE", "COPYRIGHT", "AUTHORS")


def metadata(target):
    out = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--locked", "--filter-platform", target],
        check=True,
        capture_output=True,
    ).stdout
    return json.loads(out)


def shipped(meta):
    """The packages linked into `bananav`: its normal dependencies, down."""
    nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
    root = next(p["id"] for p in meta["packages"] if p["name"] == "bananav")
    seen, todo = set(), [root]
    while todo:
        pid = todo.pop()
        if pid in seen:
            continue
        seen.add(pid)
        for dep in nodes[pid]["deps"]:
            if any(k["kind"] is None for k in dep["dep_kinds"]):
                todo.append(dep["pkg"])
    return seen


def own(pkg, workspace_root):
    """BananaV's own crates (under the GPL): the workspace's, not vendor/."""
    # Compared as text: crates from the registry may be on another drive,
    # where a relative path does not exist.
    path = os.path.normcase(os.path.abspath(os.path.dirname(pkg["manifest_path"])))
    vendor = os.path.normcase(os.path.abspath(os.path.join(workspace_root, "vendor")))
    return pkg["source"] is None and not (path + os.sep).startswith(vendor + os.sep)


def license_texts(pkg):
    folder = os.path.dirname(pkg["manifest_path"])
    texts = []
    for name in sorted(os.listdir(folder)):
        full = os.path.join(folder, name)
        if os.path.isfile(full) and name.upper().startswith(LICENSE_FILES):
            with open(full, encoding="utf-8", errors="replace") as f:
                texts.append((name, f.read().strip()))
    if pkg.get("license_file"):
        full = os.path.join(folder, pkg["license_file"])
        if os.path.isfile(full) and not any(n == os.path.basename(full) for n, _ in texts):
            with open(full, encoding="utf-8", errors="replace") as f:
                texts.append((os.path.basename(full), f.read().strip()))
    return texts


def main():
    target = sys.argv[1]
    meta = metadata(target)
    ids = shipped(meta)
    pkgs = sorted(
        (p for p in meta["packages"] if p["id"] in ids and not own(p, meta["workspace_root"])),
        key=lambda p: (p["name"].lower(), p["version"]),
    )
    out = sys.stdout
    out.reconfigure(encoding="utf-8", newline="\n")
    out.write("BananaV — third-party notices\n")
    out.write("=" * 29 + "\n\n")
    out.write(
        "BananaV is free software under the GNU General Public License, version 3 or\n"
        "later (LICENSE.txt). It is built with the open-source libraries below, each\n"
        "under its own license, whose notices and texts follow. The fonts, color and\n"
        "transition code BananaV ships have their own files beside this one.\n\n"
        f"Target: {target}. {len(pkgs)} libraries.\n\n"
    )
    # Vendored files that are not crates: the OpenFX headers bv-ofx-sys
    # declares its API from.
    openfx = os.path.join(meta["workspace_root"], "vendor", "openfx", "LICENSE.md")
    if os.path.isfile(openfx):
        with open(openfx, encoding="utf-8") as f:
            out.write("-" * 78 + "\nOpenFX 1.5 headers (vendor/openfx)\nLicense: BSD-3-Clause\n")
            out.write(f"Source: https://github.com/AcademySoftwareFoundation/openfx\n\n--- LICENSE.md ---\n{f.read().strip()}\n\n")
    for p in pkgs:
        out.write("-" * 78 + "\n")
        out.write(f"{p['name']} {p['version']}\n")
        out.write(f"License: {p.get('license') or 'see the files below'}\n")
        if p.get("authors"):
            out.write(f"Authors: {', '.join(p['authors'])}\n")
        if p.get("repository"):
            out.write(f"Source: {p['repository']}\n")
        texts = license_texts(p)
        if not texts:
            out.write("(No license file in the package: the license above applies, in its standard text.)\n")
        for name, text in texts:
            out.write(f"\n--- {name} ---\n{text}\n")
        out.write("\n")


if __name__ == "__main__":
    main()
