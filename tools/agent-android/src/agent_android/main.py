"""
agent-android: CLI for agent-controlled Android emulator

Commands mirror agent-browser for consistency:
  agent-android start --session <name>
  agent-android snapshot
  agent-android tap @e1
  agent-android type @e2 "hello"
  agent-android db query <path> <sql>
  agent-android content query <uri>
  ...
"""

import typer
from typing import Optional
from rich.console import Console
from rich.table import Table

app = typer.Typer(
    name="agent-android",
    help="Agent-controlled Android emulator for data extraction and UI automation",
    no_args_is_help=True,
)

console = Console()

# Sub-command groups
ui_app = typer.Typer(help="UI/Accessibility commands")
db_app = typer.Typer(help="Database commands")
files_app = typer.Typer(help="File system commands")
prefs_app = typer.Typer(help="SharedPreferences commands")
content_app = typer.Typer(help="Content Provider commands")
intent_app = typer.Typer(help="Intent/broadcast commands")
settings_app = typer.Typer(help="System settings commands")
net_app = typer.Typer(help="Network interception commands")
frida_app = typer.Typer(help="Frida runtime hooks")
log_app = typer.Typer(help="Logcat commands")
emu_app = typer.Typer(help="Emulator lifecycle management")
snapshot_app = typer.Typer(help="Snapshot management")

app.add_typer(ui_app, name="ui")
app.add_typer(db_app, name="db")
app.add_typer(files_app, name="files")
app.add_typer(prefs_app, name="prefs")
app.add_typer(content_app, name="content")
app.add_typer(intent_app, name="intent")
app.add_typer(settings_app, name="settings")
app.add_typer(net_app, name="net")
app.add_typer(frida_app, name="frida")
app.add_typer(log_app, name="log")
app.add_typer(emu_app, name="emu")
app.add_typer(snapshot_app, name="snapshot")


# ============================================================================
# Session Management
# ============================================================================


@app.command()
def start(
    session: str = typer.Option(..., "--session", "-s", help="Session name"),
    device: Optional[str] = typer.Option(None, "--device", "-d", help="Device serial"),
    headed: bool = typer.Option(False, "--headed", help="Show emulator window"),
):
    """Start an Android session (connect to device/emulator)."""
    from .device import DeviceManager

    mgr = DeviceManager()
    dev = mgr.connect(device_serial=device, session_name=session)
    console.print(f"[green]Connected to {dev.serial}[/green]")
    console.print(f"Session: {session}")


@app.command()
def stop(session: str = typer.Option(..., "--session", "-s", help="Session name")):
    """Stop an Android session."""
    from .device import DeviceManager

    mgr = DeviceManager()
    mgr.disconnect(session)
    console.print(f"[yellow]Disconnected session: {session}[/yellow]")


@app.command()
def screenshot(
    output: str = typer.Option("screenshot.png", "--output", "-o", help="Output path"),
):
    """Take a screenshot of the current screen."""
    from .device import get_device

    dev = get_device()
    img = dev.screenshot()
    img.save(output)
    console.print(f"[green]Screenshot saved to {output}[/green]")


@app.command()
def stream(
    port: int = typer.Option(8765, "--port", "-p", help="WebSocket port for streaming"),
):
    """Start screencast streaming via WebSocket."""
    from .stream import start_screencast

    console.print(f"[blue]Starting screencast on ws://localhost:{port}[/blue]")
    start_screencast(port=port)


# ============================================================================
# Basic Actions
# ============================================================================


@app.command()
def tap(
    target: str = typer.Argument(..., help="Element ref (@e1) or coordinates (x,y)"),
):
    """Tap on an element or coordinates."""
    from .actions import tap_target

    tap_target(target)
    console.print(f"[green]Tapped: {target}[/green]")


@app.command()
def type(
    target: str = typer.Argument(..., help="Element ref (@e1) to type into"),
    text: str = typer.Argument(..., help="Text to type"),
):
    """Type text into an element."""
    from .actions import type_text

    type_text(target, text)
    console.print(f"[green]Typed into {target}[/green]")


