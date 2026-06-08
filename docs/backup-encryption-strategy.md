# Oqto Backup Encryption Strategy

> Decisions for how and what to encrypt: backups, data at rest, and optionally
> data in transit.  Written to complement `docs/backup-file-tree.md`.

---

## TL;DR recommendation

**Option B (encrypt everything in backups) as the mandatory baseline, with
Option C (SQLCipher at-rest) as an opt-in hardening flag for multi-user or
compliance-sensitive deployments.**

Reason: Oqto is primarily self-hosted.  Full at-rest DB encryption (SQLCipher)
is a compile-time and runtime burden that hurts single-user ergonomics.
Backup-level encryption gives us a uniform security boundary with near-zero
ongoing operational cost.

---

## Threat model

| Threat | Likelihood | Impact |
|--------|-----------|--------|
| Backup archive stolen from object store / offsite copy | Medium | **High** — exposes all chat history, prompts, workspace code, API keys |
| Host disk stolen / decommissioned without wipe | Low | **Medium-High** — full user data exposure |
| Attacker with unprivileged shell access on running host | Medium | Medium — they can already read live DBs via normal APIs |
| Attacker with root on running host | High (if compromised) | **High** — game over regardless of encryption |
| Backup restored to wrong user / leaked restore key | Low | **High** — cross-user data contamination |

Key insight: **encrypting backups defends the highest-impact, most likely
threat** (backup theft/leak).  At-rest encryption defends a lower-likelihood
threat (physical disk theft) but adds daily operational friction.

---

## Option A: Encrypt secrets only

Encrypt files explicitly containing credentials:
- `~/.pi/agent/auth.json`
- `~/.config/eavs/env`
- `~/.config/oqto/setup-state.env`
- `~/.local/share/eavs/keys.db`
- `~/.local/share/eavs/providers.db`

Leave cleartext:
- `oqto-log.sqlite` (chat history, prompts)
- `hstry.db` (legacy chat history)
- Pi session JSONL files
- Workspace code and `.oqto/` metadata

### Verdict: **Reject**

Chat logs and prompts contain sensitive business logic, code, and personal
information.  Treating them as "non-secret" is a data-classification error.

---

## Option B: Encrypt everything in backups (RECOMMENDED baseline)

Every backup archive — whether per-user full, per-workspace incremental, or
platform snapshot — is encrypted as a single unit before leaving the host.

### Key model

```
Per user: one master backup key (symmetric, 256-bit AES-GCM or ChaCha20-Poly1305)
          stored in the user's keyring / systemd-creds / kyz vault.

Per backup: random data-encryption key (DEK)
            DEK encrypted by user's master key -> envelope
            Archive encrypted by DEK
```

### Why envelope encryption?

- Rotate the user master key without re-encrypting all historical backups
- DEK is unique per backup → compromise of one backup does not expose others
- Platform backups can use a separate platform master key

### Granularity

| Backup type | Encryption unit | Key |
|-------------|----------------|-----|
| Per-user full | One archive per user | User master key |
| Per-workspace incremental | One archive per workspace hash | User master key |
| Per-session export | One archive per JSONL + log slice | User master key |
| Platform snapshot | One archive per host | Platform master key |

### Implementation sketch

```bash
# backup
age -r <user-public-key> -o backup.tar.age <(tar czf - ~/.local/share/oqto ~/.pi/agent/ ...)
# or with symmetric key from systemd-creds
systemd-creds cat oqto-backup-key | age --passphrase -o backup.tar.age backup.tar

# restore
decrypt -> verify manifest checksums -> sqlite3 .backup restore -> verify
```

### Advantages
- One decision: "backup leaves host → encrypt"
- No changes to SQLite runtime (no SQLCipher compile)
- Works with any storage backend (S3, rsync, tape)
- Fast restore (single decrypt + unpack)

### Disadvantages
- Live data on disk remains cleartext (acceptable for self-hosted)
- Granular restore still touches the whole archive

---

## Option C: Encrypt data at rest (SQLCipher + file-level)

All SQLite databases use SQLCipher (AES-256-CBC or AES-256-GCM page encryption).
Config files and JSONL are individually encrypted (e.g. via `age` or
`libsodium secretbox`).

### What changes

| Component | Change |
|-----------|--------|
| `oqto-log`, `hstry`, `eavs/keys.db`, `eavs/providers.db`, `oqto.db` | Switch `rusqlite` to `rusqlite` with `sqlcipher` feature; add `PRAGMA key = ...` on open |
| Pi session JSONL | Encrypt after write, decrypt on read; or store in encrypted bind-mount/volume |
| Config files (`config.toml`, `sandbox.toml`) | Could stay cleartext (no secrets) or be encrypted if desired |
| Backups | Already-encrypted files go into encrypted archives → double encryption, but safe |

### Key model

```
Per database / per file: derived key from user passphrase or TPM/Secure Enclave
Per user: optionally a single master passphrase derived into per-DB keys via HKDF
```

### Advantages
- Stolen disk → unreadable without passphrase
- Backup archives are encrypted even if the backup tool misconfigures
- Compliance (GDPR, SOC-2) friendly

### Disadvantages
- **Compile-time burden**: every SQLite-using crate needs `sqlcipher` or
  `libsqlite3-sys/bundled-sqlcipher` feature flag
- **Performance**: 5-15% SQLite throughput hit, mostly on writes
- **Key management UX**: self-hosted users must enter a passphrase on every
  service start (or we build TPM/systemd-creds integration)
