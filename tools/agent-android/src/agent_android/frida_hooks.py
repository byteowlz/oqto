"""Frida runtime instrumentation."""

from typing import Optional, Callable
import os

# Frida is optional
try:
    import frida

    FRIDA_AVAILABLE = True
except ImportError:
    FRIDA_AVAILABLE = False


def _check_frida():
    if not FRIDA_AVAILABLE:
        raise RuntimeError("frida not installed. Run: pip install frida-tools")


# Active sessions
_sessions: dict[str, "frida.core.Session"] = {}


def attach_frida(package: str, device_id: Optional[str] = None) -> "frida.core.Session":
    """Attach Frida to a running app.

    Args:
        package: Package name to attach to
        device_id: Device ID (optional, uses first USB device if not specified)

    Returns:
        Frida session.
    """
    _check_frida()

    if device_id:
        device = frida.get_device(device_id)
    else:
        device = frida.get_usb_device()

    # Find the process
    try:
        session = device.attach(package)
    except frida.ProcessNotFoundError:
        # Try to spawn the process
        pid = device.spawn([package])
        session = device.attach(pid)
        device.resume(pid)

    _sessions[package] = session
    return session


def detach_frida(package: str):
    """Detach Frida from an app.

    Args:
        package: Package name
    """
    if package in _sessions:
        _sessions[package].detach()
        del _sessions[package]


def run_script(
    package: str,
    script_path: str,
    on_message: Optional[Callable[[dict, bytes], None]] = None,
):
    """Run a Frida script on an app.

    Args:
        package: Package name
        script_path: Path to JavaScript file
        on_message: Callback for messages from script
    """
    _check_frida()

    if package not in _sessions:
        attach_frida(package)

    session = _sessions[package]

    with open(script_path) as f:
        script_code = f.read()

    script = session.create_script(script_code)

    if on_message:
        script.on("message", on_message)
    else:
        # Default message handler
        def default_handler(message, data):
            if message["type"] == "send":
                print(f"[*] {message['payload']}")
            elif message["type"] == "error":
                print(f"[!] {message['stack']}")

        script.on("message", default_handler)

    script.load()
    return script


def trace_classes(package: str, classes: list[str]):
    """Trace method calls in specified classes.

    Args:
        package: Package name
        classes: List of class names to trace
    """
    _check_frida()

    class_patterns = [f'"{c}"' for c in classes]
    classes_str = ", ".join(class_patterns)

    script_code = f"""
    var classes = [{classes_str}];

    Java.perform(function() {{
        classes.forEach(function(className) {{
            try {{
                var clazz = Java.use(className);
                var methods = clazz.class.getDeclaredMethods();

                methods.forEach(function(method) {{
                    var methodName = method.getName();
                    var overloads = clazz[methodName].overloads;

                    overloads.forEach(function(overload) {{
                        overload.implementation = function() {{
                            var args = Array.prototype.slice.call(arguments);
                            send({{
                                type: "call",
                                class: className,
                                method: methodName,
                                args: args.map(function(a) {{ return String(a); }})
                            }});
                            return this[methodName].apply(this, arguments);
                        }};
                    }});
                }});
                send({{type: "hooked", class: className}});
            }} catch(e) {{
                send({{type: "error", class: className, error: String(e)}});
            }}
        }});
    }});
    """

    if package not in _sessions:
        attach_frida(package)

    session = _sessions[package]
    script = session.create_script(script_code)

    def on_message(message, data):
        if message["type"] == "send":
            payload = message["payload"]
            if payload.get("type") == "call":
                print(
                    f"[CALL] {payload['class']}.{payload['method']}({', '.join(payload['args'])})"
                )
            elif payload.get("type") == "hooked":
                print(f"[+] Hooked {payload['class']}")
            elif payload.get("type") == "error":
                print(f"[-] Failed to hook {payload['class']}: {payload['error']}")

    script.on("message", on_message)
    script.load()

    print(f"Tracing {len(classes)} classes. Press Ctrl+C to stop.")
    import sys

    sys.stdin.read()


