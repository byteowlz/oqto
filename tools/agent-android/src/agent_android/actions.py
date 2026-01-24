"""Action execution - tap, type, swipe, etc."""

from typing import Optional, Tuple
import re

from .device import get_device
from .snapshot import get_element_center, parse_hierarchy


def _parse_target(target: str) -> Tuple[int, int]:
    """Parse target to coordinates.

    Args:
        target: Either '@e1' (element ref) or 'x,y' coordinates.

    Returns:
        (x, y) coordinates.

    Raises:
        ValueError: If target cannot be parsed.
    """
    # Check for element ref
    if target.startswith("@e"):
        coords = get_element_center(target)
        if coords is None:
            # Re-dump hierarchy to populate cache
            device = get_device()
            parse_hierarchy(device.dump_hierarchy())
            coords = get_element_center(target)

        if coords is None:
            raise ValueError(f"Element {target} not found in current UI")
        return coords

    # Check for coordinates
    match = re.match(r"(\d+)\s*,\s*(\d+)", target)
    if match:
        return (int(match.group(1)), int(match.group(2)))

    raise ValueError(f"Invalid target: {target}. Use '@e1' or 'x,y'")


def tap_target(target: str):
    """Tap on a target.

    Args:
        target: Element ref (@e1) or coordinates (x,y).
    """
    x, y = _parse_target(target)
    device = get_device()
    device.click(x, y)


def type_text(target: str, text: str):
    """Type text into an element.

    Args:
        target: Element ref (@e1) to type into.
        text: Text to type.
    """
    # First tap to focus
    tap_target(target)

    # Then type
    device = get_device()

    # Clear existing text first
    device.clear_text()

    # Set text
    device.send_keys(text)


def swipe_direction(direction: str, duration: float = 0.5):
    """Swipe in a direction.

    Args:
        direction: 'up', 'down', 'left', 'right'
        duration: Swipe duration in seconds.
    """
    device = get_device()

    # Get screen dimensions
    info = device.info
    width = info.get("displayWidth", 1080)
    height = info.get("displayHeight", 1920)

    # Calculate swipe coordinates
    cx, cy = width // 2, height // 2
    margin = min(width, height) // 4

    swipes = {
        "up": (cx, cy + margin, cx, cy - margin),
        "down": (cx, cy - margin, cx, cy + margin),
        "left": (cx + margin, cy, cx - margin, cy),
        "right": (cx - margin, cy, cx + margin, cy),
    }

    if direction not in swipes:
        raise ValueError(f"Invalid direction: {direction}. Use up/down/left/right")

    x1, y1, x2, y2 = swipes[direction]
    device.swipe(x1, y1, x2, y2, duration=duration)


def long_press(target: str, duration: float = 1.0):
    """Long press on a target.

    Args:
        target: Element ref (@e1) or coordinates (x,y).
        duration: Press duration in seconds.
    """
    x, y = _parse_target(target)
    device = get_device()
    device.long_click(x, y, duration=duration)


def drag(from_target: str, to_target: str, duration: float = 0.5):
    """Drag from one target to another.

    Args:
        from_target: Start element ref or coordinates.
        to_target: End element ref or coordinates.
        duration: Drag duration in seconds.
    """
    x1, y1 = _parse_target(from_target)
    x2, y2 = _parse_target(to_target)
    device = get_device()
    device.drag(x1, y1, x2, y2, duration=duration)


def scroll_to(
    target: str,
    direction: str = "down",
    max_swipes: int = 10,
) -> bool:
    """Scroll until element is visible.

    Args:
        target: Element ref to find.
        direction: Scroll direction ('up' or 'down').
        max_swipes: Maximum swipes before giving up.

    Returns:
        True if element found, False otherwise.
    """
    device = get_device()

    for _ in range(max_swipes):
        # Check if element is visible
        coords = get_element_center(target)
        if coords:
            return True

        # Swipe to scroll
        swipe_direction(direction)

        # Re-dump hierarchy
        parse_hierarchy(device.dump_hierarchy())

    return False
