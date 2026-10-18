-- Transport identity is atomic and stable across restarts; clients never gain
-- trust through an automatic replacement of a missing/corrupt private key.
CREATE TABLE personal_transport_identity (
    singleton INTEGER PRIMARY KEY CHECK(singleton=1),
    leaf_der BLOB NOT NULL,
    key_der BLOB NOT NULL
);
