"""HD-6/HD-7: downstream graph and unrelated-client decoding witness, offline."""
import argparse
import json
import os
import pathlib
import shutil
import subprocess
import tempfile
import tomllib

def verify_shared_packages(workspace, witness):
    """F6: every shared package identity retains its workspace version/checksum."""
    shared = {}
    for package in workspace["package"]:
        shared.setdefault(package["name"], set()).add(
            (package["version"], package.get("checksum"), package.get("source")))
    for package in witness["package"]:
        if package["name"] in shared:
            identity = (package["version"], package.get("checksum"), package.get("source"))
            assert identity in shared[package["name"]], (
                f"{package['name']}: witness version/checksum/source differs from workspace lock")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--graph-only", action="store_true", help="Compile all witnesses without binding loopback sockets")
    args = parser.parse_args()
    crate = pathlib.Path(__file__).resolve().parents[1]
    repo = crate.parents[1]
    with tempfile.TemporaryDirectory(prefix="obi-feature-witness-") as temporary:
        root = pathlib.Path(temporary)
        features = {}
        for name, enabled in [("control", []), ("sdk", ["sdk"]), ("decoding", ["sdk", "compression"])]:
            workspace = root / name
            shutil.copytree(crate / "tests/feature-witness", workspace)
            manifest = workspace / "Cargo.toml"
            manifest.write_text(manifest.read_text().replace('path = "../.."', 'path = ' + json.dumps(str(crate))))
            # F6: copytree already copied the committed witness lock, including decoder pins.
            verify_shared_packages(
                tomllib.loads((repo / "Cargo.lock").read_text()),
                tomllib.loads((workspace / "Cargo.lock").read_text()))
            flags = ["--features", ",".join(enabled)] if enabled else []
            env = dict(os.environ, CARGO_NET_OFFLINE="true", CARGO_TARGET_DIR=os.environ.get("CARGO_TARGET_DIR", str(root / "target")))
            metadata = json.loads(subprocess.check_output(["cargo", "metadata", "--offline", "--locked", "--format-version", "1", *flags], cwd=workspace, env=env))
            reqwest = next(p["id"] for p in metadata["packages"] if p["name"] == "reqwest")
            features[name] = next(n["features"] for n in metadata["resolve"]["nodes"] if n["id"] == reqwest)
            subprocess.run(["cargo", "build", "--offline", "--locked", *flags], cwd=workspace, env=env, check=True)
            if not args.graph_only:
                subprocess.run(["cargo", "run", "--offline", "--locked", *flags], cwd=workspace, env=env, check=True)
        assert features["control"] == features["sdk"], features
        assert not {"gzip", "brotli", "deflate", "stream"}.intersection(features["sdk"]), features
        assert {"gzip", "brotli", "deflate"}.issubset(features["decoding"]), features
        print("HD-6: matched reqwest features with/without SDK; application decoding opt-in compiled" + (" (loopback execution deferred)" if args.graph_only else "; runtime decoding witnesses passed"))


if __name__ == "__main__":
    main()
