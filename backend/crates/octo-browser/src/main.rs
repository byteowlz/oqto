use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use clap::{Parser, Subcommand};
use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

#[cfg(unix)]
use std::os::unix::net::UnixStream;

#[derive(Parser)]
#[command(name = "octo-browser", version, about = "CLI for agent-browser daemon")]
struct Cli {
    /// Session ID to target
    #[arg(long, env = "AGENT_BROWSER_SESSION", default_value = "default")]
    session: String,

    /// Connection timeout in seconds
    #[arg(long, default_value_t = 5)]
    timeout_secs: u64,

    /// Output raw JSON instead of clean text
    #[arg(long)]
    json: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Navigate to a URL (auto-launches browser)
    Open {
        url: String,
        #[arg(long)]
        wait_until: Option<WaitUntil>,
    },
    /// Navigate to a URL (alias of open)
    Navigate {
        url: String,
        #[arg(long)]
        wait_until: Option<WaitUntil>,
    },
    /// Click an element
    Click {
        selector: String,
        #[arg(long, value_enum, default_value = "left")]
        button: MouseButton,
        #[arg(long)]
        click_count: Option<u32>,
        #[arg(long)]
        delay_ms: Option<u32>,
    },
    /// Fill an input element
    Fill { selector: String, value: String },
    /// Type into an element
    Type {
        selector: String,
        text: String,
        #[arg(long)]
        delay_ms: Option<u32>,
        #[arg(long)]
        clear: bool,
    },
    /// Capture a semantic snapshot
    Snapshot {
        #[arg(short, long)]
        interactive: bool,
        #[arg(long)]
        max_depth: Option<u32>,
        #[arg(long)]
        compact: bool,
        #[arg(long)]
        selector: Option<String>,
        /// Include cursor-interactive elements (cursor:pointer, onclick)
        #[arg(short, long)]
        cursor: bool,
    },
    /// Evaluate a script in the page
    Eval { script: String },
    /// Wait for a selector or timeout (e.g. "wait #el" or "wait 3000")
    Wait {
        /// CSS selector or milliseconds
        target: Option<String>,
        #[arg(long)]
        timeout_ms: Option<u64>,
        #[arg(long)]
        state: Option<WaitState>,
    },
    /// Take a screenshot
    Screenshot {
        /// File path to save the screenshot (positional or --path)
        path: Option<String>,
        /// Capture full page
        #[arg(short = 'f', long)]
        full_page: bool,
        /// Element selector to screenshot
        #[arg(long)]
        selector: Option<String>,
        /// Image format (png or jpeg)
        #[arg(long, default_value = "png")]
        format: String,
    },
    /// Go back in browser history
    Back,
    /// Go forward in browser history
    Forward,
    /// Reload the current page
    Reload,
    /// Get current page URL
    Url,
    /// Get current page title
    Title,
    /// Get page console log messages
    Console {
        /// Clear console messages instead of reading
        #[arg(long)]
        clear: bool,
    },
    /// Get page errors
    Errors {
        /// Clear errors instead of reading
        #[arg(long)]
        clear: bool,
    },
    /// Press a key (Enter, Tab, Control+a, etc.)
    Press { key: String },
    /// Double-click an element
    Dblclick {
        selector: String,
        #[arg(long)]
        delay_ms: Option<u32>,
    },
    /// Hover over an element
    Hover { selector: String },
    /// Focus an element
    Focus { selector: String },
    /// Check a checkbox
    Check { selector: String },
    /// Uncheck a checkbox
    Uncheck { selector: String },
    /// Select a dropdown option
    Select {
        selector: String,
        /// Values to select
        values: Vec<String>,
    },
    /// Drag from one element to another
    Drag { source: String, target: String },
    /// Upload files to a file input
    Upload {
        selector: String,
        /// File paths to upload
        files: Vec<String>,
    },
    /// Download a file by clicking element
    Download {
        selector: String,
        /// Path to save the file
        path: String,
    },
    /// Scroll the page (up/down/left/right) by pixels
    Scroll {
        /// Direction: up, down, left, right
        direction: String,
        /// Pixels to scroll (default: 500)
        amount: Option<i32>,
        /// Selector to scroll within
        #[arg(long)]
        selector: Option<String>,
    },
    /// Scroll element into view
    Scrollintoview { selector: String },
    /// Highlight an element (for debugging)
    Highlight { selector: String },
    /// Save page as PDF
    Pdf { path: String },
    /// Get page HTML content
    Content,
    /// Close the browser daemon
    Close,
    /// Install Playwright browsers (chromium, firefox, webkit, or all)
    Install {
        /// Browser to install (chromium, firefox, webkit). Omit for all.
        browser: Option<String>,
        /// Install system dependencies (requires sudo)
        #[arg(long)]
        deps: bool,
    },
    /// Send a raw JSON command to the daemon
    Raw { json: String },
    /// Send any agent-browser action with JSON or key=value args
    Generic {
        action: String,
        /// JSON object to merge into the command payload
        #[arg(long)]
        json: Option<String>,
        /// JSON file to merge into the command payload
        #[arg(long)]
        file: Option<PathBuf>,
        /// Key=value pairs to merge into the payload (top-level)
        #[arg(long)]
        arg: Vec<String>,
    },
}

