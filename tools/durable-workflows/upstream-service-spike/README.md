# Upstream Pi service spike

Issue: `oqto-pdbs`; integration feature: `oqto-g7rk`.

Pinned source: `earendil-works/pi-mono` commit
`cdaf6c227d4718a72b59ccd02152c9614df44cfc` (packages 1.1.0, experimental protocol 8).

This is a disposable qualification test, not an Oqto runtime adapter or activation.
It uses upstream's real Unix server/router, separate Session worker, native SQLite,
Chord service bindings and two independent clients. Only the model is synthetic,
via upstream's faux-worker fixture. It never uses real credentials or coding tools.

## Reproduce on the Linux qualification host

Prepare a **disposable** checkout at `/tmp/oqto-pi-service-spike.XXXXXX/source`,
checkout the exact commit, and give it an isolated `HOME`. Install from the lockfile
with `npm ci --ignore-scripts --no-audit --no-fund`, run `npm run hydrate:model-data`
(public catalog download), then `npm run build:offline`. Do not install globally,
copy user profiles, or change Oqto/Pi release pins.

```sh
bash tools/durable-workflows/upstream-service-spike/run.sh \
  /tmp/oqto-pi-service-spike.XXXXXX/source
```

The launcher refuses other source paths/pins and missing built dependencies. It
runs one explicit test file with one Vitest worker, file parallelism disabled,
CPU affinity restricted to one available CPU, nice 19, a 512 MiB V8 heap setting,
and a 90-second timeout. Child workers inherit CPU affinity and priority; the test
asserts the worker's priority. This is not an aggregate process-tree memory quota.

**Do not invoke Vitest at the monorepo root with only a package `--config`.** That
configuration does not constrain discovery to the package; the initial broad
attempt selected unrelated suites, timed out, and starved the shared host's CPU.
The explicit test filter and resource guard are mandatory mechanisms, not optional
performance tweaks. After timeout, check for owned leftover workers before retrying.

## Evidence and limits

The independent regression passes: both clients observe the same committed view,
share one worker, have distinct attachment IDs, and rehydrate after a client
leaves and another turn completes. It kills the idle worker with `SIGKILL`, opens
its SQLite state in a replacement worker, verifies complete prior entries, and
continues with a third answer. Prior-entry comparisons use native entry IDs and
full records, not text/order reconciliation. Post-attach transcript hydration is
asynchronous: wait for the actual replicated value, not just binding readiness.

The pinned build and 33 explicitly selected upstream coding-service/TUI tests
also passed. These are distinct evidence surfaces. The broad timed-out suite is
NOT a package-wide pass. Formatting/linting of this regression passed upstream
Biome; launcher syntax and negative-input refusals are checked separately.

Not qualified here: Oqto runner authorization, Oqto Web, physical TUI interaction,
real provider credentials, in-flight kill/uncertain tool effects, exactly-once
prompt retries, mobile lifecycle, or live deployment. Upstream test bindings use
`assertAccess() {}` and cannot establish Oqto access control. The controller facade
currently has no caller-supplied prompt idempotency key; safe retry admission is
an integration requirement, not assumed from request correlation IDs.
