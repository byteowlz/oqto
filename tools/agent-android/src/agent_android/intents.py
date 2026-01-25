"""Intent and activity management."""

from typing import Optional
import json

from .device import get_device


def send_intent(
    action: str,
    data: Optional[str] = None,
    package: Optional[str] = None,
    component: Optional[str] = None,
    extras: Optional[str] = None,
    flags: Optional[list[str]] = None,
) -> str:
    """Send an intent to start an activity.

    Args:
        action: Intent action (e.g., android.intent.action.VIEW)
        data: Data URI
        package: Target package
        component: Component name (package/class)
        extras: Extras as JSON string (e.g., '{"key": "value"}')
        flags: Intent flags

    Returns:
        Result of am start command.
    """
    device = get_device()

    cmd = f"am start -a {action}"

    if data:
        cmd += f' -d "{data}"'

    if package:
        cmd += f" -p {package}"

    if component:
        cmd += f" -n {component}"

    if extras:
        # Parse JSON extras and add as -e flags
        try:
            extras_dict = json.loads(extras)
            for key, value in extras_dict.items():
                if isinstance(value, str):
                    cmd += f' -e {key} "{value}"'
                elif isinstance(value, bool):
                    cmd += f" --ez {key} {str(value).lower()}"
                elif isinstance(value, int):
                    cmd += f" --ei {key} {value}"
                elif isinstance(value, float):
                    cmd += f" --ef {key} {value}"
        except json.JSONDecodeError:
            pass

    if flags:
        for flag in flags:
            cmd += f" -f {flag}"

    result = device.shell(cmd)
    return result.output


def send_broadcast(action: str, extras: Optional[str] = None) -> str:
    """Send a broadcast intent.

    Args:
        action: Broadcast action
        extras: Extras as JSON string

    Returns:
        Result of am broadcast command.
    """
    device = get_device()

    cmd = f"am broadcast -a {action}"

    if extras:
        try:
            extras_dict = json.loads(extras)
            for key, value in extras_dict.items():
                if isinstance(value, str):
                    cmd += f' -e {key} "{value}"'
                elif isinstance(value, bool):
                    cmd += f" --ez {key} {str(value).lower()}"
                elif isinstance(value, int):
                    cmd += f" --ei {key} {value}"
        except json.JSONDecodeError:
            pass

    result = device.shell(cmd)
    return result.output


def get_current_activity() -> str:
    """Get the current foreground activity.

    Returns:
        Current activity name (package/class).
    """
    device = get_device()

    # Try dumpsys activity top
    result = device.shell("dumpsys activity top | grep ACTIVITY")
    lines = result.output.strip().split("\n")

    for line in lines:
        if "ACTIVITY" in line:
            # Parse: ACTIVITY com.app/.MainActivity ...
            parts = line.split()
            for part in parts:
                if "/" in part:
                    return part
            if len(parts) > 1:
                return parts[1]

    return "unknown"


def get_activity_stack() -> list[str]:
    """Get the activity stack.

    Returns:
        List of activities in the stack (most recent first).
    """
    device = get_device()

    result = device.shell('dumpsys activity activities | grep "Hist #"')
    lines = result.output.strip().split("\n")

    activities = []
    for line in lines:
        # Parse: * Hist #0: ActivityRecord{...} com.app/.Activity ...
        if "Hist #" in line:
            parts = line.split()
            for part in parts:
                if "/" in part and not part.startswith("{"):
                    activities.append(part)
                    break

    return activities


def get_running_services(package: Optional[str] = None) -> list[str]:
    """Get running services.

    Args:
        package: Filter by package name (optional)

    Returns:
        List of running service names.
    """
    device = get_device()

    cmd = "dumpsys activity services"
    if package:
        cmd += f" {package}"

    result = device.shell(cmd)

    services = []
    for line in result.output.split("\n"):
        if "ServiceRecord{" in line:
            # Parse service name
            parts = line.split()
            for part in parts:
                if "/" in part:
                    services.append(part)
                    break

    return services


def force_stop(package: str) -> str:
    """Force stop an app.

    Args:
        package: Package name

    Returns:
        Result of am force-stop command.
    """
    device = get_device()
    result = device.shell(f"am force-stop {package}")
    return result.output


def clear_app_data(package: str) -> str:
    """Clear app data (like factory reset for the app).

    Args:
        package: Package name

    Returns:
        Result of pm clear command.
    """
    device = get_device()
    result = device.shell(f"pm clear {package}")
    return result.output


def start_service(component: str, extras: Optional[str] = None) -> str:
    """Start a service.

    Args:
        component: Service component (package/class)
        extras: Extras as JSON string

    Returns:
        Result of am startservice command.
    """
    device = get_device()

    cmd = f"am startservice -n {component}"

    if extras:
        try:
            extras_dict = json.loads(extras)
            for key, value in extras_dict.items():
                if isinstance(value, str):
                    cmd += f' -e {key} "{value}"'
        except json.JSONDecodeError:
            pass

    result = device.shell(cmd)
    return result.output


def stop_service(component: str) -> str:
    """Stop a service.

    Args:
        component: Service component (package/class)

    Returns:
        Result of am stopservice command.
    """
    device = get_device()
    result = device.shell(f"am stopservice -n {component}")
    return result.output
