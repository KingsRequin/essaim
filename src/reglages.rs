//! Fenêtre des réglages : une page web (dossier `ui/`, intégré à l'exécutable) affichée par
//! WebView2 dans une fenêtre Win32, sur son propre fil.
//!
//! La page ne parle à l'appli que par messages JSON (`window.ipc.postMessage`) ; l'appli lui
//! répond et la tient à jour par `essaim.recevoir(…)`. Elle ne touche jamais aux fichiers :
//! les réglages passent par `config.toml` (`config::set_value`, puis `request_reload` pour
//! un effet immédiat), les presets par `profils`. Le son analysé (kicks, basse, note) lui est
//! envoyé 30 fois par seconde pour animer les aperçus.

use crate::audio::AudioShared;
use crate::config::{self, Config};
use crate::profils;
use serde::Deserialize;
use serde_json::{json, Value};
use std::borrow::Cow;
use std::cell::RefCell;
use std::num::NonZeroIsize;
use std::rc::Rc;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::Arc;
use std::time::SystemTime;
use windows::core::{w, BOOL, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR, DWMWA_USE_IMMERSIVE_DARK_MODE};
use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{AdjustWindowRectExForDpi, GetDpiForMonitor, GetDpiForWindow, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::WindowsAndMessaging::*;
use wry::raw_window_handle::{HandleError, HasWindowHandle, RawWindowHandle, Win32WindowHandle, WindowHandle};
use wry::{WebView, WebViewBuilder, WebViewBuilderExtWindows};

const CLASS: PCWSTR = w!("EssaimReglages");
/// Taille à l'ouverture et taille minimale, à 100 % d'échelle.
const TAILLE: (i32, i32) = (1180, 800);
const TAILLE_MIN: (i32, i32) = (900, 620);
const TIMER_SON: usize = 1;
const TIMER_FICHIERS: usize = 2;
/// Adresse de la page : wry sert le protocole `essaim` sous `http://essaim.localhost/`.
const ORIGINE: &str = "http://essaim.localhost/";

/// Fenêtre ouverte (0 = aucune).
static WINDOW: AtomicIsize = AtomicIsize::new(0);

/// Ouvre la fenêtre, ou la ramène devant si elle l'est déjà.
pub fn open(son: Arc<AudioShared>) {
    let h = WINDOW.load(Ordering::Relaxed);
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
    let _ = std::thread::Builder::new().name("essaim-reglages".into()).spawn(move || {
        if let Err(e) = run(son) {
            log::warn!("réglages : fenêtre impossible à ouvrir ({e:#})");
        }
        WINDOW.store(0, Ordering::Relaxed);
    });
}

// ---------------------------------------------------------------------------------------------
// Fichiers de la page

/// Fichiers de `ui/` servis à la page : (chemin, type, contenu).
const FICHIERS: [(&str, &str, &[u8]); 5] = [
    ("index.html", "text/html; charset=utf-8", include_bytes!("../ui/index.html")),
    ("style.css", "text/css; charset=utf-8", include_bytes!("../ui/style.css")),
    ("app.js", "text/javascript; charset=utf-8", include_bytes!("../ui/app.js")),
    ("apercu.js", "text/javascript; charset=utf-8", include_bytes!("../ui/apercu.js")),
    ("polices/Archivo.ttf", "font/ttf", include_bytes!("../ui/polices/Archivo.ttf")),
];

/// La page ne charge que ses propres fichiers ; seul l'index de la bibliothèque vient d'ailleurs.
const CSP: &str = "default-src 'none'; script-src 'self'; style-src 'self' 'unsafe-inline'; font-src 'self'; img-src 'self' data:; \
                   connect-src https://raw.githubusercontent.com";

fn servir(req: wry::http::Request<Vec<u8>>) -> wry::http::Response<Cow<'static, [u8]>> {
    let chemin = req.uri().path().trim_start_matches('/');
    let chemin = if chemin.is_empty() { "index.html" } else { chemin };
    let reponse = wry::http::Response::builder().header("Content-Security-Policy", CSP).header("Cache-Control", "no-store");
    match FICHIERS.iter().find(|(c, ..)| *c == chemin) {
        Some((_, mime, octets)) => reponse.status(200).header("Content-Type", *mime).body(Cow::Borrowed(*octets)),
        None => reponse.status(404).body(Cow::Borrowed(&b""[..])),
    }
    .unwrap_or_else(|_| wry::http::Response::new(Cow::Borrowed(&b""[..])))
}

