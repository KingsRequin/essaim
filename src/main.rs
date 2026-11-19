//! Essaim : surcouche réactive à la musique autour du pointeur (comètes et essaim en orbite,
//! éclat du kick, fissures au clic).
//!
//! Le pointeur reste celui de Windows (pack de curseurs installé par les réglages de la souris) :
//! l'appli ne dessine que ce qui l'entoure.
//!
//! Options : `--reglages` ouvre la fenêtre des réglages (de l'instance en cours s'il y en a
//! une) ; `--quitter` ferme l'instance en cours ; `--restaurer-bordure` rend sa couleur à la
//! fenêtre restée colorée (fermeture, plantage, pause et plein écran le font tout seuls).

#![cfg_attr(not(test), windows_subsystem = "windows")]

mod appli;
mod audio;
mod auto;
mod backend;
mod bordure;
mod canvas;
mod charge;
mod config;
mod effets;
mod gpu;
mod ipc;
mod layer;
mod maj;
mod prisme;
mod profils;
mod reglages;
mod render;
mod souris;
mod win;

use render::{Crack, Rng, Swarm};
use std::io::Write;
use std::sync::Arc;
use std::time::{Duration, Instant};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HWND, LPARAM, WPARAM};
use windows::Win32::Graphics::Dwm::DwmFlush;
use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
use windows::Win32::System::Threading::{CreateMutexW, OpenMutexW, SYNCHRONIZATION_SYNCHRONIZE};
use windows::Win32::UI::HiDpi::{SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON, VK_RBUTTON};
use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, GetSystemMetrics, PostMessageW, SM_SWAPBUTTON, WM_CLOSE, WM_COMMAND};

const MAX_CRACKS: usize = 12;
/// Sans mouvement de souris ni musique depuis ce délai : cadence de repos.
const IDLE: Duration = Duration::from_secs(2);
/// Verrou d'instance unique.
const VERROU: PCWSTR = w!("Local\\Essaim-curseur");
/// Classe de la fenêtre des réglages (`reglages.rs`).
const CLASSE_REGLAGES: PCWSTR = w!("EssaimReglages");
/// Fondu de la veille profonde : durée de l'extinction et du réveil.
const FONDU_EXTINCTION: f32 = 1.0;
const FONDU_REVEIL: f32 = 0.25;
/// Veille profonde : souris, son et clics lus à ce rythme (un clic de 60 ms est attrapé).
const VEILLE_LECTURE: Duration = Duration::from_millis(40);
/// Attente la plus longue entre deux lectures des clics quand une cadence est choisie.
const LECTURE_CLICS: Duration = Duration::from_millis(30);
/// Capture du son impossible à lancer : nouvel essai après ce délai.
const SON_REESSAI: Duration = Duration::from_secs(30);

fn start_instant() -> Instant {
    Instant::now()
}

struct FileLog(std::sync::Mutex<std::fs::File>);
impl log::Log for FileLog {
    fn enabled(&self, m: &log::Metadata) -> bool {
        m.level() <= log::Level::Info
    }
    fn log(&self, r: &log::Record) {
        if self.enabled(r.metadata()) {
            if let Ok(mut f) = self.0.lock() {
                let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
                let _ = writeln!(f, "{secs} {} {}", r.level(), r.args());
            }
        }
    }
    fn flush(&self) {}
}

fn init_log() {
    let _ = std::fs::create_dir_all(config::dir());
    let p = config::dir().join("essaim.log");
    if std::fs::metadata(&p).map(|m| m.len() > 1 << 20).unwrap_or(false) {
        let _ = std::fs::remove_file(&p);
    }
    if let Ok(f) = std::fs::OpenOptions::new().create(true).append(true).open(&p) {
        let _ = log::set_boxed_logger(Box::new(FileLog(std::sync::Mutex::new(f))));
        log::set_max_level(log::LevelFilter::Info);
    }
}

