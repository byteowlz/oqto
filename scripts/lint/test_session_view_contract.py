"""Tamper tests for the session-view contract verifier; no runtime access."""

import json
import shutil
import tempfile
import unittest
from pathlib import Path

from session_view_contract import (
    BUNDLE,
    ContractError,
    OpRefusal,
    apply_ops,
    expected_manifest,
    verify,
)


class SessionViewContractTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="session-view-contract-")
        self.addCleanup(self.temp.cleanup)
        self.bundle = Path(self.temp.name) / "v0"
        shutil.copytree(BUNDLE, self.bundle)

    def edit(self, name, change):
        """Change one bundle file and re-seal the manifest, so only the content rule can fail."""
        path = self.bundle / name
        data = json.loads(path.read_text(encoding="utf-8"))
        change(data)
        path.write_text(json.dumps(data, indent=1) + "\n", encoding="utf-8")
        (self.bundle / "manifest.json").write_text(json.dumps(expected_manifest(self.bundle)), encoding="utf-8")

    def refused(self, pattern):
        return self.assertRaisesRegex(ContractError, pattern)

    def host_steps(self, scenario, kind):
        return [index for index, step in enumerate(scenario["steps"]) if step.get("host", {}).get("type") == kind]

    def test_committed_bundle_verifies(self):
        counts = verify(self.bundle)
        self.assertEqual(counts["scenarios"], 12)
        self.assertEqual(counts["op_vectors"], 38)
        self.assertEqual(counts["op_refusals"], 18)
        self.assertEqual(counts["fixtures_invalid"], 41)

    def test_extra_file_refused(self):
        (self.bundle / "extra.json").write_text("{}\n", encoding="utf-8")
        with self.refused("manifest"):
            verify(self.bundle)

    def test_changed_spec_refused(self):
        with (self.bundle / "spec.md").open("a", encoding="utf-8") as handle:
            handle.write("\nA receipt means approved.\n")
        with self.refused("manifest"):
            verify(self.bundle)

    def test_symlink_refused(self):
        (self.bundle / "link.json").symlink_to(self.bundle / "fixtures.json")
        with self.refused("regular files"):
            verify(self.bundle)

    def test_invalid_fixture_that_the_schema_accepts_is_refused(self):
        def change(fixtures):
            fixtures["invalid"].append({"name": "really valid", "frame": {"type": "detach", "id": "c1", "attachment": "att_1"}, "why": "x"})

        self.edit("fixtures.json", change)
        with self.refused("invalid fixture accepted: really valid"):
            verify(self.bundle)

    def test_valid_fixture_that_the_schema_refuses_is_refused(self):
        def change(fixtures):
            fixtures["valid"].append({"name": "approve", "frame": {"type": "approve", "id": "c1"}})

        self.edit("fixtures.json", change)
        with self.refused("valid fixture rejected: approve"):
            verify(self.bundle)

    def test_schema_that_accepts_unknown_fields_is_refused(self):
        def change(schema):
            schema["definitions"]["attach"]["additionalProperties"] = True

        self.edit("frames.schema.json", change)
        with self.refused("invalid fixture accepted: attach with caller identity"):
            verify(self.bundle)

    def test_error_codes_must_match_between_schema_and_operations(self):
        self.edit("operations.json", lambda operations: operations["error_codes"].pop("busy"))
        with self.refused("error codes differ"):
            verify(self.bundle)

    def test_wrong_vector_result_refused(self):
        def change(vectors):
            case = next(case for case in vectors["vectors"] if case["name"] == "d removes an array element and shifts the rest")
            case["result"] = {"xs": ["a", "c"]}

        self.edit("ops-vectors.json", change)
        with self.refused("wrong result"):
            verify(self.bundle)

    def test_wrong_vector_refusal_class_refused(self):
        def change(vectors):
            case = next(case for case in vectors["vectors"] if case["name"] == "reserved segment __proto__ is refused")
            case["error"] = "unresolvable_path"

        self.edit("ops-vectors.json", change)
        with self.refused("refused as unsafe_path"):
            verify(self.bundle)

    def test_vector_from_another_chord_refused(self):
        self.edit("ops-vectors.json", lambda vectors: vectors["source"].update(version="9.9.9"))
        with self.refused("another Chord"):
            verify(self.bundle)

    def test_dropped_view_frame_refused(self):
        def change(scenario):
            del scenario["steps"][self.host_steps(scenario, "view")[2]]

        self.edit("scenarios/prompt-answer.json", change)
        with self.refused("seq 4 does not follow 2"):
            verify(self.bundle)

    def test_changed_streamed_text_refused(self):
        def change(scenario):
            for index in self.host_steps(scenario, "view"):
                for op in scenario["steps"][index]["host"]["ops"]:
                    if op[0] == "a":
                        op[2] = op[2] + "!"
                        return
            raise AssertionError("no append op")

        self.edit("scenarios/prompt-answer.json", change)
        with self.refused("does not match value_sha256"):
            verify(self.bundle)

    def test_final_value_must_be_what_the_frames_produce(self):
        self.edit("scenarios/tool-round.json", lambda scenario: scenario["final_value"]["entries"].pop())
        with self.refused("final_value"):
            verify(self.bundle)

    def test_receipt_going_backwards_refused(self):
        def change(scenario):
            last = self.host_steps(scenario, "receipt")[-1]
            frame = scenario["steps"][last]["host"]
            self.assertEqual(frame["status"], "done")
            scenario["steps"].append({"host": {"type": "receipt", "session": frame["session"], "request_id": frame["request_id"], "kind": frame["kind"], "status": "placed", "entry": frame["entry"]}})

        self.edit("scenarios/prompt-answer.json", change)
        with self.refused("from done to placed"):
            verify(self.bundle)

    def test_request_without_reply_refused(self):
        def change(scenario):
            index = next(i for i, step in enumerate(scenario["steps"]) if step.get("host", {}).get("type") == "error")
            del scenario["steps"][index]

        self.edit("scenarios/passive-write.json", change)
        with self.refused("without a reply"):
            verify(self.bundle)

    def test_ui_part_without_fallback_refused(self):
        def change(scenario):
            part = next(e for e in scenario["full_transcript"] if e["kind"] == "oqto.ui")
            del part["data"]["part"]["fallback"]

        self.edit("scenarios/ui-part.json", change)
        with self.refused("transcript entry invalid"):
            verify(self.bundle)

    def test_second_reply_refused(self):
        def change(scenario):
            index = self.host_steps(scenario, "attached")[0]
            scenario["steps"].insert(index + 1, scenario["steps"][index])

        self.edit("scenarios/passive-write.json", change)
        with self.refused("second reply"):
            verify(self.bundle)

    def test_view_frame_after_end_refused(self):
        def change(scenario):
            views = self.host_steps(scenario, "view")
            ended = self.host_steps(scenario, "ended")[0]
            scenario["steps"].append(scenario["steps"].pop(min(ended, views[-1])))

        self.edit("scenarios/model-error.json", change)
        with self.refused("after the attachment closed"):
            verify(self.bundle)

    def test_unsafe_op_in_a_scenario_refused(self):
        def change(scenario):
            index = self.host_steps(scenario, "view")[1]
            scenario["steps"][index]["host"]["ops"].append(["s", ["docs", "pi.live", "x", "y", "z"], 1])

        self.edit("scenarios/prompt-answer.json", change)
        with self.refused("op refused as unresolvable_path"):
            verify(self.bundle)

    def test_expectation_must_hold(self):
        self.edit("scenarios/abort.json", lambda scenario: scenario["expect"].update(run_failure="model_error"))
        with self.refused("expectation run_failure"):
            verify(self.bundle)

    def test_unexpected_error_refused(self):
        self.edit("scenarios/busy-inputs.json", lambda scenario: scenario["expect"]["errors"].pop("req-busy-2"))
        with self.refused("unexpected errors"):
            verify(self.bundle)

    def test_scenario_from_another_upstream_refused(self):
        self.edit("scenarios/abort.json", lambda scenario: scenario["upstream"].update(version="1.5.1"))
        with self.refused("another upstream"):
            verify(self.bundle)

    def test_stable_path_without_a_witness_refused(self):
        self.edit("operations.json", lambda operations: operations["stable_view_subset"].append('docs["pi.live"].approval'))
        with self.refused("no scenario shows"):
            verify(self.bundle)

    def entries_steps(self, scenario):
        return self.host_steps(scenario, "entries")

    def test_delivered_entry_that_changed_is_refused(self):
        def change(scenario):
            page = scenario["steps"][self.entries_steps(scenario)[0]]["host"]
            page["items"][0]["kind"] = "pi.edited"

        self.edit("scenarios/fill-gap.json", change)
        with self.refused("differs from the Session's entry"):
            verify(self.bundle)

    def test_page_out_of_order_refused(self):
        def change(scenario):
            page = scenario["steps"][self.entries_steps(scenario)[0]]["host"]
            page["items"].reverse()

        self.edit("scenarios/load-earlier.json", change)
        with self.refused("not in the requested order"):
            verify(self.bundle)

    def test_page_outside_its_range_refused(self):
        def change(scenario):
            index = self.entries_steps(scenario)[0]
            request_id = scenario["steps"][index]["host"]["id"]
            request = next(step["client"] for step in scenario["steps"] if step.get("client", {}).get("id") == request_id)
            request["max"] = request["min"]

        self.edit("scenarios/fill-gap.json", change)
        with self.refused("outside the requested range"):
            verify(self.bundle)

    def test_receipt_before_its_entry_refused(self):
        def change(scenario):
            receipt_index = next(
                i for i, step in enumerate(scenario["steps"]) if step.get("host", {}).get("status") == "placed"
            )
            first_view = self.host_steps(scenario, "view")[1]
            scenario["steps"].insert(first_view, scenario["steps"].pop(receipt_index))

        self.edit("scenarios/prompt-answer.json", change)
        with self.refused("before it was delivered"):
            verify(self.bundle)

    def test_store_change_within_one_host_refused(self):
        def change(scenario):
            second = self.host_steps(scenario, "attached")[1]
            scenario["steps"][second]["host"]["store"] = "store_other"

        self.edit("scenarios/fill-gap.json", change)
        with self.refused("different stores"):
            verify(self.bundle)

    def test_missing_entries_fail_coverage(self):
        def change(scenario):
            del scenario["steps"][self.entries_steps(scenario)[-1]]
            last_client = max(i for i, step in enumerate(scenario["steps"]) if step.get("client", {}).get("type") == "read_entries")
            del scenario["steps"][last_client]
            scenario["expect"]["pages"] -= 1

        self.edit("scenarios/fill-gap.json", change)
        with self.refused("expectation covers_transcript"):
            verify(self.bundle)

    def test_gap_expectation_must_hold(self):
        self.edit("scenarios/window-resume.json", lambda scenario: scenario["expect"].update(gap=True))
        with self.refused("expectation gap"):
            verify(self.bundle)

    def test_operations_must_list_every_client_request(self):
        self.edit("operations.json", lambda operations: operations["client_requests"].pop("read_entries"))
        with self.refused("client requests differ"):
            verify(self.bundle)

    def test_reused_id_must_close_the_connection(self):
        def change(scenario):
            index = next(i for i, step in enumerate(scenario["steps"]) if "close" in step)
            del scenario["steps"][index]

        self.edit("scenarios/two-clients.json", change)
        with self.refused("reused frame id not answered by closing"):
            verify(self.bundle)

    def test_traffic_after_close_refused(self):
        def change(scenario):
            receipt = next(step for step in scenario["steps"] if step.get("conn") == "b" and step.get("host", {}).get("type") == "receipt")
            scenario["steps"].insert(len(scenario["steps"]) - 1, receipt)

        self.edit("scenarios/two-clients.json", change)
        with self.refused("after it was closed"):
            verify(self.bundle)

    def test_view_on_another_connection_refused(self):
        def change(scenario):
            step = next(step for step in scenario["steps"] if step.get("conn") == "b" and step.get("host", {}).get("type") == "view")
            del step["conn"]

        self.edit("scenarios/two-clients.json", change)
        with self.refused("another connection's attachment"):
            verify(self.bundle)

    def test_missing_fan_out_fails_expectation(self):
        def change(scenario):
            index = next(
                i for i, step in enumerate(scenario["steps"])
                if step.get("conn") == "b" and step.get("host", {}).get("request_id") == "req-a-1"
            )
            del scenario["steps"][index]

        self.edit("scenarios/two-clients.json", change)
        with self.refused("expectation receipts_by_conn"):
            verify(self.bundle)

    def test_non_integer_number_refused(self):
        self.edit("scenarios/abort.json", lambda scenario: scenario["final_value"].update(ratio=0.5))
        with self.refused("non-integer"):
            verify(self.bundle)