#[derive(clap::ValueEnum, Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
enum MouseButton {
    Left,
    Right,
    Middle,
}

#[derive(clap::ValueEnum, Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
enum WaitUntil {
    Load,
    DomContentLoaded,
    NetworkIdle,
}

#[derive(clap::ValueEnum, Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
enum WaitState {
    Attached,
    Detached,
    Visible,
    Hidden,
}

#[derive(Serialize)]
struct CommandPayload<T: Serialize> {
    id: String,
    #[serde(flatten)]
    payload: T,
}

#[derive(Serialize)]
struct ActionPayload<'a, T: Serialize> {
    action: &'a str,
    #[serde(flatten)]
    data: T,
}

#[derive(Serialize)]
struct NavigatePayload {
    url: String,
    #[serde(rename = "waitUntil", skip_serializing_if = "Option::is_none")]
    wait_until: Option<WaitUntil>,
}

#[derive(Serialize)]
struct ClickPayload {
    selector: String,
    button: MouseButton,
    #[serde(rename = "clickCount", skip_serializing_if = "Option::is_none")]
    click_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    delay: Option<u32>,
}

#[derive(Serialize)]
struct FillPayload {
    selector: String,
    value: String,
}

#[derive(Serialize)]
struct TypePayload {
    selector: String,
    text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    delay: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    clear: Option<bool>,
}

#[derive(Serialize)]
struct EvalPayload {
    script: String,
}

#[derive(Serialize)]
struct WaitPayload {
    #[serde(skip_serializing_if = "Option::is_none")]
    selector: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    timeout: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    state: Option<WaitState>,
}

#[derive(serde::Deserialize, serde::Serialize)]
struct ResponsePayload {
    id: String,
    success: bool,
    data: Option<Value>,
    error: Option<String>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let timeout = Duration::from_secs(cli.timeout_secs);
    let output_json = cli.json;

