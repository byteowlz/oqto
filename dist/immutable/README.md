# dist/immutable/

Assets that are shipped as immutable release content.

Expected subtrees:
- `bin/` binaries included in release payload
- `systemd/` shipped unit templates
- `seccomp/` shipped seccomp policies
- `defaults/` platform-owned default content (pi-agent/workdir templates)

Install/update behavior: staged into `/var/lib/oqto/releases/<id>/...` and consumed via symlink or copy from active release.
