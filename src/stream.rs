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
use crate::gpu::{planche_pastilles, Pinceau};
use crate::render::Swarm;
use std::cell::Cell;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use windows::core::{w, Interface, GUID, PCWSTR};
use windows::Win32::Foundation::{DXGI_STATUS_OCCLUDED, HANDLE, HMODULE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Direct2D::Common::{D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_PIXEL_FORMAT};
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE, D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP};
use windows::Win32::Graphics::Direct3D11::{D3D11CreateDevice, ID3D11Device, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_ALPHA_MODE_IGNORE, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_UNKNOWN, DXGI_SAMPLE_DESC};
use windows::Win32::Graphics::Dxgi::*;
use windows::Win32::Graphics::Gdi::{BeginPaint, EndPaint, PAINTSTRUCT};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Power::{RegisterPowerSettingNotification, UnregisterPowerSettingNotification, POWERBROADCAST_SETTING};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows_numerics::Matrix3x2;

const CLASSE: PCWSTR = w!("EssaimStream");
/// Fenêtre ouverte (0 : aucune ; `OUVERTURE` : réservée, en cours de création).
static FENETRE: AtomicIsize = AtomicIsize::new(0);
const OUVERTURE: isize = -1;
/// Fenêtre hors de vue (écran éteint, session verrouillée) : pause entre deux essais de
/// présentation.
const CACHEE: Duration = Duration::from_millis(100);
/// Fenêtre seulement recouverte (`DXGI_STATUS_OCCLUDED` sans être hors de vue) : elle est encore
/// présentée pour la capture d'OBS, mais `Present` n'attend plus l'écran : la boucle se cale
/// d'elle-même sur ce rythme.
const RECOUVERTE: Duration = Duration::from_micros(16_667);
/// Affichage perdu : premier nouvel essai après ce délai, doublé à chaque échec jusqu'à
/// `PANNE_MAX` ; une ligne de journal par `NOTE_PANNE` au plus.
const PANNE_PREMIERE: Duration = Duration::from_secs(1);
const PANNE_MAX: Duration = Duration::from_secs(30);
const NOTE_PANNE: Duration = Duration::from_secs(60);
/// Écran éteint (`GUID_CONSOLE_DISPLAY_STATE`, notifié à la fenêtre).
static ECRAN_ETEINT: AtomicBool = AtomicBool::new(false);
/// `GUID_CONSOLE_DISPLAY_STATE` : état de l'écran de la console (0 éteint, 1 allumé, 2 atténué).
const GUID_CONSOLE_DISPLAY_STATE: GUID = GUID::from_u128(0x6fe69556_704a_47a0_8f24_c28d936fda47);
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

/// Ouvre la fenêtre (ou la ramène devant si elle l'est déjà). La place est réservée avant de
/// lancer le fil : deux demandes rapprochées n'ouvrent qu'une fenêtre. Un échec d'ouverture est
/// noté au journal et dit par une bulle de l'icône (`win::bulle`), d'où qu'il vienne.
pub fn ouvrir(son: Arc<AudioShared>) {
    match reserver() {
        Place::Reservee => {}
        // déjà en cours d'ouverture : la fenêtre arrive
        Place::EnOuverture => return,
        // ouverte : ramenée devant (si elle se ferme à l'instant, son fil libère la place)
        Place::Ouverte(hwnd) => {
            unsafe {
                let _ = ShowWindow(hwnd, SW_RESTORE);
                let _ = SetForegroundWindow(hwnd);
            }
            return;
        }
    }
    let lance = std::thread::Builder::new().name("essaim-stream".into()).spawn(move || {
        // libère la place à la sortie du fil, même s'il panique
        let place = Tenue(Cell::new(OUVERTURE));
        let resultat = run(son, &place);
        drop(place);
        match resultat {
            Ok(()) => log::info!("stream : fenêtre fermée"),
            Err(e) => echec(&e.to_string()),
        }
    });
    if let Err(e) = lance {
        FENETRE.store(0, Ordering::Release);
        echec(&e.to_string());
    }
}

