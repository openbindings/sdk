"""Run a private compiled-lane allocation test in an isolated SDK copy.

The copy alone permits the unsafe System allocator forwarding. No dependency,
workspace lint, public API, or production source is changed for instrumentation.
"""

import argparse
import os
from pathlib import Path
import shutil
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--release", action="store_true")
    parser.add_argument("--baseline", action="store_true", help="Read the two guard files from 7fcc101")
    parser.add_argument("--bypass-estimate", action="store_true", help="Allocation precheck only")
    parser.add_argument("--test", default="allocation_samples_preserve_decisive_outcomes")
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[2]
    snapshot = root / "target/literal-scratch-snapshot"
    snapshot.mkdir(parents=True, exist_ok=True)
    for path in root.iterdir():
        if path.name in {".git", "target", "crates"}:
            continue
        destination = snapshot / path.name
        if not destination.exists():
            destination.symlink_to(path, target_is_directory=path.is_dir())
    crates = snapshot / "crates"
    crates.mkdir(exist_ok=True)
    subject = crates / "openbindings-json-schema-evaluator"
    for path in (root / "crates").iterdir():
        destination = crates / path.name
        if destination == subject:
            if destination.exists():
                shutil.rmtree(destination)
            shutil.copytree(path, destination)
        elif not destination.exists():
            destination.symlink_to(path, target_is_directory=path.is_dir())
    manifest = subject / "Cargo.toml"
    text = manifest.read_text().replace("[lints]\nworkspace = true\n", "")
    text += '\n[[test]]\nname = "literal_scratch_allocations"\npath = "src/allocation_main.rs"\n'
    manifest.write_text(text)
    if args.baseline:
        for name in ["admission.rs", "mod.rs"]:
            relative = f"crates/openbindings-json-schema-evaluator/src/compiled/{name}"
            previous = subprocess.check_output(["git", "show", f"7fcc101:{relative}"], cwd=root)
            (subject / "src/compiled" / name).write_bytes(previous)
    source = (subject / "src/lib.rs").read_text()
    source = source.replace("#![forbid(unsafe_code)]\n", "")
    measure = (root / "tools/literal-scratch/measure.rs").as_posix()
    source += f'\n#[path = "{measure}"]\nmod literal_scratch_measure;\n'
    (subject / "src/allocation_main.rs").write_text(source)
    compiled = subject / "src/compiled/mod.rs"
    with compiled.open("a") as output:
        output.write("""
// Measurement only: enter the current pass without its scratch estimate.
#[cfg(test)]
impl Lane {
    pub(crate) fn without_scratch_estimate(&self) -> Self {
        let Self::Compiled(prepared) = self else { panic!("compiled owner required") };
        let mut features = prepared.features.clone();
        features.scratch = false;
        Self::Compiled(Arc::new(Prepared {
            inner: prepared.inner.clone(), features, reservation: prepared.reservation,
        }))
    }
}
""")
    command = [
        "cargo", "test", "--locked", "--offline",
        "--manifest-path", str(subject / "Cargo.toml"),
        "-p", "openbindings-json-schema-evaluator",
        "--test", "literal_scratch_allocations",
    ]
    if args.release:
        command.append("--release")
    command += [f"literal_scratch_measure::{args.test}", "--", "--nocapture", "--test-threads=1"]
    environment = os.environ.copy()
    environment["CARGO_TARGET_DIR"] = str(root / "target")
    if args.bypass_estimate:
        environment["OB_LITERAL_SCRATCH_BYPASS"] = "1"
    else:
        environment.pop("OB_LITERAL_SCRATCH_BYPASS", None)
    subprocess.run(command, env=environment, check=True, cwd=root)


if __name__ == "__main__":
    main()