@app.command()
def swipe(
    direction: str = typer.Argument(..., help="Direction: up, down, left, right"),
    duration: float = typer.Option(0.5, "--duration", "-d", help="Duration in seconds"),
):
    """Swipe in a direction."""
    from .actions import swipe_direction

    swipe_direction(direction, duration)
    console.print(f"[green]Swiped {direction}[/green]")


@app.command()
def back():
    """Press the back button."""
    from .device import get_device

    get_device().press("back")
    console.print("[green]Pressed back[/green]")


@app.command()
def home():
    """Press the home button."""
    from .device import get_device

    get_device().press("home")
    console.print("[green]Pressed home[/green]")


@app.command()
def open_app(
    package: str = typer.Argument(..., help="Package name (e.g., com.example.app)"),
):
    """Open an app by package name."""
    from .device import get_device

    get_device().app_start(package)
    console.print(f"[green]Opened {package}[/green]")


# ============================================================================
# UI Commands
# ============================================================================


@ui_app.command("dump")
def ui_dump(
    format: str = typer.Option("json", "--format", "-f", help="Output format: json, xml"),
):
    """Dump the current UI hierarchy with element refs."""
    from .snapshot import dump_ui

    result = dump_ui(format=format)
    console.print(result)


@ui_app.command("tree")
def ui_tree():
    """Show the UI tree in a readable format."""
    from .snapshot import show_tree

    show_tree()


@ui_app.command("elements")
def ui_elements(
    clickable: bool = typer.Option(False, "--clickable", "-c", help="Only clickable"),
    text: Optional[str] = typer.Option(None, "--text", "-t", help="Filter by text"),
):
    """List UI elements with refs (@e1, @e2, ...)."""
    from .snapshot import list_elements

    elements = list_elements(clickable_only=clickable, text_filter=text)

    table = Table(title="UI Elements")
    table.add_column("Ref", style="cyan")
    table.add_column("Class", style="green")
    table.add_column("Text/Desc")
    table.add_column("Bounds")
    table.add_column("Clickable")

    for el in elements:
        table.add_row(
            el["ref"],
            el["class_name"].split(".")[-1],
            el["text"] or el["content_desc"] or "",
            el["bounds"],
            "Y" if el["clickable"] else "",
        )

    console.print(table)


@ui_app.command("find")
def ui_find(
    text: Optional[str] = typer.Option(None, "--text", "-t", help="Find by text"),
    resource_id: Optional[str] = typer.Option(None, "--id", "-i", help="Find by resource ID"),
    class_name: Optional[str] = typer.Option(None, "--class", "-c", help="Find by class"),
):
    """Find elements matching criteria."""
    from .snapshot import find_elements

    elements = find_elements(text=text, resource_id=resource_id, class_name=class_name)
    for el in elements:
        console.print(
            f"[cyan]{el['ref']}[/cyan] {el['class_name']} - {el['text'] or el['content_desc']}"
        )


# ============================================================================
# Database Commands
# ============================================================================


@db_app.command("query")
def db_query(
    db_path: str = typer.Argument(..., help="Database path on device"),
    sql: str = typer.Argument(..., help="SQL query"),
):
    """Execute SQL query on a database."""
    from .storage import query_database

    result = query_database(db_path, sql)
    console.print(result)


@db_app.command("tables")
def db_tables(db_path: str = typer.Argument(..., help="Database path on device")):
    """List tables in a database."""
    from .storage import list_tables

    tables = list_tables(db_path)
    for t in tables:
        console.print(f"  {t}")


@db_app.command("schema")
def db_schema(
    db_path: str = typer.Argument(..., help="Database path on device"),
    table: str = typer.Argument(..., help="Table name"),
):
    """Show schema for a table."""
    from .storage import get_schema

    schema = get_schema(db_path, table)
    console.print(schema)


