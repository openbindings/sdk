"""Validate the generated runtime inventory, including nested JavaScript modules."""
import hashlib
import json
import re


def checked_artifacts(directory):
    directory = directory.resolve()
    recorded = json.loads((directory / "build-info.json").read_text(encoding="utf-8"))["artifacts"]
    discovered = sorted(path.relative_to(directory).as_posix() for path in directory.rglob("*")
                        if path.is_file() and path.suffix in {".js", ".wasm"})
    required = {"openbindings_wasm.js", "openbindings_wasm_bg.wasm", "openbindings_wasm_signature.js"}
    if not required <= set(recorded) or set(discovered) != set(recorded):
        raise SystemExit("Incomplete Wasm artifact inventory.")
    actual = {}
    for name in discovered:
        path = directory / name
        raw = path.read_bytes()
        actual[name] = hashlib.sha256(raw).hexdigest()
        if actual[name] != recorded[name]:
            raise SystemExit("Wasm artifact hash mismatch.")
        if path.suffix == ".js":
            for imported in re.findall(r'(?:from\s*|import\s*)[\'\"](\.[^\'\"]+)[\'\"]', raw.decode("utf-8")):
                target = (path.parent / imported).resolve()
                if target.is_relative_to(directory) and target.relative_to(directory).as_posix() not in recorded:
                    raise SystemExit("Unrecorded Wasm runtime import.")
    return actual