- **Pi JSONL**: Pi writes these directly; we'd need a FUSE/bind-mount layer or
  patch Pi to write via an encrypted volume

### Verdict: **Opt-in feature flag, not default**

Enable with `oqto config set security.encrypt_at_rest true` (or similar) on
first setup.  Single-user self-hosted default stays **off** to preserve
simplicity.

---

## Option D: End-to-end fortress

Combine C (at-rest) + B (backup encryption) + mTLS + short-lived keys + HSM.

### Verdict: **Out of scope for now**

Only justified for multi-tenant SaaS or regulated enterprise deployments.
Can be added later without breaking B/C architecture.

---

## Proposed architecture: B + optional C

```
┌──────────────────────────────────────────────────────────────┐
│                     Oqto Host                                │
│                                                              │
│   ┌──────────────┐    ┌──────────────┐    ┌─────────────┐   │
│   │  oqto-log    │    │   hstry      │    │  eavs dbs   │   │
│   │  (cleartext) │    │  (cleartext) │    │ (cleartext) │   │  <-- live
│   └──────────────┘    └──────────────┘    └─────────────┘   │
│                                                              │
│   ┌──────────────────────────────────────────────────────┐   │
│   │  Backup daemon (oqto-backup)                         │   │
│   │  1. sqlite3 .backup  → /tmp/snapshots/<db>.bak       │   │
│   │  2. tar + age        → /tmp/snapshots/<user>.tar.age │   │
│   │  3. upload to object store                           │   │
│   └──────────────────────────────────────────────────────┘   │
│               │                                              │
│               ▼                                              │
│   ┌──────────────────────────────────────────────────────┐   │
│   │  Encrypted backup archive (.tar.age)                 │   │
│   │  Envelope: DEK (random) encrypted by user master key │   │
│   │  Inner:    snapshot manifest + checksums + files     │   │
│   └──────────────────────────────────────────────────────┘   │
└──────────────────────────────────────────────────────────────┘
                      │
                      ▼
              ┌───────────────┐
              │  Object store │  (untrusted -- S3, B2, MinIO, rsync target)
              │  or offsite   │
              └───────────────┘
```

With **optional C enabled** (compile flag):

```
┌──────────────────────────────────────────────────────────────┐
│   ┌──────────────┐    ┌──────────────┐    ┌─────────────┐   │
│   │  oqto-log    │    │   hstry      │    │  eavs dbs   │   │
│   │  (SQLCipher) │    │  (SQLCipher) │    │ (SQLCipher) │   │  <-- live
│   └──────────────┘    └──────────────┘    └─────────────┘   │
│                                                              │
│   Pi JSONL → encrypted bind-mount / FUSE / libsodium       │
└──────────────────────────────────────────────────────────────┘
```

---

## Key management

### Single-user mode

- Master key stored in `systemd-creds` (if available) or `~/.config/oqto/backup-master.key`
  with 0600 permissions.
- Key is generated once during `oqto setup` and displayed to the user with a
  strong recommendation to write it down / store in a password manager.
- If the key is lost, backups are unreadable.  This is acceptable for
  self-hosted users (same threat model as losing an SSH key).

### Multi-user mode

- Per-user master keys generated by `oqtoctl` and stored in a central KMS
  (e.g. HashiCorp Vault, kyz, or a simple age-keyring managed by `oqtoctl`).
- Platform master key stored in a different KMS path with stricter ACL.

### Key rotation

```
1. Generate new master key
2. Re-encrypt DEKs of recent backups (e.g. last 30 days)
3. Mark old master key as deprecated
4. After retention period, delete old master key
```

---

## Manifest format (inside encrypted archive)

Every backup archive contains a top-level `manifest.json`:

```json
{
  "version": "1",
  "created_at": "2026-05-10T09:30:00Z",
  "host": "arch-dev-01",
  "user": "wismut",
  "type": "user-full",
  "encryption": {
    "algorithm": "age-v1",
    "recipient": "age1...",
    "envelope": "base64-encoded-encrypted-dek"
  },
  "files": [
    {
      "path": "snapshots/oqto.db.bak",
      "size": 1048576,
      "sha256": "abc123...",
      "source": "~/.local/share/oqto/oqto.db",
      "snapshot_method": "sqlite3_backup"
    },
    {
      "path": "data/oqto-log/5951338a141d9880d5218b9a/oqto-log.sqlite.bak",
      "size": 2097152,
      "sha256": "def456...",
      "source": "~/.local/share/oqto/oqto-log/5951338a141d9880d5218b9a/oqto-log.sqlite",
      "snapshot_method": "sqlite3_backup"
    }
  ],
  "checksums": {
    "manifest.json": "sha256:...",
    "archive.tar": "sha256:..."
  }
}
```

---

## Next steps

1. **Update `docs/backup-file-tree.md`** — change all `[U]` and `[P]` markers to
   reflect that all backed-up data is encrypted by default.
2. **Create `oqto-backup` crate** — Rust CLI that:
   - Reads the backup file tree
   - Runs `sqlite3 .backup` for each DB
   - Builds archive + manifest
   - Encrypts with `age`
   - Uploads to configured object store
3. **Add `security.encrypt_at_rest` config flag** — when true, switch SQLite
   connections to SQLCipher mode (requires feature-gated compile).
4. **Write restore CLI** — decrypt, verify checksums, apply `.restore` to DBs.

---

*Tracked in trx: oqto-n181*
