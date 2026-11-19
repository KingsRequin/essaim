//! Fenêtre pour le stream : l'essaim au centre d'une fenêtre ordinaire, sur un fond uni (vert,
//! bleu, magenta ou noir), à ajouter dans OBS comme « Capture de fenêtre » avec une incrustation
//! chroma (ou en fusion « Écran » sur fond noir). Elle reste capturable même quand l'essaim autour
//! du pointeur est masqué des captures (`[rendu] masquer_captures`).
//!
//! Elle a son propre fil, son propre essaim et son propre dessin (processeur, tiny-skia) : elle
//! suit le son et les réglages, pas le pointeur, et continue quand l'essaim autour du pointeur est
//! en pause, en plein écran ou masqué par la frappe. Ouverte sur demande seulement (menu de
//! l'icône, réglages, `essaim.exe --stream`) : aucune fenêtre n'apparaît d'elle-même sous la souris.

use crate::audio::AudioShared;
use crate::canvas::{Canvas, PixCanvas};
use crate::config::{self, Fond};
use crate::effets::Effets;
use crate::render::Swarm;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tiny_skia::{Color, Pixmap};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::DwmFlush;
use windows::Win32::Graphics::Gdi::{
    BeginPaint, EndPaint, GetDC, ReleaseDC, SetDIBitsToDevice, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, PAINTSTRUCT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;

const CLASSE: PCWSTR = w!("EssaimStream");
/// Fenêtre ouverte (0 : aucune).
static FENETRE: AtomicIsize = AtomicIsize::new(0);
/// Taille de départ de la zone de dessin (px).
const TAILLE: i32 = 480;

/// La fenêtre pour le stream est-elle ouverte ? (Le son doit alors continuer d'être écouté.)
pub fn ouverte() -> bool {
    FENETRE.load(Ordering::Relaxed) != 0
}

/// Ouvre la fenêtre (ou la ramène devant si elle l'est déjà).
pub fn ouvrir(son: Arc<AudioShared>) {
    let h = FENETRE.load(Ordering::Relaxed);
    if h != 0 {
        let hwnd = HWND(h as *mut _);
        unsafe {
            if IsWindow(Some(hwnd)).as_bool() {
                let _ = ShowWindow(hwnd, SW_RESTORE);
                let _ = SetForegroundWindow(hwnd);
                return;
            }
        }
    }
    let _ = std::thread::Builder::new().name("essaim-stream".into()).spawn(move || {
        if let Err(e) = run(son) {
            log::warn!("stream : fenêtre impossible à ouvrir ({e})");
        }
        FENETRE.store(0, Ordering::Relaxed);
        log::info!("stream : fenêtre fermée");
    });
}

extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_ERASEBKGND => LRESULT(1),
            WM_PAINT => {
                // l'image est envoyée par la boucle à chaque tour ; ici, la zone est seulement validée
                let mut ps = PAINTSTRUCT::default();
                BeginPaint(hwnd, &mut ps);
                let _ = EndPaint(hwnd, &ps);
                LRESULT(0)
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wp, lp),
        }
    }
}

/// Couleur du fond (r, g, b).
pub fn couleur_fond(f: Fond) -> (u8, u8, u8) {
    match f {
        Fond::Vert => (0, 255, 0),
        Fond::Bleu => (0, 0, 255),
        Fond::Magenta => (255, 0, 255),
        Fond::Noir => (0, 0, 0),
    }
}


/// Échelle de l'essaim pour une zone de dessin `l` × `h` : il y tient, lueurs comprises.
fn echelle(l: i32, h: i32) -> f32 {
    (l.min(h) as f32 / 205.0).clamp(0.4, 8.0)
}

