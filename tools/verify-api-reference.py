"""Compile public Rust reference contracts and check emitted TypeScript JSDoc.

Run after `npm run build` in packages/typescript. Optional output directory receives
an exported-symbol inventory and rendered TypeScript reference. This is an omission
regression gate, not a quality percentage or a substitute for examples/review.
"""
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
PUBLIC_CRATES = (
    "openbindings",
    "openbindings-json-schema-evaluator",
    "openbindings-http-discovery",
    "openbindings-schema-evaluator-test-support",
)


def run(arguments):
    print("+ " + " ".join(map(str, arguments)), flush=True)
    subprocess.run(list(map(str, arguments)), cwd=ROOT, check=True)


def main():
    for crate in PUBLIC_CRATES:
        arguments = ["cargo", "rustdoc", "--locked", "--lib", "-p", crate]
        if crate == "openbindings-http-discovery":
            arguments += ["--features", "native"]
        run(arguments + ["--", "-D", "missing_docs", "-D", "rustdoc::broken_intra_doc_links"])
    # The implementation support crate exposes other non-SDK internals. Its two
    # selected exact-value modules carry missing_docs lints; -D warnings enforces
    # those without treating all implementation helpers as public SDK contracts.
    run([
        "cargo", "rustdoc", "--locked", "--lib", "-p", "openbindings-internal-json",
        "--", "-D", "warnings",
    ])
    packages = [
        argument
        for crate in (*PUBLIC_CRATES, "openbindings-internal-json")
        for argument in ("-p", crate)
    ]
    run([
        "cargo", "test", "--locked", "--doc", *packages,
        "--features", "openbindings-http-discovery/native",
    ])
    command = ["node", ROOT / "packages/typescript/scripts/check-api-reference.mjs"]
    if len(sys.argv) > 1:
        command.append(Path(sys.argv[1]).resolve())
    run(command)
    run([
        "node", ROOT / "packages/typescript/node_modules/typescript/bin/tsc",
        "--noEmit", "--strict", "--target", "ES2022", "--module", "NodeNext",
        "--lib", "ES2022,DOM,DOM.Iterable,ESNext.Disposable", "--skipLibCheck",
        ROOT / "packages/typescript/test/reference.types.ts",
    ])


if __name__ == "__main__":
    main()
