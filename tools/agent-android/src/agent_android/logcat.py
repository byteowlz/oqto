"""Logcat streaming and parsing."""

from typing import Optional, Callable
import subprocess
import threading
import time
import re

from .device import get_device


# Log entry pattern: date time PID TID level tag: message
LOG_PATTERN = re.compile(
    r"(\d{2}-\d{2})\s+"  # date
    r"(\d{2}:\d{2}:\d{2}\.\d{3})\s+"  # time
    r"(\d+)\s+"  # PID
    r"(\d+)\s+"  # TID
    r"([VDIWEF])\s+"  # level
    r"(.+?):\s+"  # tag
    r"(.*)"  # message
)


def stream_logs(
    tag: Optional[str] = None,
    level: str = "I",
    package: Optional[str] = None,
    callback: Optional[Callable[[dict], None]] = None,
):
    """Stream logcat in real-time.

    Args:
        tag: Filter by tag (optional)
        level: Minimum log level (V, D, I, W, E)
        package: Filter by package (requires PID lookup)
        callback: Function to call for each log entry.
                  If None, prints to stdout.
    """
    device = get_device()

    # Build logcat command
    cmd = ["adb"]

    # Add device serial if available
    if hasattr(device, "serial"):
        cmd.extend(["-s", device.serial])

    cmd.extend(["logcat", "-v", "threadtime"])

    # Add tag filter
    if tag:
        cmd.extend(["-s", f"{tag}:{level}"])
    else:
        cmd.extend(["*:" + level])

    # Get PID for package filter
    target_pid = None
    if package:
        result = device.shell(f"pidof {package}")
        pid_str = result.output.strip()
        if pid_str:
            target_pid = pid_str.split()[0]

    # Start logcat process
    proc = subprocess.Popen(
        cmd,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )

    try:
        while True:
            line = proc.stdout.readline()
            if not line:
                break

            # Parse log entry
            entry = parse_log_line(line)
            if entry is None:
                continue

            # Filter by PID if package specified
            if target_pid and entry["pid"] != target_pid:
                continue

            if callback:
                callback(entry)
            else:
                # Print formatted
                level_colors = {
                    "V": "\033[90m",  # gray
                    "D": "\033[36m",  # cyan
                    "I": "\033[32m",  # green
                    "W": "\033[33m",  # yellow
                    "E": "\033[31m",  # red
                }
                reset = "\033[0m"
                color = level_colors.get(entry["level"], "")
                print(f"{color}{entry['level']}/{entry['tag']}: {entry['message']}{reset}")

    except KeyboardInterrupt:
        pass
    finally:
        proc.terminate()


def parse_log_line(line: str) -> Optional[dict]:
    """Parse a logcat line into structured data.

    Args:
        line: Raw logcat line

    Returns:
        Dict with date, time, pid, tid, level, tag, message.
        None if line doesn't match pattern.
    """
    match = LOG_PATTERN.match(line.strip())
    if not match:
        return None

    return {
        "date": match.group(1),
        "time": match.group(2),
        "pid": match.group(3),
        "tid": match.group(4),
        "level": match.group(5),
        "tag": match.group(6),
        "message": match.group(7),
    }


def dump_logs(lines: int = 100, tag: Optional[str] = None) -> list[str]:
    """Dump recent logcat entries.

    Args:
        lines: Number of lines to return
        tag: Filter by tag (optional)

    Returns:
        List of log lines.
    """
    device = get_device()

    cmd = f"logcat -d -v threadtime -t {lines}"
    if tag:
        cmd += f" -s {tag}:*"

    result = device.shell(cmd)
    return result.output.strip().split("\n")


def clear_logs():
    """Clear logcat buffer."""
    device = get_device()
    device.shell("logcat -c")


def get_crash_logs(package: str) -> str:
    """Get crash logs for a package.

    Args:
        package: Package name

    Returns:
        Crash log output.
    """
    device = get_device()
    result = device.shell(f"logcat -d -b crash | grep -i {package}")
    return result.output


def watch_for_pattern(
    pattern: str,
    timeout: float = 30.0,
    tag: Optional[str] = None,
) -> Optional[dict]:
    """Watch logcat for a specific pattern.

    Args:
        pattern: Regex pattern to match in message
        timeout: Timeout in seconds
        tag: Filter by tag (optional)

    Returns:
        First matching log entry, or None if timeout.
    """
    result = {"entry": None}
    regex = re.compile(pattern)

    def callback(entry: dict):
        if regex.search(entry["message"]):
            result["entry"] = entry
            raise StopIteration

    # Run in thread with timeout
    def run():
        try:
            stream_logs(tag=tag, callback=callback)
        except StopIteration:
            pass

    thread = threading.Thread(target=run)
    thread.daemon = True
    thread.start()
    thread.join(timeout=timeout)

    return result["entry"]


class LogcatMonitor:
    """Monitor logcat in background and collect entries."""

    def __init__(self, tag: Optional[str] = None, level: str = "I"):
        self.tag = tag
        self.level = level
        self.entries: list[dict] = []
        self._running = False
        self._thread: Optional[threading.Thread] = None

    def start(self):
        """Start monitoring."""
        self._running = True
        self._thread = threading.Thread(target=self._run)
        self._thread.daemon = True
        self._thread.start()

    def stop(self):
        """Stop monitoring."""
        self._running = False
        if self._thread:
            self._thread.join(timeout=1.0)

    def _run(self):
        """Background thread."""

        def callback(entry: dict):
            if not self._running:
                raise StopIteration
            self.entries.append(entry)

        try:
            stream_logs(tag=self.tag, level=self.level, callback=callback)
        except StopIteration:
            pass

    def get_entries(self) -> list[dict]:
        """Get collected entries."""
        return self.entries.copy()

    def clear(self):
        """Clear collected entries."""
        self.entries.clear()

    def find(self, pattern: str) -> list[dict]:
        """Find entries matching pattern."""
        regex = re.compile(pattern)
        return [e for e in self.entries if regex.search(e["message"])]