def bypass_ssl(package: str):
    """Bypass SSL pinning for a package.

    Args:
        package: Package name
    """
    _check_frida()

    # Universal SSL bypass script
    script_code = """
    Java.perform(function() {
        // TrustManager bypass
        var TrustManager = Java.registerClass({
            name: 'com.sensepost.frida.TrustManager',
            implements: [Java.use('javax.net.ssl.X509TrustManager')],
            methods: {
                checkClientTrusted: function(chain, authType) {},
                checkServerTrusted: function(chain, authType) {},
                getAcceptedIssuers: function() { return []; }
            }
        });

        var TrustManagers = [TrustManager.$new()];

        var SSLContext = Java.use('javax.net.ssl.SSLContext');
        var SSLContext_init = SSLContext.init.overload(
            '[Ljavax.net.ssl.KeyManager;',
            '[Ljavax.net.ssl.TrustManager;',
            'java.security.SecureRandom'
        );

        SSLContext_init.implementation = function(keyManager, trustManager, secureRandom) {
            send({type: 'bypass', target: 'SSLContext.init'});
            SSLContext_init.call(this, keyManager, TrustManagers, secureRandom);
        };

        // HostnameVerifier bypass
        var HostnameVerifier = Java.registerClass({
            name: 'com.sensepost.frida.HostnameVerifier',
            implements: [Java.use('javax.net.ssl.HostnameVerifier')],
            methods: {
                verify: function(hostname, session) {
                    return true;
                }
            }
        });

        var HttpsURLConnection = Java.use('javax.net.ssl.HttpsURLConnection');
        HttpsURLConnection.setDefaultHostnameVerifier.implementation = function(hostnameVerifier) {
            send({type: 'bypass', target: 'HttpsURLConnection.setDefaultHostnameVerifier'});
            return this.setDefaultHostnameVerifier(HostnameVerifier.$new());
        };

        HttpsURLConnection.setHostnameVerifier.implementation = function(hostnameVerifier) {
            send({type: 'bypass', target: 'HttpsURLConnection.setHostnameVerifier'});
            return this.setHostnameVerifier(HostnameVerifier.$new());
        };

        // OkHttp CertificatePinner bypass (if present)
        try {
            var CertificatePinner = Java.use('okhttp3.CertificatePinner');
            CertificatePinner.check.overload('java.lang.String', 'java.util.List').implementation = function(hostname, peerCertificates) {
                send({type: 'bypass', target: 'OkHttp CertificatePinner'});
                return;
            };
        } catch(e) {}

        send({type: 'ready', message: 'SSL pinning bypass active'});
    });
    """

    if package not in _sessions:
        attach_frida(package)

    session = _sessions[package]
    script = session.create_script(script_code)

    def on_message(message, data):
        if message["type"] == "send":
            payload = message["payload"]
            if payload.get("type") == "bypass":
                print(f"[*] Bypassed: {payload['target']}")
            elif payload.get("type") == "ready":
                print(f"[+] {payload['message']}")

    script.on("message", on_message)
    script.load()

    return script


def hook_method(
    package: str,
    class_name: str,
    method_name: str,
    callback: Optional[Callable] = None,
):
    """Hook a specific method.

    Args:
        package: Package name
        class_name: Full class name
        method_name: Method name
        callback: Optional callback for when method is called
    """
    _check_frida()

    script_code = f"""
    Java.perform(function() {{
        var clazz = Java.use('{class_name}');
        var overloads = clazz['{method_name}'].overloads;

        overloads.forEach(function(overload) {{
            overload.implementation = function() {{
                var args = Array.prototype.slice.call(arguments);
                var result = this['{method_name}'].apply(this, arguments);

                send({{
                    type: 'call',
                    class: '{class_name}',
                    method: '{method_name}',
                    args: args.map(function(a) {{ return String(a); }}),
                    result: String(result)
                }});

                return result;
            }};
        }});

        send({{type: 'hooked', class: '{class_name}', method: '{method_name}'}});
    }});
    """

    if package not in _sessions:
        attach_frida(package)

    session = _sessions[package]
    script = session.create_script(script_code)

    def on_message(message, data):
        if message["type"] == "send":
            payload = message["payload"]
            if payload.get("type") == "call":
                print(f"[CALL] {payload['class']}.{payload['method']}")
                print(f"  Args: {payload['args']}")
                print(f"  Result: {payload['result']}")
                if callback:
                    callback(payload)

    script.on("message", on_message)
    script.load()

    return script


# Pre-built hook scripts directory
SCRIPTS_DIR = os.path.join(os.path.dirname(__file__), "..", "..", "scripts", "frida")


def list_scripts() -> list[str]:
    """List available Frida scripts."""
    if os.path.exists(SCRIPTS_DIR):
        return [f for f in os.listdir(SCRIPTS_DIR) if f.endswith(".js")]
    return []


def run_builtin_script(package: str, script_name: str):
    """Run a built-in Frida script.

    Args:
        package: Package name
        script_name: Script name (e.g., 'crypto.js')
    """
    script_path = os.path.join(SCRIPTS_DIR, script_name)
    if not os.path.exists(script_path):
        raise FileNotFoundError(f"Script not found: {script_name}")

    return run_script(package, script_path)
