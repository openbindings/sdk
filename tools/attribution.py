#!/usr/bin/env python3
"""Regenerate the SDK's two attribution inventories without network access.

Rules inferred from the existing DEPENDENCIES.json / THIRD-PARTY-NOTICES.txt:

* The native label aarch64-apple-darwin-native-http means Cargo's platform
  aarch64-apple-darwin, workspace default features plus
  openbindings-http-discovery/native, and all workspace members as roots.
  The wasm32-unknown-unknown inventory uses workspace default features and
  openbindings-wasm as its sole root. Both use offline, locked Cargo metadata.
* Traverse every resolved normal (kind null) and build edge, even conservative
  optional edges Cargo metadata reports; exclude edges whose kinds are all dev.
  Omit authored workspace members from attribution, but include every
  reachable registry package as a distinct package identity.
* JSON keys are target, scope, packages; package keys are name, version,
  declared_license, repository, notices. Notice keys are path, sha256, retrieved_from. License and
  repository come verbatim from Cargo metadata. JSON uses two-space indentation,
  ASCII escaping, a final LF, and lexicographic (name, version) package ordering.
* Recursively select files whose case-insensitive basename starts with LICENSE,
  COPYING, COPYRIGHT, or NOTICE, sorted by relative POSIX path. Include every
  match (including embedded licenses and micromap's copyrights.yml). SHA-256
  covers original source bytes. Source notices have retrieved_from null.
* Only when a package has no matching source files, use its exact name/version
  entries in attribution/retrieved/index.json. These texts were extracted from
  the original checked-in notices, stored once by SHA-256, and retain the
  recorded immutable upstream URL. Verify the digest and pinned URL; never fetch.
* Notice sections sort lexicographically by (name, version), with the shared
  registry serde_json section last for stable inventory ordering.
* Notice text is UTF-8: universal-newline normalization converts CRLF and CR to
  LF; preserve all other whitespace, including each source's trailing spaces
  and number of final newlines. Digests remain over unnormalized bytes.
  Use the fixed preamble, 72 '=' separators, name/version,
  license/repository lines, and '--- relative/path ---' headings. Insert two LFs
  before each separator and notice heading in addition to the preceding text's
  own final newlines. The complete file ends with the final notice's bytes
  (currently one LF), without an extra newline.

The generated inventories are outputs, never dependency or notice inputs.
Run with --check to report stale files without writing any of them.
"""

import argparse
import difflib
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys


ROOT = Path(__file__).resolve().parents[1]
SCOPE = "normal and build dependency closure; development-only edges excluded"
PREAMBLE = (
    "OpenBindings SDK third-party attribution\n\n"
    "This file retains upstream license and notice texts for the selected "
    "dependency closure, including build dependencies conservatively. "
    "Declared license expressions are metadata, not a replacement for the "
    "reproduced terms.\n"
)
INVENTORIES = (
    {
        "directory": Path("."),
        "target": "aarch64-apple-darwin-native-http",
        "platform": "aarch64-apple-darwin",
        "features": ["openbindings-http-discovery/native"],
        "root": None,
    },
    {
        "directory": Path("packages/typescript"),
        "target": "wasm32-unknown-unknown",
        "platform": "wasm32-unknown-unknown",
        "features": [],
        "root": "openbindings-wasm",
    },
)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def json_bytes(value):
    return (json.dumps(value, indent=2, ensure_ascii=True) + "\n").encode("utf-8")


def normalized_text(data):
    return data.decode("utf-8").replace("\r\n", "\n").replace("\r", "\n")


def supplements():
    directory = ROOT / "tools/attribution/retrieved"
    index = json.loads((directory / "index.json").read_text(encoding="utf-8"))
    result = {}
    texts = {}
    for notice in index["notices"]:
        sha = notice["sha256"]
        url = notice["retrieved_from"]
        if not re.fullmatch(r"[0-9a-f]{64}", sha):
            raise ValueError(f"invalid supplement SHA-256: {sha}")
        if not re.fullmatch(
            r"https://raw\.githubusercontent\.com/[^/]+/[^/]+/[0-9a-f]{40}/[^\s]+",
            url,
        ):
            raise ValueError(f"supplement URL is not pinned to a commit: {url}")
        data = (directory / (sha + ".txt")).read_bytes()
        if digest(data) != sha:
            raise ValueError(f"supplement digest mismatch: {sha}.txt ({url})")
        key = (notice["name"], notice["version"])
        entry = {k: notice[k] for k in ("path", "sha256", "retrieved_from")}
        if any(n["path"] == entry["path"] for n in result.get(key, [])):
            raise ValueError(f"duplicate supplement notice: {key} {entry['path']}")
        result.setdefault(key, []).append(entry)
        texts[sha] = data
    for notices in result.values():
        notices.sort(key=lambda n: n["path"])
    return result, texts


def metadata(inventory):
    command = [
        "cargo", "metadata", "--offline", "--locked", "--format-version", "1",
        "--manifest-path", str(ROOT / "Cargo.toml"),
        "--filter-platform", inventory["platform"],
    ]
    if inventory["features"]:
        command += ["--features", ",".join(inventory["features"])]
    env = os.environ.copy()
    env["CARGO_NET_OFFLINE"] = "true"
    completed = subprocess.run(command, cwd=ROOT, env=env, capture_output=True, text=True)
    if completed.returncode:
        raise ValueError(
            f"Cargo metadata failed for {inventory['target']}:\n{completed.stderr.rstrip()}\n"
            "Inventories cover other platforms' dependencies; acquire every platform's "
            "sources first with `cargo fetch --locked`."
        )
    return json.loads(completed.stdout)


