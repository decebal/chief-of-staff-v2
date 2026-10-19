import copy
import json
import tempfile
import threading
import unittest
from pathlib import Path
import state


class StateTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        state.initialize(self.root)
        self.demo = state.load(Path(__file__).resolve().parent.parent / "assets/demo-state.json")

    def current(self):
        return state.load(self.root / ".chief-of-staff/state.json")

    def test_empty_and_demo_validate(self):
        state.validate(self.current())
        state.validate(self.demo)

    def test_initialize_does_not_overwrite(self):
        with self.assertRaises(FileExistsError):
            state.initialize(self.root)
        self.assertEqual(self.current()["revision"], 0)

    def test_defaults_have_no_permission_or_schedule(self):
        profile = state.load(self.root / ".chief-of-staff/profile.json")
        self.assertFalse(profile["schedule"]["enabled"])
        self.assertFalse(any(profile["permissions"].values()))
        self.assertFalse(profile["recovery"]["automaticRateLimitProbe"])
        self.assertEqual(profile["sources"], [])

    def test_commit_and_readback(self):
        state.commit(self.root, self.demo, 0, "Use fictional fixture")
        saved = self.current()
        self.assertEqual(saved["revision"], 1)
        self.assertEqual(saved["deliverables"], self.demo["deliverables"])
        self.assertEqual(len(saved["journal"]), 1)

    def test_stale_writer_rejected(self):
        state.commit(self.root, self.demo, 0, "First writer")
        with self.assertRaisesRegex(ValueError, "Revision conflict"):
            state.commit(self.root, self.demo, 0, "Stale writer")
        self.assertEqual(self.current()["revision"], 1)

    def test_concurrent_writers_only_one_wins(self):
        gate, results = threading.Barrier(2), []
        def writer():
            gate.wait()
            try:
                state.commit(self.root, self.demo, 0, "Concurrent fixture")
                results.append("saved")
            except (ValueError, FileExistsError):
                results.append("stopped")
        threads = [threading.Thread(target=writer) for _ in range(2)]
        for thread in threads:
            thread.start()
        for thread in threads:
            thread.join()
        self.assertCountEqual(results, ["saved", "stopped"])
        self.assertEqual(self.current()["revision"], 1)

    def test_lock_is_not_stolen(self):
        lock = self.root / ".chief-of-staff/write.lock"
        lock.touch()
        with self.assertRaises(FileExistsError):
            state.commit(self.root, self.demo, 0, "Must stop")
        self.assertTrue(lock.exists())

    def test_reviewing_waiting_does_not_complete(self):
        state.commit(self.root, self.demo, 0, "Fixture")
        updated = self.current()
        updated["commitments"][0]["reviewedAt"] = "2030-04-09T12:00:00Z"
        state.commit(self.root, updated, 1, "Acknowledge")
        item = self.current()["commitments"][0]
        self.assertEqual(item["status"], "waiting")
        self.assertIsNone(item["dueDate"])
        self.assertIsNotNone(item["meetingAt"])

    def test_completion_needs_evidence(self):
        self.demo["deliverables"][0]["status"] = "complete"
        with self.assertRaisesRegex(ValueError, "requires evidence"):
            state.validate(self.demo)

    def test_completed_work_cannot_be_reopened(self):
        self.demo["deliverables"][0].update(status="complete", completionEvidence={"reference":"https://example.com/fictional/receipt","verifiedAt":"2030-04-09T12:00:00Z"})
        state.commit(self.root, self.demo, 0, "Verified fixture")
        updated = self.current()
        updated["deliverables"][0]["status"] = "open"
        with self.assertRaisesRegex(ValueError, "cannot be reopened"):
            state.commit(self.root, updated, 1, "Bad reopening")

    def test_record_deletion_rejected(self):
        state.commit(self.root, self.demo, 0, "Fixture")
        updated = self.current()
        updated["commitments"] = []
        with self.assertRaisesRegex(ValueError, "cannot be deleted"):
            state.commit(self.root, updated, 1, "Bad deletion")

    def test_journal_tampering_rejected(self):
        state.commit(self.root, self.demo, 0, "Fixture")
        updated = self.current()
        updated["journal"][0]["reason"] = "Rewritten"
        with self.assertRaisesRegex(ValueError, "journal is immutable"):
            state.commit(self.root, updated, 1, "Bad history")

    def test_missing_goal_rejected(self):
        self.demo["deliverables"][0]["goalId"] = "absent"
        with self.assertRaisesRegex(ValueError, "Missing linked goal"):
            state.validate(self.demo)

    def test_dependency_cycle_rejected(self):
        second = copy.deepcopy(self.demo["deliverables"][0])
        second.update(id="D-2", dependencies=["D-1"])
        self.demo["deliverables"][0]["dependencies"] = ["D-2"]
        self.demo["deliverables"].append(second)
        with self.assertRaisesRegex(ValueError, "Cyclic"):
            state.validate(self.demo)

    def test_measured_kpi_requires_evidence(self):
        self.demo["kpis"][0]["current"] = 0
        with self.assertRaisesRegex(ValueError, "requires source"):
            state.validate(self.demo)

    def test_source_probe_does_not_advance_coverage(self):
        state.commit(self.root, self.demo, 0, "Fixture")
        updated = self.current()
        updated["sources"][0]["checkedAt"] = "2030-04-09T12:00:00Z"
        state.commit(self.root, updated, 1, "Healthy probe only")
        self.assertEqual(self.current()["sources"][0]["review"], "partial")
        self.assertIsNone(self.current()["sources"][0]["lastCompleteThrough"])

    def test_partial_review_cannot_advance_watermark(self):
        state.commit(self.root, self.demo, 0, "Fixture")
        updated = self.current()
        updated["sources"][0]["lastCompleteThrough"] = "2030-04-09T09:00:00Z"
        with self.assertRaisesRegex(ValueError, "Partial review"):
            state.commit(self.root, updated, 1, "Incomplete coverage")

    def test_original_denial_preserved(self):
        self.demo["incidents"] = [{"id":"I-1","sourceId":"email","kind":"permission_denied","observedAt":"2030-04-09T09:00:00Z"}]
        state.commit(self.root, self.demo, 0, "Record fixture denial")
        updated = self.current()
        updated["incidents"][0]["kind"] = "rate_limit"
        with self.assertRaisesRegex(ValueError, "identity is immutable"):
            state.commit(self.root, updated, 1, "Bad relabel")

    def test_schema_and_unknown_fields_rejected(self):
        for changed in ({"schemaVersion":2}, {"unexpected":"data"}):
            with self.assertRaises(ValueError):
                state.validate({**self.demo, **changed})

    def test_invalid_candidate_preserves_state_and_releases_lock(self):
        before = self.current()
        with self.assertRaises(ValueError):
            state.commit(self.root, {}, 0, "Invalid candidate")
        self.assertEqual(self.current(), before)
        self.assertFalse((self.root / ".chief-of-staff/write.lock").exists())

    def test_symlinked_runtime_rejected(self):
        other = self.root / "other"
        other.mkdir()
        (other / ".chief-of-staff").symlink_to(self.root / ".chief-of-staff", target_is_directory=True)
        with self.assertRaisesRegex(ValueError, "symlinked"):
            state.runtime_dir(other)


if __name__ == "__main__":
    unittest.main()
