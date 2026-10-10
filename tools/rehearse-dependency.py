"""Reconstruct one private fork from its verified upstream archive and patch.

Requires the standard `patch` executable. This is a source reconstruction check,
not a dependency upgrade or a substitute for runtime/consumer qualification.
"""
from pathlib import Path
import argparse
import hashlib
import json
import shutil
import subprocess
import tarfile

ROOT = Path(__file__).resolve().parents[1]


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def inventory(root):
    return {p.relative_to(root).as_posix(): sha(p) for p in root.rglob("*")
            if p.is_file() and "target" not in p.parts and ".git" not in p.parts}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--package", default="jsonschema")
    parser.add_argument("--upstream-dir", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    manifest_path = ROOT / "docs/dependency-patches/manifest.json"
    records = json.loads(manifest_path.read_text())["records"]
    record = next((item for item in records if item["upstream"] == args.package), None)
    if record is None:
        raise SystemExit("Package is not in the maintained upstream inventory.")
    archive_path = args.upstream_dir / f'{record["upstream"]}-{record["version"]}.crate'
    if sha(archive_path) != record["upstream_archive_sha256"]:
        raise SystemExit("Upstream archive checksum differs from recorded provenance.")
    patch_tool = shutil.which("patch")
    if patch_tool is None:
        raise SystemExit("Install/provide the standard patch executable for this rehearsal.")
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    with tarfile.open(archive_path) as archive:
        archive.extractall(output / "source", filter="data")
    source = output / "source" / f'{record["upstream"]}-{record["version"]}'
    patch = ROOT / "docs/dependency-patches" / record["patch"]
    result = subprocess.run([patch_tool, "-p1", "--batch", "--forward", "-i", str(patch)],
                            cwd=source, text=True, capture_output=True)
    (output / "patch.log").write_text(result.stdout + result.stderr)
    if result.returncode:
        raise SystemExit("Patch replay failed; inspect patch.log. No SDK source changed.")
    # Some patch implementations keep empty files after a deletion hunk.
    for change in record["changes"]:
        target = source / change["file"]
        if change["current_sha256"] is None and target.exists():
            if target.read_bytes():
                raise SystemExit(f'Patch did not delete {change["file"]}')
            target.unlink()
    current = ROOT / "vendor" / record["modified_package"]
    actual, expected = inventory(source), inventory(current)
    differences = sorted(name for name in actual.keys() | expected.keys()
                         if actual.get(name) != expected.get(name))
    receipt = {"upstream": record["upstream"], "version": record["version"],
               "archiveSha256": sha(archive_path), "patchSha256": sha(patch),
               "provenanceSha256": sha(manifest_path), "sourceFiles": len(actual),
               "mismatches": differences,
               "scope": "exact reconstruction of current fork; no upgrade or runtime claim"}
    (output / "RECONSTRUCTION.json").write_text(json.dumps(receipt, indent=2) + "\n")
    if differences:
        raise SystemExit("Reconstruction differs: " + ", ".join(differences))
    print(f"Reconstructed {record['modified_package']}: {len(actual)} files match.")


if __name__ == "__main__":
    main()
