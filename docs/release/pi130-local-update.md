# Pi 1.3.0 Linux runtime qualification (2026-10-17)

Status: **activated locally** through the guarded canonical transaction on 2026-10-17. Shared Pi and the Oqto-bundled runtime report 1.3.0. Mac Studio was checked read-only and reports 1.3.0; no Mac/Azure rollout occurred.

The exact official npm closure is pinned with SHA512 integrity and package-lock SHA256, alongside unchanged checksum-pinned Node 24.14.1. All seven `@earendil-works/pi-*` packages in the staged closure report 1.3.0. Default deployment consumes the synchronized dependency, package/lock and release-manifest pins; never edit a live immutable runtime or run npm inside it.

Passing isolated evidence:

- Native RPC/model discovery/session and settlement qualification.
- Real runner sandbox: chat, effective reasoning, durable native JSONL and cold restore.
- All 14 manifest-declared extensions individually and combined.
- Native Selection RPC controls and synthetic-provider tool callbacks.
- Synthetic Pi 1.1 native-session restoration with unchanged identity and existing records; original fixture unchanged.
- Nine updater regressions, three runtime-staging regressions, strict distribution manifest and canonical full-release contract checks.

Receipts and the prepared artifact are under `~/byteowlz/oqto-release-artifacts/pi130-local-20261017/`. The private deployment source `cf4deda7ca7773e5b7883462f0eb8e295722949f` is based on the currently installed `1202bcfc87f689b081c1cff5418e8dbf24b03fc3`, changing only the runtime tuple/manifest. Its backend/frontend source trees are identical; their already-qualified installed assets are reused. Unfinished pairing/control work is excluded.

A probe of every directory in the old immutable payload failed because it also contains undeclared stale `pi-oqto-todos`, conflicting with declared `pi-todolist`. Failure is retained in `shipped-extensions.log`; the new candidate contains exactly the manifest's 14 directories. No user-owned extension was removed or changed.

The first privilege prompt timed out without changes. The authorized retry activated `0.5.11-dev.20261017192436.gcf4deda7`; protected receipt `/root/oqto-deploy-20261017T210313Z-3dadea01` records configuration preservation, health `ok` (Oqto 0.5.11), and runner PID 2890298 executing from the new release. `/usr/local/bin/pi` resolves to that release and reports 1.3.0; `oqtoctl status` is healthy. Activation used no busy-session override and disabled pruning. This establishes local activation, not original authenticated UI/provider acceptance, fleet acceptance or publication.
