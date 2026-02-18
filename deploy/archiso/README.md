# Oqto Arch Linux ISO

Custom Arch Linux ISO with a full installer that sets up a complete Oqto server.

**One command install** - boot the ISO, run `oqto-install`, and you have a working Oqto server.

## What's Included

### Server Essentials
- fzf, zoxide, fd, ripgrep, bat, eza, jq, yq
- neovim, vim, tmux, yazi
- git, curl, wget, htop, btop

### Oqto Core
| Binary | Purpose |
|--------|---------|
| `oqto` | Control plane server |
| `oqtoctl` | CLI for server management |
| `oqto-runner` | Multi-user process isolation daemon |
| `oqto-files` | File server for workspaces |
| `oqto-sandbox` | Sandbox wrapper (bwrap/sandbox-exec) |
| `pi-bridge` | HTTP/WebSocket bridge for Pi in containers |

### Agent Tools
| Binary | Purpose |
|--------|---------|
| `mmry` | Memory system - persistent agent knowledge |
| `trx` | Task/issue tracking |
| `agntz` | Agent operations (memory, issues, mail, reservations) |
| `hstry` | Session history management |
| `byt` | Cross-repo governance and management |
| `mailz` | Agent messaging and coordination |

### Search & Scraping
| Binary | Purpose |
|--------|---------|
| `sx` | SearXNG web search CLI |
| `scrpr` | Web scraper |

### LLM Tools
| Binary | Purpose |
|--------|---------|
| `eavs` | LLM proxy server |
| `skdlr` | Task scheduler |
| `tmpltr` | Template engine |

### Communication
| Binary | Purpose |
|--------|---------|
| `h8` | Microsoft Exchange CLI (email, calendar) |

### Media
| Binary | Purpose |
|--------|---------|
| `sldr` | Slider/media tool |
| `kokorox` | Text-to-speech server |
| `ears` | Speech-to-text server |

### Other
| Binary | Purpose |
|--------|---------|
| `dgrmr` | Diagram generator |
| `cmfy` | ComfyUI client |
| `hmr` | Home Assistant CLI |
| `ignr` | Gitignore manager |
| `ingestr` | Data ingestion tool |

### Infrastructure
- ttyd (web terminal)
- Caddy (reverse proxy)
- Docker & Podman (container runtimes)
- yay (AUR helper, from Chaotic-AUR)

### Optional (install via yay)

```bash
# OAuth credential manager for h8, neomutt, etc.
yay -S oama-bin

# SearXNG - local metasearch engine (for sx CLI)
yay -S searxng-uwsgi
sudo systemctl enable --now searxng uwsgi@searxng valkey
# Access at http://localhost:8888
```

## Building the ISO

### Prerequisites

Install archiso on an existing Arch system:

```bash
sudo pacman -S archiso
```

### Build Without Binaries

Creates an ISO with all packages but Oqto binaries must be built after install:

```bash
cd deploy/archiso
sudo ./build.sh
```

### Build With Pre-compiled Binaries

Includes all byteowlz binaries (30+ tools) in the ISO:

```bash
cd deploy/archiso
sudo ./build.sh --with-binaries
```

This builds and includes:
- Oqto core: `oqto`, `oqtoctl`, `oqto-runner`, `oqto-files`, `oqto-sandbox`, `pi-bridge`
- Agent tools: `mmry`, `trx`, `agntz`, `hstry`, `byt`, `mailz`
- Search: `sx`, `scrpr`
- LLM: `eavs`, `skdlr`, `tmpltr`
- Communication: `h8`
- Media: `sldr`, `kokorox`, `ears`
- Other: `dgrmr`, `cmfy`, `hmr`, `ignr`, `ingestr`

All binaries are installed to `/usr/local/bin` with mode 755 (accessible to all users).

### Output

The ISO will be created in `~/oqto-iso/` by default:

```
~/oqto-iso/oqto-arch-2026.01.31-x86_64.iso
```

## Installation

### 1. Write to USB

