-- Append-only harness/runtime bindings for the one public Oqto Session identity.
--
-- `oqto_session_id` is the existing unique public platform identity. Legacy
-- rows without a canonical `oqto-` identity stay on the compatibility column
-- until a separate reviewed identity migration repairs them.
CREATE TABLE oqto_log_session_bindings (
    binding_id TEXT PRIMARY KEY,
    oqto_session_id TEXT NOT NULL,
    harness TEXT NOT NULL,
    provider_instance TEXT NOT NULL DEFAULT '',
    binding_kind TEXT NOT NULL,
    external_id TEXT NOT NULL,
    source TEXT NOT NULL,
    first_seen_at TEXT NOT NULL DEFAULT (datetime('now')),
    last_seen_at TEXT,
    supersedes_binding_id TEXT,
    extensions_json TEXT NOT NULL DEFAULT '{}',
    -- The identity fact survives Session retention/deletion. Resolution checks
    -- that the currently bound Session still exists and fails explicitly.
    FOREIGN KEY (supersedes_binding_id) REFERENCES oqto_log_session_bindings(binding_id) ON DELETE RESTRICT,
    CHECK (trim(harness) != ''),
    CHECK (trim(binding_kind) != ''),
    CHECK (trim(external_id) != ''),
    CHECK (trim(source) != ''),
    CHECK (external_id NOT LIKE 'pending-%'),
    CHECK (external_id NOT LIKE 'tmp:%'),
    CHECK (oqto_session_id NOT LIKE 'pending-%'),
    CHECK (oqto_session_id NOT LIKE 'tmp:%'),
    CHECK (json_valid(extensions_json)),
    CHECK (supersedes_binding_id IS NULL OR supersedes_binding_id != binding_id)
);

-- One root fact per native identity. An explicit successor may supersede that
-- root, but cannot create a second root silently.
CREATE UNIQUE INDEX idx_oqto_log_session_bindings_native_root
    ON oqto_log_session_bindings(harness, provider_instance, binding_kind, external_id)
    WHERE supersedes_binding_id IS NULL;

-- Keep supersession linear: a fact can have at most one direct successor.
CREATE UNIQUE INDEX idx_oqto_log_session_bindings_superseded_once
    ON oqto_log_session_bindings(supersedes_binding_id)
    WHERE supersedes_binding_id IS NOT NULL;

CREATE INDEX idx_oqto_log_session_bindings_session
    ON oqto_log_session_bindings(oqto_session_id, first_seen_at);

CREATE INDEX idx_oqto_log_session_bindings_native
    ON oqto_log_session_bindings(harness, provider_instance, binding_kind, external_id, first_seen_at);

-- Backfill only mappings that are unambiguous in the legacy compatibility
-- column. Duplicate external ids are intentionally left to legacy fallback,
-- which the store resolves fail-closed; choosing a winner here could route a
-- public Session to an empty duplicate and hide the only timeline copy.
INSERT INTO oqto_log_session_bindings (
    binding_id,
    oqto_session_id,
    harness,
    provider_instance,
    binding_kind,
    external_id,
    source,
    first_seen_at,
    extensions_json
)
SELECT
    'binding:legacy:' || lower(hex(trim(s.external_id))),
    s.platform_id,
    'unknown',
    '',
    'pi-compatible-session',
    trim(s.external_id),
    'legacy-external-id',
    s.created_at,
    json_object('legacy_session_id', s.session_id)
FROM oqto_log_sessions s
WHERE s.external_id IS NOT NULL
  AND trim(s.external_id) != ''
  AND s.external_id NOT LIKE 'pending-%'
  AND s.external_id NOT LIKE 'tmp:%'
  AND s.platform_id LIKE 'oqto-%'
  AND s.platform_id NOT LIKE 'pending-%'
  AND s.platform_id NOT LIKE 'tmp:%'
  AND (
      SELECT COUNT(*)
      FROM oqto_log_sessions candidate
      WHERE trim(candidate.external_id) = trim(s.external_id)
  ) = 1;