    let response = match cli.command {
        Command::Open { url, wait_until } | Command::Navigate { url, wait_until } => send_command(
            &cli.session,
            timeout,
            ActionPayload {
                action: "navigate",
                data: NavigatePayload { url, wait_until },
            },
        )?,
        Command::Click {
            selector,
            button,
            click_count,
            delay_ms,
        } => send_command(
            &cli.session,
            timeout,
            ActionPayload {
                action: "click",
                data: ClickPayload {
                    selector,
                    button,
                    click_count,
                    delay: delay_ms,
                },
            },
        )?,
        Command::Fill { selector, value } => send_command(
            &cli.session,
            timeout,
            ActionPayload {
                action: "fill",
                data: FillPayload { selector, value },
            },
        )?,
        Command::Type {
            selector,
            text,
            delay_ms,
            clear,
        } => send_command(
            &cli.session,
            timeout,
            ActionPayload {
                action: "type",
                data: TypePayload {
                    selector,
                    text,
                    delay: delay_ms,
                    clear: if clear { Some(true) } else { None },
                },
            },
        )?,
        Command::Snapshot {
            interactive,
            max_depth,
            compact,
            selector,
            cursor,
        } => send_command(
            &cli.session,
            timeout,
            ActionPayload {
                action: "snapshot",
                data: serde_json::json!({
                    "interactive": if interactive { Some(true) } else { None::<bool> },
                    "cursor": if cursor { Some(true) } else { None::<bool> },
                    "maxDepth": max_depth,
                    "compact": if compact { Some(true) } else { None::<bool> },
                    "selector": selector,
                }),
            },
        )?,
        Command::Eval { script } => send_command(
            &cli.session,
            timeout,
            ActionPayload {
                action: "evaluate",
                data: EvalPayload { script },
            },
        )?,
        Command::Wait {
            target,
            timeout_ms,
            state,
        } => {
            // Detect if target is a number (ms) or a selector
            let (selector, wait_timeout) = match &target {
                Some(t) if t.parse::<u64>().is_ok() => {
                    (None, Some(t.parse::<u64>().unwrap()))
                }
                sel => (sel.clone(), timeout_ms),
            };
            send_command(
                &cli.session,
                timeout,
                ActionPayload {
                    action: "wait",
                    data: WaitPayload {
                        selector,
                        timeout: wait_timeout,
                        state,
                    },
                },
            )?
        }
        Command::Screenshot {
            path,
            full_page,
            selector,
            format,
        } => send_command(
            &cli.session,
            timeout,
            ActionPayload {
                action: "screenshot",
                data: serde_json::json!({
                    "path": path,
                    "fullPage": full_page,
                    "selector": selector,
                    "format": format,
                }),
            },
        )?,
        Command::Back => send_command(
            &cli.session,
            timeout,
            ActionPayload {
                action: "back",
                data: serde_json::json!({}),
            },
        )?,
        Command::Forward => send_command(
            &cli.session,
            timeout,
            ActionPayload {
                action: "forward",
                data: serde_json::json!({}),
            },
        )?,
        Command::Reload => send_command(
            &cli.session,
            timeout,
            ActionPayload {
                action: "reload",
                data: serde_json::json!({}),
            },
        )?,
        Command::Url => send_command(
            &cli.session,
            timeout,
            ActionPayload {
                action: "url",
                data: serde_json::json!({}),
            },
        )?,
        Command::Title => send_command(
            &cli.session,
            timeout,
            ActionPayload {
                action: "title",
                data: serde_json::json!({}),
            },
        )?,
        Command::Console { clear } => send_command(
            &cli.session,
            timeout,
            ActionPayload {
                action: "console",
                data: serde_json::json!({ "clear": clear }),
            },
        )?,
        Command::Errors { clear } => send_command(
            &cli.session,
            timeout,
            ActionPayload {
                action: "errors",
                data: serde_json::json!({ "clear": clear }),
            },
        )?,
        Command::Press { key } => send_command(
            &cli.session,
            timeout,
            ActionPayload {
                action: "press",
                data: serde_json::json!({ "key": key }),
            },
        )?,
        Command::Dblclick { selector, delay_ms } => send_command(
            &cli.session,
            timeout,
            ActionPayload {
                action: "dblclick",
                data: serde_json::json!({ "selector": selector, "delay": delay_ms }),
            },
        )?,
        Command::Hover { selector } => send_command(
            &cli.session,
            timeout,
            ActionPayload {
                action: "hover",
                data: serde_json::json!({ "selector": selector }),
            },
        )?,
        Command::Focus { selector } => send_command(
            &cli.session,
            timeout,
            ActionPayload {
                action: "focus",
                data: serde_json::json!({ "selector": selector }),
            },
        )?,
        Command::Check { selector } => send_command(
            &cli.session,
            timeout,
            ActionPayload {
                action: "check",
                data: serde_json::json!({ "selector": selector }),
            },
        )?,
        Command::Uncheck { selector } => send_command(
            &cli.session,
            timeout,
            ActionPayload {
                action: "uncheck",
                data: serde_json::json!({ "selector": selector }),
            },
        )?,
        Command::Select { selector, values } => send_command(
            &cli.session,
            timeout,
            ActionPayload {
                action: "select",
                data: serde_json::json!({ "selector": selector, "values": values }),
            },
        )?,
        Command::Drag { source, target } => send_command(
            &cli.session,
            timeout,
            ActionPayload {
                action: "drag",
                data: serde_json::json!({ "source": source, "target": target }),
            },
        )?,
        Command::Upload { selector, files } => send_command(
            &cli.session,
            timeout,
            ActionPayload {
                action: "upload",
                data: serde_json::json!({ "selector": selector, "files": files }),
            },
        )?,
        Command::Download { selector, path } => send_command(
            &cli.session,
            timeout,
            ActionPayload {
                action: "download",
                data: serde_json::json!({ "selector": selector, "path": path }),
            },
        )?,
        Command::Scroll {
            direction,
            amount,
            selector,
        } => send_command(
            &cli.session,
            timeout,
            ActionPayload {
                action: "scroll",
                data: serde_json::json!({
                    "direction": direction,
                    "amount": amount.unwrap_or(500),
                    "selector": selector,
                }),
            },
        )?,
        Command::Scrollintoview { selector } => send_command(
            &cli.session,
            timeout,
            ActionPayload {
                action: "scrollintoview",
                data: serde_json::json!({ "selector": selector }),
            },
        )?,
        Command::Highlight { selector } => send_command(
            &cli.session,
            timeout,
            ActionPayload {
                action: "highlight",
                data: serde_json::json!({ "selector": selector }),
            },
        )?,
        Command::Pdf { path } => send_command(
            &cli.session,
            timeout,
            ActionPayload {
                action: "pdf",
                data: serde_json::json!({ "path": path }),
            },
        )?,
        Command::Content => send_command(
            &cli.session,
            timeout,
            ActionPayload {
                action: "content",
                data: serde_json::json!({}),
            },
        )?,
        Command::Close => send_command(
            &cli.session,
            timeout,
            ActionPayload {
                action: "close",
                data: serde_json::json!({}),
            },
        )?,
        Command::Install { browser, deps } => {
            return run_playwright_install(browser.as_deref(), deps);
        }
        Command::Raw { json } => send_raw(&cli.session, timeout, &json)?,
        Command::Generic {
            action,
            json,
            file,
            arg,
        } => {
            let payload = build_generic_payload(&action, json, file, &arg)?;
            let payload_str = serde_json::to_string(&payload)?;
            send_raw(&cli.session, timeout, &payload_str)?
        }
    };

