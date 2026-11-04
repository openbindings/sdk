"""Integrity checks shared by external current-interface qualification tools."""
from pathlib import Path
import hashlib
import tomllib


def verify_resolution(root, metadata, consumer, sources, fixtures, archives, out):
    """Verify both the selected lock identities and every local resolution path.

    A smaller external graph may drop root-lock packages. It may add only named
    local fixtures; no registry dependency may change version/source/checksum.
    """
    identity = lambda p: (p["name"], p["version"], p.get("source"), p.get("checksum"))
    original = tomllib.loads((root / "Cargo.lock").read_text(encoding="utf-8"))
    generated_path = consumer / "Cargo.lock"
    generated = tomllib.loads(generated_path.read_text(encoding="utf-8"))
    allowed = {identity(p) for p in original["package"]}
    allowed.update((name, "0.0.0", None, None) for name in fixtures)
    changed = {identity(p) for p in generated["package"]} - allowed
    if changed:
        raise SystemExit(f"Consumer changed pinned dependency identities: {sorted(changed, key=repr)}")
    locked = {(p["name"], p["version"], p.get("source")) for p in generated["package"]}
    paths = {**sources, **fixtures}
    for package in metadata["packages"]:
        path = Path(package["manifest_path"]).resolve().parent
        if (package["name"], package["version"], package.get("source")) not in locked:
            raise SystemExit("Consumer metadata disagrees with its checked lock")
        if package.get("source") is None:
            if package["name"] not in paths or path != paths[package["name"]].resolve():
                raise SystemExit("Consumer resolved an unexpected local dependency")
        if archives and path.is_relative_to(root) and not path.is_relative_to(out):
            raise SystemExit("Archive consumer resolved checkout source")
    return hashlib.sha256(generated_path.read_bytes()).hexdigest()

