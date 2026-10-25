"""Verify local Cargo/npm release archives from a clean source; never publish."""
from pathlib import Path
import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
import tomllib

ROOT = Path(__file__).resolve().parents[1]
PUBLIC_CRATES = {
    "openbindings", "openbindings-json-schema-evaluator",
    "openbindings-http-discovery", "openbindings-schema-evaluator-test-support",
    "openbindings-source-inspection",
    "openbindings-interface-synthesis", "openbindings-invocation",
}

INTERNAL_CRATES = {
    "openbindings-internal-json", "openbindings-internal-schema",
    "openbindings-internal-cancellation", "openbindings-internal-interfaces",
}
BUILD_ONLY_CRATES = {"openbindings-wasm"}


def require(condition, message):
    if not condition:
        raise SystemExit(message)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


# The dependency tables of a packaged manifest. Cargo resolves every one of
# them, top level and per target, when it packages, verifies or publishes a
# crate; packaging has already dropped development dependencies that declare
# no version, so every development dependency left must already be published.
DEPENDENCY_TABLES = ("dependencies", "dev-dependencies", "build-dependencies")


def local_dependencies(manifest, local):
    """The `local` crates a packaged manifest names in any dependency table."""
    names = set()
    for section in [manifest, *manifest.get("target", {}).values()]:
        unknown = sorted(key for key in section
                         if key.endswith("dependencies") and key not in DEPENDENCY_TABLES)
        require(not unknown, f"Unrecognized dependency table in a packaged manifest: {unknown}")
        for table in DEPENDENCY_TABLES:
            for alias, spec in section.get(table, {}).items():
                name = spec.get("package", alias) if isinstance(spec, dict) else alias
                if name in local:
                    names.add(name)
    return names


def publication_order(dependencies):
    """Order the supported libraries' local closure so that each crate follows
    every local crate it depends on in any table Cargo resolves."""
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
    require(set(result) == PUBLIC_CRATES | INTERNAL_CRATES,
            "Publication closure differs from the supported artifact inventory.")
    require_dependencies_first(result, dependencies)
    return result


def require_dependencies_first(order, dependencies):
    """Fail unless each crate in `order` comes after every local crate it depends on."""
    position = {name: index for index, name in enumerate(order)}
    for name in order:
        later = sorted(dependency for dependency in dependencies[name]
                       if position.get(dependency, len(order)) >= position[name])
        require(not later, f"Publication order puts {name} before {', '.join(later)}, "
                           "which Cargo resolves when it packages or publishes it.")


def compat_fixture(compat, versions, patches):
    """Write the compatibility fixture as a consumer of the packaged `versions`:
    its whole source and lock, and its manifest naming those versions."""
    fixture = ROOT / "tools/consumer-compat"
    shutil.copytree(fixture / "src", compat / "src")
    manifest = (fixture / "Cargo.toml").read_text(encoding='utf-8')
    manifest = re.sub(r'path = "../../crates/([^\"]+)"', lambda m: 'version = "=' + versions[m[1]] + '"', manifest)
    require("path =" not in manifest, "Compatibility fixture still references the source checkout.")
    (compat / "Cargo.toml").write_text(manifest + patches)
    shutil.copyfile(fixture / "Cargo.lock", compat / "Cargo.lock")