    if !response.success {
        let message = response
            .error
            .unwrap_or_else(|| "Command failed".to_string());
        if output_json {
            let err_response = serde_json::json!({
                "id": response.id,
                "success": false,
                "error": message,
            });
            eprintln!("{}", serde_json::to_string_pretty(&err_response)?);
        } else {
            eprintln!("{message}");
        }
        std::process::exit(1);
    }

    if output_json {
        let output = serde_json::to_string_pretty(&response)?;
        println!("{output}");
    } else {
        print_clean_output(&response.data);
    }

    Ok(())
}

/// Print clean, token-efficient output matching agent-browser style.
///
/// Rules:
///   - snapshot: print the tree text directly
///   - url/back/forward/reload: print just the URL
///   - title: print just the title
///   - evaluate: print the result value
///   - screenshot/pdf: print "path"
///   - console: print each message on its own line (nothing if empty)
///   - errors: print each error on its own line (nothing if empty)
///   - cookies_get: print cookies as compact JSON
///   - content: print the html
///   - element queries (text/innertext/innerhtml/inputvalue/attribute): print value
///   - boolean checks (isvisible/isenabled/ischecked): print true/false
///   - count: print the number
///   - boundingbox: print the box as JSON
///   - styles: print elements as compact JSON
///   - requests: print requests as compact JSON
///   - tab_list: print tab info
///   - Success with no meaningful data: nothing (silent success)
///   - Everything else: compact JSON of the data field
fn print_clean_output(data: &Option<Value>) {
    let Some(data) = data else { return };
    if data.is_null() { return }

    let obj = match data.as_object() {
        Some(o) => o,
        None => {
            // Scalar data -- just print it
            print_value(data);
            return;
        }
    };

    // snapshot -> print tree directly
    if let Some(snap) = obj.get("snapshot") {
        if let Some(s) = snap.as_str() {
            println!("{s}");
        }
        return;
    }

    // url (from navigate/back/forward/reload/url)
    if let Some(url) = obj.get("url") {
        if let Some(s) = url.as_str() {
            println!("{s}");
        }
        return;
    }

    // title
    if let Some(title) = obj.get("title") {
        if let Some(s) = title.as_str() {
            println!("{s}");
        }
        return;
    }

    // evaluate result -- always JSON-encode so agents can distinguish types
    if let Some(result) = obj.get("result") {
        print_value_json(result);
        return;
    }

    // screenshot/pdf path
    if let Some(path) = obj.get("path") {
        if let Some(s) = path.as_str() {
            println!("{s}");
        }
        return;
    }

    // html (content command)
    if let Some(html) = obj.get("html") {
        if let Some(s) = html.as_str() {
            println!("{s}");
        }
        return;
    }

    // text (gettext/innertext)
    if let Some(text) = obj.get("text") {
        if let Some(s) = text.as_str() {
            println!("{s}");
        }
        return;
    }

    // value (inputvalue/attribute value/storage)
    if let Some(val) = obj.get("value") {
        print_value(val);
        return;
    }

    // console messages
    if let Some(messages) = obj.get("messages") {
        if let Some(arr) = messages.as_array() {
            for msg in arr {
                if let Some(o) = msg.as_object() {
                    let typ = o.get("type").and_then(|v| v.as_str()).unwrap_or("log");
                    let text = o.get("text").and_then(|v| v.as_str()).unwrap_or("");
                    println!("[{typ}] {text}");
                }
            }
        }
        return;
    }

    // errors
    if let Some(errors) = obj.get("errors") {
        if let Some(arr) = errors.as_array() {
            for err in arr {
                if let Some(o) = err.as_object() {
                    let msg = o.get("message").and_then(|v| v.as_str()).unwrap_or("");
                    println!("{msg}");
                }
            }
        }
        return;
    }

    // cookies
    if let Some(cookies) = obj.get("cookies") {
        if let Some(arr) = cookies.as_array() {
            if arr.is_empty() { return }
            if let Ok(s) = serde_json::to_string(cookies) {
                println!("{s}");
            }
        }
        return;
    }

    // requests
    if let Some(requests) = obj.get("requests") {
        if let Some(arr) = requests.as_array() {
            if arr.is_empty() { return }
            if let Ok(s) = serde_json::to_string(requests) {
                println!("{s}");
            }
        }
        return;
    }

    // tabs
    if let Some(tabs) = obj.get("tabs") {
        if let Some(arr) = tabs.as_array() {
            for tab in arr {
                if let Some(o) = tab.as_object() {
                    let idx = o.get("index").and_then(|v| v.as_u64()).unwrap_or(0);
                    let url = o.get("url").and_then(|v| v.as_str()).unwrap_or("");
                    let title = o.get("title").and_then(|v| v.as_str()).unwrap_or("");
                    let active = o.get("active").and_then(|v| v.as_bool()).unwrap_or(false);
                    let marker = if active { "*" } else { " " };
                    println!("{marker}[{idx}] {title} - {url}");
                }
            }
        }
        return;
    }

    // boolean queries
    for key in ["visible", "enabled", "checked"] {
        if let Some(val) = obj.get(key) {
            if let Some(b) = val.as_bool() {
                println!("{b}");
                return;
            }
        }
    }

    // count
    if let Some(count) = obj.get("count") {
        if let Some(n) = count.as_u64() {
            println!("{n}");
            return;
        }
    }

    // box (boundingbox)
    if let Some(bbox) = obj.get("box") {
        if let Ok(s) = serde_json::to_string(bbox) {
            println!("{s}");
        }
        return;
    }

    // elements (styles)
    if let Some(elements) = obj.get("elements") {
        if let Ok(s) = serde_json::to_string(elements) {
            println!("{s}");
        }
        return;
    }

    // storageState (storage_state)
    if let Some(ss) = obj.get("storageState") {
        if let Some(s) = ss.as_str() {
            println!("{s}");
        }
        return;
    }

    // data (storage_get all)
    if let Some(d) = obj.get("data") {
        if let Ok(s) = serde_json::to_string(d) {
            println!("{s}");
        }
        return;
    }

    // body (responsebody)
    if let Some(body) = obj.get("body") {
        print_value(body);
        return;
    }

    // Silent success for action confirmations (clicked, filled, typed, etc.)
    // These have only boolean flags like {"clicked": true} -- no output needed.
    let confirm_keys = [
        "launched", "clicked", "typed", "filled", "pressed", "checked",
        "unchecked", "uploaded", "focused", "hovered", "dragged",
        "selected", "tapped", "cleared", "highlighted", "scrolled",
        "switched", "waited", "set", "closed", "routed", "unrouted",
        "emulated", "added", "exposed", "paused", "injected",
        "started", "stopped", "copied", "pasted", "inserted",
        "moved", "down", "up",
    ];
    for key in confirm_keys {
        if obj.contains_key(key) {
            return; // Silent success
        }
    }

    // Fallback: print data as compact JSON if non-empty
    if !obj.is_empty() {
        if let Ok(s) = serde_json::to_string(data) {
            println!("{s}");
        }
    }
}

