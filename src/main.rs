//! Essaim : curseur animé réactif à la musique (point, comètes et essaim en orbite, fissures au clic).
//!
//! Options : `--quitter` ferme l'instance en cours ; `--restaurer-curseur` remet les curseurs
//! Windows d'origine (utile si l'appli a été tuée sans pouvoir le faire elle-même).

#![cfg_attr(not(test), windows_subsystem = "windows")]

mod audio;
mod config;
mod dsp;
mod render;
mod win;

use render::{Crack, Rng, Swarm};
use std::io::Write;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tiny_skia::Pixmap;
use win::Surface;
use windows::core::w;
use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS, LPARAM, WPARAM};
use windows::Win32::Graphics::Dwm::DwmFlush;
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::HiDpi::{SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON};
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
    let mut swarm_surface = Surface::new()?;
    let mut crack_pool: Vec<Surface> = Vec::new();
    struct Live {
        crack: Crack,
        surface: Surface,
        pm: Pixmap,
        clean: bool,
    }
    let mut cracks: Vec<Live> = Vec::new();
    let mut swarm = Swarm::new();
    let mut rng = Rng::seeded();
    let mut pm: Option<Pixmap> = None;

    let start = Instant::now();
    let mut last = start;
    let mut user_paused = false;
    let mut fs_paused = false;
    let mut arrow_hidden = false;
    let mut was_down = false;
    let mut last_state = "";
    let (mut next_conf, mut next_raise, mut next_fs) = (start, start, start);
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
        let want_hidden = active && cfg.curseur.masquer_fleche;
        if want_hidden != arrow_hidden {
            if want_hidden {
                win::hide_arrow();
            } else {
                win::restore_cursors();
            }
            arrow_hidden = want_hidden;
        }
        let state = match &cur {
            _ if !active => "en pause",
            None => "position du curseur illisible",
            Some(c) if !c.showing => "curseur masqué par Windows",
            Some(c) if c.arrow => "affiché (flèche)",
            Some(_) => "affiché (autre curseur)",
        };
        if state != last_state {
            log::info!("état : {state}");
            last_state = state;
        }
        let Some(cur) = cur.filter(|c| active && c.showing) else {
            swarm_surface.hide();
            for l in cracks.drain(..) {
                let mut s = l.surface;
                s.hide();
                crack_pool.push(s);
            }
            std::thread::sleep(Duration::from_millis(if active { 16 } else { 100 }));
            continue;
        };
        let busy_start = Instant::now();
        let scale = win::scale_at(cur.pos.0, cur.pos.1);
        let cursor = (cur.pos.0 as f32, cur.pos.1 as f32);

        // clic gauche : nouvelle fissure
        let down = unsafe { GetAsyncKeyState(VK_LBUTTON.0 as i32) } < 0;
        if down && !was_down && cfg.fissures.actives {
            if cracks.len() >= MAX_CRACKS {
                let old = cracks.remove(0);
                crack_pool.push(old.surface);
            }
            let crack = Crack::new(cursor.0, cursor.1, t, cfg.fissures.taille * scale, &mut rng);
            let side = (crack.ext * 2.0).ceil() as u32;
            if let (Some(p), Some(surface)) = (Pixmap::new(side, side), crack_pool.pop().or_else(|| Surface::new().ok())) {
                cracks.push(Live { crack, surface, pm: p, clean: false });
            }
        }
        was_down = down;

        // essaim
        swarm.update(t, dt, cursor, shared.view(), cfg, scale);
        let size = Swarm::window_size(cfg, scale);
        let pmr = match &mut pm {
            Some(p) if p.width() == size => p,
            slot => slot.insert(Pixmap::new(size, size).expect("taille d'image valide")),
        };
        pmr.fill(tiny_skia::Color::TRANSPARENT);
        let half = (size / 2) as i32;
        let origin = (cur.pos.0 - half, cur.pos.1 - half);
        let dot = (cur.arrow && cfg.curseur.masquer_fleche).then_some(cursor);
        swarm.draw(pmr, (origin.0 as f32, origin.1 as f32), t, cfg, scale, dot);
        let _ = swarm_surface.upload(pmr);
        swarm_surface.present(origin.0, origin.1, 255);

        // fissures : redessinées pendant l'éclat, puis simple fondu
        let life = cfg.fissures.duree.max(0.2) as f64;
        let rgb = render::prisme(cfg, t, 1.0);
        let mut i = 0;
        while i < cracks.len() {
            let l = &mut cracks[i];
            let age = t - l.crack.born;
            if age > life {
                let mut s = cracks.remove(i).surface;
                s.hide();
                crack_pool.push(s);
                continue;
            }
            let glow = ((1.0 - age / 0.45).max(0.0) as f32).max(swarm.env * 0.6);
            if glow > 0.02 || !l.clean {
                l.pm.fill(tiny_skia::Color::TRANSPARENT);
                l.crack.draw(&mut l.pm, glow, rgb);
                let _ = l.surface.upload(&l.pm);
                l.clean = glow <= 0.02;
            }
            let alpha = ((life - age).min(1.0) * 255.0) as u8;
            let e = l.crack.ext as i32;
            l.surface.present(l.crack.x as i32 - e, l.crack.y as i32 - e, alpha);
            i += 1;
        }
        if now >= next_raise {
            next_raise = now + Duration::from_secs(2);
            for l in &cracks {
                l.surface.raise();
            }
            swarm_surface.raise();
        }

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
