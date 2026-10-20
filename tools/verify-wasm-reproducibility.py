"""Build the shipped Wasm twice, from different places, and require identical bytes.

Each build runs packages/typescript/scripts/build-wasm.mjs in its own copy of
the checkout's files, with its own Cargo home and target directory, so every
builder path the module could embed differs between the two. The first build
uses the caller's Cargo home; the second starts from a copy of its registry
index and archives but extracts its own sources. build-wasm.mjs itself refuses
a module that embeds its builder's paths.
"""
from pathlib import Path
import argparse
import hashlib
import json
import os
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[1]
ARTIFACTS = ["openbindings_wasm.js", "openbindings_wasm_bg.wasm"]


def require(condition, message):
    if not condition:
        raise SystemExit(message)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    out = args.output.resolve()
    require(not out.is_relative_to(ROOT) or out.is_relative_to(ROOT / "target"),
            "Inside the checkout, use an ignored target/ output directory.")
    require(not out.exists(), "Use a new output directory; prior receipts are not overwritten.")
    listed = subprocess.run(["git", "ls-files", "-z", "--cached", "--others", "--exclude-standard"],
                            cwd=ROOT, check=True, capture_output=True).stdout
    files = [name for name in listed.decode().split("\0") if name and (ROOT / name).is_file()]
    home = Path(os.environ.get("CARGO_HOME", Path.home() / ".cargo")).resolve()
    second_home = out / "second-home"
    for part in ["index", "cache"]:
        if (home / "registry" / part).is_dir():
            shutil.copytree(home / "registry" / part, second_home / "registry" / part)
    builds = [("first", out / "first" / "sdk", home),
              ("second", out / "second-builder" / "checkout" / "sdk", second_home)]
    results = {}
    for label, checkout, cargo_home in builds:
        for name in files:
            (checkout / name).parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(ROOT / name, checkout / name)
        env = {**os.environ, "CARGO_HOME": str(cargo_home),
               "CARGO_TARGET_DIR": str(checkout / "target")}
        command = ["node", "packages/typescript/scripts/build-wasm.mjs"]
        print(f"+ ({label}: {checkout}, CARGO_HOME={cargo_home}) " + " ".join(command), flush=True)
        subprocess.run(command, cwd=checkout, env=env, check=True)
        results[label] = {name: hashlib.sha256(
            (checkout / "packages/typescript/src/wasm" / name).read_bytes()).hexdigest()
            for name in ARTIFACTS}
    (out / "RESULTS.json").write_text(json.dumps(results, indent=2) + "\n")
    require(results["first"] == results["second"],
            "The Wasm build depends on where it ran: " + json.dumps(results))
    print(json.dumps({"identical": True, "artifacts": results["first"]}))


if __name__ == "__main__":
    main()
