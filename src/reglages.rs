//! Fenêtre des réglages : profils et réglages principaux, appliqués en direct.
//!
//! Elle tourne dans son propre fil (le déplacement d'une fenêtre est une boucle modale qui
//! figerait l'essaim) et ne parle à la boucle principale que par `config.toml` : chaque réglage
//! touché y est écrit (commentaires gardés), puis `config::request_reload` fait relire le
//! fichier tout de suite. Une modification du fichier faite ailleurs (à la main, Horizon) est
//! reprise ici dans la seconde.

use crate::config::{self, Watched};
use crate::profils::{self, Profile};
use serde_json::Value;
use std::cell::RefCell;
use std::sync::atomic::{AtomicIsize, Ordering};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateFontIndirectW, DeleteObject, GetMonitorInfoW, GetSysColor, GetSysColorBrush, MonitorFromPoint, SetBkColor,
    COLOR_WINDOW, HDC, HFONT, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::{
    InitCommonControlsEx, BST_CHECKED, BST_UNCHECKED, ICC_BAR_CLASSES, ICC_STANDARD_CLASSES, INITCOMMONCONTROLSEX,
    TBM_SETPAGESIZE, TBM_SETPOS, TBM_SETRANGEMAX, TBM_SETRANGEMIN, TBS_HORZ, TBS_NOTICKS, TB_THUMBTRACK,
    TRACKBAR_CLASSW,
};
use windows::Win32::UI::HiDpi::{AdjustWindowRectExForDpi, GetDpiForWindow, SystemParametersInfoForDpi};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetCapture, GetFocus};
use windows::Win32::UI::WindowsAndMessaging::*;

const CLASS: PCWSTR = w!("EssaimReglages");
const ID_COMBO: usize = 10;
const ID_SAVE: usize = 11;
const ID_DELETE: usize = 12;
const ID_FILE: usize = 13;
const ID_ITEM: usize = 1000;
const TIMER_RELOAD: usize = 1;
const TIMER_FLUSH: usize = 2;
// absents de `windows` sans la fonction `Win32_System_SystemServices`
const TBM_GETPOS: u32 = WM_USER;
const SS_LEFT: u32 = 0;
const SS_RIGHT: u32 = 2;
const SS_ENDELLIPSIS: u32 = 0x4000;

/// Fenêtre ouverte (0 = aucune).
static WINDOW: AtomicIsize = AtomicIsize::new(0);

