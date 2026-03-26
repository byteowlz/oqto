# History Module

**All writes to hstry go through gRPC. No direct SQLite writes. Ever.**

The hstry daemon owns the database. oqto reads via direct SQLite (read-only pool) for performance, but every mutation (create, update, delete) must go through the `HstryClient` gRPC interface.

Direct SQLite writes bypass WAL coordination, break the daemon's in-memory caches, and silently fail on the read-only pool anyway.
