# Oqto Ansible Deployment

Ansible playbook for setting up and hardening a Linux server for Oqto.

## What It Does

1. **System hardening**
   - SSH hardening (key-only auth, strong ciphers)
   - fail2ban for brute-force protection
   - UFW firewall configuration
   - Automatic security updates
   - Kernel security parameters
   - Audit logging

2. **Installs Oqto dependencies**
   - Rust toolchain
   - Bun (JavaScript runtime)
   - Development tools (git, curl, build-essential, etc.)
   - Shell tools (tmux, zsh, ripgrep, fd, fzf, zoxide, yazi)
   - trash-cli (safe file deletion)
   - ttyd (web terminal, local mode only)
   - Docker (container mode only)

3. **Installs agent tools**
   - agntz (agent operations)
   - mmry (memory system)
   - trx (task tracking)
   - mailz (agent messaging)
   - OpenCode (local mode only)

4. **Configures Oqto**
   - Creates oqto system user
   - Sets up directories
   - Deploys systemd services
   - Generates configuration

## Prerequisites

- Ansible 2.9+
- Target server running Debian/Ubuntu, RHEL/CentOS, or Arch Linux
- SSH access to target server

## Usage

1. Copy the example inventory:
   ```bash
   cp inventory.yml.example inventory.yml
   ```

2. Edit `inventory.yml` with your server details:
   ```yaml
   servers:
     hosts:
       oqto-server:
         ansible_host: your.server.ip
         ansible_user: root
   vars:
     admin_user: yourusername
     admin_ssh_key: "ssh-ed25519 AAAA..."
     octo_mode: single      # or multi
     octo_backend: local    # or container
   ```

3. Run the playbook:
   ```bash
   ansible-playbook -i inventory.yml oqto.yml
   ```

4. After deployment, build and install Oqto binaries on the server:
   ```bash
   ssh admin@your.server.ip
   git clone https://github.com/byteowlz/oqto.git
   cd oqto
   ./setup.sh --non-interactive
   ```

## Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `admin_user` | - | Admin username to create |
| `admin_ssh_key` | - | SSH public key for admin |
| `octo_mode` | `single` | `single` or `multi` user mode |
| `octo_backend` | `local` | `local` (native) or `container` (Podman) |
| `ssh_port` | `22` | SSH port |
| `allowed_ports` | `[8080, 3000]` | Additional firewall ports |
| `octo_user` | `oqto` | System user for Oqto service |
| `octo_group` | `oqto` | System group for Oqto |

## Post-Deployment

1. **Install Oqto binaries** - The playbook prepares the environment but doesn't build Oqto from source. SSH into the server and run:
   ```bash
   cd /path/to/oqto
   cargo install --path backend
   cargo install --path fileserver
   sudo cp ~/.cargo/bin/oqto /usr/local/bin/
   sudo cp ~/.cargo/bin/fileserver /usr/local/bin/
   ```

2. **Configure JWT secret** - Edit `/etc/oqto/config.toml`:
   ```bash
   sudo -u oqto openssl rand -base64 48
   # Add to config.toml under [auth]
   ```

3. **Start Oqto**:
   ```bash
   sudo systemctl start oqto
   sudo systemctl status oqto
   ```

4. **Build frontend** (if serving from this server):
   ```bash
   cd /path/to/oqto/frontend
   bun install && bun run build
   ```

## File Structure

```
deploy/ansible/
  oqto.yml                    # Main playbook
  inventory.yml.example       # Example inventory
  README.md                   # This file
  templates/
    sshd-hardening.conf.j2    # SSH config template
    fail2ban-jail.local.j2    # fail2ban config template
    oqto.service.j2           # systemd service template
    oqto-config.toml.j2       # Oqto config template
```
