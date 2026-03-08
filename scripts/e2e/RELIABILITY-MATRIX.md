# Reliability Matrix

This matrix defines the permanent reliability scope for Oqto.

Status keys:
- `active` - automated and runnable now (currently local-only)
- `wip` - in progress, partial automation
- `planned` - defined and scheduled

## Control Plane and Session Stability

| Area | Scenario | Status |
|---|---|---|
| Health | `/api/health` stays 200 under load | active |
| Auth | login + `/api/me` roundtrip reliability | active |
| Session list | personal `/api/chat-history` latency budget (p95) | active |
| Shared list | shared workspace chat list latency budget (p95) | active |
| Restarts | watchdog-triggered restart detection and reporting | active |
| Restarts | forced restart during active user sessions (chaos) | planned |

## Chat Consistency

| Area | Scenario | Status |
|---|---|---|
| Delivery | prompt/steer requests produce assistant responses | wip |
| Durability | message counts stable across reload/reconnect | wip |
| Ordering | no out-of-order message indices in hstry | planned |
| Dedup | no duplicate messages after reconnect/compaction | planned |
| Shared chat | multi-user same-session consistency | planned |

## Files and Media

| Area | Scenario | Status |
|---|---|---|
| File browse | workdir listing + stat stability | wip |
| File read | text file read in active workspace | wip |
| Media URL | audio/video/image URL reachability via workspace files API | active |
| Media playback | browser-level play/pause/seek loops for audio/video | wip |
| File mutations | create/edit/rename/delete/copy/move + list consistency | planned |

## UI Workflows (Tab Coverage)

| Tab/View | Scenario | Status |
|---|---|---|
| Sessions | route load smoke + authenticated render | active |
| Shared Workspaces | API lifecycle create/update/delete loops | active |
| Shared Workspaces | expand/select/new chat/project/delete | wip |
| Files preview | text/pdf/image/audio/video preview | wip |
| Files mutations | WS mux create/write/read/rename/copy/move/delete loops | active |
| Settings | route load smoke + authenticated render | active |
| Dashboard/Agents/Slides | route load smoke + authenticated render | active |
| Admin | stats and user/admin panels load under stress | planned |
| Browser/Terminal/Canvas/Trx/Memories | open/switch/load smoke | planned |

## Reliability Gates

Release is blocked if any active reliability check fails.

Current active local gate implementation:
- `scripts/e2e/reliability-suite.sh`
- `scripts/e2e/reliability-browser-journey.sh`
- `scripts/e2e/reliability-shared-workspaces.sh`
- `scripts/e2e/reliability-files-local.sh` + `scripts/e2e/reliability-files-mux.ts`

Planned gate additions:
- expand browser E2E journeys to full shared-workspace and file mutation flows
- long soak (>= 8h) with reconnect/restart chaos
- message integrity verifier (loss/dup/order)
