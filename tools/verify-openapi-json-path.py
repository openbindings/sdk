"""Qualify the experimental OpenAPI adapter and public consumer; optionally use Cargo archives and Wasm.

No publication, network service, or registry access is required with cached tools.
Outputs use the shared target tree. SDK dependencies use package versions; the
independent client uses an exact git revision. Patches are qualification plumbing.
"""
from pathlib import Path
import argparse
import hashlib
import json
import os
import shutil
import subprocess
import tarfile
import tomllib
import time

ROOT = Path(__file__).resolve().parents[1]
FIXTURE = ROOT / "examples/openapi-json-path-consumer"
CLIENT_URL = "https://github.com/openbindings/openapi-client"
CLIENT_REV = "fe09522a9b99a6c6b4918e3de0467b5a638afa5e"


def check_client_pin(manifest):
    spec = tomllib.loads(manifest.read_text())["dependencies"]["dynamic-openapi-client"]
    if spec.get("git") != CLIENT_URL or spec.get("rev") != CLIENT_REV or "path" in spec:
        raise SystemExit("Product/consumer client dependency is not the qualified git revision")

LOCAL = [
    "crates/openbindings", "crates/openbindings-internal-json",
    "crates/openbindings-source-inspection", "crates/openbindings-openapi",
    "crates/openbindings-interface-synthesis", "crates/openbindings-invocation", "crates/openbindings-json-schema-evaluator",
    "vendor/openbindings-internal-jsonschema",
    "vendor/openbindings-internal-jsonschema-value",
    "vendor/openbindings-internal-serde-json",
    "vendor/openbindings-internal-referencing",
    "vendor/openbindings-internal-regress",
]


