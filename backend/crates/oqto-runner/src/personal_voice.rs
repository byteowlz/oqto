//! Speech through the Host's own local engines for paired devices (owner request
//! 2026-11-14): eaRS transcribes dictation, spqx speaks replies. Host-scoped and
//! Session-agnostic. No listener is added: eaRS is reached on loopback and spqx runs
//! as one private warm child process. A device only reaches its own handles.
//!
//! Protocols: eaRS WebSocket (binary f32 LE 24 kHz mono in; JSON `word`/`final` out;
//! it does not close after `final`), spqx `serve` worker frames (u8 type, u32
//! request id, u32 length, little-endian; speak=1/cancel=2/shutdown=3 in,
//! ready=1/audio_start=2/audio_chunk=3/audio_done=4/error=5 out; PCM16 LE mono).
use base64::Engine;
use futures::{SinkExt, StreamExt};
use oqto_personal::{AuthorizedAction, Grants, SessionAccess, actions::ActionFailure};
use oqto_personal_wire::actions::{HostAction, MAX_VOICE_CHUNK, MAX_VOICE_TEXT, VoiceCapabilities};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, VecDeque},
    path::PathBuf,
    process::Stdio,
    sync::{Arc, Mutex as StdMutex},
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::{Child, ChildStdin, ChildStdout, Command},
    sync::Mutex,
};
use tokio_tungstenite::tungstenite::Message;

const EARS_RATE: u32 = 24_000;
const SPEAK_RATE: u32 = 24_000;
const MAX_TRANSCRIPTIONS: usize = 4;
const MAX_SPEECHES_PER_DEVICE: usize = 2;
const HANDLE_IDLE: Duration = Duration::from_secs(90);
const WORKER_IDLE: Duration = Duration::from_secs(300);
const READ_CHUNK: usize = 192 * 1024;
const SPEECH_BUFFER: usize = 24 * 1024 * 1024;

type EarsSink = futures::stream::SplitSink<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    Message,
>;

#[derive(Default)]
struct Heard {
    words: Vec<String>,
    final_text: Option<String>,
    failed: Option<String>,
    closed: bool,
}

struct Transcription {
    owner: String,
    rate: u32,
    sink: Arc<Mutex<EarsSink>>,
    heard: Arc<StdMutex<Heard>>,
    used: Instant,
}

#[derive(Default)]
struct Spoken {
    audio: VecDeque<u8>,
    done: bool,
    failed: Option<String>,
    cancelled: bool,
}

struct Speech {
    owner: String,
    state: Arc<StdMutex<Spoken>>,
    notify: Arc<tokio::sync::Notify>,
    used: Instant,
}

struct Worker {
    child: Child,
    stdin: ChildStdin,
    stdout: ChildStdout,
    used: Instant,
    next_id: u32,
}

struct Voice {
    transcriptions: StdMutex<HashMap<String, Transcription>>,
    speeches: StdMutex<HashMap<String, Speech>>,
    worker: Mutex<Option<Worker>>,
    reaper: std::sync::Once,
}

static VOICE: once_cell::sync::Lazy<Voice> = once_cell::sync::Lazy::new(|| Voice {
    transcriptions: StdMutex::default(),
    speeches: StdMutex::default(),
    worker: Mutex::default(),
    reaper: std::sync::Once::new(),
});

/// The account's own home, not the Runner's isolated `HOME`: spqx config, voice
/// registry and model cache live there (same user, same authority).
fn account_home() -> Option<PathBuf> {
    // SAFETY: getpwuid returns a pointer into static storage or null; the string is
    // copied before any other passwd call.
    unsafe {
        let entry = libc::getpwuid(libc::getuid());
        if entry.is_null() || (*entry).pw_dir.is_null() {
            return None;
        }
        let dir = std::ffi::CStr::from_ptr((*entry).pw_dir)
            .to_string_lossy()
            .into_owned();
        Some(PathBuf::from(dir))
    }
}

fn ears_url() -> Option<String> {
    loopback_ws(
        std::env::var("OQTO_VOICE_EARS_URL").unwrap_or_else(|_| "ws://127.0.0.1:8765".into()),
    )
}

