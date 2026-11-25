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
//! - `apply_preset` `{"name": "Discret"}` : applique le preset, comme le menu de l'icône. Si
//!   les réglages sont appliqués mais pas le jeu de curseurs du preset, la réponse reste
//!   `{"id": 1, "ok": true, "data": null, "warning": "…"}` (raison de l'échec des curseurs) ;
//!   `ok: false` seulement si les réglages n'ont pas été appliqués ;
//! - `open_settings` : ouvre la fenêtre des réglages (ou la ramène devant) ;
//! - `open_config` : ouvre `config.toml` dans l'éditeur.
//!
//! Clés d'avant les modulations et les déclencheurs, encore lues et écrites par Horizon
//! (`orbite.vitesse_basse`, `orbite.eloignement_grave`, `fissures.actives`) : `config` les
//! donne, calculées, et `set_config` les traduit (`ALIAS`).
//!
//! Les pages web et les applis web locales (Tauri, Electron) sont refusées : tout en-tête
//! `Origin` ferme la connexion. `set_config` ne touche pas aux réglages qui font télécharger ou
//! changer de comportement en arrière-plan (`SECTIONS_PROTEGEES`). La poignée de main doit finir
//! dans les 3 s ; `open_config` passe une fois toutes les 5 s au plus, `set_config` et
//! `apply_preset` sont étalés à 20 par seconde (tous clients confondus).
//!
//! Un jeton aléatoire est écrit au premier lancement dans `%APPDATA%\Essaim\ipc.jeton`, lisible
//! par l'utilisateur seul (droits du dossier). Il n'est pas encore demandé aux clients.

use crate::config::{Action, Cible, Config, Declencheur, Evenement, Groupe, Modulation, Source};
use crate::modulation;
use crate::profils::{self, Echec, Profile};
use serde::Deserialize;
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
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
/// Temps laissé à un client pour finir la poignée de main, depuis l'acceptation de la connexion.
const POIGNEE_MAX: Duration = Duration::from_secs(3);
/// `open_config` ouvre une fenêtre : une fois par ce délai au plus.
const OUVERTURE_MIN: Duration = Duration::from_secs(5);
/// Écritures (`set_config`, `apply_preset`) par seconde, tous clients confondus ; au-delà, elles
/// attendent leur tour, jusqu'à `ATTENTE_MAX` (puis sont refusées).
const ECRITURES_PAR_S: f64 = 20.0;
const ATTENTE_MAX: Duration = Duration::from_secs(2);

/// Ce que touchent les commandes : fichier des réglages et presets (remplacés dans les essais,
/// qui ne doivent jamais toucher aux vrais), et débits.
pub struct Contexte {
    config: PathBuf,
    lister: fn() -> Vec<Profile>,
    appliquer: fn(&str) -> Result<(), Echec>,
    debit: Mutex<Debit>,
}

impl Contexte {
    fn new(config: PathBuf, lister: fn() -> Vec<Profile>, appliquer: fn(&str) -> Result<(), Echec>) -> Self {
        Self { config, lister, appliquer, debit: Mutex::new(Debit::new(Instant::now())) }
    }

    /// Les vrais réglages et presets de l'utilisateur.
    fn reel() -> Self {
        Self::new(crate::config::path(), profils::list, |nom| profils::appliquer_detaille(nom, false))
    }

    fn debit(&self) -> std::sync::MutexGuard<'_, Debit> {
        self.debit.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Attend le tour d'une écriture, ou la refuse si la file est trop longue.
    fn tour_d_ecriture(&self) -> Result<(), String> {
        let attente = self.debit().ecriture(Instant::now())?;
        if !attente.is_zero() {
            std::thread::sleep(attente);
        }
        Ok(())
    }
}

/// Débits des commandes coûteuses (seau à jetons pour les écritures).
struct Debit {
    config_ouverte: Option<Instant>,
    jetons: f64,
    rempli: Instant,
}

