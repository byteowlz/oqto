"""UI snapshot and element extraction - full accessibility info."""

from typing import Optional
from dataclasses import dataclass, field
from xml.etree import ElementTree
import json

from .device import get_device


@dataclass
class UIElement:
    """Represents a UI element with all accessibility properties."""

    ref: str  # @e1, @e2, etc.
    index: int

    # Identity
    class_name: str
    package: str
    resource_id: str

    # Content
    text: str
    content_desc: str
    hint_text: str
    tooltip_text: str

    # Bounds
    bounds: str  # "[x1,y1][x2,y2]"
    bounds_rect: tuple[int, int, int, int]  # (left, top, right, bottom)

    # State flags
    checkable: bool
    checked: bool
    clickable: bool
    enabled: bool
    focusable: bool
    focused: bool
    long_clickable: bool
    scrollable: bool
    selected: bool
    password: bool

    # Hierarchy
    depth: int
    child_count: int
    children: list["UIElement"] = field(default_factory=list)


# Element refs cache for current snapshot
_element_cache: dict[str, UIElement] = {}


def dump_ui(format: str = "json") -> str:
    """Dump the current UI hierarchy.

    Args:
        format: Output format - 'json' or 'xml'

    Returns:
        UI hierarchy as string in requested format.
    """
    device = get_device()

    if format == "xml":
        return device.dump_hierarchy()

    # Parse to structured JSON with refs
    xml = device.dump_hierarchy()
    elements = parse_hierarchy(xml)
    return json.dumps([_element_to_dict(e) for e in elements], indent=2)


def parse_hierarchy(xml_str: str) -> list[UIElement]:
    """Parse UI hierarchy XML into UIElement objects.

    Args:
        xml_str: Raw XML from uiautomator dump

    Returns:
        List of UIElement objects with refs assigned.
    """
    global _element_cache
    _element_cache = {}

    root = ElementTree.fromstring(xml_str)
    elements: list[UIElement] = []
    index = 0

    def parse_node(node: ElementTree.Element, depth: int = 0) -> Optional[UIElement]:
        nonlocal index

        # Extract bounds
        bounds_str = node.get("bounds", "[0,0][0,0]")
        bounds_rect = _parse_bounds(bounds_str)

        element = UIElement(
            ref=f"@e{index}",
            index=index,
            class_name=node.get("class", ""),
            package=node.get("package", ""),
            resource_id=node.get("resource-id", ""),
            text=node.get("text", ""),
            content_desc=node.get("content-desc", ""),
            hint_text=node.get("hint-text", ""),
            tooltip_text=node.get("tooltip-text", ""),
            bounds=bounds_str,
            bounds_rect=bounds_rect,
            checkable=node.get("checkable", "false") == "true",
            checked=node.get("checked", "false") == "true",
            clickable=node.get("clickable", "false") == "true",
            enabled=node.get("enabled", "true") == "true",
            focusable=node.get("focusable", "false") == "true",
            focused=node.get("focused", "false") == "true",
            long_clickable=node.get("long-clickable", "false") == "true",
            scrollable=node.get("scrollable", "false") == "true",
            selected=node.get("selected", "false") == "true",
            password=node.get("password", "false") == "true",
            depth=depth,
            child_count=len(node),
            children=[],
        )

        _element_cache[element.ref] = element
        elements.append(element)
        index += 1

        # Parse children
        for child_node in node:
            child = parse_node(child_node, depth + 1)
            if child:
                element.children.append(child)

        return element

    # Parse from root
    for child in root:
        parse_node(child)

    return elements


def _parse_bounds(bounds_str: str) -> tuple[int, int, int, int]:
    """Parse bounds string like '[0,0][100,200]' to (left, top, right, bottom)."""
    try:
        # Remove brackets and split
        parts = bounds_str.replace("][", ",").strip("[]").split(",")
        return (int(parts[0]), int(parts[1]), int(parts[2]), int(parts[3]))
    except (ValueError, IndexError):
        return (0, 0, 0, 0)


