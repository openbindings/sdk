"""Independent finite truth and completion controls for static-reference holes.

Run after the native and TypeScript static-reference-defects tests with
REFERENCE_RECEIPT set to the respective output path. No library resolver is
reimplemented: the oracle only evaluates complete Boolean/type/applicator cases.
"""
import argparse
import copy
import json
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
INPUTS = ROOT / "crates/openbindings-json-schema-evaluator/tests/fixtures"
CASES = sum((json.loads((INPUTS / name).read_text()) for name in
             ["reference-matrix.json", "reference-completions.json"]), [])


def subset(expected, actual):
    if isinstance(expected, dict):
        assert isinstance(actual, dict)
        for key, value in expected.items():
            subset(value, actual.get(key))
    else:
        assert expected == actual, (expected, actual)


def oracle(schema, value, supplied):
    if isinstance(schema, bool):
        return schema
    clauses = []
    for key, operand in schema.items():
        if key == "$id":
            continue
        if key == "$ref":
            clauses.append(oracle(supplied[operand], value, supplied))
        elif key in ["allOf", "anyOf", "oneOf"]:
            truths = [oracle(s, value, supplied) for s in operand]
            clauses.append(all(truths) if key == "allOf" else any(truths) if key == "anyOf" else sum(truths) == 1)
        elif key == "type":
            assert operand in ["string", "object"]
            clauses.append(isinstance(value, str) if operand == "string" else isinstance(value, dict))
        elif key == "properties":
            clauses.append(not isinstance(value, dict) or all(oracle(s, value[n], supplied) for n, s in operand.items() if n in value))
        elif key == "prefixItems":
            clauses.append(not isinstance(value, list) or all(oracle(s, v, supplied) for s, v in zip(operand, value)))
        elif key == "items":
            clauses.append(not isinstance(value, list) or all(oracle(operand, v, supplied) for v in value[len(schema.get("prefixItems", [])):]))
        else:
            raise AssertionError(f"oracle keyword not admitted: {key}")
    return all(clauses)


def check(rows):
    actual = {r["id"]: r["actual"] for r in rows}
    assert len(actual) == len(rows) == len(CASES)
    groups, completions = defaultdict(list), defaultdict(list)
    checked = 0
    for c in CASES:
        result = actual[c["id"]]
        try:
            subset(c["expected"], result)
        except AssertionError as error:
            raise AssertionError(c["id"]) from error
        if c["family"] in ["differential", "semantic-differential", "completion"]:
            if c["family"] == "completion" or c["context"] == "complete":
                supplied = {r["uri"]: r["schema"] for r in c.get("resources", [])}
                want = "S" if oracle(c["schema"], c["value"], supplied) else "F"
                assert result["outcome"] == want, c["id"]
                checked += 1
            if "group" in c:
                groups[c["group"]].append(c)
        if "completion_of" in c:
            completions[c["completion_of"]].append(result["outcome"])
    for group in groups.values():
        partial = next(c for c in group if c["context"] == "partial")
        completions[partial["id"]] = [actual[c["id"]]["outcome"] for c in group if c["context"] == "complete"]
    for key, outcomes in completions.items():
        partial = actual[key]
        if partial["outcome"] in ["S", "F"]:
            assert all(v == partial["outcome"] for v in outcomes), key
        elif partial["detail"]["reason"] == "undefined":
            assert set(outcomes) == {"S", "F"}, key
    return {"cases": len(rows), "independent_complete_checks": checked,
            "completion_groups": len(completions)}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("receipts", nargs="+", type=Path)
    args = parser.parse_args()
    reference = None
    for path in args.receipts:
        rows = json.loads(path.read_text())
        summary = check(rows)
        if reference is not None:
            assert rows == reference, "Rust/TypeScript observation drift"
        reference = rows
        print(path, summary)
    # A gate which accepts any of these injected errors is not a control.
    mutations = [
        ("pointer-missing/any-true", {"stage": "ready", "outcome": "F"}),
        ("pointer-scalar/deciding", {"stage": "ready", "outcome": "S"}),
        ("influence/budget", {"stage": "ready", "outcome": "N", "detail": {"reason": "undefined"}}),
        ("influence/oneof/correlated", {"stage": "ready", "outcome": "N", "detail": {"reason": "undefined"}}),
        ("influence/mixture/anyOfFalseFalse", {"stage": "ready", "outcome": "N", "detail": {"reason": "undefined"}}),
    ]
    for key, bad in mutations:
        rows = copy.deepcopy(reference)
        next(r for r in rows if r["id"] == key)["actual"] = bad
        try:
            check(rows)
        except AssertionError:
            pass
        else:
            raise AssertionError(f"false verdict/classification accepted: {key}")
    rows = copy.deepcopy(reference)
    next(r for r in rows if r["id"] == "pointer-missing/deciding")["actual"]["detail"]["location"]["pointer"] = "/$defs/n0"
    try:
        check(rows)
    except AssertionError:
        pass
    else:
        raise AssertionError("invented generated location accepted")
    print("6 injected false verdict/classification/location controls rejected")


if __name__ == "__main__":
    main()