@db_app.command("pull")
def db_pull(
    db_path: str = typer.Argument(..., help="Database path on device"),
    output: str = typer.Argument(..., help="Local output path"),
):
    """Pull database file to local machine."""
    from .storage import pull_database

    pull_database(db_path, output)
    console.print(f"[green]Pulled to {output}[/green]")


# ============================================================================
# Files Commands
# ============================================================================


@files_app.command("ls")
def files_ls(path: str = typer.Argument(..., help="Path on device")):
    """List files in a directory."""
    from .storage import list_files

    files = list_files(path)
    for f in files:
        console.print(f)


@files_app.command("pull")
def files_pull(
    remote: str = typer.Argument(..., help="Remote path on device"),
    local: str = typer.Argument(..., help="Local output path"),
):
    """Pull file from device."""
    from .storage import pull_file

    pull_file(remote, local)
    console.print(f"[green]Pulled {remote} to {local}[/green]")


@files_app.command("push")
def files_push(
    local: str = typer.Argument(..., help="Local file path"),
    remote: str = typer.Argument(..., help="Remote path on device"),
):
    """Push file to device."""
    from .storage import push_file

    push_file(local, remote)
    console.print(f"[green]Pushed {local} to {remote}[/green]")


@files_app.command("cat")
def files_cat(path: str = typer.Argument(..., help="File path on device")):
    """Display file contents."""
    from .storage import cat_file

    content = cat_file(path)
    console.print(content)


# ============================================================================
# SharedPreferences Commands
# ============================================================================


@prefs_app.command("get")
def prefs_get(
    package: str = typer.Argument(..., help="Package name"),
    pref_file: Optional[str] = typer.Option(None, "--file", "-f", help="Specific prefs file"),
):
    """Get SharedPreferences for a package."""
    from .storage import get_prefs

    prefs = get_prefs(package, pref_file)
    console.print(prefs)


@prefs_app.command("list")
def prefs_list(package: str = typer.Argument(..., help="Package name")):
    """List SharedPreferences files for a package."""
    from .storage import list_prefs

    files = list_prefs(package)
    for f in files:
        console.print(f)


# ============================================================================
# Content Provider Commands
# ============================================================================


@content_app.command("query")
def content_query(
    uri: str = typer.Argument(..., help="Content URI (e.g., content://contacts/phones)"),
    projection: Optional[str] = typer.Option(None, "--projection", "-p", help="Columns to return"),
    where: Optional[str] = typer.Option(None, "--where", "-w", help="WHERE clause"),
):
    """Query a content provider."""
    from .content import query_provider

    result = query_provider(uri, projection=projection, where=where)
    console.print(result)


@content_app.command("insert")
def content_insert(
    uri: str = typer.Argument(..., help="Content URI"),
    values: str = typer.Argument(..., help="Values as key=value pairs"),
):
    """Insert into a content provider."""
    from .content import insert_provider

    result = insert_provider(uri, values)
    console.print(result)


@content_app.command("delete")
def content_delete(
    uri: str = typer.Argument(..., help="Content URI"),
    where: Optional[str] = typer.Option(None, "--where", "-w", help="WHERE clause"),
):
    """Delete from a content provider."""
    from .content import delete_provider

    result = delete_provider(uri, where=where)
    console.print(result)


# ============================================================================
# Intent Commands
# ============================================================================


@intent_app.command("send")
def intent_send(
    action: str = typer.Argument(..., help="Intent action"),
    data: Optional[str] = typer.Option(None, "--data", "-d", help="Data URI"),
    package: Optional[str] = typer.Option(None, "--package", "-p", help="Target package"),
    component: Optional[str] = typer.Option(None, "--component", "-c", help="Component name"),
    extras: Optional[str] = typer.Option(None, "--extras", "-e", help="Extras as JSON"),
):
    """Send an intent."""
    from .intents import send_intent

    result = send_intent(action, data=data, package=package, component=component, extras=extras)
    console.print(result)