fn print_value(val: &Value) {
    match val {
        Value::String(s) => println!("{s}"),
        Value::Null => {}
        Value::Bool(b) => println!("{b}"),
        Value::Number(n) => println!("{n}"),
        _ => {
            if let Ok(s) = serde_json::to_string(val) {
                println!("{s}");
            }
        }
    }
}

/// Print a value as JSON (preserving type info for agents).
/// Strings are quoted, numbers/bools printed as-is, null is silent.
fn print_value_json(val: &Value) {
    match val {
        Value::Null => {}
        _ => {
            if let Ok(s) = serde_json::to_string(val) {
                println!("{s}");
            }
        }
    }
}

/// Resolve the octo-browserd lib directory.
///
/// Checks (in order):
///   1. /usr/local/lib/octo-browserd  (system install)
///   2. <repo>/backend/crates/octo-browserd  (dev checkout, relative to this binary)
///   3. OCTO_BROWSERD_DIR env var
fn find_browserd_dir() -> Result<PathBuf> {
    // Env override
    if let Ok(dir) = std::env::var("OCTO_BROWSERD_DIR") {
        let p = PathBuf::from(dir);
        if p.join("node_modules").exists() {
            return Ok(p);
        }
    }

    // System install
    let system = PathBuf::from("/usr/local/lib/octo-browserd");
    if system.join("node_modules").exists() {
        return Ok(system);
    }

    // Dev checkout: walk up from the binary location looking for backend/crates/octo-browserd
    if let Ok(exe) = std::env::current_exe() {
        let mut dir = exe.as_path();
        for _ in 0..10 {
            if let Some(parent) = dir.parent() {
                let candidate = parent.join("backend/crates/octo-browserd");
                if candidate.join("node_modules").exists() {
                    return Ok(candidate);
                }
                dir = parent;
            }
        }
    }

    Err(anyhow!(
        "Cannot find octo-browserd installation.\n\
         Checked /usr/local/lib/octo-browserd and dev checkout paths.\n\
         Run 'just install' to build octo-browserd, or set OCTO_BROWSERD_DIR."
    ))
}