```bash
sudo dd if=~/oqto-iso/oqto-arch-*.iso of=/dev/sdX bs=4M status=progress oflag=sync
```

### 2. Boot and Run Installer

Boot from USB. Once at the shell, run:

```bash
oqto-install
```

The installer will:

1. **Check requirements** - internet, boot mode (UEFI/BIOS)
2. **Select disk** - shows available disks, confirms before wiping
3. **Configure system** - hostname, timezone, keyboard layout
4. **Create admin user** - username, password, optional SSH key
5. **Partition disk** - GPT for UEFI, MBR for BIOS
6. **Install base system** - Arch Linux with all packages
7. **Configure bootloader** - GRUB for both UEFI and BIOS
8. **Install Oqto** - binaries and setup scripts
9. **Reboot** into installed system

### 3. Complete Oqto Setup

After reboot, log in as your admin user and run:

```bash
sudo oqto-setup
```

This will:
- Create the `oqto` system user
- Generate JWT secret
- Create default configuration
- Configure secure sudoers (regex patterns, sudo 1.9.10+)
- Enable systemd services

### 4. Create Oqto Admin User

```bash
oqtoctl user bootstrap -u admin -e admin@example.com
```

### 5. Start Oqto

```bash
sudo systemctl start oqto
```

Access the web UI at `http://<server-ip>:8080`

## Configuration

Configuration is stored in `/etc/oqto/config.toml`.

For production deployments, configure:

1. **Domain and HTTPS** - Edit `/etc/caddy/Caddyfile`
2. **Allowed origins** - Update `auth.allowed_origins` in config
3. **Firewall** - Enable UFW: `sudo ufw enable`

## Customization

### Adding Packages

Edit `packages.x86_64` to add or remove packages before building.

### Custom Configuration

Place files in `airootfs/` to include them in the ISO:
- `airootfs/etc/` - System configuration
- `airootfs/usr/local/bin/` - Custom scripts
- `airootfs/root/` - Root user files

### Post-Install Scripts

Modify `airootfs/usr/local/bin/oqto-setup` for custom setup steps.

## Directory Structure

```
deploy/archiso/
|-- airootfs/
|   |-- etc/
|   |   |-- oqto/              # Oqto config (generated at runtime)
|   |   |-- systemd/system/    # Systemd service files
|   |-- usr/local/bin/
|       |-- oqto-setup         # Post-install setup script
|       |-- oqto-first-boot    # First boot script
|-- packages.x86_64            # Package list
|-- pacman.conf                # Pacman configuration
|-- profiledef.sh              # Archiso profile definition
|-- build.sh                   # Build script
|-- README.md                  # This file
```

## Security

### Sudoers Configuration

The setup uses **regex-based sudoers rules** (requires sudo 1.9.10+) to prevent privilege escalation:

- **UID restriction**: Only UIDs 2000-2999 can be created (avoids system/user UIDs)
- **Username prefix**: Only `octo_*` usernames allowed
- **Shell restriction**: Only `/bin/bash` allowed (no arbitrary shells)
- **Group restriction**: Only `oqto` group allowed
- **Path restriction**: chown/mkdir restricted to specific paths with no traversal

For details, see `docs/security/SUDOERS_AUDIT.md` in the main repository.

### Verify Sudoers Security

After installation, run the security test:

```bash
./scripts/test-sudoers-security.sh
```

## Troubleshooting

### Build Fails with Missing Packages

Update mirrors and try again:

```bash
sudo pacman -Sy archiso
```

### ISO Too Large

Remove unnecessary packages from `packages.x86_64`. The base ISO is ~2GB.

### Binaries Not Working

Ensure you're building on the same architecture (x86_64) as the target.

### Sudoers Validation Fails

The secure sudoers configuration requires sudo 1.9.10+ for regex support. Arch Linux ships with a recent sudo version, but if you see validation errors:

```bash
# Check sudo version
sudo --version

# If regex not supported, you'll see syntax errors during visudo -c
```

If using an older sudo, the setup will warn and skip sudoers configuration. You'll need to manually configure less secure wildcard-based rules or upgrade sudo.