def dependency_closure(graph, inventory):
    packages = {p["id"]: p for p in graph["packages"]}
    nodes = {n["id"]: n for n in graph["resolve"]["nodes"]}
    members = set(graph["workspace_members"])
    if inventory["root"] is None:
        pending = list(members)
    else:
        pending = [i for i in members if packages[i]["name"] == inventory["root"]]
        if len(pending) != 1:
            raise ValueError(f"expected one workspace root: {inventory['root']}")
    seen = set()
    while pending:
        identity = pending.pop()
        if identity in seen:
            continue
        seen.add(identity)
        for dependency in nodes[identity]["deps"]:
            if any(kind["kind"] in (None, "build") for kind in dependency["dep_kinds"]):
                pending.append(dependency["pkg"])
    return sorted(
        (packages[i] for i in seen - members),
        key=lambda p: (p["name"], p["version"]),
    )


def package_record(package, retrieved, texts):
    source = Path(package["manifest_path"]).resolve().parent
    if package["source"] is None:
        raise ValueError(f"attributed path package is not a workspace member: {source}")
    if not package["source"].startswith("registry+"):
        raise ValueError(f"unsupported offline package source: {package['id']}")
    paths = sorted(
        (p for p in source.rglob("*")
         if p.is_file() and re.match(r"^(license|copying|copyright|notice)", p.name, re.I)),
        key=lambda p: p.relative_to(source).as_posix(),
    )
    notices = []
    bodies = {}
    for path in paths:
        relative = path.relative_to(source).as_posix()
        data = path.read_bytes()
        notices.append({"path": relative, "sha256": digest(data), "retrieved_from": None})
        bodies[relative] = normalized_text(data)
    if not notices:
        notices = retrieved.get((package["name"], package["version"]), [])
        for notice in notices:
            bodies[notice["path"]] = normalized_text(texts[notice["sha256"]])
    if not notices:
        raise ValueError(
            f"no notice texts for {package['name']} {package['version']}; "
            "provide a hash-verified, pinned offline supplement"
        )
    record = {
        "name": package["name"],
        "version": package["version"],
        "declared_license": package["license"],
        "repository": package["repository"],
        "notices": notices,
    }
    return record, bodies


def notice_order(item):
    record = item[0]
    return (record["name"] == "serde_json", record["name"], record["version"])


def render_notices(items):
    result = PREAMBLE
    for record, bodies in sorted(items, key=notice_order):
        result += "\n\n" + "=" * 72 + "\n"
        result += f"{record['name']} {record['version']}\n"
        result += f"Declared license: {record['declared_license']}\n"
        result += f"Repository: {record['repository']}\n"
        for notice in record["notices"]:
            result += f"\n\n--- {notice['path']} ---\n" + bodies[notice["path"]]
    return result.encode("utf-8")


def generate():
    retrieved, texts = supplements()
    outputs = {}
    for inventory in INVENTORIES:
        graph = metadata(inventory)
        items = [package_record(p, retrieved, texts) for p in dependency_closure(graph, inventory)]
        directory = inventory["directory"]
        outputs[directory / "DEPENDENCIES.json"] = json_bytes({
            "target": inventory["target"],
            "scope": SCOPE,
            "packages": [record for record, _ in items],
        })
        outputs[directory / "THIRD-PARTY-NOTICES.txt"] = render_notices(items)
    return outputs


def report_difference(path, actual, expected):
    if actual is None:
        print(f"STALE {path}: missing file; expected {len(expected)} bytes", file=sys.stderr)
        return
    offset = next((i for i, (a, b) in enumerate(zip(actual, expected)) if a != b), min(len(actual), len(expected)))
    line = actual[:offset].count(b"\n") + 1
    column = offset - actual.rfind(b"\n", 0, offset)
    print(
        f"STALE {path}: first difference at byte {offset} (line {line}, column {column}); "
        f"checked-in {len(actual)} bytes, expected {len(expected)} bytes",
        file=sys.stderr,
    )
    changes = list(difflib.unified_diff(
        actual.decode("utf-8", errors="replace").splitlines(keepends=True),
        expected.decode("utf-8", errors="replace").splitlines(keepends=True),
        fromfile=str(path) + " (checked-in)", tofile=str(path) + " (generated)", n=3,
    ))
    for change in changes[:80]:
        sys.stderr.write(change if change.endswith("\n") else change + "\n\\ No newline at end of file\n")
    if len(changes) > 80:
        print(f"... {len(changes) - 80} more diff lines omitted", file=sys.stderr)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="Fail if any inventory differs; do not write files")
    args = parser.parse_args()
    try:
        outputs = generate()
        stale = []
        for path, expected in outputs.items():
            destination = ROOT / path
            actual = destination.read_bytes() if destination.exists() else None
            if actual != expected:
                stale.append(path)
                if args.check:
                    report_difference(path, actual, expected)
                else:
                    destination.write_bytes(expected)
                    print(f"Wrote {path}")
        if args.check and stale:
            print(f"{len(stale)} stale attribution file(s); refresh with python3 tools/attribution.py", file=sys.stderr)
            return 1
        print("All four attribution files match the offline dependency closures.")
        return 0
    except (OSError, ValueError, KeyError) as error:
        print(f"Attribution error: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