// ---------------------------------------------------------------------------------------------
// Messages de la page

#[derive(Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
enum Demande {
    /// Réglages, presets et dépôt de la bibliothèque.
    Etat,
    Regler { section: String, cle: String, valeur: Value },
    Appliquer { nom: String },
    /// Enregistre les réglages actuels sous `nom` ; `publier` ouvre la proposition sur GitHub.
    Enregistrer { nom: String, publier: bool },
    Supprimer { nom: String },
    /// Copie un preset de la bibliothèque dans les presets de l'utilisateur.
    Installer { nom: String, reglages: Value, appliquer: bool },
    OuvrirConfig,
    OuvrirDepot,
    /// Jeux de curseurs de Windows, avec leur aperçu.
    Jeux,
    /// Installe un jeu de curseurs et le garde dans les réglages.
    Souris { jeu: String },
    /// Les 17 curseurs d'un jeu, un par un.
    DetailJeu { cle: String },
}

struct Fenetre {
    hwnd: HWND,
    vue: WebView,
    son: Arc<AudioShared>,
    /// Dernier son envoyé (kicks, basse) : rien n'est renvoyé s'il n'a pas changé.
    son_envoye: (u32, u32),
    /// Dates de `config.toml` et du dossier des presets au dernier état envoyé.
    dates: (Option<SystemTime>, Option<SystemTime>),
    /// Aperçus des jeux de curseurs déjà dessinés (clé → `data:` URL).
    apercus: std::collections::HashMap<String, Option<String>>,
}

thread_local! {
    static FENETRE: RefCell<Option<Fenetre>> = const { RefCell::new(None) };
    /// Messages reçus de la page, traités après chaque message Windows (le gestionnaire de
    /// wry n'a pas accès à la vue).
    static RECUS: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
}

fn dates() -> (Option<SystemTime>, Option<SystemTime>) {
    let date = |p: std::path::PathBuf| std::fs::metadata(p).and_then(|m| m.modified()).ok();
    (date(config::path()), date(profils::dir()))
}

impl Fenetre {
    fn envoyer(&self, v: &Value) {
        if let Err(e) = self.vue.evaluate_script(&format!("essaim.recevoir({v})")) {
            log::warn!("réglages : envoi à la page impossible ({e})");
        }
    }

    fn etat(&mut self) {
        self.dates = dates();
        let cfg = Config::read(&config::path()).unwrap_or_default();
        let liste = profils::list();
        let actif = liste.iter().find(|p| profils::same(&p.cfg, &cfg)).map(|p| p.name.clone());
        let presets: Vec<Value> = liste.iter().map(|p| json!({ "nom": p.name, "fourni": p.builtin, "reglages": p.cfg })).collect();
        self.envoyer(&json!({
            "type": "etat",
            "reglages": cfg,
            "presets": presets,
            "actif": actif,
            "depot": cfg.bibliotheque.depot,
            "curseurs": crate::souris::actuel(),
        }));
    }

    fn message(&self, texte: &str) {
        self.envoyer(&json!({ "type": "message", "texte": texte }));
    }

    fn erreur(&self, texte: &str) {
        self.envoyer(&json!({ "type": "erreur", "texte": texte }));
    }

