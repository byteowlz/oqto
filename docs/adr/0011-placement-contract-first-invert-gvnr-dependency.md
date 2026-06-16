# Decomposition depends on the placement contract, not on building gvnr

Extracting `oqto-sessions` (3ct7.8) requires separating product session facts (id pair, owner, workspace, model, status) from placement facts (which runner, alive?, reachable where?) — currently fused in `session` + `session_target` + `runner`. After ADR-0003/0004 placement is gvnr's truth. This does NOT mean gvnr must be built before the refactor proceeds. We invert the dependency: `oqto-sessions` depends on a `PlacementStore` trait, not on gvnr.

## Sequence

1. Design the `PlacementStore` contract — operations + `AGENT_CTX` identities. A design artifact, no server.
2. Extract `oqto-sessions` against the trait, backed by a thin `LocalPlacementStore` (single-host routing). oqto runs as today; gvnr need not exist.
3. Build gvnr (core lib + server) on its own timeline behind `GvnrPlacementStore`.
4. Select impl by config: personal embeds gvnr-core, fleet talks to the gvnr server.

Extraction value comes from step 1's contract; fleet value comes from step 3. They are decoupled — gvnr is never a prerequisite for the decomposition.

## Constraints

- `LocalPlacementStore` is a deliberately thin stopgap, not a parallel registry. Making it rich rebuilds a mini-gvnr inside oqto — the drift `gvnr/DESIGN.md` forbids. On personal installs it is eventually replaced by embedded gvnr-core (ADR-0003), not kept as a third codepath.
- `oqto-sessions` consumes only the read/resolve half of the contract. The write half (runner register/heartbeat) is the runner's concern and is not a blocker for the session crate.
- The contract speaks `AGENT_CTX` ids (ADR-0004), so the same identities join oqto session metadata to gvnr registry records.
