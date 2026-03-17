# Backend Refactor Target Tree (Runner-First)

Date: 2026-03-17
Status: Target architecture for incremental refactor

## Goals

- Make runtime behavior consistent: **runner path in all normal modes**
- Reduce cognitive load by splitting process/runtime concerns into separate crates
- Eliminate ambiguous module boundaries (`history` vs `hstry`, `canon` duplication)
- Break up oversized files and enforce clear ownership per crate

## Target Workspace Tree

```text
backend/
├── Cargo.toml
├── crates/
│   ├── oqto-protocol/
│   │   └── src/{lib.rs,commands.rs,events.rs,messages.rs,runner.rs,delegation.rs}
│   │
│   ├── oqto-domain/
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── users/{mod.rs,models.rs,repository.rs,service.rs}
│   │       ├── sessions/{mod.rs,models.rs,repository.rs,service.rs}
│   │       ├── projects/{mod.rs,models.rs,repository.rs,service.rs}
│   │       ├── shared_workspaces/{mod.rs,models.rs,repository.rs,service.rs}
│   │       ├── settings/{mod.rs,schema.rs,service.rs}
│   │       ├── invites/{mod.rs,models.rs,repository.rs,service.rs}
│   │       ├── onboarding/{mod.rs,models.rs,service.rs}
│   │       └── prompts/{mod.rs,models.rs,service.rs}
│   │
│   ├── oqto-history/
│   │   └── src/{lib.rs,client.rs,read.rs,write.rs,convert.rs,canon.rs,models.rs}
│   │
│   ├── oqto-harness-pi/
│   │   └── src/{lib.rs,client.rs,runtime.rs,translator.rs,types.rs,session_files.rs}
│   │
│   ├── oqto-user-plane/
│   │   └── src/{lib.rs,trait.rs,types.rs,runner.rs,direct.rs}
│   │
│   ├── oqto-runner/
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── protocol.rs
│   │       ├── client.rs
│   │       ├── daemon/
│   │       │   ├── mod.rs
│   │       │   ├── server.rs
│   │       │   ├── state.rs
│   │       │   └── handlers/{mod.rs,process.rs,files.rs,sessions.rs,memories.rs,pi.rs}
│   │       └── bin/oqto-runner.rs
│   │
│   ├── oqto-sandbox/
│   │   └── src/{lib.rs,config.rs,policy.rs,linux_bwrap.rs,macos_seatbelt.rs,bin/oqto-sandbox.rs}
│   │
│   ├── oqto-server/
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── bin/oqto.rs
│   │       ├── app/{mod.rs,bootstrap.rs,context.rs,config.rs}
│   │       ├── api/
│   │       │   ├── {mod.rs,routes.rs,state.rs}
│   │       │   ├── middleware/{mod.rs,auth.rs,audit.rs,errors.rs}
│   │       │   └── handlers/{mod.rs,auth.rs,admin.rs,chat.rs,sessions.rs,projects.rs,settings.rs,invites.rs,feedback.rs,shared_workspaces.rs}
│   │       ├── ws/
│   │       │   ├── {mod.rs,hub.rs,connection.rs,events.rs}
│   │       │   └── multiplexed/{mod.rs,agent.rs,files.rs,terminal.rs,history.rs,system.rs}
│   │       ├── proxy/{mod.rs,fileserver.rs,terminal.rs,browser.rs,mmry.rs}
│   │       ├── audit/{mod.rs,writer.rs,types.rs}
│   │       └── observability/{mod.rs,logging.rs,metrics.rs}
│   │
│   ├── oqto-files/
│   ├── oqto-browser/
│   ├── oqto-usermgr/
│   ├── oqto-setup/
│   └── oqto-scaffold/
│
└── docs/architecture/
    ├── backend-target-tree.md
    ├── backend-crate-map.md
    ├── runtime-boundaries.md
    └── dependency-rules.md
```

## Runtime Policy

- Normal runtime path is **runner-mediated**.
- `DirectUserPlane` is allowed only for tests and explicit emergency/debug mode.
- No production logic divergence by mode.

## Dependency Rules

- `oqto-protocol` is foundational and harness-agnostic.
- `oqto-server` depends on domain/history/user-plane/protocol crates.
- `oqto-runner` depends on harness + sandbox + protocol.
- Lower-level crates must not depend on `oqto-server`.

## Migration Strategy (Incremental)

1. Make runner default path everywhere (keep temporary direct fallback flag).
2. Split giant files into submodules (`ws_multiplexed`, runner daemon handlers).
3. Extract `oqto-runner` and `oqto-sandbox` into dedicated crates.
4. Consolidate history integration (`history` + `hstry`) into one crate.
5. Move canonical types into `oqto-protocol` only.
6. Remove deprecated direct runtime path and dead code.

## Notes

- This is a target map, not a one-shot migration.
- Refactor should preserve external protocol behavior during each step.