@intent_app.command("broadcast")
def intent_broadcast(
    action: str = typer.Argument(..., help="Broadcast action"),
    extras: Optional[str] = typer.Option(None, "--extras", "-e", help="Extras as JSON"),
):
    """Send a broadcast."""
    from .intents import send_broadcast

    result = send_broadcast(action, extras=extras)
    console.print(result)


@intent_app.command("activity")
def intent_activity():
    """Show current foreground activity."""
    from .intents import get_current_activity

    activity = get_current_activity()
    console.print(f"[cyan]{activity}[/cyan]")


@intent_app.command("stack")
def intent_stack():
    """Show activity stack."""
    from .intents import get_activity_stack

    stack = get_activity_stack()
    for i, act in enumerate(stack):
        console.print(f"  {i}: {act}")


# ============================================================================
# Settings Commands
# ============================================================================


@settings_app.command("get")
def settings_get(
    namespace: str = typer.Argument(..., help="Namespace: secure, global, system"),
    key: str = typer.Argument(..., help="Setting key"),
):
    """Get a system setting."""
    from .settings import get_setting

    value = get_setting(namespace, key)
    console.print(f"{key} = {value}")


@settings_app.command("set")
def settings_set(
    namespace: str = typer.Argument(..., help="Namespace: secure, global, system"),
    key: str = typer.Argument(..., help="Setting key"),
    value: str = typer.Argument(..., help="Setting value"),
):
    """Set a system setting."""
    from .settings import set_setting

    set_setting(namespace, key, value)
    console.print(f"[green]Set {namespace}/{key} = {value}[/green]")


@settings_app.command("list")
def settings_list(namespace: str = typer.Argument(..., help="Namespace: secure, global, system")):
    """List all settings in a namespace."""
    from .settings import list_settings

    settings = list_settings(namespace)
    for k, v in settings.items():
        console.print(f"  {k} = {v}")


# ============================================================================
# Network Commands
# ============================================================================


@net_app.command("capture")
def net_capture(
    port: int = typer.Option(8080, "--port", "-p", help="Proxy port"),
    output: str = typer.Option("traffic.har", "--output", "-o", help="Output HAR file"),
):
    """Start network traffic capture via mitmproxy."""
    from .traffic import start_capture

    console.print(f"[blue]Starting mitmproxy on port {port}[/blue]")
    console.print("[yellow]Configure device proxy to this port[/yellow]")
    start_capture(port=port, output=output)


@net_app.command("stop")
def net_stop():
    """Stop network capture."""
    from .traffic import stop_capture

    stop_capture()
    console.print("[yellow]Stopped capture[/yellow]")


@net_app.command("export")
def net_export(output: str = typer.Argument(..., help="Output HAR file path")):
    """Export captured traffic to HAR."""
    from .traffic import export_har

    export_har(output)
    console.print(f"[green]Exported to {output}[/green]")


@net_app.command("state")
def net_state():
    """Show network state (wifi, mobile, airplane mode)."""
    from .traffic import get_network_state

    state = get_network_state()
    console.print(f"WiFi: {state['wifi']}")
    console.print(f"Mobile: {state['mobile']}")
    console.print(f"Airplane: {state['airplane']}")


# ============================================================================
# Frida Commands
# ============================================================================


@frida_app.command("attach")
def frida_attach(
    package: str = typer.Argument(..., help="Package name to attach to"),
):
    """Attach Frida to a running app."""
    from .frida_hooks import attach_frida

    session = attach_frida(package)
    console.print(f"[green]Attached to {package}[/green]")
    console.print(f"Session ID: {session.id}")


@frida_app.command("hook")
def frida_hook(
    package: str = typer.Argument(..., help="Package name"),
    script: str = typer.Option(..., "--script", "-s", help="Frida script path"),
):
    """Run a Frida hook script."""
    from .frida_hooks import run_script

    run_script(package, script)
    console.print(f"[green]Running script on {package}[/green]")