enum Kind {
    Bool,
    /// Bornes de la glissière, pas, et texte affiché pour 0 s'il a un sens particulier.
    Num { min: f64, max: f64, step: f64, zero: Option<&'static str> },
}

struct Item {
    section: &'static str,
    key: &'static str,
    label: &'static str,
    kind: Kind,
    /// Écrit pendant le glissement (sinon au lâcher : ces réglages refont les curseurs Windows).
    live: bool,
}

const fn b(section: &'static str, key: &'static str, label: &'static str) -> Item {
    Item { section, key, label, kind: Kind::Bool, live: true }
}
const fn n(section: &'static str, key: &'static str, label: &'static str, min: f64, max: f64, step: f64) -> Item {
    Item { section, key, label, kind: Kind::Num { min, max, step, zero: None }, live: true }
}

/// Réglages montrés, par colonne puis par groupe.
const COLUMNS: [&[(&str, &[Item])]; 3] = [
    &[
        ("Pointeur", &[Item { live: false, ..n("curseur", "taille_point", "Taille du point", 1.0, 12.0, 0.1) }]),
        (
            "Orbite et comètes",
            &[
                b("orbite", "cometes", "Deux comètes en orbite"),
                n("orbite", "rayon", "Rayon", 4.0, 80.0, 1.0),
                n("orbite", "vitesse_base", "Vitesse sans basse", 0.0, 15.0, 0.1),
                n("orbite", "vitesse_basse", "Vitesse ajoutée (basse)", 0.0, 40.0, 0.5),
                n("orbite", "eloignement_grave", "Écart des notes graves", 0.0, 4.0, 0.1),
                n("orbite", "trainee", "Traînée (s)", 0.05, 2.0, 0.05),
            ],
        ),
        (
            "Fissures au clic",
            &[
                b("fissures", "actives", "Fissure sous chaque clic"),
                n("fissures", "taille", "Taille", 0.2, 6.0, 0.1),
                n("fissures", "duree", "Durée (s)", 0.2, 15.0, 0.1),
            ],
        ),
    ],
    &[
        (
            "Essaim de satellites",
            &[
                n("essaim", "par_kick_min", "Ajoutés par kick (min)", 0.0, 10.0, 1.0),
                n("essaim", "par_kick_max", "Ajoutés par kick (max)", 0.0, 10.0, 1.0),
                n("essaim", "maximum", "Satellites au plus", 0.0, 64.0, 1.0),
                n("essaim", "duree", "Durée de vie (s)", 0.5, 10.0, 0.1),
            ],
        ),
        (
            "Couleur",
            &[
                b("couleur", "synchro_prisme", "Teinte de l'éclairage RGB (Prisme)"),
                Item { live: false, ..n("couleur", "teinte", "Teinte de départ (°)", 0.0, 360.0, 1.0) },
                Item {
                    live: false,
                    kind: Kind::Num { min: 0.0, max: 360.0, step: 1.0, zero: Some("fixe") },
                    ..n("couleur", "vitesse_teinte", "Défilement (°/s)", 0.0, 0.0, 0.0)
                },
                Item { live: false, ..n("couleur", "ecart_teinte", "Écart tête-queue (°)", 0.0, 360.0, 1.0) },
            ],
        ),
        (
            "Son",
            &[
                b("audio", "actif", "Réagir à la musique"),
                n("audio", "sensibilite_kick", "Sensibilité au kick", 1.0, 10.0, 0.5),
                n("audio", "delai_kick", "Écart min. des kicks (s)", 0.05, 0.5, 0.01),
            ],
        ),
    ],
    &[
        (
            "Bordure de la fenêtre active",
            &[
                b("bordure", "active", "Bordure à la couleur de l'essaim"),
                n("bordure", "intensite", "Éclat sur le kick", 0.0, 1.0, 0.05),
                n("bordure", "repos", "Luminosité au repos", 0.0, 1.0, 0.05),
            ],
        ),
        (
            "Windows (hors profils)",
            &[
                b("curseur", "remplacer_curseurs", "Remplacer la flèche par le point"),
                b("curseur", "variantes", "Remplacer aussi les autres curseurs"),
                b("curseur", "taille_windows", "Suivre la taille du pointeur de Windows"),
                b("rendu", "carte_graphique", "Dessin par la carte graphique"),
                Item {
                    kind: Kind::Num { min: 0.0, max: 240.0, step: 1.0, zero: Some("écran") },
                    ..n("rendu", "cadence", "Images par seconde", 0.0, 0.0, 0.0)
                },
                Item {
                    kind: Kind::Num { min: 0.0, max: 120.0, step: 1.0, zero: Some("aucun") },
                    ..n("rendu", "cadence_repos", "Images/s au repos", 0.0, 0.0, 0.0)
                },
            ],
        ),
    ],
];

fn items() -> impl Iterator<Item = &'static Item> {
    COLUMNS.iter().flat_map(|c| c.iter()).flat_map(|(_, items)| items.iter())
}

/// Contrôles d'un réglage : libellé, case ou glissière, valeur affichée.
struct Ctrl {
    label: Option<HWND>,
    input: HWND,
    value: Option<HWND>,
}

struct Ui {
    hwnd: HWND,
    dpi: u32,
    font: HFONT,
    groups: Vec<HWND>,
    ctrls: Vec<Ctrl>,
    profile_label: HWND,
    combo: HWND,
    save: HWND,
    delete: HWND,
    file: HWND,
    status: HWND,
    conf: Watched,
    /// Réglages tels que la fenêtre les connaît (fichier + changements pas encore écrits).
    values: Value,
    /// Changements à écrire au prochain tour du minuteur : (indice du réglage, valeur).
    pending: Vec<(usize, Value)>,
    profiles: Vec<Profile>,
    /// Message d'action (profil enregistré, erreur…), prioritaire sur l'état des réglages.
    message: Option<String>,
}

thread_local! {
    static UI: RefCell<Option<Ui>> = const { RefCell::new(None) };
}

