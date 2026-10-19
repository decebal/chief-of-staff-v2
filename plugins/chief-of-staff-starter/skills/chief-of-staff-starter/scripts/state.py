#!/usr/bin/env python3
"""Local, single-workspace state. No network calls or permission delegation."""
import argparse
import copy
import json
import math
import os
import tempfile
from datetime import date, datetime, timezone
from pathlib import Path

COLLECTIONS = ("goals", "kpis", "deliverables", "commitments", "sources", "runs", "incidents")
TERMINAL = {"complete", "cancelled"}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def timestamp(value):
    require(isinstance(value, str), "Timestamp must be an ISO string")
    result = datetime.fromisoformat(value.replace("Z", "+00:00"))
    require(result.tzinfo is not None, "Timestamp must include a timezone")
    return result


def nonempty(value):
    return isinstance(value, str) and bool(value.strip())


def load(path):
    require(not path.is_symlink(), "Refusing a symlinked data file")
    with path.open(encoding="utf-8") as handle:
        return json.load(handle)


def empty_state():
    return {"schemaVersion": 1, "revision": 0, **{key: [] for key in COLLECTIONS}, "journal": []}


def validate(state):
    require(isinstance(state, dict), "State must be an object")
    require(set(state) == {"schemaVersion", "revision", "journal", *COLLECTIONS}, "Unexpected or missing top-level state fields")
    require(type(state["schemaVersion"]) is int and state["schemaVersion"] == 1, "Unsupported schema version")
    require(type(state["revision"]) is int and state["revision"] >= 0, "Invalid revision")
    ids = {}
    for collection in COLLECTIONS:
        records = state[collection]
        require(isinstance(records, list), f"{collection} must be a list")
        ids[collection] = set()
        for record in records:
            require(isinstance(record, dict) and nonempty(record.get("id")), f"Invalid {collection} record ID")
            require(record["id"] not in ids[collection], f"Duplicate ID in {collection}")
            ids[collection].add(record["id"])
            for field in ("targetDate", "dueDate", "week"):
                if record.get(field) is not None:
                    require(isinstance(record[field], str), f"{field} must be a date string")
                    require(date.fromisoformat(record[field]).isoformat() == record[field], f"Invalid {field}")
            for field in ("evidenceAt", "checkedAt", "reviewedAt", "meetingAt", "invokedAt", "finishedAt", "observedAt", "retryNotBefore", "recoveryClaimedAt", "resolvedAt", "lastCompleteThrough", "windowStart", "windowEnd"):
                if record.get(field) is not None:
                    timestamp(record[field])
            if collection in ("goals", "deliverables", "commitments"):
                require(nonempty(record.get("title")), "Tracked work needs a title")
                require(record.get("owner") is None or nonempty(record["owner"]), "Invalid owner")
                require(record.get("status") in {"proposed", "open", "active", "in_progress", "blocked", "waiting", "paused", *TERMINAL}, "Invalid work status")
                if record["status"] in TERMINAL:
                    evidence = record.get("completionEvidence")
                    require(isinstance(evidence, dict) and nonempty(evidence.get("reference")), "Terminal work requires evidence")
                    timestamp(evidence.get("verifiedAt"))
            if collection == "goals":
                require(nonempty(record.get("finishLine")), "Goal requires a finish line")
            if collection == "deliverables":
                require(nonempty(record.get("doneWhen")), "Deliverable requires acceptance criteria")
                require(record.get("ownerBasis") in {"proposed", "confirmed", "unknown"}, "Invalid owner basis")
                require(record.get("dueBasis") in {"proposed", "confirmed", "unknown"}, "Invalid due-date basis")
                require(record.get("dueDate") is None or record["dueBasis"] != "unknown", "Date needs a stated basis")
                require(isinstance(record.get("dependencies", []), list), "Dependencies must be a list")
            if collection == "commitments":
                require(record["status"] in TERMINAL or nonempty(record.get("nextReview")), "Open commitment requires a review trigger")
            if collection == "kpis":
                require(nonempty(record.get("name")) and nonempty(record.get("unit")), "KPI needs a name and unit")
                for field in ("baseline", "target", "current"):
                    value = record.get(field)
                    require(value is None or (type(value) in (int, float) and math.isfinite(value)), "KPI values must be finite numbers or null")
                if record.get("current") is not None:
                    require(nonempty(record.get("source")) and nonempty(record.get("reportingWindow")), "Measured KPI requires source and window")
                    timestamp(record.get("evidenceAt"))
            if collection == "sources":
                require(record.get("health") in {"not_connected", "healthy", "rate_limited", "permission_denied", "auth_failed", "unavailable"}, "Invalid source health")
                require(record.get("review") in {"not_reviewed", "partial", "complete"}, "Invalid review completeness")
                if record.get("windowStart") and record.get("windowEnd"):
                    require(timestamp(record["windowStart"]) <= timestamp(record["windowEnd"]), "Invalid source window")
                if record["review"] == "complete":
                    require(record.get("lastCompleteThrough") and record.get("windowStart") and record.get("windowEnd"), "Complete review needs a bounded window and watermark")
                    require(timestamp(record["lastCompleteThrough"]) == timestamp(record["windowEnd"]), "Complete watermark must match the reviewed window end")
                    require(timestamp(record["checkedAt"]) >= timestamp(record["windowEnd"]), "Review cannot cover a future window")
            if collection == "runs":
                require(record.get("status") in {"running", "complete", "partial", "blocked", "interrupted"}, "Invalid run status")
                timestamp(record.get("invokedAt"))
                if record["status"] == "complete":
                    require(timestamp(record.get("finishedAt")) >= timestamp(record["invokedAt"]), "Invalid completion time")
            if collection == "incidents":
                require(record.get("kind") in {"rate_limit", "permission_denied", "auth_failed", "unknown"}, "Invalid incident kind")
                timestamp(record.get("observedAt"))
                if record.get("resolvedAt"):
                    require(nonempty(record.get("resolutionRef")), "Resolved incident needs a resolution reference")
    for record in state["deliverables"] + state["kpis"]:
        require(record.get("goalId") in ids["goals"], "Missing linked goal")
    for record in state["deliverables"]:
        for dependency in record.get("dependencies", []):
            require(dependency in ids["deliverables"] and dependency != record["id"], "Invalid deliverable dependency")
    graph = {record["id"]: record.get("dependencies", []) for record in state["deliverables"]}
    visiting, visited = set(), set()

    def visit(node):
        require(node not in visiting, "Cyclic deliverable dependencies")
        if node not in visited:
            visiting.add(node)
            for child in graph[node]:
                visit(child)
            visiting.remove(node)
            visited.add(node)

    for node in graph:
        visit(node)
    for record in state["commitments"]:
        require(record.get("goalId") is None or record["goalId"] in ids["goals"], "Missing commitment goal")
    for record in state["incidents"]:
        require(record.get("sourceId") in ids["sources"], "Incident requires a known source")
    require(isinstance(state["journal"], list) and len(state["journal"]) == state["revision"], "Journal must match revision")
    for number, entry in enumerate(state["journal"], 1):
        require(isinstance(entry, dict) and entry.get("revision") == number and nonempty(entry.get("reason")), "Invalid journal entry")
        timestamp(entry.get("at"))
    return state