/// Une instance tient le verrou (elle tourne, ou démarre).
fn instance_lancee() -> bool {
    match unsafe { OpenMutexW(SYNCHRONIZATION_SYNCHRONIZE, false, VERROU) } {
        Ok(h) => {
            let _ = unsafe { CloseHandle(h) };
            true
        }
        Err(_) => false,
    }
}

/// Fenêtre de l'icône de l'instance en cours. Une instance qui tient le verrou sans avoir encore
/// créé sa fenêtre (elle démarre) est attendue jusqu'à 2 s.
fn fenetre_instance() -> Option<HWND> {
    for i in 0..20 {
        if let Ok(h) = unsafe { FindWindowW(win::TRAY_CLASS, None) } {
            return Some(h);
        }
        if i == 0 && !instance_lancee() {
            return None;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    None
}

fn main() {
    // COM en appartement monothread pour le fil principal (DirectComposition, ShellExecute)
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--restaurer-bordure") {
        bordure::restore();
        return;
    }
    let open_settings = args.iter().any(|a| a == "--reglages");
    if open_settings {
        // instance en cours : c'est elle qui ouvre sa fenêtre
        if let Some(h) = fenetre_instance() {
            unsafe {
                let _ = PostMessageW(Some(h), WM_COMMAND, WPARAM(win::CMD_CONFIG), LPARAM(0));
            }
            return;
        }
    }
    if args.iter().any(|a| a == "--quitter") {
        unsafe {
            if let Ok(h) = FindWindowW(win::TRAY_CLASS, None) {
                let _ = PostMessageW(Some(h), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
        }
        return;
    }
    init_log();
    // verrou d'instance unique, gardé jusqu'à la fin (ou rendu avant de relancer)
    let verrou = unsafe {
        let verrou = CreateMutexW(None, true, VERROU);
        if GetLastError() == ERROR_ALREADY_EXISTS {
            log::info!("déjà lancé, sortie");
            return;
        }
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        verrou
    };
    // Quoi qu'il arrive, la bordure des fenêtres doit revenir (y compris celle laissée colorée
    // par une instance tuée).
    bordure::restore();
    // Plantage du fil principal : la bordure est rendue. Celui d'un autre fil (réglages, mises à
    // jour…) ne l'est pas : le fil principal continue de colorer, et lui retirer la couleur et
    // le témoin la laisserait sans filet si Essaim était tué ensuite.
    let principal = std::thread::current().id();
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if std::thread::current().id() == principal {
            bordure::restore();
        }
        log::error!("plantage : {info}");
        default_hook(info);
    }));
    log::info!("démarrage d'Essaim {}", env!("CARGO_PKG_VERSION"));
    let relancer = run(open_settings).unwrap_or_else(|e| {
        log::error!("arrêt sur erreur : {e:#}");
        false
    });
    bordure::restore();
    // nouvelle version en place : le verrou est rendu avant de lancer la suivante
    if let (true, Some(exe)) = (relancer, maj::exe()) {
        if let Ok(v) = verrou {
            unsafe {
                let _ = CloseHandle(v);
            }
        }
        match std::process::Command::new(exe).spawn() {
            Ok(_) => log::info!("relance pour la mise à jour"),
            Err(e) => log::error!("relance impossible : {e}"),
        }
    }
    log::info!("arrêt");
}

/// Bouton principal de la souris enfoncé (le droit si les boutons sont inversés dans Windows :
/// `GetAsyncKeyState` lit le bouton physique).
fn bouton_principal() -> bool {
    unsafe {
        let vk = if GetSystemMetrics(SM_SWAPBUTTON) != 0 { VK_RBUTTON } else { VK_LBUTTON };
        GetAsyncKeyState(vk.0 as i32) < 0
    }
}

/// Fenêtre des réglages ouverte (elle montre le son en direct : la capture doit continuer).
fn reglages_ouverts() -> bool {
    unsafe { FindWindowW(CLASSE_REGLAGES, None) }.is_ok()
}

