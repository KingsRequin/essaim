//! Essaim : curseur animé réactif à la musique (point, comètes et essaim en orbite, fissures au clic).
//!
//! Le point et ses variantes sont de vrais curseurs Windows (`cursors.rs`) : aucun retard. L'appli
//! dessine par-dessus les sphères, l'éclat du kick et les fissures.
//!
//! Options : `--quitter` ferme l'instance en cours ; `--restaurer-curseur` remet les curseurs
//! Windows d'origine (fermeture, plantage, pause et plein écran le font tout seuls).

#![cfg_attr(not(test), windows_subsystem = "windows")]

mod audio;
mod backend;
mod canvas;
mod config;
mod cursors;
mod dsp;
mod gpu;
mod ipc;
mod layer;
mod pointer;
mod prisme;
mod render;
mod win;

use render::{Crack, Rng, Swarm};
use std::io::Write;
use std::sync::Arc;
use std::time::{Duration, Instant};
use windows::core::w;
use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS, LPARAM, WPARAM};
use windows::Win32::Graphics::Dwm::DwmFlush;
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::HiDpi::{SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON};
use pointer::Role;
use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, PostMessageW, WM_CLOSE};

const MAX_CRACKS: usize = 12;
/// Sans mouvement de souris ni musique depuis ce délai : cadence de repos.
const IDLE: Duration = Duration::from_secs(2);

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

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--restaurer-curseur") {
        win::restore_cursors();
        return;
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
    unsafe {
        let _mutex = CreateMutexW(None, true, w!("Local\\Essaim-curseur"));
        if GetLastError() == ERROR_ALREADY_EXISTS {
            log::info!("déjà lancé, sortie");
            return;
        }
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        std::mem::forget(_mutex); // garde le verrou jusqu'à la fin du processus
    }
    // Quoi qu'il arrive, la flèche de Windows doit revenir.
    win::restore_cursors();
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        win::restore_cursors();
        log::error!("plantage : {info}");
        default_hook(info);
    }));
    log::info!("démarrage d'Essaim {}", env!("CARGO_PKG_VERSION"));
    if let Err(e) = run() {
        log::error!("arrêt sur erreur : {e:#}");
    }
    win::restore_cursors();
    log::info!("arrêt");
}

