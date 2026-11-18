"""Verify the session-view contract bundle: schema, fixtures, op vectors and captured scenarios.

This checks the published contract files against each other with an independent op applier.
It does not prove that any host or client implements the contract.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from itertools import pairwise
from pathlib import Path

from jsonschema import Draft7Validator

ROOT = Path(__file__).resolve().parents[2]
BUNDLE = ROOT / "backend/crates/oqto-protocol/contracts/session-view/v0"
CONTRACT = "oqto.session-view"
VERSION = 0
RESERVED = {"__proto__", "constructor", "prototype"}


class ContractError(Exception):
    """The bundle violates the contract."""


class OpRefusal(Exception):
    """An op must be refused; `code` is one of operations.json op_refusals."""

    def __init__(self, code: str) -> None:
        super().__init__(code)
        self.code = code


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def reject_floats(value: object, where: str) -> None:
    """Fixtures hold integers only, so canonical JSON is the same text in every language."""
    if isinstance(value, float):
        raise ContractError(f"{where}: non-integer number {value!r}")
    if isinstance(value, dict):
        for key, item in value.items():
            reject_floats(item, f"{where}.{key}")
    elif isinstance(value, list):
        for index, item in enumerate(value):
            reject_floats(item, f"{where}[{index}]")


def canonical(value: object) -> str:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


def value_sha(value: object) -> str:
    return sha256(canonical(value).encode("utf-8"))


# --- Independent op applier (mirrors upstream Chord's rules, written from the spec) ---------


def _is_index(value: object) -> bool:
    return isinstance(value, int) and not isinstance(value, bool)


def _check_path(path: object, non_empty: bool) -> list:
    if not isinstance(path, list):
        raise OpRefusal("invalid_op")
    if non_empty and not path:
        raise OpRefusal("invalid_op")
    for seg in path:
        if isinstance(seg, str):
            if seg in RESERVED:
                raise OpRefusal("unsafe_path")
        elif not _is_index(seg) or seg < 0:
            raise OpRefusal("unsafe_path")
    return path


def _validate(op: object) -> None:
    if not isinstance(op, list) or not op:
        raise OpRefusal("invalid_op")
    verb = op[0]
    if verb == "r":
        if len(op) != 2:
            raise OpRefusal("invalid_op")
    elif verb == "s":
        if len(op) != 3:
            raise OpRefusal("invalid_op")
        _check_path(op[1], True)
    elif verb == "d":
        if len(op) != 2:
            raise OpRefusal("invalid_op")
        _check_path(op[1], True)
    elif verb == "a":
        if len(op) != 3 or not isinstance(op[2], str):
            raise OpRefusal("invalid_op")
        _check_path(op[1], True)
    elif verb == "t":
        if len(op) != 3 or not _is_index(op[2]) or op[2] < 0:
            raise OpRefusal("invalid_op")
        _check_path(op[1], True)
    elif verb == "p":
        if len(op) != 5:
            raise OpRefusal("invalid_op")
        _check_path(op[1], False)
        if not _is_index(op[2]) or op[2] < 0 or not _is_index(op[3]) or op[3] < 0:
            raise OpRefusal("invalid_op")
        if not isinstance(op[4], list):
            raise OpRefusal("invalid_op")
    elif verb == "m":
        if len(op) != 3:
            raise OpRefusal("invalid_op")
        _check_path(op[1], False)
        permutation = op[2]
        if not isinstance(permutation, list):
            raise OpRefusal("invalid_op")
        seen: set[int] = set()
        for index in permutation:
            if not _is_index(index) or index < 0 or index >= len(permutation) or index in seen:
                raise OpRefusal("invalid_op")
            seen.add(index)
    else:
        raise OpRefusal("invalid_op")


def _step(node: object, seg: object) -> object:
    if isinstance(node, list):
        if not _is_index(seg):
            raise OpRefusal("unsafe_path")
        if seg >= len(node):
            raise OpRefusal("unresolvable_path")
        return node[seg]
    if isinstance(node, dict):
        key = str(seg) if _is_index(seg) else seg
        if key not in node:
            raise OpRefusal("unresolvable_path")
        return node[key]
    raise OpRefusal("unresolvable_path")


def _container(root: object, path: list) -> object:
    node = root
    for seg in path:
        node = _step(node, seg)
    if not isinstance(node, (dict, list)):
        raise OpRefusal("unresolvable_path")
    return node


def _drop_utf16(text: str, count: int) -> str:
    units = text.encode("utf-16-le", "surrogatepass")
    return units[2 * count :].decode("utf-16-le", "surrogatepass")


def apply_ops(value: object, ops: list) -> object:
    """Apply decoded ops in order. Mutates containers of `value`; returns the new root."""
    root = value
    for op in ops:
        _validate(op)
        verb = op[0]
        if verb == "r":
            root = op[1]
            continue
        path = op[1]
        if verb == "p":
            target = root if not path else _container(root, path)
            if not isinstance(target, list):
                raise OpRefusal("unresolvable_path")
            start = min(op[2], len(target))
            del target[start : start + op[3]]
            start = min(op[2], len(target))
            target[start:start] = op[4]
            continue
        if verb == "m":
            target = root if not path else _container(root, path)
            if not isinstance(target, list) or len(target) != len(op[2]):
                raise OpRefusal("unresolvable_path")
            previous = list(target)
            for index, source in enumerate(op[2]):
                target[index] = previous[source]
            continue
        parent = _container(root, path[:-1])
        key = path[-1]
        if isinstance(parent, list):
            if not _is_index(key):
                raise OpRefusal("unsafe_path")
            if key > len(parent):
                raise OpRefusal("unsafe_path")
            present = key < len(parent)
        else:
            key = str(key) if _is_index(key) else key
            present = key in parent
        if verb == "s":
            if isinstance(parent, list) and not present:
                parent.append(op[2])
            else:
                parent[key] = op[2]
        elif verb == "d":
            if isinstance(parent, list):
                if not present:
                    raise OpRefusal("unresolvable_path")
                del parent[key]
            elif present:
                del parent[key]
        else:
            current = parent[key] if present else None
            if not isinstance(current, str):
                raise OpRefusal("unresolvable_path")
            parent[key] = current + op[2] if verb == "a" else _drop_utf16(current, op[2])
    return root


# --- Bundle checks ---------------------------------------------------------------------------


def load(path: Path) -> object:
    data = json.loads(path.read_text(encoding="utf-8"))
    reject_floats(data, path.name)
    return data


def bundle_files(bundle: Path) -> dict[str, Path]:
    if bundle.is_symlink():
        raise ContractError("bundle must not be a symlink")
    members = list(bundle.rglob("*"))
    if any(p.is_symlink() or not (p.is_file() or p.is_dir()) for p in members):
        raise ContractError("bundle must contain only regular files and directories")
    return {p.relative_to(bundle).as_posix(): p for p in members if p.is_file()}


def expected_manifest(bundle: Path) -> dict:
    files = bundle_files(bundle)
    files.pop("manifest.json", None)
    return {
        "contract": CONTRACT,
        "version": VERSION,
        "files": {name: sha256(path.read_bytes()) for name, path in sorted(files.items())},
    }


def check_manifest(bundle: Path) -> None:
    path = bundle / "manifest.json"
    if not path.is_file():
        raise ContractError("manifest.json missing")
    if json.loads(path.read_text(encoding="utf-8")) != expected_manifest(bundle):
        raise ContractError("manifest.json does not match the bundle files (run with --write-manifest after a reviewed change)")


def check_fixtures(validator: Draft7Validator, fixtures: dict) -> tuple[int, int]:
    for case in fixtures["valid"]:
        errors = sorted(validator.iter_errors(case["frame"]), key=str)
        if errors:
            raise ContractError(f"valid fixture rejected: {case['name']}: {errors[0].message}")
    for case in fixtures["invalid"]:
        if validator.is_valid(case["frame"]):
            raise ContractError(f"invalid fixture accepted: {case['name']}")
        if not case.get("why"):
            raise ContractError(f"invalid fixture without a reason: {case['name']}")
    return len(fixtures["valid"]), len(fixtures["invalid"])


def check_vectors(vectors: dict, operations: dict) -> tuple[int, int]:
    refusals = set(operations["op_refusals"])
    refused = 0
    for case in vectors["vectors"]:
        name = case["name"]
        value = json.loads(json.dumps(case["value"]))
        if "error" in case:
            refused += 1
            if case["error"] not in refusals:
                raise ContractError(f"vector {name}: unknown refusal {case['error']}")
            try:
                apply_ops(value, case["ops"])
            except OpRefusal as refusal:
                if refusal.code != case["error"]:
                    raise ContractError(f"vector {name}: refused as {refusal.code}, expected {case['error']}") from None
            else:
                raise ContractError(f"vector {name}: applied, expected refusal {case['error']}")
        else:
            try:
                result = apply_ops(value, case["ops"])
            except OpRefusal as refusal:
                raise ContractError(f"vector {name}: refused as {refusal.code}, expected a result") from None
            if canonical(result) != canonical(case["result"]):
                raise ContractError(f"vector {name}: wrong result")
    return len(vectors["vectors"]), refused


def _paths(value: object, prefix: str, out: set[str]) -> None:
    """Collect the paths of a view value in the notation of operations.json stable_view_subset."""
    if isinstance(value, dict):
        for key, item in value.items():
            if "." in key or "/" in key:
                path = f'{prefix}["{key}"]'
            else:
                path = f"{prefix}.{key}" if prefix else key
            out.add(path)
            _paths(item, path, out)
    elif isinstance(value, list):
        for item in value:
            out.add(f"{prefix}[]")
            _paths(item, f"{prefix}[]", out)


def _expectations(
    scenario: dict, receipts: dict, errors: dict, client: dict, ops_seen: dict, ran: bool, splice: bool, track: dict
) -> None:
    name = scenario["scenario"]
    expect = scenario["expect"]
    final = scenario["final_value"]
    live = final["docs"].get("pi.live", {})
    checks = {
        "entry_kinds": lambda want: [entry["kind"] for entry in final["entries"]] == want,
        "receipts": lambda want: {key: receipts.get(key) for key in want} == want and set(receipts) >= set(want),
        "idle": lambda want: ("run" not in live) is want,
        "run_failure": lambda want: live.get("runFailure", {}).get("reason") == want,
        "never_ran": lambda want: (not ran) is want,
        "before": lambda want: final.get("before") == want,
        "splice_seen": lambda want: splice is want,
        "min_ops": lambda want: all(ops_seen.get(verb, 0) >= count for verb, count in want.items()),
        "assistant_entries": lambda want: sum(1 for entry in final["entries"] if entry["kind"] == "pi.assistant") == want,
        "gap": lambda want: any(track["gaps"]) is want,
        "pages": lambda want: track["pages"] == want,
        "covers_transcript": lambda want: (set(track["seen"]) == set(track["transcript"])) is want,
        "receipts_by_conn": lambda want: all(
            {request: track["receipts_by_conn"].get(conn, {}).get(request) for request in requests} == requests
            for conn, requests in want.items()
        ),
        "closed": lambda want: track["closed"] == want,
    }
    for key, want in expect.items():
        if key == "errors":
            resolved = {}
            for ref, code in want.items():
                ids = [ref] if ref in client else [cid for cid, frame in client.items() if frame.get("request_id") == ref and cid in errors]
                if len(ids) != 1 or errors.get(ids[0]) != code:
                    raise ContractError(f"scenario {name}: expected error {code} for {ref}")
                resolved[ids[0]] = code
            if resolved != errors:
                raise ContractError(f"scenario {name}: unexpected errors {sorted(set(errors) - set(resolved))}")
            continue
        if key not in checks:
            raise ContractError(f"scenario {name}: unknown expectation {key}")
        if not checks[key](want):
            raise ContractError(f"scenario {name}: expectation {key} does not hold")
    if "errors" not in expect and errors:
        raise ContractError(f"scenario {name}: errors without an expectation")


def check_scenario(
    scenario: dict,
    validator: Draft7Validator,
    view_validator: Draft7Validator,
    entry_validator: Draft7Validator,
    operations: dict,
    witnessed: set[str],
) -> int:
    name = scenario.get("scenario", "?")
    if scenario.get("contract") != CONTRACT or scenario.get("v") != VERSION:
        raise ContractError(f"scenario {name}: wrong contract or version")
    pinned = operations["upstream"]["@earendil-works/pi-durable"]
    if scenario["upstream"] != {"package": "@earendil-works/pi-durable", "version": pinned}:
        raise ContractError(f"scenario {name}: captured from another upstream than operations.json pins")
    transitions = operations["receipts"]["transitions"]
    # Steps without `conn` belong to connection "a"; its request keys are the plain frame ids.
    client: dict[str, dict] = {}
    answered: set[str] = set()
    attachments: dict[str, dict] = {}
    receipts_by_conn: dict[str, dict[str, list[str]]] = {}
    kinds: dict[str, str] = {}
    errors: dict[str, str] = {}
    ops_seen: dict[str, int] = {}
    closed: list[str] = []
    ran = False
    splice = False
    frames = 0
    transcript: dict[int, str] = {}
    for entry in scenario.get("full_transcript", []):
        problems = sorted(entry_validator.iter_errors(entry), key=str)
        if problems:
            raise ContractError(f"scenario {name}: transcript entry invalid: {problems[0].message}")
        transcript[entry["id"]] = canonical(entry)
    if not transcript and scenario["final_value"]["entries"]:
        raise ContractError(f"scenario {name}: full_transcript missing")
    seen: dict[int, str] = {}
    delivered: dict[str, set[int]] = {}
    stores: set[str] = set()
    track = {"gaps": [], "pages": 0, "seen": seen, "transcript": transcript}

    def key(conn: str, frame_id: str) -> str:
        return frame_id if conn == "a" else f"{conn}:{frame_id}"

    def remember(conn: str, entries: list, where: str) -> None:
        """Entries never change: every delivered entry equals the Session's entry with its id."""
        for entry in entries:
            text = canonical(entry)
            if transcript.get(entry["id"]) != text:
                raise ContractError(f"{where}: entry {entry['id']} differs from the Session's entry with that id")
            seen[entry["id"]] = text
            delivered.setdefault(conn, set()).add(entry["id"])

    def readable(conn: str, entry_id: int) -> bool:
        """Delivered on this connection, or at or below the `before` of one of its attachments."""
        if entry_id in delivered.get(conn, set()):
            return True
        return any(
            state["conn"] == conn and state["open"] and state["value"] is not None
            and entry_id <= state["value"].get("before", -1)
            for state in attachments.values()
        )

    steps = scenario["steps"]
    expected_close: set[int] = set()
    for index, step in enumerate(steps):
        where = f"scenario {name} step {index}"
        conn = step.get("conn", "a")
        if index in expected_close:
            continue
        if conn in closed:
            raise ContractError(f"{where}: traffic on connection {conn} after it was closed")
        if set(step) - {"conn"} == {"close"}:
            raise ContractError(f"{where}: connection closed without a request that requires it")
        if set(step) - {"conn"} == {"client"}:
            frame = step["client"]
            problems = sorted(validator.iter_errors(frame), key=str)
            # A scenario may send a request the schema refuses, to show the host's refusal.
            if problems and not _deliberately_invalid(frame):
                raise ContractError(f"{where}: client frame invalid: {problems[0].message}")
            request_key = key(conn, frame["id"])
            if request_key in client:
                # A reused id cannot be answered unambiguously: the host must close at once.
                following = steps[index + 1] if index + 1 < len(steps) else {}
                if following.get("conn", "a") != conn or set(following) - {"conn"} != {"close"}:
                    raise ContractError(f"{where}: reused frame id not answered by closing the connection")
                closed.append(conn)
                expected_close.add(index + 1)
                for state in attachments.values():
                    if state["conn"] == conn:
                        state["open"] = False
                continue
            frame = dict(frame)
            client[request_key] = frame
            continue
        if not set(step) - {"conn"} <= {"host", "value_sha256"} or "host" not in step:
            raise ContractError(f"{where}: unknown step shape")
        frame = step["host"]
        frames += 1
        problems = sorted(validator.iter_errors(frame), key=str)
        if problems:
            raise ContractError(f"{where}: host frame invalid: {problems[0].message}")
        kind = frame["type"]
        if "id" in frame:
            request_key = key(conn, frame["id"])
            if request_key not in client:
                raise ContractError(f"{where}: reply to an unknown request")
            if request_key in answered:
                raise ContractError(f"{where}: second reply to one request")
            answered.add(request_key)
        owner = attachments.get(frame.get("attachment", ""), {}).get("conn", conn)
        if kind in ("detached", "ended", "view") and owner != conn:
            raise ContractError(f"{where}: frame for another connection's attachment")
        if kind == "attached":
            stores.add(frame["store"])
            if len(stores) != 1:
                raise ContractError(f"{where}: one host announced different stores")
            window = client[key(conn, frame["id"])].get("window", {})
            attachments[frame["attachment"]] = {"seq": 0, "value": None, "open": True, "from": window.get("from"), "conn": conn}
        elif kind == "detached":
            attachments[frame["attachment"]]["open"] = False
        elif kind == "ended":
            state = attachments.get(frame["attachment"])
            if state is None or not state["open"]:
                raise ContractError(f"{where}: ended for an attachment that is not open")
            state["open"] = False
        elif kind == "view":
            state = attachments.get(frame["attachment"])
            if state is None:
                raise ContractError(f"{where}: view frame before attached")
            if not state["open"]:
                raise ContractError(f"{where}: view frame after the attachment closed")
            if frame["seq"] != state["seq"] + 1:
                raise ContractError(f"{where}: seq {frame['seq']} does not follow {state['seq']}")
            state["seq"] = frame["seq"]
            ops = frame["ops"]
            if frame["seq"] == 1:
                if not ops or ops[0][0] != "r":
                    raise ContractError(f"{where}: first view frame is not a base")
                base_problems = sorted(view_validator.iter_errors(ops[0][1]), key=str)
                if base_problems:
                    raise ContractError(f"{where}: base is not a conversation view: {base_problems[0].message}")
                if state["from"] is not None:
                    track["gaps"].append(ops[0][1].get("before") != state["from"] - 1)
            for op in ops:
                ops_seen[op[0]] = ops_seen.get(op[0], 0) + 1
                if op[0] == "s" and op[1] == ["docs", "pi.live", "run"]:
                    ran = True
                if op[0] == "p" and op[1] == ["entries"] and op[2] == 0 and op[3] > 0:
                    splice = True
            try:
                state["value"] = apply_ops(state["value"], json.loads(json.dumps(ops)))
            except OpRefusal as refusal:
                raise ContractError(f"{where}: op refused as {refusal.code}") from None
            if value_sha(state["value"]) != step.get("value_sha256"):
                raise ContractError(f"{where}: applied value does not match value_sha256")
            _paths(state["value"], "", witnessed)
            remember(conn, state["value"]["entries"], where)
        elif kind == "entries":
            _check_page(frame, client[key(conn, frame["id"])], where)
            remember(conn, frame["items"], where)
            track["pages"] += 1
        elif kind == "receipt":
            if any(state["open"] and state["conn"] == conn for state in attachments.values()):
                for field in ("entry", "answer"):
                    if field in frame and not readable(conn, frame[field]):
                        raise ContractError(f"{where}: receipt names entry {frame[field]} before it was delivered")
            request = frame["request_id"]
            history = receipts_by_conn.setdefault(conn, {}).setdefault(request, [])
            if kinds.setdefault(request, frame["kind"]) != frame["kind"]:
                raise ContractError(f"{where}: receipt kind changed")
            if history and frame["status"] != history[-1] and frame["status"] not in transitions[history[-1]]:
                raise ContractError(f"{where}: receipt went from {history[-1]} to {frame['status']}")
            history.append(frame["status"])
        elif kind == "error":
            errors[key(conn, frame["id"])] = frame["code"]
        if kind != "view" and "value_sha256" in step:
            raise ContractError(f"{where}: value_sha256 on a non-view frame")
    unanswered = set(client) - answered
    if unanswered:
        raise ContractError(f"scenario {name}: requests without a reply: {sorted(unanswered)}")
    state = attachments.get(scenario["attachment"])
    if state is None or canonical(state["value"]) != canonical(scenario["final_value"]):
        raise ContractError(f"scenario {name}: final_value is not what the frames produce")
    track["receipts_by_conn"] = receipts_by_conn
    track["closed"] = closed
    _expectations(scenario, receipts_by_conn.get("a", {}), errors, client, ops_seen, ran, splice, track)
    return frames


