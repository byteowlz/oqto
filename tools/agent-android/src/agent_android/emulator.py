"""
Emulator lifecycle management with persistent state.

Key features:
- Per-session AVD instances with isolated userdata
- Persistent app data across emulator restarts
- Named snapshots for state checkpoints
- Headless mode for CI/server environments

Storage structure:
    ~/.local/share/agent-android/
    ├── avds/                      # Base AVD templates
    │   └── base_pixel7_api34/
    ├── sessions/                  # Per-session data (persists)
    │   ├── session_abc123/
    │   │   ├── userdata.img       # App data, settings
    │   │   ├── sdcard.img         # SD card storage
    │   │   ├── snapshots/         # Named snapshots
    │   │   └── metadata.json      # Session info
    │   └── session_def456/
    └── shared/
        └── base_userdata.img      # Template with pre-installed apps
"""

import os
import json
import shutil
import subprocess
import time
import signal
from pathlib import Path
from dataclasses import dataclass, field, asdict
from typing import Optional
from datetime import datetime
import threading


# Default paths
DEFAULT_DATA_DIR = Path.home() / ".local" / "share" / "agent-android"
DEFAULT_ANDROID_SDK = Path(os.environ.get("ANDROID_HOME", Path.home() / "Android" / "Sdk"))


@dataclass
class EmulatorConfig:
    """Configuration for an emulator instance."""

    # AVD settings
    avd_name: str = "base_pixel7_api34"
    system_image: str = "system-images;android-34;google_apis;x86_64"
    device_profile: str = "pixel_7"

    # Display
    headless: bool = True
    gpu: str = "swiftshader_indirect"  # For headless: swiftshader_indirect, host for display

    # Resources
    memory_mb: int = 4096
    cores: int = 4
    sdcard_mb: int = 2048

    # Networking
    port: Optional[int] = None  # Auto-assign if None

    # Persistence
    writable_system: bool = False
    snapshot_on_exit: bool = True


@dataclass
class SessionMetadata:
    """Metadata for a persistent session."""

    session_id: str
    created_at: str
    last_accessed: str
    avd_name: str
    system_image: str
    installed_packages: list[str] = field(default_factory=list)
    snapshots: list[str] = field(default_factory=list)
    notes: str = ""


