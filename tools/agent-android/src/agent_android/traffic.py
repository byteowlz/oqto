"""Network traffic interception via mitmproxy."""

from typing import Optional
import subprocess
import threading
import json
import os

from .device import get_device


# Global mitmproxy process
_mitm_process: Optional[subprocess.Popen] = None
_capture_file: Optional[str] = None


def start_capture(
    port: int = 8080,
    output: str = "traffic.har",
    install_cert: bool = True,
):
    """Start network traffic capture via mitmproxy.

    Args:
        port: Proxy port
        output: Output HAR file path
        install_cert: Install mitmproxy CA cert on device (requires root or user cert)

    Note:
        Device must be configured to use this proxy.
        For Android: Settings > WiFi > Modify network > Proxy > Manual
        Host: <your IP>, Port: <port>
    """
    global _mitm_process, _capture_file

    if _mitm_process is not None:
        raise RuntimeError("Capture already running. Call stop_capture() first.")

    _capture_file = output

    # Start mitmproxy with HAR dump addon
    cmd = [
        "mitmdump",
        "-p",
        str(port),
        "--set",
        f"hardump={output}",
        "-q",  # Quiet mode
    ]

    _mitm_process = subprocess.Popen(
        cmd,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )

    if install_cert:
        _install_ca_cert()

    print(f"mitmproxy running on port {port}")
    print(f"Configure device proxy to this machine's IP:{port}")


def stop_capture():
    """Stop network capture."""
    global _mitm_process

    if _mitm_process:
        _mitm_process.terminate()
        _mitm_process.wait(timeout=5)
        _mitm_process = None


def export_har(output: str):
    """Export captured traffic to HAR file.

    Args:
        output: Output file path
    """
    global _capture_file

    if _capture_file and os.path.exists(_capture_file):
        import shutil

        shutil.copy(_capture_file, output)
    else:
        raise RuntimeError("No capture data available")


def _install_ca_cert():
    """Install mitmproxy CA certificate on device."""
    device = get_device()

    # mitmproxy CA cert location
    cert_path = os.path.expanduser("~/.mitmproxy/mitmproxy-ca-cert.cer")

    if not os.path.exists(cert_path):
        print("mitmproxy CA cert not found. Run 'mitmproxy' once to generate it.")
        return

    # Push to device
    device.push(cert_path, "/sdcard/mitmproxy-ca-cert.cer")
    print("CA cert pushed to /sdcard/mitmproxy-ca-cert.cer")
    print("Install manually: Settings > Security > Install from storage")


def get_network_state() -> dict:
    """Get network state (wifi, mobile, airplane mode).

    Returns:
        Dict with wifi, mobile, airplane status.
    """
    device = get_device()

    # Check WiFi
    wifi_result = device.shell("dumpsys wifi | grep 'Wi-Fi is'")
    wifi_enabled = "enabled" in wifi_result.output.lower()

    # Check mobile data
    mobile_result = device.shell("dumpsys telephony.registry | grep mDataConnectionState")
    mobile_connected = "2" in mobile_result.output  # 2 = connected

    # Check airplane mode
    airplane_result = device.shell("settings get global airplane_mode_on")
    airplane_enabled = airplane_result.output.strip() == "1"

    return {
        "wifi": wifi_enabled,
        "mobile": mobile_connected,
        "airplane": airplane_enabled,
    }


def set_wifi(enabled: bool):
    """Enable/disable WiFi."""
    device = get_device()
    cmd = "enable" if enabled else "disable"
    device.shell(f"svc wifi {cmd}")


def set_mobile_data(enabled: bool):
    """Enable/disable mobile data."""
    device = get_device()
    cmd = "enable" if enabled else "disable"
    device.shell(f"svc data {cmd}")


def set_proxy(host: str, port: int):
    """Set HTTP proxy on device.

    Args:
        host: Proxy host
        port: Proxy port
    """
    device = get_device()
    device.shell(f"settings put global http_proxy {host}:{port}")


def clear_proxy():
    """Clear HTTP proxy setting."""
    device = get_device()
    device.shell("settings put global http_proxy :0")


def get_connections() -> str:
    """Get active network connections.

    Returns:
        netstat output.
    """
    device = get_device()
    result = device.shell("netstat -tuln")
    return result.output


def dns_lookup(hostname: str) -> str:
    """Perform DNS lookup on device.

    Args:
        hostname: Hostname to resolve

    Returns:
        DNS resolution result.
    """
    device = get_device()
    result = device.shell(f"nslookup {hostname}")
    return result.output


class TrafficMonitor:
    """Monitor traffic through mitmproxy programmatically."""

    def __init__(self, port: int = 8080):
        self.port = port
        self.requests: list[dict] = []
        self._running = False
        self._thread: Optional[threading.Thread] = None

    def start(self):
        """Start monitoring."""
        self._running = True

        def run():
            # Use mitmproxy's Python API
            try:
                from mitmproxy import options
                from mitmproxy.tools.dump import DumpMaster

                opts = options.Options(listen_port=self.port)
                master = DumpMaster(opts)

                # Add addon to capture requests
                class CaptureAddon:
                    def __init__(self, monitor):
                        self.monitor = monitor

                    def request(self, flow):
                        self.monitor.requests.append(
                            {
                                "method": flow.request.method,
                                "url": flow.request.url,
                                "headers": dict(flow.request.headers),
                                "timestamp": flow.request.timestamp_start,
                            }
                        )

                    def response(self, flow):
                        if self.monitor.requests:
                            self.monitor.requests[-1]["response"] = {
                                "status": flow.response.status_code,
                                "headers": dict(flow.response.headers),
                            }

                master.addons.add(CaptureAddon(self))
                master.run()

            except ImportError:
                print("mitmproxy not installed")

        self._thread = threading.Thread(target=run)
        self._thread.daemon = True
        self._thread.start()

    def stop(self):
        """Stop monitoring."""
        self._running = False

    def get_requests(self) -> list[dict]:
        """Get captured requests."""
        return self.requests.copy()

    def clear(self):
        """Clear captured requests."""
        self.requests.clear()

    def find(self, url_pattern: str) -> list[dict]:
        """Find requests matching URL pattern."""
        import re

        regex = re.compile(url_pattern)
        return [r for r in self.requests if regex.search(r["url"])]