def _check_page(frame: dict, request: dict, where: str) -> None:
    """An entries reply stays inside its request: bounds, order and page size."""
    if request.get("type") != "read_entries":
        raise ContractError(f"{where}: entries reply to a {request.get('type')} request")
    ids = [entry["id"] for entry in frame["items"]]
    if len(ids) > request.get("limit", 50):
        raise ContractError(f"{where}: page larger than its limit")
    if any(entry_id < request.get("min", 0) or entry_id > request.get("max", entry_id) for entry_id in ids):
        raise ContractError(f"{where}: entry outside the requested range")
    descending = request.get("order", "descending") == "descending"
    if any((a <= b) if descending else (a >= b) for a, b in pairwise(ids)):
        raise ContractError(f"{where}: page not in the requested order")


def _deliberately_invalid(frame: dict) -> bool:
    """Requests a scenario sends on purpose although the schema refuses them."""
    kind = str(frame.get("entry", {}).get("kind", "")) if frame.get("type") == "write" else ""
    if kind.startswith("pi.") or kind == "oqto.ui":
        return True
    if frame.get("type") == "read_entries" and (
        frame.get("order", "descending") not in ("ascending", "descending") or not 1 <= frame.get("limit", 50) <= 200
    ):
        return True
    return frame.get("type") == "attach" and frame.get("v") != VERSION


