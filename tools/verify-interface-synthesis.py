"""Qualify an external synthesis provider/consumer; optionally use Cargo archives and Wasm.

No publication, network service, or registry access is required with cached tools.
Outputs use the shared target tree; the source fixture contains version dependencies
only. The generated patch table is local qualification plumbing.
"""
from pathlib import Path
import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import tarfile
import tomllib

ROOT = Path(__file__).resolve().parents[1]
FIXTURE = ROOT / "examples/interface-synthesis-consumer"
LOCAL = [
    "crates/openbindings", "crates/openbindings-internal-json",
    "crates/openbindings-internal-cancellation", "crates/openbindings-internal-schema",
    "crates/openbindings-interface-synthesis", "crates/openbindings-source-inspection",
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
    parser.add_argument("--performance", action="store_true", help="Run fixed release workload measurements")
    parser.add_argument("--wasm", action="store_true", help="Run the narrow TypeScript/Node Wasm probe")
    parser.add_argument("--tsc", type=Path, default=ROOT / "packages/typescript/node_modules/typescript/bin/tsc")
    args = parser.parse_args()
    env = os.environ.copy()
    target = Path(env.get("CARGO_TARGET_DIR", ROOT / "target")).resolve()
    env["CARGO_TARGET_DIR"] = str(target)
    out = target / "interface-synthesis-qualification" / ("archives" if args.archives else "source")
    out.mkdir(parents=True, exist_ok=True)
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

    sources = {}
    hashes = {}
    version = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding='utf-8'))["workspace"]["package"]["version"]
    for local in LOCAL:
        source = ROOT / local
        package = tomllib.loads((source / "Cargo.toml").read_text(encoding='utf-8'))["package"]
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
            normalized = tomllib.loads((source / "Cargo.toml").read_text(encoding='utf-8'))
            for section in [normalized, *normalized.get("target", {}).values()]:
                for kind in ["dependencies", "dev-dependencies", "build-dependencies"]:
                    for spec in section.get(kind, {}).values():
                        if isinstance(spec, dict) and "path" in spec:
                            raise SystemExit(f"Packaged path dependency: {name}")
        sources[name] = source

    consumer = out / "consumer"
    consumer.mkdir(exist_ok=True)
    for directory in ["src", "tests", "examples"]:
        shutil.copytree(FIXTURE / directory, consumer / directory, dirs_exist_ok=True)
    patches = "\n[patch.crates-io]\n" + "".join(f"{name}={{path={json.dumps(str(source))}}}\n" for name, source in sources.items())
    (consumer / "Cargo.toml").write_text((FIXTURE / "Cargo.toml").read_text(encoding='utf-8') + patches)
    shutil.copyfile(ROOT / "Cargo.lock", consumer / "Cargo.lock")
    # Preserve the root lock's selected versions, adding only the external fixture.
    run("consumer-test", ["cargo", "test"], consumer)
    run("consumer-recovery", ["cargo", "run", "--locked", "--example", "recovery"], consumer)
    run("consumer-lint", ["cargo", "clippy", "--locked", "--all-targets", "--", "-D", "warnings"], consumer)
    metadata = json.loads(run("consumer-metadata", ["cargo", "metadata", "--locked", "--format-version", "1"], consumer))
    for package in metadata["packages"]:
        path = Path(package["manifest_path"]).resolve().parent
        if package["name"] in sources and path != sources[package["name"]]:
            raise SystemExit("Consumer resolved an unexpected local dependency")
        if args.archives and path.is_relative_to(ROOT) and not path.is_relative_to(out):
            raise SystemExit("Archive consumer resolved checkout source")

    standalone = out / "synthesis-only"
    shutil.copytree(FIXTURE / "synthesis-only", standalone, dirs_exist_ok=True)
    standalone_patches = "\n[patch.crates-io]\n" + "".join(
        f"{name}={{path={json.dumps(str(source))}}}\n" for name, source in sources.items()
        if name != "openbindings-source-inspection")
    (standalone / "Cargo.toml").write_text((FIXTURE / "synthesis-only/Cargo.toml").read_text(encoding='utf-8') + standalone_patches)
    shutil.copyfile(ROOT / "Cargo.lock", standalone / "Cargo.lock")
    run("synthesis-only", ["cargo", "test"], standalone)
    isolated = json.loads(run("synthesis-only-metadata", ["cargo", "metadata", "--locked", "--format-version", "1"], standalone))
    if any(p["name"] == "openbindings-source-inspection" for p in isolated["packages"]):
        raise SystemExit("Synthesis-only consumer depends on inspection")

    if args.wasm:
        if not args.tsc.is_file():
            raise SystemExit("Install the repository TypeScript tools, or pass --tsc path/to/typescript/bin/tsc")
        run("wasm-build", ["cargo", "build", "--locked", "--lib", "--target", "wasm32-unknown-unknown", "--features", "wasm-probe"], consumer)
        host = out / "host"
        host.mkdir(exist_ok=True)
        run("wasm-bindgen", ["wasm-bindgen", "--target", "nodejs", "--out-dir", host / "wasm", target / "wasm32-unknown-unknown/debug/interface_synthesis_consumer.wasm"])
        shutil.copyfile(FIXTURE / "wasm-probe.ts", host / "wasm-probe.ts")
        run("typescript", ["node", args.tsc.resolve(), "--strict", "--target", "ES2022", "--module", "commonjs", "--lib", "ES2022,DOM", host / "wasm-probe.ts"])
        run("wasm-host", ["node", host / "wasm-probe.js"])

    if args.performance:
        run("performance", ["cargo", "run", "--locked", "--release", "--example", "measure"], consumer)

    receipt = {
        "head": run("head", ["git", "rev-parse", "HEAD"]).strip(),
        "interfacesRevision": re.search(r'APPLIED_INTERFACE_REVISION: &str =\s*"([0-9a-f]{40})"',
            (ROOT / "crates/openbindings-interface-synthesis/src/lib.rs").read_text(encoding='utf-8')).group(1),
        "dirty": bool(run("status", ["git", "status", "--porcelain"]).strip()),
        "archives": hashes, "wasmProbe": args.wasm, "performance": args.performance,
        "rustc": run("rustc", ["rustc", "--version"]).strip(),
        "commands": commands,
    }
    (out / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
    print(f"Passed; receipt: {out / 'receipt.json'}")


if __name__ == "__main__":
    main()
