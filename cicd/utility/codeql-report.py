#!/usr/bin/env python3

##	Purpose: Read the SARIF from cicd.bash's CodeQL step, print each finding,
##		and fail on any that the suppressions file doesn't list. A suppression
##		that matches nothing fails too, and so does a database with no code
##		of ours in it, since then nothing was checked.
##	Syntax: codeql-report.py --root SRC --supp FILE SARIF...
##	Suppressions: one per line, "RULE PATH LINE", where LINE is the flagged
##		line's text with its leading and trailing space dropped. Matching on
##		the text rather than the line number lets it survive edits above it.
##		A # starts a comment line. A suppression for a language no SARIF
##		given is of, by its rule's prefix, isn't counted as stale.
##	Exit: 0 = nothing found; 1 = a finding, a stale suppression, or no code;
##		2 = a file that won't read.
##	History: At bottom of script.

##	Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)
##	Licensed under The MIT License (MIT). Full text at:
##		https://mit-license.org/
##	SPDX-License-Identifier: MIT


from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path


def read_supp(path: Path) -> list[tuple[str, str, str]]:
    supp = []
    for raw in path.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        parts = line.split(None, 2)
        if len(parts) != 3:
            raise ValueError(f"{path}: not RULE PATH LINE: {raw}")
        supp.append((parts[0], parts[1], parts[2]))
    return supp


def line_text(root: Path, uri: str, num: int, cache: dict[str, list[str]]) -> str:
    if uri not in cache:
        try:
            cache[uri] = (root / uri).read_text(encoding="utf-8", errors="replace").splitlines()
        except OSError:
            cache[uri] = []
    lines = cache[uri]
    return lines[num - 1].strip() if 0 < num <= len(lines) else ""


def main() -> int:
    ap = argparse.ArgumentParser(description="Report on CodeQL SARIF, failing on what isn't suppressed.")
    ap.add_argument("--root", required=True, type=Path, help="the source root the databases were made from")
    ap.add_argument("--supp", required=True, type=Path, help="the suppressions file")
    ap.add_argument("sarif", nargs="+", type=Path)
    args = ap.parse_args()

    try:
        supp = read_supp(args.supp)
    except (OSError, ValueError) as e:
        print(f"codeql-report: {e}", file=sys.stderr)
        return 2
    used = [False] * len(supp)
    cache: dict[str, list[str]] = {}
    langs: set[str] = set()
    bad = 0

    for path in args.sarif:
        try:
            doc = json.loads(path.read_text(encoding="utf-8"))
            runs = doc["runs"]
        except (OSError, ValueError, KeyError) as e:
            print(f"codeql-report: {path}: {e}", file=sys.stderr)
            return 2
        for run in runs:
            langs.update(r.get("id", "").split("/")[0] for r in run.get("tool", {}).get("driver", {}).get("rules", []))
            metrics = run.get("properties", {}).get("metricResults", [])
            ours = [m.get("value", 0) for m in metrics if m.get("ruleId", "").endswith("/summary/lines-of-user-code")]
            found = shown = 0
            if not ours or not max(ours):
                print(f"{path.name}: no lines of our code in the database, so nothing was checked")
                bad += 1
            for res in run.get("results", []):
                rule = res.get("ruleId", "?")
                loc = (res.get("locations") or [{}])[0].get("physicalLocation", {})
                uri = loc.get("artifactLocation", {}).get("uri", "?")
                num = loc.get("region", {}).get("startLine", 0)
                text = line_text(args.root, uri, num, cache)
                found += 1
                hit = [i for i, s in enumerate(supp) if s == (rule, uri, text)]
                for i in hit:
                    used[i] = True
                if hit:
                    continue
                shown += 1
                msg = " ".join(res.get("message", {}).get("text", "").split())
                print(f"{uri}:{num}: {rule}: {msg}")
                print(f"    {text}")
            print(f"{path.name}: {max(ours, default=0)} lines of ours, {found} finding(s), {found - shown} suppressed")
            bad += shown

    for i, s in enumerate(supp):
        if not used[i] and s[0].split("/")[0] in langs:
            print(f"{args.supp.name}: matches nothing, so it can go: {' '.join(s)}")
            bad += 1
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())


##	History:
##		- 2026-11-10: Created.
