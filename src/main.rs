//! Essaim : curseur animé réactif à la musique (point, comètes et essaim en orbite, fissures au clic).
//!
//! Le point et ses variantes sont de vrais curseurs Windows (`cursors.rs`) : aucun retard. L'appli
//! dessine par-dessus les sphères, l'éclat du kick et les fissures.
//!
//! Options : `--quitter` ferme l'instance en cours ; `--restaurer-curseur` remet les curseurs
//! Windows d'origine (fermeture, plantage, pause et plein écran le font tout seuls).

#![cfg_attr(not(test), windows_subsystem = "windows")]

mod audio;
mod config;
mod cursors;
mod dsp;
mod layer;
mod pointer;
mod render;
mod win;

use render::{Crack, Rng, Swarm};
use std::io::Write;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tiny_skia::Pixmap;
use windows::core::w;
use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS, LPARAM, WPARAM};
use windows::Win32::Graphics::Dwm::DwmFlush;
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::HiDpi::{SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON};
use pointer::Role;
use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, PostMessageW, WM_CLOSE};

const MAX_CRACKS: usize = 12;

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
    // un calque immobile par écran visité ; chaque fissure garde son image toute prête
    let mut layers: Vec<layer::Layer> = Vec::new();
    struct Live {
        crack: Crack,
        pm: Pixmap,
        clean: bool,
    }
    let mut cracks: Vec<Live> = Vec::new();
    let mut swarm = Swarm::new();
    let mut rng = Rng::seeded();

    let start = Instant::now();
    let mut last = start;
    let mut user_paused = false;
    let mut fs_paused = false;
    // curseurs Windows appliqués : (remplacer, variantes, échelle en %, génération des réglages)
    let mut applied: Option<(bool, bool, u32, u32)> = None;
    let mut conf_gen = 0u32;
    let mut was_down = false;
    let mut last_state = "";
    let mut last_handle = 0isize;
    let (mut next_conf, mut next_fs) = (start, start);
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
        let now = Instant::now();
        let dt = (now - last).as_secs_f32().clamp(0.0005, 0.05);
        last = now;
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

        let active = !user_paused && !fs_paused;
        // curseurs Windows : refaits si les réglages ou l'échelle de l'écran changent
        let scale_pct = cur.as_ref().map_or(100, |c| (win::scale_at(c.pos.0, c.pos.1) * 100.0).round() as u32);
        let want = active.then_some((cfg.curseur.remplacer_curseurs, cfg.curseur.variantes, scale_pct, conf_gen));
        if want != applied {
            win::restore_cursors();
            match want {
                Some((true, true, ..)) => _ = cursors::apply(&Role::ALL, cfg, scale_pct as f32 / 100.0),
                Some((true, false, ..)) => _ = cursors::apply(&[Role::Normal], cfg, scale_pct as f32 / 100.0),
                _ => {}
            }
            applied = want;
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
            layers.iter_mut().for_each(layer::Layer::hide);
            cracks.clear();
            std::thread::sleep(Duration::from_millis(if active { 16 } else { 100 }));
            continue;
        };
        let busy_start = Instant::now();
        let scale = scale_pct as f32 / 100.0;
        let cursor = (cur.pos.0 as f32, cur.pos.1 as f32);

        // clic gauche : nouvelle fissure (son image est faite une fois, puis recopiée)
        let down = unsafe { GetAsyncKeyState(VK_LBUTTON.0 as i32) } < 0;
        if down && !was_down && cfg.fissures.actives {
            if cracks.len() >= MAX_CRACKS {
                cracks.remove(0);
            }
            let crack = Crack::new(cursor.0, cursor.1, t, cfg.fissures.taille * scale, &mut rng);
            let side = (crack.ext * 2.0).ceil() as u32;
            if let Some(p) = Pixmap::new(side, side) {
                cracks.push(Live { crack, pm: p, clean: false });
            }
        }
        was_down = down;

        // simulation
        swarm.update(t, dt, cursor, shared.view(), cfg, scale);
        let life = cfg.fissures.duree.max(0.2) as f64;
        cracks.retain(|l| t - l.crack.born <= life);
        let rgb = render::prisme(cfg, t, 1.0);
        for l in &mut cracks {
            // éclat coloré au début et sur les kicks : image refaite ; sinon réutilisée
            let glow = ((1.0 - (t - l.crack.born) / 0.45).max(0.0) as f32).max(swarm.env * 0.6);
            if glow > 0.02 || !l.clean {
                l.pm.fill(tiny_skia::Color::TRANSPARENT);
                l.crack.draw(&mut l.pm, glow, rgb);
                l.clean = glow <= 0.02;
            }
        }

        // calques : celui de l'écran de la souris porte l'essaim ; chacun porte ses fissures
        let Some((mon, mon_rect)) = win::monitor_at(cur.pos.0, cur.pos.1) else { continue };
        if !layers.iter().any(|l| l.mon == mon) {
            match layer::Layer::new(mon, mon_rect) {
                Ok(l) => layers.push(l),
                Err(e) => log::warn!("calque impossible pour l'écran {mon_rect:?} : {e:#}"),
            }
        }
        let (x0, y0, x1, y1) = swarm.bounds(cursor, scale);
        let swarm_rect = (x0.floor() as i32, y0.floor() as i32, x1.ceil() as i32, y1.ceil() as i32);
        let crack_rect = |l: &Live| {
            let (e, x, y) = (l.crack.ext.ceil() as i32 + 1, l.crack.x as i32, l.crack.y as i32);
            (x - e, y - e, x + e, y + e)
        };
        for lay in &mut layers {
            let here = lay.mon == mon;
            let mut dirty = here.then_some(swarm_rect);
            for l in cracks.iter().filter(|l| layer::intersects(lay.rect, crack_rect(l))) {
                dirty = layer::union(dirty, Some(crack_rect(l)));
            }
            lay.frame(dirty, |pm, origin| {
                for l in cracks.iter() {
                    let age = t - l.crack.born;
                    let paint = tiny_skia::PixmapPaint { opacity: (life - age).clamp(0.0, 1.0) as f32, ..Default::default() };
                    let e = l.crack.ext as i32;
                    let (x, y) = (l.crack.x as i32 - e - origin.0 as i32, l.crack.y as i32 - e - origin.1 as i32);
                    pm.draw_pixmap(x, y, l.pm.as_ref(), &paint, tiny_skia::Transform::identity(), None);
                }
                if here {
                    swarm.draw(pm, origin, t, cfg, scale, cursor);
                }
            });
        }
        // écrans quittés depuis 10 s et vides : mémoire rendue
        layers.retain(|l| l.mon == mon || l.empty_since.is_none_or(|s| now - s < Duration::from_secs(10)));

        let busy = busy_start.elapsed();
        stat_frames += 1;
        stat_time += dt as f64;
        stat_busy += busy;
        stat_max = stat_max.max(busy);
        if now >= next_stat {
            log::info!(
                "images : {:.0}/s, calcul moyen {:.2} ms, pire {:.2} ms",
                stat_frames as f64 / stat_time.max(1e-3),
                stat_busy.as_secs_f64() * 1000.0 / stat_frames.max(1) as f64,
                stat_max.as_secs_f64() * 1000.0
            );
            (stat_frames, stat_time, stat_busy, stat_max, next_stat) = (0, 0.0, Duration::ZERO, Duration::ZERO, now + Duration::from_secs(60));
        }
        // cadence : une image par rafraîchissement de l'écran
        if unsafe { DwmFlush() }.is_err() {
            std::thread::sleep(Duration::from_millis(7));
        }
    }
    drop(audio_thread);
    Ok(())
}
