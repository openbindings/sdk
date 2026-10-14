"""Qualify validity-only document findings against the frozen allocation generators."""
from pathlib import Path
import os
import subprocess
import sys
root = Path(__file__).resolve().parents[1]
env = os.environ.copy()
env.setdefault("CARGO_TARGET_DIR", str(root / "target"))
out = root / "target/document-diagnostics"
out.mkdir(parents=True, exist_ok=True)
# Compilation can return schema errors; the instance path must never request
# library diagnostics or activate the fork's eager diagnostic admission scopes.
for file in ["fixed_schema.rs", "fixed_locations.rs"]:
    source = (root / "crates/openbindings-internal-schema/src" / file).read_text(encoding='utf-8')
    for forbidden in [".validate(", ".iter_errors(", ".evaluate(", "ob_work::diagnostics(", "ob_work::diagnostic_metadata("]:
        assert forbidden not in source, (file, forbidden)
subprocess.run([sys.executable, str(root / "tools/diagnostic-memory-probe/generate_inputs.py")], cwd=root, check=True)
with (out / "memory.jsonl").open("w") as log:
    subprocess.run(["cargo", "run", "--release", "--locked", "--manifest-path", str(root / "tools/diagnostic-memory-probe/Cargo.toml"), "--", str(out / "fixtures/inputs.json")], cwd=root, env=env, stdout=log, check=True)
print(out / "memory.jsonl")

subprocess.run([sys.executable, str(root / "tools/diagnostic-memory-probe/summarize.py")], cwd=root, check=True)