/// `ws://` on loopback only; anything else is refused rather than dialled.
fn loopback_ws(url: String) -> Option<String> {
    let host = url.strip_prefix("ws://")?.split(['/', '?']).next()?;
    let name = host.rsplit_once(':').map_or(host, |(name, _)| name);
    matches!(name, "127.0.0.1" | "localhost" | "[::1]").then_some(url)
}

fn ears_address(url: &str) -> Option<String> {
    let host = url.strip_prefix("ws://")?.split(['/', '?']).next()?;
    Some(if host.contains(':') && !host.ends_with(']') {
        host.into()
    } else {
        format!("{host}:80")
    })
}

fn spqx_path() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("OQTO_VOICE_SPQX") {
        let path = PathBuf::from(path);
        return path.is_file().then_some(path);
    }
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(home) = account_home() {
        candidates.push(home.join(".cargo/bin/spqx"));
        candidates.push(home.join(".local/bin/spqx"));
    }
    candidates.push("/opt/homebrew/bin/spqx".into());
    candidates.push("/usr/local/bin/spqx".into());
    if let Some(path) = std::env::var_os("PATH") {
        candidates.extend(std::env::split_paths(&path).map(|dir| dir.join("spqx")));
    }
    candidates.into_iter().find(|path| path.is_file())
}

async fn transcribe_supported() -> bool {
    let Some(address) = ears_url().as_deref().and_then(ears_address) else {
        return false;
    };
    matches!(
        tokio::time::timeout(
            Duration::from_millis(400),
            tokio::net::TcpStream::connect(address)
        )
        .await,
        Ok(Ok(_))
    )
}

fn observing(grants: &Grants) -> bool {
    grants.sessions != SessionAccess::None
}

pub(crate) async fn execute(
    action: HostAction,
    authorization: AuthorizedAction,
) -> Result<Value, ActionFailure> {
    if !observing(authorization.grants()) {
        return Err(ActionFailure::Denied);
    }
    let voice = &*VOICE;
    voice.reaper.call_once(|| {
        tokio::spawn(async {
            loop {
                tokio::time::sleep(Duration::from_secs(15)).await;
                VOICE.reap().await;
            }
        });
    });
    let device = authorization.fence().device_id().to_owned();
    let result = match action {
        HostAction::VoiceCapabilities => serde_json::to_value(VoiceCapabilities {
            transcribe: transcribe_supported().await,
            speak: spqx_path().is_some(),
        })
        .map_err(|_| ActionFailure::Unavailable),
        HostAction::VoiceTranscribeOpen {
            sample_rate,
            language,
        } => {
            let url = ears_url().ok_or(ActionFailure::Unavailable)?;
            voice.open(&url, &device, sample_rate, language).await
        }
        HostAction::VoiceTranscribeAppend {
            transcription_id,
            audio,
        } => voice.append(&device, &transcription_id, &audio).await,
        HostAction::VoiceTranscribeFinish { transcription_id } => {
            voice.finish(&device, &transcription_id).await
        }
        HostAction::VoiceTranscribeCancel { transcription_id } => voice
            .take_transcription(&device, &transcription_id)
            .map(|_| json!({"cancelled": true})),
        HostAction::VoiceSpeakOpen { text } => voice.speak(&device, &text),
        HostAction::VoiceSpeakRead { stream_id } => voice.read(&device, &stream_id).await,
        HostAction::VoiceSpeakClose { stream_id } => voice.close(&device, &stream_id),
        _ => Err(ActionFailure::Invalid),
    };
    if !authorization.fence().is_current() {
        return Err(ActionFailure::Denied);
    }
    result
}

