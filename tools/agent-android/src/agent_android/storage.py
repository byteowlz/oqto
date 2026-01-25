"""Storage access - SQLite, SharedPreferences, file system."""

from typing import Optional
import tempfile
import os

from .device import get_device


def query_database(db_path: str, sql: str) -> str:
    """Execute SQL query on a device database.

    Args:
        db_path: Path to database on device (e.g., /data/data/com.app/databases/app.db)
        sql: SQL query to execute

    Returns:
        Query results as string.
    """
    device = get_device()

    # Escape quotes in SQL
    sql_escaped = sql.replace('"', '\\"')

    result = device.shell(f'sqlite3 "{db_path}" "{sql_escaped}"')
    return result.output


def list_tables(db_path: str) -> list[str]:
    """List tables in a database.

    Args:
        db_path: Path to database on device

    Returns:
        List of table names.
    """
    result = query_database(db_path, ".tables")
    # Parse table names (space/newline separated)
    tables = []
    for line in result.strip().split("\n"):
        tables.extend(line.split())
    return tables


def get_schema(db_path: str, table: str) -> str:
    """Get schema for a table.

    Args:
        db_path: Path to database on device
        table: Table name

    Returns:
        CREATE TABLE statement.
    """
    return query_database(db_path, f".schema {table}")


def pull_database(db_path: str, local_path: str):
    """Pull database file to local machine.

    Args:
        db_path: Path to database on device
        local_path: Local destination path
    """
    device = get_device()
    device.pull(db_path, local_path)


def push_database(local_path: str, db_path: str):
    """Push local database to device.

    Args:
        local_path: Local database path
        db_path: Destination path on device
    """
    device = get_device()
    device.push(local_path, db_path)


def list_files(path: str) -> list[str]:
    """List files in a directory on device.

    Args:
        path: Directory path on device

    Returns:
        List of file entries (ls -la output).
    """
    device = get_device()
    result = device.shell(f'ls -la "{path}"')
    return result.output.strip().split("\n")


def pull_file(remote_path: str, local_path: str):
    """Pull file from device.

    Args:
        remote_path: Path on device
        local_path: Local destination path
    """
    device = get_device()
    device.pull(remote_path, local_path)


def push_file(local_path: str, remote_path: str):
    """Push file to device.

    Args:
        local_path: Local file path
        remote_path: Destination path on device
    """
    device = get_device()
    device.push(local_path, remote_path)


def cat_file(path: str) -> str:
    """Get contents of a file on device.

    Args:
        path: File path on device

    Returns:
        File contents as string.
    """
    device = get_device()
    result = device.shell(f'cat "{path}"')
    return result.output


def get_prefs(package: str, pref_file: Optional[str] = None) -> str:
    """Get SharedPreferences for a package.

    Args:
        package: Package name (e.g., com.example.app)
        pref_file: Specific prefs file name (optional)

    Returns:
        SharedPreferences XML content.
    """
    device = get_device()

    prefs_dir = f"/data/data/{package}/shared_prefs"

    if pref_file:
        path = f"{prefs_dir}/{pref_file}"
        return cat_file(path)

    # Get all prefs files
    result = device.shell(f'ls "{prefs_dir}"')
    files = result.output.strip().split("\n")

    output = []
    for f in files:
        if f.endswith(".xml"):
            output.append(f"=== {f} ===")
            content = cat_file(f"{prefs_dir}/{f}")
            output.append(content)
            output.append("")

    return "\n".join(output)


def list_prefs(package: str) -> list[str]:
    """List SharedPreferences files for a package.

    Args:
        package: Package name

    Returns:
        List of prefs file names.
    """
    device = get_device()
    prefs_dir = f"/data/data/{package}/shared_prefs"
    result = device.shell(f'ls "{prefs_dir}"')
    return [f for f in result.output.strip().split("\n") if f.endswith(".xml")]


def find_databases(package: str) -> list[str]:
    """Find all databases for a package.

    Args:
        package: Package name

    Returns:
        List of database file paths.
    """
    device = get_device()
    db_dir = f"/data/data/{package}/databases"
    result = device.shell(f'ls "{db_dir}"')
    files = result.output.strip().split("\n")
    return [
        f"{db_dir}/{f}"
        for f in files
        if f and not f.endswith("-journal") and not f.endswith("-wal")
    ]


def dump_app_data(package: str, local_dir: str):
    """Dump all app data to local directory.

    Args:
        package: Package name
        local_dir: Local directory to store data
    """
    device = get_device()

    os.makedirs(local_dir, exist_ok=True)

    # Pull databases
    db_dir = os.path.join(local_dir, "databases")
    os.makedirs(db_dir, exist_ok=True)
    for db_path in find_databases(package):
        db_name = os.path.basename(db_path)
        device.pull(db_path, os.path.join(db_dir, db_name))

    # Pull SharedPreferences
    prefs_dir = os.path.join(local_dir, "shared_prefs")
    os.makedirs(prefs_dir, exist_ok=True)
    for pref in list_prefs(package):
        remote_path = f"/data/data/{package}/shared_prefs/{pref}"
        device.pull(remote_path, os.path.join(prefs_dir, pref))

    # Pull files directory
    files_dir = os.path.join(local_dir, "files")
    os.makedirs(files_dir, exist_ok=True)
    # Note: This may fail for some files due to permissions
    device.shell(f"tar -cf /sdcard/app_files.tar -C /data/data/{package}/files .")
    device.pull("/sdcard/app_files.tar", os.path.join(local_dir, "files.tar"))