/// Run `npx playwright install [browser]` or `npx playwright install-deps`
/// from the octo-browserd directory so Playwright resolves correctly.
fn run_playwright_install(browser: Option<&str>, deps: bool) -> Result<()> {
    let browserd_dir = find_browserd_dir()?;

    if deps {
        println!("Installing Playwright system dependencies (requires sudo)...");
        let status = process::Command::new("npx")
            .arg("playwright")
            .arg("install-deps")
            .current_dir(&browserd_dir)
            .status()
            .context("Failed to run 'npx playwright install-deps'")?;
        if !status.success() {
            return Err(anyhow!("playwright install-deps failed"));
        }
        // If a specific browser was also requested, install it too
        if browser.is_none() {
            return Ok(());
        }
    }

    let browser_name = browser.unwrap_or("chromium");
    let valid = ["chromium", "firefox", "webkit", "all"];
    let to_install: Vec<&str> = if browser_name == "all" {
        vec!["chromium", "firefox", "webkit"]
    } else if valid.contains(&browser_name) {
        vec![browser_name]
    } else {
        return Err(anyhow!(
            "Unknown browser '{}'. Valid options: chromium, firefox, webkit, all",
            browser_name
        ));
    };

    for name in &to_install {
        println!("Installing Playwright {name}...");
        let status = process::Command::new("npx")
            .arg("playwright")
            .arg("install")
            .arg(name)
            .current_dir(&browserd_dir)
            .status()
            .with_context(|| format!("Failed to run 'npx playwright install {name}'"))?;
        if !status.success() {
            return Err(anyhow!("playwright install {name} failed"));
        }
    }

    println!("Playwright browsers installed successfully.");
    Ok(())
}

