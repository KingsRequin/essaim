//! Fenêtre pour le stream : l'essaim au centre d'une fenêtre ordinaire, affichée par Direct3D
//! (chaîne d'échange DXGI) et dessinée par Direct2D, avec le moteur des calques (`gpu.rs`). Le
//! fond est transparent par défaut : dans OBS, « Capture de jeu » avec « Autoriser la
//! transparence » et « Valeurs alpha prémultipliées » ; OBS reprend l'image sur la carte
//! graphique, sans recopie ni incrustation. Fond uni au choix (`[stream] fond`) pour une
//! « Capture de fenêtre » avec incrustation chroma. Elle reste capturable même quand l'essaim
//! autour du pointeur est masqué des captures (`[rendu] masquer_captures`).
//!
//! L'essaim y est simulé à 100 % autour d'un centre virtuel, puis agrandi pour tenir dans la
//! fenêtre : sa portée (le plus grand demi-côté de ce qu'il dessine) est mesurée sur quelques
//! secondes de musique synthétique à chaque changement des réglages, et suivie en direct si la
//! vraie musique le pousse plus loin. Rien ne sort ainsi de la fenêtre, quelle que soit sa forme.
//!
//! Elle a son propre fil, son propre essaim et ses effets : elle suit le son et les réglages, pas
//! le pointeur, et continue quand l'essaim autour du pointeur est en pause, en plein écran ou
//! masqué par la frappe. Ouverte sur demande seulement (menu de l'icône, réglages,
//! `essaim.exe --stream`) : aucune fenêtre n'apparaît d'elle-même sous la souris.

use crate::audio::{AudioShared, AudioView};
use crate::canvas::Canvas;
use crate::config::{self, Config, Fond};
use crate::effets::Effets;
use crate::gpu::{make_disc, Pinceau};
use crate::render::Swarm;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use windows::core::{w, Interface, PCWSTR};
use windows::Win32::Foundation::{HMODULE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Direct2D::Common::{D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_PIXEL_FORMAT};
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE, D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP};
use windows::Win32::Graphics::Direct3D11::{D3D11CreateDevice, ID3D11Device, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_ALPHA_MODE_IGNORE, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_UNKNOWN, DXGI_SAMPLE_DESC};
use windows::Win32::Graphics::Dxgi::*;
use windows::Win32::Graphics::Gdi::{BeginPaint, EndPaint, PAINTSTRUCT};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows_numerics::Matrix3x2;

const CLASSE: PCWSTR = w!("EssaimStream");
/// Fenêtre ouverte (0 : aucune).
static FENETRE: AtomicIsize = AtomicIsize::new(0);
/// Taille de départ de la zone de dessin (px).
const TAILLE: i32 = 480;
/// Marge laissée vide le long des bords (px de la fenêtre).
const BORD: f32 = 4.0;
/// Musique synthétique jouée pour mesurer la portée (s) et images par seconde de la mesure.
const CALIBRAGE: f64 = 12.0;
const CALIBRAGE_IPS: f64 = 60.0;
/// Portée vue en direct : oubliée en ~20 s ; la portée affichée la rejoint vite quand elle
/// grandit (l'essaim rapetisse), doucement quand elle diminue.
const OUBLI: f32 = 20.0;
const GRANDIT: f32 = 12.0;
const DIMINUE: f32 = 0.5;
/// Rayon du point blanc qui tient lieu de pointeur au centre (px à 100 %).
const POINTEUR: f32 = 2.2;

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
                // l'image est présentée par la boucle à chaque tour ; ici, la zone est seulement validée
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

/// Couleur du fond (r, g, b, opacité), prémultipliée : transparent = tout à zéro.
pub fn couleur_fond(f: Fond) -> D2D1_COLOR_F {
    let (r, g, b, a) = match f {
        Fond::Transparent => (0.0, 0.0, 0.0, 0.0),
        Fond::Vert => (0.0, 1.0, 0.0, 1.0),
        Fond::Bleu => (0.0, 0.0, 1.0, 1.0),
        Fond::Magenta => (1.0, 0.0, 1.0, 1.0),
        Fond::Noir => (0.0, 0.0, 0.0, 1.0),
    };
    D2D1_COLOR_F { r, g, b, a }
}

// --- Taille de l'essaim ----------------------------------------------------------------------

