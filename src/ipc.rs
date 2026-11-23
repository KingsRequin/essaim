//! Interface locale pour Horizon (et tout client natif) : WebSocket `ws://127.0.0.1:28670`,
//! JSON, même forme que Prisme.
//!
//! Requête : `{"id": 1, "cmd": "get_state"}`. Réponse : `{"id": 1, "ok": true, "data": …}` ou
//! `{"id": 1, "ok": false, "error": "…"}`. Événement poussé sans `id` :
//! `{"event": "state", "data": …}` à chaque changement et au moins toutes les 2 s.
//!
//! Commandes :
//! - `get_state` : état (`paused`, `fullscreen`, `active`, `gpu`, `prisme`), preset en place
//!   (`preset` : son nom, ou `null` si les réglages ne sont ceux d'aucun preset) et réglages
//!   (`config`) ;
//! - `set_paused` `{"paused": bool}` ;
//! - `set_config` `{"section": "orbite", "key": "rayon", "value": 24}` : écrit la valeur dans
//!   `config.toml` en gardant ses commentaires ; Essaim la recharge dans la seconde ;
//! - `list_presets` : presets fournis puis ceux de l'utilisateur,
//!   `[{"name": "Discret", "builtin": true, "active": false}]` (relus à chaque demande) ;
//! - `apply_preset` `{"name": "Discret"}` : applique le preset, comme le menu de l'icône ;
//! - `open_settings` : ouvre la fenêtre des réglages (ou la ramène devant) ;
//! - `open_config` : ouvre `config.toml` dans l'éditeur.
//!
//! Clés d'avant les modulations et les déclencheurs, encore lues et écrites par Horizon
//! (`orbite.vitesse_basse`, `orbite.eloignement_grave`, `fissures.actives`) : `config` les
//! donne, calculées, et `set_config` les traduit (`ALIAS`).
//!
//! Les pages web et les applis web locales (Tauri, Electron) sont refusées : tout en-tête
//! `Origin` ferme la connexion. `set_config` ne touche pas aux réglages qui font télécharger ou
//! changer de comportement en arrière-plan (`SECTIONS_PROTEGEES`).

use crate::config::{Action, Cible, Config, Declencheur, Evenement, Groupe, Modulation, Source};
use crate::modulation;
use crate::profils::{self, Profile};
use serde::Deserialize;
use serde_json::{json, Value};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tungstenite::handshake::server::{ErrorResponse, Request as HsRequest, Response as HsResponse};
use tungstenite::protocol::WebSocketConfig;
use tungstenite::{Message, WebSocket};

pub const PORT: u16 = 28670;
const MAX_CLIENTS: usize = 8;
/// Taille maximale d'un message reçu (une commande JSON tient en quelques centaines d'octets).
const MESSAGE_MAX: usize = 64 * 1024;
/// Un client muet depuis ce délai est fermé (il libère sa place).
const INACTIVITE_MAX: Duration = Duration::from_secs(60);
/// Sections que `set_config` refuse : mises à jour, dépôt de la bibliothèque, règles
/// automatiques et pauses. Elles ne se changent que dans la fenêtre des réglages ou le fichier.
const SECTIONS_PROTEGEES: [&str; 4] = ["mise_a_jour", "bibliotheque", "automatique", "pause"];