def validate_transition(old, new):
    validate(old)
    validate(new)
    require(new["journal"] == old["journal"], "Prior journal is immutable")
    require(new["revision"] == old["revision"], "Candidate must preserve current revision")
    for collection in COLLECTIONS:
        before = {item["id"]: item for item in old[collection]}
        after = {item["id"]: item for item in new[collection]}
        require(before.keys() <= after.keys(), "Existing records cannot be deleted by a routine commit")
        for key, prior in before.items():
            updated = after[key]
            if collection in ("goals", "deliverables", "commitments") and prior["status"] in TERMINAL:
                require(updated["status"] == prior["status"] and updated.get("completionEvidence") == prior.get("completionEvidence"), "Completed history cannot be reopened or rewritten")
            if collection == "incidents":
                for field in ("sourceId", "kind", "observedAt"):
                    require(prior.get(field) == updated.get(field), "Original incident identity is immutable")
                for field in ("recoveryClaimedAt", "recoveryOutcome", "resolvedAt", "resolutionRef"):
                    if prior.get(field) is not None:
                        require(prior[field] == updated.get(field), "Existing recovery history is immutable")
            if collection == "runs":
                require(prior["invokedAt"] == updated["invokedAt"], "Original invocation time is immutable")
                if prior["status"] == "complete":
                    require(prior == updated, "Completed run receipt is immutable")
            if collection == "sources":
                previous, current = prior.get("lastCompleteThrough"), updated.get("lastCompleteThrough")
                if previous:
                    require(current and timestamp(current) >= timestamp(previous), "Review watermark cannot move backward")
                if current != previous and current:
                    require(updated["review"] == "complete", "Partial review cannot advance the watermark")
                    if previous:
                        require(timestamp(updated["windowStart"]) <= timestamp(previous), "Review cannot skip a coverage gap")


