"""Fail if the pinned document/meta first-error paths amplify a large document."""
from pathlib import Path
import os
import subprocess

root = Path(__file__).resolve().parents[1]
env = os.environ.copy()
env.setdefault("CARGO_TARGET_DIR", str(root / "target"))
subprocess.run(["cargo", "run", "--locked", "--manifest-path", str(root / "tools/diagnostic-memory-probe/Cargo.toml")], cwd=root, env=env, check=True)
