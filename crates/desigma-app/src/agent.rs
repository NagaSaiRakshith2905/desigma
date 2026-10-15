//! Authenticated loopback bridge. Only the UI thread touches editor state.
use crate::app::App;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::VecDeque,
    fs,
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
    time::{Duration, Instant},
};

const MAX_REQUEST: u64 = 1024 * 1024;
#[derive(Clone, Serialize)]
struct Endpoint {
    protocol_version: u32,
    address: String,
    token: String,
    document_id: String,
}
#[derive(Deserialize)]
struct StaleEndpoint {
    address: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    token: String,
    document_id: String,
    tool: String,
    args: Value,
    #[serde(default)]
    client_id: Option<String>,
}
struct Pending {
    tool: String,
    args: Value,
    client_id: String,
    deadline: Instant,
    cancelled: Arc<AtomicBool>,
    reply: mpsc::SyncSender<Value>,
}
struct Bridge {
    receiver: mpsc::Receiver<Pending>,
    stop: Arc<AtomicBool>,
    path: PathBuf,
    document_id: String,
}
#[derive(Default)]
struct RateLimit(Mutex<VecDeque<Instant>>);
impl RateLimit {
    fn allow(&self) -> bool {
        let now = Instant::now();
        let Ok(mut hits) = self.0.lock() else { return false };
        while hits.front().is_some_and(|t| now.duration_since(*t) > Duration::from_secs(60)) {
            hits.pop_front();
        }
        if hits.len() >= 60 {
            return false;
        }
        hits.push_back(now);
        true
    }
}
struct ActiveConnection(Arc<std::sync::atomic::AtomicUsize>);
impl Drop for ActiveConnection {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}
impl Drop for Bridge {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let _ = fs::remove_file(&self.path);
    }
}
#[derive(Default)]
pub struct AgentState {
    bridge: Option<Bridge>,
    revision_archive: desigma_agent_core::revision_history::RevisionArchive,
    pub controls_open: bool,
    pub paused: bool,
    pub revoked: bool,
    /// Every document starts with the bridge disabled and read-only.
    pub enabled: bool,
    pub connection_approved: bool,
    pub allow_edits: bool,
    pub audit: Vec<String>,
    pub clients: Vec<String>,
    changes: Vec<Value>,
    observed_revision: Option<u64>,
    latest_undo_client: Option<String>,
    pub last_activity: Option<Instant>,
    pub error: Option<String>,
}
fn error(code: &str, message: &str) -> Value {
    json!({"error":{"code":code,"message":message}})
}
fn tool_error(message: &str) -> Value {
    if let Some((code, detail)) = message.split_once(": ") {
        if matches!(
            code,
            "stale_revision"
                | "missing_node"
                | "invalid_operation"
                | "permission_denied"
                | "operation_conflict"
                | "unsupported_operation"
                | "revision_not_found"
                | "checkpoint_not_found"
                | "checkpoint_exists"
                | "checkpoint_limit"
                | "invalid_checkpoint_name"
                | "revision_exhausted"
                | "id_exhausted"
        ) {
            return error(code, detail);
        }
    }
    error("invalid_operation", message)
}
fn summarize_args(args: &Value) -> String {
    match args {
        Value::Object(fields) => format!("keys=[{}]", fields.keys().cloned().collect::<Vec<_>>().join(",")),
        Value::Array(items) => format!("items={}", items.len()),
        _ => "no structured args".into(),
    }
}
fn mcp_config(executable: &std::path::Path, workspace: &std::path::Path, vscode: bool) -> String {
    let server = if vscode {
        json!({"type":"stdio","command":executable,"args":["--workspace",workspace]})
    } else {
        json!({"command":executable,"args":["--workspace",workspace]})
    };
    let value = if vscode { json!({"servers":{"desigma":server}}) } else { json!({"mcpServers":{"desigma":server}}) };
    serde_json::to_string_pretty(&value).expect("MCP config JSON is serializable")
}
fn bundled_mcp_executable() -> Option<PathBuf> {
    let filename = if cfg!(windows) { "desigma-mcp.exe" } else { "desigma-mcp" };
    std::env::current_exe().ok()?.parent().map(|dir| dir.join(filename)).filter(|path| path.is_file())
}