    fn traiter(&mut self, texte: &str) {
        let demande = match serde_json::from_str::<Demande>(texte) {
            Ok(d) => d,
            Err(e) => return log::warn!("réglages : message de la page illisible ({e})"),
        };
        match demande {
            Demande::Etat => self.etat(),
            Demande::Regler { section, cle, valeur } => match config::set_value(&section, &cle, &valeur) {
                Ok(()) => config::request_reload(),
                Err(e) => self.erreur(&format!("Réglage refusé : {e}.")),
            },
            Demande::Appliquer { nom } => {
                let liste = profils::list();
                match liste.iter().find(|p| p.name == nom).map(|p| (p, profils::apply(p))) {
                    Some((p, Ok(()))) => {
                        config::request_reload();
                        self.curseurs_du_preset(&p.cfg);
                    }
                    Some((_, Err(e))) => self.erreur(&format!("Preset « {nom} » non appliqué : {e}.")),
                    None => self.erreur(&format!("Preset « {nom} » introuvable.")),
                }
                self.etat();
            }
            Demande::Enregistrer { nom, publier } => {
                let mut cfg = Config::read(&config::path()).unwrap_or_default();
                // le preset garde le jeu de curseurs installé en ce moment
                cfg.souris.jeu = crate::souris::actuel();
                match profils::save(&nom, &cfg) {
                    Ok(nom) => {
                        self.etat();
                        self.envoyer(&json!({ "type": "enregistre", "nom": nom }));
                        if publier {
                            self.publier(&nom);
                        } else {
                            self.message(&format!("Preset « {nom} » enregistré."));
                        }
                    }
                    Err(e) => self.envoyer(&json!({ "type": "refus", "texte": format!("{}.", majuscule(&e)) })),
                }
            }
            Demande::Supprimer { nom } => match profils::delete(&nom) {
                Ok(()) => {
                    self.etat();
                    self.message(&format!("Preset « {} » supprimé.", nom.trim()));
                }
                Err(e) => self.erreur(&format!("{}.", majuscule(&e))),
            },
            Demande::Installer { nom, reglages, appliquer } => {
                let cfg = serde_json::from_value::<Config>(reglages).map_err(|e| format!("preset illisible ({e})"));
                match cfg.and_then(|cfg| profils::save(&nom, &cfg).map(|n| (n, cfg))) {
                    Ok((nom, cfg)) => {
                        if appliquer {
                            let p = profils::list().into_iter().find(|p| p.name == nom);
                            match p.as_ref().map(profils::apply) {
                                Some(Ok(())) => self.curseurs_du_preset(&cfg),
                                Some(Err(e)) => self.erreur(&format!("Preset « {nom} » installé mais non appliqué : {e}.")),
                                None => {}
                            }
                            config::request_reload();
                        }
                        self.etat();
                        self.message(&format!("Preset « {nom} » installé dans Mes presets."));
                    }
                    Err(e) => self.erreur(&format!("Installation impossible : {e}.")),
                }
            }
            Demande::OuvrirConfig => crate::win::open_file(&config::path()),
            Demande::Jeux => {
                let jeux: Vec<Value> = crate::souris::jeux()
                    .iter()
                    .map(|j| {
                        let apercu = self.apercus.entry(j.cle.clone()).or_insert_with(|| crate::souris::apercu(j)).clone();
                        json!({ "cle": j.cle, "nom": j.nom, "systeme": j.systeme, "apercu": apercu })
                    })
                    .collect();
                self.envoyer(&json!({ "type": "jeux", "jeux": jeux }));
            }
            Demande::DetailJeu { cle } => {
                let jeux = crate::souris::jeux();
                if let Some(j) = jeux.iter().find(|j| j.cle == cle) {
                    let curseurs: Vec<Value> =
                        crate::souris::curseurs(j).into_iter().map(|(nom, image)| json!({ "nom": nom, "image": image })).collect();
                    self.envoyer(&json!({ "type": "detail_jeu", "cle": cle, "nom": j.nom, "curseurs": curseurs }));
                }
            }
            Demande::Souris { jeu } => {
                match crate::souris::installer(&jeu).and_then(|()| config::set_value("souris", "jeu", &json!(jeu))) {
                    Ok(()) => {
                        config::request_reload();
                        self.message(&format!("Curseurs « {jeu} » installés."));
                    }
                    Err(e) => self.erreur(&format!("Curseurs non installés : {e}.")),
                }
                self.etat();
            }
            Demande::OuvrirDepot => {
                let depot = Config::read(&config::path()).unwrap_or_default().bibliotheque.depot;
                crate::win::open_url(&format!("https://github.com/{depot}"));
            }
        }
    }

