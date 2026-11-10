#!/usr/bin/env python3
"""Prints the crashed thread of a macOS crash report (`.ips`, what the
system writes in ~/Library/Logs/DiagnosticReports when a program dies):
the program, the exception and what the system said of it, then the
thread's frames — library and function. The CI prints those of tests that
died on GitHub's Macs.

    python3 packaging/macos/crash_report.py <report.ips>
"""

import json
import sys


def main():
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    with open(sys.argv[1], encoding="utf-8", errors="replace") as f:
        text = f.read()
    # A header line of its own, then the report.
    _, _, body = text.partition("\n")
    try:
        report = json.loads(body)
    except json.JSONDecodeError:
        print(text[:4000])
        return
    print(report.get("procName"), report.get("procPath"))
    print("exception:", report.get("exception"))
    if report.get("termination"):
        print("termination:", report["termination"])
    for key in ("asi", "crashInfo"):
        if report.get(key):
            print(f"{key}:", report[key])
    images = report.get("usedImages", [])
    threads = report.get("threads", [])
    crashed = report.get("faultingThread", 0)
    if crashed >= len(threads):
        return
    thread = threads[crashed]
    print("thread:", thread.get("name") or thread.get("queue") or crashed)
    for frame in thread.get("frames", [])[:48]:
        i = frame.get("imageIndex")
        image = images[i].get("name", "?") if isinstance(i, int) and i < len(images) else "?"
        what = frame.get("symbol") or hex(frame.get("imageOffset", 0))
        print(f"  {image}  {what}")


if __name__ == "__main__":
    main()
