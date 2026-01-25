"""System settings access."""

from .device import get_device


def get_setting(namespace: str, key: str) -> str:
    """Get a system setting.

    Args:
        namespace: Setting namespace - 'secure', 'global', or 'system'
        key: Setting key

    Returns:
        Setting value.
    """
    device = get_device()
    result = device.shell(f"settings get {namespace} {key}")
    return result.output.strip()


def set_setting(namespace: str, key: str, value: str):
    """Set a system setting.

    Args:
        namespace: Setting namespace - 'secure', 'global', or 'system'
        key: Setting key
        value: Setting value
    """
    device = get_device()
    device.shell(f"settings put {namespace} {key} {value}")


def delete_setting(namespace: str, key: str):
    """Delete a system setting.

    Args:
        namespace: Setting namespace
        key: Setting key
    """
    device = get_device()
    device.shell(f"settings delete {namespace} {key}")


def list_settings(namespace: str) -> dict[str, str]:
    """List all settings in a namespace.

    Args:
        namespace: Setting namespace - 'secure', 'global', or 'system'

    Returns:
        Dict of key -> value.
    """
    device = get_device()
    result = device.shell(f"settings list {namespace}")

    settings = {}
    for line in result.output.strip().split("\n"):
        if "=" in line:
            key, value = line.split("=", 1)
            settings[key] = value

    return settings


# Common setting helpers


def get_screen_brightness() -> int:
    """Get screen brightness (0-255)."""
    value = get_setting("system", "screen_brightness")
    return int(value) if value.isdigit() else 0


def set_screen_brightness(value: int):
    """Set screen brightness (0-255)."""
    set_setting("system", "screen_brightness", str(max(0, min(255, value))))


def get_screen_timeout() -> int:
    """Get screen timeout in milliseconds."""
    value = get_setting("system", "screen_off_timeout")
    return int(value) if value.isdigit() else 0


def set_screen_timeout(ms: int):
    """Set screen timeout in milliseconds."""
    set_setting("system", "screen_off_timeout", str(ms))


def is_airplane_mode() -> bool:
    """Check if airplane mode is enabled."""
    return get_setting("global", "airplane_mode_on") == "1"


def set_airplane_mode(enabled: bool):
    """Set airplane mode."""
    device = get_device()
    value = "1" if enabled else "0"
    set_setting("global", "airplane_mode_on", value)
    # Broadcast the change
    device.shell(
        f"am broadcast -a android.intent.action.AIRPLANE_MODE --ez state {str(enabled).lower()}"
    )


def is_wifi_enabled() -> bool:
    """Check if WiFi is enabled."""
    return get_setting("global", "wifi_on") == "1"


def set_wifi_enabled(enabled: bool):
    """Enable/disable WiFi."""
    device = get_device()
    cmd = "enable" if enabled else "disable"
    device.shell(f"svc wifi {cmd}")


def is_bluetooth_enabled() -> bool:
    """Check if Bluetooth is enabled."""
    return get_setting("global", "bluetooth_on") == "1"


def set_bluetooth_enabled(enabled: bool):
    """Enable/disable Bluetooth."""
    device = get_device()
    cmd = "enable" if enabled else "disable"
    device.shell(f"svc bluetooth {cmd}")


def get_location_mode() -> int:
    """Get location mode (0=off, 1=sensors, 2=battery, 3=high accuracy)."""
    value = get_setting("secure", "location_mode")
    return int(value) if value.isdigit() else 0


def set_location_mode(mode: int):
    """Set location mode."""
    set_setting("secure", "location_mode", str(mode))


def enable_accessibility_service(service: str):
    """Enable an accessibility service.

    Args:
        service: Service component (e.g., com.app/com.app.MyAccessibilityService)
    """
    # Get current services
    current = get_setting("secure", "enabled_accessibility_services")

    if current and current != "null":
        if service not in current:
            services = f"{current}:{service}"
        else:
            services = current
    else:
        services = service

    set_setting("secure", "enabled_accessibility_services", services)
    set_setting("secure", "accessibility_enabled", "1")


def disable_accessibility_service(service: str):
    """Disable an accessibility service."""
    current = get_setting("secure", "enabled_accessibility_services")

    if current and service in current:
        services = ":".join([s for s in current.split(":") if s != service])
        if services:
            set_setting("secure", "enabled_accessibility_services", services)
        else:
            set_setting("secure", "accessibility_enabled", "0")
            delete_setting("secure", "enabled_accessibility_services")


def get_default_input_method() -> str:
    """Get default input method."""
    return get_setting("secure", "default_input_method")


def grant_permission(package: str, permission: str):
    """Grant a runtime permission to an app.

    Args:
        package: Package name
        permission: Permission (e.g., android.permission.READ_CONTACTS)
    """
    device = get_device()
    device.shell(f"pm grant {package} {permission}")


def revoke_permission(package: str, permission: str):
    """Revoke a runtime permission from an app."""
    device = get_device()
    device.shell(f"pm revoke {package} {permission}")


def list_permissions(package: str) -> str:
    """List permissions for a package."""
    device = get_device()
    result = device.shell(f"dumpsys package {package} | grep permission")
    return result.output