    /// Installe le jeu de curseurs d'un preset qu'on vient d'appliquer, si l'option est cochée
    /// et qu'il en précise un autre que celui en place.
    fn curseurs_du_preset(&self, preset: &Config) {
        let avec = Config::read(&config::path()).map(|c| c.souris.avec_preset).unwrap_or(true);
        let jeu = &preset.souris.jeu;
        if !avec || jeu.is_empty() || *jeu == crate::souris::actuel() {
            return;
        }
        if let Err(e) = crate::souris::installer(jeu) {
            self.erreur(&format!("Preset appliqué, mais curseurs non installés : {e}."));
        }
    }

    /// Ouvre dans le navigateur la création du fichier du preset dans le dépôt de la
    /// bibliothèque : GitHub propose alors une copie du dépôt et une pull request.
    fn publier(&self, nom: &str) {
        let depot = Config::read(&config::path()).unwrap_or_default().bibliotheque.depot;
        let texte = match std::fs::read_to_string(profils::dir().join(format!("{nom}.toml"))) {
            Ok(t) => t,
            Err(e) => return self.erreur(&format!("Preset « {nom} » enregistré, mais illisible pour la publication ({e}).")),
        };
        let url = format!(
            "https://github.com/{depot}/new/main?filename={}&value={}",
            encoder(&format!("presets/{nom}.toml")),
            encoder(&texte)
        );
        crate::win::open_url(&url);
        self.message(&format!("Preset « {nom} » enregistré. GitHub s'ouvre pour le proposer à la bibliothèque."));
    }

    /// Son et fichiers : envoyés à la page quand ils changent.
    fn tick(&mut self, quoi: usize) {
        if quoi == TIMER_SON {
            let au = self.son.view();
            let cle = (au.kicks, au.bass.to_bits());
            if cle != self.son_envoye {
                self.son_envoye = cle;
                self.envoyer(&json!({ "type": "son", "kicks": au.kicks, "basse": au.bass, "hauteur": au.pitch, "teinte": crate::render::sync_hue() }));
            }
        } else {
            let (ms, ips) = crate::charge::lire();
            self.envoyer(&json!({ "type": "charge", "ms": ms, "ips": ips }));
            if dates() != self.dates {
                self.etat();
            }
        }
    }
}

fn majuscule(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|p| p.to_uppercase().chain(c).collect()).unwrap_or_default()
}

/// Encodage d'URL (RFC 3986) : tout sauf les caractères non réservés.
fn encoder(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------
// Thème : celui des applications de Windows (clair ou sombre)

fn theme_sombre() -> bool {
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
    let mut v = 1u32;
    let mut n = std::mem::size_of::<u32>() as u32;
    let ok = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            w!("AppsUseLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut v as *mut u32 as *mut _),
            Some(&mut n),
        )
    }
    .is_ok();
    ok && v == 0
}

/// Fond de la page (jeton `fond` de la charte), pour la barre de titre et avant le chargement.
fn fond(sombre: bool) -> (u8, u8, u8) {
    if sombre { (0x12, 0x12, 0x11) } else { (0xe8, 0xe7, 0xe1) }
}

/// Barre de titre aux couleurs de la page (Windows 11 ; ignoré ailleurs).
fn barre_de_titre(hwnd: HWND) {
    let sombre = theme_sombre();
    let ((r, g, b), texte) = (fond(sombre), if sombre { 0x00e4ebec } else { 0x00111111 });
    let fond = r as u32 | (g as u32) << 8 | (b as u32) << 16;
    unsafe {
        let s = BOOL::from(sombre);
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, &s as *const _ as *const _, 4);
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_CAPTION_COLOR, &COLORREF(fond) as *const _ as *const _, 4);
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_TEXT_COLOR, &COLORREF(texte) as *const _ as *const _, 4);
    }
}