@frida_app.command("trace")
def frida_trace(
    package: str = typer.Argument(..., help="Package name"),
    classes: str = typer.Option(..., "--classes", "-c", help="Classes to trace (comma-separated)"),
):
    """Trace method calls in specified classes."""
    from .frida_hooks import trace_classes

    trace_classes(package, classes.split(","))


@frida_app.command("bypass-ssl")
def frida_bypass_ssl(package: str = typer.Argument(..., help="Package name")):
    """Bypass SSL pinning for a package."""
    from .frida_hooks import bypass_ssl

    bypass_ssl(package)
    console.print(f"[green]SSL pinning bypassed for {package}[/green]")


# ============================================================================
# Logcat Commands
# ============================================================================


@log_app.command("stream")
def log_stream(
    tag: Optional[str] = typer.Option(None, "--tag", "-t", help="Filter by tag"),
    level: str = typer.Option("I", "--level", "-l", help="Min level: V, D, I, W, E"),
    package: Optional[str] = typer.Option(None, "--package", "-p", help="Filter by package"),
):
    """Stream logcat in real-time."""
    from .logcat import stream_logs

    stream_logs(tag=tag, level=level, package=package)


@log_app.command("dump")
def log_dump(
    lines: int = typer.Option(100, "--lines", "-n", help="Number of lines"),
    tag: Optional[str] = typer.Option(None, "--tag", "-t", help="Filter by tag"),
):
    """Dump recent logcat entries."""
    from .logcat import dump_logs

    logs = dump_logs(lines=lines, tag=tag)
    for line in logs:
        console.print(line)


@log_app.command("clear")
def log_clear():
    """Clear logcat buffer."""
    from .logcat import clear_logs

    clear_logs()
    console.print("[yellow]Logcat cleared[/yellow]")


# ============================================================================
# Info Commands
# ============================================================================


@app.command()
def info():
    """Show device information."""
    from .device import get_device

    dev = get_device()
    info = dev.info

    table = Table(title="Device Info")
    table.add_column("Property", style="cyan")
    table.add_column("Value")

    for key in ["productName", "model", "brand", "sdkInt", "displayWidth", "displayHeight"]:
        if key in info:
            table.add_row(key, str(info[key]))

    console.print(table)


@app.command()
def packages(
    filter: Optional[str] = typer.Option(None, "--filter", "-f", help="Filter by name"),
):
    """List installed packages."""
    from .device import get_device

    dev = get_device()
    pkgs = dev.shell("pm list packages").output.strip().split("\n")

    for pkg in pkgs:
        name = pkg.replace("package:", "")
        if filter is None or filter.lower() in name.lower():
            console.print(name)


# ============================================================================
# Emulator Lifecycle Commands
# ============================================================================


@emu_app.command("create")
def emu_create(
    session: str = typer.Argument(..., help="Session name"),
    from_template: Optional[str] = typer.Option(
        None, "--from", "-f", help="Copy from session or 'base'"
    ),
    system_image: str = typer.Option(
        "system-images;android-34;google_apis;x86_64", "--image", "-i", help="System image package"
    ),
    device: str = typer.Option("pixel_7", "--device", "-d", help="Device profile"),
):
    """Create a new persistent emulator session."""
    from .emulator import get_manager, EmulatorConfig

    mgr = get_manager()
    config = EmulatorConfig(system_image=system_image, device_profile=device)

    session_dir = mgr.create_session(session, config, from_template=from_template)
    console.print(f"[green]Created session: {session}[/green]")
    console.print(f"  Path: {session_dir}")
    console.print(f"  System: {system_image}")
    console.print(f"  Device: {device}")

    if from_template:
        console.print(f"  Copied data from: {from_template}")