impl Debit {
    fn new(maintenant: Instant) -> Self {
        Self { config_ouverte: None, jetons: ECRITURES_PAR_S, rempli: maintenant }
    }

    fn ouverture(&mut self, maintenant: Instant) -> Result<(), String> {
        if self.config_ouverte.is_some_and(|t| maintenant.saturating_duration_since(t) < OUVERTURE_MIN) {
            return Err(format!("open_config : une fois toutes les {} s au plus", OUVERTURE_MIN.as_secs()));
        }
        self.config_ouverte = Some(maintenant);
        Ok(())
    }

    /// Attente avant une écriture (zéro s'il reste un jeton) ; refusée au-delà d'`ATTENTE_MAX`.
    fn ecriture(&mut self, maintenant: Instant) -> Result<Duration, String> {
        let ecoule = maintenant.saturating_duration_since(self.rempli).as_secs_f64();
        self.jetons = (self.jetons + ecoule * ECRITURES_PAR_S).min(ECRITURES_PAR_S);
        self.rempli = maintenant;
        let attente = Duration::from_secs_f64(((1.0 - self.jetons) / ECRITURES_PAR_S).max(0.0));
        if attente > ATTENTE_MAX {
            return Err("trop de demandes : réessaie dans un instant".into());
        }
        self.jetons -= 1.0;
        Ok(attente)
    }
}

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

/// Essais d'ouverture du port : à une relance (mise à jour, `--quitter` puis lancement), l'instance
/// d'avant peut le tenir encore un instant.
const ESSAIS_PORT: u32 = 10;
const ATTENTE_PORT: Duration = Duration::from_millis(500);

/// Ouvre le port, en réessayant `essais` fois en tout, `attente` entre deux.
fn ecouter(essais: u32, attente: Duration) -> std::io::Result<TcpListener> {
    let mut i = 1;
    loop {
        match TcpListener::bind(("127.0.0.1", PORT)) {
            Ok(l) => return Ok(l),
            Err(e) if i >= essais => return Err(e),
            Err(_) => {
                i += 1;
                std::thread::sleep(attente);
            }
        }
    }
}

pub fn start(shared: Arc<Shared>, tx: Sender<Cmd>) {
    let lance = std::thread::Builder::new().name("essaim-ipc".into()).spawn(move || {
        let listener = match ecouter(ESSAIS_PORT, ATTENTE_PORT) {
            Ok(l) => l,
            Err(e) => {
                log::warn!("interface locale : port {PORT} indisponible ({e})");
                return;
            }
        };
        log::info!("interface locale : ws://127.0.0.1:{PORT}");
        if let Err(e) = ecrire_jeton(&crate::config::dir()) {
            log::warn!("interface locale : jeton non écrit ({e})");
        }
        let contexte = Arc::new(Contexte::reel());
        let clients = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        for stream in listener.incoming().flatten() {
            // quelques clients au plus (Horizon, un outil) : pas de fils sans limite
            if clients.load(Ordering::Relaxed) >= MAX_CLIENTS {
                log::warn!("interface locale : trop de clients, connexion refusée");
                continue;
            }
            // la place est rendue quand le client se termine, ou tout de suite si son fil
            // ne démarre pas (la fermeture est alors détruite avec la garde)
            let slot = Slot::take(&clients);
            let (shared, tx, contexte) = (shared.clone(), tx.clone(), contexte.clone());
            let _ = std::thread::Builder::new().name("essaim-ipc-client".into()).spawn(move || {
                let _slot = slot;
                if let Err(e) = client(stream, &shared, &tx, &contexte) {
                    log::debug!("interface locale : client fermé ({e})");
                }
            });
        }
    });
    if let Err(e) = lance {
        log::warn!("interface locale : fil impossible à lancer ({e})");
    }
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

/// Flux du client borné par une échéance : chaque lecture ou écriture n'attend que le temps qui
/// reste avant elle (un client qui envoie un octet de temps en temps ne gagne rien).
struct Borne {
    flux: TcpStream,
    echeance: Option<Instant>,
}

impl Borne {
    /// Temps restant avant l'échéance, appliqué à la socket ; erreur s'il est écoulé.
    fn reste(&self, ecriture: bool) -> std::io::Result<()> {
        let Some(e) = self.echeance else { return Ok(()) };
        let reste = e.saturating_duration_since(Instant::now());
        if reste.is_zero() {
            return Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "poignée de main trop longue"));
        }
        if ecriture { self.flux.set_write_timeout(Some(reste)) } else { self.flux.set_read_timeout(Some(reste)) }
    }
}