def self_test():
    """The order counts every dependency table, and the check refuses an order
    that does not; the archive consumer holds the whole compatibility fixture."""
    local = PUBLIC_CRATES | INTERNAL_CRATES
    dev_only = {
        "package": {"name": "openbindings-http-discovery"},
        "dependencies": {"openbindings": {"version": "=0.2.0"}},
        "dev-dependencies": {"openbindings-invocation": {"version": "=0.2.0"}},
        "target": {"cfg(unix)": {"dev-dependencies": {
            "inspection": {"package": "openbindings-source-inspection", "version": "=0.2.0"}}}},
    }
    assert local_dependencies(dev_only, local) == {
        "openbindings", "openbindings-invocation", "openbindings-source-inspection"}
    try:
        local_dependencies({"dev_dependencies": {}}, local)
    except SystemExit:
        pass
    else:
        raise AssertionError("Accepted an unrecognized dependency table")
    # Every library builds on core and the internal crates; HTTP discovery
    # depends on invocation only for its tests.
    dependencies = {name: {"openbindings", "openbindings-internal-cancellation",
                           "openbindings-internal-interfaces"} for name in PUBLIC_CRATES}
    dependencies["openbindings"] = {"openbindings-internal-json", "openbindings-internal-schema"}
    dependencies["openbindings-internal-json"] = set()
    dependencies["openbindings-internal-schema"] = {"openbindings-internal-json"}
    for name in ["openbindings-internal-cancellation", "openbindings-internal-interfaces"]:
        dependencies[name] = {"openbindings"}
    dependencies["openbindings-http-discovery"].add("openbindings-invocation")
    order = publication_order(dependencies)
    assert order.index("openbindings-invocation") < order.index("openbindings-http-discovery")
    swapped = [name for name in order if name != "openbindings-invocation"]
    swapped.insert(swapped.index("openbindings-http-discovery") + 1, "openbindings-invocation")
    try:
        require_dependencies_first(swapped, dependencies)
    except SystemExit:
        pass
    else:
        raise AssertionError("Accepted a crate published before its development dependency")
    with tempfile.TemporaryDirectory(prefix="openbindings-compat-") as temporary:
        compat = Path(temporary) / "compat-consumer"
        compat_fixture(compat, {name: "0.0.0" for name in PUBLIC_CRATES}, "")
        source = ROOT / "tools/consumer-compat/src"
        files = lambda root: sorted(path.relative_to(root) for path in root.rglob("*") if path.is_file())
        assert files(compat / "src") == files(source), "Archive consumer lacks fixture sources"
        tomllib.loads((compat / "Cargo.toml").read_text(encoding='utf-8'))