@emu_app.command("start")
def emu_start(
    session: str = typer.Argument(..., help="Session name"),
    headed: bool = typer.Option(False, "--headed", help="Show emulator window"),
    memory: int = typer.Option(4096, "--memory", "-m", help="Memory in MB"),
    cores: int = typer.Option(4, "--cores", "-c", help="CPU cores"),
    no_wait: bool = typer.Option(False, "--no-wait", help="Don't wait for boot"),
):
    """Start an emulator session (creates if doesn't exist)."""
    from .emulator import get_manager, EmulatorConfig

    mgr = get_manager()
    config = EmulatorConfig(
        headless=not headed,
        memory_mb=memory,
        cores=cores,
    )

    console.print(f"[blue]Starting emulator for session '{session}'...[/blue]")
    serial = mgr.start(session, config, wait_boot=not no_wait)

    console.print(f"[green]Emulator running: {serial}[/green]")
    console.print(f"  Connect with: adb -s {serial} shell")


@emu_app.command("stop")
def emu_stop(
    session: str = typer.Argument(..., help="Session name"),
    no_save: bool = typer.Option(False, "--no-save", help="Don't save snapshot on exit"),
):
    """Stop an emulator session (state persists)."""
    from .emulator import get_manager

    mgr = get_manager()
    mgr.stop(session, save_snapshot=not no_save)
    console.print(f"[yellow]Stopped session: {session}[/yellow]")
    console.print("  App data and state have been preserved.")


@emu_app.command("restart")
def emu_restart(
    session: str = typer.Argument(..., help="Session name"),
    headed: bool = typer.Option(False, "--headed", help="Show emulator window"),
):
    """Restart an emulator session."""
    from .emulator import get_manager, EmulatorConfig

    mgr = get_manager()
    config = EmulatorConfig(headless=not headed)

    console.print(f"[blue]Restarting session '{session}'...[/blue]")
    serial = mgr.restart(session, config)
    console.print(f"[green]Emulator running: {serial}[/green]")


@emu_app.command("list")
def emu_list():
    """List all sessions."""
    from .emulator import get_manager

    mgr = get_manager()
    sessions = mgr.list_sessions()

    if not sessions:
        console.print("[dim]No sessions found[/dim]")
        return

    table = Table(title="Emulator Sessions")
    table.add_column("Session", style="cyan")
    table.add_column("Created")
    table.add_column("Last Accessed")
    table.add_column("Packages")
    table.add_column("Snapshots")
    table.add_column("Running", style="green")

    for s in sessions:
        running = "Yes" if mgr.is_running(s.session_id) else ""
        table.add_row(
            s.session_id,
            s.created_at[:10],
            s.last_accessed[:10],
            str(len(s.installed_packages)),
            str(len(s.snapshots)),
            running,
        )

    console.print(table)


@emu_app.command("delete")
def emu_delete(
    session: str = typer.Argument(..., help="Session name"),
    force: bool = typer.Option(False, "--force", "-f", help="Force delete even if running"),
):
    """Delete a session and all its data."""
    from .emulator import get_manager

    mgr = get_manager()

    if not force:
        confirm = typer.confirm(f"Delete session '{session}' and all its data?")
        if not confirm:
            raise typer.Abort()

    mgr.delete_session(session, force=force)
    console.print(f"[red]Deleted session: {session}[/red]")


@emu_app.command("status")
def emu_status(session: str = typer.Argument(..., help="Session name")):
    """Show status of a session."""
    from .emulator import get_manager

    mgr = get_manager()
    session_dir = mgr.get_session(session)

    if not session_dir:
        console.print(f"[red]Session '{session}' not found[/red]")
        raise typer.Exit(1)

    metadata = mgr._load_metadata(session_dir)
    running = mgr.is_running(session)
    serial = mgr.get_serial(session)

    console.print(f"[bold]Session: {session}[/bold]")
    console.print(f"  Status: {'[green]Running[/green]' if running else '[dim]Stopped[/dim]'}")
    if serial:
        console.print(f"  Serial: {serial}")

    if metadata:
        console.print(f"  Created: {metadata.created_at}")
        console.print(f"  Last accessed: {metadata.last_accessed}")
        console.print(f"  System image: {metadata.system_image}")

        if metadata.installed_packages:
            console.print(f"  Installed packages ({len(metadata.installed_packages)}):")
            for pkg in metadata.installed_packages[:10]:
                console.print(f"    - {pkg}")
            if len(metadata.installed_packages) > 10:
                console.print(f"    ... and {len(metadata.installed_packages) - 10} more")

        if metadata.snapshots:
            console.print(f"  Snapshots: {', '.join(metadata.snapshots)}")