def _element_to_dict(el: UIElement) -> dict:
    """Convert UIElement to dictionary for JSON serialization."""
    return {
        "ref": el.ref,
        "class": el.class_name,
        "package": el.package,
        "resource_id": el.resource_id,
        "text": el.text,
        "content_desc": el.content_desc,
        "hint_text": el.hint_text,
        "bounds": el.bounds,
        "bounds_rect": list(el.bounds_rect),
        "clickable": el.clickable,
        "long_clickable": el.long_clickable,
        "checkable": el.checkable,
        "checked": el.checked,
        "scrollable": el.scrollable,
        "enabled": el.enabled,
        "focusable": el.focusable,
        "focused": el.focused,
        "selected": el.selected,
        "password": el.password,
        "depth": el.depth,
        "child_count": el.child_count,
    }


def show_tree():
    """Display UI tree in a readable format."""
    from rich.tree import Tree
    from rich.console import Console

    device = get_device()
    xml = device.dump_hierarchy()
    elements = parse_hierarchy(xml)

    console = Console()

    def build_tree(el: UIElement, tree: Tree):
        # Format node label
        label = f"[cyan]{el.ref}[/cyan] {el.class_name.split('.')[-1]}"
        if el.text:
            label += f' "{el.text}"'
        elif el.content_desc:
            label += f" ({el.content_desc})"
        if el.clickable:
            label += " [green][clickable][/green]"
        if el.scrollable:
            label += " [yellow][scrollable][/yellow]"

        branch = tree.add(label)
        for child in el.children:
            build_tree(child, branch)

    # Find root elements (depth=0)
    roots = [e for e in elements if e.depth == 0]

    tree = Tree("[bold]UI Hierarchy[/bold]")
    for root in roots:
        build_tree(root, tree)

    console.print(tree)


def list_elements(
    clickable_only: bool = False,
    text_filter: Optional[str] = None,
) -> list[dict]:
    """List UI elements with filtering.

    Args:
        clickable_only: Only return clickable elements.
        text_filter: Filter by text content (case-insensitive).

    Returns:
        List of element dicts.
    """
    device = get_device()
    xml = device.dump_hierarchy()
    elements = parse_hierarchy(xml)

    result = []
    for el in elements:
        # Apply filters
        if clickable_only and not el.clickable:
            continue
        if text_filter:
            text_lower = text_filter.lower()
            if (
                text_lower not in (el.text or "").lower()
                and text_lower not in (el.content_desc or "").lower()
            ):
                continue

        result.append(_element_to_dict(el))

    return result


def find_elements(
    text: Optional[str] = None,
    resource_id: Optional[str] = None,
    class_name: Optional[str] = None,
) -> list[dict]:
    """Find elements matching criteria.

    Args:
        text: Match by text (partial, case-insensitive).
        resource_id: Match by resource ID (partial).
        class_name: Match by class name (partial).

    Returns:
        List of matching element dicts.
    """
    device = get_device()
    xml = device.dump_hierarchy()
    elements = parse_hierarchy(xml)

    result = []
    for el in elements:
        if text and text.lower() not in (el.text or "").lower():
            continue
        if resource_id and resource_id.lower() not in (el.resource_id or "").lower():
            continue
        if class_name and class_name.lower() not in (el.class_name or "").lower():
            continue

        result.append(_element_to_dict(el))

    return result


def get_element(ref: str) -> Optional[UIElement]:
    """Get a cached element by ref.

    Args:
        ref: Element reference like '@e1'

    Returns:
        UIElement if found, None otherwise.
    """
    return _element_cache.get(ref)


def get_element_center(ref: str) -> Optional[tuple[int, int]]:
    """Get the center coordinates of an element.

    Args:
        ref: Element reference like '@e1'

    Returns:
        (x, y) center coordinates, or None if not found.
    """
    el = get_element(ref)
    if not el:
        return None

    left, top, right, bottom = el.bounds_rect
    return ((left + right) // 2, (top + bottom) // 2)
