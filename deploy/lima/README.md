# Oqto on Lima (native Linux mode, no Docker-in-Lima)

Use Lima as a real Linux VM and run Oqto with `setup.sh`.

## Prerequisites

- macOS
- Lima installed (`brew install lima`)
- `jq` installed (`brew install jq`)

## 1) Start the VM

```bash
limactl start --name oqto deploy/lima/oqto.yaml
```

## 2) Run setup with Lima profile

```bash
limactl shell oqto -- bash -lc '
  cd "$HOME/byteowlz/oqto_refactor" && \
  ./setup.sh --config deploy/lima/oqto.setup.toml
'
```

The profile sets:

- multi-user mode (`deployment.user_mode = "multi"`)
- local backend mode (`deployment.backend_mode = "local"`)
- Caddy enabled on `localhost:8086`
- `searxng = true`

## 3) Open Oqto

- http://localhost:8086

## Notes

- Caddy is configured without Let's Encrypt in this profile (`domain = "localhost"`).
- To change the local Caddy port, edit `network.caddy_port` in `deploy/lima/oqto.setup.toml`.