def artifact_manifests():
    manifests = sorted(ROOT.glob("crates/*/Cargo.toml"))
    names = [tomllib.loads(manifest.read_text(encoding='utf-8'))["package"]["name"] for manifest in manifests]
    require(len(names) == len(set(names)), "Duplicate local package names.")
    require(set(names) == PUBLIC_CRATES | INTERNAL_CRATES | BUILD_ONLY_CRATES,
            "Local packages differ from the supported artifact inventory.")
    return manifests


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--offline", action="store_true", help="Require cached Cargo dependencies")
    parser.add_argument("--self-test", action="store_true",
                        help="Check the publication-order rule and the archive consumer, and exit")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        print("Publication order self-test passed.")
        return
    require(args.output is not None, "--output is required.")
    manifests = artifact_manifests()
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
    # The candidate is packaged, never published: every package keeps its guard.
    run("publication-guards", [sys.executable, ROOT / "tools/verify-publication-guards.py"])
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
    identity = json.loads((npm_root / "dist/wasm/build-info.json").read_text(encoding='utf-8'))
    require(identity["commit"] == commit and identity["treeClean"], "Wasm was not built from this clean candidate.")
    current_identity = json.loads(run("wasm-source-identity", ["node", "--input-type=module", "-e",
        "import {rustSourceIdentity} from './packages/typescript/scripts/source-identity.mjs'; "
        "console.log(JSON.stringify(await rustSourceIdentity()));"]))
    require(identity["rustSource"] == current_identity, "Wasm source identity differs from this candidate.")
    require(set(identity["artifacts"]) == {"openbindings_wasm.js", "openbindings_wasm_bg.wasm",
                                           "openbindings_wasm_signature.js"},
            "Incomplete Wasm artifact inventory.")
    for name, expected in identity["artifacts"].items():
        require(digest(npm_root / "dist/wasm" / name) == expected, f"Wasm artifact mismatch: {name}")
    version = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding='utf-8'))["workspace"]["package"]["version"]
    packages = []
    for manifest in manifests:
        spec = tomllib.loads(manifest.read_text(encoding='utf-8'))["package"]
        name = spec["name"]
        package_version = spec["version"] if isinstance(spec["version"], str) else version
        run("package-" + name, ["cargo", "package", "--locked", *offline, "--no-verify", "--exclude-lockfile", "--manifest-path", manifest])
        archive_path = out / f"{name}-{package_version}.crate"
        shutil.copyfile(target / "package" / archive_path.name, archive_path)
        with tarfile.open(archive_path) as archive:
            archive.extractall(out / "cargo", filter="data")
            source = out / "cargo" / archive_path.name.removesuffix(".crate")
            require(json.loads((source / ".cargo_vcs_info.json").read_text(encoding='utf-8'))["git"]["sha1"] == commit, f"Stale Cargo source: {name}")
            for member in archive.getmembers():
                if not member.isfile():
                    continue
                relative = Path(member.name).relative_to(member.name.split("/")[0])
                if str(relative) in {".cargo_vcs_info.json", "Cargo.lock", "Cargo.toml"}:
                    continue
                original = manifest.parent / ("Cargo.toml" if str(relative) == "Cargo.toml.orig" else relative)
                require(original.is_file() and original.read_bytes() == archive.extractfile(member).read(), f"Archive/source mismatch: {name}/{relative}")
            # Every package must deliver the license terms it declares. The SDK's own
            # packages inherit Apache-2.0 and carry the root text and a notice.
            delivered = {str(Path(m.name).relative_to(m.name.split("/")[0])) for m in archive.getmembers() if m.isfile()}
            if spec.get("license") == {"workspace": True}:
                require({"LICENSE", "NOTICE"} <= delivered, f"Archive omits LICENSE or NOTICE: {name}")
                require((source / "LICENSE").read_bytes() == (ROOT / "LICENSE").read_bytes(), f"Archive license text differs: {name}")
            else:
                require(any(Path(member).name.upper().startswith("LICENSE") for member in delivered),
                        f"Archive omits its license text: {name}")
        normalized = tomllib.loads((source / "Cargo.toml").read_text(encoding='utf-8'))
        for section in [normalized, *normalized.get("target", {}).values()]:
            for kind in ["dependencies", "dev-dependencies", "build-dependencies"]:
                for dependency in section.get(kind, {}).values():
                    require(not isinstance(dependency, dict) or "path" not in dependency, f"Development dependency path remains: {name}")
        packages.append({"name": name, "version": package_version, "archive": archive_path.name,
                         "sha256": digest(archive_path), "source": str(source)})
    # Order publication by what Cargo resolves: the packaged manifests.
    local = {item["name"] for item in packages}
    dependencies = {item["name"]: local_dependencies(tomllib.loads(
        (Path(item["source"]) / "Cargo.toml").read_text(encoding='utf-8')), local) for item in packages}
    patches = "\n[patch.crates-io]\n" + "".join(f'{item["name"]}={{path={json.dumps(item["source"])}}}\n' for item in packages)
    consumer = out / "cargo-consumer"
    (consumer / "src/bin").mkdir(parents=True)
    (consumer / "Cargo.toml").write_text(
        '[package]\nname="sdk-release-consumer"\nversion="0.0.0"\nedition="2024"\n[workspace]\n[dependencies]\n'
        + "".join(f'{name}="={version}"\n' for name in sorted(PUBLIC_CRATES)) +
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
    compat_fixture(compat, {item["name"]: item["version"] for item in packages}, patches)
    for features in ["", "ap", "sdk", "sdk,ap", "evaluator", "evaluator,po", "native", "native,ap",
                     "portable", "inspection", "synthesis", "invocation"]:
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
    spec = json.loads(spec_inventory.read_text(encoding='utf-8'))
    receipt = {"commit": commit, "version": version, "rustc": toolchain,
               "node": node_version, "npmVersion": npm_version,
               "specification": {"commit": spec["spec_commit"], "cases": spec["count"],
                                 "inventorySha256": digest(spec_inventory)},
               "dependencyIdentitiesSha256": digest(ROOT / "docs/dependency-isolation.json"),
               "workspaceLockSha256": digest(ROOT / "Cargo.lock"),
               "npm": {"archive": npm_archive.name, "sha256": digest(npm_archive), "buildIdentity": identity},
               "cargo": packages, "publicationOrder": publication_order(dependencies),
               "cargoBuildOnly": sorted(BUILD_ONLY_CRATES), "archiveConsumers": "passed",
               "status": "local artifacts verified; publication not performed"}
    (out / "MANIFEST.json").write_text(json.dumps(receipt, indent=2) + "\n")
    print(json.dumps({"manifest": str(out / "MANIFEST.json"), "cargoArchives": len(packages), "publicationPerformed": False}))


if __name__ == "__main__":
    main()