@emu_app.command("install")
def emu_install(
    session: str = typer.Argument(..., help="Session name"),
    apk: str = typer.Argument(..., help="Path to APK file"),
):
    """Install an APK to a running session."""
    from .emulator import get_manager

    mgr = get_manager()

    if not mgr.is_running(session):
        console.print(f"[red]Session '{session}' is not running. Start it first.[/red]")
        raise typer.Exit(1)

    console.print(f"[blue]Installing {apk}...[/blue]")
    if mgr.install_apk(session, apk):
        console.print(f"[green]Installed successfully[/green]")
    else:
        console.print(f"[red]Installation failed[/red]")
        raise typer.Exit(1)


# ============================================================================
# Snapshot Commands
# ============================================================================


@snapshot_app.command("save")
def snapshot_save(
    session: str = typer.Argument(..., help="Session name"),
    name: str = typer.Argument(..., help="Snapshot name"),
):
    """Save a named snapshot of the current state."""
    from .emulator import get_manager

    mgr = get_manager()

    if not mgr.is_running(session):
        console.print(f"[red]Session '{session}' is not running[/red]")
        raise typer.Exit(1)

    console.print(f"[blue]Saving snapshot '{name}'...[/blue]")
    if mgr.save_snapshot(session, name):
        console.print(f"[green]Snapshot saved: {name}[/green]")
    else:
        console.print(f"[red]Failed to save snapshot[/red]")
        raise typer.Exit(1)


@snapshot_app.command("load")
def snapshot_load(
    session: str = typer.Argument(..., help="Session name"),
    name: str = typer.Argument(..., help="Snapshot name"),
):
    """Load a named snapshot."""
    from .emulator import get_manager

    mgr = get_manager()

    if not mgr.is_running(session):
        console.print(f"[red]Session '{session}' is not running[/red]")
        raise typer.Exit(1)

    console.print(f"[blue]Loading snapshot '{name}'...[/blue]")
    if mgr.load_snapshot(session, name):
        console.print(f"[green]Snapshot loaded: {name}[/green]")
    else:
        console.print(f"[red]Failed to load snapshot[/red]")
        raise typer.Exit(1)


@snapshot_app.command("list")
def snapshot_list(session: str = typer.Argument(..., help="Session name")):
    """List available snapshots for a session."""
    from .emulator import get_manager

    mgr = get_manager()
    snapshots = mgr.list_snapshots(session)

    if not snapshots:
        console.print("[dim]No snapshots found[/dim]")
        return

    console.print(f"[bold]Snapshots for '{session}':[/bold]")
    for snap in snapshots:
        console.print(f"  - {snap}")


@snapshot_app.command("delete")
def snapshot_delete(
    session: str = typer.Argument(..., help="Session name"),
    name: str = typer.Argument(..., help="Snapshot name"),
):
    """Delete a snapshot."""
    from .emulator import get_manager

    mgr = get_manager()

    if not mgr.is_running(session):
        console.print(f"[red]Session '{session}' is not running[/red]")
        raise typer.Exit(1)

    if mgr.delete_snapshot(session, name):
        console.print(f"[yellow]Deleted snapshot: {name}[/yellow]")
    else:
        console.print(f"[red]Failed to delete snapshot[/red]")
        raise typer.Exit(1)


if __name__ == "__main__":
    app()
