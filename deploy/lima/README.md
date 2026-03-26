# Oqto on Lima (macOS)

This gives you a Linux VM with Docker, then runs `deploy/docker` inside that VM.

## Prerequisites

- macOS
- [Lima](https://lima-vm.io/) installed (`brew install lima`)
- `jq` installed (`brew install jq`)

## Quick start

From repo root:

```bash
./deploy/lima/bootstrap.sh up
```

Open:

- http://localhost:8086

Default local login (set by bootstrap if not present in `.env`):

- username: `admin`
- password: `admin123456`

## Commands

```bash
./deploy/lima/bootstrap.sh up       # start VM + start oqto docker compose
./deploy/lima/bootstrap.sh status   # VM + compose status
./deploy/lima/bootstrap.sh logs     # follow oqto logs
./deploy/lima/bootstrap.sh ssh      # shell into VM
./deploy/lima/bootstrap.sh down     # stop compose + stop VM
```

## Notes

- The VM template is `deploy/lima/oqto.yaml`.
- Host home is mounted into the VM, so your repo path is available unchanged.
- This setup uses the Docker single-user runtime (`OQTO_SINGLE_USER=true`) for reliability.
