"""Verify local Cargo/npm release archives from a clean source; never publish."""
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
PUBLIC_CRATES = {
    "openbindings", "openbindings-json-schema-evaluator",
    "openbindings-http-discovery", "openbindings-schema-evaluator-test-support",
}


def require(condition, message):
    if not condition:
        raise SystemExit(message)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def publication_order(packages):
    """Order the local dependency closure of supported libraries, excluding dev deps."""
    by_name = {item["name"]: item for item in packages}
    dependencies = {}
    for item in packages:
        manifest = tomllib.loads((Path(item["source"]) / "Cargo.toml").read_text())
        sections = [manifest, *manifest.get("target", {}).values()]
        names = set()
        for section in sections:
            for kind in ["dependencies", "build-dependencies"]:
                for alias, spec in section.get(kind, {}).items():
                    name = spec.get("package", alias) if isinstance(spec, dict) else alias
                    if name in by_name:
                        names.add(name)
        dependencies[item["name"]] = names
    result, active = [], set()

    def visit(name):
        if name in result:
            return
        require(name not in active, f"Local publication dependency cycle: {name}")
        active.add(name)
        for dependency in sorted(dependencies[name]):
            visit(dependency)
        active.remove(name)
        result.append(name)

    for name in sorted(PUBLIC_CRATES):
        visit(name)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--offline", action="store_true", help="Require cached Cargo dependencies")
    args = parser.parse_args()
    out = args.output.resolve()
    require(not out.is_relative_to(ROOT) or out.is_relative_to(ROOT / "target"),
            "Inside the checkout, use an ignored target/ output directory.")
    require(not out.exists(), "Use a new output directory; prior receipts are not overwritten.")
    out.mkdir(parents=True)
    env = os.environ.copy()
    env.setdefault("CARGO_TARGET_DIR", str(ROOT / "target"))
    target = Path(env["CARGO_TARGET_DIR"]).resolve()
    npm = "npm.cmd" if os.name == "nt" else "npm"
    offline = ["--offline"] if args.offline else []
    commands = []

    def run(label, arguments, cwd=ROOT):
        result = subprocess.run(list(map(str, arguments)), cwd=cwd, env=env, text=True, capture_output=True)
        log = out / (label + ".log")
        log.write_text(result.stdout + result.stderr)
        commands.append({"name": label, "command": list(map(str, arguments)),
                         "exit": result.returncode, "logSha256": digest(log)})
        (out / "COMMANDS.json").write_text(json.dumps(commands, indent=2) + "\n")
        require(result.returncode == 0, f"{label} failed; inspect {log}")
        return result.stdout

    require(not run("source-status", ["git", "status", "--porcelain"]).strip(), "Commit source changes before packaging.")
    commit = run("source-head", ["git", "rev-parse", "HEAD"]).strip()
    toolchain = run("rustc", ["rustc", "-vV"])
    node_version = run("node-version", ["node", "--version"]).strip()
    npm_version = run("npm-version", [npm, "--version"]).strip()
    host = next(line.split(": ", 1)[1] for line in toolchain.splitlines() if line.startswith("host: "))
    run("npm-build", [npm, "run", "build"], ROOT / "packages/typescript")
    packed = json.loads(run("npm-pack", [npm, "pack", "--json", "--pack-destination", out], ROOT / "packages/typescript"))[0]
    npm_archive = out / packed["filename"]
    with tarfile.open(npm_archive) as archive:
        archive.extractall(out / "npm", filter="data")
    npm_root = out / "npm/package"
    identity = json.loads((npm_root / "dist/wasm/build-info.json").read_text())
    require(identity["commit"] == commit and identity["treeClean"], "Wasm was not built from this clean candidate.")
    for name, expected in identity["artifacts"].items():
        require(digest(npm_root / "dist/wasm" / name) == expected, f"Wasm artifact mismatch: {name}")
    version = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]["package"]["version"]
    packages = []
    for manifest in sorted([*ROOT.glob("crates/*/Cargo.toml"), *ROOT.glob("vendor/*/Cargo.toml")]):
        spec = tomllib.loads(manifest.read_text())["package"]
        name = spec["name"]
        package_version = spec["version"] if isinstance(spec["version"], str) else version
        run("package-" + name, ["cargo", "package", "--locked", *offline, "--no-verify", "--exclude-lockfile", "--manifest-path", manifest])
        archive_path = out / f"{name}-{package_version}.crate"
        shutil.copyfile(target / "package" / archive_path.name, archive_path)
        with tarfile.open(archive_path) as archive:
            archive.extractall(out / "cargo", filter="data")
            source = out / "cargo" / archive_path.name.removesuffix(".crate")
            require(json.loads((source / ".cargo_vcs_info.json").read_text())["git"]["sha1"] == commit, f"Stale Cargo source: {name}")
            for member in archive.getmembers():
                if not member.isfile():
                    continue
                relative = Path(member.name).relative_to(member.name.split("/")[0])
                if str(relative) in {".cargo_vcs_info.json", "Cargo.lock", "Cargo.toml"}:
                    continue
                original = manifest.parent / ("Cargo.toml" if str(relative) == "Cargo.toml.orig" else relative)
                require(original.is_file() and original.read_bytes() == archive.extractfile(member).read(), f"Archive/source mismatch: {name}/{relative}")
        normalized = tomllib.loads((source / "Cargo.toml").read_text())
        for section in [normalized, *normalized.get("target", {}).values()]:
            for kind in ["dependencies", "dev-dependencies", "build-dependencies"]:
                for dependency in section.get(kind, {}).values():
                    require(not isinstance(dependency, dict) or "path" not in dependency, f"Development dependency path remains: {name}")
        packages.append({"name": name, "version": package_version, "archive": archive_path.name,
                         "sha256": digest(archive_path), "source": str(source)})
    patches = "\n[patch.crates-io]\n" + "".join(f'{item["name"]}={{path={json.dumps(item["source"])}}}\n' for item in packages)
    consumer = out / "cargo-consumer"
    (consumer / "src/bin").mkdir(parents=True)
    (consumer / "Cargo.toml").write_text(
        '[package]\nname="sdk-release-consumer"\nversion="0.0.0"\nedition="2024"\n[workspace]\n[dependencies]\n'
        f'openbindings="={version}"\nopenbindings-json-schema-evaluator="={version}"\n'
        'serde={version="1.0.229",features=["derive"]}\nserde_json="=1.0.151"\n' + patches)
    shutil.copyfile(ROOT / "Cargo.lock", consumer / "Cargo.lock")
    evaluator = Path(next(item["source"] for item in packages if item["name"] == "openbindings-json-schema-evaluator"))
    examples = ["first_use", "replacement"]
    for name in examples:
        shutil.copyfile(evaluator / "examples" / (name + ".rs"), consumer / "src/bin" / (name + ".rs"))
    core = Path(next(item["source"] for item in packages if item["name"] == "openbindings"))
    shutil.copyfile(core / "examples/exact_edit.rs", consumer / "src/bin/exact_edit.rs")
    examples.append("exact_edit")
    snippets = re.findall(rb"^```rust\r?\n(.*?)^```[ \t]*\r?$", (evaluator / "README.md").read_bytes(), re.M | re.S)
    require(bool(snippets), "No executable evaluator README example found.")
    for i, snippet in enumerate(snippets):
        name = f"readme_{i}"
        (consumer / "src/bin" / (name + ".rs")).write_bytes(snippet)
        examples.append(name)
    for name in examples:
        run("archive-" + name, ["cargo", "run", *offline, "--bin", name], consumer)
    run("archive-exact-edit-tests", ["cargo", "test", *offline, "--bin", "exact_edit"], consumer)
    compat = out / "compat-consumer"
    (compat / "src").mkdir(parents=True)
    versions = {item["name"]: item["version"] for item in packages}
    manifest = (ROOT / "tools/consumer-compat/Cargo.toml").read_text()
    manifest = re.sub(r'path = "../../(?:crates|vendor)/([^\"]+)"', lambda m: 'version = "=' + versions[m[1]] + '"', manifest)
    require("path =" not in manifest, "Compatibility fixture still references the source checkout.")
    (compat / "Cargo.toml").write_text(manifest + patches)
    shutil.copyfile(ROOT / "tools/consumer-compat/src/lib.rs", compat / "src/lib.rs")
    shutil.copyfile(ROOT / "tools/consumer-compat/Cargo.lock", compat / "Cargo.lock")
    for features in ["", "ap", "sdk", "sdk,ap", "evaluator", "native", "native,ap"]:
        feature_args = ["--features", features] if features else []
        label = features.replace(",", "-") or "baseline"
        run("compat-" + label, ["cargo", "test", *offline, "--no-default-features", *feature_args], compat)
        run("features-" + label, ["cargo", "tree", *offline, "--no-default-features", *feature_args, "-e", "features", "-i", "serde_json"], compat)
    for label, path, features in [("examples", consumer, []), ("compat", compat, ["--features", "native,ap"])]:
        metadata = json.loads(run("metadata-" + label, ["cargo", "metadata", "--locked", *offline, "--format-version", "1", "--filter-platform", host, *features], path))
        expected = {item["name"]: Path(item["source"]) for item in packages}
        for dependency in metadata["packages"]:
            resolved = Path(dependency["manifest_path"]).resolve()
            require(not resolved.is_relative_to(ROOT) or resolved.is_relative_to(out),
                    "Archive consumer resolves a checkout dependency.")
            if dependency["name"] in expected:
                require(resolved.parent == expected[dependency["name"]], "Archive consumer resolves the wrong package extraction.")
    node = out / "node-consumer"
    node.mkdir()
    (node / "package.json").write_text('{"name":"sdk-release-consumer","private":true,"type":"module"}\n')
    run("node-install", [npm, "install", "--ignore-scripts", "--no-audit", "--no-fund", npm_archive], node)
    require(not (node / "node_modules/@openbindings/sdk").is_symlink(), "npm consumer must not be symlinked.")
    run("node-first-use", ["node", "node_modules/@openbindings/sdk/examples/first-use-node.mjs"], node)
    require(run("source-head-after", ["git", "rev-parse", "HEAD"]).strip() == commit, "Candidate changed during packaging.")
    require(not run("source-status-after", ["git", "status", "--porcelain"]).strip(), "Source changed during packaging.")
    spec_inventory = ROOT / "tools/qualification/inputs/blueprint/case-inventory.json"
    spec = json.loads(spec_inventory.read_text())
    receipt = {"commit": commit, "version": version, "rustc": toolchain,
               "node": node_version, "npmVersion": npm_version,
               "specification": {"commit": spec["spec_commit"], "cases": spec["count"],
                                 "inventorySha256": digest(spec_inventory)},
               "dependencyProvenanceSha256": digest(ROOT / "docs/dependency-patches/manifest.json"),
               "workspaceLockSha256": digest(ROOT / "Cargo.lock"),
               "npm": {"archive": npm_archive.name, "sha256": digest(npm_archive), "buildIdentity": identity},
               "cargo": packages, "publicationOrder": publication_order(packages),
               "cargoBuildOnly": ["openbindings-wasm"], "archiveConsumers": "passed",
               "status": "local artifacts verified; publication not performed"}
    (out / "MANIFEST.json").write_text(json.dumps(receipt, indent=2) + "\n")
    print(json.dumps({"manifest": str(out / "MANIFEST.json"), "cargoArchives": len(packages), "publicationPerformed": False}))


if __name__ == "__main__":
    main()