fn build_generic_payload(
    action: &str,
    json: Option<String>,
    file: Option<PathBuf>,
    args: &[String],
) -> Result<Value> {
    let mut obj = serde_json::Map::new();

    if let Some(file_path) = file {
        let contents = std::fs::read_to_string(&file_path)
            .with_context(|| format!("Failed to read {}", file_path.display()))?;
        let value: Value = serde_json::from_str(&contents)
            .with_context(|| format!("Failed to parse {}", file_path.display()))?;
        merge_object(&mut obj, value)?;
    }

    if let Some(json_str) = json {
        let value: Value =
            serde_json::from_str(&json_str).context("Failed to parse --json payload")?;
        merge_object(&mut obj, value)?;
    }

    for pair in args {
        let (key, value) = pair
            .split_once('=')
            .ok_or_else(|| anyhow!("Invalid --arg '{}', expected key=value", pair))?;
        let parsed = parse_arg_value(value);
        obj.insert(key.to_string(), parsed);
    }

    obj.insert("action".to_string(), Value::String(action.to_string()));
    if !obj.contains_key("id") {
        obj.insert("id".to_string(), Value::String(Uuid::new_v4().to_string()));
    }
    Ok(Value::Object(obj))
}

fn merge_object(target: &mut serde_json::Map<String, Value>, value: Value) -> Result<()> {
    match value {
        Value::Object(map) => {
            for (key, val) in map {
                target.insert(key, val);
            }
            Ok(())
        }
        _ => Err(anyhow!("Expected JSON object for payload")),
    }
}

fn parse_arg_value(value: &str) -> Value {
    if let Ok(parsed) = serde_json::from_str::<Value>(value) {
        return parsed;
    }
    Value::String(value.to_string())
}

fn send_command<T: Serialize>(
    session: &str,
    timeout: Duration,
    payload: ActionPayload<'_, T>,
) -> Result<ResponsePayload> {
    let id = Uuid::new_v4().to_string();
    let command = CommandPayload { id, payload };
    let json = serde_json::to_string(&command)?;
    send_raw(session, timeout, &json)
}

fn send_raw(session: &str, timeout: Duration, json: &str) -> Result<ResponsePayload> {
    let mut conn = connect(session, timeout)?;
    let mut line = json.to_string();
    if !line.ends_with('\n') {
        line.push('\n');
    }
    conn.write_all(line.as_bytes())?;
    conn.flush()?;

    let mut reader = BufReader::new(conn.try_clone()?);
    let mut response_line = String::new();
    reader
        .read_line(&mut response_line)
        .context("Failed to read response from daemon")?;

    if response_line.trim().is_empty() {
        return Err(anyhow!("Empty response from daemon"));
    }

    let response: ResponsePayload =
        serde_json::from_str(&response_line).context("Failed to parse daemon response")?;
    Ok(response)
}