impl Read for Borne {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.reste(false)?;
        self.flux.read(buf)
    }
}

impl Write for Borne {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.reste(true)?;
        self.flux.write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.flux.flush()
    }
}

/// Poignée de main WebSocket, finie avant `delai` depuis l'acceptation ou abandonnée.
fn poignee(stream: TcpStream, delai: Duration) -> anyhow::Result<WebSocket<Borne>> {
    let limites = WebSocketConfig::default().max_message_size(Some(MESSAGE_MAX)).max_frame_size(Some(MESSAGE_MAX));
    let flux = Borne { flux: stream, echeance: Some(Instant::now() + delai) };
    let mut ws = tungstenite::accept_hdr_with_config(flux, check_origin, Some(limites))?;
    let b = ws.get_mut();
    b.echeance = None;
    b.flux.set_read_timeout(Some(Duration::from_millis(250)))?;
    b.flux.set_write_timeout(Some(Duration::from_secs(3)))?;
    Ok(ws)
}

fn client(stream: TcpStream, shared: &Shared, tx: &Sender<Cmd>, contexte: &Contexte) -> anyhow::Result<()> {
    // une poignée de main qui traîne ne doit pas garder sa place
    let mut ws = poignee(stream, POIGNEE_MAX)?;
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
                let reply = handle(t.as_str(), shared, tx, contexte);
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

fn handle(text: &str, shared: &Shared, tx: &Sender<Cmd>, ctx: &Contexte) -> Value {
    let raw: Value = serde_json::from_str(text).unwrap_or(Value::Null);
    let id = raw.get("id").cloned().unwrap_or(Value::Null);
    let envoyer = |cmd: Cmd| tx.send(cmd).map(|_| Value::Null).map_err(|e| e.to_string());
    // réussite partielle : la réponse reste `ok`, avec la raison de ce qui a manqué
    let mut avertissement = None;
    let result = serde_json::from_value::<Request>(raw).map_err(|e| e.to_string()).and_then(|r| match r {
        Request::GetState => Ok(shared.snapshot().1),
        Request::SetPaused { paused } => envoyer(Cmd::SetPaused(paused)),
        Request::OpenConfig => ctx.debit().ouverture(Instant::now()).and_then(|()| envoyer(Cmd::OpenConfig)),
        Request::OpenSettings => envoyer(Cmd::OpenSettings),
        Request::ListPresets => Ok(presets_publies(&(ctx.lister)(), &Config::read(&ctx.config).unwrap_or_default())),
        Request::ApplyPreset { name } => {
            if !(ctx.lister)().iter().any(|p| profils::meme_nom(&p.name, &name)) {
                return Err(format!("aucun preset « {name} »"));
            }
            ctx.tour_d_ecriture()?;
            match (ctx.appliquer)(&name) {
                Ok(()) => Ok(Value::Null),
                // réglages appliqués, curseurs non : le preset est bien en place
                Err(Echec::Curseurs(e)) => {
                    avertissement = Some(format!("preset appliqué, mais curseurs non installés : {e}"));
                    Ok(Value::Null)
                }
                Err(e) => Err(e.to_string()),
            }
        }
        Request::SetConfig { section, key, .. } if SECTIONS_PROTEGEES.contains(&section.as_str()) => {
            Err(format!("[{section}] {key} : la section [{section}] ne se change pas par l'interface locale"))
        }
        Request::SetConfig { section, key, value } => {
            ctx.tour_d_ecriture()?;
            if ALIAS.contains(&(section.as_str(), key.as_str())) {
                ecrire_alias(&ctx.config, &section, &key, &value)
            } else {
                crate::config::set_value_in(&ctx.config, &section, &key, &value)
            }
            .map(|_| Value::Null)
        }
    });
    match (result, avertissement) {
        (Ok(data), None) => json!({ "id": id, "ok": true, "data": data }),
        (Ok(data), Some(w)) => json!({ "id": id, "ok": true, "data": data, "warning": w }),
        (Err(e), _) => json!({ "id": id, "ok": false, "error": e }),
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
fn ecrire_alias(config: &Path, section: &str, cle: &str, valeur: &Value) -> Result<(), String> {
    if section == "fissures" {
        let oui = valeur.as_bool().ok_or("[fissures] actives : vrai ou faux attendu")?;
        return crate::config::modifier_in(config, |c| {
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
    crate::config::modifier_in(config, |c| {
        let l = &mut c.modulations.liste;
        let i = l.iter().position(|m| m.source == source && m.cible == cible && m.sur == Groupe::Tout);
        match (i, q == 0.0) {
            (Some(i), true) => {
                l.remove(i);
            }
            (Some(i), false) => l[i].quantite = q,
            // liste pleine : la modulation d'avant n'est pas ajoutée (elle ferait tomber la
            // dernière de l'utilisateur à la relecture)
            (None, false) if l.len() >= crate::config::MODULATIONS_MAX => {}
            (None, false) => l.insert(0, Modulation { source, cible, sur: Groupe::Tout, quantite: q, ..Default::default() }),
            (None, true) => {}
        }
    })
    .map(|_| crate::config::request_reload())
}

/// Écrit `ipc.jeton` dans `dossier` s'il n'y est pas : 32 octets aléatoires en hexadécimal. Le
/// fichier hérite des droits du dossier (`%APPDATA%` : l'utilisateur, le système et les
/// administrateurs seulement).
fn ecrire_jeton(dossier: &Path) -> std::io::Result<()> {
    let p = dossier.join("ipc.jeton");
    if p.exists() {
        return Ok(());
    }
    std::fs::create_dir_all(dossier)?;
    let mut octets = [0u8; 32];
    let status = unsafe {
        use windows::Win32::Security::Cryptography::{BCryptGenRandom, BCRYPT_USE_SYSTEM_PREFERRED_RNG};
        BCryptGenRandom(None, &mut octets, BCRYPT_USE_SYSTEM_PREFERRED_RNG)
    };
    if status.is_err() {
        return Err(std::io::Error::other(format!("tirage aléatoire impossible ({:#x})", status.0)));
    }
    let texte: String = octets.iter().map(|o| format!("{o:02x}")).collect();
    // création exclusive : un autre lancement l'a peut-être écrit entre-temps
    match std::fs::OpenOptions::new().write(true).create_new(true).open(&p) {
        Ok(mut f) => f.write_all(texte.as_bytes()),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Dossier temporaire supprimé à la fin de l'essai.
    struct Dossier(PathBuf);
    impl Drop for Dossier {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn dossier(nom: &str) -> Dossier {
        let d = std::env::temp_dir().join(format!("essaim-test-ipc-{}-{nom}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        Dossier(d)
    }

    fn aucun_preset() -> Vec<Profile> {
        Vec::new()
    }
    fn jamais_applique(nom: &str) -> Result<(), Echec> {
        panic!("preset « {nom} » appliqué pendant un essai")
    }

    /// Contexte d'essai : config dans un dossier temporaire, aucun preset, rien d'appliqué.
    fn contexte(d: &Dossier) -> Contexte {
        Contexte::new(d.0.join("config.toml"), aucun_preset, jamais_applique)
    }

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
        let d = dossier("protegees");
        let ctx = contexte(&d);
        // de vraies clés, qui seraient acceptées par `set_value`
        let cles = [("mise_a_jour", "auto", json!(false)), ("bibliotheque", "depot", json!("a/b")), ("automatique", "actif", json!(true)), ("pause", "batterie", json!("rien"))];
        assert_eq!(cles.iter().map(|(s, ..)| *s).collect::<Vec<_>>(), SECTIONS_PROTEGEES);
        for (section, key, value) in cles {
            let r = handle(&json!({ "id": 1, "cmd": "set_config", "section": section, "key": key, "value": value }).to_string(), &shared, &tx, &ctx);
            assert_eq!(r["ok"], false, "{section}");
            assert!(r["error"].as_str().unwrap().contains("interface locale"), "{r}");
        }
        assert!(!ctx.config.exists(), "rien n'a été écrit");
        // une clé ordinaire, elle, est écrite (dans le fichier de l'essai)
        let r = handle(&json!({ "id": 2, "cmd": "set_config", "section": "orbite", "key": "rayon", "value": 30 }).to_string(), &shared, &tx, &ctx);
        assert_eq!(r["ok"], true, "{r}");
        assert_eq!(Config::read(&ctx.config).unwrap().orbite.rayon, 30.0);
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
        let d = dossier("reglages");
        let r = handle(&json!({ "id": 7, "cmd": "open_settings" }).to_string(), &Shared::default(), &tx, &contexte(&d));
        assert_eq!(r, json!({ "id": 7, "ok": true, "data": null }));
        assert!(matches!(rx.try_recv(), Ok(Cmd::OpenSettings)));
    }

    #[test]
    fn preset_inconnu_refuse() {
        let (tx, _rx) = std::sync::mpsc::channel();
        let shared = Shared::default();
        let d = dossier("preset");
        let ctx = contexte(&d);
        let r = handle(&json!({ "id": 2, "cmd": "apply_preset", "name": "aucun preset de ce nom ?" }).to_string(), &shared, &tx, &ctx);
        assert_eq!(r["ok"], false);
        assert!(r["error"].as_str().unwrap().contains("aucun preset"));
        // sans nom : requête refusée, rien n'est appliqué
        assert_eq!(handle(&json!({ "id": 3, "cmd": "apply_preset" }).to_string(), &shared, &tx, &ctx)["ok"], false);
    }

    #[test]
    fn preset_applique_sans_ses_curseurs_reste_ok() {
        fn un_preset() -> Vec<Profile> {
            vec![Profile { name: "Nuit".into(), builtin: false, cfg: Config::default(), date: None }]
        }
        fn curseurs_refuses(_: &str) -> Result<(), Echec> {
            Err(Echec::Curseurs("jeu introuvable".into()))
        }
        fn reglages_refuses(_: &str) -> Result<(), Echec> {
            Err(Echec::Reglages("config.toml verrouillé".into()))
        }
        let (tx, _rx) = std::sync::mpsc::channel();
        let shared = Shared::default();
        let d = dossier("curseurs");
        let demande = json!({ "id": 4, "cmd": "apply_preset", "name": "nuit" }).to_string();
        let r = handle(&demande, &shared, &tx, &Contexte::new(d.0.join("config.toml"), un_preset, curseurs_refuses));
        assert_eq!(r["ok"], true, "{r}");
        assert_eq!(r["data"], Value::Null);
        assert!(r["warning"].as_str().unwrap().contains("jeu introuvable"), "{r}");
        // réglages non écrits : échec, sans avertissement
        let r = handle(&demande, &shared, &tx, &Contexte::new(d.0.join("config.toml"), un_preset, reglages_refuses));
        assert_eq!(r, json!({ "id": 4, "ok": false, "error": "config.toml verrouillé" }));
    }

    #[test]
    fn ouverture_de_la_config_espacee() {
        let (tx, rx) = std::sync::mpsc::channel();
        let d = dossier("ouverture");
        let ctx = contexte(&d);
        let ouvrir = || handle(&json!({ "id": 1, "cmd": "open_config" }).to_string(), &Shared::default(), &tx, &ctx);
        assert_eq!(ouvrir()["ok"], true);
        assert_eq!(ouvrir()["ok"], false, "deuxième demande aussitôt : refusée");
        assert!(matches!(rx.try_recv(), Ok(Cmd::OpenConfig)) && rx.try_recv().is_err());
        let mut debit = Debit::new(Instant::now());
        let t = Instant::now();
        debit.ouverture(t).unwrap();
        assert!(debit.ouverture(t + Duration::from_secs(4)).is_err());
        assert!(debit.ouverture(t + OUVERTURE_MIN).is_ok());
    }

    #[test]
    fn ecritures_etalees_puis_refusees() {
        let t = Instant::now();
        let mut debit = Debit::new(t);
        // une rafale de 20 passe aussitôt, la suite attend son tour (1/20 s chacune)
        for _ in 0..20 {
            assert_eq!(debit.ecriture(t).unwrap(), Duration::ZERO);
        }
        let attente = debit.ecriture(t).unwrap();
        assert!((attente.as_secs_f64() - 0.05).abs() < 1e-6, "{attente:?}");
        // au-delà de 2 s d'attente : refusée
        let mut refus = false;
        for _ in 0..60 {
            refus |= debit.ecriture(t).is_err();
        }
        assert!(refus);
        // le seau se remplit avec le temps
        assert_eq!(debit.ecriture(t + Duration::from_secs(10)).unwrap(), Duration::ZERO);
    }

    #[test]
    fn poignee_de_main_bornee_dans_le_temps() {
        let ecoute = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let adresse = ecoute.local_addr().unwrap();
        // un client qui envoie sa requête un octet toutes les 50 ms, sans jamais la finir
        let lent = std::thread::spawn(move || {
            let mut c = TcpStream::connect(adresse).unwrap();
            for o in b"GET / HTTP/1.1\r\nHost: 127.0.0.1\r\nX-Lent: aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".iter() {
                if c.write_all(&[*o]).is_err() {
                    return;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        });
        let (flux, _) = ecoute.accept().unwrap();
        let debut = Instant::now();
        assert!(poignee(flux, Duration::from_millis(400)).is_err());
        let duree = debut.elapsed();
        assert!(duree < Duration::from_millis(1500), "échéance dépassée : {duree:?}");
        lent.join().unwrap();
        // un vrai client, lui, passe
        let ecoute = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let adresse = ecoute.local_addr().unwrap();
        let client = std::thread::spawn(move || tungstenite::connect(format!("ws://{adresse}/")).map(|_| ()));
        let (flux, _) = ecoute.accept().unwrap();
        assert!(poignee(flux, Duration::from_secs(3)).is_ok());
        assert!(client.join().unwrap().is_ok());
    }

    #[test]
    fn jeton_ecrit_une_fois() {
        let d = dossier("jeton");
        let sous = d.0.join("Essaim");
        ecrire_jeton(&sous).unwrap();
        let jeton = std::fs::read_to_string(sous.join("ipc.jeton")).unwrap();
        assert_eq!(jeton.len(), 64);
        assert!(jeton.chars().all(|c| c.is_ascii_hexdigit()));
        ecrire_jeton(&sous).unwrap();
        assert_eq!(std::fs::read_to_string(sous.join("ipc.jeton")).unwrap(), jeton, "gardé d'un lancement à l'autre");
        let autre = d.0.join("autre");
        ecrire_jeton(&autre).unwrap();
        assert_ne!(std::fs::read_to_string(autre.join("ipc.jeton")).unwrap(), jeton, "tiré au hasard");
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
