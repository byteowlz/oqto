# Runner CLI

This script sends raw JSON requests to the runner socket for debugging.

## Usage

```bash
node scripts/runner-cli.mjs ping
node scripts/runner-cli.mjs pi-list
node scripts/runner-cli.mjs pi-get-state <session_id> --pretty
node scripts/runner-cli.mjs pi-create <session_id> --cwd /home/wismut/byteowlz/octo
node scripts/runner-cli.mjs pi-prompt <session_id> "hello"
node scripts/runner-cli.mjs --raw '{"type":"pi_get_commands","session_id":"..."}' --pretty
```

## Socket selection

The script checks these in order:

1. `OCTO_RUNNER_SOCKET`
2. `/run/octo/runner-sockets/$USER/octo-runner.sock`
3. `/run/user/$UID/octo-runner.sock`
4. `/tmp/octo-runner.sock`
```
