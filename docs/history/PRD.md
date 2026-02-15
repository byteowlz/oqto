# **Product Requirements Document (PRD)**

> **ARCHIVED DOCUMENT** - This document describes the original architecture design when OpenCode was planned as the primary agent runtime. The current implementation uses **Pi** as the primary harness via the canonical protocol. This document is kept for historical reference only.

## **AI Agent Workspace Platform**

**Version:** 2.0 (Final)
**Status:** Archived - Historical Reference

---

# **1. Product Vision**

To build a secure, scalable, and enterprise-ready **Agent Workspace Platform** hosted on internal infrastructure.

The platform provides persistent, isolated environments where **both technical and non-technical users** collaborate with specialized AI agents. Unlike rigid "coding tools," this system uses a **Universal Agent Engine** (`opencode`) that adapts to different tasks (coding, research, data analysis) via configuration templates.

**Key Value Propositions:**

- **For Devs:** A secure cloud IDE with terminal access, Git, and unrestricted coding agents.
- **For Non-Techs:** A safe, document-centric research assistant with no scary terminals.
- **For IT:** Self-hosted, single-binary deployment (v1), strict isolation, and deep observability.

---

# **2. Architecture Overview**

The system is designed as a **Control Plane / Worker** architecture. In V1, both run on a single VPS, but they are logically decoupled to allow future scaling.

### **2.1 The Stack**

| Layer              | Technology               | Role                                                                     |
| :----------------- | :----------------------- | :----------------------------------------------------------------------- |
| **Frontend**       | **Next.js** (App Router) | OIDC Auth, UI, Multilinguality, Admin Dashboard.                         |
| **Control Plane**  | **Rust** (Axum/Actix)    | Stateless API. Orchestrates sessions, proxies streams, manages DB.       |
| **Worker Runtime** | **Podman** (Rootless)    | Runs ephemeral agent containers.                                         |
| **Agent Engine**   | **opencode** (CLI)       | Runs inside containers. The "brain" executing tools and reading context. |
| **Storage**        | **PostgreSQL**           | User profiles, project metadata, session registry.                       |
| **File Storage**   | **Azure Blob Storage**   | Canonical source of truth for workspace files (synced to local disk).    |
| **Reverse Proxy**  | **Caddy**                | SSL termination, routing to Next.js vs Rust API.                         |

### **2.2 The "Box" Model (Network Flow)**

The browser **never** talks to containers directly.

```
Browser  <-->  Caddy  <-->  Next.js (UI/Auth)
                          <-->  Rust Control Plane (API)
                                    |
                                    +--> (Internal Network / Localhost)
                                            |
                                            +--> Podman Container (Worker)
                                                   |-- opencode (HTTP 8080)
                                                   +-- ttyd (WS 9090)
```

---

# **3. Core Concepts**

### **3.1 The Universal Engine (Runtime)**

Instead of maintaining multiple Docker images, we use **one standard image** (`agent-runtime`) containing:

- `opencode` (The Agent)
- `ttyd` (Terminal Bridge)
- Standard Linux Toolchain (`git`, `curl`, `ripgrep`, `python`, `node`, `neovim`, `fd`, `yazi`, `tmux`)
- Document Parsers (`pandoc`, `poppler`)
- byteowlz toolbox (`lst`, `mmry`, `sx`, `scrpr`, `tmpltr`)

### **3.2 Agent Templates (Configuration Injection)**

Different user experiences are created by injecting specific configurations (`AGENTS.md` + `opencode.json`) into the universal image at startup.

| Template               | Persona          | Capabilities (Injected Config)                                                                                              | UI Experience                                     |
| :--------------------- | :--------------- | :-------------------------------------------------------------------------------------------------------------------------- | :------------------------------------------------ |
| **Coding Copilot**     | Developer        | **Role:** Senior Engineer<br>**Tools:** Shell access, Git, Edit Code<br>**Network:** Unrestricted (or Allow List)           | Chat + File Tree + **Terminal**                   |
| **Research Assistant** | Knowledge Worker | **Role:** Analyst<br>**Tools:** Read Files, Search, Summarize<br>**Block:** `bash`, `write_code`<br>**Network:** Restricted | Chat + File Tree + **Preview Pane** (No Terminal) |
| **Meeting Synth**      | Manager          | **Role:** Secretary<br>**Tools:** Transcript parsing, Summarization<br>**Block:** All Exec                                  | Chat + **Transcript View**                        |

### **3.3 The Workspace**

A persistent directory (`/srv/projects/<id>`) synced from Azure Blob Storage. It survives container restarts.

---

# **4. Functional Requirements: Frontend**

