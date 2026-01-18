use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
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
	Fill {
		selector: String,
		value: String,
	},
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
		#[arg(long)]
		interactive: bool,
		#[arg(long)]
		max_depth: Option<u32>,
		#[arg(long)]
		compact: bool,
		#[arg(long)]
		selector: Option<String>,
	},
	/// Evaluate a script in the page
	Eval {
		script: String,
	},
	/// Wait for a selector or timeout
	Wait {
		#[arg(long)]
		selector: Option<String>,
		#[arg(long)]
		timeout_ms: Option<u64>,
		#[arg(long)]
		state: Option<WaitState>,
	},
	/// Close the browser daemon
	Close,
	/// Send a raw JSON command to the daemon
	Raw {
		json: String,
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
struct SnapshotPayload {
	#[serde(skip_serializing_if = "Option::is_none")]
	interactive: Option<bool>,
	#[serde(rename = "maxDepth", skip_serializing_if = "Option::is_none")]
	max_depth: Option<u32>,
	#[serde(skip_serializing_if = "Option::is_none")]
	compact: Option<bool>,
	#[serde(skip_serializing_if = "Option::is_none")]
	selector: Option<String>,
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

	let response = match cli.command {
		Command::Open { url, wait_until } | Command::Navigate { url, wait_until } => {
			send_command(
				&cli.session,
				timeout,
				ActionPayload {
					action: "navigate",
					data: NavigatePayload { url, wait_until },
				},
			)?
		}
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
		} => send_command(
			&cli.session,
			timeout,
			ActionPayload {
				action: "snapshot",
				data: SnapshotPayload {
					interactive: if interactive { Some(true) } else { None },
					max_depth,
					compact: if compact { Some(true) } else { None },
					selector,
				},
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
			selector,
			timeout_ms,
			state,
		} => send_command(
			&cli.session,
			timeout,
			ActionPayload {
				action: "wait",
				data: WaitPayload {
					selector,
					timeout: timeout_ms,
					state,
				},
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
		Command::Raw { json } => send_raw(&cli.session, timeout, &json)?,
	};

	let output = serde_json::to_string_pretty(&response)?;
	println!("{output}");

	if !response.success {
		let message = response
			.error
			.clone()
			.unwrap_or_else(|| "Command failed".to_string());
		return Err(anyhow!(message));
	}

	Ok(())
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

	let response: ResponsePayload = serde_json::from_str(&response_line)
		.context("Failed to parse daemon response")?;
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
		return Ok(Connection::Unix(stream));
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

#[cfg(unix)]
fn socket_path(session: &str) -> PathBuf {
	let mut path = std::env::temp_dir();
	path.push(format!("agent-browser-{session}.sock"));
	path
}

#[cfg(not(unix))]
fn resolve_port(session: &str) -> Result<u16> {
	let mut path = std::env::temp_dir();
	path.push(format!("agent-browser-{session}.port"));
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
		hash = hash.wrapping_shl(5).wrapping_sub(hash).wrapping_add(byte as i64);
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