/// Capture du son : lancée quand elle est voulue, arrêtée sinon (pause, plein écran, son
/// désactivé). État publié pour la fenêtre des réglages.
#[derive(Default)]
struct Son {
    capture: Option<audio::AudioThread>,
    /// Le dernier lancement a échoué ; nouvel essai à cette date.
    reessai: Option<Instant>,
    publie: Option<&'static str>,
}

impl Son {
    fn regler(&mut self, voulu: bool, shared: &Arc<audio::AudioShared>, now: Instant) {
        if !voulu {
            self.reessai = None;
            if self.capture.take().is_some() {
                // fil arrêté (attendu) : plus rien n'écrit, les niveaux peuvent être remis à zéro
                shared.reset();
                log::info!("son : capture arrêtée");
            }
        } else if self.capture.is_none() && self.reessai.is_none_or(|r| now >= r) {
            match audio::start(shared.clone()) {
                Ok(c) => {
                    self.capture = Some(c);
                    self.reessai = None;
                    log::info!("son : capture lancée");
                }
                Err(e) => {
                    if self.reessai.is_none() {
                        log::error!("son : capture impossible ({e}) ; Essaim continue sans son, nouvel essai toutes les 30 s");
                    }
                    self.reessai = Some(now + SON_REESSAI);
                }
            }
        }
        let etat = match (&self.capture, self.reessai) {
            (Some(_), _) if shared.perdu() => "erreur",
            (Some(_), _) => "active",
            (None, Some(_)) => "erreur",
            (None, None) => "coupee",
        };
        if self.publie != Some(etat) {
            charge::publier_capture(etat);
            self.publie = Some(etat);
        }
    }
}

