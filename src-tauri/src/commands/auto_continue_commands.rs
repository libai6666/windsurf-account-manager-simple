use log::{error, info, warn};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

pub const AUTO_CONTINUE_BRIDGE_PORT: u16 = 38478;
const AUTO_CONTINUE_BRIDGE_PREFIX: &str = "/wam-auto-continue";
const MAX_RECENT_EVENTS: usize = 50;
const AUTO_CONTINUE_ACTION_COOLDOWN_MS: u64 = 15_000;

static AUTO_CONTINUE_BRIDGE_STATE: OnceLock<Arc<AutoContinueBridgeState>> = OnceLock::new();

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoContinueBridgeConfig {
    pub enabled: bool,
    pub continue_text: String,
    pub debounce_ms: u64,
    pub markers: Vec<String>,
}

impl Default for AutoContinueBridgeConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            continue_text: "继续工作".to_string(),
            debounce_ms: 10_000,
            markers: vec![
                "third-party model provider is experiencing issues".to_string(),
                "included daily usage quota is exhausted".to_string(),
                "all API providers are over their global rate limit for trial users".to_string(),
                "daily usage quota".to_string(),
                "usage quota is exhausted".to_string(),
                "quota is exhausted".to_string(),
                "global rate limit".to_string(),
                "rate limit for trial users".to_string(),
                "purchase extra usage".to_string(),
                "premium models".to_string(),
            ],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoContinueBridgeEvent {
    pub id: String,
    pub received_at: String,
    pub event_type: String,
    pub source: String,
    pub url: Option<String>,
    pub message: String,
    pub matched: bool,
    pub action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoContinueBridgeAction {
    pub id: String,
    pub source_event_id: String,
    pub created_at: String,
    pub text: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IncomingAutoContinueBridgeEvent {
    #[serde(default)]
    event_type: Option<String>,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    location: Option<String>,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    payload: Option<Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IncomingAutoContinueActionResult {
    #[serde(default)]
    action_id: Option<String>,
    #[serde(default)]
    success: bool,
    #[serde(default)]
    method: Option<String>,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Debug)]
struct AutoContinueBridgeState {
    started: AtomicBool,
    config: Mutex<AutoContinueBridgeConfig>,
    events: Mutex<VecDeque<AutoContinueBridgeEvent>>,
    pending_actions: Mutex<VecDeque<AutoContinueBridgeAction>>,
    last_fingerprint: Mutex<Option<(String, Instant)>>,
    last_action_at: Mutex<Option<Instant>>,
    sent_actions: AtomicUsize,
}

impl AutoContinueBridgeState {
    fn new() -> Self {
        Self {
            started: AtomicBool::new(false),
            config: Mutex::new(AutoContinueBridgeConfig::default()),
            events: Mutex::new(VecDeque::new()),
            pending_actions: Mutex::new(VecDeque::new()),
            last_fingerprint: Mutex::new(None),
            last_action_at: Mutex::new(None),
            sent_actions: AtomicUsize::new(0),
        }
    }
}

fn auto_continue_bridge_state() -> Arc<AutoContinueBridgeState> {
    AUTO_CONTINUE_BRIDGE_STATE
        .get_or_init(|| Arc::new(AutoContinueBridgeState::new()))
        .clone()
}

pub fn start_auto_continue_bridge_server() {
    let state = auto_continue_bridge_state();
    if state.started.swap(true, Ordering::SeqCst) {
        return;
    }

    thread::spawn(move || {
        let address = format!("127.0.0.1:{}", AUTO_CONTINUE_BRIDGE_PORT);
        let listener = match TcpListener::bind(&address) {
            Ok(listener) => listener,
            Err(e) => {
                state.started.store(false, Ordering::SeqCst);
                error!("Auto continue bridge failed to bind {}: {}", address, e);
                return;
            }
        };
        info!("Auto continue bridge listening on http://{}", address);

        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    let state = state.clone();
                    thread::spawn(move || {
                        if let Err(e) = handle_auto_continue_bridge_connection(stream, &state) {
                            warn!("Auto continue bridge request failed: {}", e);
                        }
                    });
                }
                Err(e) => warn!("Auto continue bridge incoming connection failed: {}", e),
            }
        }
    });
}