fn run(son: Arc<AudioShared>) -> windows::core::Result<()> {
    let hwnd = unsafe {
        let instance = GetModuleHandleW(None)?;
        let wc = WNDCLASSW {
            lpfnWndProc: Some(proc),
            hInstance: instance.into(),
            lpszClassName: CLASSE,
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            hIcon: crate::win::app_icon().unwrap_or_default(),
            ..Default::default()
        };
        RegisterClassW(&wc); // déjà inscrite à la deuxième ouverture : sans conséquence
        let mut r = RECT { left: 0, top: 0, right: TAILLE, bottom: TAILLE };
        let _ = AdjustWindowRectEx(&mut r, WS_OVERLAPPEDWINDOW, false, WINDOW_EX_STYLE(0));
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            CLASSE,
            w!("Essaim — stream"),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            r.right - r.left,
            r.bottom - r.top,
            None,
            None,
            Some(instance.into()),
            None,
        )?
    };
    FENETRE.store(hwnd.0 as isize, Ordering::Relaxed);
    log::info!("stream : fenêtre ouverte");

    let mut conf = config::Watched::load();
    let (mut swarm, mut effets) = (Swarm::new(), Effets::new());
    let mut pm: Option<Pixmap> = None;
    let mut bgra = Vec::new();
    let debut = Instant::now();
    let (mut avant, mut relu) = (debut, debut);
    let mut pistes = crate::media::pistes();
    let mut doux = crate::win::sans_flashs(conf.cfg.rendu.sans_flashs);
    loop {
        unsafe {
            let mut msg = MSG::default();
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                if msg.message == WM_QUIT {
                    return Ok(());
                }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        let now = Instant::now();
        if now - relu >= Duration::from_secs(1) {
            relu = now;
            conf.reload();
            doux = crate::win::sans_flashs(conf.cfg.rendu.sans_flashs);
        }
        let cfg = &conf.cfg;
        let mut zone = RECT::default();
        let _ = unsafe { GetClientRect(hwnd, &mut zone) };
        let (l, h) = (zone.right - zone.left, zone.bottom - zone.top);
        if l <= 0 || h <= 0 || unsafe { IsIconic(hwnd) }.as_bool() {
            avant = now;
            std::thread::sleep(Duration::from_millis(50));
            continue;
        }
        if pm.as_ref().is_none_or(|p| p.width() != l as u32 || p.height() != h as u32) {
            pm = Pixmap::new(l as u32, h as u32);
        }
        let Some(pm) = pm.as_mut() else { continue };
        let t = (now - debut).as_secs_f64();
        let dt = (now - avant).as_secs_f32().clamp(0.0005, 0.25);
        avant = now;
        // l'essaim au centre, à une taille qui suit celle de la fenêtre
        let scale = echelle(l, h);
        let centre = (l as f32 / 2.0, h as f32 / 2.0);
        let au = son.view();
        let p = crate::media::pistes();
        if p != pistes {
            swarm.evenement(crate::config::Evenement::Piste, centre.0, centre.1, t, cfg, scale);
        }
        pistes = p;
        swarm.doux = doux;
        swarm.avance(t, dt, centre, au, cfg, scale);
        // pas de fissures dans la fenêtre pour le stream
        swarm.fissures.clear();
        effets.avance(t, dt, centre, scale, swarm.env, &son, cfg, &[]);
        let (r, g, b) = couleur_fond(cfg.stream.fond);
        pm.fill(Color::from_rgba8(r, g, b, 255));
        {
            let mut cv = PixCanvas { pm, origin: (0.0, 0.0) };
            effets.dessiner(&mut cv, t, cfg, scale, centre);
            swarm.draw(&mut cv, t, cfg, scale, centre);
            // le pointeur, au centre (dans la fenêtre, il n'y a pas celui de Windows)
            cv.circle(centre.0, centre.1, 2.2 * scale, (255, 255, 255), 0.95);
        }
        bgra.resize(pm.data().len(), 0);
        crate::win::rgba_vers_bgra(&mut bgra, pm.data());
        unsafe {
            let bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: l,
                    biHeight: -h, // de haut en bas
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let dc = GetDC(Some(hwnd));
            SetDIBitsToDevice(dc, 0, 0, l as u32, h as u32, 0, 0, 0, h as u32, bgra.as_ptr().cast(), &bmi, DIB_RGB_COLORS);
            ReleaseDC(Some(hwnd), dc);
            // une image par rafraîchissement de l'écran
            if DwmFlush().is_err() {
                std::thread::sleep(Duration::from_millis(16));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn l_essaim_tient_dans_la_fenetre_et_le_fond_reste_aux_bords() {
        // une seconde de musique avec kicks, à la taille de départ : les bords restent du fond
        let cfg = config::Config::default();
        let mut s = Swarm::new();
        let scale = echelle(TAILLE, TAILLE);
        let centre = (TAILLE as f32 / 2.0, TAILLE as f32 / 2.0);
        for i in 0..60 {
            let au = crate::audio::AudioView { kicks: i / 15, bass: 0.8, pitch: 35.0, ..Default::default() };
            s.avance(i as f64 / 60.0, 1.0 / 60.0, centre, au, &cfg, scale);
        }
        let mut pm = Pixmap::new(TAILLE as u32, TAILLE as u32).unwrap();
        pm.fill(Color::from_rgba8(0, 255, 0, 255));
        s.draw(&mut PixCanvas { pm: &mut pm, origin: (0.0, 0.0) }, 1.0, &cfg, scale, centre);
        // bande de 4 px le long des bords : rien que le fond
        let n = TAILLE as usize;
        let fond = |x: usize, y: usize| pm.data()[(y * n + x) * 4..(y * n + x) * 4 + 4] == [0, 255, 0, 255];
        for i in 0..n {
            for j in 0..4 {
                assert!(fond(i, j) && fond(i, n - 1 - j) && fond(j, i) && fond(n - 1 - j, i), "dessin au bord ({i}, {j})");
            }
        }
        assert!(pm.data().chunks_exact(4).any(|p| p != [0, 255, 0, 255]), "rien de dessiné");
    }
}