/// Commandes qui doivent passer par la boucle principale.
pub enum Cmd {
    SetPaused(bool),
    OpenConfig,
    OpenSettings,
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
    ListPresets,
    ApplyPreset { name: String },
    OpenSettings,
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
    match req.headers().get("origin") {
        None => Ok(resp),
        Some(o) => {
            let o = o.to_str().unwrap_or("?");
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
    let limites = WebSocketConfig::default().max_message_size(Some(MESSAGE_MAX)).max_frame_size(Some(MESSAGE_MAX));
    let mut ws: WebSocket<TcpStream> = tungstenite::accept_hdr_with_config(stream, check_origin, Some(limites))?;
    ws.get_mut().set_read_timeout(Some(Duration::from_millis(250)))?;
    let (mut sent_version, mut last_push, mut dernier_message) = (u64::MAX, Instant::now(), Instant::now());
    loop {
        if dernier_message.elapsed() >= INACTIVITE_MAX {
            let _ = ws.close(None);
            return Ok(());
        }
        let lu = ws.read();
        if lu.is_ok() {
            dernier_message = Instant::now();
        }
        match lu {
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
        Request::OpenSettings => tx.send(Cmd::OpenSettings).map(|_| Value::Null).map_err(|e| e.to_string()),
        Request::ListPresets => Ok(presets_publies(&profils::list(), &Config::read(&crate::config::path()).unwrap_or_default())),
        Request::ApplyPreset { name } => profils::appliquer(&name).map(|_| Value::Null),
        Request::SetConfig { section, .. } if SECTIONS_PROTEGEES.contains(&section.as_str()) => {
            Err(format!("la section [{section}] ne se change pas par l'interface locale"))
        }
        Request::SetConfig { section, key, value } if ALIAS.contains(&(section.as_str(), key.as_str())) => ecrire_alias(&section, &key, &value).map(|_| Value::Null),
        Request::SetConfig { section, key, value } => crate::config::set_value(&section, &key, &value).map(|_| Value::Null),
    });
    match result {
        Ok(data) => json!({ "id": id, "ok": true, "data": data }),
        Err(e) => json!({ "id": id, "ok": false, "error": e }),
    }
}

/// Presets publiés aux clients, dans l'ordre du menu de l'icône ; `active` : ses réglages sont
/// ceux de `cfg`.
fn presets_publies(liste: &[Profile], cfg: &Config) -> Value {
    liste.iter().map(|p| json!({ "name": p.name, "builtin": p.builtin, "active": profils::same(&p.cfg, cfg) })).collect()
}

/// Clés d'avant gardées pour les clients (Horizon) : la vitesse ajoutée par la basse (rad/s à
/// fond) et l'éloignement sur les graves (part du rayon) sont les deux modulations de base ; les
/// fissures, le déclencheur « clic → fissure ».
const ALIAS: [(&str, &str); 3] = [("orbite", "vitesse_basse"), ("orbite", "eloignement_grave"), ("fissures", "actives")];

/// Modulation de base visée par un alias (source, cible, échelle de la quantité).
fn base(cle: &str) -> (Source, Cible, f32) {
    if cle == "vitesse_basse" { (Source::Basse, Cible::Vitesse, modulation::VITESSE) } else { (Source::Grave, Cible::Rayon, modulation::RAYON) }
}

/// Réglages publiés aux clients : ceux de `config.toml`, plus les clés d'avant calculées.
pub fn config_publiee(cfg: &Config) -> Value {
    let mut v = serde_json::to_value(cfg).unwrap_or(Value::Null);
    for cle in ["vitesse_basse", "eloignement_grave"] {
        let (source, cible, echelle) = base(cle);
        let q = cfg.modulations.liste.iter().find(|m| m.source == source && m.cible == cible && m.sur == Groupe::Tout).map_or(0.0, |m| m.quantite);
        v["orbite"][cle] = json!(q * echelle);
    }
    v["fissures"]["actives"] = json!(cfg.declencheurs.liste.iter().any(|d| d.quand == Evenement::Clic && d.faire == Action::Fissure));
    v
}

/// Écrit une clé d'avant : la modulation de base (ajoutée au début si elle manque, retirée à 0),
/// ou le déclencheur « clic → fissure » (ajouté ou retiré).
fn ecrire_alias(section: &str, cle: &str, valeur: &Value) -> Result<(), String> {
    if section == "fissures" {
        let oui = valeur.as_bool().ok_or("[fissures] actives : vrai ou faux attendu")?;
        return crate::config::modifier(|c| {
            let l = &mut c.declencheurs.liste;
            let est = |d: &Declencheur| d.quand == Evenement::Clic && d.faire == Action::Fissure;
            if !oui {
                l.retain(|d| !est(d));
            } else if !l.iter().any(est) {
                l.push(Declencheur::new(Evenement::Clic, Action::Fissure));
            }
        })
        .map(|_| crate::config::request_reload());
    }
    let (source, cible, echelle) = base(cle);
    let x = valeur.as_f64().filter(|x| x.is_finite()).ok_or(format!("[{section}] {cle} : nombre attendu"))? as f32;
    let q = ((x / echelle).clamp(-1.0, 1.0) * 1e4).round() / 1e4;
    crate::config::modifier(|c| {
        let l = &mut c.modulations.liste;
        let i = l.iter().position(|m| m.source == source && m.cible == cible && m.sur == Groupe::Tout);
        match (i, q == 0.0) {
            (Some(i), true) => {
                l.remove(i);
            }
            (Some(i), false) => l[i].quantite = q,
            (None, false) => l.insert(0, Modulation { source, cible, sur: Groupe::Tout, quantite: q, ..Default::default() }),
            (None, true) => {}
        }
    })
    .map(|_| crate::config::request_reload())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cles_d_avant_publiees() {
        let mut cfg = Config::default();
        let v = config_publiee(&cfg);
        assert!((v["orbite"]["vitesse_basse"].as_f64().unwrap() - 11.0).abs() < 1e-3);
        assert!((v["orbite"]["eloignement_grave"].as_f64().unwrap() - 1.3).abs() < 1e-3);
        assert_eq!(v["fissures"]["actives"], true);
        cfg.declencheurs.liste.retain(|d| d.faire != Action::Fissure);
        cfg.modulations.liste.clear();
        let v = config_publiee(&cfg);
        assert_eq!((v["fissures"]["actives"].clone(), v["orbite"]["vitesse_basse"].clone()), (json!(false), json!(0.0)));
    }

    #[test]
    fn sections_protegees_refusees() {
        let (tx, _rx) = std::sync::mpsc::channel();
        let shared = Shared::default();
        for section in SECTIONS_PROTEGEES {
            let r = handle(&json!({ "id": 1, "cmd": "set_config", "section": section, "key": "x", "value": 1 }).to_string(), &shared, &tx);
            assert_eq!(r["ok"], false, "{section}");
        }
    }

    #[test]
    fn presets_publies_avec_celui_en_place() {
        let liste = crate::prereglages::builtins();
        let v = presets_publies(&liste, &liste[1].cfg);
        let presets = v.as_array().unwrap();
        assert_eq!(presets.len(), liste.len());
        assert_eq!(presets[1], json!({ "name": liste[1].name, "builtin": true, "active": true }));
        let actifs = presets.iter().filter(|p| p["active"] == true).count();
        assert_eq!(actifs, liste.iter().filter(|p| profils::same(&p.cfg, &liste[1].cfg)).count());
        // un profil de l'utilisateur n'est pas « fourni »
        let a_moi = [Profile { name: "À moi".into(), builtin: false, cfg: Config::default(), date: None }];
        assert_eq!(presets_publies(&a_moi, &Config::default()), json!([{ "name": "À moi", "builtin": false, "active": true }]));
    }

    #[test]
    fn reglages_ouverts_par_la_boucle_principale() {
        let (tx, rx) = std::sync::mpsc::channel();
        let r = handle(&json!({ "id": 7, "cmd": "open_settings" }).to_string(), &Shared::default(), &tx);
        assert_eq!(r, json!({ "id": 7, "ok": true, "data": null }));
        assert!(matches!(rx.try_recv(), Ok(Cmd::OpenSettings)));
    }

    #[test]
    fn preset_inconnu_refuse() {
        let (tx, _rx) = std::sync::mpsc::channel();
        let shared = Shared::default();
        let r = handle(&json!({ "id": 2, "cmd": "apply_preset", "name": "aucun preset de ce nom ?" }).to_string(), &shared, &tx);
        assert_eq!(r["ok"], false);
        assert!(r["error"].as_str().unwrap().contains("aucun preset"));
        // sans nom : requête refusée, rien n'est appliqué
        assert_eq!(handle(&json!({ "id": 3, "cmd": "apply_preset" }).to_string(), &shared, &tx)["ok"], false);
    }

    #[test]
    fn origine_web_refusee() {
        let req = |origine: Option<&str>| {
            let mut b = HsRequest::builder().uri("/");
            if let Some(o) = origine {
                b = b.header("origin", o);
            }
            check_origin(&b.body(()).unwrap(), HsResponse::default()).is_ok()
        };
        assert!(req(None));
        for o in ["http://tauri.localhost", "tauri://localhost", "https://exemple.org", "null"] {
            assert!(!req(Some(o)), "{o}");
        }
    }
}
