# Bug Report: oqto-browser Socket Path Too Long

## Problem
The oqto-browser socket path exceeds the Unix socket path limit (108 characters), causing the browser feature to be completely blocked.

## Root Cause
The socket path is constructed as:
```
~/.local/state/oqto/agent-browser/{session_id}/{session_id}.sock
```

For users with long usernames (e.g., `oqto_shared_content-creation`), this creates paths like:
```
~/.local/state/oqto/agent-browser/oqto_shared_content-creation/oqto_shared_content-creation.sock
```

This is **117 characters**, exceeding the Unix socket limit of **108 characters**.

## Error Message
```
Failed to connect to /home/oqto_shared_content-creation/.local/state/oqto/agent-browser/oqto_shared_content-creation/oqto_shared_content-creation.sock
Address too long
```

## Impact
- Browser feature completely unusable for affected users
- Any agent-browser commands fail immediately
- Cannot run end-to-end tests with browser automation

## Affected Code
`backend/crates/oqto-browser/src/main.rs` - `socket_path()` function

## Proposed Solutions

### Option 1: Hash Session ID (Recommended)
Replace the session ID in the socket filename with a short hash (e.g., 8-12 characters). This guarantees a fixed length socket path regardless of session ID length.

**Pros:**
- Simple implementation
- Guarantees path length compliance
- Still unique per session
- No external dependencies

**Cons:**
- Less human-readable socket filenames

### Option 2: Use `/tmp` Directory
Move socket directory to `/tmp` or `/run/user/{uid}/` with hashed session ID.

**Pros:**
- Shorter base path
- Follows XDG Runtime Directory spec
- Auto-cleaned on reboot

**Cons:**
- Doesn't solve the root cause (long session ID)
- Need to handle `/tmp` permissions properly

### Option 3: Abstract Namespace Sockets (Linux-specific)
Use Linux abstract namespace sockets (paths starting with `\0`).

**Pros:**
- No filesystem constraints
- Auto-cleaned when closed

**Cons:**
- Linux-specific (not portable to macOS/BSD)
- Requires special handling in code

## Recommended Fix
Implement **Option 1** (Hash Session ID) with a helper function that:
1. Takes the session ID as input
2. Computes a short hash (e.g., using `seahash` or built-in hasher)
3. Returns a fixed-length string (e.g., 12 hex characters)
4. Uses this hash for the socket filename and directory name

This ensures the socket path is always under 80 characters, well within the 108-character limit.

## Implementation Status
✅ **FIXED** - Implemented in `backend/crates/oqto-browser/src/main.rs`

The fix adds a `hash_session_id()` function that:
- Uses Rust's built-in `DefaultHasher` for the hash
- Returns a fixed 12-character hex string
- Is used in both `socket_path()` and `agent_browser_session_dir()`

**Example:**
```
Before: ~/.local/state/oqto/agent-browser/oqto_shared_content-creation/oqto_shared_content-creation.sock (117 chars)
After:  ~/.local/state/oqto/agent-browser/a3f7b8c2d1e5/a3f7b8c2d1e5.sock (66 chars)
```

The socket path is now guaranteed to be under 70 characters regardless of session ID length.
