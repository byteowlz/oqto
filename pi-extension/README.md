# Octo Pi Extensions

This package provides Pi runtime extensions used by Octo Main Chat sessions.

## Tools

- `todowrite`: Write or update the session todo list (used by the frontend todo view)
- `todoread`: Read the current session todo list
- `octo_session`: Delegate work to OpenCode sessions via the Octo backend

Todo state is stored in tool result details so it can be reconstructed across session switches or branches.