class EmulatorManager:
    """Manages Android emulator instances with persistent state."""

    def __init__(self, data_dir: Optional[Path] = None, sdk_path: Optional[Path] = None):
        self.data_dir = Path(data_dir) if data_dir else DEFAULT_DATA_DIR
        self.sdk_path = Path(sdk_path) if sdk_path else DEFAULT_ANDROID_SDK

        # Create directory structure
        self.avds_dir = self.data_dir / "avds"
        self.sessions_dir = self.data_dir / "sessions"
        self.shared_dir = self.data_dir / "shared"

        for d in [self.avds_dir, self.sessions_dir, self.shared_dir]:
            d.mkdir(parents=True, exist_ok=True)

        # Track running emulators
        self._processes: dict[str, subprocess.Popen] = {}
        self._ports: dict[str, int] = {}

    @property
    def emulator_bin(self) -> Path:
        return self.sdk_path / "emulator" / "emulator"

    @property
    def avdmanager_bin(self) -> Path:
        return self.sdk_path / "cmdline-tools" / "latest" / "bin" / "avdmanager"

    @property
    def sdkmanager_bin(self) -> Path:
        return self.sdk_path / "cmdline-tools" / "latest" / "bin" / "sdkmanager"

    @property
    def adb_bin(self) -> Path:
        return self.sdk_path / "platform-tools" / "adb"

    # =========================================================================
    # AVD Management
    # =========================================================================

    def ensure_system_image(self, system_image: str) -> bool:
        """Ensure a system image is installed.

        Args:
            system_image: System image package name (e.g., system-images;android-34;google_apis;x86_64)

        Returns:
            True if installed/available.
        """
        # Check if already installed
        result = subprocess.run(
            [str(self.sdkmanager_bin), "--list_installed"],
            capture_output=True,
            text=True,
        )

        if system_image in result.stdout:
            return True

        # Install it
        print(f"Installing system image: {system_image}")
        result = subprocess.run(
            [str(self.sdkmanager_bin), system_image],
            input="y\n",  # Accept license
            capture_output=True,
            text=True,
        )

        return result.returncode == 0

    def create_base_avd(self, config: EmulatorConfig) -> bool:
        """Create a base AVD that sessions will use.

        Args:
            config: Emulator configuration

        Returns:
            True if created successfully.
        """
        # Ensure system image is available
        if not self.ensure_system_image(config.system_image):
            raise RuntimeError(f"Failed to install system image: {config.system_image}")

        # Check if AVD already exists
        result = subprocess.run(
            [str(self.avdmanager_bin), "list", "avd", "-c"],
            capture_output=True,
            text=True,
        )

        if config.avd_name in result.stdout.split("\n"):
            print(f"AVD '{config.avd_name}' already exists")
            return True

        # Create AVD
        cmd = [
            str(self.avdmanager_bin),
            "create",
            "avd",
            "--name",
            config.avd_name,
            "--package",
            config.system_image,
            "--device",
            config.device_profile,
            "--force",
        ]

        result = subprocess.run(
            cmd,
            input="no\n",  # Don't create custom hardware profile
            capture_output=True,
            text=True,
        )

        if result.returncode != 0:
            raise RuntimeError(f"Failed to create AVD: {result.stderr}")

        print(f"Created AVD: {config.avd_name}")
        return True

    def list_avds(self) -> list[str]:
        """List available AVDs."""
        result = subprocess.run(
            [str(self.avdmanager_bin), "list", "avd", "-c"],
            capture_output=True,
            text=True,
        )
        return [avd.strip() for avd in result.stdout.split("\n") if avd.strip()]

    def delete_avd(self, avd_name: str) -> bool:
        """Delete an AVD."""
        result = subprocess.run(
            [str(self.avdmanager_bin), "delete", "avd", "--name", avd_name],
            capture_output=True,
            text=True,
        )
        return result.returncode == 0

    # =========================================================================
    # Session Management (Persistent State)
    # =========================================================================

    def create_session(
        self,
        session_id: str,
        config: Optional[EmulatorConfig] = None,
        from_template: Optional[str] = None,
    ) -> Path:
        """Create a new persistent session.

        Args:
            session_id: Unique session identifier
            config: Emulator configuration (uses defaults if None)
            from_template: Copy userdata from another session or 'base'

        Returns:
            Path to session directory.
        """
        config = config or EmulatorConfig()

        session_dir = self.sessions_dir / session_id
        if session_dir.exists():
            raise ValueError(f"Session '{session_id}' already exists")

        session_dir.mkdir(parents=True)
        (session_dir / "snapshots").mkdir()

        # Create metadata
        metadata = SessionMetadata(
            session_id=session_id,
            created_at=datetime.now().isoformat(),
            last_accessed=datetime.now().isoformat(),
            avd_name=config.avd_name,
            system_image=config.system_image,
        )

        self._save_metadata(session_dir, metadata)

        # Copy userdata from template if specified
        if from_template:
            if from_template == "base":
                base_userdata = self.shared_dir / "base_userdata.img"
            else:
                base_userdata = self.sessions_dir / from_template / "userdata.img"

            if base_userdata.exists():
                shutil.copy(base_userdata, session_dir / "userdata.img")
                print(f"Copied userdata from {from_template}")

        print(f"Created session: {session_id} at {session_dir}")
        return session_dir

    def get_session(self, session_id: str) -> Optional[Path]:
        """Get path to a session directory."""
        session_dir = self.sessions_dir / session_id
        if session_dir.exists():
            return session_dir
        return None

    def list_sessions(self) -> list[SessionMetadata]:
        """List all sessions with their metadata."""
        sessions = []
        for session_dir in self.sessions_dir.iterdir():
            if session_dir.is_dir():
                metadata = self._load_metadata(session_dir)
                if metadata:
                    sessions.append(metadata)
        return sessions

    def delete_session(self, session_id: str, force: bool = False) -> bool:
        """Delete a session and all its data.

        Args:
            session_id: Session to delete
            force: Delete even if emulator is running

        Returns:
            True if deleted.
        """
        if session_id in self._processes:
            if force:
                self.stop(session_id)
            else:
                raise RuntimeError(
                    f"Session '{session_id}' is running. Stop it first or use force=True"
                )

        session_dir = self.sessions_dir / session_id
        if session_dir.exists():
            shutil.rmtree(session_dir)
            print(f"Deleted session: {session_id}")
            return True
        return False

    def _save_metadata(self, session_dir: Path, metadata: SessionMetadata):
        """Save session metadata."""
        metadata_file = session_dir / "metadata.json"
        with open(metadata_file, "w") as f:
            json.dump(asdict(metadata), f, indent=2)

    def _load_metadata(self, session_dir: Path) -> Optional[SessionMetadata]:
        """Load session metadata."""
        metadata_file = session_dir / "metadata.json"
        if metadata_file.exists():
            with open(metadata_file) as f:
                data = json.load(f)
                return SessionMetadata(**data)
        return None

    # =========================================================================
    # Emulator Lifecycle
    # =========================================================================

    def start(
        self,
        session_id: str,
        config: Optional[EmulatorConfig] = None,
        wait_boot: bool = True,
        timeout: int = 120,
    ) -> str:
        """Start an emulator for a session.

        Args:
            session_id: Session to start
            config: Override configuration
            wait_boot: Wait for device to fully boot
            timeout: Boot timeout in seconds

        Returns:
            ADB device serial (e.g., emulator-5554)
        """
        if session_id in self._processes:
            serial = f"emulator-{self._ports[session_id]}"
            print(f"Session '{session_id}' already running at {serial}")
            return serial

        session_dir = self.get_session(session_id)
        if not session_dir:
            # Auto-create session
            session_dir = self.create_session(session_id, config)

        config = config or EmulatorConfig()
        metadata = self._load_metadata(session_dir)

        # Ensure base AVD exists
        self.create_base_avd(config)

        # Find available port
        port = config.port or self._find_available_port()
        self._ports[session_id] = port

        # Build emulator command
        cmd = [
            str(self.emulator_bin),
            "-avd",
            config.avd_name,
            "-port",
            str(port),
            "-memory",
            str(config.memory_mb),
            "-cores",
            str(config.cores),
        ]

        # Session-specific userdata for persistence
        userdata_path = session_dir / "userdata.img"
        if userdata_path.exists():
            cmd.extend(["-data", str(userdata_path)])

        # SD card
        sdcard_path = session_dir / "sdcard.img"
        if sdcard_path.exists():
            cmd.extend(["-sdcard", str(sdcard_path)])

        # Headless mode
        if config.headless:
            cmd.extend(
                [
                    "-no-window",
                    "-no-audio",
                    "-no-boot-anim",
                    "-gpu",
                    config.gpu,
                ]
            )
        else:
            cmd.extend(["-gpu", "host"])

        # Writable system
        if config.writable_system:
            cmd.append("-writable-system")

        # Snapshot behavior
        if not config.snapshot_on_exit:
            cmd.append("-no-snapshot-save")

        # Start emulator
        print(f"Starting emulator for session '{session_id}' on port {port}...")
        print(f"Command: {' '.join(cmd)}")

        process = subprocess.Popen(
            cmd,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            preexec_fn=os.setsid,  # New process group for clean shutdown
        )

        self._processes[session_id] = process

        serial = f"emulator-{port}"

        # Wait for boot
        if wait_boot:
            if not self._wait_for_boot(serial, timeout):
                self.stop(session_id)
                raise RuntimeError(f"Emulator failed to boot within {timeout}s")

        # Update metadata
        if metadata:
            metadata.last_accessed = datetime.now().isoformat()
            self._save_metadata(session_dir, metadata)

        print(f"Emulator ready: {serial}")
        return serial

    def stop(self, session_id: str, save_snapshot: bool = True) -> bool:
        """Stop an emulator.

        Args:
            session_id: Session to stop
            save_snapshot: Save quickboot snapshot on exit

        Returns:
            True if stopped successfully.
        """
        if session_id not in self._processes:
            print(f"Session '{session_id}' is not running")
            return False

        process = self._processes[session_id]
        port = self._ports[session_id]
        serial = f"emulator-{port}"

        # Graceful shutdown via ADB
        try:
            subprocess.run(
                [str(self.adb_bin), "-s", serial, "emu", "kill"],
                capture_output=True,
                timeout=10,
            )
        except subprocess.TimeoutExpired:
            pass

        # Wait for process to exit
        try:
            process.wait(timeout=30)
        except subprocess.TimeoutExpired:
            # Force kill
            os.killpg(os.getpgid(process.pid), signal.SIGKILL)
            process.wait()

        del self._processes[session_id]
        del self._ports[session_id]

        print(f"Stopped emulator for session '{session_id}'")
        return True

    def restart(self, session_id: str, config: Optional[EmulatorConfig] = None) -> str:
        """Restart an emulator."""
        self.stop(session_id)
        time.sleep(2)  # Brief pause
        return self.start(session_id, config)

    def is_running(self, session_id: str) -> bool:
        """Check if a session's emulator is running."""
        if session_id not in self._processes:
            return False

        process = self._processes[session_id]
        return process.poll() is None

    def get_serial(self, session_id: str) -> Optional[str]:
        """Get ADB serial for a running session."""
        if session_id in self._ports:
            return f"emulator-{self._ports[session_id]}"
        return None

    # =========================================================================
    # Snapshots
    # =========================================================================

    def save_snapshot(self, session_id: str, snapshot_name: str) -> bool:
        """Save a named snapshot for a session.

        Args:
            session_id: Running session
            snapshot_name: Name for the snapshot

        Returns:
            True if saved successfully.
        """
        serial = self.get_serial(session_id)
        if not serial:
            raise RuntimeError(f"Session '{session_id}' is not running")

        result = subprocess.run(
            [str(self.adb_bin), "-s", serial, "emu", "avd", "snapshot", "save", snapshot_name],
            capture_output=True,
            text=True,
        )

        if result.returncode == 0:
            # Update metadata
            session_dir = self.get_session(session_id)
            metadata = self._load_metadata(session_dir)
            if metadata and snapshot_name not in metadata.snapshots:
                metadata.snapshots.append(snapshot_name)
                self._save_metadata(session_dir, metadata)

            print(f"Saved snapshot: {snapshot_name}")
            return True

        print(f"Failed to save snapshot: {result.stderr}")
        return False

    def load_snapshot(self, session_id: str, snapshot_name: str) -> bool:
        """Load a named snapshot.

        Args:
            session_id: Running session
            snapshot_name: Snapshot to load

        Returns:
            True if loaded successfully.
        """
        serial = self.get_serial(session_id)
        if not serial:
            raise RuntimeError(f"Session '{session_id}' is not running")

        result = subprocess.run(
            [str(self.adb_bin), "-s", serial, "emu", "avd", "snapshot", "load", snapshot_name],
            capture_output=True,
            text=True,
        )

        if result.returncode == 0:
            print(f"Loaded snapshot: {snapshot_name}")
            return True

        print(f"Failed to load snapshot: {result.stderr}")
        return False

    def list_snapshots(self, session_id: str) -> list[str]:
        """List available snapshots for a session."""
        serial = self.get_serial(session_id)
        if not serial:
            # Return from metadata if not running
            session_dir = self.get_session(session_id)
            if session_dir:
                metadata = self._load_metadata(session_dir)
                if metadata:
                    return metadata.snapshots
            return []

        result = subprocess.run(
            [str(self.adb_bin), "-s", serial, "emu", "avd", "snapshot", "list"],
            capture_output=True,
            text=True,
        )

        # Parse snapshot list
        snapshots = []
        for line in result.stdout.split("\n"):
            line = line.strip()
            if line and not line.startswith("ID") and not line.startswith("-"):
                parts = line.split()
                if parts:
                    snapshots.append(parts[0])

        return snapshots

    def delete_snapshot(self, session_id: str, snapshot_name: str) -> bool:
        """Delete a snapshot."""
        serial = self.get_serial(session_id)
        if not serial:
            raise RuntimeError(f"Session '{session_id}' is not running")

        result = subprocess.run(
            [str(self.adb_bin), "-s", serial, "emu", "avd", "snapshot", "delete", snapshot_name],
            capture_output=True,
            text=True,
        )

        if result.returncode == 0:
            # Update metadata
            session_dir = self.get_session(session_id)
            metadata = self._load_metadata(session_dir)
            if metadata and snapshot_name in metadata.snapshots:
                metadata.snapshots.remove(snapshot_name)
                self._save_metadata(session_dir, metadata)

            print(f"Deleted snapshot: {snapshot_name}")
            return True

        return False

    # =========================================================================
    # App Management
    # =========================================================================

    def install_apk(self, session_id: str, apk_path: str) -> bool:
        """Install an APK to a running session.

        Args:
            session_id: Running session
            apk_path: Path to APK file

        Returns:
            True if installed successfully.
        """
        serial = self.get_serial(session_id)
        if not serial:
            raise RuntimeError(f"Session '{session_id}' is not running")

        result = subprocess.run(
            [str(self.adb_bin), "-s", serial, "install", "-r", apk_path],
            capture_output=True,
            text=True,
        )

        if result.returncode == 0:
            # Try to extract package name
            try:
                aapt_result = subprocess.run(
                    ["aapt", "dump", "badging", apk_path],
                    capture_output=True,
                    text=True,
                )
                for line in aapt_result.stdout.split("\n"):
                    if line.startswith("package:"):
                        package = line.split("name='")[1].split("'")[0]
                        # Update metadata
                        session_dir = self.get_session(session_id)
                        metadata = self._load_metadata(session_dir)
                        if metadata and package not in metadata.installed_packages:
                            metadata.installed_packages.append(package)
                            self._save_metadata(session_dir, metadata)
                        break
            except Exception:
                pass

            print(f"Installed: {apk_path}")
            return True

        print(f"Failed to install: {result.stderr}")
        return False

    def list_installed_packages(self, session_id: str) -> list[str]:
        """List installed packages (third-party)."""
        serial = self.get_serial(session_id)
        if not serial:
            raise RuntimeError(f"Session '{session_id}' is not running")

        result = subprocess.run(
            [str(self.adb_bin), "-s", serial, "shell", "pm", "list", "packages", "-3"],
            capture_output=True,
            text=True,
        )

        packages = []
        for line in result.stdout.split("\n"):
            if line.startswith("package:"):
                packages.append(line.replace("package:", "").strip())

        return packages

    # =========================================================================
    # Helpers
    # =========================================================================

    def _find_available_port(self) -> int:
        """Find an available emulator port (must be even, 5554-5682)."""
        import socket

        used_ports = set(self._ports.values())

        for port in range(5554, 5682, 2):
            if port in used_ports:
                continue

            # Check if port is actually available
            with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
                try:
                    s.bind(("localhost", port))
                    s.bind(("localhost", port + 1))  # Console port
                    return port
                except OSError:
                    continue

        raise RuntimeError("No available emulator ports")

    def _wait_for_boot(self, serial: str, timeout: int) -> bool:
        """Wait for emulator to fully boot."""
        start = time.time()

        # Wait for device to appear
        while time.time() - start < timeout:
            result = subprocess.run(
                [str(self.adb_bin), "devices"],
                capture_output=True,
                text=True,
            )
            if (
                serial in result.stdout
                and "device" in result.stdout.split(serial)[1].split("\n")[0]
            ):
                break
            time.sleep(2)
        else:
            return False

        # Wait for boot to complete
        while time.time() - start < timeout:
            result = subprocess.run(
                [str(self.adb_bin), "-s", serial, "shell", "getprop", "sys.boot_completed"],
                capture_output=True,
                text=True,
            )
            if result.stdout.strip() == "1":
                return True
            time.sleep(2)

        return False

    def _cleanup(self):
        """Stop all running emulators."""
        for session_id in list(self._processes.keys()):
            self.stop(session_id)


# Global manager instance
_manager: Optional[EmulatorManager] = None


def get_manager() -> EmulatorManager:
    """Get the global emulator manager."""
    global _manager
    if _manager is None:
        _manager = EmulatorManager()
    return _manager