impl Voice {
    async fn open(
        &self,
        url: &str,
        device: &str,
        rate: u32,
        language: Option<String>,
    ) -> Result<Value, ActionFailure> {
        if !(8_000..=48_000).contains(&rate) {
            return Err(ActionFailure::Invalid);
        }
        if self
            .transcriptions
            .lock()
            .map_err(|_| ActionFailure::Unavailable)?
            .len()
            >= MAX_TRANSCRIPTIONS
        {
            return Err(ActionFailure::Unavailable);
        }
        let (socket, _) = tokio::time::timeout(
            Duration::from_secs(3),
            tokio_tungstenite::connect_async(url),
        )
        .await
        .map_err(|_| ActionFailure::Unavailable)?
        .map_err(|_| ActionFailure::Unavailable)?;
        let (mut sink, mut stream) = socket.split();
        if let Some(lang) = language.filter(|l| {
            (2..=8).contains(&l.len()) && l.chars().all(|c| c.is_ascii_alphabetic() || c == '-')
        }) {
            let _ = sink
                .send(Message::Text(
                    json!({"type":"setlanguage","lang":lang}).to_string().into(),
                ))
                .await;
        }
        let heard = Arc::new(StdMutex::new(Heard::default()));
        let reader = Arc::clone(&heard);
        tokio::spawn(async move {
            while let Some(message) = stream.next().await {
                let Ok(Message::Text(text)) = message else {
                    if message.is_err() {
                        break;
                    }
                    continue;
                };
                let Ok(value) = serde_json::from_str::<Value>(&text) else {
                    continue;
                };
                let Ok(mut heard) = reader.lock() else { break };
                match value["type"].as_str() {
                    Some("word") => {
                        if let Some(word) = value["word"].as_str().filter(|w| w.len() <= 256)
                            && heard.words.len() < 20_000
                        {
                            heard.words.push(word.to_owned());
                        }
                    }
                    Some("final") => {
                        heard.final_text = value["text"]
                            .as_str()
                            .map(|t| t.chars().take(64 * 1024).collect());
                    }
                    Some("error") => {
                        heard.failed = value["message"]
                            .as_str()
                            .map(|m| m.chars().take(200).collect())
                    }
                    _ => {}
                }
            }
            if let Ok(mut heard) = reader.lock() {
                heard.closed = true;
            }
        });
        let id = uuid::Uuid::new_v4().to_string();
        self.transcriptions
            .lock()
            .map_err(|_| ActionFailure::Unavailable)?
            .insert(
                id.clone(),
                Transcription {
                    owner: device.into(),
                    rate,
                    sink: Arc::new(Mutex::new(sink)),
                    heard,
                    used: Instant::now(),
                },
            );
        Ok(json!({"transcription_id": id}))
    }

    fn owned_transcription(
        &self,
        device: &str,
        id: &str,
    ) -> Result<(u32, Arc<Mutex<EarsSink>>, Arc<StdMutex<Heard>>), ActionFailure> {
        let mut map = self
            .transcriptions
            .lock()
            .map_err(|_| ActionFailure::Unavailable)?;
        let entry = map
            .get_mut(id)
            .filter(|t| t.owner == device)
            .ok_or(ActionFailure::NotFound)?;
        entry.used = Instant::now();
        Ok((
            entry.rate,
            Arc::clone(&entry.sink),
            Arc::clone(&entry.heard),
        ))
    }

    fn take_transcription(&self, device: &str, id: &str) -> Result<Transcription, ActionFailure> {
        let mut map = self
            .transcriptions
            .lock()
            .map_err(|_| ActionFailure::Unavailable)?;
        if map.get(id).is_none_or(|t| t.owner != device) {
            return Err(ActionFailure::NotFound);
        }
        map.remove(id).ok_or(ActionFailure::NotFound)
    }