def runtime_dir(workspace):
    path = Path(workspace)
    require(path.is_absolute() and path.is_dir(), "Choose an existing absolute workspace directory")
    root = path.resolve()
    plugin = Path(__file__).resolve().parents[3]
    require(root != plugin and plugin not in root.parents, "Runtime must be outside the installed plugin")
    result = root / ".chief-of-staff"
    require(not result.is_symlink(), "Refusing a symlinked runtime directory")
    return result


def atomic_write(path, value):
    descriptor, name = tempfile.mkstemp(prefix=".state-", suffix=".tmp", dir=path.parent)
    try:
        with os.fdopen(descriptor, "w", encoding="utf-8") as handle:
            json.dump(value, handle, indent=2, ensure_ascii=False, allow_nan=False)
            handle.write("\n")
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(name, path)
        if os.name == "posix":
            directory = os.open(path.parent, os.O_RDONLY)
            try:
                os.fsync(directory)
            finally:
                os.close(directory)
    finally:
        if os.path.exists(name):
            os.unlink(name)


def initialize(workspace):
    folder = runtime_dir(workspace)
    folder.mkdir(mode=0o700, exist_ok=False)
    # A partial initialization intentionally remains visible; never overwrite it.
    profile = load(Path(__file__).resolve().parent.parent / "assets/profile.example.json")
    atomic_write(folder / "profile.json", profile)
    atomic_write(folder / "state.json", empty_state())
    return {"initialized": True, "revision": 0, "scheduleEnabled": False}


def commit(workspace, candidate, expected_revision, reason):
    require(nonempty(reason) and len(reason) <= 300, "Use a short nonempty commit reason")
    folder = runtime_dir(workspace)
    lock = folder / "write.lock"
    descriptor = os.open(lock, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
    try:
        os.write(descriptor, str(os.getpid()).encode())
        os.close(descriptor)
        descriptor = None
        path = folder / "state.json"
        old = validate(load(path))
        require(old["revision"] == expected_revision, "Revision conflict: reread and reconcile")
        new = copy.deepcopy(candidate)
        validate_transition(old, new)
        require(new != old, "No state change to commit")
        new["revision"] += 1
        new["journal"].append({"revision": new["revision"], "at": datetime.now(timezone.utc).isoformat(), "reason": reason})
        validate(new)
        atomic_write(path, new)
        return {"committed": True, "revision": new["revision"]}
    finally:
        if descriptor is not None:
            os.close(descriptor)
        lock.unlink()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    for command in ("init", "read", "commit"):
        sub = commands.add_parser(command)
        sub.add_argument("--workspace", required=True)
        if command == "commit":
            sub.add_argument("--candidate", required=True)
            sub.add_argument("--expected-revision", required=True, type=int)
            sub.add_argument("--reason", required=True)
    check = commands.add_parser("check")
    check.add_argument("--file", required=True)
    args = parser.parse_args()
    try:
        if args.command == "init":
            result = initialize(args.workspace)
        elif args.command == "read":
            result = validate(load(runtime_dir(args.workspace) / "state.json"))
        elif args.command == "check":
            validate(load(Path(args.file)))
            result = {"valid": True}
        else:
            result = commit(args.workspace, load(Path(args.candidate)), args.expected_revision, args.reason)
        print(json.dumps(result, indent=2, allow_nan=False))
    except (ValueError, OSError, TypeError, KeyError, RecursionError) as error:
        parser.exit(1, f"Stopped: {type(error).__name__}: {error}\n")


if __name__ == "__main__":
    main()
