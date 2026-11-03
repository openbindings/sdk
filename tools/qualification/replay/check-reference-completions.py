"""Independent finite truth and completion controls for static-reference holes.

Run after the native and TypeScript static-reference-defects tests with
REFERENCE_RECEIPT set to the respective output path. No library resolver is
reimplemented: the oracle only evaluates complete Boolean/type/applicator cases.
"""
import argparse
import copy
import json
import hashlib
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
INPUTS = ROOT / "crates/openbindings-json-schema-evaluator/tests/fixtures"
CASES = sum((json.loads((INPUTS / name).read_text()) for name in
             ["reference-matrix.json", "reference-completions.json"]), [])

ADJUDICATION = json.loads((INPUTS / "reference-adjudication.json").read_text())
assert ADJUDICATION["spec_revision"] == "c1419c9fc3ae5a00c2b83da710c11bf67dab7934"
assert len({c["id"] for c in CASES}) == len(CASES) == ADJUDICATION["total_cases"] == 2348
for source in ADJUDICATION["sources"]:
    assert hashlib.sha256((INPUTS / source["file"]).read_bytes()).hexdigest() == source["sha256"]
ADMISSION = json.loads((INPUTS / "reference-admission.json").read_text())
NEGATIVES = {c["id"]: c for c in ADMISSION["cases"]}
assert len(NEGATIVES) == len(ADMISSION["cases"]) == 194
assert set(NEGATIVES) <= {c["id"] for c in CASES}
CAPABILITIES = {c["id"]: c for c in json.loads((INPUTS / "reference-capabilities.json").read_text())["cases"]}
assert len(CAPABILITIES) == 1
assert not set(CAPABILITIES) & set(NEGATIVES)
for case in CASES:
    if case["id"] in CAPABILITIES:
        capability = CAPABILITIES[case["id"]]
        assert case == capability["historical_case"]
        case["expected"] = capability["expected"]

# Original cases remain exact; current admission replaces their value expectation.
for case in CASES:
    if case["id"] in NEGATIVES:
        negative = NEGATIVES[case["id"]]
        assert case == negative["historical_case"]
        case["expected"] = {"stage":"admission", "outcome":"non-conformant", "evidence":{rule:"violated" for rule in negative["required_violated_rules"]}}

# Translate the historical representation without changing the frozen controls.
for case in CASES:
    expected = case["expected"]
    if expected["outcome"] == "N" and expected.get("detail", {}).get("reason") == "undefined":
        expected["outcome"] = "undefined"
        del expected["detail"]["reason"]


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
    admission_boundaries = 0
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
        if partial["stage"] == "admission":
            assert key in NEGATIVES
            admission_boundaries += 1
            continue
        if partial["outcome"] in ["S", "F"]:
            assert all(v == partial["outcome"] for v in outcomes), key
        elif partial["outcome"] == "undefined":
            assert set(outcomes) == {"S", "F"}, key
    return {"cases": len(rows), "independent_complete_checks": checked,
            "completion_groups": len(completions), "admission_boundaries": admission_boundaries}


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
        ("pointer-missing/deciding", {"stage": "admission", "outcome": "non-conformant", "evidence": {"OBI-12":"met"}}),
        ("influence/budget", {"stage": "ready", "outcome": "undefined", "detail": {}}),
        ("influence/oneof/correlated", {"stage": "ready", "outcome": "undefined", "detail": {}}),
        ("influence/mixture/anyOfFalseFalse", {"stage": "ready", "outcome": "undefined", "detail": {}}),
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
    next(r for r in rows if r["id"] == "cycle/deciding")["actual"]["detail"]["location"]["pointer"] = "/$defs/n0"
    try:
        check(rows)
    except AssertionError:
        pass
    else:
        raise AssertionError("invented generated location accepted")
    print("7 injected false verdict/classification/admission/location controls rejected")


if __name__ == "__main__":
    main()