fn run() -> anyhow::Result<()> {
    let mut conf = config::Watched::load();
    let shared = Arc::new(audio::AudioShared::default());
    shared.set_sensitivity(conf.cfg.audio.sensibilite_kick);
    let mut audio_thread = if conf.cfg.audio.actif { Some(audio::start(shared.clone())?) } else { None };

    let _tray = win::Tray::new()?;
    // interface locale pour Horizon, et synchro de teinte avec Prisme
    let ipc_state = Arc::new(ipc::Shared::default());
    let (ipc_tx, ipc_rx) = std::sync::mpsc::channel();
    ipc::start(ipc_state.clone(), ipc_tx);
    let prisme = prisme::start();
    let mut sync_hue: Option<f32> = None;
    let mut synced = false;
    let mut monitors = win::monitors();
    let mut cursor_size = win::cursor_base_size();
    let (mut last_pos, mut last_move, mut last_sound, mut last_kicks) = ((0, 0), start_instant(), start_instant(), 0u32);
    let mut published: Option<(bool, bool, bool, bool, bool, u32)> = None;
    // un calque immobile par écran visité ; chaque fissure garde son image toute prête
    let mut backend = backend::Backend::new(conf.cfg.rendu.carte_graphique);
    let mut cracks: Vec<Crack> = Vec::new();
    let mut swarm = Swarm::new();
    let mut rng = Rng::seeded();

    let start = Instant::now();
    let mut last = start;
    let mut user_paused = false;
    let mut fs_paused = false;
    // curseurs Windows appliqués : (remplacer, variantes, échelle en %, génération des réglages)
    let mut applied: Option<(bool, bool, u32, u32)> = None;
    let mut size_applied = cursor_size;
    let mut conf_gen = 0u32;
    let mut was_down = false;
    let mut last_state = "";
    let mut last_handle = 0isize;
    let mut cleared = false;
    let mut backend_gen = 0u32;
    let (mut next_conf, mut next_fs) = (start, start);
    let mut stat_send = Duration::ZERO;
    let (mut stat_frames, mut stat_time, mut stat_busy, mut stat_max, mut next_stat) =
        (0u32, 0f64, Duration::ZERO, Duration::ZERO, start + Duration::from_secs(60));

    loop {
        if !win::pump() {
            break;
        }
        match win::take_command() {
            win::CMD_QUIT => break,
            win::CMD_PAUSE => {
                user_paused = !user_paused;
                win::set_paused_label(user_paused);
                log::info!("{}", if user_paused { "pause" } else { "reprise" });
            }
            win::CMD_CONFIG => win::open_file(&config::path()),
            _ => {}
        }
        while let Ok(c) = ipc_rx.try_recv() {
            match c {
                ipc::Cmd::SetPaused(p) if p != user_paused => {
                    user_paused = p;
                    win::set_paused_label(p);
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

        let cur = win::cursor_state();
        if now >= next_fs {
            next_fs = now + Duration::from_millis(250);
            let fs = cur.as_ref().is_some_and(|c| win::fullscreen_under(c.pos.0, c.pos.1));
            if fs != fs_paused {
                fs_paused = fs;
                log::info!("{}", if fs { "plein écran : en veille" } else { "fin du plein écran" });
            }
        }
        if now >= next_conf {
            next_conf = now + Duration::from_secs(1);
            monitors = win::monitors();
            cursor_size = win::cursor_base_size();
            if conf.reload() {
                conf_gen += 1;
                shared.set_sensitivity(conf.cfg.audio.sensibilite_kick);
                match (conf.cfg.audio.actif, audio_thread.is_some()) {
                    (true, false) => audio_thread = audio::start(shared.clone()).ok(),
                    (false, true) => audio_thread = None,
                    _ => {}
                }
            }
        }
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

        let active = !user_paused && !fs_paused;
        // curseurs Windows : refaits si les réglages ou l'échelle de l'écran changent
        let scale_pct = cur.as_ref().map_or(100, |c| (win::scale_at(c.pos.0, c.pos.1) * 100.0).round() as u32);
        // taille du pointeur de Windows : quand elle change, Windows recharge ses curseurs
        // (les nôtres sont perdus) ; on les refait à la nouvelle taille
        let size = if cfg.curseur.taille_windows { cursor_size } else { 32 };
        let want = active.then_some((cfg.curseur.remplacer_curseurs, cfg.curseur.variantes, scale_pct, conf_gen));
        if want != applied || size != size_applied {
            win::restore_cursors();
            let s = scale_pct as f32 / 100.0 * size as f32 / 32.0;
            match want {
                Some((true, true, ..)) => _ = cursors::apply(&Role::ALL, cfg, s),
                Some((true, false, ..)) => _ = cursors::apply(&[Role::Normal], cfg, s),
                _ => {}
            }
            applied = want;
            size_applied = size;
        }
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
        if cfg.diagnostic.curseur {
            if let Some(c) = cur.as_ref().filter(|c| c.handle != last_handle) {
                let down = unsafe { GetAsyncKeyState(VK_LBUTTON.0 as i32) } < 0;
                log::info!(
                    "diagnostic : curseur {} → {} sur {}{}",
                    win::cursor_name(last_handle),
                    win::cursor_name(c.handle),
                    win::app_under(c.pos.0, c.pos.1),
                    if down { ", bouton enfoncé" } else { "" }
                );
                last_handle = c.handle;
            }
        }
        let Some(cur) = cur.filter(|c| active && c.showing) else {
            if !cleared {
                backend.clear();
                cracks.clear();
                cleared = true;
            }
            std::thread::sleep(Duration::from_millis(if active { 16 } else { 100 }));
            continue;
        };
        cleared = false;
        let busy_start = Instant::now();
        let scale = scale_pct as f32 / 100.0;
        let cursor = (cur.pos.0 as f32, cur.pos.1 as f32);

        // clic gauche : nouvelle fissure (son image est faite une fois, puis recopiée)
        let down = unsafe { GetAsyncKeyState(VK_LBUTTON.0 as i32) } < 0;
        if down && !was_down && cfg.fissures.actives {
            if cracks.len() >= MAX_CRACKS {
                cracks.remove(0);
            }
            cracks.push(Crack::new(cursor.0, cursor.1, t, cfg.fissures.taille * scale, &mut rng));
        }
        was_down = down;

        // repos : souris immobile et pas de musique depuis 2 s → cadence réduite
        let au = shared.view();
        if cur.pos != last_pos {
            last_pos = cur.pos;
            last_move = now;
        }
        if au.bass > 0.03 || au.kicks != last_kicks {
            last_kicks = au.kicks;
            last_sound = now;
        }
        let idle = cfg.rendu.cadence_repos > 0 && now - last_move > IDLE && now - last_sound > IDLE && !down;
        let cadence = match (idle, cfg.rendu.cadence) {
            (true, 0) => cfg.rendu.cadence_repos,
            (true, c) => c.min(cfg.rendu.cadence_repos),
            (false, c) => c,
        };
        // cadence choisie : on saute les images en trop (clics et curseur restent lus à chaque tour)
        if cadence > 0 && (now - last).as_secs_f64() < 1.0 / cadence as f64 - 0.002 {
            if unsafe { DwmFlush() }.is_err() {
                std::thread::sleep(Duration::from_millis(4));
            }
            continue;
        }
        let dt = (now - last).as_secs_f32().clamp(0.0005, 0.05);
        last = now;

        // simulation
        swarm.update(t, dt, cursor, au, cfg, scale);
        // teinte de l'éclairage (Prisme) : suivie en douceur, en passant par le plus court
        sync_hue = match (cfg.couleur.synchro_prisme, prisme.current(), sync_hue) {
            (true, Some(target), Some(h)) => {
                let d = (target - h + 540.0).rem_euclid(360.0) - 180.0;
                Some((h + d * (dt * 4.0).min(1.0)).rem_euclid(360.0))
            }
            (true, Some(target), None) => Some(target),
            _ => None,
        };
        render::set_sync_hue(sync_hue);
        if sync_hue.is_some() != synced {
            synced = sync_hue.is_some();
            log::info!("teinte : {}", if synced { "celle de l'éclairage (Prisme)" } else { "cycle propre" });
        }
        let life = cfg.fissures.duree.max(0.2) as f64;
        cracks.retain(|c| t - c.born <= life);
        if cfg.rendu.carte_graphique != backend.wants_gpu() && conf_gen != backend_gen {
            backend = backend::Backend::new(cfg.rendu.carte_graphique);
        }
        backend_gen = conf_gen;

        // dessin : sur chaque écran que l'essaim ou une fissure touche
        let (x0, y0, x1, y1) = swarm.bounds(cursor, scale);
        let swarm_rect = (x0.floor() as i32, y0.floor() as i32, x1.ceil() as i32, y1.ceil() as i32);
        let crack_rects: Vec<layer::IRect> = cracks.iter().map(Crack::rect).collect();
        let rgb = render::prisme(cfg, t, 1.0);
        let env = swarm.env;
        let draw = |cv: &mut dyn canvas::Canvas, here: bool| {
            for c in &cracks {
                let age = t - c.born;
                // éclat coloré au début et sur les kicks, fondu sur la dernière seconde
                let glow = ((1.0 - age / 0.45).max(0.0) as f32).max(env * 0.6);
                c.draw(cv, glow, rgb, (life - age).clamp(0.0, 1.0) as f32);
            }
            if here {
                swarm.draw(cv, t, cfg, scale, cursor);
            }
        };
        stat_send += backend.frame(&monitors, swarm_rect, &crack_rects, &draw);

        let busy = busy_start.elapsed();
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
        // cadence : une image par rafraîchissement de l'écran
        if unsafe { DwmFlush() }.is_err() {
            std::thread::sleep(Duration::from_millis(7));
        }
    }
    drop(audio_thread);
    Ok(())
}
