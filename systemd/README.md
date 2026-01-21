# Systemd Service Files

This directory contains systemd service files for running Octo components.

## octo-runner

The `octo-runner` daemon enables multi-user process isolation without requiring the main Octo server to have elevated privileges.

### How It Works

1. Each Linux user runs their own `octo-runner` instance as a systemd user service
2. The runner listens on a Unix socket at `/run/user/<uid>/octo-runner.sock`
3. The main Octo server communicates with runners to spawn processes as the appropriate user
4. Processes (opencode, pi, etc.) run with the user's privileges naturally

### Installation (Per User)

```bash
# Copy the service file
mkdir -p ~/.config/systemd/user
cp octo-runner.service ~/.config/systemd/user/

# Reload systemd and enable the service
systemctl --user daemon-reload
systemctl --user enable octo-runner
systemctl --user start octo-runner

# Check status
systemctl --user status octo-runner
```

### System-Wide Installation (Admin)

For deploying to all users automatically:

```bash
# Install service file system-wide
sudo cp octo-runner.service /usr/lib/systemd/user/

# Enable for a specific user
sudo loginctl enable-linger <username>
sudo -u <username> systemctl --user enable octo-runner
sudo -u <username> systemctl --user start octo-runner
```

Note: When Octo is configured for local multi-user Linux isolation, it will also
attempt to enable lingering and start `octo-runner` automatically when provisioning
new platform users.

For non-root Octo backends, prefer shared runner sockets under:
`/run/octo/runner-sockets/<user>/octo-runner.sock` (see `systemd/octo-runner.tmpfiles.conf`).

### Socket Activation (Optional)

For on-demand startup instead of always-running:

```bash
cp octo-runner.socket ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user enable octo-runner.socket
systemctl --user start octo-runner.socket
# Don't start the .service directly - it will start on first connection
```

### Troubleshooting

Check logs:
```bash
journalctl --user -u octo-runner -f
```

Test the runner manually:
```bash
octo-runner --socket /tmp/test-runner.sock --verbose
```

Test communication:
```bash
echo '{"type":"ping"}' | nc -U /run/user/$(id -u)/octo-runner.sock
```
