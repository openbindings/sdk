"""S5b review F6 / HD-6: the witness rejects drift in shared versions/checksums."""
import copy
import pathlib
import runpy
import tomllib
import unittest

HERE = pathlib.Path(__file__).resolve().parent

class WitnessLockTests(unittest.TestCase):
    def test_shared_packages_are_pinned_and_drift_is_rejected(self):
        fixture = HERE / "feature-witness/Cargo.lock"
        self.assertTrue(fixture.is_file(), "witness needs a checked-in lockfile")
        check = runpy.run_path(str(HERE / "feature-unification.py"))["verify_shared_packages"]
        workspace = tomllib.loads((HERE.parents[2] / "Cargo.lock").read_text())
        witness = tomllib.loads(fixture.read_text())
        check(workspace, witness)
        for field, replacement in [("version", "9999.0.0"), ("checksum", "0" * 64)]:
            changed = copy.deepcopy(witness)
            package = next(p for p in changed["package"] if p["name"] == "reqwest")
            package[field] = replacement
            with self.assertRaisesRegex(AssertionError, "reqwest"):
                check(workspace, changed)

if __name__ == "__main__":
    unittest.main()