/// Demi-côté (px à 100 %) du carré centré en (0, 0) qui contient tout ce qui est dessiné cette
/// image : essaim (`Swarm::bounds`, éclat du kick compris), réticule et spectre, point central.
fn demi_cote(swarm: &Swarm, effets: &Effets, cfg: &Config) -> f32 {
    let (x0, y0, x1, y1) = swarm.bounds((0.0, 0.0), 1.0, cfg.eclat.taille);
    let mut e = (-x0).max(-y0).max(x1).max(y1).max(POINTEUR);
    if let Some(r) = effets.rect_pointeur((0.0, 0.0), 1.0, cfg) {
        e = e.max((-r.0).max(-r.1).max(r.2).max(r.3) as f32);
    }
    if e.is_finite() {
        e
    } else {
        POINTEUR
    }
}

/// Musique synthétique au temps `t` (s) : 128 BPM, kick sur chaque temps, caisse claire un temps
/// sur deux, charleston sur les croches, basse et hauteur qui ondulent, balance qui va d'un côté
/// à l'autre ; de 5 à 7 s une montée (plus de kick, roulement de caisse claire qui s'accélère),
/// puis un drop.
fn musique(t: f64) -> AudioView {
    let temps = 60.0 / 128.0;
    let (montee, drop) = (5.0, 7.0);
    let battus = |t: f64| (t / temps) as u32;
    let kicks = if t < montee { battus(t) } else if t < drop { battus(montee) } else { battus(t) - battus(drop) + battus(montee) };
    let roulement = if t < montee { 0 } else { ((t.min(drop) - montee).powi(2) * 4.0) as u32 };
    let tension = if (montee..drop).contains(&t) { ((t - montee) / (drop - montee)) as f32 } else { 0.0 };
    let tf = t as f32;
    AudioView {
        kicks,
        bass: if (montee..drop).contains(&t) { 0.2 } else { 0.6 + 0.4 * (tf * 1.7).sin() },
        pitch: 45.0 + 15.0 * (tf * 0.9).sin(),
        aigus: (t / (temps / 2.0)) as u32,
        brillance: 0.6 + 0.4 * (tf * 2.3).sin().abs(),
        bpm: Some(128.0),
        phase: ((t / temps).fract()) as f32,
        relais: false,
        caisses: battus(t) / 2 + roulement,
        drops: u32::from(t >= drop),
        tension,
        balance: (tf * 0.8).sin(),
    }
}

/// Portée de l'essaim avec `cfg` (px à 100 %) : le plus grand demi-côté atteint pendant
/// `CALIBRAGE` s de musique synthétique, changement de piste compris.
fn portee(cfg: &Config) -> f32 {
    let (mut swarm, effets) = (Swarm::new(), Effets::new());
    let dt = 1.0 / CALIBRAGE_IPS;
    let n = (CALIBRAGE * CALIBRAGE_IPS) as usize;
    let mut max = POINTEUR;
    for i in 0..n {
        let t = i as f64 * dt;
        if i == (1.0 * CALIBRAGE_IPS) as usize {
            swarm.evenement(config::Evenement::Piste, 0.0, 0.0, t, cfg, 1.0);
        }
        swarm.avance(t, dt as f32, (0.0, 0.0), musique(t), cfg, 1.0);
        swarm.fissures.clear();
        max = max.max(demi_cote(&swarm, &effets, cfg));
    }
    max
}

/// Agrandissement qui fait tenir une portée `p` (px à 100 %) dans une zone `l` × `h`.
fn zoom(l: i32, h: i32, p: f32) -> f32 {
    ((l.min(h) as f32 / 2.0 - BORD).max(1.0) / p.max(1.0)).clamp(0.02, 50.0)
}

/// Portée affichée : rejoint vite `cible` quand elle grandit, doucement quand elle diminue.
fn suivre(affichee: f32, cible: f32, dt: f32) -> f32 {
    if affichee <= 0.0 {
        return cible;
    }
    let vitesse = if cible > affichee { GRANDIT } else { DIMINUE };
    affichee + (cible - affichee) * (1.0 - (-dt * vitesse).exp())
}

// --- Affichage Direct3D / Direct2D -------------------------------------------------------------

/// Périphérique, chaîne d'échange de la fenêtre et contexte Direct2D qui dessine dans son tampon.
struct Rendu {
    _d3d: ID3D11Device,
    chaine: IDXGISwapChain1,
    dc: ID2D1DeviceContext,
    disc: ID2D1Bitmap1,
    /// Tampon de la chaîne vu par Direct2D, et sa taille ; refait quand la fenêtre change de taille.
    cible: Option<(ID2D1Bitmap1, (u32, u32))>,
    pinceau: Pinceau,
}

