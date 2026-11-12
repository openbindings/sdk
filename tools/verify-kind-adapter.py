"""Compile, test and lint the external guide adapter with pinned checkout dependencies."""
from pathlib import Path
import argparse
import json
import os
import re
import shutil
import subprocess
from consumer_qualification import verify_resolution

ROOT = Path(__file__).resolve().parents[1]
FIXTURE = ROOT / "examples/kind-adapter"
LOCAL = [
    "openbindings", "openbindings-internal-json", "openbindings-internal-schema",
    "openbindings-internal-cancellation", "openbindings-context",
    "openbindings-invocation", "openbindings-source-pull", "openbindings-kind-description",
]


def check_snippets():
    guide = (ROOT / "docs/writing-a-kind.md").read_text()
    snippets = re.findall(r'<!-- guide:([a-z]+) -->\n```rust\n(.*?)```', guide, re.S)
    expected = {"setup", "invoke", "describe", "pull", "application", "agreement", "partial", "proposals", "updates"}
    assert {name for name, _ in snippets} == expected and len(snippets) == len(expected)
    assert len(re.findall(r'^```rust\s*$', guide, re.M)) == len(snippets), "unverified Rust fence"
    for name, body in snippets:
        source = FIXTURE / "src/lib.rs"
        if name in {"application", "agreement"}:
            source = FIXTURE / "tests/adapter.rs"
        elif name in {"partial", "proposals", "updates"}:
            source = ROOT / "examples/source-pull-consumer/src/using_the_result.rs"
        match = re.search(rf'// guide:{name}:start\n(.*?)// guide:{name}:end', source.read_text(), re.S)
        assert match and match[1] == body, f"guide snippet differs from compiled source: {name}"
    print(f"Guide: {len(snippets)} verbatim compiled snippets", flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--wasm", action="store_true", help="Also build the library for wasm32-unknown-unknown")
    args = parser.parse_args()
    env = os.environ.copy()
    target = Path(env.get("CARGO_TARGET_DIR", ROOT / "target")).resolve()
    out = target / "kind-adapter-qualification"
    consumer = out / "consumer"
    consumer.mkdir(parents=True, exist_ok=True)
    # A failed rerun must not leave an earlier success receipt in place.
    (out / "receipt.json").unlink(missing_ok=True)
    check_snippets()
    env["CARGO_TARGET_DIR"] = str(out / "build")
    env["CARGO_NET_OFFLINE"] = "true"
    for directory in ["src", "tests"]:
        shutil.rmtree(consumer / directory, ignore_errors=True)
        shutil.copytree(FIXTURE / directory, consumer / directory)
    sources = {name: ROOT / "crates" / name for name in LOCAL}
    patches = "\n[patch.crates-io]\n" + "".join(
        f"{name}={{path={json.dumps(str(path))}}}\n" for name, path in sources.items())
    (consumer / "Cargo.toml").write_text((FIXTURE / "Cargo.toml").read_text() + patches)
    shutil.copyfile(FIXTURE / "Cargo.lock", consumer / "Cargo.lock")
    commands = []

    def run(label, command):
        print("+ " + " ".join(command), flush=True)
        result = subprocess.run(command, cwd=consumer, env=env, text=True, capture_output=True)
        (out / f"{label}.log").write_text(result.stdout + result.stderr)
        commands.append({"command": command, "exit": result.returncode})
        (out / "commands.json").write_text(json.dumps(commands, indent=2) + "\n")
        if result.returncode:
            raise SystemExit(result.stdout + result.stderr)
        return result.stdout

    run("format", ["cargo", "fmt", "--all", "--check"])
    run("test", ["cargo", "test", "--offline", "--locked"])
    run("lint", ["cargo", "clippy", "--offline", "--locked", "--all-targets", "--", "-D", "warnings"])
    metadata = json.loads(run("metadata", ["cargo", "metadata", "--offline", "--locked", "--format-version", "1"]))
    lock_hash = verify_resolution(ROOT, metadata, consumer, sources, {"kind-adapter": consumer}, False, out)
    if args.wasm:
        run("wasm", ["cargo", "build", "--offline", "--locked", "--lib", "--target", "wasm32-unknown-unknown"])
    (out / "receipt.json").write_text(json.dumps({
        "consumerLockSha256": lock_hash, "wasmBuild": args.wasm, "commands": commands,
    }, indent=2) + "\n")
    print(f"Passed; receipt: {out / 'receipt.json'}")


if __name__ == "__main__":
    main()