#[derive(Debug, PartialEq)]
enum Place {
    /// Libre : elle est maintenant réservée à cette demande.
    Reservee,
    EnOuverture,
    Ouverte(HWND),
}

/// Réserve la place de la fenêtre, d'un seul geste (`compare_exchange`). Une place marquée
/// ouverte sur une fenêtre qui n'existe plus est reprise.
fn reserver() -> Place {
    loop {
        match FENETRE.compare_exchange(0, OUVERTURE, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => return Place::Reservee,
            Err(OUVERTURE) => return Place::EnOuverture,
            Err(h) => {
                if unsafe { IsWindow(Some(HWND(h as *mut _))) }.as_bool() {
                    return Place::Ouverte(HWND(h as *mut _));
                }
                // fenêtre disparue sans que son fil ait rendu la place : reprise (si elle vient
                // de changer, nouvel essai)
                if FENETRE.compare_exchange(h, OUVERTURE, Ordering::AcqRel, Ordering::Acquire).is_ok() {
                    return Place::Reservee;
                }
            }
        }
    }
}

/// Place tenue par le fil de la fenêtre (`OUVERTURE`, puis sa fenêtre) : rendue quand le fil
/// s'arrête, panique comprise, seulement si elle est encore la sienne (une place reprise par
/// `reserver` n'est pas rendue par l'ancien fil).
struct Tenue(Cell<isize>);

impl Drop for Tenue {
    fn drop(&mut self) {
        let _ = FENETRE.compare_exchange(self.0.get(), 0, Ordering::AcqRel, Ordering::Acquire);
    }
}