/// Boucle de l'appli. Renvoie vrai s'il faut relancer Essaim (mise à jour prête).
fn run(open_settings: bool) -> anyhow::Result<bool> {
    // premier lancement (aucun réglage encore) : la fenêtre des réglages et son accueil
    let premier = !config::path().exists();
    let mut conf = config::Watched::load();
    render::set_trace(&conf.cfg.orbite.trace);
    let shared = Arc::new(audio::AudioShared::default());
    shared.set_sensitivity(conf.cfg.audio.sensibilite_kick);
    shared.set_min_gap(conf.cfg.audio.delai_kick);
    // capture lancée au premier tour de boucle ; un échec n'empêche pas Essaim de tourner
    let mut son = Son::default();

    let _tray = win::Tray::new()?;
    maj::start(conf.cfg.mise_a_jour.auto);
    win::raccourcis(conf.cfg.raccourcis.actif);
    let mut raccourcis = conf.cfg.raccourcis.actif;
    let mut automatique = auto::Auto::default();
    if open_settings || premier {
        reglages::open(shared.clone());
    }
    // interface locale pour Horizon, et synchro de teinte avec Prisme
    let ipc_state = Arc::new(ipc::Shared::default());
    let (ipc_tx, ipc_rx) = std::sync::mpsc::channel();
    ipc::start(ipc_state.clone(), ipc_tx);
    let prisme = prisme::start();
    let mut appli = appli::TeinteAppli::default();
    let mut sync_hue: Option<f32> = None;
    let mut synced = false;
    // écrans : relus quand leur configuration change (WM_DISPLAYCHANGE)
    let mut monitors = win::monitors();
    let (mut last_pos, mut last_move, mut last_sound, mut last_click, mut last_kicks) =
        ((0, 0), start_instant(), start_instant(), start_instant(), 0u32);
    let mut published: Option<(bool, bool, bool, bool, bool, u32)> = None;
    // un calque immobile par écran visité ; les fissures y sont retracées à chaque image
    let mut backend = backend::Backend::new(conf.cfg.rendu.carte_graphique);
    let mut cracks: Vec<Crack> = Vec::new();
    let mut swarm = Swarm::new();
    let mut effets = effets::Effets::new();
    let mut rng = Rng::seeded();
    let mut border = bordure::Border::default();

    let start = Instant::now();
    let mut last = start;
    let mut user_paused = false;
    let mut fs_paused = false;
    let mut conf_gen = 0u32;
    let mut was_down = false;
    let mut last_state = "";
    let mut cleared = false;
    let mut backend_gen = 0u32;
    // veille profonde : niveau du fondu (1 = éveillé, 0 = éteint) et extinction complète
    let (mut eveil, mut profond) = (1.0f32, false);
    // icône : état montré (pause, veille, génération des réglages) et preset en place
    let mut icone: Option<(bool, bool, u32)> = None;
    let mut preset: Option<(u32, Option<String>)> = None;
    let mut next_conf = start;
    let mut next_fs = start;
    let mut stat_send = Duration::ZERO;
    let mut relancer = false;
    // charge publiée chaque seconde pour la fenêtre des réglages
    let (mut charge_busy, mut charge_frames, mut charge_depuis) = (Duration::ZERO, 0u32, start);
    let (mut stat_frames, mut stat_time, mut stat_busy, mut stat_max, mut next_stat) =
        (0u32, 0f64, Duration::ZERO, Duration::ZERO, start + Duration::from_secs(60));

    loop {
        if !win::pump() {
            break;
        }
        match win::take_command() {
            win::CMD_QUIT => break,
            win::CMD_REDEMARRER if maj::prete().is_some() => {
                relancer = true;
                break;
            }
            win::CMD_PAUSE => {
                user_paused = !user_paused;
                charge::publier_pause(user_paused);
                log::info!("{}", if user_paused { "pause" } else { "reprise" });
            }
            win::CMD_CONFIG => reglages::open(shared.clone()),
            win::CMD_PROJECTEUR => effets.demander_projecteur(),
            c @ (win::CMD_SUIVANT | win::CMD_PRECEDENT) => {
                let pas = if c == win::CMD_SUIVANT { 1 } else { -1 };
                if let Some(nom) = profils::voisin(&conf.cfg, pas) {
                    match profils::appliquer(&nom) {
                        Ok(()) => win::bulle(&format!("Preset « {nom} »")),
                        Err(e) => log::warn!("raccourci : preset « {nom} » non appliqué ({e})"),
                    }
                }
            }
            c if c >= win::CMD_PRESET => {
                if let Some(p) = profils::list().get(c - win::CMD_PRESET) {
                    if let Err(e) = profils::appliquer(&p.name) {
                        log::warn!("menu : preset « {} » non appliqué ({e})", p.name);
                    }
                }
            }
            _ => {}
        }
        while let Ok(c) = ipc_rx.try_recv() {
            match c {
                ipc::Cmd::SetPaused(p) if p != user_paused => {
                    user_paused = p;
                    charge::publier_pause(p);
                    log::info!("{} (Horizon)", if p { "pause" } else { "reprise" });
                }
                ipc::Cmd::SetPaused(_) => {}
                ipc::Cmd::OpenConfig => win::open_file(&config::path()),
            }
        }
        if win::take_display_changed() {
            log::info!("écrans modifiés : calques refaits");
            monitors = win::monitors();
            backend.reset_layers();
        }
        let now = Instant::now();
        let t = (now - start).as_secs_f64();
        if now - charge_depuis >= Duration::from_secs(1) {
            let ms = charge_busy.as_secs_f32() * 1000.0 / charge_frames.max(1) as f32;
            charge::publier(ms, charge_frames as f32 / (now - charge_depuis).as_secs_f32());
            (charge_busy, charge_frames, charge_depuis) = (Duration::ZERO, 0, now);
        }

        let cur = win::cursor_state();
        if now >= next_fs {
            next_fs = now + Duration::from_millis(250);
            let fs = cur.as_ref().is_some_and(|c| win::fullscreen_under(c.pos.0, c.pos.1));
            if fs != fs_paused {
                fs_paused = fs;
                log::info!("{}", if fs { "plein écran : en veille" } else { "fin du plein écran" });
            }
        }
        // réglages : relus toutes les secondes, ou tout de suite s'ils viennent de la fenêtre
        let timed = now >= next_conf;
        if timed {
            next_conf = now + Duration::from_secs(1);
        }
        if (timed || config::take_reload_request()) && conf.reload() {
            conf_gen += 1;
            render::set_trace(&conf.cfg.orbite.trace);
            if conf.cfg.raccourcis.actif != raccourcis {
                raccourcis = conf.cfg.raccourcis.actif;
                win::raccourcis(raccourcis);
            }
            shared.set_sensitivity(conf.cfg.audio.sensibilite_kick);
            shared.set_min_gap(conf.cfg.audio.delai_kick);
            maj::regler(conf.cfg.mise_a_jour.auto);
        }
        // son : coupé en pause et en plein écran (sauf fenêtre des réglages ouverte, qui le montre)
        let son_voulu = conf.cfg.audio.actif && (!(user_paused || fs_paused) || reglages_ouverts());
        son.regler(son_voulu, &shared, now);
        automatique.tick(&conf.cfg, now);
        let cfg = &conf.cfg;
        prisme.set_wanted(cfg.couleur.synchro_prisme);
        let fields = (user_paused, fs_paused, !user_paused && !fs_paused, backend.is_gpu(), prisme.connected(), conf_gen);
        if published != Some(fields) {
            ipc_state.publish(serde_json::json!({
                "paused": user_paused,
                "fullscreen": fs_paused,
                "active": fields.2,
                "gpu": fields.3,
                "prisme": fields.4,
                "config": cfg,
            }));
            published = Some(fields);
        }
        // icône : grisée en pause, infobulle avec le preset en place et l'état
        let montre = (user_paused, fs_paused || profond, conf_gen);
        if icone != Some(montre) {
            if preset.as_ref().is_none_or(|p| p.0 != conf_gen) {
                let nom = profils::list().into_iter().find(|p| profils::same(&p.cfg, cfg)).map(|p| p.name);
                preset = Some((conf_gen, nom));
            }
            let nom = preset.as_ref().and_then(|p| p.1.as_deref());
            win::etat_icone(user_paused, &win::infobulle(nom, montre.0, montre.1));
            icone = Some(montre);
        }

        let active = !user_paused && !fs_paused;
        let state = match &cur {
            _ if !active => "en pause",
            None => "position du curseur illisible",
            Some(c) if !c.showing => "curseur masqué par Windows",
            Some(_) => "affiché",
        };
        if state != last_state {
            log::info!("état : {state}");
            last_state = state;
        }
        let au = shared.view();
        if au.bass > 0.03 || au.kicks != last_kicks {
            last_kicks = au.kicks;
            last_sound = now;
        }
        let down = bouton_principal();
        if down {
            last_click = now;
        }
        let Some(cur) = cur.filter(|c| active && c.showing) else {
            // curseur masqué (frappe) : la bordure continue ; pause ou plein écran : remise
            border.tick(active, &cfg.bordure, render::prisme(cfg, t, 1.0), au.kicks, now);
            if !cleared {
                backend.clear();
                cracks.clear();
                cleared = true;
            }
            // à la reprise, un bouton tenu pendant la pause ne compte pas comme un clic, et le
            // compte à rebours de la veille profonde repart de zéro
            was_down = down;
            last_move = now;
            if profond {
                profond = false;
                charge::publier_veille(false);
            }
            win::attendre(Duration::from_millis(if active { 16 } else { 100 }));
            continue;
        };
        cleared = false;
        let busy_start = Instant::now();
        let scale = win::scale_at(cur.pos.0, cur.pos.1);
        let cursor = (cur.pos.0 as f32, cur.pos.1 as f32);

        // clic : onde, éclats et nouvelle fissure (lus à chaque tour, même sans image à dessiner)
        if down && !was_down && (cfg.clic.onde || cfg.clic.eclats) {
            swarm.clic(cursor.0, cursor.1, t, cfg, scale);
        }
        if down && !was_down && cfg.fissures.actives {
            if cracks.len() >= MAX_CRACKS {
                cracks.remove(0);
            }
            cracks.push(Crack::new(cursor.0, cursor.1, t, cfg.fissures.taille * scale, &mut rng));
        }
        was_down = down;
        if cur.pos != last_pos {
            last_pos = cur.pos;
            last_move = now;
        }

        // veille profonde : souris immobile, ni son ni clic depuis `veille_apres` → fondu, puis
        // plus aucun dessin jusqu'au prochain mouvement, son ou clic
        let calme = now - last_move.max(last_sound).max(last_click);
        let endormi = cfg.rendu.veille_apres > 0 && calme >= Duration::from_secs(cfg.rendu.veille_apres.into());
        if endormi && eveil <= 0.0 {
            if !profond {
                profond = true;
                backend.clear();
                cracks.clear();
                charge::publier_veille(true);
                log::info!("veille profonde : plus rien n'est dessiné");
            }
            border.tick(true, &cfg.bordure, render::prisme(cfg, t, 1.0), au.kicks, now);
            win::attendre(VEILLE_LECTURE);
            continue;
        }
        if profond {
            profond = false;
            charge::publier_veille(false);
            log::info!("fin de la veille profonde");
        }

        // repos : souris immobile et pas de musique depuis 2 s → cadence réduite
        let idle = cfg.rendu.cadence_repos > 0 && now - last_move > IDLE && now - last_sound > IDLE && !down;
        let cadence = match (idle, cfg.rendu.cadence) {
            (true, 0) => cfg.rendu.cadence_repos,
            (true, c) => c.min(cfg.rendu.cadence_repos),
            (false, c) => c,
        };
        // cadence choisie : attente jusqu'à l'image suivante, par tranches courtes pour que les
        // clics restent lus (un clic de 60 ms doit être vu)
        if cadence > 0 {
            let echeance = last + Duration::from_secs_f64(1.0 / cadence as f64);
            if now + Duration::from_millis(1) < echeance {
                win::attendre((echeance - now).min(LECTURE_CLICS));
                continue;
            }
        }
        // écart réel depuis l'image précédente : la simulation le découpe en pas courts
        let dt = (now - last).as_secs_f32().clamp(0.0005, 0.25);
        last = now;
        // fondu de veille : extinction lente, réveil rapide (écart borné : pas de saut au réveil)
        let df = dt.min(render::PAS_MAX);
        eveil = if endormi { (eveil - df / FONDU_EXTINCTION).max(0.0) } else { (eveil + df / FONDU_REVEIL).min(1.0) };

        // simulation ; teinte qui glisse avec la musique, lissée (~150 ms)
        swarm.avance(t, dt, cursor, au, cfg, scale);
        effets.avance(t, dt, cursor, scale, swarm.env, &shared, cfg, &monitors);
        let d = render::decalage();
        render::set_decalage(d + (render::decalage_vise(&au) - d) * (dt * 6.0).min(1.0));
        // teinte imposée : celle de l'application au premier plan, sinon celle de l'éclairage
        // (Prisme) ; suivie en douceur, en passant par le plus court
        let cible = if cfg.couleur.application || cfg.couleur.site {
            appli.courante(cfg.couleur.application, cfg.couleur.site, now)
        } else if cfg.couleur.synchro_prisme {
            prisme.current()
        } else {
            None
        };
        sync_hue = match (cible, sync_hue) {
            (Some(target), Some(h)) => {
                let d = (target - h + 540.0).rem_euclid(360.0) - 180.0;
                Some((h + d * (dt * 4.0).min(1.0)).rem_euclid(360.0))
            }
            (Some(target), None) => Some(target),
            _ => None,
        };
        render::set_sync_hue(sync_hue);
        if sync_hue.is_some() != synced {
            synced = sync_hue.is_some();
            let source = if cfg.couleur.application || cfg.couleur.site { "celle de l'application ou du site au premier plan" } else { "celle de l'éclairage (Prisme)" };
            log::info!("teinte : {}", if synced { source } else { "cycle propre" });
        }
        let life = cfg.fissures.duree.max(0.2) as f64;
        cracks.retain(|c| t - c.born <= life);
        if cfg.rendu.carte_graphique != backend.wants_gpu() && conf_gen != backend_gen {
            backend = backend::Backend::new(cfg.rendu.carte_graphique);
        }
        backend_gen = conf_gen;

        // dessin : sur chaque écran que l'essaim ou une fissure touche
        let (x0, y0, x1, y1) = swarm.bounds(cursor, scale, cfg.eclat.taille);
        let swarm_rect = (x0.floor() as i32, y0.floor() as i32, x1.ceil() as i32, y1.ceil() as i32);
        // réticule et spectre autour du pointeur : dans la zone de l'essaim ; projecteur : tout
        // l'écran du pointeur, le temps de l'effet seulement
        let swarm_rect = layer::union(Some(swarm_rect), effets.rect_pointeur(cursor, scale, cfg)).unwrap_or(swarm_rect);
        let mut crack_rects: Vec<layer::IRect> = cracks.iter().map(Crack::rect).collect();
        crack_rects.extend(effets.rect_ecran());
        let rgb = render::prisme(cfg, t, 1.0);
        border.tick(true, &cfg.bordure, rgb, au.kicks, now);
        let env = swarm.env;
        let dessiner = |cv: &mut dyn canvas::Canvas, here: bool| {
            effets.dessiner_voile(cv, cfg, t, scale);
            for c in &cracks {
                let age = t - c.born;
                // éclat coloré au début et sur les kicks, fondu sur la dernière seconde
                let glow = ((1.0 - age / 0.45).max(0.0) as f32).max(env * 0.6);
                c.draw(cv, glow, rgb, (life - age).clamp(0.0, 1.0) as f32);
            }
            if here {
                effets.dessiner(cv, t, cfg, scale, cursor);
                swarm.draw(cv, t, cfg, scale, cursor);
            }
        };
        let draw = |cv: &mut dyn canvas::Canvas, here: bool| {
            if eveil < 1.0 {
                dessiner(&mut canvas::Fondu { cv, k: eveil }, here);
            } else {
                dessiner(cv, here);
            }
        };
        stat_send += backend.frame(&monitors, swarm_rect, &crack_rects, &draw);

        let busy = busy_start.elapsed();
        (charge_busy, charge_frames) = (charge_busy + busy, charge_frames + 1);
        stat_frames += 1;
        stat_time += dt as f64;
        stat_busy += busy;
        stat_max = stat_max.max(busy);
        if now >= next_stat {
            log::info!(
                "images : {:.0}/s, calcul moyen {:.2} ms dont envoi {:.2} ms, pire {:.2} ms",
                stat_frames as f64 / stat_time.max(1e-3),
                stat_busy.as_secs_f64() * 1000.0 / stat_frames.max(1) as f64,
                stat_send.as_secs_f64() * 1000.0 / stat_frames.max(1) as f64,
                stat_max.as_secs_f64() * 1000.0
            );
            (stat_frames, stat_time, stat_busy, stat_send, stat_max, next_stat) =
                (0, 0.0, Duration::ZERO, Duration::ZERO, Duration::ZERO, now + Duration::from_secs(60));
        }
        // cadence de l'écran : une image par rafraîchissement (une cadence choisie attend en haut)
        if cadence == 0 && unsafe { DwmFlush() }.is_err() {
            win::attendre(Duration::from_millis(7));
        }
    }
    drop(son);
    Ok(relancer)
}