#[tauri::command]
pub async fn auto_continue_windsurf_conversations() -> Result<Value, String> {
    start_auto_continue_bridge_server();
    get_auto_continue_bridge_status().await
}

#[tauri::command]
pub async fn get_auto_continue_bridge_status() -> Result<Value, String> {
    start_auto_continue_bridge_server();
    let state = auto_continue_bridge_state();
    let config = state
        .config
        .lock()
        .map_err(|_| "自动继续配置锁异常".to_string())?
        .clone();
    let recent_events: Vec<_> = state
        .events
        .lock()
        .map_err(|_| "自动继续事件锁异常".to_string())?
        .iter()
        .rev()
        .take(20)
        .cloned()
        .collect();
    let detected_count = recent_events.iter().filter(|event| event.matched).count();
    let pending_action_count = state
        .pending_actions
        .lock()
        .map_err(|_| "自动继续动作队列锁异常".to_string())?
        .len();
    let message = if config.enabled {
        "自动继续 Bridge 已开启"
    } else {
        "自动继续 Bridge 已关闭"
    };
    Ok(json!({
        "success": true,
        "bridge": true,
        "running": state.started.load(Ordering::SeqCst),
        "port": AUTO_CONTINUE_BRIDGE_PORT,
        "config": config,
        "recentEvents": recent_events,
        "detectedCount": detected_count,
        "pendingActionCount": pending_action_count,
        "sentActionCount": state.sent_actions.load(Ordering::SeqCst),
        "continued": 0,
        "windows": [],
        "message": message
    }))
}

#[tauri::command]
pub async fn set_auto_continue_bridge_config(
    enabled: bool,
) -> Result<Value, String> {
    start_auto_continue_bridge_server();
    let state = auto_continue_bridge_state();
    {
        let mut config = state
            .config
            .lock()
            .map_err(|_| "自动继续配置锁异常".to_string())?;
        config.enabled = enabled;
    }
    get_auto_continue_bridge_status().await
}

