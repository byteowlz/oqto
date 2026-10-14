-- Account-owned metadata only. Credential bytes remain in host providers.
CREATE TABLE credential_connections (
 id TEXT PRIMARY KEY,
 account_id TEXT NOT NULL,
 revision INTEGER NOT NULL CHECK(revision BETWEEN 1 AND 4294967295),
 label TEXT NOT NULL,
 policy_json TEXT NOT NULL,
 provider_id TEXT NOT NULL,
 credential_id TEXT NOT NULL,
 credential_revision TEXT NOT NULL,
 revoked INTEGER NOT NULL DEFAULT 0 CHECK(revoked IN (0,1))
);
CREATE INDEX credential_connections_owner ON credential_connections(account_id);
CREATE TABLE app_connection_bindings (
 account_id TEXT NOT NULL,
 instance_id TEXT NOT NULL REFERENCES app_instances(id) ON DELETE CASCADE,
 slot TEXT NOT NULL,
 connection_id TEXT NOT NULL REFERENCES credential_connections(id) ON DELETE CASCADE,
 connection_revision INTEGER NOT NULL CHECK(connection_revision BETWEEN 1 AND 4294967295),
 content_digest TEXT NOT NULL,
 PRIMARY KEY(account_id, instance_id, slot)
);