/// Ouvre la fenêtre, ou la ramène devant si elle l'est déjà.
pub fn open() {
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
    let _ = std::thread::Builder::new().name("essaim-reglages".into()).spawn(|| {
        if let Err(e) = run() {
            log::warn!("réglages : fenêtre impossible à ouvrir ({e})");
        }
        WINDOW.store(0, Ordering::Relaxed);
    });
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

fn set_text(h: HWND, s: &str) {
    let w = wide(s);
    unsafe {
        let _ = SetWindowTextW(h, PCWSTR(w.as_ptr()));
    }
}

fn get_text(h: HWND) -> String {
    let mut buf = [0u16; 256];
    let n = unsafe { GetWindowTextW(h, &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

fn send(h: HWND, msg: u32, wp: usize, lp: isize) -> isize {
    unsafe { SendMessageW(h, msg, Some(WPARAM(wp)), Some(LPARAM(lp))).0 }
}

unsafe fn child(parent: HWND, class: PCWSTR, text: &str, style: u32, id: usize) -> windows::core::Result<HWND> {
    let t = wide(text);
    CreateWindowExW(
        WINDOW_EX_STYLE(0),
        class,
        PCWSTR(t.as_ptr()),
        WS_CHILD | WS_VISIBLE | WINDOW_STYLE(style),
        0,
        0,
        0,
        0,
        Some(parent),
        Some(HMENU(id as *mut _)),
        None,
        None,
    )
}

fn run() -> windows::core::Result<()> {
    unsafe {
        let icc = INITCOMMONCONTROLSEX {
            dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_BAR_CLASSES | ICC_STANDARD_CLASSES,
        };
        let _ = InitCommonControlsEx(&icc);
        let hinst = GetModuleHandleW(None)?;
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(proc),
            hInstance: hinst.into(),
            lpszClassName: CLASS,
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            hbrBackground: GetSysColorBrush(COLOR_WINDOW),
            ..Default::default()
        };
        RegisterClassExW(&wc);
        let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX;
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            CLASS,
            w!("Essaim - réglages"),
            style,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            100,
            100,
            None,
            None,
            Some(hinst.into()),
            None,
        )?;
        WINDOW.store(hwnd.0 as isize, Ordering::Relaxed);
        if let Some(icon) = crate::win::app_icon() {
            send(hwnd, WM_SETICON, ICON_BIG as usize, icon.0 as isize);
            send(hwnd, WM_SETICON, ICON_SMALL as usize, icon.0 as isize);
        }
        let ui = build(hwnd)?;
        UI.with(|u| *u.borrow_mut() = Some(ui));
        with_ui(|ui| {
            ui.layout(true);
            ui.refresh_profiles();
            ui.show_values();
            ui.select_matching();
        });
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
        SetTimer(Some(hwnd), TIMER_RELOAD, 1000, None);

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            if !IsDialogMessageW(hwnd, &msg).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        UI.with(|u| {
            if let Some(ui) = u.borrow_mut().take() {
                let _ = DeleteObject(ui.font.into());
            }
        });
    }
    Ok(())
}

unsafe fn build(hwnd: HWND) -> windows::core::Result<Ui> {
    let (st, btn) = (w!("STATIC"), w!("BUTTON"));
    let tab = WS_TABSTOP.0;
    let mut groups = Vec::new();
    let mut ctrls = Vec::new();
    let mut id = ID_ITEM;
    for col in COLUMNS {
        for (title, items) in col {
            groups.push(child(hwnd, btn, title, BS_GROUPBOX as u32, 0)?);
            for it in *items {
                ctrls.push(match it.kind {
                    Kind::Bool => Ctrl { label: None, input: child(hwnd, btn, it.label, BS_AUTOCHECKBOX as u32 | tab, id)?, value: None },
                    Kind::Num { min, max, step, .. } => {
                        let label = child(hwnd, st, it.label, SS_LEFT, 0)?;
                        let input = child(hwnd, TRACKBAR_CLASSW, "", TBS_HORZ | TBS_NOTICKS | tab, id)?;
                        let steps = ((max - min) / step).round() as isize;
                        send(input, TBM_SETRANGEMIN, 0, 0);
                        send(input, TBM_SETRANGEMAX, 0, steps);
                        send(input, TBM_SETPAGESIZE, 0, (steps / 10).max(1));
                        let value = child(hwnd, st, "", SS_RIGHT, 0)?;
                        Ctrl { label: Some(label), input, value: Some(value) }
                    }
                });
                id += 1;
            }
        }
    }
    let conf = Watched::load();
    let values = serde_json::to_value(&conf.cfg).unwrap_or(Value::Null);
    Ok(Ui {
        hwnd,
        dpi: GetDpiForWindow(hwnd).max(96),
        font: HFONT::default(),
        groups,
        ctrls,
        profile_label: child(hwnd, st, "Profil :", SS_LEFT, 0)?,
        combo: child(hwnd, w!("COMBOBOX"), "", (CBS_DROPDOWN | CBS_HASSTRINGS | CBS_AUTOHSCROLL) as u32 | WS_VSCROLL.0 | tab, ID_COMBO)?,
        save: child(hwnd, btn, "Enregistrer", BS_PUSHBUTTON as u32 | tab, ID_SAVE)?,
        delete: child(hwnd, btn, "Supprimer", BS_PUSHBUTTON as u32 | tab, ID_DELETE)?,
        file: child(hwnd, btn, "Ouvrir config.toml", BS_PUSHBUTTON as u32 | tab, ID_FILE)?,
        status: child(hwnd, st, "", SS_LEFT | SS_ENDELLIPSIS, 0)?,
        conf,
        values,
        pending: Vec::new(),
        profiles: Vec::new(),
        message: None,
    })
}

fn with_ui<R>(f: impl FnOnce(&mut Ui) -> R) -> Option<R> {
    UI.with(|u| u.try_borrow_mut().ok().and_then(|mut u| u.as_mut().map(f)))
}

impl Ui {
    fn px(&self, v: i32) -> i32 {
        v * self.dpi as i32 / 96
    }

    /// Police et position de tout à l'échelle de l'écran ; `resize` : taille (et position au
    /// centre de l'écran de la souris) de la fenêtre.
    fn layout(&mut self, center: bool) {
        unsafe {
            let mut ncm = NONCLIENTMETRICSW { cbSize: std::mem::size_of::<NONCLIENTMETRICSW>() as u32, ..Default::default() };
            let _ = SystemParametersInfoForDpi(
                SPI_GETNONCLIENTMETRICS.0,
                ncm.cbSize,
                Some(&mut ncm as *mut _ as *mut _),
                0,
                self.dpi,
            );
            let font = CreateFontIndirectW(&ncm.lfMessageFont);
            if !font.is_invalid() {
                if !self.font.is_invalid() {
                    let _ = DeleteObject(self.font.into());
                }
                self.font = font;
            }
        }
        let p = |v| self.px(v);
        let (margin, col_w, gap) = (p(12), p(316), p(12));
        let place = |h: HWND, x: i32, y: i32, w: i32, ht: i32| unsafe {
            let _ = MoveWindow(h, x, y, w, ht, false);
            send(h, WM_SETFONT, self.font.0 as usize, 0);
        };
        // ligne des profils
        let top = margin;
        place(self.profile_label, margin, top + p(4), p(48), p(20));
        place(self.combo, margin + p(52), top, p(230), p(300));
        place(self.save, margin + p(292), top - p(1), p(100), p(26));
        place(self.delete, margin + p(400), top - p(1), p(100), p(26));
        let width = margin * 2 + col_w * 3 + gap * 2;
        place(self.file, width - margin - p(150), top - p(1), p(150), p(26));
        // colonnes de groupes
        let (mut gi, mut ci) = (0, 0);
        let mut bottom = 0;
        for (c, col) in COLUMNS.iter().enumerate() {
            let x = margin + c as i32 * (col_w + gap);
            let mut y = top + p(40);
            for (_, items) in col.iter() {
                let y0 = y;
                y += p(22);
                for it in items.iter() {
                    let ctrl = &self.ctrls[ci];
                    match it.kind {
                        Kind::Bool => {
                            place(ctrl.input, x + p(12), y, col_w - p(24), p(22));
                            y += p(24);
                        }
                        Kind::Num { .. } => {
                            if let (Some(l), Some(v)) = (ctrl.label, ctrl.value) {
                                place(l, x + p(12), y + p(4), p(148), p(20));
                                place(ctrl.input, x + p(160), y, p(104), p(26));
                                place(v, x + p(264), y + p(4), p(42), p(20));
                            }
                            y += p(28);
                        }
                    }
                    ci += 1;
                }
                y += p(8);
                place(self.groups[gi], x, y0, col_w, y - y0);
                gi += 1;
                y += p(8);
            }
            bottom = bottom.max(y);
        }
        place(self.status, margin, bottom + p(2), width - margin * 2, p(20));
        let height = bottom + p(30);
        unsafe {
            let style = WINDOW_STYLE(GetWindowLongW(self.hwnd, GWL_STYLE) as u32);
            let mut r = RECT { left: 0, top: 0, right: width, bottom: height };
            let _ = AdjustWindowRectExForDpi(&mut r, style, false, WINDOW_EX_STYLE(0), self.dpi);
            let (w, h) = (r.right - r.left, r.bottom - r.top);
            if center {
                let mut pt = Default::default();
                let _ = GetCursorPos(&mut pt);
                let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
                let _ = GetMonitorInfoW(MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST), &mut mi);
                let wa = mi.rcWork;
                let _ = SetWindowPos(
                    self.hwnd,
                    None,
                    wa.left + (wa.right - wa.left - w).max(0) / 2,
                    wa.top + (wa.bottom - wa.top - h).max(0) / 2,
                    w,
                    h,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
            } else {
                let _ = SetWindowPos(self.hwnd, None, 0, 0, w, h, SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOMOVE);
            }
            let _ = windows::Win32::Graphics::Gdi::InvalidateRect(Some(self.hwnd), None, true);
        }
    }

    fn value(&self, it: &Item) -> Option<&Value> {
        self.values.get(it.section)?.get(it.key)
    }

    fn show_values(&self) {
        for (it, ctrl) in items().zip(&self.ctrls) {
            let Some(v) = self.value(it) else { continue };
            match it.kind {
                Kind::Bool => {
                    let on = v.as_bool().unwrap_or(false);
                    send(ctrl.input, BM_SETCHECK, if on { BST_CHECKED.0 } else { BST_UNCHECKED.0 } as usize, 0);
                }
                Kind::Num { min, step, .. } => {
                    let x = v.as_f64().unwrap_or(0.0);
                    send(ctrl.input, TBM_SETPOS, 1, ((x - min) / step).round() as isize);
                    if let Some(h) = ctrl.value {
                        set_text(h, &format_value(it, x));
                    }
                }
            }
        }
        self.show_status();
    }

    fn cfg(&self) -> config::Config {
        serde_json::from_value(self.values.clone()).unwrap_or_default()
    }

    fn matching(&self) -> Option<&Profile> {
        let cfg = self.cfg();
        self.profiles.iter().find(|p| profils::same(&p.cfg, &cfg))
    }

    fn show_status(&self) {
        let text = match (&self.message, self.matching()) {
            (Some(m), _) => m.clone(),
            (None, Some(p)) => format!("Réglages actuels : profil « {} »{}.", p.name, if p.builtin { " (préréglage)" } else { "" }),
            (None, None) => "Réglages modifiés : tape un nom puis « Enregistrer » pour en faire un profil.".into(),
        };
        set_text(self.status, &text);
    }

    fn refresh_profiles(&mut self) {
        self.profiles = profils::list();
        let typed = get_text(self.combo);
        send(self.combo, CB_RESETCONTENT, 0, 0);
        for p in &self.profiles {
            let w = wide(&p.name);
            send(self.combo, CB_ADDSTRING, 0, w.as_ptr() as isize);
        }
        set_text(self.combo, &typed);
    }

    /// Met dans la liste le profil qui correspond aux réglages, s'il y en a un.
    fn select_matching(&self) {
        if let Some(i) = self.matching().and_then(|m| self.profiles.iter().position(|p| p.name == m.name)) {
            send(self.combo, CB_SETCURSEL, i, 0);
        }
    }

    /// Relit `config.toml` s'il a changé (sauf pendant un glissement ou avant d'avoir tout écrit).
    fn reload(&mut self) {
        if !self.pending.is_empty() || unsafe { !GetCapture().is_invalid() } {
            return;
        }
        if self.conf.reload() {
            self.values = serde_json::to_value(&self.conf.cfg).unwrap_or(Value::Null);
            self.show_values();
        }
    }

    /// Le réglage `i` a changé dans la fenêtre : affiché tout de suite, écrit au prochain tour.
    fn changed(&mut self, i: usize, write: bool) {
        let Some(it) = items().nth(i) else { return };
        let ctrl = &self.ctrls[i];
        let v = match it.kind {
            Kind::Bool => Value::Bool(send(ctrl.input, BM_GETCHECK, 0, 0) == BST_CHECKED.0 as isize),
            Kind::Num { min, step, .. } => {
                let x = min + send(ctrl.input, TBM_GETPOS, 0, 0) as f64 * step;
                let x = (x / step).round() * step; // pas de 0,30000000000000004
                if let Some(h) = ctrl.value {
                    set_text(h, &format_value(it, x));
                }
                serde_json::json!(x)
            }
        };
        if let Some(s) = self.values.get_mut(it.section).and_then(|s| s.as_object_mut()) {
            s.insert(it.key.into(), v.clone());
        }
        self.message = None;
        if write {
            self.pending.retain(|(j, _)| *j != i);
            self.pending.push((i, v));
            unsafe {
                SetTimer(Some(self.hwnd), TIMER_FLUSH, 60, None);
            }
        }
        self.show_status();
    }

    fn flush(&mut self) {
        unsafe {
            let _ = KillTimer(Some(self.hwnd), TIMER_FLUSH);
        }
        let all: Vec<_> = items().collect();
        for (i, v) in std::mem::take(&mut self.pending) {
            if let Err(e) = config::set_value(all[i].section, all[i].key, &v) {
                self.message = Some(format!("Réglage refusé : {e}"));
                self.show_status();
            }
        }
        config::request_reload();
    }

    fn apply_selected(&mut self) {
        let i = send(self.combo, CB_GETCURSEL, 0, 0);
        let Some(p) = usize::try_from(i).ok().and_then(|i| self.profiles.get(i)) else { return };
        self.pending.clear();
        self.message = Some(match profils::apply(p) {
            Ok(()) => format!("Profil « {} » appliqué.", p.name),
            Err(e) => format!("Profil « {} » non appliqué : {e}", p.name),
        });
        config::request_reload();
        self.conf.reload();
        self.values = serde_json::to_value(&self.conf.cfg).unwrap_or(Value::Null);
        self.show_values();
    }

    fn save_profile(&mut self) {
        self.flush();
        let name = get_text(self.combo);
        self.message = Some(match profils::save(&name, &self.cfg()) {
            Ok(name) => {
                self.refresh_profiles();
                set_text(self.combo, &name);
                format!("Profil « {name} » enregistré.")
            }
            Err(e) => format!("Profil non enregistré : {e}."),
        });
        self.show_status();
    }

    fn delete_profile(&mut self) {
        let name = get_text(self.combo);
        self.message = Some(match profils::delete(&name) {
            Ok(()) => {
                set_text(self.combo, "");
                self.refresh_profiles();
                format!("Profil « {} » supprimé.", name.trim())
            }
            Err(e) => format!("Profil non supprimé : {e}."),
        });
        self.show_status();
    }
}

fn format_value(it: &Item, x: f64) -> String {
    let Kind::Num { step, zero, .. } = it.kind else { return String::new() };
    if let (Some(z), true) = (zero, x.abs() < 1e-9) {
        return z.into();
    }
    let decimals = if step >= 1.0 { 0 } else if step >= 0.1 { 1 } else { 2 };
    format!("{x:.decimals$}").replace('.', ",")
}

extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            // fond des libellés, cases et glissières : celui de la fenêtre (sans toucher à l'état,
            // ces messages arrivent pendant un dessin déclenché de l'intérieur)
            WM_CTLCOLORSTATIC | WM_CTLCOLORBTN => {
                SetBkColor(HDC(wp.0 as *mut _), COLORREF(GetSysColor(COLOR_WINDOW)));
                return LRESULT(GetSysColorBrush(COLOR_WINDOW).0 as isize);
            }
            WM_HSCROLL => {
                let code = (wp.0 & 0xFFFF) as i32;
                let src = HWND(lp.0 as *mut _);
                with_ui(|ui| {
                    if let Some(i) = ui.ctrls.iter().position(|c| c.input == src) {
                        let live = items().nth(i).is_some_and(|it| it.live);
                        ui.changed(i, live || code != TB_THUMBTRACK as i32);
                    }
                });
                return LRESULT(0);
            }
            WM_COMMAND => {
                let (id, code) = (wp.0 & 0xFFFF, (wp.0 >> 16) & 0xFFFF);
                let done = with_ui(|ui| match id {
                    ID_COMBO if code == CBN_SELCHANGE as usize => ui.apply_selected(),
                    // Entrée dans la zone du nom : enregistrer
                    ID_SAVE => ui.save_profile(),
                    x if x == IDOK.0 as usize && GetParent(GetFocus()).is_ok_and(|p| p == ui.combo) => ui.save_profile(),
                    ID_DELETE => ui.delete_profile(),
                    ID_FILE => crate::win::open_file(&config::path()),
                    x if x == IDCANCEL.0 as usize => {
                        let _ = DestroyWindow(hwnd);
                    }
                    x if x >= ID_ITEM && code == BN_CLICKED as usize => ui.changed(x - ID_ITEM, true),
                    _ => {}
                });
                if done.is_some() {
                    return LRESULT(0);
                }
            }
            WM_TIMER => {
                with_ui(|ui| match wp.0 {
                    TIMER_FLUSH => ui.flush(),
                    _ => ui.reload(),
                });
                return LRESULT(0);
            }
            WM_DPICHANGED => {
                let dpi = ((wp.0 >> 16) & 0xFFFF) as u32;
                let r = &*(lp.0 as *const RECT);
                let _ = SetWindowPos(hwnd, None, r.left, r.top, 0, 0, SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOSIZE);
                with_ui(|ui| {
                    ui.dpi = dpi.max(96);
                    ui.layout(false);
                });
                return LRESULT(0);
            }
            WM_CLOSE => {
                with_ui(|ui| {
                    if !ui.pending.is_empty() {
                        ui.flush();
                    }
                });
                let _ = DestroyWindow(hwnd);
                return LRESULT(0);
            }
            WM_DESTROY => {
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
    fn chaque_reglage_montre_existe_et_ses_bornes_tiennent() {
        let known = serde_json::to_value(config::Config::default()).unwrap();
        let mut seen = std::collections::HashSet::new();
        for it in items() {
            let v = known.get(it.section).and_then(|s| s.get(it.key));
            // le groupe « hors profils » contient exactement ce que les profils ignorent
            let machine = COLUMNS[2][1].1.iter().any(|o| (o.section, o.key) == (it.section, it.key));
            assert_eq!(profils::in_profile(it.section, it.key), !machine, "[{}] {}", it.section, it.key);
            assert!(v.is_some(), "[{}] {} inconnu", it.section, it.key);
            assert!(seen.insert((it.section, it.key)), "[{}] {} en double", it.section, it.key);
            match it.kind {
                Kind::Bool => assert!(v.unwrap().is_boolean()),
                Kind::Num { min, max, step, .. } => {
                    assert!(min < max && step > 0.0, "[{}] {}", it.section, it.key);
                    // les deux bouts de la glissière passent la vérification d'écriture
                    for x in [min, max] {
                        let p = std::env::temp_dir().join(format!("essaim-ui-{}-{}.toml", std::process::id(), it.key));
                        let r = config::set_value_in(&p, it.section, it.key, &serde_json::json!(x));
                        let _ = std::fs::remove_file(&p);
                        assert!(r.is_ok(), "[{}] {} = {x} : {r:?}", it.section, it.key);
                    }
                }
            }
        }
    }

    #[test]
    fn valeurs_affichees() {
        let it = items().find(|i| i.key == "trainee").unwrap();
        assert_eq!(format_value(it, 0.3), "0,30");
        let it = items().find(|i| i.key == "cadence").unwrap();
        assert_eq!(format_value(it, 0.0), "écran");
        assert_eq!(format_value(it, 72.0), "72");
    }
}