// ---------------------------------------------------------------------------------------------
// Fenêtre

struct Parent(HWND);

impl HasWindowHandle for Parent {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        let h = NonZeroIsize::new(self.0 .0 as isize).ok_or(HandleError::Unavailable)?;
        // SAFETY : la fenêtre vit tant que la vue (détruite avant elle, dans `run`).
        Ok(unsafe { WindowHandle::borrow_raw(RawWindowHandle::Win32(Win32WindowHandle::new(h))) })
    }
}

fn run(son: Arc<AudioShared>) -> anyhow::Result<()> {
    let hwnd = unsafe { creer_fenetre()? };
    WINDOW.store(hwnd.0 as isize, Ordering::Relaxed);
    barre_de_titre(hwnd);

    let recus = RECUS.with(|r| r.clone());
    let (r, g, b) = fond(theme_sombre());
    let mut contexte = wry::WebContext::new(Some(config::dir().join("webview")));
    let vue = WebViewBuilder::new_with_web_context(&mut contexte)
        .with_custom_protocol("essaim".into(), |_, req| servir(req))
        .with_url(format!("{ORIGINE}index.html"))
        .with_background_color((r, g, b, 255))
        .with_ipc_handler(move |req| recus.borrow_mut().push(req.into_body()))
        .with_navigation_handler(|url| url.starts_with(ORIGINE))
        .with_new_window_req_handler(|_, _| wry::NewWindowResponse::Deny)
        .with_hotkeys_zoom(false)
        .with_default_context_menus(cfg!(debug_assertions))
        .with_browser_accelerator_keys(false)
        .with_devtools(cfg!(debug_assertions))
        .build(&Parent(hwnd))?;
    let _ = vue.focus();
    FENETRE.with(|f| *f.borrow_mut() = Some(Fenetre { hwnd, vue, son, son_envoye: (u32::MAX, 0), dates: (None, None), apercus: Default::default() }));
    unsafe {
        SetTimer(Some(hwnd), TIMER_SON, 33, None);
        SetTimer(Some(hwnd), TIMER_FICHIERS, 700, None);
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
            traiter_recus();
        }
    }
    // la vue avant le contexte et la fenêtre
    FENETRE.with(|f| f.borrow_mut().take());
    drop(contexte);
    Ok(())
}

fn traiter_recus() {
    let lot: Vec<String> = RECUS.with(|r| std::mem::take(&mut *r.borrow_mut()));
    if lot.is_empty() {
        return;
    }
    FENETRE.with(|f| {
        if let Some(f) = f.borrow_mut().as_mut() {
            for m in &lot {
                f.traiter(m);
            }
        }
    });
}

unsafe fn creer_fenetre() -> windows::core::Result<HWND> {
    let hinst = GetModuleHandleW(None)?;
    let wc = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(proc),
        hInstance: hinst.into(),
        lpszClassName: CLASS,
        hCursor: LoadCursorW(None, IDC_ARROW)?,
        hIcon: crate::win::app_icon().unwrap_or_default(),
        ..Default::default()
    };
    RegisterClassExW(&wc);
    // au centre de l'écran de la souris, à son échelle
    let mut pt = POINT::default();
    let _ = GetCursorPos(&mut pt);
    let mon = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
    let (mut dpi, mut dy) = (96u32, 96u32);
    let _ = GetDpiForMonitor(mon, MDT_EFFECTIVE_DPI, &mut dpi, &mut dy);
    let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
    let _ = GetMonitorInfoW(mon, &mut mi);
    let wa = mi.rcWork;
    let style = WS_OVERLAPPEDWINDOW;
    let mut r = RECT { left: 0, top: 0, right: TAILLE.0 * dpi as i32 / 96, bottom: TAILLE.1 * dpi as i32 / 96 };
    let _ = AdjustWindowRectExForDpi(&mut r, style, false, WINDOW_EX_STYLE(0), dpi);
    let (w, h) = ((r.right - r.left).min(wa.right - wa.left), (r.bottom - r.top).min(wa.bottom - wa.top));
    CreateWindowExW(
        WINDOW_EX_STYLE(0),
        CLASS,
        w!("Essaim"),
        style,
        wa.left + (wa.right - wa.left - w) / 2,
        wa.top + (wa.bottom - wa.top - h) / 2,
        w,
        h,
        None,
        None,
        Some(hinst.into()),
        None,
    )
}

extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_TIMER => {
                FENETRE.with(|f| {
                    if let Ok(mut f) = f.try_borrow_mut() {
                        if let Some(f) = f.as_mut() {
                            f.tick(wp.0);
                        }
                    }
                });
                return LRESULT(0);
            }
            WM_GETMINMAXINFO => {
                let dpi = GetDpiForWindow(hwnd).max(96) as i32;
                let mm = &mut *(lp.0 as *mut MINMAXINFO);
                mm.ptMinTrackSize = POINT { x: TAILLE_MIN.0 * dpi / 96, y: TAILLE_MIN.1 * dpi / 96 };
                return LRESULT(0);
            }
            WM_DPICHANGED => {
                let r = &*(lp.0 as *const RECT);
                let _ = SetWindowPos(hwnd, None, r.left, r.top, r.right - r.left, r.bottom - r.top, SWP_NOZORDER | SWP_NOACTIVATE);
                return LRESULT(0);
            }
            WM_SETTINGCHANGE => {
                // « ImmersiveColorSet » : thème clair/sombre changé (la page suit d'elle-même)
                barre_de_titre(hwnd);
            }
            WM_SETFOCUS => {
                FENETRE.with(|f| {
                    if let Ok(f) = f.try_borrow() {
                        if let Some(f) = f.as_ref() {
                            let _ = f.vue.focus();
                        }
                    }
                });
            }
            WM_CLOSE => {
                let _ = DestroyWindow(hwnd);
                return LRESULT(0);
            }
            WM_DESTROY => {
                FENETRE.with(|f| {
                    if let Ok(f) = f.try_borrow() {
                        if let Some(f) = f.as_ref() {
                            let _ = KillTimer(Some(f.hwnd), TIMER_SON);
                            let _ = KillTimer(Some(f.hwnd), TIMER_FICHIERS);
                        }
                    }
                });
                PostQuitMessage(0);
                return LRESULT(0);
            }
            _ => {}
        }
        DefWindowProcW(hwnd, msg, wp, lp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodage_d_url() {
        assert_eq!(encoder("presets/Nuit blanche.toml"), "presets%2FNuit%20blanche.toml");
        assert_eq!(encoder("é=1\n"), "%C3%A9%3D1%0A");
    }

    #[test]
    fn la_page_et_ses_fichiers_sont_servis() {
        let req = |p: &str| wry::http::Request::builder().uri(format!("{ORIGINE}{p}")).body(Vec::new()).unwrap();
        assert_eq!(servir(req("")).status(), 200);
        assert_eq!(servir(req("app.js")).headers()["Content-Type"], "text/javascript; charset=utf-8");
        assert_eq!(servir(req("../src/main.rs")).status(), 404);
        // chaque fichier nommé par la page existe
        let page = std::str::from_utf8(FICHIERS[0].2).unwrap();
        for (chemin, ..) in &FICHIERS[1..4] {
            assert!(page.contains(chemin), "{chemin} non chargé par index.html");
        }
    }

    #[test]
    fn messages_de_la_page() {
        let d: Demande = serde_json::from_str(r#"{"cmd":"regler","section":"orbite","cle":"rayon","valeur":30}"#).unwrap();
        assert!(matches!(d, Demande::Regler { .. }));
        let d: Demande = serde_json::from_str(r#"{"cmd":"enregistrer","nom":"Nuit","publier":true}"#).unwrap();
        assert!(matches!(d, Demande::Enregistrer { publier: true, .. }));
        assert!(serde_json::from_str::<Demande>(r#"{"cmd":"formater_disque"}"#).is_err());
    }
}
