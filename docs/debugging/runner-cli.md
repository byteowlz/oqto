# Runner CLI

This script sends raw JSON requests to the runner socket for debugging.

## Usage

```bash
node scripts/runner-cli.mjs ping
node scripts/runner-cli.mjs pi-list
node scripts/runner-cli.mjs pi-get-state <session_id> --pretty
node scripts/runner-cli.mjs pi-create <session_id> --cwd /home/wismut/byteowlz/oqto
node scripts/runner-cli.mjs pi-prompt <session_id> "hello"
node scripts/runner-cli.mjs --raw '{"type":"pi_get_commands","session_id":"..."}' --pretty
```

## Socket selection

The script checks these in order:

1. `OQTO_RUNNER_SOCKET`
2. `/run/oqto/runner-sockets/$USER/oqto-runner.sock`
3. `/run/user/$UID/oqto-runner.sock`
4. `/tmp/oqto-runner.sock`
```