fn handle_auto_continue_bridge_connection(
    mut stream: TcpStream,
    state: &Arc<AutoContinueBridgeState>,
) -> Result<(), String> {
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(|e| e.to_string())?;

    let mut buffer = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        let read = stream.read(&mut chunk).map_err(|e| e.to_string())?;
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
        if buffer.windows(4).any(|window| window == b"\r\n\r\n") {
            let content_length = parse_content_length(&buffer);
            if let Some(header_end) = find_header_end(&buffer) {
                let body_len = buffer.len().saturating_sub(header_end);
                if body_len >= content_length {
                    break;
                }
            }
        }
        if buffer.len() > 128 * 1024 {
            return Err("请求过大".to_string());
        }
    }

    let request = String::from_utf8_lossy(&buffer);
    let first_line = request.lines().next().unwrap_or_default();
    let parts: Vec<&str> = first_line.split_whitespace().collect();
    if parts.len() < 2 {
        return write_bridge_response(&mut stream, 400, json!({"success": false, "error": "bad request"}));
    }

    let method = parts[0];
    let path = parts[1].split('?').next().unwrap_or(parts[1]);

    if method == "OPTIONS" {
        return write_bridge_response(&mut stream, 204, json!({}));
    }

    match (method, path) {
        ("GET", path) if path == format!("{}/config", AUTO_CONTINUE_BRIDGE_PREFIX) => {
            let config = state
                .config
                .lock()
                .map_err(|_| "自动继续配置锁异常".to_string())?
                .clone();
            write_bridge_response(&mut stream, 200, json!({
                "success": true,
                "config": config,
            }))
        }
        ("GET", path) if path == format!("{}/status", AUTO_CONTINUE_BRIDGE_PREFIX) => {
            let config = state
                .config
                .lock()
                .map_err(|_| "自动继续配置锁异常".to_string())?
                .clone();
            write_bridge_response(&mut stream, 200, json!({
                "success": true,
                "running": state.started.load(Ordering::SeqCst),
                "config": config,
            }))
        }
        ("GET", path) if path == format!("{}/actions", AUTO_CONTINUE_BRIDGE_PREFIX) => {
            let actions: Vec<_> = state
                .pending_actions
                .lock()
                .map_err(|_| "自动继续动作队列锁异常".to_string())?
                .drain(..)
                .collect();
            if !actions.is_empty() {
                info!(
                    "Auto continue bridge actions polled: count={}",
                    actions.len()
                );
            }
            write_bridge_response(&mut stream, 200, json!({
                "success": true,
                "actions": actions,
            }))
        }
        ("POST", path) if path == format!("{}/action-result", AUTO_CONTINUE_BRIDGE_PREFIX) => {
            let body = extract_body(&buffer);
            let result: IncomingAutoContinueActionResult = serde_json::from_slice(body)
                .map_err(|e| format!("解析自动继续动作结果失败: {}", e))?;
            if result.success {
                state.sent_actions.fetch_add(1, Ordering::SeqCst);
                if let Ok(mut last_action_at) = state.last_action_at.lock() {
                    *last_action_at = Some(Instant::now());
                }
                info!(
                    "Auto continue bridge action sent: action={:?}, method={:?}",
                    result.action_id,
                    result.method
                );
            } else {
                warn!(
                    "Auto continue bridge action failed: action={:?}, error={:?}",
                    result.action_id,
                    result.error
                );
            }
            write_bridge_response(&mut stream, 200, json!({
                "success": true,
            }))
        }
        ("POST", path) if path == format!("{}/event", AUTO_CONTINUE_BRIDGE_PREFIX) => {
            let body = extract_body(&buffer);
            let incoming: IncomingAutoContinueBridgeEvent = serde_json::from_slice(body)
                .map_err(|e| format!("解析自动继续事件失败: {}", e))?;
            let event = process_auto_continue_bridge_event(state, incoming)?;
            write_bridge_response(&mut stream, 200, json!({
                "success": true,
                "event": event,
            }))
        }
        _ => write_bridge_response(&mut stream, 404, json!({"success": false, "error": "not found"})),
    }
}