fn private_directory(path: &std::path::Path) -> std::io::Result<()> {
    fs::create_dir_all(path)?;
    if fs::symlink_metadata(path)?.file_type().is_symlink() {
        return Err(std::io::Error::other("Endpoint directory cannot be a symlink"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    // Windows AppData inherits the user's ACL. Never publish in a shared override directory.
    #[cfg(windows)]
    {
        let status = std::process::Command::new("icacls")
            .arg(path)
            .args(["/inheritance:r", "/grant:r"])
            .arg(format!("{}:(OI)(CI)F", std::env::var("USERNAME").map_err(std::io::Error::other)?))
            .output()?;
        if !status.status.success() {
            return Err(std::io::Error::other("Cannot restrict endpoint directory ACL"));
        }
    }
    Ok(())
}
impl Bridge {
    fn start(ctx: egui::Context) -> Result<Self, String> {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).map_err(|e| e.to_string())?;
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        let document_id = uuid::Uuid::new_v4().to_string();
        let endpoint = Endpoint {
            protocol_version: desigma_agent_core::LIVE_PROTOCOL_VERSION,
            address: listener.local_addr().map_err(|e| e.to_string())?.to_string(),
            token: format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple()),
            document_id: document_id.clone(),
        };
        let directory = desigma_library::Library::default_root().join("agent-endpoints");
        private_directory(&directory).map_err(|e| e.to_string())?;
        // Remove crash leftovers while preserving endpoints whose editor still answers.
        if let Ok(entries) = fs::read_dir(&directory) {
            for entry in entries.flatten() {
                let stale_path = entry.path();
                if stale_path.extension().and_then(|x| x.to_str()) != Some("json") {
                    continue;
                }
                let stale = fs::read(&stale_path).ok().and_then(|bytes| serde_json::from_slice::<StaleEndpoint>(&bytes).ok());
                let alive = stale.as_ref().and_then(|e| e.address.parse().ok()).is_some_and(|address: std::net::SocketAddr| {
                    address.ip().is_loopback() && TcpStream::connect_timeout(&address, Duration::from_millis(100)).is_ok()
                });
                if !alive {
                    let _ = fs::remove_file(stale_path);
                }
            }
        }
        let path = directory.join(format!("{}-{document_id}.json", std::process::id()));
        let mut options = fs::OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&path).map_err(|e| e.to_string())?;
        file.write_all(&serde_json::to_vec(&endpoint).map_err(|e| e.to_string())?).and_then(|_| file.sync_all()).map_err(|e| e.to_string())?;
        let (sender, receiver) = mpsc::sync_channel(8);
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let rate = Arc::new(RateLimit::default());
        let shared_rate = rate.clone();
        let active = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let shared_active = active.clone();
        std::thread::spawn(move || {
            while !stopped.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((stream, peer)) if peer.ip().is_loopback() => {
                        if shared_active.fetch_add(1, Ordering::AcqRel) >= 8 {
                            shared_active.fetch_sub(1, Ordering::AcqRel);
                            let mut stream = stream;
                            let _ = writeln!(stream, "{}", error("operation_conflict", "Too many active local agent clients"));
                            continue;
                        }
                        let endpoint = endpoint.clone();
                        let sender = sender.clone();
                        let stopped = stopped.clone();
                        let ctx = ctx.clone();
                        let rate = shared_rate.clone();
                        let active = shared_active.clone();
                        let peer = peer.to_string();
                        std::thread::spawn(move || {
                            let _active = ActiveConnection(active);
                            serve(stream, &endpoint, &sender, &stopped, &ctx, &rate, &peer);
                        });
                    }
                    Ok(_) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(10)),
                    Err(_) => break,
                }
            }
        });
        Ok(Self { receiver, stop, path, document_id })
    }
}
fn token_eq(a: &str, b: &str) -> bool {
    let mut diff = a.len() ^ b.len();
    for i in 0..a.len().max(b.len()) {
        diff |= (a.as_bytes().get(i).copied().unwrap_or(0) ^ b.as_bytes().get(i).copied().unwrap_or(0)) as usize;
    }
    diff == 0
}
fn serve(
    mut stream: TcpStream,
    endpoint: &Endpoint,
    sender: &mpsc::SyncSender<Pending>,
    stopped: &AtomicBool,
    ctx: &egui::Context,
    rate: &RateLimit,
    peer: &str,
) {
    let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
    let result = (|| {
        use std::io::Read;
        let mut line = String::new();
        BufReader::new((&mut stream).take(MAX_REQUEST + 1)).read_line(&mut line).map_err(|_| error("invalid_operation", "Cannot read request"))?;
        if line.len() as u64 > MAX_REQUEST || !line.ends_with('\n') {
            return Err(error("invalid_operation", "Request exceeds limit or is incomplete"));
        }
        let request: Request = serde_json::from_str(&line).map_err(|_| error("invalid_operation", "Invalid request"))?;
        if !token_eq(&request.token, &endpoint.token) {
            return Err(error("permission_denied", "Authentication failed"));
        }
        if !rate.allow() {
            return Err(error("operation_conflict", "Local agent request rate limit exceeded"));
        }
        if request.document_id != endpoint.document_id {
            return Err(error("operation_conflict", "Document is no longer open"));
        }
        let (reply, receive) = mpsc::sync_channel(1);
        let cancelled = Arc::new(AtomicBool::new(false));
        let deadline = Instant::now() + Duration::from_secs(10);
        let client_id = request
            .client_id
            .map(|s| s.chars().filter(|c| !c.is_control()).take(64).collect::<String>())
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| format!("loopback {peer}"));
        sender
            .try_send(Pending { tool: request.tool, args: request.args, client_id, deadline, cancelled: cancelled.clone(), reply })
            .map_err(|_| error("operation_conflict", "Editor queue is full"))?;
        ctx.request_repaint();
        loop {
            match receive.recv_timeout(Duration::from_millis(50)) {
                Ok(value) => return Ok(value),
                Err(mpsc::RecvTimeoutError::Disconnected) => return Err(error("operation_conflict", "Editor closed")),
                Err(_) => {}
            }
            let mut peek = [0u8];
            let _ = stream.set_nonblocking(true);
            let disconnected = matches!(stream.peek(&mut peek), Ok(0));
            let _ = stream.set_nonblocking(false);
            if stopped.load(Ordering::Acquire) || disconnected || Instant::now() >= deadline {
                cancelled.store(true, Ordering::Release);
                return Err(error("operation_conflict", "Request cancelled, disconnected, or timed out"));
            }
        }
    })()
    .unwrap_or_else(|value| value);
    let _ = writeln!(stream, "{result}");
}
impl App {
    fn record_agent_change(&mut self, source: &str, summary: &str) {
        let revision = self.doc.revision;
        self.agent.changes.push(json!({"revision":revision,"source":source,"summary":summary}));
        if self.agent.changes.len() > 128 {
            self.agent.changes.remove(0);
        }
        self.agent.audit.insert(0, format!("{source} change · {summary} · rev {revision}"));
        self.agent.audit.truncate(20);
        self.agent.observed_revision = Some(revision);
        if source == "editor" {
            self.agent.latest_undo_client = None;
        }
    }
    fn latest_agent_undo_label(&self) -> Option<String> {
        let client = self.agent.latest_undo_client.as_ref()?;
        let history_label = self.hist.undo_label()?;
        (self.hist.can_undo() && history_label.starts_with(&format!("{client}: "))).then(|| format!("Undo latest agent edit ({client})"))
    }
    fn undo_latest_agent_edit(&mut self) {
        if self.latest_agent_undo_label().is_some() {
            self.undo();
            self.agent.latest_undo_client = None;
        }
    }
    pub fn revoke_agent(&mut self) {
        self.agent.bridge = None;
        self.agent.revoked = true;
        self.agent.enabled = false;
        self.agent.connection_approved = false;
        self.agent.allow_edits = false;
    }
    pub fn poll_agent(&mut self, ctx: &egui::Context) {
        if self.home.is_some() || self.history.viewing.is_some() {
            self.agent.bridge = None;
            self.agent.enabled = false;
            self.agent.connection_approved = false;
            self.agent.allow_edits = false;
            self.agent.observed_revision = Some(self.doc.revision);
            return;
        }
        let mut capture_revision = false;
        if let Some(previous) = self.agent.observed_revision {
            if self.doc.revision > previous {
                self.record_agent_change("editor", "document revision changed in the editor");
                capture_revision = true;
            }
        } else {
            self.agent.observed_revision = Some(self.doc.revision);
            capture_revision = true;
        }
        if capture_revision {
            if let Err(error) = self.agent.revision_archive.capture(&self.doc) {
                // A same-revision document replacement begins a new checkpoint
                // lineage; never compare revisions from two different documents.
                if error.starts_with("revision_conflict:") {
                    let _ = self.agent.revision_archive.reset(&self.doc);
                }
            }
        }
        if self.agent.bridge.is_none() && self.agent.enabled && !self.agent.revoked && self.agent.error.is_none() {
            match Bridge::start(ctx.clone()) {
                Ok(bridge) => {
                    self.agent.audit.clear();
                    self.agent.changes.clear();
                    self.agent.clients.clear();
                    self.agent.observed_revision = Some(self.doc.revision);
                    self.agent.latest_undo_client = None;
                    self.agent.bridge = Some(bridge);
                }
                Err(e) => self.agent.error = Some(e),
            }
        }
        let pending = self.agent.bridge.as_ref().and_then(|b| b.receiver.try_recv().ok());
        if let Some(request) = pending {
            if request.cancelled.load(Ordering::Acquire) || Instant::now() >= request.deadline {
                return;
            }
            let busy = self.scrub.is_some()
                || self.dialog.is_some()
                || self.proto.present.is_some()
                || !matches!(self.canvas.inter, crate::canvas::Interaction::None)
                || self.canvas.pen.is_some()
                || self.canvas.builder.is_some()
                || self.canvas.vedit.is_some()
                || self.canvas.text.is_some();
            // Read operations must also avoid transient drag/preview state with an old committed revision.
            let is_write = !(request.tool.starts_with("desigma_get_")
                || request.tool.starts_with("desigma_list_")
                || matches!(request.tool.as_str(), "desigma_generate_code" | "desigma_render_node" | "desigma_diff_revisions"));
            let args_summary = summarize_args(&request.args);
            let result = if !self.agent.connection_approved {
                self.agent.controls_open = true;
                error("permission_denied", "Approve the local agent connection in Agent access, then retry")
            } else if is_write && !self.agent.allow_edits {
                self.agent.controls_open = true;
                error("permission_denied", "Agent access is read-only; enable edits in Agent access")
            } else if self.agent.paused {
                error("permission_denied", "Agent access is paused")
            } else if busy {
                error("operation_conflict", "Finish the current UI edit before accessing the document")
            } else if request.tool == "desigma_get_live_context" {
                let tl = self.view.offset;
                let br = self.view.to_page(self.canvas_rect.min, self.canvas_rect.max);
                let bounds = desigma_doc::Rect::new(tl.x, tl.y, br.x - tl.x, br.y - tl.y);
                let visible = self
                    .doc
                    .nodes
                    .keys()
                    .copied()
                    .filter(|id| self.doc.page_of(*id) == Some(self.page))
                    .filter(|id| crate::util::world_aabb(&self.doc, *id).intersects(&bounds))
                    .take(1001)
                    .collect::<Vec<_>>();
                let truncated = visible.len() > 1000;
                let visible = visible.into_iter().take(1000).collect::<Vec<_>>();
                json!({"result":{"document_id":self.agent.bridge.as_ref().map(|b|&b.document_id),"revision":self.doc.revision,"page_id":self.page,"selection":self.selection,"hovered_node_id":self.hover.or(self.hover_layer),"viewport":{"offset":self.view.offset,"zoom":self.view.zoom,"node_ids":visible,"truncated":truncated}}})
            } else if request.tool == "desigma_get_changes" {
                match request.args.as_object().filter(|o| o.len() == 1).and_then(|_| request.args.get("since_revision")).and_then(Value::as_u64) {
                    Some(since) if since <= self.doc.revision => {
                        let oldest = self.agent.changes.first().and_then(|c| c.get("revision")).and_then(Value::as_u64).unwrap_or(self.doc.revision);
                        let truncated = !self.agent.changes.is_empty() && since.saturating_add(1) < oldest;
                        let changes = self.agent.changes.iter().filter(|c| c["revision"].as_u64().is_some_and(|r| r > since)).cloned().collect::<Vec<_>>();
                        json!({"result":{"changes":changes,"current_revision":self.doc.revision,"truncated":truncated}})
                    }
                    Some(_) => error("stale_revision", "since_revision is ahead of the current document"),
                    None => error("invalid_operation", "since_revision must be an unsigned revision number"),
                }
            } else if request.tool == "desigma_set_selection" {
                let ids = request
                    .args
                    .as_object()
                    .filter(|o| o.len() == 1)
                    .and_then(|_| request.args.get("node_ids"))
                    .and_then(Value::as_array)
                    .and_then(|xs| xs.iter().map(|v| v.as_u64().map(desigma_doc::NodeId)).collect::<Option<Vec<_>>>());
                match ids {
                    Some(ids) if ids.iter().all(|id| self.doc.get(*id).is_some() && self.doc.page_of(*id) == Some(self.page)) => {
                        self.selection = ids;
                        json!({"result":{"selection":self.selection,"revision":self.doc.revision}})
                    }
                    _ => error("invalid_operation", "node_ids must reference existing nodes on the active page"),
                }
            } else if request.tool == "desigma_set_page" {
                match request
                    .args
                    .as_object()
                    .filter(|o| o.len() == 1)
                    .and_then(|_| request.args.get("page_id"))
                    .and_then(Value::as_u64)
                    .map(desigma_doc::NodeId)
                    .filter(|id| self.doc.pages.contains(id))
                {
                    Some(page) => {
                        self.page = page;
                        self.selection.clear();
                        self.hover = None;
                        self.hover_layer = None;
                        self.fit_all();
                        json!({"result":{"page_id":self.page,"revision":self.doc.revision}})
                    }
                    None => error("invalid_operation", "page_id must reference an existing page"),
                }
            } else if request.tool == "desigma_zoom_to_node" {
                match request
                    .args
                    .as_object()
                    .filter(|o| o.len() == 1)
                    .and_then(|_| request.args.get("node_id"))
                    .and_then(Value::as_u64)
                    .map(desigma_doc::NodeId)
                    .filter(|id| self.doc.get(*id).is_some())
                {
                    Some(id) => {
                        if let Some(page) = self.doc.page_of(id) {
                            self.page = page;
                            self.selection.retain(|n| self.doc.page_of(*n) == Some(page));
                        }
                        self.view = crate::app::View::fit(crate::util::world_aabb(&self.doc, id), self.canvas_rect.size());
                        self.changed();
                        json!({"result":{"node_id":id,"page_id":self.page,"viewport":{"offset":self.view.offset,"zoom":self.view.zoom},"revision":self.doc.revision}})
                    }
                    None => error("invalid_operation", "node_id must reference an existing node"),
                }
            } else {
                let before_doc = self.doc.clone();
                let before_hist = self.hist.clone();
                let before_archive = self.agent.revision_archive.clone();
                let mut engine = desigma_agent_core::engine::Engine {
                    doc: std::mem::take(&mut self.doc),
                    history: std::mem::take(&mut self.hist),
                    revision_archive: std::mem::take(&mut self.agent.revision_archive),
                };
                let revision = engine.doc.revision;
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| engine.call(&request.tool, request.args))).unwrap_or_else(|_| {
                    engine.doc = before_doc.clone();
                    engine.history = before_hist.clone();
                    engine.revision_archive = before_archive.clone();
                    Err("operation_conflict: Editor operation failed safely".into())
                });
                self.doc = engine.doc;
                self.hist = engine.history;
                self.agent.revision_archive = engine.revision_archive;
                if self.doc.revision != revision {
                    self.record_agent_change("agent", &request.tool);
                    if request.tool != "desigma_undo" && request.tool != "desigma_redo" && request.tool != "desigma_restore_checkpoint" {
                        self.hist.label_last(format!("{}: {}", request.client_id, request.tool.trim_start_matches("desigma_")));
                        self.agent.latest_undo_client = Some(request.client_id.clone());
                    } else {
                        self.agent.latest_undo_client = None;
                    }
                    self.selection.retain(|id| self.doc.get(*id).is_some());
                    self.changed();
                    self.fonts.load_for(&self.doc);
                    self.show_toast(if request.tool == "desigma_restore_checkpoint" { "Checkpoint restored" } else { "Agent edit (undo available)" }, false);
                }
                match result {
                    Ok(value) => json!({"result":value}),
                    Err(message) => serde_json::from_str::<Value>(&message)
                        .ok()
                        .filter(|v| v.get("code").is_some())
                        .map(|e| json!({"error":e}))
                        .unwrap_or_else(|| tool_error(&message)),
                }
            };
            if self.agent.last_activity.is_none() {
                self.agent.controls_open = true;
            }
            self.agent.clients.retain(|id| id != &request.client_id);
            self.agent.clients.insert(0, request.client_id.clone());
            self.agent.clients.truncate(8);
            let outcome = result.get("error").and_then(|e| e.get("code")).and_then(Value::as_str).unwrap_or("ok");
            self.agent.audit.insert(
                0,
                format!(
                    "{} · {} · {} {} · {} · rev {}",
                    request.client_id,
                    if is_write { "write" } else { "read" },
                    request.tool,
                    args_summary,
                    outcome,
                    self.doc.revision
                ),
            );
            self.agent.audit.truncate(20);
            self.agent.last_activity = Some(Instant::now());
            let _ = request.reply.send(result);
        }
        self.agent.observed_revision = Some(self.doc.revision);
    }
    pub fn agent_controls(&mut self, ctx: &egui::Context) {
        // Keep the editor's existing header/canvas hit targets unchanged until
        // the user opts into local agent access. The access controls remain
        // available from the Agent access menu item while the bridge is off.
        if !self.agent.enabled && !self.agent.revoked {
            return;
        }
        egui::Area::new(egui::Id::new("agent-status-indicator")).anchor(egui::Align2::RIGHT_TOP, egui::vec2(-12.0, 12.0)).show(ctx, |ui| {
            let status = if !self.agent.enabled {
                "Agent: off"
            } else if self.agent.paused {
                "Agent: paused"
            } else if self.agent.allow_edits {
                "Agent: edits allowed"
            } else if self.agent.connection_approved {
                "Agent: read-only"
            } else {
                "Agent: approval needed"
            };
            ui.horizontal(|ui| {
                if ui.small_button(status).clicked() {
                    self.agent.controls_open = true;
                }
                if self.agent.enabled && self.agent.connection_approved {
                    let action = if self.agent.paused { "Resume" } else { "Pause" };
                    if ui.small_button(action).clicked() {
                        self.agent.paused = !self.agent.paused;
                    }
                }
            });
        });
        if !self.agent.controls_open {
            return;
        }
        let mut open = true;
        egui::Window::new("Agent access")
            .open(&mut open)
            .id(egui::Id::new("agent-access"))
            .default_open(true)
            .default_pos(egui::pos2(12.0, 80.0))
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(if self.agent.revoked {
                    "Access revoked"
                } else if self.agent.paused {
                    "Paused"
                } else if self.agent.last_activity.is_some_and(|t| t.elapsed() < Duration::from_secs(30)) {
                    "Agent active"
                } else {
                    "Ready for local agent"
                });
                if let Some(e) = &self.agent.error {
                    ui.label(e);
                }
                if self.agent.revoked {
                    if ui.button("Enable agent access").clicked() {
                        self.agent.revoked = false;
                        self.agent.error = None;
                        self.agent.paused = false;
                        self.agent.enabled = true;
                        self.agent.connection_approved = false;
                    }
                } else if !self.agent.enabled {
                    ui.label("Local agent access is off for this document.");
                    if ui.button("Enable local agent access…").clicked() {
                        self.agent.enabled = true;
                        self.agent.revoked = false;
                        self.agent.error = None;
                    }
                } else {
                    if !self.agent.connection_approved {
                        ui.label("Approve a local MCP client to allow read-only access.");
                        if ui.button("Approve connection (read-only)").clicked() {
                            self.agent.connection_approved = true;
                        }
                    } else {
                        ui.label("Connected • read-only unless edits are enabled");
                        ui.checkbox(&mut self.agent.allow_edits, "Allow document edits");
                    }
                    ui.checkbox(&mut self.agent.paused, "Pause agent access");
                    if ui.button("Revoke access").clicked() {
                        self.revoke_agent();
                    }
                }
                ui.separator();
                if let Some(executable) = bundled_mcp_executable() {
                    let workspace = desigma_library::Library::default_root();
                    ui.label("Copy a local MCP launch config (no access token is included):");
                    ui.horizontal(|ui| {
                        if ui.button("Claude Desktop / Claude Code / Cursor").clicked() {
                            ctx.copy_text(mcp_config(&executable, &workspace, false));
                        }
                        if ui.button("VS Code").clicked() {
                            ctx.copy_text(mcp_config(&executable, &workspace, true));
                        }
                    });
                } else {
                    ui.label("Copy MCP config is unavailable: desigma-mcp is not beside the editor executable.");
                }
                ui.separator();
                if !self.agent.clients.is_empty() {
                    ui.label(format!("Recent clients: {}", self.agent.clients.join(", ")));
                }
                ui.label("Recent agent activity");
                if let Some(label) = self.latest_agent_undo_label() {
                    if ui.button(label).clicked() {
                        self.undo_latest_agent_edit();
                    }
                }
                for entry in &self.agent.audit {
                    ui.label(entry);
                }
            });
        self.agent.controls_open = open;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use desigma_doc::{Document, Node, Rect};

    fn app_with_queue() -> (App, mpsc::SyncSender<Pending>, egui::Context, desigma_doc::NodeId) {
        let mut doc = Document::new("Agent IPC test");
        let page = doc.add_page("Page");
        let id = doc.alloc_id();
        doc.insert(page, Node::rectangle(id, Rect::new(0.0, 0.0, 10.0, 10.0))).unwrap();
        let mut app = App::new(doc);
        let (sender, receiver) = mpsc::sync_channel(8);
        // No discovery files or global environment variables: parallel tests
        // each own their queue and, for socket checks, an ephemeral port.
        app.agent.bridge = Some(Bridge {
            receiver,
            stop: Arc::new(AtomicBool::new(false)),
            path: std::env::temp_dir().join(format!("desigma-agent-nonexistent-{}.json", uuid::Uuid::new_v4())),
            document_id: "test-document".into(),
        });
        app.agent.connection_approved = true;
        app.agent.allow_edits = true;
        app.agent.enabled = true;
        (app, sender, egui::Context::default(), id)
    }

    fn queue(sender: &mpsc::SyncSender<Pending>, tool: &str, args: Value, cancelled: bool, expired: bool) -> mpsc::Receiver<Value> {
        let (reply, receive) = mpsc::sync_channel(1);
        sender
            .send(Pending {
                tool: tool.into(),
                args,
                client_id: "test client".into(),
                deadline: if expired { Instant::now() - Duration::from_secs(1) } else { Instant::now() + Duration::from_secs(10) },
                cancelled: Arc::new(AtomicBool::new(cancelled)),
                reply,
            })
            .unwrap();
        receive
    }

    #[test]
    fn live_atomic_batch_is_visible_and_undoable_from_editor() {
        let (mut app, sender, ctx, id) = app_with_queue();
        let before = app.doc.nodes.clone();
        let response = queue(
            &sender,
            "desigma_apply_ops",
            json!({"expected_revision":app.doc.revision,"ops":[{"op":"set_properties","id":id,"properties":{"name":"Agent rectangle","width":70}}]}),
            false,
            false,
        );
        app.poll_agent(&ctx);
        let result = response.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(result.get("result").is_some(), "{result}");
        assert_eq!(app.doc.get(id).unwrap().name, "Agent rectangle");
        assert_eq!(app.doc.get(id).unwrap().size.x, 70.0);
        assert_eq!(app.hist.undo_label(), Some("test client: apply_ops"));
        assert_eq!(app.latest_agent_undo_label().as_deref(), Some("Undo latest agent edit (test client)"));
        app.undo_latest_agent_edit();
        assert_eq!(app.doc.nodes, before);
        assert!(app.latest_agent_undo_label().is_none());
        assert!(!app.hist.can_undo());
        assert!(app.hist.can_redo());
    }

    #[test]
    fn paused_busy_cancelled_expired_and_revoked_requests_cannot_edit() {
        let (mut app, sender, ctx, id) = app_with_queue();
        let before = app.doc.to_json().unwrap();
        let args = json!({"expected_revision":app.doc.revision,"ops":[{"op":"set_properties","id":id,"properties":{"width":70}}]});
        app.agent.paused = true;
        let response = queue(&sender, "desigma_apply_ops", args.clone(), false, false);
        app.poll_agent(&ctx);
        assert_eq!(response.recv_timeout(Duration::from_secs(1)).unwrap()["error"]["code"], "permission_denied");
        app.agent.paused = false;
        // An active canvas gesture must reject access to transient state.
        app.canvas.inter = crate::canvas::Interaction::Pan;
        let response = queue(&sender, "desigma_apply_ops", args.clone(), false, false);
        app.poll_agent(&ctx);
        assert_eq!(response.recv_timeout(Duration::from_secs(1)).unwrap()["error"]["code"], "operation_conflict");
        app.canvas.inter = crate::canvas::Interaction::None;
        for (cancelled, expired) in [(true, false), (false, true)] {
            let response = queue(&sender, "desigma_apply_ops", args.clone(), cancelled, expired);
            app.poll_agent(&ctx);
            assert!(response.recv_timeout(Duration::from_secs(1)).is_err());
        }
        let response = queue(&sender, "desigma_apply_ops", args, false, false);
        app.revoke_agent();
        app.poll_agent(&ctx);
        assert!(response.recv_timeout(Duration::from_secs(1)).is_err());
        assert!(app.agent.bridge.is_none());
        assert_eq!(app.doc.to_json().unwrap(), before);
        assert!(!app.hist.can_undo());
    }

    #[test]
    fn bridge_is_opt_in_and_approved_connection_is_read_only_by_default() {
        let mut doc = Document::new("Agent opt in");
        doc.add_page("Page");
        let app = App::new(doc);
        assert!(!app.agent.enabled);
        assert!(!app.agent.allow_edits);
        let (mut app, sender, ctx, id) = app_with_queue();
        app.agent.allow_edits = false;
        let before = app.doc.to_json().unwrap();
        let response = queue(
            &sender,
            "desigma_apply_ops",
            json!({"expected_revision":app.doc.revision,"ops":[{"op":"set_properties","id":id,"properties":{"width":70}}]}),
            false,
            false,
        );
        app.poll_agent(&ctx);
        assert_eq!(response.recv_timeout(Duration::from_secs(1)).unwrap()["error"]["code"], "permission_denied");
        assert_eq!(app.doc.to_json().unwrap(), before);
    }

    #[test]
    fn live_navigation_selection_and_context_do_not_change_document_history() {
        let (mut app, sender, ctx, id) = app_with_queue();
        let second_page = app.doc.add_page("Second page");
        let rev = app.doc.revision;
        let undo = app.hist.can_undo();
        let response = queue(&sender, "desigma_set_selection", json!({"node_ids":[id]}), false, false);
        app.poll_agent(&ctx);
        assert_eq!(response.recv_timeout(Duration::from_secs(1)).unwrap()["result"]["selection"][0], id.0);
        assert_eq!(app.selection, vec![id]);
        assert_eq!(app.doc.revision, rev);
        assert_eq!(app.hist.can_undo(), undo);

        let response = queue(&sender, "desigma_set_page", json!({"page_id":second_page.0}), false, false);
        app.poll_agent(&ctx);
        assert_eq!(response.recv_timeout(Duration::from_secs(1)).unwrap()["result"]["page_id"], second_page.0);
        assert_eq!(app.page, second_page);
        assert_eq!(app.doc.revision, rev);
        assert_eq!(app.hist.can_undo(), undo);

        let response = queue(&sender, "desigma_zoom_to_node", json!({"node_id":id.0}), false, false);
        app.poll_agent(&ctx);
        let zoomed = response.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(zoomed["result"]["page_id"], app.doc.page_of(id).unwrap().0);
        assert_eq!(app.page, app.doc.page_of(id).unwrap());
        assert_eq!(app.doc.revision, rev);
        assert_eq!(app.hist.can_undo(), undo);

        let response = queue(&sender, "desigma_get_live_context", json!({}), false, false);
        app.poll_agent(&ctx);
        let context = response.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(context["result"].get("hovered_node_id").is_some());
        assert!(context["result"]["viewport"]["node_ids"].is_array());
        assert_eq!(context["result"]["revision"], rev);
        assert!(app.agent.audit[0].contains("test client"));
    }

    #[test]
    fn change_feed_reports_editor_and_agent_revisions_with_bounded_cursor() {
        let (mut app, sender, ctx, id) = app_with_queue();
        let initial = app.doc.revision;
        app.agent.observed_revision = Some(initial);
        app.doc.revision += 1;
        let response = queue(&sender, "desigma_get_changes", json!({"since_revision":initial}), false, false);
        app.poll_agent(&ctx);
        let result = response.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(result["result"]["changes"][0]["source"], "editor");
        assert_eq!(result["result"]["current_revision"], initial + 1);

        let current = app.doc.revision;
        let response = queue(
            &sender,
            "desigma_apply_ops",
            json!({"expected_revision":current,"ops":[{"op":"set_properties","id":id,"properties":{"name":"Feed update"}}]}),
            false,
            false,
        );
        app.poll_agent(&ctx);
        let _ = response.recv_timeout(Duration::from_secs(1)).unwrap();
        let response = queue(&sender, "desigma_get_changes", json!({"since_revision":initial + 1}), false, false);
        app.poll_agent(&ctx);
        let result = response.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(result["result"]["changes"].as_array().unwrap().iter().any(|event| event["source"] == "agent"));
        assert!(app.agent.audit.iter().any(|line| line.contains("editor change") && line.contains(&format!("rev {}", initial + 1))));
    }

    #[test]
    fn auth_comparison_requires_exact_token() {
        assert!(token_eq("secret", "secret"));
        assert!(!token_eq("secret", "secre\u{0}"));
        assert!(!token_eq("secret", "secret\u{0}"));
        assert!(!token_eq("secret", "secrex"));
    }

    #[test]
    fn copied_mcp_configs_use_supported_shapes_and_never_include_live_token() {
        let exe = std::path::Path::new("/opt/desigma/desigma-mcp");
        let workspace = std::path::Path::new("/Users/test/designs");
        let claude: Value = serde_json::from_str(&mcp_config(exe, workspace, false)).unwrap();
        assert_eq!(claude["mcpServers"]["desigma"]["command"], exe.to_str().unwrap());
        assert_eq!(claude["mcpServers"]["desigma"]["args"][0], "--workspace");
        assert!(claude["mcpServers"]["desigma"].get("token").is_none());
        let vscode: Value = serde_json::from_str(&mcp_config(exe, workspace, true)).unwrap();
        assert_eq!(vscode["servers"]["desigma"]["type"], "stdio");
        assert_eq!(vscode["servers"]["desigma"]["command"], exe.to_str().unwrap());
        assert!(vscode["mcpServers"].is_null());
    }

    fn socket_request(request: Value, drop_client: bool) -> (Option<Value>, Option<Pending>) {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let (sender, receiver) = mpsc::sync_channel(8);
        let thread = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let endpoint = Endpoint { protocol_version: 1, address: address.to_string(), token: "secret".into(), document_id: "document".into() };
            serve(stream, &endpoint, &sender, &AtomicBool::new(false), &egui::Context::default(), &RateLimit::default(), "127.0.0.1:0");
        });
        let mut client = TcpStream::connect(address).unwrap();
        client.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        writeln!(client, "{request}").unwrap();
        let result = if drop_client {
            None
        } else {
            let mut response = String::new();
            BufReader::new(&mut client).read_line(&mut response).unwrap();
            Some(serde_json::from_str(&response).unwrap())
        };
        drop(client);
        thread.join().unwrap();
        (result, receiver.try_recv().ok())
    }

    #[test]
    fn transport_rejects_wrong_authentication_and_document_before_queueing() {
        let (result, queued) = socket_request(json!({"token":"wrong","document_id":"document","tool":"desigma_get_document","args":{}}), false);
        assert_eq!(result.unwrap()["error"]["code"], "permission_denied");
        assert!(queued.is_none());
        let (result, queued) = socket_request(json!({"token":"secret","document_id":"closed-document","tool":"desigma_get_document","args":{}}), false);
        assert_eq!(result.unwrap()["error"]["code"], "operation_conflict");
        assert!(queued.is_none());
    }

    #[test]
    fn disconnected_client_cancels_queued_write() {
        let (_, queued) = socket_request(json!({"token":"secret","document_id":"document","tool":"desigma_apply_ops","args":{}}), true);
        assert!(queued.expect("authenticated request was queued").cancelled.load(Ordering::Acquire));
    }
}
