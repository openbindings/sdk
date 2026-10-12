"""Run the public invocation consumer's release allocation/resource measurements.

All build and receipt output uses the shared Cargo target. Structural gates are
mandatory; host-specific timing budgets are opt-in and never a parity claim.
"""
from pathlib import Path
import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
import platform
import subprocess

ROOT = Path(__file__).resolve().parents[1]
PROBE = ROOT / "tools/invocation-resource-probe"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, help="Receipt path (default: shared target/invocation-resources/receipt.json)")
    parser.add_argument("--baseline", type=Path, default=PROBE / "timing-baseline.json")
    parser.add_argument("--record-baseline", action="store_true", help="Record the first measured host-specific timing baseline and provisional budgets")
    parser.add_argument("--check-timing", action="store_true", help="Also enforce recorded host-specific timing budgets")
    args = parser.parse_args()
    env = os.environ.copy()
    target = Path(env.get("CARGO_TARGET_DIR", ROOT / "target")).resolve()
    env["CARGO_TARGET_DIR"] = str(target)
    env.setdefault("CARGO_NET_OFFLINE", "true")
    output = (args.output or target / "invocation-resources/receipt.json").resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    output.unlink(missing_ok=True)
    hashed = [Path(__file__).resolve(), ROOT / "Cargo.toml", PROBE / "Cargo.toml", PROBE / "Cargo.lock", ROOT / "crates/openbindings-invocation/Cargo.toml", *sorted((PROBE / "src").glob("*.rs")), *sorted((ROOT / "crates/openbindings-invocation/src").glob("*.rs"))]
    hashes = {str(path.relative_to(ROOT)): hashlib.sha256(path.read_bytes()).hexdigest() for path in hashed}

    def run(label, command):
        result = subprocess.run(list(map(str, command)), cwd=ROOT, env=env, text=True, capture_output=True)
        (output.parent / f"{label}.log").write_text(result.stdout + result.stderr)
        if result.returncode:
            raise SystemExit(f"Failed: {output.parent / (label + '.log')}")
        return result.stdout

    run("build", ["cargo", "build", "--release", "--locked", "--manifest-path", PROBE / "Cargo.toml"])
    raw = run("probe", [target / "release/openbindings-invocation-resource-probe"])
    measurements = json.loads(raw)
    rows = measurements["measurements"]
    for row in rows:
        row["ns_per_call"] = row["elapsed_ns"] / row["iterations"]
        row["allocation_events_per_call"] = (row["allocations"] + row["reallocations"]) / row["iterations"]
        if row["values"]:
            row["ns_per_value"] = row["elapsed_ns"] / row["values"]
            row["allocation_events_per_value"] = (row["allocations"] + row["reallocations"]) / row["values"]

    retention = measurements["retention"]
    retention_deltas = [v for key, v in retention.items() if key.endswith("_delta")]
    checked = [row for row in rows if row["name"].startswith("checked_") or row["name"] == "handle_creation_release"]
    gates = {
        "retention_10000_completed_and_10000_cancelled": retention["completed"] >= 10_000 and retention["cancelled"] >= 10_000,
        "no_live_allocation_or_byte_growth": all(v == 0 for v in retention_deltas) and all(row["live_allocation_delta"] == 0 and row["live_byte_delta"] == 0 for row in rows),
        "provider_guards_released": measurements["provider_live_guards"] == 0 and retention["live_provider_guards"] == 0,
        "at_most_one_active_provider": measurements["provider_peak_guards"] == 1,
        "streams_exceed_queue_capacity": measurements["stream_values_per_call"] > max(measurements["input_capacity"], measurements["output_capacity"]),
        "one_byte_subtree_retains_one_mib_backing": measurements["subtree_logical_bytes"] == 1 and measurements["subtree_backing_bytes"] == 1024 * 1024,
        "no_full_source_sized_allocation_per_call": all(row["max_single_allocation"] < measurements["source_logical_bytes"] and row["allocated_bytes"] / row["iterations"] < measurements["source_logical_bytes"] for row in checked),
        "default_capacities_one_and_four": measurements["input_capacity"] == 1 and measurements["output_capacity"] == 4,
        "source_stable_during_run": hashes == {str(path.relative_to(ROOT)): hashlib.sha256(path.read_bytes()).hexdigest() for path in hashed},
    }
    host = {"system": platform.system(), "release": platform.release(), "machine": platform.machine()}
    rustc = run("rustc", ["rustc", "--version"]).strip()
    git_head = run("git-head", ["git", "rev-parse", "HEAD"]).strip()
    timing = {"enforced": args.check_timing, "baseline": str(args.baseline), "claim": "initial synthetic overhead baseline only; no parity, browser RSS, or total-memory claim"}
    if args.record_baseline and all(gates.values()):
        if args.baseline.exists():
            raise SystemExit("Refusing to overwrite an existing timing baseline; choose a new --baseline path")
        baseline = {
            "recorded_at": datetime.now(timezone.utc).isoformat(), "host": host, "rustc": rustc,
            "git_head": git_head, "source_sha256": hashes,
            "budget_policy": "Provisional same-host ns/call ceiling: max(3 times initial ns/call, initial ns/call + 2000 ns); timing enforcement is opt-in",
            "measurements": {row["name"]: {"ns_per_call": row["ns_per_call"], "budget_ns_per_call": max(3 * row["ns_per_call"], row["ns_per_call"] + 2000)} for row in rows},
        }
        args.baseline.parent.mkdir(parents=True, exist_ok=True)
        args.baseline.write_text(json.dumps(baseline, indent=2) + "\n")
    if args.check_timing:
        baseline = json.loads(args.baseline.read_text())
        if baseline["host"] != host:
            raise SystemExit("Timing baseline is from a different host platform; run structural gates without --check-timing")
        timing["within_budget"] = {row["name"]: row["ns_per_call"] <= baseline["measurements"][row["name"]]["budget_ns_per_call"] for row in rows}
        gates["host_specific_timing_budgets"] = all(timing["within_budget"].values())

    receipt = {
        "completed_at": datetime.now(timezone.utc).isoformat(), "host": host, "rustc": rustc,
        "git_head": git_head, "source_sha256": hashes, "measurement": measurements,
        "gates": gates, "passed": all(gates.values()), "timing": timing,
        "limits_of_evidence": [
            "Allocator counts describe System requests in one synthetic process, not allocator usable sizes or RSS.",
            "Both paths use the same exact-value semantic fixture; checked path includes boxed provider future, safe test executor, and SDK lifecycle.",
            "Queue/pending maxima require separate SDK unit instrumentation; public capacities here are configuration, not observed queue maxima.",
            "Logical delivery bytes do not bound retained backing owners; a one-byte value here retains a one-MiB source.",
        ],
    }
    output.write_text(json.dumps(receipt, indent=2) + "\n")
    print(json.dumps({"receipt": str(output), "passed": receipt["passed"], "gates": gates}, indent=2))
    if not receipt["passed"]:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
