# Workbench source boundary

The replacement shell lives here and follows ADR-0031, ADR-0032, and `docs/frontend/workbench-guardrails.md`.

```text
routes/    -> surfaces/
surfaces/  -> modules/
modules/   -> adapters/
adapters/  -> generated protocol/client types
```

Imports may stay within the same named area (for example `modules/timeline/*`) but may not cross sideways into another area. Reusable visual primitives come from `@byteowlz/design-system` rather than this tree.

Run `bun run lint:workbench-guardrails` before adding functional code. The Workbench starts with a zero baseline; suppressions require Tommy's documented approval.