def verify(bundle: Path) -> dict:
    check_manifest(bundle)
    schema = load(bundle / "frames.schema.json")
    Draft7Validator.check_schema(schema)
    validator = Draft7Validator(schema)
    view_validator = Draft7Validator({"$ref": "#/definitions/conversation_view", "definitions": schema["definitions"]})
    entry_validator = Draft7Validator({"$ref": "#/definitions/entry", "definitions": schema["definitions"]})
    operations = load(bundle / "operations.json")
    if operations.get("contract") != CONTRACT or operations.get("version") != VERSION:
        raise ContractError("operations.json: wrong contract or version")
    code_enum = set(schema["definitions"]["error"]["properties"]["code"]["enum"])
    if code_enum != set(operations["error_codes"]):
        raise ContractError("error codes differ between schema and operations.json")
    if set(schema["definitions"]["receipt"]["properties"]["status"]["enum"]) != set(operations["receipts"]["statuses"]):
        raise ContractError("receipt statuses differ between schema and operations.json")
    if set(schema["definitions"]["receipt"]["properties"]["kind"]["enum"]) != set(operations["receipts"]["kinds"]):
        raise ContractError("receipt kinds differ between schema and operations.json")
    if set(schema["definitions"]["ended"]["properties"]["reason"]["enum"]) != set(operations["ended_reasons"]):
        raise ContractError("ended reasons differ between schema and operations.json")
    client_types = {ref["$ref"].rsplit("/", 1)[1] for ref in schema["definitions"]["client_frame"]["oneOf"]}
    if client_types != set(operations["client_requests"]):
        raise ContractError("client requests differ between schema and operations.json")
    host_types = {ref["$ref"].rsplit("/", 1)[1] for ref in schema["definitions"]["host_frame"]["oneOf"]}
    if host_types != set(operations["host_frames"]):
        raise ContractError("host frames differ between schema and operations.json")
    valid, invalid = check_fixtures(validator, load(bundle / "fixtures.json"))
    vectors = load(bundle / "ops-vectors.json")
    if vectors["source"] != {"package": "@earendil-works/chord", "version": operations["upstream"]["@earendil-works/chord"]}:
        raise ContractError("ops-vectors.json: produced by another Chord than operations.json pins")
    total, refused = check_vectors(vectors, operations)
    scenarios = sorted((bundle / "scenarios").glob("*.json"))
    if not scenarios:
        raise ContractError("no scenarios")
    witnessed: set[str] = set()
    frames = sum(check_scenario(load(path), validator, view_validator, entry_validator, operations, witnessed) for path in scenarios)
    unwitnessed = sorted(set(operations["stable_view_subset"]) - witnessed)
    if unwitnessed:
        raise ContractError(f"stable_view_subset paths that no scenario shows: {unwitnessed}")
    return {
        "fixtures_valid": valid,
        "fixtures_invalid": invalid,
        "op_vectors": total,
        "op_refusals": refused,
        "scenarios": len(scenarios),
        "scenario_host_frames": frames,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bundle", type=Path, default=BUNDLE)
    parser.add_argument("--write-manifest", action="store_true", help="rewrite manifest.json from the bundle files")
    args = parser.parse_args()
    if args.write_manifest:
        (args.bundle / "manifest.json").write_text(json.dumps(expected_manifest(args.bundle), indent=2) + "\n", encoding="utf-8")
    try:
        counts = verify(args.bundle)
    except ContractError as error:
        print(f"session-view contract: FAIL: {error}", file=sys.stderr)
        return 1
    print("session-view contract: ok " + " ".join(f"{key}={value}" for key, value in counts.items()))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
