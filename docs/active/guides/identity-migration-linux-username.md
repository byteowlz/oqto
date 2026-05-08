# Identity migration runbook: linux_username/linux_uid contract

This runbook covers no-downtime rollout of the canonical identity contract in multi-user mode.

## Canonical contract

- `user_id`: internal platform identifier (not OS execution identity)
- `username`: login/display identity
- `linux_username` + `linux_uid`: authoritative execution identity in multi-user mode

## Pre-flight

1. Backup DB and service config.
2. Confirm current mode and runner socket pattern.
3. Run doctor in dry-run:

```bash
oqtoctl user doctor-identity
# or JSON
oqtoctl --json user doctor-identity
```

## Migration sequence (no downtime)

1. Keep strict mode disabled (`local.linux_users.strict_identity = false`).
2. Run remediation:

```bash
oqtoctl user doctor-identity --apply
```

3. Re-run dry-run and ensure no remaining blocking errors.
4. Monitor logs for fallback warnings (`legacy identity fallback`).
5. Enable strict mode for canary cohort/host:

```toml
[local.linux_users]
strict_identity = true
```

6. Validate login/chat/runner operations, then roll out strict mode fleet-wide.

## Validation checks

- `oqtoctl user doctor-identity` returns no errors.
- Active users have `linux_username` and `linux_uid` populated.
- Runner socket location is consistent (avoid dual legacy + user runtime sockets).
- Auth/login succeeds without fallback warnings.

## Rollback

If strict mode causes unexpected failures:

1. Set `strict_identity = false`.
2. Restart backend.
3. Re-run `oqtoctl user doctor-identity --apply`.
4. Fix remaining identity mismatches before re-enabling strict mode.

## Notes

- Strict mode intentionally blocks legacy fallback in auth/runtime paths.
- `doctor-identity --apply` is idempotent for DB field remediations.
