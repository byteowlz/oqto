# Pi is the only first-class harness; all others attach via bridges

The canonical protocol remains the only contract the frontend and backend know. Inside the runner, Pi keeps its native full-fidelity translator permanently — it is the only first-class harness, and its native RPC is a deliberate fidelity advantage (fork, compact, mid-session model switching, session identity, thinking granularity). Every other harness attaches through a bridge translator: a generic ACP bridge (`oqto-acp`, per oqto-3ct7.13/14) for the ACP ecosystem (Claude Code, gemini-cli, Zed-compatible agents), or a dedicated bridge where a harness exposes its own richer interface (e.g. Codex app server). We rejected rebasing the harness boundary on ACP universally: ACP is narrower than the canonical protocol and forcing Pi through it would degrade the primary harness to a standard we do not control.

## Capability advertisement (required companion contract)

Bridged harnesses will not support every canonical command. At session start the runner reports the session's supported capability set (fork, compact, set_model, thinking, ...); the frontend derives UI affordances from that report instead of assuming Pi's feature set. Without this, every non-Pi harness surfaces as silently broken buttons; with it, "harness-agnostic frontend" is enforceable. This mirrors fleet-level capability registration in the control plane (ADR-0003/0004) at session granularity.

## Consequences

- Two translator maintenance paths (Pi native + bridges) are accepted as the honest price of a first-class harness.
- Bridge fidelity is explicit, queryable truth, not a bug class.
- Pi gaining native ACP support is a non-goal; deleting `PiTranslator` is a non-goal.