fn connect(session: &str, timeout: Duration) -> Result<Connection> {
    #[cfg(unix)]
    {
        let path = socket_path(session);
        let stream = UnixStream::connect(&path)
            .with_context(|| format!("Failed to connect to {}", path.display()))?;
        stream
            .set_read_timeout(Some(timeout))
            .context("Failed to set read timeout")?;
        stream
            .set_write_timeout(Some(timeout))
            .context("Failed to set write timeout")?;
        Ok(Connection::Unix(stream))
    }

    #[cfg(not(unix))]
    {
        let port = resolve_port(session)?;
        let addr = format!("127.0.0.1:{port}");
        let stream = std::net::TcpStream::connect(&addr)
            .with_context(|| format!("Failed to connect to {addr}"))?;
        stream
            .set_read_timeout(Some(timeout))
            .context("Failed to set read timeout")?;
        stream
            .set_write_timeout(Some(timeout))
            .context("Failed to set write timeout")?;
        return Ok(Connection::Tcp(stream));
    }
}

/// Resolve the base directory for agent-browser session socket directories.
///
/// Priority:
///   AGENT_BROWSER_SOCKET_DIR_BASE > XDG_RUNTIME_DIR/octo/agent-browser >
///   ~/.agent-browser/octo > tmpdir/agent-browser/octo
fn agent_browser_base_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("AGENT_BROWSER_SOCKET_DIR_BASE") {
        return PathBuf::from(dir);
    }
    if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
        return PathBuf::from(runtime_dir)
            .join("octo")
            .join("agent-browser");
    }
    if let Some(home) = dirs::home_dir() {
        return home.join(".agent-browser").join("octo");
    }
    std::env::temp_dir().join("agent-browser").join("octo")
}

/// Resolve the agent-browser socket directory for a session.
fn agent_browser_session_dir(session: &str) -> PathBuf {
    if let Ok(dir) = std::env::var("AGENT_BROWSER_SOCKET_DIR") {
        return PathBuf::from(dir);
    }
    agent_browser_base_dir().join(session)
}

#[cfg(unix)]
fn socket_path(session: &str) -> PathBuf {
    agent_browser_session_dir(session).join(format!("{session}.sock"))
}

#[cfg(not(unix))]
fn resolve_port(session: &str) -> Result<u16> {
    let path = agent_browser_session_dir(session).join(format!("{session}.port"));
    if let Ok(port_str) = std::fs::read_to_string(&path) {
        if let Ok(port) = port_str.trim().parse::<u16>() {
            return Ok(port);
        }
    }
    let port = port_for_session(session);
    Ok(port)
}

#[cfg(not(unix))]
fn port_for_session(session: &str) -> u16 {
    let mut hash: i64 = 0;
    for byte in session.bytes() {
        hash = hash
            .wrapping_shl(5)
            .wrapping_sub(hash)
            .wrapping_add(byte as i64);
    }
    let port = 49152 + (hash.abs() as u32 % 16383);
    port as u16
}

enum Connection {
    #[cfg(unix)]
    Unix(UnixStream),
    Tcp(std::net::TcpStream),
}

impl Connection {
    fn try_clone(&self) -> io::Result<Self> {
        match self {
            #[cfg(unix)]
            Self::Unix(stream) => Ok(Self::Unix(stream.try_clone()?)),
            Self::Tcp(stream) => Ok(Self::Tcp(stream.try_clone()?)),
        }
    }
}

impl Read for Connection {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            #[cfg(unix)]
            Self::Unix(stream) => stream.read(buf),
            Self::Tcp(stream) => stream.read(buf),
        }
    }
}

impl Write for Connection {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            #[cfg(unix)]
            Self::Unix(stream) => stream.write(buf),
            Self::Tcp(stream) => stream.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            #[cfg(unix)]
            Self::Unix(stream) => stream.flush(),
            Self::Tcp(stream) => stream.flush(),
        }
    }
}