fn peripherique(genre: D3D_DRIVER_TYPE) -> windows::core::Result<ID3D11Device> {
    let mut d3d = None;
    unsafe { D3D11CreateDevice(None, genre, HMODULE::default(), D3D11_CREATE_DEVICE_BGRA_SUPPORT, None, D3D11_SDK_VERSION, Some(&mut d3d), None, None)? };
    d3d.ok_or_else(|| windows::core::Error::from_hresult(windows::Win32::Foundation::E_FAIL))
}

impl Rendu {
    /// Carte graphique, sinon WARP (rendu Direct3D par le processeur) : la capture de jeu d'OBS
    /// fonctionne avec les deux.
    fn new(hwnd: HWND) -> windows::core::Result<Self> {
        let d3d = match peripherique(D3D_DRIVER_TYPE_HARDWARE) {
            Ok(d) => d,
            Err(e) => {
                log::warn!("stream : carte graphique indisponible ({e}), rendu WARP");
                peripherique(D3D_DRIVER_TYPE_WARP)?
            }
        };
        unsafe {
            let dxgi: IDXGIDevice = d3d.cast()?;
            let fabrique: IDXGIFactory2 = dxgi.GetAdapter()?.GetParent()?;
            let desc = DXGI_SWAP_CHAIN_DESC1 {
                Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
                BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
                BufferCount: 2,
                Scaling: DXGI_SCALING_STRETCH,
                SwapEffect: DXGI_SWAP_EFFECT_FLIP_DISCARD,
                // une fenêtre ordinaire n'est pas composée avec sa transparence : le tampon la
                // garde quand même, et c'est lui que lit la capture de jeu
                AlphaMode: DXGI_ALPHA_MODE_IGNORE,
                ..Default::default()
            };
            let chaine = fabrique.CreateSwapChainForHwnd(&d3d, hwnd, &desc, None, None)?;
            let _ = fabrique.MakeWindowAssociation(hwnd, DXGI_MWA_NO_ALT_ENTER);
            let usine: ID2D1Factory1 = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
            let dc = usine.CreateDevice(&dxgi)?.CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE)?;
            let disc = make_disc(&dc)?;
            Ok(Self { _d3d: d3d, chaine, dc, disc, cible: None, pinceau: Pinceau::default() })
        }
    }

    /// Tampon à la taille `l` × `h`, refait si besoin.
    fn cible(&mut self, l: u32, h: u32) -> windows::core::Result<()> {
        if self.cible.as_ref().is_some_and(|c| c.1 == (l, h)) {
            return Ok(());
        }
        unsafe {
            // toute référence au tampon doit être lâchée avant de le redimensionner
            self.dc.SetTarget(None);
            self.cible = None;
            self.chaine.ResizeBuffers(0, l, h, DXGI_FORMAT_UNKNOWN, DXGI_SWAP_CHAIN_FLAG(0))?;
            let surface: IDXGISurface = self.chaine.GetBuffer(0)?;
            let props = D2D1_BITMAP_PROPERTIES1 {
                pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED },
                dpiX: 96.0,
                dpiY: 96.0,
                bitmapOptions: D2D1_BITMAP_OPTIONS_TARGET | D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
                ..Default::default()
            };
            let bmp = self.dc.CreateBitmapFromDxgiSurface(&surface, Some(&props))?;
            self.dc.SetTarget(&bmp);
            self.cible = Some((bmp, (l, h)));
        }
        Ok(())
    }

    /// Dessine une image de `l` × `h` sur `fond` : `draw` dessine en px à 100 % autour de
    /// (0, 0), agrandi `z` fois et centré. Sans `presenter`, l'image reste dans le tampon (essais).
    fn image(&mut self, l: u32, h: u32, fond: D2D1_COLOR_F, z: f32, presenter: bool, draw: impl FnOnce(&mut dyn Canvas)) -> windows::core::Result<()> {
        self.cible(l, h)?;
        unsafe {
            self.dc.BeginDraw();
            self.dc.SetTransform(&Matrix3x2::identity());
            self.dc.Clear(Some(&fond));
            self.dc.SetTransform(&Matrix3x2 { M11: z, M12: 0.0, M21: 0.0, M22: z, M31: l as f32 / 2.0, M32: h as f32 / 2.0 });
            let peint = self.pinceau.peindre(&self.dc, &self.disc, draw);
            self.dc.EndDraw(None, None)?;
            peint?;
            if presenter {
                // une image par rafraîchissement de l'écran
                self.chaine.Present(1, DXGI_PRESENT(0)).ok()?;
            }
        }
        Ok(())
    }
}

