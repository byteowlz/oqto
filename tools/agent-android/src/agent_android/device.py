"""Device management - wraps uiautomator2 for device connection and control."""

from typing import Optional
import uiautomator2 as u2
from adbutils import adb

# Global device reference for single-session mode
_current_device: Optional[u2.Device] = None
_sessions: dict[str, u2.Device] = {}


class DeviceManager:
    """Manages Android device connections."""

    def connect(
        self,
        device_serial: Optional[str] = None,
        session_name: Optional[str] = None,
    ) -> u2.Device:
        """Connect to a device.

        Args:
            device_serial: ADB serial (e.g., 'emulator-5554'). If None, connects to first device.
            session_name: Optional session name for multi-session management.

        Returns:
            Connected uiautomator2 Device instance.
        """
        global _current_device, _sessions

        if device_serial:
            device = u2.connect(device_serial)
        else:
            # Connect to first available device
            devices = adb.device_list()
            if not devices:
                raise RuntimeError(
                    "No Android devices found. Start an emulator or connect a device."
                )
            device = u2.connect(devices[0].serial)

        _current_device = device

        if session_name:
            _sessions[session_name] = device

        return device

    def disconnect(self, session_name: Optional[str] = None):
        """Disconnect a session."""
        global _current_device, _sessions

        if session_name and session_name in _sessions:
            del _sessions[session_name]
        else:
            _current_device = None

    def get_session(self, session_name: str) -> Optional[u2.Device]:
        """Get device for a named session."""
        return _sessions.get(session_name)


def get_device() -> u2.Device:
    """Get the current device.

    Returns:
        The currently connected device.

    Raises:
        RuntimeError: If no device is connected.
    """
    global _current_device

    if _current_device is None:
        # Try to auto-connect
        mgr = DeviceManager()
        _current_device = mgr.connect()

    return _current_device


def list_devices() -> list[dict]:
    """List all connected devices.

    Returns:
        List of device info dicts with serial, state, etc.
    """
    devices = []
    for dev in adb.device_list():
        devices.append(
            {
                "serial": dev.serial,
                "state": dev.get_state(),
            }
        )
    return devices