**Tech:** Next.js, Tailwind, Shadcn UI, `next-intl`.

### **4.1 Authentication & Profile**

- **OIDC Login:** Authenticate via Fraunhofer IdP.
- **Session:** Secure HTTP-only cookies managed by Next.js.
- **Multilingual:** Support **English (en)** and **German (de)**. Auto-detect via headers, persist in DB. All static text localized.

### **4.2 User Interface**

- **Workspace Picker:** Grid view of projects.
- **Agent Gallery:** Card-based selection to start a session ("Pick your Agent").
- **Active Session View (Split Pane):**
  - **Left:** Chat (streaming, markdown support).
  - **Right (Dynamic):**
    - **File Tree:** Monaco-based editor (read-only or edit depending on role).
    - **Terminal:** `ghostty-web` (WASM) connected via WebSocket. **Hidden** for non-tech templates.
    - **Preview:** Safe iframe for PDFs/HTML.

### **4.3 Admin Dashboard ("Dokploy-Style")**

- **Access:** Protected route (Admin role only).
- **Live Metrics:** Real-time charts (CPU/RAM/Network) driven by SSE stream from Rust backend.
- **Management:**
  - List all active sessions (User, Template, Duration).
  - Force-kill sessions.
  - View Worker Node status.

---

# **5. Functional Requirements: Backend (Control Plane)**

**Tech:** Rust.

### **5.1 Session Orchestration**

- **Start:**
  1.  Validate User & Project access.
  2.  Trigger **Sync Worker** (Blob -\> Local).
  3.  Generate `AGENTS.md` based on selected Template.
  4.  Run Podman Container (Rootless) mounting Workspace + Generated Config.
- **Stop:**
  1.  Kill Container.
  2.  Trigger **Sync Worker** (Local -\> Blob).

### **5.2 Proxying**

- **HTTP Proxy:** Forward `/api/session/<id>/code/*` -\> Container Port 8080 (opencode). Support **Streaming** (Chunked Transfer).
- **WebSocket Proxy:** Forward `/api/session/<id>/term` -\> Container Port 9090 (ttyd).

### **5.3 Observability**

- Collect host metrics (CPU/RAM) via `/proc`.
- Collect container stats via `podman stats`.
- Broadcast via SSE endpoint `/api/admin/metrics`.

---

# **6. Functional Requirements: Runtime**

### **6.1 The Container**

- **Rootless:** Must run as a non-root user mapped to the project owner on the host.
- **Network:** `slirp4netns`. No inbound access. Outbound restricted by template policy.

### **6.2 Tools & Integrations**

- **Git:** Agent uses injected credentials (GitLab Tokens) to clone/push.
- **Files:** Agent reads/writes to `/workspace` mount.

---

# **7. Operational & Infrastructure**

### **7.1 Provisioning (Ansible)**

- **Goal:** "One-Click VPS Setup."
- **Tasks:** Install Podman, Caddy, Postgres (local for v1), Create Users (`ai-platform`), Deploy Binaries.

### **7.2 CI/CD (GitLab CI)**

- **Pipeline:** Test -\> Build Containers -\> Push to Registry -\> **Trigger Webhook**.
- **Auto-Deploy:** VPS runs a tiny agent (or Rust endpoint) that receives the webhook, pulls new images, and restarts systemd services.

### **7.3 Monitoring**

- Basic logs to `journald`.
- Application-level metrics via Admin Dashboard.

---

# **8. Security Constraints**

1.  **Isolation:** Rootless containers are mandatory.
2.  **No Direct Access:** Frontend never sees container IPs/Ports.
3.  **Token Safety:** GitLab/Blob tokens are injected as Environment Variables or Secrets, never written to disk inside the image.
4.  **Admin Gating:** Admin dashboard APIs must verify Admin role on every request.

---

# **9. Scale Strategy**

- **V1:** Single VPS. Local Postgres. Local Podman.
- **V2:** External Postgres & Blob.
- **V3:** Multiple Worker Nodes. Rust API becomes a scheduler/load-balancer for sessions.

---

# **10. Implementation Plan (Next Steps)**

1.  **Infrastructure (Day 1):** Create the **Ansible Playbook** to provision the VPS (Podman, Users, Caddy).
2.  **Control Plane (Day 2-3):** Scaffold **Rust API** with Podman wrapper and Proxy logic.
3.  **Frontend (Day 4-5):** Scaffold **Next.js** with OIDC and Shadcn.
4.  **Integration (Day 6):** Connect Frontend -\> Backend -\> Podman.
5.  **Refinement:** Add Admin Dashboard and Templates.
