"""Run the dependency forks' own regression tests in a standalone workspace.

The forks sit outside the SDK workspace with their upstream dev-dependencies,
and Cargo tests a package outside the workspace only when it has none. This
copies the forks into a new output directory as members of their own workspace
and tests them there, so no file in the checkout changes. Resolution starts from
the SDK workspace lock, and each package of that lock the tested fork code builds
with on the host must keep the lock's version; test-only dependencies, and those
of features the SDK never selects, resolve afresh. CARGO_NET_OFFLINE controls
cache-only runs.
"""
from pathlib import Path
import argparse
import hashlib
import json
import os
import shutil
import subprocess
import tomllib

ROOT = Path(__file__).resolve().parents[1]
# The configurations DEPENDENCY-MAINTENANCE.md describes. The SDK selects no
# jsonschema features; that fork's two tests compile and release long reference
# chains and reference cycles, whose release order the fork changes.
SUITES = {
    "openbindings-internal-regress": [
        ["--no-default-features", "--features", "std,utf16,prohibit-unsafe", "--lib"],
        ["--no-default-features", "--features", "std"],
    ],
    "openbindings-internal-referencing": [
        ["--lib"],
        ["--lib", "--features", "retrieve-async"],
    ],
    "openbindings-internal-jsonschema": [
        ["--no-default-features", "--test", "ref_chain", "--test", "leak"],
    ],
}


def require(condition, message):
    if not condition:
        raise SystemExit(message)


def features(arguments):
    """The feature selection among cargo test arguments."""
    selected, rest = [], iter(arguments)
    for argument in rest:
        if argument == "--features":
            selected += [argument, next(rest)]
        elif argument in {"--no-default-features", "--all-features"}:
            selected.append(argument)
    return selected


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    out = args.output.resolve()
    require(not out.is_relative_to(ROOT) or out.is_relative_to(ROOT / "target"),
            "Inside the checkout, use an ignored target/ output directory.")
    require(not out.exists(), "Use a new output directory; prior receipts are not overwritten.")
    env = os.environ.copy()
    env.setdefault("CARGO_TARGET_DIR", str(ROOT / "target"))
    forks = sorted(path.name for path in (ROOT / "vendor").iterdir() if path.is_dir())
    require(forks == sorted(SUITES), "The fork inventory differs from the regression suites.")
    for fork in forks:
        # A standalone command run in a fork leaves its own lock and target directory.
        shutil.copytree(ROOT / "vendor" / fork, out / "vendor" / fork,
                        ignore=shutil.ignore_patterns("target", "Cargo.lock"))
    members = ", ".join(json.dumps("vendor/" + fork) for fork in forks)
    (out / "Cargo.toml").write_text(f'[workspace]\nresolver = "3"\nmembers = [{members}]\n')
    shutil.copyfile(ROOT / "Cargo.lock", out / "Cargo.lock")
    sdk_lock = {}
    for package in tomllib.loads((ROOT / "Cargo.lock").read_text())["package"]:
        if "source" in package:
            sdk_lock.setdefault(package["name"], set()).add(package["version"])

    def run(arguments, capture=True):
        print("+ " + " ".join(arguments), flush=True)
        result = subprocess.run(arguments, cwd=out, env=env, text=True, encoding="utf-8",
                                stdout=subprocess.PIPE if capture else None)
        require(result.returncode == 0, " ".join(arguments) + " failed.")
        return result.stdout

    # The one resolution prunes the seed and adds the test-only dependencies.
    run(["cargo", "fetch"])
    for fork, configurations in SUITES.items():
        for configuration in configurations:
            # The packages the tests build on this host, test-only ones excepted.
            tree = run(["cargo", "tree", "--locked", "-p", fork, *features(configuration),
                        "--edges", "normal,build", "--prefix", "none", "--format", "{p}"])
            # Registry packages print as "name vVERSION"; local ones add their path.
            built = {tuple(line.split(" v", 1)) for line in tree.splitlines() if "(" not in line}
            drift = sorted(f"{name} {version}" for name, version in built
                           if version not in sdk_lock.get(name, {version}))
            require(not drift, f"{fork} builds with versions other than the SDK lock's: "
                    + ", ".join(drift))
            run(["cargo", "test", "--locked", "-p", fork, *configuration], capture=False)
    resolved = tomllib.loads((out / "Cargo.lock").read_text())["package"]
    receipt = {
        "suites": SUITES,
        "lockSha256": hashlib.sha256((out / "Cargo.lock").read_bytes()).hexdigest(),
        "outsideSdkLock": sorted(f'{package["name"]} {package["version"]}'
                                 for package in resolved if "source" in package and
                                 package["version"] not in sdk_lock.get(package["name"], ())),
    }
    (out / "RESULTS.json").write_text(json.dumps(receipt, indent=2) + "\n")
    print(json.dumps({"forks": len(SUITES), "results": str(out / "RESULTS.json")}))


if __name__ == "__main__":
    main()