/// Ouverture ratée : au journal, et dans une bulle de l'icône.
fn echec(raison: &str) {
    log::warn!("stream : fenêtre impossible à ouvrir ({raison})");
    crate::win::bulle(&format!("Fenêtre pour le stream impossible à ouvrir : {}", raison.trim()));
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
            WM_POWERBROADCAST if wp.0 == PBT_POWERSETTINGCHANGE as usize && lp.0 != 0 => {
                let r = &*(lp.0 as *const POWERBROADCAST_SETTING);
                if r.PowerSetting == GUID_CONSOLE_DISPLAY_STATE && r.DataLength >= 4 {
                    let etat = std::ptr::read_unaligned(r.Data.as_ptr() as *const u32);
                    ECRAN_ETEINT.store(etat == 0, Ordering::Relaxed);
                }
                LRESULT(1)
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
    portee_de(Swarm::new(), cfg)
}

/// La même mesure avec l'essaim `swarm` (les essais lui donnent des tirages fixés).
fn portee_de(mut swarm: Swarm, cfg: &Config) -> f32 {
    let effets = Effets::new();
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
    planche: ID2D1Bitmap1,
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
            let planche = planche_pastilles(&dc)?;
            Ok(Self { _d3d: d3d, chaine, dc, planche, cible: None, pinceau: Pinceau::default() })
        }
    }

    /// Libère tout avant d'en refaire un : la fenêtre n'accepte qu'une chaîne d'échange, et
    /// Direct3D ne détruit ses objets qu'une fois le contexte vidé (`ClearState` + `Flush`).
    fn liberer(self) {
        let Self { _d3d: d3d, chaine, dc, planche, cible, pinceau } = self;
        unsafe { dc.SetTarget(None) };
        // vue Direct2D du tampon et ressources de dessin d'abord, puis la chaîne
        drop((cible, pinceau, planche, dc));
        drop(chaine);
        if let Ok(ctx) = unsafe { d3d.GetImmediateContext() } {
            unsafe {
                ctx.ClearState();
                ctx.Flush();
            }
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
    /// (0, 0), agrandi `z` fois et centré, puis présentée avec l'intervalle de synchronisation
    /// `presenter` (`intervalle_synchro`). Sans `presenter`, l'image reste dans le tampon (essais).
    /// `Ok(true)` : la fenêtre n'est pas visible (`DXGI_STATUS_OCCLUDED` : recouverte, écran
    /// éteint…).
    fn image(&mut self, l: u32, h: u32, fond: D2D1_COLOR_F, z: f32, presenter: Option<u32>, draw: impl FnOnce(&mut dyn Canvas)) -> windows::core::Result<bool> {
        self.cible(l, h)?;
        unsafe {
            self.dc.BeginDraw();
            self.dc.SetTransform(&Matrix3x2::identity());
            self.dc.Clear(Some(&fond));
            self.dc.SetTransform(&Matrix3x2 { M11: z, M12: 0.0, M21: 0.0, M22: z, M31: l as f32 / 2.0, M32: h as f32 / 2.0 });
            let peint = self.pinceau.peindre(&self.dc, &self.planche, draw);
            self.dc.EndDraw(None, None)?;
            peint?;
            if let Some(intervalle) = presenter {
                // une image tous les `intervalle` rafraîchissements de l'écran ; cachée, Present
                // ne freine plus la boucle (pas de synchronisation) : c'est à elle d'attendre
                let r = self.chaine.Present(intervalle, DXGI_PRESENT(0));
                r.ok()?;
                return Ok(r == DXGI_STATUS_OCCLUDED);
            }
        }
        Ok(false)
    }

    /// La fenêtre est-elle de nouveau visible ? (Rien n'est présenté.)
    fn visible(&self) -> bool {
        unsafe { self.chaine.Present(0, DXGI_PRESENT_TEST) != DXGI_STATUS_OCCLUDED }
    }
}

fn run(son: Arc<AudioShared>, place: &Tenue) -> windows::core::Result<()> {
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
    FENETRE.store(hwnd.0 as isize, Ordering::Release);
    place.0.set(hwnd.0 as isize);
    // écran éteint ou rallumé : notifié à la fenêtre (l'état présent arrive tout de suite)
    ECRAN_ETEINT.store(false, Ordering::Relaxed);
    let ecran = unsafe { RegisterPowerSettingNotification(HANDLE(hwnd.0), &GUID_CONSOLE_DISPLAY_STATE, DEVICE_NOTIFY_WINDOW_HANDLE) }.ok();
    let resultat = boucle(hwnd, &son);
    unsafe {
        if let Some(e) = ecran {
            let _ = UnregisterPowerSettingNotification(e);
        }
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

/// Personne ne peut voir la fenêtre : réduite, écran éteint, ou session verrouillée,
/// déconnectée ou à distance. Seulement recouverte, elle reste présentée (capture d'OBS).
fn hors_de_vue(hwnd: HWND) -> bool {
    unsafe { IsIconic(hwnd) }.as_bool() || ECRAN_ETEINT.load(Ordering::Relaxed) || crate::win::session_retenue().is_some()
}

/// Attente avant le nouvel essai qui suit `echecs` échecs d'affilée de l'affichage.
fn attente_panne(echecs: u32) -> Duration {
    PANNE_PREMIERE.saturating_mul(2u32.saturating_pow(echecs.saturating_sub(1))).min(PANNE_MAX)
}

/// Affichage perdu (pilote réinitialisé, sortie de veille…) : essais espacés, journal sobre.
#[derive(Default)]
struct Panne {
    /// Échecs d'affilée.
    echecs: u32,
    /// Prochain essai.
    prochain: Option<Instant>,
    /// Dernière ligne du journal, et échecs tus depuis.
    note: Option<Instant>,
    tus: u32,
}

impl Panne {
    fn echec(&mut self, now: Instant, quoi: &str, e: &windows::core::Error) {
        self.echecs = self.echecs.saturating_add(1);
        let attente = attente_panne(self.echecs);
        self.prochain = Some(now + attente);
        if self.note.is_none_or(|t| now - t >= NOTE_PANNE) {
            log::warn!("stream : {quoi} ({e}) ; {} autre(s) échec(s) depuis la dernière note, nouvel essai dans {} s", self.tus, attente.as_secs());
            (self.note, self.tus) = (Some(now), 0);
        } else {
            self.tus += 1;
        }
    }

    /// Une image a été présentée : les échecs passés sont oubliés.
    fn retablie(&mut self) {
        if self.echecs > 0 {
            log::info!("stream : affichage rétabli après {} échec(s)", self.echecs);
            *self = Self::default();
        }
    }

    fn attendre(&self, now: Instant) -> bool {
        self.prochain.is_some_and(|p| now < p)
    }
}

/// Cadence de la fenêtre quand `[rendu] cadence` n'est pas fixée (celle d'une capture de stream).
const CADENCE_STREAM: u32 = 60;

/// Cadence visée par la fenêtre : `[rendu] cadence`, ou `CADENCE_STREAM` si elle n'est pas fixée (0).
fn cadence_visee(cadence: u32) -> u32 {
    if cadence == 0 { CADENCE_STREAM } else { cadence }
}

/// Intervalle de synchronisation de `Present` sur un écran à `frequence` Hz (0 = inconnue) : une
/// image tous les `n` rafraîchissements, `n` le plus grand qui garde `fréquence / n` au moins à la
/// cadence visée (`cadence_visee`), soit `fréquence / n` le plus proche d'elle sans passer dessous
/// (144 Hz : 72 i/s pour 60, 90 Hz : 90 i/s, 240 Hz : 60 i/s ; 60 Hz pour 30 : 30 i/s). DXGI accepte
/// 1 à 4 : au-delà, `periode_min` tient la cadence.
fn intervalle_synchro(frequence: u32, cadence: u32) -> u32 {
    (frequence / cadence_visee(cadence)).clamp(1, 4)
}

/// Écart minimal entre deux images pour `[rendu] cadence` : aucun si elle n'est pas fixée, ni
/// quand l'intervalle de synchronisation la tient déjà à 10 % près (attendre en plus ferait sauter
/// un rafraîchissement sur deux : images inégales) ; sinon une période de la cadence.
fn periode_min(frequence: u32, cadence: u32) -> Duration {
    if cadence == 0 {
        return Duration::ZERO;
    }
    let obtenue = f64::from(frequence) / f64::from(intervalle_synchro(frequence, cadence));
    if frequence > 0 && obtenue <= 1.1 * f64::from(cadence) {
        Duration::ZERO
    } else {
        Duration::from_secs_f64(1.0 / f64::from(cadence))
    }
}

fn boucle(hwnd: HWND, son: &AudioShared) -> windows::core::Result<()> {
    let mut rendu = Some(Rendu::new(hwnd)?);
    log::info!("stream : fenêtre ouverte");
    let mut conf = config::Watched::load();
    let (mut swarm, mut effets) = (Swarm::new(), Effets::new());
    let mut calibree = portee(&conf.cfg);
    let (mut vue, mut affichee) = (0.0f32, 0.0f32);
    let debut = Instant::now();
    let (mut avant, mut relu) = (debut, debut);
    let mut pistes = crate::media::pistes();
    let mut doux = crate::win::sans_flashs(conf.cfg.rendu.sans_flashs);
    let mut panne = Panne::default();
    let mut cachee = false;
    let mut presets_vus = crate::profils::applications();
    // une image tous les `intervalle` rafraîchissements de l'écran de la fenêtre (relu chaque
    // seconde : la fenêtre peut changer d'écran), et attente en plus pour une cadence fixée
    let rythme = |cadence: u32| {
        let f = crate::win::frequence_fenetre(hwnd);
        (intervalle_synchro(f, cadence), periode_min(f, cadence))
    };
    let (mut intervalle, mut periode) = rythme(conf.cfg.rendu.cadence);
    loop {
        unsafe {
            let mut msg = MSG::default();
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                if msg.message == WM_QUIT {
                    if let Some(r) = rendu.take() {
                        r.liberer();
                    }
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
                // preset appliqué : comme la surcouche, les traînées repartent de zéro
                let presets = crate::profils::applications();
                if presets != presets_vus {
                    presets_vus = presets;
                    swarm.vider_trainees();
                }
            }
            doux = crate::win::sans_flashs(conf.cfg.rendu.sans_flashs);
            (intervalle, periode) = rythme(conf.cfg.rendu.cadence);
        }
        let cfg = &conf.cfg;
        let mut zone = RECT::default();
        let _ = unsafe { GetClientRect(hwnd, &mut zone) };
        let (l, h) = (zone.right - zone.left, zone.bottom - zone.top);
        let reduite = l <= 0 || h <= 0 || unsafe { IsIconic(hwnd) }.as_bool();
        // hors de vue (écran éteint, session verrouillée) : rien n'est simulé, un essai de
        // présentation toutes les `CACHEE`
        if !reduite && cachee {
            std::thread::sleep(CACHEE);
            cachee = !rendu.as_ref().is_none_or(Rendu::visible) && hors_de_vue(hwnd);
        }
        if reduite || cachee {
            // les coups joués entre-temps sont lus et jetés : pas de rafale au retour
            swarm.oublier_coups(&son.view());
            effets.oublier_coups(son);
            avant = Instant::now();
            if reduite {
                std::thread::sleep(Duration::from_millis(50));
            }
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
        (swarm.doux, effets.doux) = (doux, doux);
        swarm.avance(t, dt, (0.0, 0.0), au, cfg, 1.0);
        // pas de fissures dans la fenêtre pour le stream
        swarm.fissures.clear();
        effets.moduler_figures(swarm.effets_figures());
        effets.avance(t, dt, (0.0, 0.0), 1.0, swarm.env, son, cfg, &[]);
        // taille : la portée mesurée, ou celle vue en direct si la musique pousse plus loin
        vue = (vue * (-dt / OUBLI).exp()).max(demi_cote(&swarm, &effets, cfg));
        affichee = suivre(affichee, calibree.max(vue), dt);
        let z = zoom(l, h, affichee);
        // affichage perdu : refait quand vient l'heure du prochain essai
        if rendu.is_none() && !panne.attendre(now) {
            match Rendu::new(hwnd) {
                Ok(r) => rendu = Some(r),
                Err(e) => panne.echec(now, "affichage impossible à refaire", &e),
            }
        }
        let Some(r) = rendu.as_mut() else {
            std::thread::sleep(Duration::from_millis(50));
            continue;
        };
        match r.image(l as u32, h as u32, couleur_fond(cfg.stream.fond), z, Some(intervalle), |cv| dessiner(cv, &swarm, &effets, t, cfg)) {
            Err(e) => {
                // pilote réinitialisé, sortie de veille… : l'ancien affichage est libéré (la
                // fenêtre n'accepte qu'une chaîne d'échange), puis refait à l'essai suivant
                if let Some(r) = rendu.take() {
                    r.liberer();
                }
                panne.echec(now, "erreur d'affichage", &e);
            }
            Ok(occultee) => {
                panne.retablie();
                if occultee {
                    if hors_de_vue(hwnd) {
                        cachee = true;
                    } else {
                        // seulement recouverte : toujours présentée (OBS la capture), au rythme
                        // que `Present` ne donne plus
                        std::thread::sleep(RECOUVERTE.max(periode).saturating_sub(now.elapsed()));
                    }
                } else {
                    // cadence fixée plus basse que ce que l'intervalle (4 au plus) retient
                    std::thread::sleep(periode.saturating_sub(now.elapsed()));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Plus grand demi-côté atteint par l'essaim de `cfg` sur une autre musique que celle de la
    /// mesure (autre tempo, kicks irréguliers, drop plus tôt), avec un changement de piste.
    fn portee_autre_musique(cfg: &Config, graine: u64) -> f32 {
        let (mut s, effets) = (Swarm::avec_graine(graine), Effets::new());
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

    /// Mesure du hasard : sur 40 tirages par preset, le pire rapport entre la portée atteinte sur
    /// une autre musique et la portée mesurée, et le nombre de dépassements de la marge de 10 %
    /// (2 sur 2 120 en novembre 2026, pour « Ressorts » : 1,15 au pire).
    /// `cargo test --release mesure_des_ecarts_de_portee -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn mesure_des_ecarts_de_portee() {
        let mut cfgs = vec![("défaut".to_string(), Config::default())];
        cfgs.extend(crate::prereglages::builtins().into_iter().map(|p| (p.name, p.cfg)));
        let (mut pire, mut depassements, mut total) = ((0.0f32, String::new()), 0, 0);
        for (nom, cfg) in &cfgs {
            for g in 0..40u64 {
                let r = portee_autre_musique(cfg, 500_000 + g) / portee_de(Swarm::avec_graine(1000 * g + 3), cfg);
                total += 1;
                depassements += (r > 1.1) as usize;
                if r > pire.0 {
                    pire = (r, nom.clone());
                }
            }
        }
        eprintln!("pire rapport {:.3} ({}), {depassements} dépassements de 10 % sur {total}", pire.0, pire.1);
    }

    #[test]
    fn chaque_preset_tient_dans_la_fenetre() {
        // la portée mesurée couvre ce que l'essaim dessine sur une autre musique, pour chaque
        // preset fourni ; une petite marge pour le hasard des particules (la portée vue en direct
        // prend le relais au-delà). Tirages fixés : au hasard libre, la marge est dépassée environ
        // une fois sur mille par preset (`mesure_des_ecarts_de_portee`), et l'essai échouait une
        // fois sur dix.
        let mut cfgs = vec![("défaut".to_string(), Config::default())];
        cfgs.extend(crate::prereglages::builtins().into_iter().map(|p| (p.name, p.cfg)));
        for (nom, cfg) in cfgs {
            let t0 = std::time::Instant::now();
            let p = portee_de(Swarm::avec_graine(7), &cfg);
            eprintln!("{nom:24} portée {p:5.0} px, mesurée en {:.0} ms", t0.elapsed().as_secs_f64() * 1000.0);
            let autre = portee_autre_musique(&cfg, 11);
            assert!(p.is_finite() && p > POINTEUR, "{nom} : portée {p}");
            assert!(autre <= p * 1.1, "{nom} : {autre:.0} px atteints, portée mesurée {p:.0} px");
            // dans une fenêtre de 480 px, le carré de la portée tient entre les marges
            let z = zoom(TAILLE, TAILLE, p);
            assert!(p * z <= TAILLE as f32 / 2.0 - BORD + 0.01, "{nom} : déborde");
        }
    }

    #[test]
    fn une_seule_ouverture_a_la_fois() {
        // seul essai à toucher `FENETRE` : deux demandes rapprochées, une seule place réservée
        assert_eq!(reserver(), Place::Reservee);
        assert!(ouverte());
        assert_eq!(reserver(), Place::EnOuverture);
        // fil arrêté (panique comprise) avant d'avoir sa fenêtre : la place est rendue
        drop(Tenue(Cell::new(OUVERTURE)));
        assert!(!ouverte());
        // fenêtre ouverte : ramenée devant, pas rouverte
        let hwnd = fenetre_essai();
        FENETRE.store(hwnd.0 as isize, Ordering::Release);
        assert_eq!(reserver(), Place::Ouverte(hwnd));
        // fenêtre disparue sans que son fil ait rendu la place : reprise
        let _ = unsafe { DestroyWindow(hwnd) };
        assert_eq!(reserver(), Place::Reservee);
        assert_eq!(FENETRE.load(Ordering::Acquire), OUVERTURE);
        // l'ancien fil s'arrête ensuite : il ne rend pas la place reprise
        drop(Tenue(Cell::new(hwnd.0 as isize)));
        assert!(ouverte());
        FENETRE.store(0, Ordering::Release);
        assert!(!ouverte());
    }

    #[test]
    fn essais_d_affichage_espaces() {
        let attentes: Vec<u64> = (1..=7).map(|n| attente_panne(n).as_secs()).collect();
        assert_eq!(attentes, [1, 2, 4, 8, 16, 30, 30]);
        assert_eq!(attente_panne(u32::MAX), PANNE_MAX);
        let t0 = Instant::now();
        let mut p = Panne::default();
        assert!(!p.attendre(t0));
        let e = windows::core::Error::from_hresult(windows::Win32::Foundation::E_FAIL);
        p.echec(t0, "essai", &e);
        p.echec(t0, "essai", &e);
        assert!(p.attendre(t0 + Duration::from_millis(1500)));
        assert!(!p.attendre(t0 + Duration::from_secs(2)));
        assert_eq!(p.tus, 1, "une seule ligne de journal par minute");
        p.retablie();
        assert!(!p.attendre(t0) && p.echecs == 0);
    }

    /// Trait fin (0,8 px) horizontal sur la carte graphique, relu : continu et d'intensité
    /// régulière, qu'il tombe sur une ligne de pixels (y = 10,0) ou entre deux (10,5) ; petites
    /// pastilles de même lumière où qu'elles tombent (pas de clignotement).
    /// `cargo test --release traits_fins_relus -- --ignored`
    #[test]
    #[ignore]
    fn traits_fins_relus() {
        let hwnd = fenetre_essai();
        let (l, h) = (64u32, 64u32);
        let mut r = Rendu::new(hwnd).unwrap();
        // `image` centre (0, 0) : (x, y) de la fenêtre = (x − 32, y − 32) du dessin
        let c = (l as f32 / 2.0, h as f32 / 2.0);
        let mut relu = |draw: &dyn Fn(&mut dyn Canvas)| {
            r.image(l, h, couleur_fond(Fond::Transparent), 1.0, None, |cv| draw(cv)).unwrap();
            relire(&r)
        };
        for y in [10.0f32, 10.5] {
            let px = relu(&|cv| cv.line((8.0 - c.0, y - c.1), (56.0 - c.0, y - c.1), 0.8, (255, 255, 255), 1.0, true));
            // lumière de chaque colonne du trait (hors des bouts)
            let colonnes: Vec<u32> = (14..50).map(|x| (5..16).map(|yy| px[(yy * l + x) as usize][3] as u32).sum()).collect();
            let (min, max) = (*colonnes.iter().min().unwrap(), *colonnes.iter().max().unwrap());
            eprintln!("y = {y} : lumière par colonne {min} à {max}");
            assert!(min > 0, "y = {y} : trait coupé");
            assert!(min as f32 >= 0.85 * max as f32, "y = {y} : intensité irrégulière ({min} à {max})");
            // à peu près la lumière d'un trait de 0,8 px (0,8 × 255)
            assert!((150..=260).contains(&max), "y = {y} : lumière {max}");
        }
        let mut lumieres = Vec::new();
        for k in 0..4 {
            let d = k as f32 * 0.25;
            let px = relu(&|cv| cv.circle(32.0 + d - c.0, 32.0 + d - c.1, 0.4, (255, 255, 255), 1.0));
            lumieres.push(px.iter().map(|p| p[3] as u32).sum::<u32>());
        }
        let _ = unsafe { DestroyWindow(hwnd) };
        eprintln!("petite pastille selon sa position : {lumieres:?}");
        let (min, max) = (*lumieres.iter().min().unwrap(), *lumieres.iter().max().unwrap());
        assert!(min > 0 && min as f32 >= 0.8 * max as f32, "pastille qui clignote : {lumieres:?}");
    }

    /// Anneau épais (halo du kick : de 6 à 28 px, opacité 0,35) sur la carte graphique, relu : rien
    /// sous le creux, et même lumière dans toutes les directions (pas de rosace).
    /// `cargo test --release anneau_epais_relu -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn anneau_epais_relu() {
        let hwnd = fenetre_essai();
        let (l, h) = (64u32, 64u32);
        let mut r = Rendu::new(hwnd).unwrap();
        let (vide, dehors) = (6.0f32, 28.0f32);
        r.image(l, h, couleur_fond(Fond::Transparent), 1.0, None, |cv| cv.ring(0.0, 0.0, (vide + dehors) / 2.0, dehors - vide, (255, 255, 255), 0.35))
            .unwrap();
        let px = relire(&r);
        let _ = unsafe { DestroyWindow(hwnd) };
        // opacité moyenne par secteur de 15°, dans l'anneau (loin de ses bords)
        let mut secteurs = [(0u32, 0u32); 24];
        let mut sous_le_creux = 0u8;
        for y in 0..h {
            for x in 0..l {
                let (dx, dy) = (x as f32 + 0.5 - 32.0, y as f32 + 0.5 - 32.0);
                let d = dx.hypot(dy);
                let a = px[(y * l + x) as usize][3];
                if d < vide - 1.5 {
                    sous_le_creux = sous_le_creux.max(a);
                } else if d > vide + 2.0 && d < dehors - 2.0 {
                    let i = ((dy.atan2(dx) + std::f32::consts::PI) / std::f32::consts::TAU * 24.0) as usize % 24;
                    secteurs[i].0 += u32::from(a);
                    secteurs[i].1 += 1;
                }
            }
        }
        let moyennes: Vec<f32> = secteurs.iter().map(|&(s, n)| s as f32 / n.max(1) as f32).collect();
        let (min, max) = moyennes.iter().fold((f32::MAX, 0.0f32), |(m, n), &v| (m.min(v), n.max(v)));
        eprintln!("opacité par secteur {min:.1} à {max:.1} (sur 255), sous le creux {sous_le_creux}");
        assert!(sous_le_creux <= 3, "voilé sous le creux : {sous_le_creux}");
        assert!(min >= 0.9 * max, "rosace : {min:.1} à {max:.1}");
        // à peu près l'opacité voulue (0,35 × 255 = 89)
        assert!((60.0..=120.0).contains(&max), "opacité {max}");
    }

    #[test]
    fn synchro_a_la_frequence_de_l_ecran() {
        assert_eq!([60, 75, 90, 120, 144, 165, 240, 360].map(|f| intervalle_synchro(f, 0)), [1, 1, 1, 2, 2, 2, 4, 4]);
        // fréquence inconnue : chaque rafraîchissement
        assert_eq!(intervalle_synchro(0, 0), 1);
        // cadence fixée haute : jamais en dessous (240 Hz pour 100 : 120 i/s, pas 80)
        assert_eq!(intervalle_synchro(144, 144), 1);
        assert_eq!(intervalle_synchro(144, 100), 1);
        assert_eq!(intervalle_synchro(60, 144), 1);
        assert_eq!(intervalle_synchro(240, 100), 2);
        // cadence fixée basse : l'intervalle s'en approche par au-dessus (144 Hz pour 30 : 36 i/s,
        // 4 au plus), `periode_min` tient le reste
        assert_eq!(intervalle_synchro(144, 30), 4);
        assert_eq!(intervalle_synchro(60, 30), 2);
        // aucune combinaison ne descend sous la cadence visée quand l'écran va au moins aussi vite
        for f in [60u32, 75, 90, 100, 120, 144, 165, 175, 240, 360] {
            for c in [0u32, 30, 60, 75, 100, 120, 144, 240] {
                let voulu = cadence_visee(c);
                let n = intervalle_synchro(f, c);
                assert!(f < voulu || f / n >= voulu, "{f} Hz, cadence {c} : {} i/s", f / n);
            }
        }
        // attente en plus : jamais sans cadence fixée, ni quand l'intervalle la tient à 10 % près
        // (60 Hz pour 30, 165 Hz pour 80 : 82,5 i/s) ; sinon une période de la cadence
        assert_eq!(periode_min(144, 0), Duration::ZERO);
        assert_eq!(periode_min(60, 30), Duration::ZERO);
        assert_eq!(periode_min(165, 80), Duration::ZERO);
        assert_eq!(periode_min(144, 50), Duration::from_millis(20));
        assert_eq!(periode_min(0, 50), Duration::from_millis(20));
        assert_eq!(periode_min(144, 100), Duration::from_millis(10));
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
        r.image(l, h, couleur_fond(Fond::Transparent), z, None, |cv| s.draw(cv, 2.0, &cfg, 1.0, (0.0, 0.0))).unwrap();
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
        r.image(n, n, couleur_fond(Fond::Transparent), zoom(TAILLE, TAILLE, portee(&cfg)), None, |cv| dessiner(cv, &s, &e, t, &cfg)).unwrap();
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
