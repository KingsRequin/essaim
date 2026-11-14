//! Interface locale pour Horizon (et tout client natif) : WebSocket `ws://127.0.0.1:28670`,
//! JSON, même forme que Prisme.
//!
//! Requête : `{"id": 1, "cmd": "get_state"}`. Réponse : `{"id": 1, "ok": true, "data": …}` ou
//! `{"id": 1, "ok": false, "error": "…"}`. Événement poussé sans `id` :
//! `{"event": "state", "data": …}` à chaque changement et au moins toutes les 2 s.
//!
//! Commandes :
//! - `get_state` : état (`paused`, `fullscreen`, `active`, `gpu`, `prisme`) et réglages (`config`) ;
//! - `set_paused` `{"paused": bool}` ;
//! - `set_config` `{"section": "orbite", "key": "rayon", "value": 24}` : écrit la valeur dans
//!   `config.toml` en gardant ses commentaires ; Essaim la recharge dans la seconde ;
//! - `open_config` : ouvre `config.toml` dans l'éditeur.
//!
//! Les pages web sont refusées (en-tête `Origin` présent), sauf Horizon (Tauri).

use serde::Deserialize;
use serde_json::{json, Value};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tungstenite::handshake::server::{ErrorResponse, Request as HsRequest, Response as HsResponse};
use tungstenite::{Message, WebSocket};

pub const PORT: u16 = 28670;
const MAX_CLIENTS: usize = 8;

/// Commandes qui doivent passer par la boucle principale.
pub enum Cmd {
    SetPaused(bool),
    OpenConfig,
}

/// État publié par la boucle principale.
#[derive(Default)]
pub struct Shared {
    state: Mutex<Value>,
    version: AtomicU64,
}

impl Shared {
    /// Publie l'état s'il a changé (les clients le reçoivent dans le quart de seconde).
    pub fn publish(&self, v: Value) {
        if let Ok(mut s) = self.state.lock() {
            if *s != v {
                *s = v;
                self.version.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
    fn snapshot(&self) -> (u64, Value) {
        let v = self.state.lock().map(|s| s.clone()).unwrap_or(Value::Null);
        (self.version.load(Ordering::Relaxed), v)
    }
}

#[derive(Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
enum Request {
    GetState,
    SetPaused { paused: bool },
    SetConfig { section: String, key: String, value: Value },
    OpenConfig,
}

pub fn start(shared: Arc<Shared>, tx: Sender<Cmd>) {
    let listener = match TcpListener::bind(("127.0.0.1", PORT)) {
        Ok(l) => l,
        Err(e) => {
            log::warn!("interface locale : port {PORT} indisponible ({e})");
            return;
        }
    };
    log::info!("interface locale : ws://127.0.0.1:{PORT}");
    let clients = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let _ = std::thread::Builder::new().name("essaim-ipc".into()).spawn(move || {
        for stream in listener.incoming().flatten() {
            // quelques clients au plus (Horizon, un outil) : pas de fils sans limite
            if clients.load(Ordering::Relaxed) >= MAX_CLIENTS {
                log::warn!("interface locale : trop de clients, connexion refusée");
                continue;
            }
            // la place est rendue quand le client se termine, ou tout de suite si son fil
            // ne démarre pas (la fermeture est alors détruite avec la garde)
            let slot = Slot::take(&clients);
            let (shared, tx) = (shared.clone(), tx.clone());
            let _ = std::thread::Builder::new().name("essaim-ipc-client".into()).spawn(move || {
                let _slot = slot;
                if let Err(e) = client(stream, &shared, &tx) {
                    log::debug!("interface locale : client fermé ({e})");
                }
            });
        }
    });
}

// signature imposée par tungstenite (rappel de la poignée de main)
#[allow(clippy::result_large_err)]
fn check_origin(req: &HsRequest, resp: HsResponse) -> Result<HsResponse, ErrorResponse> {
    match req.headers().get("origin").and_then(|o| o.to_str().ok()) {
        None | Some("tauri://localhost") | Some("http://tauri.localhost") => Ok(resp),
        Some(o) => {
            log::warn!("interface locale : connexion refusée depuis l'origine « {o} »");
            let mut r = ErrorResponse::new(Some("origine refusée".into()));
            *r.status_mut() = tungstenite::http::StatusCode::FORBIDDEN;
            Err(r)
        }
    }
}

/// Place de client occupée, rendue à la destruction.
struct Slot(Arc<std::sync::atomic::AtomicUsize>);

impl Slot {
    fn take(count: &Arc<std::sync::atomic::AtomicUsize>) -> Self {
        count.fetch_add(1, Ordering::Relaxed);
        Slot(count.clone())
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
}

fn client(stream: TcpStream, shared: &Shared, tx: &Sender<Cmd>) -> anyhow::Result<()> {
    // une poignée de main qui traîne ne doit pas garder sa place
    stream.set_read_timeout(Some(Duration::from_secs(3)))?;
    stream.set_write_timeout(Some(Duration::from_secs(3)))?;
    let mut ws: WebSocket<TcpStream> = tungstenite::accept_hdr(stream, check_origin)?;
    ws.get_mut().set_read_timeout(Some(Duration::from_millis(250)))?;
    let (mut sent_version, mut last_push) = (u64::MAX, Instant::now());
    loop {
        match ws.read() {
            Ok(Message::Text(t)) => {
                let reply = handle(t.as_str(), shared, tx);
                ws.send(Message::Text(reply.to_string().into()))?;
            }
            Ok(Message::Close(_)) => return Ok(()),
            Ok(_) => {}
            Err(tungstenite::Error::Io(e)) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {}
            Err(e) => return Err(e.into()),
        }
        let (version, state) = shared.snapshot();
        if version != sent_version || last_push.elapsed() >= Duration::from_secs(2) {
            ws.send(Message::Text(json!({ "event": "state", "data": state }).to_string().into()))?;
            sent_version = version;
            last_push = Instant::now();
        }
    }
}

fn handle(text: &str, shared: &Shared, tx: &Sender<Cmd>) -> Value {
    let raw: Value = serde_json::from_str(text).unwrap_or(Value::Null);
    let id = raw.get("id").cloned().unwrap_or(Value::Null);
    let result = serde_json::from_value::<Request>(raw).map_err(|e| e.to_string()).and_then(|r| match r {
        Request::GetState => Ok(shared.snapshot().1),
        Request::SetPaused { paused } => tx.send(Cmd::SetPaused(paused)).map(|_| Value::Null).map_err(|e| e.to_string()),
        Request::OpenConfig => tx.send(Cmd::OpenConfig).map(|_| Value::Null).map_err(|e| e.to_string()),
        Request::SetConfig { section, key, value } => crate::config::set_value(&section, &key, &value).map(|_| Value::Null),
    });
    match result {
        Ok(data) => json!({ "id": id, "ok": true, "data": data }),
        Err(e) => json!({ "id": id, "ok": false, "error": e }),
    }
}