def extract_fresh(archive, destination):
    with tarfile.open(archive) as packed:
        packed.extractall(destination, filter="data")
        # Cargo archives normalize source timestamps into the past. Re-extracting
        # a changed package at the same path can otherwise reuse an older build
        # from the shared target tree. Make every extracted input freshly visible.
        for member in packed.getmembers():
            if member.isfile():
                os.utime(destination / member.name, None)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--archives", action="store_true", help="Compile extracted .crate sources, including uncommitted work")
    parser.add_argument("--wasm", action="store_true", help="Run the narrow TypeScript/Node Wasm probe")
    parser.add_argument("--tsc", type=Path, default=ROOT / "packages/typescript/node_modules/typescript/bin/tsc")
    args = parser.parse_args()
    env = os.environ.copy()
    target = Path(env.get("CARGO_TARGET_DIR", ROOT / "target")).resolve()
    env["CARGO_TARGET_DIR"] = str(target)
    out = target / "openapi-json-path-qualification" / ("archives" if args.archives else "source")
    out.mkdir(parents=True, exist_ok=True)
    prior = [*out.glob("*.log"), *out.glob("*.json")]
    if prior:
        history = out / "history" / str(time.time_ns())
        history.mkdir(parents=True)
        for path in prior:
            shutil.copyfile(path, history / path.name)
    def input_hashes():
        files = [*FIXTURE.rglob("*"), *(ROOT / "crates/openbindings-openapi").rglob("*"), ROOT / "Cargo.lock", Path(__file__)]
        return {str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(files) if p.is_file()}
    started_inputs = input_hashes()
    # A receipt certifies this completed run, never an earlier run at the same
    # revision. Invalidate it before any operation that can fail.
    (out / "receipt.json").unlink(missing_ok=True)
    commands = []
    (out / "commands.json").write_text("[]\n")

    def run(label, command, cwd=ROOT):
        print("+ " + " ".join(map(str, command)), flush=True)
        result = subprocess.run(list(map(str, command)), cwd=cwd, env=env, text=True, capture_output=True)
        (out / f"{label}.log").write_text(result.stdout + result.stderr)
        commands.append({"label": label, "command": list(map(str, command)), "exit": result.returncode})
        (out / "commands.json").write_text(json.dumps(commands, indent=2) + "\n")
        if result.returncode:
            print(result.stdout + result.stderr, flush=True)
            raise SystemExit(f"Failed: {out / (label + '.log')}")
        return result.stdout

    run("adapter-tests", ["cargo", "test", "--locked", "-p", "openbindings-openapi", "-p", "openbindings-source-inspection", "-p", "openbindings-interface-synthesis", "-p", "openbindings-invocation"])
    run("adapter-lint", ["cargo", "clippy", "--locked", "-p", "openbindings-openapi", "--all-targets", "--", "-D", "warnings"])
    check_client_pin(ROOT / "crates/openbindings-openapi/Cargo.toml")
    check_client_pin(FIXTURE / "Cargo.toml")
    sources = {}
    hashes = {}
    version = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]["package"]["version"]
    for local in LOCAL:
        source = ROOT / local
        package = tomllib.loads((source / "Cargo.toml").read_text())["package"]
        name = package["name"]
        if args.archives:
            run("package-" + name, ["cargo", "package", "--locked", "--allow-dirty", "--no-verify", "--exclude-lockfile", "--manifest-path", source / "Cargo.toml"])
            package_version = package["version"] if isinstance(package["version"], str) else version
            archive = target / "package" / f"{name}-{package_version}.crate"
            hashes[archive.name] = hashlib.sha256(archive.read_bytes()).hexdigest()
            source = out / "cargo" / archive.stem
            if source.exists():
                shutil.rmtree(source)
            extract_fresh(archive, out / "cargo")
            if name == "openbindings-openapi":
                check_client_pin(source / "Cargo.toml.orig")
            normalized = tomllib.loads((source / "Cargo.toml").read_text())
            for section in [normalized, *normalized.get("target", {}).values()]:
                for kind in ["dependencies", "dev-dependencies", "build-dependencies"]:
                    for spec in section.get(kind, {}).values():
                        if isinstance(spec, dict) and "path" in spec:
                            raise SystemExit(f"Packaged path dependency: {name}")
        sources[name] = source

    consumer = out / "consumer"
    consumer.mkdir(exist_ok=True)
    for directory in ["src", "tests", "examples"]:
        if (consumer / directory).exists():
            shutil.rmtree(consumer / directory)
        shutil.copytree(FIXTURE / directory, consumer / directory)
    patches = "\n[patch.crates-io]\n" + "".join(f"{name}={{path={json.dumps(str(source))}}}\n" for name, source in sources.items())
    patches += f'dynamic-openapi-client={{git="{CLIENT_URL}",rev="{CLIENT_REV}"}}\n'
    (consumer / "Cargo.toml").write_text((FIXTURE / "Cargo.toml").read_text() + patches)
    shutil.copyfile(ROOT / "Cargo.lock", consumer / "Cargo.lock")
    # Preserve the root lock's selected versions, adding only the external fixture.
    run("consumer-test", ["cargo", "test", "--", "--test-threads=1"], consumer)
    run("consumer-lint", ["cargo", "clippy", "--locked", "--lib", "--tests", "--", "-D", "warnings"], consumer)
    for example in sorted((consumer / "examples").glob("*.rs")):
        lints = ["-D", "warnings"]
        if example.stem == "auth_resolution":
            # Preserve the frozen, stopped measurement protocol byte-for-byte.
            # Its labelled closures and fixed experimental coordinates trigger
            # two style lints. These exceptions apply only to this target; all
            # product/library/test targets keep warnings-as-errors. Do not run it.
            lints += ["-A", "clippy::type_complexity", "-A", "clippy::too_many_arguments"]
        run("consumer-lint-" + example.stem, ["cargo", "clippy", "--locked", "--example", example.stem, "--", *lints], consumer)
    metadata = json.loads(run("consumer-metadata", ["cargo", "metadata", "--locked", "--format-version", "1"], consumer))
    client = [p for p in metadata["packages"] if p["name"] == "dynamic-openapi-client"]
    if len(client) != 1 or not client[0].get("source", "").endswith(f"#{CLIENT_REV}"):
        raise SystemExit("Consumer did not resolve the pinned git client")
    core_tree = run("core-isolation", ["cargo", "tree", "--locked", "-p", "openbindings", "--edges", "normal"])
    if "dynamic-openapi-client" in core_tree or "openbindings-openapi" in core_tree:
        raise SystemExit("OpenAPI dependency leaked into core")
    for package in metadata["packages"]:
        path = Path(package["manifest_path"]).resolve().parent
        if package["name"] in sources and path != sources[package["name"]]:
            raise SystemExit("Consumer resolved an unexpected local dependency")
        if args.archives and path.is_relative_to(ROOT) and not path.is_relative_to(out):
            raise SystemExit("Archive consumer resolved checkout source")

    if args.wasm:
        if not args.tsc.is_file():
            raise SystemExit("Install the repository TypeScript tools, or pass --tsc path/to/typescript/bin/tsc")
        run("wasm-build", ["cargo", "build", "--locked", "--lib", "--target", "wasm32-unknown-unknown", "--features", "wasm-probe"], consumer)
        host = out / "host"
        host.mkdir(exist_ok=True)
        run("wasm-bindgen", ["wasm-bindgen", "--target", "nodejs", "--out-dir", host / "wasm", target / "wasm32-unknown-unknown/debug/openapi_json_path_consumer.wasm"])
        shutil.copyfile(FIXTURE / "wasm-probe.ts", host / "wasm-probe.ts")
        run("typescript", ["node", args.tsc.resolve(), "--strict", "--target", "ES2022", "--module", "commonjs", "--lib", "ES2022,DOM", host / "wasm-probe.ts"])
        run("wasm-host", ["node", host / "wasm-probe.js"])

    fixture_hashes = {str(p.relative_to(FIXTURE)): hashlib.sha256(p.read_bytes()).hexdigest()
                      for p in sorted(FIXTURE.rglob("*")) if p.is_file() and p.suffix in {".rs", ".ts", ".toml"}}
    product_hashes = {str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest()
                      for p in sorted((ROOT / "crates/openbindings-openapi").rglob("*")) if p.is_file() and p.suffix in {".rs", ".toml", ".json", ".yaml"}}
    if input_hashes() != started_inputs:
        raise SystemExit("Qualification inputs changed during this run; no receipt written")
    receipt = {
        "testedInputs": started_inputs,
        "consumerLockSha256": hashlib.sha256((consumer / "Cargo.lock").read_bytes()).hexdigest(),
        "head": run("head", ["git", "rev-parse", "HEAD"]).strip(),
        "dirty": bool(run("status", ["git", "status", "--porcelain"]).strip()),
        "archives": hashes, "wasmProbe": args.wasm, "clientSource": client[0]["source"],
        "consumerFiles": fixture_hashes, "productFiles": product_hashes,
        "rustc": run("rustc", ["rustc", "--version"]).strip(),
        "commands": commands,
    }
    (out / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
    print(f"Passed; receipt: {out / 'receipt.json'}")


if __name__ == "__main__":
    main()
