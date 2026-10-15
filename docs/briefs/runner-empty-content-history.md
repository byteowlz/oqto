# Empty structured history content is not JSON prose

Issue: `oqto-6hg1`; mobile investigation `omob-k0c7.10.7.8`.

The runner's `decode_agent_message_parts` handled an empty structured content array by falling through to JSON serialization, manufacturing a text part containing `[]`. Null/missing content similarly became `null`. Canonical live translation already produces no parts for this input. This is a source-level history/live mismatch matching reported empty assistant rows; no private/live messages were fetched.

Non-error null content now yields no parts, and an empty decoded array returns its empty part vector instead of entering the text fallback. Deliberately authored strings `[]`, fenced JSON and `null` stay unchanged. Tool-result raw outputs and assistant error fallback remain unchanged, including errors whose semantic content is empty. Unknown nonempty blocks still use the existing lossless fallback. Message/client IDs and visible part IDs remain stable. Durable authority remains oqto-log; this read projection never edits Pi JSONL or reconciles by text/index.

## Validation and isolation

- RED protocol suite: 12 passed, one failed on manufactured empty JSON prose.
- GREEN: all 13 runner protocol tests passed. `cargo check -p oqto-runner` and `cargo clippy -p oqto-runner --lib -- -D warnings` passed without warnings. Single-file rustfmt and diff checks passed.
- Tests explicitly cover null/missing/empty arrays, authored plain/fenced JSON, tool-result empty arrays, assistant errors, unknown nonempty content and unchanged message/client/part identity.
- Tested base: `3da0dcf2a0825620254260e63ded24998fcc9f53` plus this patch, in the existing primary checkout. Its decoder was byte-identical to `origin/main` `f8295d5701936ab427187374d2f1de3dab436647`. To preserve that checkout's unrelated unpublished commits and staged tracker changes, only this patch and brief are isolated on a separate branch based on that origin/main commit via an alternate Git index. Primary branch/index and original source were restored; no worktree created. This is not a claim that the entire origin/main workspace was rebuilt.
- Scoped static quality reports one major test-module verbosity finding, minor decoder complexity/length growth and three test-discovery dead-code reports. Test growth is retained to cover the requested semantic boundaries, not suppressed or called clean. Contract check completed. Local logs: `target/empty-history-proof/{red,green,check,clippy}.log`.

No backend deployment/restart, live/private Session access, agent resumption, merge to main or release. The service owner must integrate/build/deploy this runner correction before the existing Server's history projection changes. A phone-only heuristic must not hide legitimately authored JSON to conceal the upstream bug.
