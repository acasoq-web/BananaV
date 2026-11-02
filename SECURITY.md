# Security policy

BananaV opens files that come from anywhere — media, projects, interchange
files, presets, archives — and loads plug-ins. A file crafted to crash it,
run code or write outside the folder it should is a security problem, and we
want to hear about it before anyone else does.

## Supported versions

BananaV is young (before 1.0): fixes go into `main` and the next release.
Only the latest release gets security fixes.

| Version | Supported |
|---|---|
| Latest release | Yes |
| `main` | Yes |
| Older releases | No: please update |

## Reporting a vulnerability

**Please do not open a public issue, discussion or pull request for a
security problem.**

Report it privately, by one of:

1. **GitHub's private vulnerability reporting**: the repository's
   **Security** tab › **Report a vulnerability**
   (<https://github.com/acasoq-web/BananaV/security/advisories/new>).
2. **A private message** to a maintainer on the
   [Discord server](https://discord.gg/vtC7Yasz98), asking for a private channel (never in a
   public channel).

Please include:

- what an attacker can do, and how (the kind of file or action);
- the BananaV version or commit, and the system (Windows or Linux, x64 or
  ARM64);
- the steps to reproduce it and, if you can, a small file that shows it;
- whether the problem is already public anywhere.

## What happens next

- We aim to answer within **7 days** that we received the report, and to keep
  you told about the fix.
- We work on the fix privately, publish it in a release and then publish an
  advisory. We credit you in the advisory, unless you prefer not to be named.
- Please give us a reasonable time (90 days at most) to release a fix before
  you disclose the problem publicly.

## Scope

In scope: BananaV's own code in this repository, including its readers and
writers of files, the OpenFX and VST 3 hosts, project archives and bundles,
and the packages built by its workflows.

Out of scope (please report these to their own projects): bugs in third-party
plug-ins themselves, in the system's codecs (Media Foundation on Windows), in
FFmpeg's libraries, in graphics drivers or in other dependencies — although
if BananaV makes such a bug reachable in a way it should not, tell us too.