    async fn append(&self, device: &str, id: &str, audio: &str) -> Result<Value, ActionFailure> {
        let (rate, sink, heard) = self.owned_transcription(device, id)?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(audio)
            .map_err(|_| ActionFailure::Invalid)?;
        if bytes.len() > MAX_VOICE_CHUNK || bytes.len() % 2 != 0 {
            return Err(ActionFailure::Invalid);
        }
        let samples: Vec<f32> = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| f32::from(i16::from_le_bytes([b[0], b[1]])) / 32768.0)
            .collect();
        let resampled = resample(&samples, rate, EARS_RATE);
        let payload: Vec<u8> = resampled.iter().flat_map(|s| s.to_le_bytes()).collect();
        if !payload.is_empty() {
            sink.lock()
                .await
                .send(Message::Binary(payload.into()))
                .await
                .map_err(|_| ActionFailure::Unavailable)?;
        }
        let heard = heard.lock().map_err(|_| ActionFailure::Unavailable)?;
        if let Some(reason) = &heard.failed {
            return Ok(json!({"text": heard.words.join(" "), "error": reason}));
        }
        Ok(json!({"text": heard.words.join(" ")}))
    }

    async fn finish(&self, device: &str, id: &str) -> Result<Value, ActionFailure> {
        let transcription = self.take_transcription(device, id)?;
        let _ = transcription
            .sink
            .lock()
            .await
            .send(Message::Text(json!({"type":"stop"}).to_string().into()))
            .await;
        let deadline = Instant::now() + Duration::from_secs(12);
        loop {
            {
                let heard = transcription
                    .heard
                    .lock()
                    .map_err(|_| ActionFailure::Unavailable)?;
                if let Some(text) = &heard.final_text {
                    return Ok(json!({"text": text}));
                }
                if heard.closed || heard.failed.is_some() || Instant::now() >= deadline {
                    return Ok(json!({"text": heard.words.join(" "), "partial": true}));
                }
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    fn speak(&self, device: &str, text: &str) -> Result<Value, ActionFailure> {
        let spoken = speakable(text);
        if spoken.is_empty() || text.chars().count() > MAX_VOICE_TEXT {
            return Err(ActionFailure::Invalid);
        }
        let path = spqx_path().ok_or(ActionFailure::Unavailable)?;
        let mut speeches = self
            .speeches
            .lock()
            .map_err(|_| ActionFailure::Unavailable)?;
        if speeches.values().filter(|s| s.owner == device).count() >= MAX_SPEECHES_PER_DEVICE {
            return Err(ActionFailure::Unavailable);
        }
        let state = Arc::new(StdMutex::new(Spoken::default()));
        let notify = Arc::new(tokio::sync::Notify::new());
        let id = uuid::Uuid::new_v4().to_string();
        speeches.insert(
            id.clone(),
            Speech {
                owner: device.into(),
                state: Arc::clone(&state),
                notify: Arc::clone(&notify),
                used: Instant::now(),
            },
        );
        tokio::spawn(async move {
            let outcome = VOICE.synthesize(&path, &spoken, &state, &notify).await;
            if let Ok(mut spoken) = state.lock() {
                if let Err(error) = outcome {
                    spoken.failed = Some(error);
                }
                spoken.done = true;
            }
            notify.notify_waiters();
        });
        Ok(json!({"stream_id": id, "sample_rate": SPEAK_RATE}))
    }

    /// One utterance on the warm worker; requests serialize on the worker lock.
    async fn synthesize(
        &self,
        path: &PathBuf,
        text: &str,
        state: &StdMutex<Spoken>,
        notify: &tokio::sync::Notify,
    ) -> Result<(), String> {
        let mut slot = self.worker.lock().await;
        if state.lock().map(|s| s.cancelled).unwrap_or(true) {
            return Ok(());
        }
        if slot
            .as_mut()
            .is_none_or(|worker| worker.child.try_wait().ok().flatten().is_some())
        {
            *slot = Some(start_worker(path).await?);
        }
        let worker = slot.as_mut().ok_or("speech engine unavailable")?;
        worker.used = Instant::now();
        worker.next_id = worker.next_id.wrapping_add(1).max(1);
        let id = worker.next_id;
        let outcome = async {
            write_frame(&mut worker.stdin, 1, id, text.as_bytes()).await?;
            let mut cancel_sent = false;
            loop {
                let cancelled = state.lock().map(|s| s.cancelled).unwrap_or(true);
                if cancelled && !cancel_sent {
                    write_frame(&mut worker.stdin, 2, id, &[]).await?;
                    cancel_sent = true;
                }
                let (kind, rid, payload) =
                    tokio::time::timeout(Duration::from_secs(60), read_frame(&mut worker.stdout))
                        .await
                        .map_err(|_| "speech engine stopped answering".to_string())??;
                if rid != id {
                    continue;
                }
                match kind {
                    3 => {
                        if let Ok(mut spoken) = state.lock()
                            && !spoken.cancelled
                            && spoken.audio.len() + payload.len() <= SPEECH_BUFFER
                        {
                            spoken.audio.extend(payload);
                        }
                        notify.notify_waiters();
                    }
                    4 => return Ok(()),
                    5 => {
                        return Err(String::from_utf8_lossy(&payload)
                            .chars()
                            .take(200)
                            .collect());
                    }
                    _ => {}
                }
            }
        }
        .await;
        if outcome.is_err() {
            // An engine fault ends this worker; the next request starts a fresh one.
            if let Some(mut worker) = slot.take() {
                let _ = worker.child.start_kill();
            }
        }
        outcome
    }

    async fn read(&self, device: &str, id: &str) -> Result<Value, ActionFailure> {
        let (state, notify) = {
            let mut map = self
                .speeches
                .lock()
                .map_err(|_| ActionFailure::Unavailable)?;
            let entry = map
                .get_mut(id)
                .filter(|s| s.owner == device)
                .ok_or(ActionFailure::NotFound)?;
            entry.used = Instant::now();
            (Arc::clone(&entry.state), Arc::clone(&entry.notify))
        };
        let waiting = notify.notified();
        let empty = state
            .lock()
            .map(|s| s.audio.is_empty() && !s.done)
            .unwrap_or(false);
        if empty {
            let _ = tokio::time::timeout(Duration::from_millis(800), waiting).await;
        }
        let mut spoken = state.lock().map_err(|_| ActionFailure::Unavailable)?;
        let take = spoken.audio.len().min(READ_CHUNK) & !1;
        let bytes: Vec<u8> = spoken.audio.drain(..take).collect();
        let done = spoken.done && spoken.audio.is_empty();
        let mut reply =
            json!({"audio": base64::engine::general_purpose::STANDARD.encode(bytes), "done": done});
        if done {
            if let Some(reason) = &spoken.failed {
                reply["error"] = json!(reason);
            }
            drop(spoken);
            if let Ok(mut map) = self.speeches.lock() {
                map.remove(id);
            }
        }
        Ok(reply)
    }

    fn close(&self, device: &str, id: &str) -> Result<Value, ActionFailure> {
        let mut map = self
            .speeches
            .lock()
            .map_err(|_| ActionFailure::Unavailable)?;
        if map.get(id).is_none_or(|s| s.owner != device) {
            return Err(ActionFailure::NotFound);
        }
        if let Some(speech) = map.remove(id)
            && let Ok(mut spoken) = speech.state.lock()
        {
            spoken.cancelled = true;
            spoken.audio.clear();
        }
        Ok(json!({"closed": true}))
    }

    async fn reap(&self) {
        if let Ok(mut map) = self.transcriptions.lock() {
            map.retain(|_, t| t.used.elapsed() < HANDLE_IDLE);
        }
        if let Ok(mut map) = self.speeches.lock() {
            map.retain(|_, s| {
                let keep = s.used.elapsed() < HANDLE_IDLE;
                if !keep && let Ok(mut spoken) = s.state.lock() {
                    spoken.cancelled = true;
                }
                keep
            });
        }
        if let Ok(mut slot) = self.worker.try_lock()
            && slot
                .as_ref()
                .is_some_and(|w| w.used.elapsed() > WORKER_IDLE)
            && let Some(mut worker) = slot.take()
        {
            let _ = write_frame(&mut worker.stdin, 3, 0, &[]).await;
            let _ = tokio::time::timeout(Duration::from_secs(5), worker.child.wait()).await;
            let _ = worker.child.start_kill();
        }
    }
}

async fn start_worker(path: &PathBuf) -> Result<Worker, String> {
    let mut command = Command::new(path);
    command.args(["serve", "--output-sample-rate", "24000", "--quiet"]);
    if let Ok(voice) = std::env::var("OQTO_VOICE_SPQX_VOICE")
        && !voice.is_empty()
        && voice.len() <= 128
        && voice
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_-.".contains(c))
    {
        command.args(["--voice", &voice]);
    }
    if let Some(home) = account_home() {
        command
            .env("HOME", &home)
            .env_remove("XDG_CONFIG_HOME")
            .env_remove("XDG_RUNTIME_DIR");
    }
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    let mut child = command
        .spawn()
        .map_err(|_| "speech engine could not start".to_string())?;
    let stdin = child.stdin.take().ok_or("speech engine has no input")?;
    let mut stdout = child.stdout.take().ok_or("speech engine has no output")?;
    let (kind, _, payload) =
        tokio::time::timeout(Duration::from_secs(120), read_frame(&mut stdout))
            .await
            .map_err(|_| "speech engine did not become ready".to_string())??;
    if kind != 1 {
        let _ = child.start_kill();
        return Err(String::from_utf8_lossy(&payload)
            .chars()
            .take(200)
            .collect());
    }
    Ok(Worker {
        child,
        stdin,
        stdout,
        used: Instant::now(),
        next_id: 0,
    })
}

async fn write_frame(
    stdin: &mut ChildStdin,
    kind: u8,
    id: u32,
    payload: &[u8],
) -> Result<(), String> {
    let mut frame = Vec::with_capacity(9 + payload.len());
    frame.push(kind);
    frame.extend_from_slice(&id.to_le_bytes());
    frame.extend_from_slice(
        &u32::try_from(payload.len())
            .map_err(|_| "too long")?
            .to_le_bytes(),
    );
    frame.extend_from_slice(payload);
    stdin
        .write_all(&frame)
        .await
        .map_err(|_| "speech engine closed".to_string())?;
    stdin
        .flush()
        .await
        .map_err(|_| "speech engine closed".to_string())
}

async fn read_frame(stdout: &mut ChildStdout) -> Result<(u8, u32, Vec<u8>), String> {
    let mut header = [0u8; 9];
    stdout
        .read_exact(&mut header)
        .await
        .map_err(|_| "speech engine closed".to_string())?;
    let id = u32::from_le_bytes([header[1], header[2], header[3], header[4]]);
    let len = u32::from_le_bytes([header[5], header[6], header[7], header[8]]) as usize;
    if len > 8 * 1024 * 1024 {
        return Err("speech engine frame too large".into());
    }
    let mut payload = vec![0u8; len];
    stdout
        .read_exact(&mut payload)
        .await
        .map_err(|_| "speech engine closed".to_string())?;
    Ok((header[0], id, payload))
}

/// Linear resampling; good enough for speech recognition input.
fn resample(samples: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || samples.is_empty() {
        return samples.to_vec();
    }
    let ratio = f64::from(from) / f64::from(to);
    let out = ((samples.len() as f64) / ratio).floor() as usize;
    (0..out)
        .map(|i| {
            let position = i as f64 * ratio;
            let index = position.floor() as usize;
            let fraction = (position - index as f64) as f32;
            let a = samples[index.min(samples.len() - 1)];
            let b = samples[(index + 1).min(samples.len() - 1)];
            a + (b - a) * fraction
        })
        .collect()
}

/// What a listener should hear of an agent reply: prose, not markup or code.
pub(crate) fn speakable(text: &str) -> String {
    let mut out = String::new();
    let mut in_fence = false;
    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            if !in_fence {
                out.push_str(" Code block. ");
            }
            in_fence = !in_fence;
            continue;
        }
        if in_fence || trimmed.starts_with('|') {
            continue;
        }
        let line = trimmed.trim_start_matches(['#', '>', '-', '*', '+', ' ']);
        let mut cleaned = String::new();
        let mut rest = line;
        // [label](url) -> label; bare URLs dropped.
        while let Some(start) = rest.find('[') {
            cleaned.push_str(&rest[..start]);
            let after = &rest[start + 1..];
            match after
                .find("](")
                .and_then(|mid| after[mid..].find(')').map(|end| (mid, mid + end)))
            {
                Some((mid, end)) => {
                    cleaned.push_str(&after[..mid]);
                    rest = &after[end + 1..];
                }
                None => {
                    cleaned.push('[');
                    rest = after;
                }
            }
        }
        cleaned.push_str(rest);
        let words: Vec<&str> = cleaned
            .split_whitespace()
            .filter(|w| !w.starts_with("http://") && !w.starts_with("https://"))
            .collect();
        let line: String = words
            .join(" ")
            .chars()
            .filter(|c| !matches!(c, '*' | '_' | '`' | '#'))
            .collect();
        if !line.is_empty() {
            out.push_str(&line);
            out.push(if line.ends_with(['.', '!', '?', ':', ';']) {
                ' '
            } else {
                '.'
            });
            out.push(' ');
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speakable_reads_prose_and_skips_markup_code_tables_and_urls() {
        let text = "## Done\n\nThe **build** passed, see [the log](https://x.y/z).\n\n```rust\nfn main() {}\n```\n| a | b |\n- next: deploy\nhttps://example.com";
        assert_eq!(
            speakable(text),
            "Done. The build passed, see the log. Code block. next: deploy."
        );
        assert_eq!(speakable("```\nonly code\n```"), "Code block.");
        assert_eq!(speakable("   "), "");
    }

    #[test]
    fn resample_keeps_duration() {
        let one_second = vec![0.5_f32; 16_000];
        assert_eq!(resample(&one_second, 16_000, 24_000).len(), 24_000);
        assert_eq!(resample(&one_second, 24_000, 24_000).len(), 16_000);
        assert!(
            resample(&one_second, 16_000, 24_000)
                .iter()
                .all(|s| (*s - 0.5).abs() < 1e-6)
        );
    }

    #[test]
    fn ears_url_must_be_loopback_websocket() {
        let ok = |url: &str| loopback_ws(url.into()).is_some();
        assert!(ok("ws://127.0.0.1:8765"));
        assert!(ok("ws://localhost:8765/"));
        assert!(ok("ws://[::1]:8765"));
        assert!(!ok("ws://10.0.0.5:8765"));
        assert!(!ok("wss://127.0.0.1:8765"));
        assert!(!ok("ws://evil.example:8765"));
        assert!(!ok("ws://127.0.0.1.evil.example:8765"));
        assert_eq!(
            ears_address("ws://127.0.0.1:8765").as_deref(),
            Some("127.0.0.1:8765")
        );
    }

    fn fresh() -> Voice {
        Voice {
            transcriptions: StdMutex::default(),
            speeches: StdMutex::default(),
            worker: Mutex::default(),
            reaper: std::sync::Once::new(),
        }
    }

    /// A fake eaRS: counts audio bytes, answers words, and on stop sends final
    /// without closing (as the real server does).
    async fn fake_ears() -> (String, Arc<StdMutex<(usize, Vec<String>)>>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}", listener.local_addr().unwrap());
        let seen = Arc::new(StdMutex::new((0usize, Vec::<String>::new())));
        let record = Arc::clone(&seen);
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let socket = tokio_tungstenite::accept_async(stream).await.unwrap();
            let (mut sink, mut stream) = socket.split();
            while let Some(Ok(message)) = stream.next().await {
                match message {
                    Message::Binary(bytes) => {
                        record.lock().unwrap().0 += bytes.len();
                        let _ = sink.send(Message::Text(r#"{"type":"word","word":"hallo","start_time":0.1,"end_time":null}"#.into())).await;
                    }
                    Message::Text(text) => {
                        record.lock().unwrap().1.push(text.to_string());
                        if text.contains("stop") {
                            let _ = sink
                                .send(Message::Text(
                                    r#"{"type":"final","text":"Hallo Welt","words":[]}"#.into(),
                                ))
                                .await;
                        }
                    }
                    _ => {}
                }
            }
            futures::future::pending::<()>().await;
        });
        (url, seen)
    }

    #[tokio::test]
    async fn transcription_relays_resampled_audio_partials_and_final_to_its_owner_only() {
        let (url, seen) = fake_ears().await;
        let voice = fresh();
        let opened = voice
            .open(&url, "phone", 16_000, Some("de".into()))
            .await
            .unwrap();
        let id = opened["transcription_id"].as_str().unwrap().to_owned();
        // 0.5 s of 16 kHz PCM16 becomes 0.5 s of 24 kHz f32: 12,000 samples, 48,000 bytes.
        let pcm: Vec<u8> = (0..8_000)
            .flat_map(|i| ((i % 100) as i16 * 100).to_le_bytes())
            .collect();
        let audio = base64::engine::general_purpose::STANDARD.encode(&pcm);
        assert!(matches!(
            voice.append("other-device", &id, &audio).await,
            Err(ActionFailure::NotFound)
        ));
        voice.append("phone", &id, &audio).await.unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        let partial = voice.append("phone", &id, &audio).await.unwrap();
        assert!(
            partial["text"].as_str().unwrap().starts_with("hallo"),
            "{partial}"
        );
        assert!(matches!(
            voice.append("phone", &id, "not base64!").await,
            Err(ActionFailure::Invalid)
        ));
        let done = voice.finish("phone", &id).await.unwrap();
        assert_eq!(done["text"], "Hallo Welt");
        let (bytes, texts) = seen.lock().unwrap().clone();
        assert_eq!(bytes, 2 * 48_000);
        assert!(
            texts
                .iter()
                .any(|t| t.contains("setlanguage") && t.contains("\"de\""))
        );
        assert!(texts.iter().any(|t| t.contains("stop")));
        assert!(
            matches!(
                voice.finish("phone", &id).await,
                Err(ActionFailure::NotFound)
            ),
            "a finished handle is gone"
        );
    }

    #[tokio::test]
    async fn transcription_refuses_bad_rates_and_unreachable_engines() {
        let voice = fresh();
        assert!(matches!(
            voice.open("ws://127.0.0.1:1", "phone", 7_999, None).await,
            Err(ActionFailure::Invalid)
        ));
        assert!(matches!(
            voice.open("ws://127.0.0.1:1", "phone", 16_000, None).await,
            Err(ActionFailure::Unavailable)
        ));
    }

    /// Live check against the real engines on this machine (spqx installed, eaRS
    /// on its default port). Run: OQTO_VOICE_LIVE=1 cargo test -p oqto-runner --lib
    /// personal_voice -- --ignored
    #[tokio::test]
    #[ignore]
    async fn live_speak_then_transcribe_round_trip() {
        if std::env::var("OQTO_VOICE_LIVE").is_err() {
            return;
        }
        let voice = fresh();
        let opened = voice
            .speak("phone", "Hallo, das ist ein Test der Sprachausgabe.")
            .unwrap();
        let id = opened["stream_id"].as_str().unwrap().to_owned();
        let start = Instant::now();
        let (mut pcm, mut first) = (Vec::new(), None);
        loop {
            let reply = voice.read("phone", &id).await.unwrap();
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(reply["audio"].as_str().unwrap())
                .unwrap();
            if !bytes.is_empty() && first.is_none() {
                first = Some(start.elapsed());
            }
            pcm.extend(bytes);
            if reply["done"] == true {
                assert!(reply.get("error").is_none(), "{reply}");
                break;
            }
        }
        println!(
            "LIVE tts first={first:?} total={:?} seconds={}",
            start.elapsed(),
            pcm.len() / 2 / 24_000
        );
        assert!(pcm.len() > 24_000 * 2, "at least a second of audio");
        let url = ears_url().unwrap();
        let opened = voice
            .open(&url, "phone", 24_000, Some("de".into()))
            .await
            .unwrap();
        let tid = opened["transcription_id"].as_str().unwrap().to_owned();
        for chunk in pcm.chunks(24_000) {
            voice
                .append(
                    "phone",
                    &tid,
                    &base64::engine::general_purpose::STANDARD.encode(chunk),
                )
                .await
                .unwrap();
        }
        let done = voice.finish("phone", &tid).await.unwrap();
        println!("LIVE stt {done}");
        assert!(!done["text"].as_str().unwrap().is_empty());
    }
}