fn run(son: Arc<AudioShared>) -> windows::core::Result<()> {
    let hwnd = unsafe {
        let instance = GetModuleHandleW(None)?;
        let wc = WNDCLASSW {
            lpfnWndProc: Some(proc),
            hInstance: instance.into(),
            lpszClassName: CLASSE,
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            hIcon: crate::win::app_icon(false).unwrap_or_default(),
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
    let resultat = boucle(hwnd, &son);
    unsafe {
        if IsWindow(Some(hwnd)).as_bool() {
            let _ = DestroyWindow(hwnd);
        }
    }
    resultat
}

/// Ce que montre la fenêtre, en px à 100 % autour de (0, 0) : effets, essaim et, au centre, un
/// point blanc pour le pointeur (dans la fenêtre, il n'y a pas celui de Windows).
fn dessiner(cv: &mut dyn Canvas, swarm: &Swarm, effets: &Effets, t: f64, cfg: &Config) {
    effets.dessiner(cv, t, cfg, 1.0, (0.0, 0.0));
    swarm.draw(cv, t, cfg, 1.0, (0.0, 0.0));
    cv.circle(0.0, 0.0, POINTEUR, (255, 255, 255), 0.95);
}

fn boucle(hwnd: HWND, son: &AudioShared) -> windows::core::Result<()> {
    let mut rendu = Rendu::new(hwnd)?;
    log::info!("stream : fenêtre ouverte");
    let mut conf = config::Watched::load();
    let (mut swarm, mut effets) = (Swarm::new(), Effets::new());
    let mut calibree = portee(&conf.cfg);
    let (mut vue, mut affichee) = (0.0f32, 0.0f32);
    let debut = Instant::now();
    let (mut avant, mut relu) = (debut, debut);
    let mut pistes = crate::media::pistes();
    let mut doux = crate::win::sans_flashs(conf.cfg.rendu.sans_flashs);
    let mut panne: Option<Instant> = None;
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
            if conf.reload() {
                calibree = portee(&conf.cfg);
            }
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
        let t = (now - debut).as_secs_f64();
        let dt = (now - avant).as_secs_f32().clamp(0.0005, 0.25);
        avant = now;
        // l'essaim est simulé à 100 % autour de (0, 0), puis agrandi au dessin
        let au = son.view();
        let p = crate::media::pistes();
        if p != pistes {
            swarm.evenement(crate::config::Evenement::Piste, 0.0, 0.0, t, cfg, 1.0);
        }
        pistes = p;
        swarm.doux = doux;
        swarm.avance(t, dt, (0.0, 0.0), au, cfg, 1.0);
        // pas de fissures dans la fenêtre pour le stream
        swarm.fissures.clear();
        effets.avance(t, dt, (0.0, 0.0), 1.0, swarm.env, son, cfg, &[]);
        // taille : la portée mesurée, ou celle vue en direct si la musique pousse plus loin
        vue = (vue * (-dt / OUBLI).exp()).max(demi_cote(&swarm, &effets, cfg));
        affichee = suivre(affichee, calibree.max(vue), dt);
        let z = zoom(l, h, affichee);
        if panne.is_some_and(|p| now - p < Duration::from_secs(1)) {
            std::thread::sleep(Duration::from_millis(50));
            continue;
        }
        let dessin = rendu.image(l as u32, h as u32, couleur_fond(cfg.stream.fond), z, true, |cv| dessiner(cv, &swarm, &effets, t, cfg));
        if let Err(e) = dessin {
            // pilote réinitialisé, sortie de veille… : tout est refait, au plus une fois par seconde
            log::warn!("stream : erreur d'affichage ({e}), nouvel essai");
            panne = Some(now);
            match Rendu::new(hwnd) {
                Ok(r) => rendu = r,
                Err(e) => log::warn!("stream : affichage impossible à refaire ({e})"),
            }
        } else {
            panne = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Plus grand demi-côté atteint par l'essaim de `cfg` sur une autre musique que celle de la
    /// mesure (autre tempo, kicks irréguliers, drop plus tôt), avec un changement de piste.
    fn portee_autre_musique(cfg: &Config) -> f32 {
        let (mut s, effets) = (Swarm::new(), Effets::new());
        let mut max = 0.0f32;
        let ips = 90.0;
        for i in 0..(10.0 * ips) as usize {
            let t = i as f64 / ips;
            let mut au = musique(t * 1.13 + 0.4);
            au.kicks += (t * 0.7) as u32; // quelques kicks en plus, à contretemps
            if i == 300 {
                s.evenement(config::Evenement::Piste, 0.0, 0.0, t, cfg, 1.0);
            }
            s.avance(t, 1.0 / ips as f32, (0.0, 0.0), au, cfg, 1.0);
            s.fissures.clear();
            max = max.max(demi_cote(&s, &effets, cfg));
        }
        max
    }

    #[test]
    fn chaque_preset_tient_dans_la_fenetre() {
        // la portée mesurée couvre ce que l'essaim dessine sur une autre musique, pour chaque
        // preset fourni ; une petite marge pour le hasard des particules (la portée vue en direct
        // prend le relais au-delà)
        let mut cfgs = vec![("défaut".to_string(), Config::default())];
        cfgs.extend(crate::prereglages::builtins().into_iter().map(|p| (p.name, p.cfg)));
        for (nom, cfg) in cfgs {
            let t0 = std::time::Instant::now();
            let p = portee(&cfg);
            eprintln!("{nom:24} portée {p:5.0} px, mesurée en {:.0} ms", t0.elapsed().as_secs_f64() * 1000.0);
            let autre = portee_autre_musique(&cfg);
            assert!(p.is_finite() && p > POINTEUR, "{nom} : portée {p}");
            assert!(autre <= p * 1.1, "{nom} : {autre:.0} px atteints, portée mesurée {p:.0} px");
            // dans une fenêtre de 480 px, le carré de la portée tient entre les marges
            let z = zoom(TAILLE, TAILLE, p);
            assert!(p * z <= TAILLE as f32 / 2.0 - BORD + 0.01, "{nom} : déborde");
        }
    }

    #[test]
    fn zoom_suit_le_petit_cote() {
        assert_eq!(zoom(800, 400, 98.0), zoom(400, 900, 98.0));
        assert!(zoom(1920, 1080, 100.0) > zoom(480, 480, 100.0));
    }

    #[test]
    fn portee_affichee_grandit_vite_et_diminue_doucement() {
        let mut a = suivre(0.0, 100.0, 0.016);
        assert_eq!(a, 100.0);
        // en un quart de seconde, presque toute la hausse
        for _ in 0..16 {
            a = suivre(a, 200.0, 0.016);
        }
        assert!(a > 190.0, "{a}");
        for _ in 0..10 {
            a = suivre(a, 100.0, 0.016);
        }
        assert!(a > 180.0, "{a}");
    }

    /// Fenêtre cachée pour les essais d'affichage.
    fn fenetre_essai() -> HWND {
        unsafe {
            let instance = GetModuleHandleW(None).unwrap();
            let wc = WNDCLASSW { lpfnWndProc: Some(proc), hInstance: instance.into(), lpszClassName: w!("EssaimStreamEssai"), ..Default::default() };
            RegisterClassW(&wc);
            CreateWindowExW(WINDOW_EX_STYLE(0), w!("EssaimStreamEssai"), w!("essai"), WS_OVERLAPPEDWINDOW, 0, 0, 300, 300, None, None, Some(instance.into()), None).unwrap()
        }
    }

    /// Image restée dans le tampon de la chaîne (`presenter` faux), relue par le processeur :
    /// pixels BGRA prémultipliés, ligne par ligne.
    fn relire(r: &Rendu) -> Vec<[u8; 4]> {
        use windows::Win32::Graphics::Direct3D11::*;
        unsafe {
            let tampon: ID3D11Texture2D = r.chaine.GetBuffer(0).unwrap();
            let mut desc = D3D11_TEXTURE2D_DESC::default();
            tampon.GetDesc(&mut desc);
            let (l, h) = (desc.Width, desc.Height);
            desc.Usage = D3D11_USAGE_STAGING;
            desc.BindFlags = 0;
            desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
            desc.MiscFlags = 0;
            let mut copie = None;
            r._d3d.CreateTexture2D(&desc, None, Some(&mut copie)).unwrap();
            let copie = copie.unwrap();
            let ctx = r._d3d.GetImmediateContext().unwrap();
            ctx.CopyResource(&copie, &tampon);
            let mut m = D3D11_MAPPED_SUBRESOURCE::default();
            ctx.Map(&copie, 0, D3D11_MAP_READ, 0, Some(&mut m)).unwrap();
            let mut px = Vec::with_capacity((l * h) as usize);
            for y in 0..h {
                let ligne = std::slice::from_raw_parts((m.pData as *const u8).add((y * m.RowPitch) as usize), (l * 4) as usize);
                px.extend(ligne.chunks_exact(4).map(|p| [p[0], p[1], p[2], p[3]]));
            }
            ctx.Unmap(&copie, 0);
            px
        }
    }

    /// Image réelle (carte graphique ou WARP) relue depuis le tampon de la chaîne : fond
    /// transparent aux bords, essaim au centre, valeurs prémultipliées (aucune composante au-dessus
    /// de l'opacité).
    #[test]
    #[ignore]
    fn image_transparente_relue() {
        let hwnd = fenetre_essai();
        let cfg = Config::default();
        let mut s = Swarm::new();
        for i in 0..120 {
            let t = i as f64 / 60.0;
            s.avance(t, 1.0 / 60.0, (0.0, 0.0), musique(t), &cfg, 1.0);
        }
        let (l, h) = (256u32, 256u32);
        let mut r = Rendu::new(hwnd).unwrap();
        let z = zoom(l as i32, h as i32, portee(&cfg));
        r.image(l, h, couleur_fond(Fond::Transparent), z, false, |cv| s.draw(cv, 2.0, &cfg, 1.0, (0.0, 0.0))).unwrap();
        let px = relire(&r);
        let _ = unsafe { DestroyWindow(hwnd) };
        let at = |x: u32, y: u32| px[(y * l + x) as usize];
        for i in 0..l {
            for p in [at(i, 0), at(i, h - 1), at(0, i), at(l - 1, i)] {
                assert_eq!(p, [0, 0, 0, 0], "bord non transparent");
            }
        }
        let peints: Vec<_> = px.iter().filter(|p| p[3] > 0).collect();
        assert!(!peints.is_empty(), "rien de dessiné");
        assert!(px.iter().all(|p| p[0] <= p[3] && p[1] <= p[3] && p[2] <= p[3]), "valeurs non prémultipliées");
        assert!(peints.iter().any(|p| p[3] < 255), "aucune demi-transparence (lueurs)");
        eprintln!("{} px peints sur {}", peints.len(), px.len());
    }

    /// Image du README (`docs/images/stream.png`, refaite par `scripts/images-readme/rendre.py`) :
    /// la fenêtre à sa taille de départ, réglages par défaut, juste après le drop de la musique
    /// synthétique, en PNG transparent :
    /// `ESSAIM_STREAM=<fichier.png> cargo test --release exporter_image_stream -- --ignored`.
    #[test]
    #[ignore]
    fn exporter_image_stream() {
        let sortie = std::env::var("ESSAIM_STREAM").expect("ESSAIM_STREAM : fichier de sortie");
        let fin: f64 = std::env::var("ESSAIM_STREAM_T").ok().and_then(|v| v.parse().ok()).unwrap_or(7.5);
        let (cfg, son) = (Config::default(), AudioShared::default());
        let (mut s, mut e) = (Swarm::new(), Effets::new());
        let ips = 60.0;
        let mut t = 0.0;
        for i in 0..=(fin * ips) as usize {
            t = i as f64 / ips;
            s.avance(t, 1.0 / ips as f32, (0.0, 0.0), musique(t), &cfg, 1.0);
            s.fissures.clear();
            e.avance(t, 1.0 / ips as f32, (0.0, 0.0), 1.0, s.env, &son, &cfg, &[]);
        }
        let hwnd = fenetre_essai();
        let mut r = Rendu::new(hwnd).unwrap();
        let n = TAILLE as u32;
        r.image(n, n, couleur_fond(Fond::Transparent), zoom(TAILLE, TAILLE, portee(&cfg)), false, |cv| dessiner(cv, &s, &e, t, &cfg)).unwrap();
        let px = relire(&r);
        let _ = unsafe { DestroyWindow(hwnd) };
        // tiny-skia garde du RGBA prémultiplié et l'écrit en PNG à opacité ordinaire
        let mut image = tiny_skia::Pixmap::new(n, n).unwrap();
        for (d, p) in image.data_mut().chunks_exact_mut(4).zip(&px) {
            d.copy_from_slice(&[p[2], p[1], p[0], p[3]]);
        }
        image.save_png(sortie).unwrap();
    }
}
