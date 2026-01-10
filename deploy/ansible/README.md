# Octo Ansible Deployment

Ansible playbook for setting up and hardening a Linux server for Octo.

## What It Does

1. **System hardening**
   - SSH hardening (key-only auth, strong ciphers)
   - fail2ban for brute-force protection
   - UFW firewall configuration
   - Automatic security updates
   - Kernel security parameters
   - Audit logging

2. **Installs Octo dependencies**
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

4. **Configures Octo**
   - Creates octo system user
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
       octo-server:
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
   ansible-playbook -i inventory.yml octo.yml
   ```

4. After deployment, build and install Octo binaries on the server:
   ```bash
   ssh admin@your.server.ip
   git clone https://github.com/byteowlz/octo.git
   cd octo
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
| `octo_user` | `octo` | System user for Octo service |
| `octo_group` | `octo` | System group for Octo |

## Post-Deployment

1. **Install Octo binaries** - The playbook prepares the environment but doesn't build Octo from source. SSH into the server and run:
   ```bash
   cd /path/to/octo
   cargo install --path backend
   cargo install --path fileserver
   sudo cp ~/.cargo/bin/octo /usr/local/bin/
   sudo cp ~/.cargo/bin/fileserver /usr/local/bin/
   ```

2. **Configure JWT secret** - Edit `/etc/octo/config.toml`:
   ```bash
   sudo -u octo openssl rand -base64 48
   # Add to config.toml under [auth]
   ```

3. **Start Octo**:
   ```bash
   sudo systemctl start octo
   sudo systemctl status octo
   ```

4. **Build frontend** (if serving from this server):
   ```bash
   cd /path/to/octo/frontend
   bun install && bun run build
   ```

## File Structure

```
deploy/ansible/
  octo.yml                    # Main playbook
  inventory.yml.example       # Example inventory
  README.md                   # This file
  templates/
    sshd-hardening.conf.j2    # SSH config template
    fail2ban-jail.local.j2    # fail2ban config template
    octo.service.j2           # systemd service template
    octo-config.toml.j2       # Octo config template
```