fn process_auto_continue_bridge_event(
    state: &Arc<AutoContinueBridgeState>,
    incoming: IncomingAutoContinueBridgeEvent,
) -> Result<AutoContinueBridgeEvent, String> {
    let config = state
        .config
        .lock()
        .map_err(|_| "自动继续配置锁异常".to_string())?
        .clone();
    let message = incoming_message_text(&incoming);
    let event_type = incoming
        .event_type
        .clone()
        .unwrap_or_else(|| "runtime".to_string());
    let is_login_diagnostic = event_type.starts_with("windsurf_login_diagnostic");
    let lower_message = message.to_lowercase();
    let marker_hit = config
        .markers
        .iter()
        .any(|marker| lower_message.contains(&marker.to_lowercase()));
    let matched = marker_hit;
    let fingerprint = format!(
        "{}:{}",
        incoming.source.as_deref().unwrap_or("unknown"),
        lower_message.chars().take(180).collect::<String>()
    );
    let deduped = if matched {
        let mut last = state
            .last_fingerprint
            .lock()
            .map_err(|_| "自动继续去重锁异常".to_string())?;
        let now = Instant::now();
        let deduped = last
            .as_ref()
            .map(|(last_fingerprint, last_at)| {
                last_fingerprint == &fingerprint
                    && now.duration_since(*last_at).as_millis() < config.debounce_ms as u128
            })
            .unwrap_or(false);
        if !deduped {
            *last = Some((fingerprint, now));
        }
        deduped
    } else {
        false
    };

    let event_id = uuid::Uuid::new_v4().to_string();
    let mut action = if is_login_diagnostic {
        "diagnostic_logged".to_string()
    } else if !matched {
        "ignored".to_string()
    } else if !config.enabled {
        "detected_disabled".to_string()
    } else if deduped {
        "deduped".to_string()
    } else {
        "queued_send".to_string()
    };

    if action == "queued_send" {
        let mut actions = state
            .pending_actions
            .lock()
            .map_err(|_| "自动继续动作队列锁异常".to_string())?;
        let last_action_at = state
            .last_action_at
            .lock()
            .map_err(|_| "自动继续动作冷却锁异常".to_string())?;
        let now = Instant::now();
        let cooling_down = last_action_at
            .as_ref()
            .map(|last_at| {
                now.duration_since(*last_at).as_millis()
                    < AUTO_CONTINUE_ACTION_COOLDOWN_MS as u128
            })
            .unwrap_or(false);
        if !actions.is_empty() {
            action = "deduped_pending".to_string();
        } else if cooling_down {
            action = "cooldown".to_string();
        } else {
            let bridge_action = AutoContinueBridgeAction {
                id: uuid::Uuid::new_v4().to_string(),
                source_event_id: event_id.clone(),
                created_at: chrono::Local::now().to_rfc3339(),
                text: config.continue_text.clone(),
            };
            actions.push_back(bridge_action);
            info!(
                "Auto continue bridge action queued: event_id={}, text={}",
                event_id,
                config.continue_text
            );
        }
    }

    let event = AutoContinueBridgeEvent {
        id: event_id.clone(),
        received_at: chrono::Local::now().to_rfc3339(),
        event_type,
        source: incoming.source.unwrap_or_else(|| "windsurf-workbench".to_string()),
        url: incoming.url.or(incoming.location),
        message: truncate_chars(&message, 1200),
        matched,
        action,
    };

    {
        let mut events = state
            .events
            .lock()
            .map_err(|_| "自动继续事件锁异常".to_string())?;
        events.push_back(event.clone());
        while events.len() > MAX_RECENT_EVENTS {
            events.pop_front();
        }
    }

    if is_login_diagnostic {
        info!(
            "[WindsurfLoginDiagnostic] source={}, type={}, url={}, message={}",
            event.source,
            event.event_type,
            event.url.as_deref().unwrap_or(""),
            truncate_chars(&event.message, 800)
        );
    } else if event.matched {
        info!(
            "Auto continue bridge event: action={}, source={}, type={}, message={}",
            event.action,
            event.source,
            event.event_type,
            truncate_chars(&event.message, 200)
        );
    }

    Ok(event)
}

fn incoming_message_text(incoming: &IncomingAutoContinueBridgeEvent) -> String {
    let mut parts = Vec::new();
    if let Some(message) = incoming.message.as_deref() {
        parts.push(message.to_string());
    }
    if let Some(error) = incoming.error.as_deref() {
        parts.push(error.to_string());
    }
    if let Some(payload) = incoming.payload.as_ref() {
        parts.push(payload.to_string());
    }
    if let Some(url) = incoming.url.as_deref() {
        parts.push(url.to_string());
    }
    if let Some(location) = incoming.location.as_deref() {
        parts.push(location.to_string());
    }
    parts.join("\n")
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|position| position + 4)
}

fn extract_body(buffer: &[u8]) -> &[u8] {
    find_header_end(buffer)
        .map(|index| &buffer[index..])
        .unwrap_or_default()
}

fn parse_content_length(buffer: &[u8]) -> usize {
    let request = String::from_utf8_lossy(buffer);
    request
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            if name.eq_ignore_ascii_case("content-length") {
                value.trim().parse::<usize>().ok()
            } else {
                None
            }
        })
        .unwrap_or(0)
}

fn write_bridge_response(stream: &mut TcpStream, status: u16, body: Value) -> Result<(), String> {
    let status_text = match status {
        200 => "OK",
        204 => "No Content",
        400 => "Bad Request",
        404 => "Not Found",
        _ => "OK",
    };
    let body_text = if status == 204 {
        String::new()
    } else {
        serde_json::to_string(&body).map_err(|e| e.to_string())?
    };
    let response = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nAccess-Control-Allow-Headers: content-type\r\nConnection: close\r\n\r\n{}",
        status,
        status_text,
        body_text.as_bytes().len(),
        body_text
    );
    stream
        .write_all(response.as_bytes())
        .map_err(|e| e.to_string())
}
