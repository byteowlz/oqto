-- Destination hands journal (ADR-0086). Separate from the personal pairing
-- store: pairing alone never authorizes hands.
CREATE TABLE hands_meta (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    next_generation INTEGER NOT NULL
);
INSERT INTO hands_meta (singleton, next_generation) VALUES (1, 1);

-- Owner-installed standing delegation of one binding to one paired device.
CREATE TABLE hands_delegations (
    device_id TEXT NOT NULL,
    binding_id TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (device_id, binding_id)
);

-- Persisted controller takeover revision per (device, durable namespace).
CREATE TABLE hands_controllers (
    device_id TEXT NOT NULL,
    namespace TEXT NOT NULL,
    epoch INTEGER NOT NULL,
    PRIMARY KEY (device_id, namespace)
);

-- Highest origin policy revision fenced per device and binding.
CREATE TABLE hands_fences (
    device_id TEXT NOT NULL,
    binding_id TEXT NOT NULL,
    policy_revision INTEGER NOT NULL,
    PRIMARY KEY (device_id, binding_id)
);

-- Immutable dispatch claims, persisted before any effect. Never pruned while
-- their namespace can admit work; a duplicate inspects, never dispatches.
CREATE TABLE hands_claims (
    call_id TEXT PRIMARY KEY NOT NULL,
    device_id TEXT NOT NULL,
    namespace TEXT NOT NULL,
    controller_epoch INTEGER NOT NULL,
    binding_id TEXT NOT NULL,
    policy_revision INTEGER NOT NULL,
    digest TEXT NOT NULL,
    operation TEXT NOT NULL,
    cwd TEXT NOT NULL,
    generation INTEGER NOT NULL UNIQUE,
    host_boot TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('claimed','running','released','cancelled','expired','fenced','unknown')),
    stop TEXT NOT NULL CHECK (stop IN ('not_started','pending','confirmed','unconfirmed')),
    bytes_in INTEGER NOT NULL DEFAULT 0,
    bytes_out INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

-- A binding whose stop could not be confirmed stays frozen until the owner
-- inspects it and clears the attention explicitly.
CREATE TABLE hands_attention (
    binding_id TEXT PRIMARY KEY NOT NULL,
    reason TEXT NOT NULL,
    at INTEGER NOT NULL
);