class ApplyOpsTests(unittest.TestCase):
    def refusal(self, value, ops):
        with self.assertRaises(OpRefusal) as caught:
            apply_ops(value, ops)
        return caught.exception.code

    def test_trim_counts_utf16_code_units(self):
        self.assertEqual(apply_ops({"t": "\U0001f600ab"}, [["t", ["t"], 2]]), {"t": "ab"})

    def test_reserved_segments_refused_before_any_change(self):
        value = {"a": {}}
        self.assertEqual(self.refusal(value, [["s", ["a", "__proto__"], 1]]), "unsafe_path")
        self.assertEqual(self.refusal(value, [["s", ["prototype"], 1]]), "unsafe_path")
        self.assertEqual(value, {"a": {}})

    def test_booleans_are_not_indices(self):
        self.assertEqual(self.refusal({"xs": [1]}, [["s", ["xs", True], 2]]), "unsafe_path")
        self.assertEqual(self.refusal({"xs": [1]}, [["p", ["xs"], False, 0, []]]), "invalid_op")

    def test_permutation_and_splice(self):
        self.assertEqual(apply_ops({"xs": ["a", "b", "c"]}, [["m", ["xs"], [2, 0, 1]]]), {"xs": ["c", "a", "b"]})
        self.assertEqual(apply_ops([1, 2, 3], [["p", [], 1, 9, ["z"]]]), [1, "z"])

    def test_unknown_verb_is_never_skipped(self):
        self.assertEqual(self.refusal({}, [["s", ["a"], 1], ["#", 1, ["a"]]]), "invalid_op")


if __name__ == "__main__":
    unittest.main()
