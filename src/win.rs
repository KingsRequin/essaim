//! Tout ce qui touche à Windows : fenêtres superposées transparentes, pointeur, écrans,
//! icône de notification.

use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::Once;
use std::time::{Duration, Instant};
use tiny_skia::Pixmap;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HANDLE, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS};
use windows::Win32::Graphics::Gdi::{
    CreateBitmap, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetMonitorInfoW, MonitorFromPoint,
    MonitorFromWindow, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP, HDC, HGDIOBJ,
    EnumDisplayMonitors, HMONITOR, MONITORINFO, MONITOR_DEFAULTTONEAREST, AC_SRC_ALPHA, AC_SRC_OVER,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::{CreateWaitableTimerExW, SetWaitableTimer, CREATE_WAITABLE_TIMER_HIGH_RESOLUTION, TIMER_ALL_ACCESS};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::Shell::{
    ShellExecuteW, Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW,
};
use windows::Win32::UI::WindowsAndMessaging::*;

// ---------------------------------------------------------------------------------------------
// Fenêtre superposée : transparente aux clics, toujours devant, hors barre des tâches.

static OVERLAY_CLASS: Once = Once::new();
const CLASSE_CALQUE: PCWSTR = w!("EssaimOverlay");

/// Exécute le corps d'une procédure de fenêtre (ou d'un autre rappel de Windows) en arrêtant une
/// panique à sa frontière : sortie d'une fonction `extern "system"`, elle arrêterait le processus
/// d'un coup (`abort`), sans passer par le filtre de plantage qui rend la bordure. Le gestionnaire
/// de paniques l'a déjà notée au journal ; `None` : le corps a paniqué.
pub fn sans_panique<T>(lieu: &str, corps: impl FnOnce() -> T) -> Option<T> {
    // le crochet de panique de `main` ne rend pas la bordure pour une panique arrêtée ici :
    // l'essaim continue et colore encore
    let avant = ARRETEE.with(|a| a.replace(true));
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(corps));
    ARRETEE.with(|a| a.set(avant));
    match r {
        Ok(r) => Some(r),
        Err(_) => {
            log::error!("{lieu} : panique arrêtée");
            None
        }
    }
}

thread_local! {
    static ARRETEE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Vrai pendant un corps protégé par `sans_panique` : une panique y sera arrêtée.
pub fn panique_arretee() -> bool {
    ARRETEE.with(|a| a.get())
}

extern "system" fn overlay_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let corps = || {
        if msg == WM_NCHITTEST {
            return LRESULT(HTTRANSPARENT as isize);
        }
        unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
    };
    sans_panique("calque", corps).unwrap_or_else(|| unsafe { DefWindowProcW(hwnd, msg, wp, lp) })
}

pub struct Surface {
    hwnd: HWND,
    dc: HDC,
    bmp: HBITMAP,
    old: HGDIOBJ,
    bits: *mut u8,
    w: i32,
    h: i32,
    /// Affichée au premier envoi, puis jamais cachée : vider le calque, c'est lui envoyer une zone
    /// transparente (`Layer::clear`). Une fenêtre cachée puis remontrée sous la souris ferait
    /// redemander leur curseur aux applications.
    shown: bool,
    /// Le dernier envoi a échoué : le suivant renvoie toute l'image (la zone perdue serait
    /// restée périmée à l'écran).
    resend: bool,
}

/// Fenêtre superposée transparente aux clics, toujours devant, hors barre des tâches, au
/// rectangle écran `rect`, créée cachée. `composition` : sans surface GDI, pour un contenu
/// DirectComposition (à afficher par `show_window` une fois prête ; elle ne bouge plus ensuite).
/// En cas d'erreur, la fenêtre déjà créée est détruite.
pub fn overlay_window(rect: (i32, i32, i32, i32), composition: bool) -> windows::core::Result<HWND> {
    unsafe {
        let hinst = GetModuleHandleW(None)?;
        OVERLAY_CLASS.call_once(|| {
            let wc = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                lpfnWndProc: Some(overlay_proc),
                hInstance: hinst.into(),
                lpszClassName: CLASSE_CALQUE,
                ..Default::default()
            };
            RegisterClassExW(&wc);
        });
        let mut ex = WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE;
        if composition {
            ex |= WS_EX_NOREDIRECTIONBITMAP;
        }
        let hwnd = CreateWindowExW(
            ex,
            CLASSE_CALQUE,
            w!("Essaim"),
            WS_POPUP,
            rect.0,
            rect.1,
            (rect.2 - rect.0).max(1),
            (rect.3 - rect.1).max(1),
            None,
            None,
            Some(hinst.into()),
            None,
        )?;
        let preparer = || -> windows::core::Result<()> {
            // Sans cette propriété, l'Explorateur prend ce calque (toujours devant, de la taille
            // de l'écran) pour une appli plein écran : la barre des tâches masquée
            // automatiquement perd son « toujours devant » et ressort derrière les fenêtres.
            SetPropW(hwnd, w!("NonRudeHWND"), Some(windows::Win32::Foundation::HANDLE(std::ptr::without_provenance_mut(1))))?;
            if composition {
                // une fenêtre « layered » n'est montrée qu'avec des attributs ; l'opacité globale
                // reste pleine, la transparence vient du contenu DirectComposition
                SetLayeredWindowAttributes(hwnd, COLORREF(0), 255, LWA_ALPHA)?;
            }
            Ok(())
        };
        // fenêtre encore cachée : la détruire ne fait rien apparaître ni disparaître à l'écran
        if let Err(e) = preparer() {
            let _ = DestroyWindow(hwnd);
            return Err(e);
        }
        if MASQUER_CAPTURES.load(Ordering::Relaxed) {
            exclure_des_captures(hwnd, true);
        }
        Ok(hwnd)
    }
}

/// Affiche un calque DirectComposition une fois son contenu prêt (`overlay_window` le crée
/// caché : un calque dont la préparation échoue n'apparaît jamais).
pub fn show_window(hwnd: HWND) {
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    }
}

pub fn destroy_window(hwnd: HWND) {
    unsafe {
        let _ = DestroyWindow(hwnd);
    }
}

/// Calques exclus des captures d'écran et des streams (`[rendu] masquer_captures`).
static MASQUER_CAPTURES: AtomicBool = AtomicBool::new(false);

fn exclure_des_captures(hwnd: HWND, oui: bool) {
    let affinite = if oui { WDA_EXCLUDEFROMCAPTURE } else { WDA_NONE };
    if let Err(e) = unsafe { SetWindowDisplayAffinity(hwnd, affinite) } {
        log::warn!("calque : exclusion des captures impossible ({e})");
    }
}

/// Exclut les calques des captures d'écran et des streams (`WDA_EXCLUDEFROMCAPTURE`, Windows 10
/// 2004 et plus), ou les y remet : les calques existants tout de suite, les suivants à leur
/// création. Fil principal (celui qui crée les calques).
pub fn masquer_captures(oui: bool) {
    if MASQUER_CAPTURES.swap(oui, Ordering::Relaxed) == oui {
        return;
    }
    chaque_calque(|h| exclure_des_captures(h, oui));
    log::info!("{}", if oui { "calques masqués des captures et des streams" } else { "calques de nouveau visibles des captures" });
}

/// Appelle `f` sur chaque calque. Fil principal (celui qui crée les calques).
fn chaque_calque(mut f: impl FnMut(HWND)) {
    unsafe extern "system" fn chaque(h: HWND, l: LPARAM) -> windows::core::BOOL {
        let mut classe = [0u16; 32];
        let n = unsafe { GetClassNameW(h, &mut classe) } as usize;
        if classe[..n] == *unsafe { CLASSE_CALQUE.as_wide() } {
            let f = unsafe { &mut *(l.0 as *mut &mut dyn FnMut(HWND)) };
            f(h);
        }
        true.into()
    }
    let mut f: &mut dyn FnMut(HWND) = &mut f;
    unsafe {
        let _ = EnumThreadWindows(
            windows::Win32::System::Threading::GetCurrentThreadId(),
            Some(chaque),
            LPARAM(&mut f as *mut &mut dyn FnMut(HWND) as isize),
        );
    }
}

/// Remet « toujours devant » les calques affichés qui ne le sont plus, et renvoie leur nombre.
/// Rien dans Essaim ne change leur place, mais un autre programme peut la leur retirer
/// (`SetWindowPos` avec `HWND_NOTOPMOST` ou `HWND_BOTTOM`) : l'essaim passerait alors derrière
/// les fenêtres ordinaires jusqu'à ce que le calque soit refait. Ne touche à rien (pas de
/// changement d'ordre sous la souris) tant que tout est en place. Fil principal.
pub fn remettre_devant() -> u32 {
    let mut n = 0;
    chaque_calque(|h| unsafe {
        let devant = GetWindowLongPtrW(h, GWL_EXSTYLE) as u32 & WS_EX_TOPMOST.0 != 0;
        if !devant && IsWindowVisible(h).as_bool() {
            let drapeaux = SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOOWNERZORDER;
            if SetWindowPos(h, Some(HWND_TOPMOST), 0, 0, 0, 0, drapeaux).is_ok() {
                n += 1;
            }
        }
    });
    n
}

impl Surface {
    pub fn new() -> windows::core::Result<Self> {
        unsafe {
            let hwnd = overlay_window((0, 0, 1, 1), false)?;
            let dc = CreateCompatibleDC(None);
            Ok(Self {
                hwnd,
                dc,
                bmp: HBITMAP::default(),
                old: HGDIOBJ::default(),
                bits: std::ptr::null_mut(),
                w: 0,
                h: 0,
                shown: false,
                resend: false,
            })
        }
    }

    /// Adapte la surface à la taille de l'image. Renvoie vrai si elle a été (re)créée.
    fn ensure_size(&mut self, w: i32, h: i32) -> windows::core::Result<bool> {
        if w == self.w && h == self.h && !self.bits.is_null() {
            return Ok(false);
        }
        unsafe {
            if !self.bmp.is_invalid() {
                SelectObject(self.dc, self.old);
                let _ = DeleteObject(self.bmp.into());
            }
            // ancienne image libérée : rien ne doit plus y renvoyer, même si la suivante échoue
            (self.bmp, self.bits, self.w, self.h) = (HBITMAP::default(), std::ptr::null_mut(), 0, 0);
            let bi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: w,
                    biHeight: -h, // de haut en bas
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits: *mut c_void = std::ptr::null_mut();
            self.bmp = CreateDIBSection(Some(self.dc), &bi, DIB_RGB_COLORS, &mut bits, None, 0)?;
            self.old = SelectObject(self.dc, self.bmp.into());
            self.bits = bits as *mut u8;
        }
        self.w = w;
        self.h = h;
        Ok(true)
    }

    /// Envoie l'image (RGBA prémultiplié) et l'affiche avec son coin haut-gauche en (x, y).
    /// `region` (gauche, haut, droite, bas, en pixels de l'image) limite l'envoi à la zone qui a
    /// changé ; `None` envoie tout. La fenêtre ne bouge pas tant que (x, y) et la taille restent
    /// les mêmes : une fenêtre qui bouge sous la souris fait redemander leur curseur aux
    /// applications (l'Explorateur répond alors « flèche »).
    pub fn update(&mut self, pm: &Pixmap, x: i32, y: i32, region: Option<(i32, i32, i32, i32)>) -> windows::core::Result<()> {
        let resized = self.ensure_size(pm.width() as i32, pm.height() as i32)?;
        let full = resized || !self.shown || self.resend || region.is_none();
        let (l, t, r, b) = if full {
            (0, 0, self.w, self.h)
        } else {
            let (l, t, r, b) = region.unwrap_or_default();
            (l.max(0), t.max(0), r.min(self.w), b.min(self.h))
        };
        if r <= l || b <= t {
            return Ok(());
        }
        let stride = self.w as usize * 4;
        let dst = unsafe { std::slice::from_raw_parts_mut(self.bits, stride * self.h as usize) };
        let src = pm.data();
        for row in t as usize..b as usize {
            let (a, z) = (row * stride + l as usize * 4, row * stride + r as usize * 4);
            rgba_vers_bgra(&mut dst[a..z], &src[a..z]);
        }
        let blend = windows::Win32::Graphics::Gdi::BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        let (dst_pt, size, src_pt) = (POINT { x, y }, SIZE { cx: self.w, cy: self.h }, POINT { x: 0, y: 0 });
        let dirty = RECT { left: l, top: t, right: r, bottom: b };
        let info = UPDATELAYEREDWINDOWINFO {
            cbSize: std::mem::size_of::<UPDATELAYEREDWINDOWINFO>() as u32,
            pptDst: &dst_pt,
            psize: &size,
            hdcSrc: self.dc,
            pptSrc: &src_pt,
            pblend: &blend,
            dwFlags: ULW_ALPHA,
            prcDirty: if full { std::ptr::null() } else { &dirty },
            ..Default::default()
        };
        unsafe {
            if let Err(e) = UpdateLayeredWindowIndirect(self.hwnd, &info).ok() {
                self.resend = true;
                return Err(e);
            }
            self.resend = false;
            if !self.shown {
                let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
                self.shown = true;
            }
        }
        Ok(())
    }
}

/// RGBA → BGRA, pixel par pixel sur des mots de 32 bits (le compilateur le vectorise).
/// Pixels RGBA (tiny-skia) → BGRA (GDI).
pub fn rgba_vers_bgra(dst: &mut [u8], src: &[u8]) {
    for (d, s) in dst.as_chunks_mut::<4>().0.iter_mut().zip(src.as_chunks::<4>().0.iter()) {
        let p = u32::from_le_bytes([s[0], s[1], s[2], s[3]]);
        let q = (p & 0xFF00_FF00) | (p >> 16 & 0xFF) | (p & 0xFF) << 16;
        d.copy_from_slice(&q.to_le_bytes());
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        unsafe {
            if !self.bmp.is_invalid() {
                SelectObject(self.dc, self.old);
                let _ = DeleteObject(self.bmp.into());
            }
            let _ = DeleteDC(self.dc);
            let _ = DestroyWindow(self.hwnd);
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Pointeur

pub struct CursorState {
    pub pos: (i32, i32),
    /// Le curseur est affiché (les jeux et lecteurs vidéo le cachent).
    pub showing: bool,
    /// C'est le curseur de texte de Windows (zone de saisie, texte d'une page) : `[pause] texte`.
    pub texte: bool,
}

pub fn cursor_state() -> Option<CursorState> {
    unsafe {
        let mut ci = CURSORINFO { cbSize: std::mem::size_of::<CURSORINFO>() as u32, ..Default::default() };
        GetCursorInfo(&mut ci).ok()?;
        Some(CursorState {
            pos: (ci.ptScreenPos.x, ci.ptScreenPos.y),
            showing: ci.flags.0 & CURSOR_SHOWING.0 != 0,
            texte: !ci.hCursor.is_invalid() && ci.hCursor.0 as isize == curseur_texte(),
        })
    }
}

/// Poignée du curseur de texte partagé (`IDC_IBEAM`), celle que les applications (navigateurs,
/// Bloc-notes, Office…) posent sur une zone de texte. Un autre jeu de curseurs en change l'image,
/// pas la poignée : lue une fois.
fn curseur_texte() -> isize {
    static TEXTE: AtomicIsize = AtomicIsize::new(0);
    match TEXTE.load(Ordering::Relaxed) {
        0 => {
            let h = unsafe { LoadCursorW(None, IDC_IBEAM) }.map_or(-1, |h| h.0 as isize);
            TEXTE.store(h, Ordering::Relaxed);
            h
        }
        h => h,
    }
}

/// Tous les écrans : (identifiant, rectangle gauche, haut, droite, bas en pixels physiques).
pub fn monitors() -> Vec<(isize, (i32, i32, i32, i32))> {
    unsafe extern "system" fn each(mon: HMONITOR, _: HDC, r: *mut RECT, data: LPARAM) -> windows::core::BOOL {
        let list = unsafe { &mut *(data.0 as *mut Vec<(isize, (i32, i32, i32, i32))>) };
        let r = unsafe { *r };
        list.push((mon.0 as isize, (r.left, r.top, r.right, r.bottom)));
        true.into()
    }
    let mut list: Vec<(isize, (i32, i32, i32, i32))> = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(None, None, Some(each), LPARAM(&mut list as *mut _ as isize));
    }
    list
}

/// Échelles déjà lues, par écran, et date de la lecture. Oubliées au changement de configuration
/// des écrans et, par prudence (un changement d'échelle n'arrive pas toujours à une fenêtre
/// cachée), toutes les 2 s.
type Echelles = (Vec<(isize, f32)>, Option<Instant>);
thread_local! {
    static ECHELLES: RefCell<Echelles> = const { RefCell::new((Vec::new(), None)) };
}
const ECHELLES_VALIDES: Duration = Duration::from_secs(2);

/// Échelle de l'écran sous le point (1.0 = 100 %).
pub fn scale_at(x: i32, y: i32) -> f32 {
    let mon = unsafe { MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONEAREST) };
    ECHELLES.with(|e| {
        let mut e = e.borrow_mut();
        let now = Instant::now();
        if e.1.is_none_or(|t| now - t >= ECHELLES_VALIDES) {
            *e = (Vec::new(), Some(now));
        }
        if let Some(&(_, s)) = e.0.iter().find(|(m, _)| *m == mon.0 as isize) {
            return s;
        }
        let (mut dx, mut dy) = (96u32, 96u32);
        let s = if unsafe { GetDpiForMonitor(mon, MDT_EFFECTIVE_DPI, &mut dx, &mut dy) }.is_ok() { dx as f32 / 96.0 } else { 1.0 };
        e.0.push((mon.0 as isize, s));
        s
    })
}

thread_local! {
    /// Dernière fréquence lue : écran, images par seconde (oubliée au changement des écrans).
    static FREQUENCE: Cell<Option<(isize, u32)>> = const { Cell::new(None) };
}

/// Fréquence de rafraîchissement de l'écran `mon` (Hz) ; 0 si Windows ne la donne pas.
fn frequence_moniteur(mon: HMONITOR) -> u32 {
    use windows::Win32::Graphics::Gdi::{EnumDisplaySettingsW, DEVMODEW, ENUM_CURRENT_SETTINGS, MONITORINFOEXW};
    let mut mi = MONITORINFOEXW::default();
    mi.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
    let mut dm = DEVMODEW { dmSize: std::mem::size_of::<DEVMODEW>() as u16, ..Default::default() };
    unsafe {
        if GetMonitorInfoW(mon, &mut mi.monitorInfo).as_bool() && EnumDisplaySettingsW(PCWSTR(mi.szDevice.as_ptr()), ENUM_CURRENT_SETTINGS, &mut dm).as_bool() {
            // 0 et 1 : « fréquence par défaut du matériel », inconnue
            if dm.dmDisplayFrequency > 1 { dm.dmDisplayFrequency } else { 0 }
        } else {
            0
        }
    }
}

/// Fréquence de rafraîchissement de l'écran sous le point (Hz) ; 0 si Windows ne la donne pas.
/// Relue seulement quand le point change d'écran ou que les écrans changent.
pub fn frequence_ecran(x: i32, y: i32) -> u32 {
    let mon = unsafe { MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONEAREST) };
    if let Some((m, f)) = FREQUENCE.get() {
        if m == mon.0 as isize {
            return f;
        }
    }
    let f = frequence_moniteur(mon);
    FREQUENCE.set(Some((mon.0 as isize, f)));
    f
}

/// Fréquence de l'écran le plus rapide parmi `ecrans` (`monitors`), en Hz ; 0 si aucune n'est
/// connue. Avec des écrans de fréquences différentes, le compositeur (et donc `DwmFlush`) suit
/// celui-là.
pub fn frequence_max(ecrans: &[(isize, crate::layer::IRect)]) -> u32 {
    ecrans.iter().map(|&(h, _)| frequence_moniteur(HMONITOR(h as *mut _))).max().unwrap_or(0)
}

/// Fréquence de l'écran qui porte la fenêtre `hwnd` (Hz, 0 = inconnue). Sans cache : sert à un
/// autre fil que la boucle principale (fenêtre pour le stream), qui la relit de lui-même.
pub fn frequence_fenetre(hwnd: HWND) -> u32 {
    frequence_moniteur(unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) })
}

/// Ce qu'il faut savoir d'une fenêtre pour chercher celle qui est sous le pointeur.
#[derive(Clone, Debug, Default)]
pub struct VueFenetre {
    /// Visible et non réduite.
    pub visible: bool,
    /// Masquée par DWM (`DWMWA_CLOAKED` : bureau virtuel caché, appli du Store suspendue…).
    pub masquee: bool,
    pub style: u32,
    pub style_etendu: u32,
    pub classe: String,
    /// A une propriétaire (`GetWindow(GW_OWNER)`) : menu, bulle, liste déroulante, dialogue d'une
    /// autre fenêtre.
    pub possedee: bool,
    /// En plein écran sur l'écran du pointeur (`covers_monitor`, même écran que le point).
    pub plein_ecran: bool,
    /// Racine de la fenêtre au premier plan (`GetAncestor(GetForegroundWindow(), GA_ROOTOWNER)`) :
    /// l'application qu'on utilise, même sans cadre ni barre de titre.
    pub premier_plan: bool,
    /// Cadre affiché, sans la marge invisible de redimensionnement.
    pub cadre: RECT,
}

/// Classes jamais prises pour la fenêtre sous le pointeur : bureau (et fond animé de Wallpaper
/// Engine), barres des tâches, calques d'Essaim, infobulles (elles passent sous le pointeur sans
/// être ce qu'on regarde).
const CLASSES_PAS_SOUS_POINTEUR: [&str; 6] = ["Progman", "WorkerW", "Shell_TrayWnd", "Shell_SecondaryTrayWnd", "EssaimOverlay", "tooltips_class32"];

/// Classe des menus (menu contextuel, menu d'une barre).
const CLASSE_MENU: &str = "#32768";

/// Fenêtre de passage, qui ne couvre pas l'écran et passe au-dessus de ce qu'on regarde sans
/// l'être : menu, fenêtre d'outil (`WS_EX_TOOLWINDOW` : bulles, notifications, palettes), fenêtre
/// qui ne prend jamais le clavier (`WS_EX_NOACTIVATE`), fenêtre possédée (menu, liste, bulle ou
/// dialogue d'une autre), surcouche toujours devant (`WS_EX_LAYERED` + `WS_EX_TOPMOST` : celles de
/// Discord, Steam, NVIDIA, même sans `WS_EX_TRANSPARENT`). Survolée au-dessus d'une vidéo en plein
/// écran, elle ne lève pas la pause.
pub fn de_passage(v: &VueFenetre) -> bool {
    let ex = v.style_etendu;
    let surcouche = (WS_EX_LAYERED | WS_EX_TOPMOST).0;
    !v.plein_ecran
        && (v.classe == CLASSE_MENU
            || ex & WS_EX_TOOLWINDOW.0 != 0
            || ex & WS_EX_NOACTIVATE.0 != 0
            || v.possedee
            || ex & surcouche == surcouche)
}

/// La fenêtre `v` est-elle celle que le pointeur survole en (`x`, `y`), si rien n'est au-dessus ?
/// Un calque traversé par la souris (d'Essaim ou d'une autre surcouche) n'en est pas une, ni une
/// fenêtre de passage (`de_passage`).
pub fn retenue_sous_point(v: &VueFenetre, x: i32, y: i32) -> bool {
    let r = v.cadre;
    v.visible
        && !v.masquee
        && !calque(v.style_etendu)
        && !CLASSES_PAS_SOUS_POINTEUR.contains(&v.classe.as_str())
        && !de_passage(v)
        && (r.left..r.right).contains(&x)
        && (r.top..r.bottom).contains(&y)
}

/// Vraie fenêtre de premier niveau (à barre de titre ou bord de redimensionnement : une
/// application qu'on regarde), parmi celles que `retenue_sous_point` garde déjà. Survolée, elle
/// lève la pause d'un plein écran resté derrière.
fn vraie_fenetre(v: &VueFenetre) -> bool {
    v.style & WS_CAPTION.0 == WS_CAPTION.0 || v.style & WS_THICKFRAME.0 != 0
}

/// Première fenêtre retenue (`retenue_sous_point`) parmi `fenetres`, données dans l'ordre Z (du
/// dessus vers le dessous) : celle sous le pointeur, avec sa vue.
fn survolee<T>(fenetres: impl IntoIterator<Item = (T, VueFenetre)>, x: i32, y: i32) -> Option<(T, VueFenetre)> {
    fenetres.into_iter().find(|(_, v)| retenue_sous_point(v, x, y))
}

/// Fenêtre en plein écran qui met l'essaim en pause, parmi `fenetres` (ordre Z, du dessus vers le
/// dessous) avec le pointeur en (`x`, `y`). `precedente` : celle retenue au dernier passage, si
/// elle couvre encore l'écran du pointeur (`None` sinon). La fenêtre survolée en plein écran
/// l'emporte ; sinon la précédente reste tant que la fenêtre survolée n'est pas une vraie fenêtre
/// (`vraie_fenetre`) ou la racine de la fenêtre au premier plan (une appli sans cadre qu'on vient
/// d'activer : Spotify, Discord, un lanceur de jeu) : une fenêtre sans cadre ni bulle reconnue,
/// un instant de flottement de l'ordre Z, ne lèvent pas la pause.
pub fn plein_ecran_retenu<T>(fenetres: impl IntoIterator<Item = (T, VueFenetre)>, x: i32, y: i32, precedente: Option<T>) -> Option<T> {
    match survolee(fenetres, x, y) {
        Some((h, v)) if v.plein_ecran => Some(h),
        Some((_, v)) if vraie_fenetre(&v) || v.premier_plan => None,
        _ => precedente,
    }
}

/// Vue de `hwnd` pour `retenue_sous_point`, ou `None` si elle ne peut pas contenir le point (lue
/// d'abord : visibilité et rectangle, deux appels peu coûteux ; le reste seulement ensuite).
/// `ecran` : celui du point.
fn vue_fenetre(hwnd: HWND, x: i32, y: i32, ecran: HMONITOR) -> Option<VueFenetre> {
    unsafe {
        if !IsWindowVisible(hwnd).as_bool() || IsIconic(hwnd).as_bool() {
            return None;
        }
        let mut r = RECT::default();
        GetWindowRect(hwnd, &mut r).ok()?;
        if !(r.left..r.right).contains(&x) || !(r.top..r.bottom).contains(&y) {
            return None;
        }
        let mut cadre = RECT::default();
        if DwmGetWindowAttribute(hwnd, DWMWA_EXTENDED_FRAME_BOUNDS, &mut cadre as *mut RECT as *mut c_void, std::mem::size_of::<RECT>() as u32).is_err() {
            cadre = r;
        }
        let mut classe = [0u16; 64];
        let n = GetClassNameW(hwnd, &mut classe) as usize;
        Some(VueFenetre {
            visible: true,
            masquee: masquee_par_dwm(hwnd),
            style: GetWindowLongPtrW(hwnd, GWL_STYLE) as u32,
            style_etendu: GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32,
            classe: String::from_utf16_lossy(&classe[..n]),
            possedee: GetWindow(hwnd, GW_OWNER).is_ok_and(|p| !p.is_invalid()),
            plein_ecran: plein_ecran_sur(hwnd, ecran),
            premier_plan: GetAncestor(GetForegroundWindow(), GA_ROOTOWNER) == hwnd,
            cadre,
        })
    }
}

/// Fenêtre masquée par DWM (`DWMWA_CLOAKED`).
fn masquee_par_dwm(hwnd: HWND) -> bool {
    let mut masquee = 0u32;
    let lu = unsafe { DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED, &mut masquee as *mut u32 as *mut c_void, 4) };
    lu.is_ok() && masquee != 0
}

/// `hwnd` est en plein écran sur l'écran `ecran`.
fn plein_ecran_sur(hwnd: HWND, ecran: HMONITOR) -> bool {
    unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) == ecran && covers_monitor(hwnd) }
}

/// La fenêtre `hwnd`, retenue en plein écran au dernier passage, couvre-t-elle encore l'écran
/// `ecran` (elle existe, visible, non réduite, non masquée) ?
fn couvre_encore(hwnd: HWND, ecran: HMONITOR) -> bool {
    let la = unsafe { IsWindow(Some(hwnd)).as_bool() && IsWindowVisible(hwnd).as_bool() && !IsIconic(hwnd).as_bool() };
    la && !masquee_par_dwm(hwnd) && plein_ecran_sur(hwnd, ecran)
}

/// Fenêtres de premier niveau, du dessus vers le dessous (ordre Z).
fn fenetres_ordre_z() -> Vec<HWND> {
    unsafe extern "system" fn chaque(hwnd: HWND, liste: LPARAM) -> windows::core::BOOL {
        unsafe { &mut *(liste.0 as *mut Vec<HWND>) }.push(hwnd);
        true.into()
    }
    let mut liste: Vec<HWND> = Vec::with_capacity(256);
    let _ = unsafe { EnumWindows(Some(chaque), LPARAM(&mut liste as *mut Vec<HWND> as isize)) };
    liste
}

/// Fenêtre en plein écran (jeu, vidéo F11…) sous le point qui met l'essaim en pause, ou `None`
/// (`plein_ecran_retenu`). C'est la fenêtre survolée qui compte, pas celle qui a le clavier : un
/// jeu en plein écran sur un écran et le pointeur sur l'autre, rien n'est retenu ; une vidéo en
/// plein écran survolée pendant qu'on tape ailleurs, si. `precedente` : la fenêtre rendue au
/// dernier appel (gardée tant qu'elle couvre l'écran du point et qu'aucune vraie fenêtre n'est
/// survolée). Une énumération des fenêtres par appel (la boucle l'appelle toutes les 250 ms).
pub fn plein_ecran_sous(x: i32, y: i32, precedente: Option<isize>) -> Option<isize> {
    let ecran = ecran_du_point(x, y);
    let precedente = precedente.map(|h| HWND(h as *mut c_void)).filter(|&h| couvre_encore(h, ecran));
    let fenetres = fenetres_ordre_z().into_iter().filter_map(|h| vue_fenetre(h, x, y, ecran).map(|v| (h, v)));
    plein_ecran_retenu(fenetres, x, y, precedente).map(|h| h.0 as isize)
}

/// Écran sous le point (le plus proche s'il tombe entre deux).
fn ecran_du_point(x: i32, y: i32) -> HMONITOR {
    unsafe { MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONEAREST) }
}

/// Identifiant (celui de `monitors()`) de l'écran sous le point.
pub fn ecran_sous(x: i32, y: i32) -> isize {
    ecran_du_point(x, y).0 as isize
}

/// Cadre affiché (gauche, haut, droite, bas, en pixels de l'écran) de la fenêtre au premier
/// plan, si c'est une fenêtre d'application qui pourrait aussi prendre la couleur de la bordure
/// (`bordure::eligible` : une vraie fenêtre, à cadre ou à barre de titre ; ni une barre ou un
/// menu sans cadre, comme celle d'Horizon et son menu du clic droit, ni le bureau, la barre des
/// tâches ou un calque d'Essaim), ni réduite, ni en plein écran. La fenêtre des réglages
/// d'Essaim en est une. Les particules rebondissent dessus, le halo et les équerres l'habillent.
pub fn cadre_premier_plan() -> Option<(f32, f32, f32, f32)> {
    unsafe {
        let fg = GetForegroundWindow();
        if fg.is_invalid() || IsIconic(fg).as_bool() {
            return None;
        }
        let mut class = [0u16; 64];
        let n = GetClassNameW(fg, &mut class) as usize;
        let class = String::from_utf16_lossy(&class[..n]);
        let style = GetWindowLongPtrW(fg, GWL_STYLE) as u32;
        let ex = GetWindowLongPtrW(fg, GWL_EXSTYLE) as u32;
        if !crate::bordure::eligible(&class, style, ex) || covers_monitor(fg) {
            return None;
        }
        let mut r = RECT::default();
        DwmGetWindowAttribute(fg, DWMWA_EXTENDED_FRAME_BOUNDS, &mut r as *mut RECT as *mut c_void, std::mem::size_of::<RECT>() as u32).ok()?;
        (r.right > r.left && r.bottom > r.top).then_some((r.left as f32, r.top as f32, r.right as f32, r.bottom as f32))
    }
}

/// Mode sans flashs voulu : selon le réglage, ou selon Windows (animations réduites).
pub fn sans_flashs(mode: crate::config::SansFlashs) -> bool {
    match mode {
        crate::config::SansFlashs::Oui => true,
        crate::config::SansFlashs::Non => false,
        crate::config::SansFlashs::Auto => !animations_windows(),
    }
}

/// Windows affiche-t-il les animations (Accessibilité › Effets visuels › Effets d'animation) ?
/// Non : l'utilisateur a demandé de les réduire.
pub fn animations_windows() -> bool {
    let mut oui = windows::core::BOOL(1);
    let lu = unsafe { SystemParametersInfoW(SPI_GETCLIENTAREAANIMATION, 0, Some(&mut oui as *mut _ as *mut c_void), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0)) };
    lu.is_err() || oui.as_bool()
}

/// La fenêtre est en plein écran sur son écran (jeu, vidéo F11, présentation…). Une fenêtre
/// simplement agrandie n'en est pas une, même quand la barre des tâches masquée la laisse couvrir
/// tout l'écran.
pub fn covers_monitor(hwnd: HWND) -> bool {
    unsafe {
        let mon = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        if !GetMonitorInfoW(mon, &mut mi).as_bool() {
            return false;
        }
        // cadre réellement affiché (sans la marge invisible de redimensionnement)
        let mut r = RECT::default();
        let cadre = DwmGetWindowAttribute(hwnd, DWMWA_EXTENDED_FRAME_BOUNDS, &mut r as *mut RECT as *mut c_void, std::mem::size_of::<RECT>() as u32);
        if cadre.is_err() && GetWindowRect(hwnd, &mut r).is_err() {
            return false;
        }
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
        plein_ecran(style, IsZoomed(hwnd).as_bool(), r, mi.rcMonitor)
    }
}

/// Calque d'affichage (comme ceux d'Essaim) : superposé et traversé par la souris. Il couvre
/// souvent tout un écran sans être une appli en plein écran.
pub fn calque(style_etendu: u32) -> bool {
    let calque = (WS_EX_LAYERED | WS_EX_TRANSPARENT).0;
    style_etendu & calque == calque
}

/// Une vraie fenêtre (visible, ni masquée par DWM, ni calque, ni bureau, ni d'Essaim) est-elle en
/// plein écran sur l'écran du point `pointeur` (sur l'un des écrans si `None` : position
/// illisible) ? Confirme le « occupé » (`QUNS_BUSY`) de Windows, qui prend aussi les calques
/// transparents de la taille d'un écran (une autre surcouche) pour une appli en plein écran, et
/// le dit pour tous les écrans : une vidéo en plein écran sur l'écran voisin ne met pas l'essaim
/// en pause.
pub fn plein_ecran_sur_ecran_du(pointeur: Option<(i32, i32)>) -> bool {
    unsafe fn fenetre_plein_ecran(hwnd: HWND) -> bool {
        unsafe {
            if !IsWindowVisible(hwnd).as_bool() || IsIconic(hwnd).as_bool() || calque(GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32) || masquee_par_dwm(hwnd) {
                return false;
            }
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            if pid == windows::Win32::System::Threading::GetCurrentProcessId() {
                return false;
            }
            let mut class = [0u16; 64];
            let n = GetClassNameW(hwnd, &mut class) as usize;
            if matches!(String::from_utf16_lossy(&class[..n]).as_str(), "Progman" | "WorkerW") {
                return false;
            }
            covers_monitor(hwnd)
        }
    }
    let ecran = pointeur.map(|(x, y)| unsafe { MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONEAREST) });
    let sur_l_ecran = |h: HWND| ecran.is_none_or(|e| unsafe { MonitorFromWindow(h, MONITOR_DEFAULTTONEAREST) } == e);
    fenetres_ordre_z().into_iter().any(|h| unsafe { fenetre_plein_ecran(h) } && sur_l_ecran(h))
}

/// Décision du plein écran : ni agrandie, ni barre de titre, ni bord de redimensionnement (les
/// jeux, la vidéo F11 des navigateurs et les diaporamas les retirent), et le cadre couvre l'écran.
pub fn plein_ecran(style: u32, agrandie: bool, cadre: RECT, ecran: RECT) -> bool {
    if agrandie || style & WS_CAPTION.0 == WS_CAPTION.0 || style & WS_THICKFRAME.0 != 0 {
        return false;
    }
    cadre.left <= ecran.left && cadre.top <= ecran.top && cadre.right >= ecran.right && cadre.bottom >= ecran.bottom
}

thread_local! {
    /// Minuterie haute résolution du fil (0 : pas encore créée ; -1 : indisponible).
    static MINUTERIE: Cell<isize> = const { Cell::new(0) };
}

/// Attend `d` au plus, en se réveillant dès qu'un message arrive pour le fil (menu, raccourci,
/// fermeture) : minuterie haute résolution (précise à la milliseconde, sans réveil inutile) et
/// `MsgWaitForMultipleObjectsEx`. Le message n'est pas traité ici (`pump` s'en charge).
pub fn attendre(d: Duration) {
    unsafe {
        let mut h = MINUTERIE.with(Cell::get);
        if h == 0 {
            h = CreateWaitableTimerExW(None, PCWSTR::null(), CREATE_WAITABLE_TIMER_HIGH_RESOLUTION, TIMER_ALL_ACCESS.0)
                .or_else(|_| CreateWaitableTimerExW(None, PCWSTR::null(), 0, TIMER_ALL_ACCESS.0))
                .map_or(-1, |t| t.0 as isize);
            MINUTERIE.with(|m| m.set(h));
        }
        let ms = d.as_millis().min(10_000) as u32;
        let due = -((d.as_nanos() / 100).min(100_000_000) as i64).max(1);
        if h != -1 {
            let t = HANDLE(h as *mut c_void);
            if SetWaitableTimer(t, &due, 0, None, None, false).is_ok() {
                // délai de secours un peu plus long que la minuterie
                MsgWaitForMultipleObjectsEx(Some(&[t]), ms + 5, QS_ALLINPUT, MWMO_INPUTAVAILABLE);
                return;
            }
        }
        MsgWaitForMultipleObjectsEx(None, ms, QS_ALLINPUT, MWMO_INPUTAVAILABLE);
    }
}

// ---------------------------------------------------------------------------------------------
// Icône de notification

pub const CMD_PAUSE: usize = 1;
pub const CMD_CONFIG: usize = 2;
pub const CMD_QUIT: usize = 3;
pub const CMD_SUIVANT: usize = 4;
pub const CMD_PRECEDENT: usize = 5;
/// Relance Essaim pour passer à la version téléchargée (`maj`).
pub const CMD_REDEMARRER: usize = 6;
/// Projecteur « retrouver la souris » (Ctrl+Alt+F).
pub const CMD_PROJECTEUR: usize = 7;
/// Fenêtre pour le stream (`stream.rs`).
pub const CMD_STREAM: usize = 8;
/// Presets du menu : `CMD_PRESET + rang` dans le dernier menu construit (`preset_du_menu`).
pub const CMD_PRESET: usize = 100;
const WM_TRAY: u32 = WM_APP + 1;
pub const TRAY_CLASS: PCWSTR = w!("EssaimTray");

/// Commandes reçues (menu, raccourcis, icône, fermeture) en attente de la boucle d'affichage,
/// dans l'ordre d'arrivée. Bornée : une rafale anormale ne remplit pas la mémoire, mais
/// « Quitter » passe toujours.
#[derive(Default)]
struct FileCommandes(std::collections::VecDeque<usize>);

impl FileCommandes {
    const MAX: usize = 32;

    fn pousser(&mut self, cmd: usize) {
        if cmd == 0 {
            return;
        }
        if cmd == CMD_QUIT {
            // rien après « Quitter » ne compte : il passe devant tout le reste
            self.0.clear();
        } else if self.0.len() >= Self::MAX {
            return;
        }
        self.0.push_back(cmd);
    }

    fn prendre(&mut self) -> usize {
        self.0.pop_front().unwrap_or(0)
    }
}

thread_local! {
    static COMMANDES: RefCell<FileCommandes> = RefCell::new(FileCommandes::default());
    static PAUSED: Cell<bool> = const { Cell::new(false) };
    static TASKBAR_CREATED: Cell<u32> = const { Cell::new(0) };
    static ICON: Cell<HICON> = const { Cell::new(HICON(std::ptr::null_mut())) };
    /// Variante grisée de l'icône, montrée en pause.
    static ICON_PAUSE: Cell<HICON> = const { Cell::new(HICON(std::ptr::null_mut())) };
    /// Infobulle de l'icône.
    static TIP: RefCell<String> = const { RefCell::new(String::new()) };
    static DISPLAY_CHANGED: Cell<bool> = const { Cell::new(false) };
    /// Alimentation changée (secteur, batterie, économiseur) : `WM_POWERBROADCAST`.
    static POWER_CHANGED: Cell<bool> = const { Cell::new(false) };
    /// Noms des presets du dernier menu construit, dans l'ordre des entrées.
    static MENU_PRESETS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

/// Preset de l'entrée `rang` du dernier menu de l'icône (le nom vu par l'utilisateur, même si la
/// liste a changé depuis).
pub fn preset_du_menu(rang: usize) -> Option<String> {
    MENU_PRESETS.with(|m| m.borrow().get(rang).cloned())
}

// ---------------------------------------------------------------------------------------------
// Session Windows : verrouillée, console déconnectée ou ouverte à distance, l'essaim se retire
// (comme en plein écran). Notifications `WM_WTSSESSION_CHANGE` de la fenêtre de l'icône.

#[link(name = "wtsapi32", kind = "raw-dylib")]
extern "system" {
    fn WTSRegisterSessionNotification(hwnd: HWND, flags: u32) -> windows::core::BOOL;
    fn WTSUnRegisterSessionNotification(hwnd: HWND) -> windows::core::BOOL;
}

const NOTIFY_FOR_THIS_SESSION: u32 = 0;
const WM_SESSION: u32 = 0x02B1; // WM_WTSSESSION_CHANGE
const WTS_CONSOLE_CONNECT: usize = 1;
const WTS_CONSOLE_DISCONNECT: usize = 2;
const WTS_SESSION_LOCK: usize = 7;
const WTS_SESSION_UNLOCK: usize = 8;

static VERROUILLEE: AtomicBool = AtomicBool::new(false);
static CONSOLE_DECONNECTEE: AtomicBool = AtomicBool::new(false);
static A_DISTANCE: AtomicBool = AtomicBool::new(false);

fn relire_distance() {
    A_DISTANCE.store(unsafe { GetSystemMetrics(SM_REMOTESESSION) } != 0, Ordering::Relaxed);
}

/// Changement de session signalé par Windows (`wParam` de `WM_WTSSESSION_CHANGE`).
fn session_changee(evenement: usize) {
    match evenement {
        WTS_SESSION_LOCK => VERROUILLEE.store(true, Ordering::Relaxed),
        // déverrouillée : l'utilisateur est revenu, à la console ou à distance
        WTS_SESSION_UNLOCK => {
            VERROUILLEE.store(false, Ordering::Relaxed);
            CONSOLE_DECONNECTEE.store(false, Ordering::Relaxed);
        }
        WTS_CONSOLE_DISCONNECT => CONSOLE_DECONNECTEE.store(true, Ordering::Relaxed),
        WTS_CONSOLE_CONNECT => CONSOLE_DECONNECTEE.store(false, Ordering::Relaxed),
        _ => {}
    }
    // connexion ou déconnexion à distance, et tout autre changement : relu
    relire_distance();
}

/// Raison de retirer l'essaim tenant à la session (`pauses.rs`) : verrouillée, console
/// déconnectée ou ouverte à distance (bureau à distance : l'écran est envoyé par le réseau).
pub fn session_retenue() -> Option<&'static str> {
    if VERROUILLEE.load(Ordering::Relaxed) {
        Some("Session verrouillée")
    } else if A_DISTANCE.load(Ordering::Relaxed) {
        Some("Session à distance")
    } else if CONSOLE_DECONNECTEE.load(Ordering::Relaxed) {
        Some("Session déconnectée")
    } else {
        None
    }
}

// ---------------------------------------------------------------------------------------------
// Écran éteint (session ouverte : délai d'extinction de Windows, bouton de l'écran sur un
// portable) : `GUID_CONSOLE_DISPLAY_STATE`, notifié à la fenêtre de l'icône
// (`RegisterPowerSettingNotification`, `WM_POWERBROADCAST` + `PBT_POWERSETTINGCHANGE`). Rien n'est
// composé : `DwmFlush` rendrait la main tout de suite et la boucle tournerait à vide.

/// `GUID_CONSOLE_DISPLAY_STATE` (6FE69556-704A-47A0-8F24-C28D936FDA47) : 0 éteint, 1 allumé,
/// 2 atténué.
const GUID_ETAT_ECRAN: windows::core::GUID = windows::core::GUID::from_u128(0x6fe69556_704a_47a0_8f24_c28d936fda47);

static ECRAN_ETEINT: AtomicBool = AtomicBool::new(false);

/// Nouvel état de l'écran (`Data` de `POWERBROADCAST_SETTING` pour `GUID_ETAT_ECRAN`) : seul 0
/// (éteint) retire l'essaim ; atténué (2), il reste visible.
fn etat_ecran(valeur: u32) {
    let eteint = valeur == 0;
    if ECRAN_ETEINT.swap(eteint, Ordering::Relaxed) != eteint {
        log::info!("{}", if eteint { "écran éteint" } else { "écran rallumé" });
    }
}

/// L'écran est éteint, session ouverte (`pauses.rs` : pause automatique « Écran éteint »).
pub fn ecran_eteint() -> bool {
    ECRAN_ETEINT.load(Ordering::Relaxed)
}

/// Raccourcis clavier enregistrés (le menu les affiche alors à côté des entrées).
static RACCOURCIS: AtomicBool = AtomicBool::new(false);

/// Fenêtre de l'icône : partagée, pour que le fil des mises à jour puisse afficher sa bulle.
static TRAY_HWND: AtomicIsize = AtomicIsize::new(0);
/// La dernière bulle annonce une mise à jour prête : un clic dessus relance Essaim.
static BULLE_MAJ: AtomicBool = AtomicBool::new(false);

fn tray_hwnd() -> HWND {
    HWND(TRAY_HWND.load(Ordering::Relaxed) as *mut _)
}

/// Raccourci non enregistré au dernier passage, et pourquoi.
#[derive(Clone, Debug, PartialEq)]
pub struct Refus {
    /// « Ctrl+Alt+F ».
    pub raccourci: &'static str,
    pub raison: &'static str,
}

/// Raison d'un raccourci que Windows a refusé.
pub const PRIS_AILLEURS: &str = "pris par une autre application";
/// Raison d'un raccourci écarté parce que Ctrl+Alt+touche (AltGr+touche) écrit un caractère.
pub const ECRIT_UN_CARACTERE: &str = "AltGr y écrit un caractère sur un des claviers installés";

/// Raccourcis non enregistrés au dernier passage ; dispositions de clavier installées alors.
static REFUSES: std::sync::Mutex<Vec<Refus>> = std::sync::Mutex::new(Vec::new());
static DISPOSITIONS: std::sync::Mutex<Vec<isize>> = std::sync::Mutex::new(Vec::new());

/// Raccourcis globaux : identifiant, touche virtuelle, libellé.
const RACCOURCIS_GLOBAUX: [(i32, u32, &str); 4] = [(1, 0x27, "Ctrl+Alt+→"), (2, 0x25, "Ctrl+Alt+←"), (3, b'P' as u32, "Ctrl+Alt+P"), (4, b'F' as u32, "Ctrl+Alt+F")];

/// Caractère que Ctrl+Alt+touche écrit sur l'une des dispositions de clavier, d'après ce que
/// chacune produit (`(disposition, caractère)`, lu par `ToUnicodeEx`) ; `None` si aucune n'écrit
/// rien d'imprimable. AltGr vaut Ctrl+Alt pour Windows : sur un clavier tchèque, AltGr+F écrit
/// « [ », et un raccourci Ctrl+Alt+F volerait ce caractère dans toutes les applications. Un
/// caractère de contrôle (Ctrl+lettre sur certaines dispositions) n'écrit rien.
pub fn caractere_vole(produits: &[(isize, Option<char>)]) -> Option<char> {
    produits.iter().filter_map(|p| p.1).find(|c| !c.is_control())
}

/// Dispositions de clavier installées pour la session (`GetKeyboardLayoutList`).
fn dispositions() -> Vec<isize> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyboardLayoutList, HKL};
    unsafe {
        let n = GetKeyboardLayoutList(None);
        if n <= 0 {
            return Vec::new();
        }
        let mut liste = vec![HKL::default(); n as usize];
        let n = GetKeyboardLayoutList(Some(&mut liste)).max(0) as usize;
        liste.truncate(n);
        // triée : Windows peut rendre la même liste dans un autre ordre (disposition active en
        // tête), ce qui n'est pas un changement
        let mut liste: Vec<isize> = liste.into_iter().map(|h| h.0 as isize).collect();
        liste.sort_unstable();
        liste.dedup();
        liste
    }
}

/// Ce que Ctrl+Alt+`vk` écrit sur la disposition `hkl` (une touche morte compte : elle compose
/// le caractère suivant). Drapeau 4 : la lecture ne touche pas à l'état des touches mortes.
fn produit_ctrl_alt(hkl: isize, vk: u32) -> Option<char> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{MapVirtualKeyExW, ToUnicodeEx, HKL, MAPVK_VK_TO_VSC, VK_CONTROL, VK_LCONTROL, VK_MENU, VK_RMENU};
    let hkl = HKL(hkl as *mut c_void);
    let mut etat = [0u8; 256];
    for v in [VK_CONTROL, VK_MENU, VK_LCONTROL, VK_RMENU] {
        etat[v.0 as usize] = 0x80;
    }
    let mut buf = [0u16; 8];
    unsafe {
        let scan = MapVirtualKeyExW(vk, MAPVK_VK_TO_VSC, Some(hkl));
        let n = ToUnicodeEx(vk, scan, &etat, &mut buf, 0x4, Some(hkl));
        if n == 0 {
            return None;
        }
        char::decode_utf16(buf.iter().copied()).next().and_then(Result::ok)
    }
}

/// Raccourcis globaux (RegisterHotKey, sans hook) : Ctrl+Alt+→ / ← preset suivant / précédent,
/// Ctrl+Alt+P pause, Ctrl+Alt+F projecteur. Les enregistre ou les retire. Un raccourci dont la
/// touche, avec AltGr, écrit un caractère sur l'une des dispositions installées n'est pas
/// enregistré (`caractere_vole`).
pub fn raccourcis(actif: bool) {
    use windows::Win32::UI::Input::KeyboardAndMouse::{RegisterHotKey, UnregisterHotKey, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT};
    let hwnd = tray_hwnd();
    RACCOURCIS.store(actif, Ordering::Relaxed);
    let disp = dispositions();
    let mut refuses = Vec::new();
    unsafe {
        for (id, vk, libelle) in RACCOURCIS_GLOBAUX {
            let _ = UnregisterHotKey(Some(hwnd), id);
            if !actif {
                continue;
            }
            let produits: Vec<(isize, Option<char>)> = disp.iter().map(|&h| (h, produit_ctrl_alt(h, vk))).collect();
            if let Some(c) = caractere_vole(&produits) {
                log::warn!("raccourci {libelle} non enregistré : il écrit « {c} » sur une des dispositions de clavier");
                refuses.push(Refus { raccourci: libelle, raison: ECRIT_UN_CARACTERE });
            } else if RegisterHotKey(Some(hwnd), id, MOD_CONTROL | MOD_ALT | MOD_NOREPEAT, vk).is_err() {
                log::warn!("raccourci {libelle} déjà pris par une autre application");
                refuses.push(Refus { raccourci: libelle, raison: PRIS_AILLEURS });
            }
        }
    }
    *DISPOSITIONS.lock().unwrap_or_else(|e| e.into_inner()) = disp;
    *REFUSES.lock().unwrap_or_else(|e| e.into_inner()) = refuses;
}

/// Relit les dispositions de clavier installées (à appeler toutes les quelques secondes :
/// `WM_INPUTLANGCHANGE` ne vient qu'à la fenêtre active) ; si elles ont changé, les raccourcis
/// sont réévalués.
pub fn raccourcis_suivent_clavier() {
    if !RACCOURCIS.load(Ordering::Relaxed) {
        return;
    }
    let disp = dispositions();
    if *DISPOSITIONS.lock().unwrap_or_else(|e| e.into_inner()) != disp {
        log::info!("dispositions de clavier changées : raccourcis réévalués");
        raccourcis(true);
    }
}

/// Raccourcis globaux non enregistrés au dernier passage, avec leur raison ; vide quand tous
/// sont pris par Essaim ou que les raccourcis sont coupés.
pub fn raccourcis_refuses() -> Vec<Refus> {
    REFUSES.lock().map(|r| r.clone()).unwrap_or_default()
}

/// Petite notification de l'icône (changement de preset au clavier, téléchargement).
/// Utilisable depuis n'importe quel fil.
pub fn bulle(texte: &str) {
    BULLE_MAJ.store(false, Ordering::Relaxed);
    afficher_bulle(texte);
}

/// Bulle d'une mise à jour prête : un clic dessus relance Essaim.
pub fn bulle_maj(texte: &str) {
    BULLE_MAJ.store(true, Ordering::Relaxed);
    afficher_bulle(texte);
}

fn afficher_bulle(texte: &str) {
    let hwnd = tray_hwnd();
    if hwnd.0.is_null() {
        return;
    }
    let mut n = nid(hwnd);
    n.uFlags = windows::Win32::UI::Shell::NIF_INFO | windows::Win32::UI::Shell::NIF_REALTIME;
    for (d, s) in n.szInfo.iter_mut().zip(texte.encode_utf16().take(250)) {
        *d = s;
    }
    for (d, s) in n.szInfoTitle.iter_mut().zip("Essaim".encode_utf16()) {
        *d = s;
    }
    n.dwInfoFlags = windows::Win32::UI::Shell::NIIF_NOSOUND;
    unsafe {
        let _ = Shell_NotifyIconW(windows::Win32::UI::Shell::NIM_MODIFY, &n);
    }
}

/// Vrai une fois après un changement de la configuration des écrans (résolution, échelle,
/// écran branché ou débranché).
pub fn take_display_changed() -> bool {
    let changed = DISPLAY_CHANGED.with(|d| d.replace(false));
    if changed {
        ECHELLES.with(|e| *e.borrow_mut() = (Vec::new(), None));
        FREQUENCE.set(None);
    }
    changed
}

/// Vrai une fois après un changement d'alimentation signalé par Windows (fil principal).
pub fn take_power_changed() -> bool {
    POWER_CHANGED.with(|p| p.replace(false))
}

/// Plus ancienne commande en attente (menu, raccourci, icône, fermeture), retirée de la file ;
/// 0 quand il n'y en a plus. Plusieurs peuvent arriver dans un même passage de la file de
/// messages : la boucle les vide toutes (`while let c @ 1.. = take_command()`).
pub fn take_command() -> usize {
    COMMANDES.with(|f| f.borrow_mut().prendre())
}

fn commande(cmd: usize) {
    COMMANDES.with(|f| f.borrow_mut().pousser(cmd));
}

/// Infobulle de l'icône : « Essaim — preset « X » · en pause / en veille ».
pub fn infobulle(preset: Option<&str>, pause: bool, veille: bool) -> String {
    let mut t = String::from("Essaim");
    if let Some(p) = preset {
        t += &format!(" — preset « {p} »");
    }
    if pause {
        t += " · en pause";
    } else if veille {
        t += " · en veille";
    }
    t
}

/// État montré par l'icône (fil principal) : grisée en pause, entrée du menu « Reprendre »,
/// infobulle `tip`.
pub fn etat_icone(paused: bool, tip: &str) {
    PAUSED.with(|p| p.set(paused));
    TIP.with(|t| *t.borrow_mut() = tip.to_owned());
    let hwnd = tray_hwnd();
    if !hwnd.0.is_null() {
        unsafe {
            let _ = Shell_NotifyIconW(windows::Win32::UI::Shell::NIM_MODIFY, &nid(hwnd));
        }
    }
}

fn nid(hwnd: HWND) -> NOTIFYICONDATAW {
    let paused = PAUSED.with(Cell::get);
    let icon = if paused { ICON_PAUSE.with(Cell::get) } else { HICON::default() };
    let mut n = NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: 1,
        uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
        uCallbackMessage: WM_TRAY,
        hIcon: if icon.is_invalid() { ICON.with(Cell::get) } else { icon },
        ..Default::default()
    };
    let tip = TIP.with(|t| t.borrow().clone());
    let tip = if tip.is_empty() { "Essaim".to_owned() } else { tip };
    // 127 caractères au plus, plus le zéro final
    for (d, s) in n.szTip.iter_mut().zip(tip.encode_utf16().take(127)) {
        *d = s;
    }
    n
}

/// Texte d'une entrée de menu, avec son raccourci si les raccourcis sont actifs et que celui-ci
/// a été enregistré.
fn entree(texte: &str, raccourci: &str) -> Vec<u16> {
    let pris = RACCOURCIS.load(Ordering::Relaxed) && !raccourcis_refuses().iter().any(|r| r.raccourci == raccourci);
    let t = if pris { format!("{texte}\t{raccourci}") } else { texte.to_owned() };
    t.encode_utf16().chain(Some(0)).collect()
}

/// Menu de l'icône (clic droit).
unsafe fn menu(hwnd: HWND) {
    let Ok(menu) = (unsafe { CreatePopupMenu() }) else { return };
    let paused = PAUSED.with(Cell::get);
    let pause = entree(if paused { "Reprendre" } else { "Mettre en pause" }, "Ctrl+Alt+P");
    let suivant = entree("Preset suivant", "Ctrl+Alt+→");
    let precedent = entree("Preset précédent", "Ctrl+Alt+←");
    unsafe {
        let _ = AppendMenuW(menu, MF_STRING, CMD_PAUSE, PCWSTR(pause.as_ptr()));
        let _ = AppendMenuW(menu, MF_STRING, CMD_CONFIG, w!("Réglages…"));
        let _ = AppendMenuW(menu, MF_STRING, CMD_STREAM, w!("Fenêtre pour le stream"));
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);
        // presets : celui en place coché
        if let Ok(sous) = CreatePopupMenu() {
            let cfg = crate::config::Config::read(&crate::config::path()).unwrap_or_default();
            let mut noms = Vec::new();
            for (i, p) in crate::profils::list().into_iter().enumerate().take(80) {
                let nom: Vec<u16> = p.name.encode_utf16().chain(Some(0)).collect();
                let coche = if crate::profils::same(&p.cfg, &cfg) { MF_CHECKED } else { MF_UNCHECKED };
                let _ = AppendMenuW(sous, MF_STRING | coche, CMD_PRESET + i, PCWSTR(nom.as_ptr()));
                noms.push(p.name);
            }
            // la liste peut changer avant le choix : l'entrée choisie désigne ce nom-là
            MENU_PRESETS.with(|m| *m.borrow_mut() = noms);
            let _ = AppendMenuW(menu, MF_POPUP, sous.0 as usize, w!("Presets"));
        }
        let _ = AppendMenuW(menu, MF_STRING, CMD_SUIVANT, PCWSTR(suivant.as_ptr()));
        let _ = AppendMenuW(menu, MF_STRING, CMD_PRECEDENT, PCWSTR(precedent.as_ptr()));
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);
        if let Some(version) = crate::maj::prete() {
            let texte: Vec<u16> = format!("Redémarrer avec la version {version}").encode_utf16().chain(Some(0)).collect();
            let _ = AppendMenuW(menu, MF_STRING, CMD_REDEMARRER, PCWSTR(texte.as_ptr()));
        }
        let _ = AppendMenuW(menu, MF_STRING, CMD_QUIT, w!("Quitter"));
        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        let _ = SetForegroundWindow(hwnd);
        let _ = TrackPopupMenu(menu, TPM_RIGHTBUTTON, pt.x, pt.y, None, hwnd, None);
        // le menu se ferme bien au clic ailleurs (recommandation de Windows)
        let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
        let _ = DestroyMenu(menu);
    }
}

extern "system" fn tray_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    sans_panique("icône de notification", || unsafe { tray_message(hwnd, msg, wp, lp) }).unwrap_or_else(|| {
        // « Fermer » n'est pas laissé à Windows, qui détruirait la fenêtre de l'icône : la
        // fermeture passe par la boucle principale
        if msg == WM_CLOSE {
            commande(CMD_QUIT);
            LRESULT(0)
        } else {
            unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
        }
    })
}

unsafe fn tray_message(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        if msg == WM_TRAY {
            let ev = (lp.0 & 0xFFFF) as u32;
            if ev == windows::Win32::UI::Shell::NIN_BALLOONUSERCLICK && BULLE_MAJ.load(Ordering::Relaxed) {
                commande(CMD_REDEMARRER);
                return LRESULT(0);
            }
            // clic gauche (ou double-clic) : réglages ; clic droit : menu
            if ev == WM_LBUTTONUP || ev == WM_LBUTTONDBLCLK {
                commande(CMD_CONFIG);
            } else if ev == WM_RBUTTONUP || ev == WM_CONTEXTMENU {
                menu(hwnd);
            }
            return LRESULT(0);
        }
        if msg == WM_COMMAND {
            commande(wp.0 & 0xFFFF);
            return LRESULT(0);
        }
        if msg == WM_HOTKEY {
            let cmd = match wp.0 {
                1 => CMD_SUIVANT,
                2 => CMD_PRECEDENT,
                4 => CMD_PROJECTEUR,
                _ => CMD_PAUSE,
            };
            commande(cmd);
            return LRESULT(0);
        }
        // La zone de travail (`SPI_SETWORKAREA` : barre des tâches redimensionnée ou masquée,
        // barre d'appli, Explorateur relancé) ne compte pas : les calques couvrent tout l'écran.
        if msg == WM_DISPLAYCHANGE || msg == WM_DPICHANGED {
            DISPLAY_CHANGED.with(|d| d.set(true));
        }
        if msg == WM_POWERBROADCAST && wp.0 == PBT_APMPOWERSTATUSCHANGE as usize {
            POWER_CHANGED.with(|p| p.set(true));
        }
        if msg == WM_POWERBROADCAST && wp.0 == PBT_POWERSETTINGCHANGE as usize && lp.0 != 0 {
            use windows::Win32::System::Power::POWERBROADCAST_SETTING;
            let s = lp.0 as *const POWERBROADCAST_SETTING;
            // `Data` : un DWORD pour cet état, pas forcément aligné
            if (*s).PowerSetting == GUID_ETAT_ECRAN && (*s).DataLength >= 4 {
                etat_ecran(std::ptr::read_unaligned(std::ptr::addr_of!((*s).Data) as *const u32));
            }
            return LRESULT(1);
        }
        // sortie de veille : écrans et alimentation ont pu changer pendant le sommeil
        if msg == WM_POWERBROADCAST && wp.0 == PBT_APMRESUMEAUTOMATIC as usize {
            DISPLAY_CHANGED.with(|d| d.set(true));
            POWER_CHANGED.with(|p| p.set(true));
        }
        if msg == WM_SESSION {
            session_changee(wp.0);
            return LRESULT(0);
        }
        if msg == WM_ENDSESSION {
            // la session se ferme vraiment (wParam ≠ 0) : le processus peut être arrêté à tout
            // moment après ce message, la bordure est rendue tout de suite
            if wp.0 != 0 {
                crate::bordure::restore();
                commande(CMD_QUIT);
            }
            return LRESULT(0);
        }
        if msg == WM_CLOSE {
            commande(CMD_QUIT);
            return LRESULT(0);
        }
        if msg != 0 && msg == TASKBAR_CREATED.with(|t| t.get()) {
            let _ = Shell_NotifyIconW(NIM_ADD, &nid(hwnd)); // l'Explorateur a redémarré
            return LRESULT(0);
        }
        DefWindowProcW(hwnd, msg, wp, lp)
    }
}

pub struct Tray {
    hwnd: HWND,
    /// Inscription aux changements d'état de l'écran (`GUID_ETAT_ECRAN`).
    ecran: Option<windows::Win32::System::Power::HPOWERNOTIFY>,
}

impl Tray {
    pub fn new() -> windows::core::Result<Self> {
        unsafe {
            let hinst = GetModuleHandleW(None)?;
            let wc = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                lpfnWndProc: Some(tray_proc),
                hInstance: hinst.into(),
                lpszClassName: TRAY_CLASS,
                ..Default::default()
            };
            RegisterClassExW(&wc);
            let hwnd = CreateWindowExW(WINDOW_EX_STYLE(0), TRAY_CLASS, w!("Essaim"), WS_POPUP, 0, 0, 0, 0, None, None, Some(hinst.into()), None)?;
            TASKBAR_CREATED.with(|t| t.set(RegisterWindowMessageW(w!("TaskbarCreated"))));
            TRAY_HWND.store(hwnd.0 as isize, Ordering::Relaxed);
            ICON.with(|i| i.set(make_icon(false).unwrap_or_else(|| LoadIconW(None, IDI_APPLICATION).unwrap_or_default())));
            ICON_PAUSE.with(|i| i.set(make_icon(true).unwrap_or_default()));
            if !Shell_NotifyIconW(NIM_ADD, &nid(hwnd)).as_bool() {
                log::warn!("icône de notification : ajout impossible");
            }
            relire_distance();
            if !WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION).as_bool() {
                log::warn!("session : notifications de verrouillage indisponibles");
            }
            // l'état courant arrive aussitôt après l'inscription
            use windows::Win32::System::Power::RegisterPowerSettingNotification;
            let ecran = RegisterPowerSettingNotification(HANDLE(hwnd.0), &GUID_ETAT_ECRAN, DEVICE_NOTIFY_WINDOW_HANDLE)
                .inspect_err(|e| log::warn!("écran : notifications d'extinction indisponibles ({e})"))
                .ok();
            Ok(Self { hwnd, ecran })
        }
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
        unsafe {
            TRAY_HWND.store(0, Ordering::Relaxed);
            if let Some(e) = self.ecran.take() {
                let _ = windows::Win32::System::Power::UnregisterPowerSettingNotification(e);
            }
            let _ = WTSUnRegisterSessionNotification(self.hwnd);
            let _ = Shell_NotifyIconW(NIM_DELETE, &nid(self.hwnd));
            let _ = DestroyWindow(self.hwnd);
        }
    }
}

/// Icône de l'appli pour une fenêtre (barre des tâches, Alt+Tab), à la taille voulue par Windows
/// (`petite` : celle de la barre de titre) : celle de l'exécutable (`ressources/essaim.ico`,
/// intégrée par `build.rs`), sinon dessinée à la volée. Chargée une fois par taille et gardée
/// jusqu'à la fin (chaque ouverture des réglages en créait une nouvelle, jamais détruite) ; à ne
/// pas détruire.
pub fn app_icon(petite: bool) -> Option<HICON> {
    /// Icônes déjà chargées : (côté, poignée).
    static ICONES: std::sync::Mutex<Vec<((i32, i32), isize)>> = std::sync::Mutex::new(Vec::new());
    unsafe {
        let (cx, cy) = if petite { (SM_CXSMICON, SM_CYSMICON) } else { (SM_CXICON, SM_CYICON) };
        let taille = (GetSystemMetrics(cx), GetSystemMetrics(cy));
        let mut icones = ICONES.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(&(_, h)) = icones.iter().find(|(t, _)| *t == taille) {
            return Some(HICON(h as *mut c_void));
        }
        let hinst = GetModuleHandleW(None).ok()?;
        let h = LoadImageW(Some(hinst.into()), PCWSTR(std::ptr::without_provenance(1)), IMAGE_ICON, taille.0, taille.1, LR_DEFAULTCOLOR);
        let icone = h.ok().map(|h| HICON(h.0)).or_else(|| make_icon(false))?;
        icones.push((taille, icone.0 as isize));
        Some(icone)
    }
}

/// Couleur de l'icône : le rouge `signal` de la charte (thème sombre), lisible sur une barre des
/// tâches sombre comme claire.
const ICONE_ROUGE: (u8, u8, u8) = (0xff, 0x3b, 0x2f);
/// Couleur de l'icône en pause : un gris moyen, lisible lui aussi sur les deux thèmes.
const ICONE_GRIS: (u8, u8, u8) = (0x8a, 0x89, 0x84);

/// Géométrie du motif pour une icône de `cote` pixels : rayon de la comète, centre de son
/// cercle (x, y) et rayon de l'orbe, en pixels. Jusqu'à 40 px chaque taille a la sienne : la
/// marge est plus étroite que dans la référence (l'icône garde sa présence à côté des autres) et
/// les bords tombent sur un pixel entier, ce qu'une simple réduction ne donnerait pas. Au-delà,
/// c'est la référence sur 48 unités (`docs/images/logo.svg`).
fn geometrie_icone(cote: u32) -> (f32, f32, f32, f32) {
    match cote {
        16 => (7.0, 6.5, 8.0, 2.5),
        20 => (8.0, 8.0, 10.0, 3.0),
        24 => (11.0, 9.5, 12.0, 3.5),
        32 => (14.0, 12.0, 16.0, 5.0),
        40 => (18.0, 16.0, 20.0, 6.0),
        _ => {
            let k = cote as f32 / 48.0;
            (18.0 * k, 21.0 * k, 24.0 * k, 6.0 * k)
        }
    }
}

/// Le motif de l'icône d'Essaim dans un carré de `cote` pixels : une comète en virgule (tête
/// ronde, queue qui s'effile en tournant) enroulée autour d'un orbe, d'une seule couleur ;
/// `gris` : variante éteinte (pause). Sert à l'icône de notification et au fichier
/// `ressources/essaim.ico` (essai `exporter_icone`).
pub fn dessin_icone(cote: u32, gris: bool) -> Option<Pixmap> {
    use tiny_skia::{BlendMode, Color, FillRule, Paint, PathBuilder, Rect, Transform};
    fn disque(pm: &mut Pixmap, x: f32, y: f32, rayon: f32, paint: &Paint) {
        if let Some(path) = PathBuilder::from_circle(x, y, rayon) {
            pm.fill_path(&path, paint, FillRule::Winding, Transform::identity(), None);
        }
    }
    let (rayon, cx, cy, orbe) = geometrie_icone(cote);
    let c = if gris { ICONE_GRIS } else { ICONE_ROUGE };
    let mut encre = Paint::default();
    encre.set_color(Color::from_rgba8(c.0, c.1, c.2, 255));
    encre.anti_alias = true;
    let gomme = Paint { blend_mode: BlendMode::Clear, anti_alias: true, ..Paint::default() };
    let mut pm = Pixmap::new(cote, cote)?;
    // La virgule : la moitié droite d'un disque, plus la tête (un disque de rayon moitié, en haut
    // sur l'axe), moins le creux (le même disque, en bas). L'orbe se loge dans le creux.
    disque(&mut pm, cx, cy, rayon, &encre);
    pm.fill_rect(Rect::from_ltrb(0.0, 0.0, cx, cote as f32)?, &gomme, Transform::identity(), None);
    disque(&mut pm, cx, cy - rayon / 2.0, rayon / 2.0, &encre);
    disque(&mut pm, cx, cy + rayon / 2.0, rayon / 2.0, &gomme);
    disque(&mut pm, cx, cy + rayon / 2.0, orbe, &encre);
    Some(pm)
}

/// Icône dessinée à la volée (`dessin_icone`) à la taille d'une petite icône de l'écran (16 px à
/// 100 %, 20 à 125 %…) : Windows n'a pas à réduire une image plus grande ; `gris` : variante
/// éteinte (pause).
fn make_icon(gris: bool) -> Option<HICON> {
    let cote = unsafe { GetSystemMetrics(SM_CXSMICON) }.max(16);
    let pm = dessin_icone(cote as u32, gris)?;
    unsafe {
        let dc = CreateCompatibleDC(None);
        let bi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: cote,
                biHeight: -cote,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits: *mut c_void = std::ptr::null_mut();
        let color = CreateDIBSection(Some(dc), &bi, DIB_RGB_COLORS, &mut bits, None, 0).ok()?;
        let dst = std::slice::from_raw_parts_mut(bits as *mut u8, (cote * cote * 4) as usize);
        for (d, s) in dst.as_chunks_mut::<4>().0.iter_mut().zip(pm.data().as_chunks::<4>().0.iter()) {
            let a = s[3] as u32;
            let un = |v: u8| (v as u32 * 255 + a / 2).checked_div(a).map_or(0, |x| x.min(255) as u8);
            d[0] = un(s[2]);
            d[1] = un(s[1]);
            d[2] = un(s[0]);
            d[3] = s[3];
        }
        // masque d'un bit par pixel, tout à zéro (la transparence vient du canal alpha) : ses
        // lignes sont alignées sur 16 bits
        let masque = vec![0u8; (cote as usize).div_ceil(16) * 2 * cote as usize];
        let mask = CreateBitmap(cote, cote, 1, 1, Some(masque.as_ptr() as *const c_void));
        let info = ICONINFO { fIcon: true.into(), hbmMask: mask, hbmColor: color, ..Default::default() };
        let icon = CreateIconIndirect(&info).ok();
        let _ = DeleteObject(color.into());
        let _ = DeleteObject(mask.into());
        let _ = DeleteDC(dc);
        icon
    }
}

/// Ouvre un fichier avec l'application associée (le Bloc-notes pour un .toml en général).
pub fn open_file(path: &std::path::Path) {
    let wide: Vec<u16> = path.as_os_str().to_string_lossy().encode_utf16().chain(Some(0)).collect();
    unsafe {
        let r = ShellExecuteW(None, w!("open"), PCWSTR(wide.as_ptr()), None, None, SW_SHOWNORMAL);
        if r.0 as isize <= 32 {
            // pas d'appli associée aux .toml : Bloc-notes (chemin entre guillemets : il peut
            // contenir des espaces)
            let param: Vec<u16> = format!("\"{}\"", path.display()).encode_utf16().chain(Some(0)).collect();
            let _ = ShellExecuteW(None, w!("open"), w!("notepad.exe"), PCWSTR(param.as_ptr()), None, SW_SHOWNORMAL);
        }
    }
}

/// Ouvre une page GitHub dans le navigateur (seules ces adresses sont acceptées).
pub fn open_url(url: &str) {
    if !url.starts_with("https://github.com/") {
        log::warn!("adresse refusée : {url}");
        return;
    }
    let wide: Vec<u16> = url.encode_utf16().chain(Some(0)).collect();
    unsafe {
        ShellExecuteW(None, w!("open"), PCWSTR(wide.as_ptr()), None, None, SW_SHOWNORMAL);
    }
}

/// Attend, `delai` au plus, que le verrou nommé `verrou` (celui de l'instance unique) n'existe
/// plus : l'instance qui le tenait est sortie (Windows le détruit avec la dernière poignée, à la
/// fin du processus). Vrai si c'est fait.
pub fn attendre_fin_instance(verrou: PCWSTR, delai: Duration) -> bool {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{OpenMutexW, SYNCHRONIZATION_SYNCHRONIZE};
    let fin = Instant::now() + delai;
    loop {
        match unsafe { OpenMutexW(SYNCHRONIZATION_SYNCHRONIZE, false, verrou) } {
            Err(_) => return true,
            Ok(h) => unsafe {
                let _ = CloseHandle(h);
            },
        }
        if Instant::now() >= fin {
            return false;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Traite les messages en attente. Renvoie faux quand WM_QUIT est reçu.
pub fn pump() -> bool {
    unsafe {
        let mut msg = MSG::default();
        while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
            if msg.message == WM_QUIT {
                return false;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raccourci_qui_ecrit_un_caractere_ecarte() {
        // français et anglais : Ctrl+Alt+F n'écrit rien, le raccourci peut être pris
        assert_eq!(caractere_vole(&[(0x040C, None), (0x0409, None)]), None);
        // un clavier tchèque en plus : AltGr+F écrit « [ »
        assert_eq!(caractere_vole(&[(0x040C, None), (0x0405, Some('[')), (0x0409, None)]), Some('['));
        // un caractère de contrôle n'écrit rien
        assert_eq!(caractere_vole(&[(0x0409, Some('\u{6}'))]), None);
        assert_eq!(caractere_vole(&[]), None);
    }

    #[test]
    fn rgba_vers_bgra_inverse_rouge_et_bleu() {
        let src = [1u8, 2, 3, 4, 250, 0, 7, 255];
        let mut dst = [0u8; 8];
        rgba_vers_bgra(&mut dst, &src);
        assert_eq!(dst, [3, 2, 1, 4, 7, 0, 250, 255]);
    }

    #[test]
    fn calque_d_affichage() {
        // calque d'Essaim ou d'une autre surcouche (SharkApp : 0x82800a8)
        assert!(calque(0x0828_00a8) && calque((WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST).0));
        // fenêtre superposée qui prend les clics (fondu, jeu), fenêtre ordinaire
        assert!(!calque(WS_EX_LAYERED.0) && !calque(WS_EX_TRANSPARENT.0) && !calque(0x0020_0000));
    }

    #[test]
    fn fenetre_sous_le_pointeur() {
        let sous_point = |z: [(&'static str, VueFenetre); 8], x, y| survolee(z, x, y).map(|(n, _)| n);
        let vue = |classe: &str, cadre: (i32, i32, i32, i32)| VueFenetre {
            visible: true,
            classe: classe.to_owned(),
            cadre: RECT { left: cadre.0, top: cadre.1, right: cadre.2, bottom: cadre.3 },
            ..Default::default()
        };
        let ecran_gauche = (0, 0, 1920, 1080);
        let jeu = VueFenetre { plein_ecran: true, ..vue("UnityWndClass", ecran_gauche) };
        let editeur = VueFenetre { style: WS_OVERLAPPEDWINDOW.0, ..vue("Notepad", (1920, 0, 3000, 800)) };
        // calques (Essaim, autre surcouche), infobulle, barre des tâches, fenêtre masquée par DWM
        // ou cachée : au-dessus dans l'ordre Z, ils ne comptent pas
        let calque_essaim = VueFenetre { style_etendu: (WS_EX_LAYERED | WS_EX_TRANSPARENT).0, ..vue("EssaimOverlay", ecran_gauche) };
        let surcouche = VueFenetre { style_etendu: 0x0828_00a8, ..vue("Autre", ecran_gauche) };
        let infobulle = vue("tooltips_class32", (500, 500, 700, 530));
        let barre = vue("Shell_TrayWnd", (0, 1032, 1920, 1080));
        let masquee = VueFenetre { masquee: true, ..vue("ApplicationFrameWindow", ecran_gauche) };
        let cachee = VueFenetre { visible: false, ..vue("Notepad", ecran_gauche) };
        let z = || {
            [("calque", &calque_essaim), ("surcouche", &surcouche), ("infobulle", &infobulle), ("barre", &barre), ("masquee", &masquee), ("cachee", &cachee), ("editeur", &editeur), ("jeu", &jeu)]
                .map(|(n, v)| (n, v.clone()))
        };
        // pointeur sur l'écran du jeu : le jeu, même sous l'infobulle et la barre
        assert_eq!(sous_point(z(), 510, 510), Some("jeu"));
        assert_eq!(sous_point(z(), 10, 1050), Some("jeu"));
        // pointeur sur l'éditeur de l'autre écran : l'éditeur (le jeu, ailleurs, ne compte pas)
        assert_eq!(sous_point(z(), 2000, 100), Some("editeur"));
        // sous l'éditeur, rien : ni bureau ni fenêtre
        assert_eq!(sous_point(z(), 3500, 100), None);
        // bord droit et bas exclus
        assert!(!retenue_sous_point(&jeu, 1920, 10) && !retenue_sous_point(&jeu, 10, 1080));
        assert!(!retenue_sous_point(&vue("WorkerW", ecran_gauche), 10, 10) && !retenue_sous_point(&vue("Progman", ecran_gauche), 10, 10));
    }

    #[test]
    fn menus_et_bulles_au_dessus_d_une_video_gardent_la_pause() {
        let vue = |classe: &str, cadre: (i32, i32, i32, i32)| VueFenetre {
            visible: true,
            classe: classe.to_owned(),
            cadre: RECT { left: cadre.0, top: cadre.1, right: cadre.2, bottom: cadre.3 },
            ..Default::default()
        };
        let ecran = (0, 0, 1920, 1080);
        let video = VueFenetre { plein_ecran: true, style: WS_POPUP.0, ..vue("Chrome_WidgetWin_1", ecran) };
        let petit = (800, 400, 1100, 700);
        // fenêtres de passage au-dessus de la vidéo : menu, outil, sans activation, possédée,
        // surcouche toujours devant (Discord, Steam, NVIDIA) sans `WS_EX_TRANSPARENT`
        let passages = [
            vue("#32768", petit),
            VueFenetre { style_etendu: WS_EX_TOOLWINDOW.0, ..vue("Notif", petit) },
            VueFenetre { style_etendu: WS_EX_NOACTIVATE.0, ..vue("Bulle", petit) },
            VueFenetre { possedee: true, style: WS_OVERLAPPEDWINDOW.0, ..vue("Liste", petit) },
            VueFenetre { style_etendu: (WS_EX_LAYERED | WS_EX_TOPMOST).0, ..vue("Surcouche", ecran) },
        ];
        for p in &passages {
            assert!(de_passage(p), "{}", p.classe);
            // sans pause avant : la vidéo est trouvée sous la fenêtre de passage
            assert_eq!(plein_ecran_retenu([("passage", p.clone()), ("video", video.clone())], 900, 500, None), Some("video"), "{}", p.classe);
        }
        // une fenêtre de passage en plein écran (jeu en surcouche toujours devant) compte
        let jeu_devant = VueFenetre { plein_ecran: true, ..passages[4].clone() };
        assert!(!de_passage(&jeu_devant));
        assert_eq!(plein_ecran_retenu([("jeu", jeu_devant), ("video", video.clone())], 900, 500, None), Some("jeu"));

        // hystérésis : une fenêtre sans cadre inconnue (ni outil, ni possédée) au-dessus de la
        // vidéo encore en plein écran garde la pause ; rien de retenu sous le point aussi
        let popup = VueFenetre { style: WS_POPUP.0, ..vue("Popup", petit) };
        assert_eq!(plein_ecran_retenu([("popup", popup.clone())], 900, 500, Some("video")), Some("video"));
        assert_eq!(plein_ecran_retenu(std::iter::empty::<(&str, VueFenetre)>(), 900, 500, Some("video")), Some("video"));
        // sans pause avant, la même fenêtre ne la crée pas
        assert_eq!(plein_ecran_retenu([("popup", popup)], 900, 500, None), None);
        // une vraie fenêtre (barre de titre) survolée la lève
        let appli = VueFenetre { style: WS_OVERLAPPEDWINDOW.0, ..vue("Notepad", petit) };
        assert_eq!(plein_ecran_retenu([("appli", appli), ("video", video)], 900, 500, Some("video")), None);
        // la précédente ne couvre plus l'écran (`None` donné par l'appelant) : plus de pause
        let fenetree = VueFenetre { style: WS_OVERLAPPEDWINDOW.0, ..vue("Chrome_WidgetWin_1", (0, 0, 1600, 900)) };
        assert_eq!(plein_ecran_retenu([("video", fenetree)], 900, 500, None), None);
    }

    #[test]
    fn icone_une_comete_et_son_orbe() {
        for cote in [16u32, 20, 24, 32, 40, 48, 64, 256] {
            let (rayon, cx, cy, orbe) = geometrie_icone(cote);
            let pm = dessin_icone(cote, false).unwrap();
            let pixel = |x: f32, y: f32| {
                let p = pm.pixel(x as u32, y as u32).unwrap();
                (p.red(), p.green(), p.blue(), p.alpha())
            };
            let plein = (ICONE_ROUGE.0, ICONE_ROUGE.1, ICONE_ROUGE.2, 255);
            // l'orbe, la tête de la comète et sa queue (à droite de l'axe) sont pleins
            assert_eq!(pixel(cx, cy + rayon / 2.0), plein, "orbe à {cote} px");
            assert_eq!(pixel(cx - rayon / 4.0, cy - rayon / 2.0), plein, "tête à {cote} px");
            assert_eq!(pixel(cx + rayon * 0.75, cy), plein, "queue à {cote} px");
            // le jeu entre l'orbe et la comète reste ouvert (un pixel de large aux petites
            // tailles : il garde un reste de lissage) ; la moitié gauche du bas et les coins sont
            // vides
            let jeu = cx + (orbe + rayon / 2.0) / 2.0;
            assert!(pixel(jeu, cy + rayon / 2.0).3 < 64, "jeu à {cote} px");
            assert_eq!(pixel(cx - rayon * 0.75, cy + rayon / 2.0).3, 0, "bas gauche à {cote} px");
            assert_eq!(pixel(0.0, 0.0).3, 0);
            assert_eq!(pixel(cote as f32 - 1.0, cote as f32 - 1.0).3, 0);
        }
        // en pause : le même motif, en gris
        let p = dessin_icone(32, true).unwrap().pixel(12, 23).unwrap();
        assert_eq!((p.red(), p.green(), p.blue(), p.alpha()), (ICONE_GRIS.0, ICONE_GRIS.1, ICONE_GRIS.2, 255));
    }

    /// Refait `ressources/essaim.ico` (le motif de l'icône en 16 à 256 px, images PNG) :
    /// `cargo test --release exporter_icone -- --ignored`. `build.rs` l'intègre à l'exécutable.
    #[test]
    #[ignore]
    fn exporter_icone() {
        let tailles = [16u32, 20, 24, 32, 40, 48, 64, 96, 128, 256];
        let images: Vec<Vec<u8>> = tailles.iter().map(|&c| dessin_icone(c, false).unwrap().encode_png().unwrap()).collect();
        // ICONDIR, une entrée de 16 octets par image, puis les images
        let mut ico = vec![0, 0, 1, 0];
        ico.extend((tailles.len() as u16).to_le_bytes());
        let mut decalage = 6 + 16 * tailles.len();
        for (&c, png) in tailles.iter().zip(&images) {
            let cote = if c >= 256 { 0 } else { c as u8 };
            ico.extend([cote, cote, 0, 0]);
            ico.extend(1u16.to_le_bytes());
            ico.extend(32u16.to_le_bytes());
            ico.extend((png.len() as u32).to_le_bytes());
            ico.extend((decalage as u32).to_le_bytes());
            decalage += png.len();
        }
        for png in &images {
            ico.extend(png);
        }
        let dossier = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("ressources");
        std::fs::create_dir_all(&dossier).unwrap();
        std::fs::write(dossier.join("essaim.ico"), ico).unwrap();
    }

    #[test]
    fn plein_ecran_et_fenetre_agrandie() {
        let ecran = RECT { left: 0, top: 0, right: 1920, bottom: 1080 };
        let tout = ecran;
        // jeu, vidéo F11, diaporama : ni titre ni bord, couvre l'écran
        assert!(plein_ecran(WS_POPUP.0, false, tout, ecran));
        assert!(plein_ecran(WS_POPUP.0, false, RECT { left: -8, top: -8, right: 1928, bottom: 1088 }, ecran));
        // fenêtre agrandie (barre des tâches masquée) : pas du plein écran
        assert!(!plein_ecran(WS_OVERLAPPEDWINDOW.0, true, tout, ecran));
        assert!(!plein_ecran(WS_POPUP.0, true, tout, ecran));
        assert!(!plein_ecran(WS_POPUP.0 | WS_THICKFRAME.0, false, tout, ecran));
        assert!(!plein_ecran(WS_CAPTION.0, false, tout, ecran));
        // ne couvre pas l'écran
        assert!(!plein_ecran(WS_POPUP.0, false, RECT { left: 0, top: 0, right: 1920, bottom: 1040 }, ecran));
    }

    /// Précision de l'attente minutée : `cargo test --release mesure_attente -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn mesure_attente() {
        for ms in [1u64, 5, 16, 30] {
            let n = 40;
            let t0 = Instant::now();
            for _ in 0..n {
                attendre(Duration::from_millis(ms));
            }
            eprintln!("attente de {ms} ms : {:.2} ms en moyenne", t0.elapsed().as_secs_f64() * 1000.0 / n as f64);
        }
    }

    #[test]
    fn file_des_commandes() {
        let mut f = FileCommandes::default();
        assert_eq!(f.prendre(), 0);
        // plusieurs commandes d'un même passage : aucune perdue, dans l'ordre
        f.pousser(CMD_PAUSE);
        f.pousser(0);
        f.pousser(CMD_SUIVANT);
        f.pousser(CMD_PAUSE);
        assert_eq!((f.prendre(), f.prendre(), f.prendre(), f.prendre()), (CMD_PAUSE, CMD_SUIVANT, CMD_PAUSE, 0));
        // bornée, mais « Quitter » passe toujours, et en premier
        for _ in 0..100 {
            f.pousser(CMD_SUIVANT);
        }
        assert_eq!(f.0.len(), FileCommandes::MAX);
        f.pousser(CMD_QUIT);
        f.pousser(CMD_CONFIG);
        assert_eq!((f.prendre(), f.prendre(), f.prendre()), (CMD_QUIT, CMD_CONFIG, 0));
    }

    #[test]
    fn session_verrouillee_puis_rendue() {
        let distance = unsafe { GetSystemMetrics(SM_REMOTESESSION) } != 0;
        session_changee(WTS_SESSION_LOCK);
        assert_eq!(session_retenue(), Some("Session verrouillée"));
        session_changee(WTS_CONSOLE_DISCONNECT);
        session_changee(WTS_SESSION_UNLOCK);
        assert_eq!(session_retenue(), distance.then_some("Session à distance"), "reprise au déverrouillage");
        session_changee(WTS_CONSOLE_DISCONNECT);
        assert!(session_retenue().is_some());
        session_changee(WTS_CONSOLE_CONNECT);
        assert_eq!(session_retenue(), distance.then_some("Session à distance"));
    }

    #[test]
    fn texte_de_l_infobulle() {
        assert_eq!(infobulle(None, false, false), "Essaim");
        assert_eq!(infobulle(Some("Néon"), false, false), "Essaim — preset « Néon »");
        assert_eq!(infobulle(Some("Néon"), true, true), "Essaim — preset « Néon » · en pause");
        assert_eq!(infobulle(None, false, true), "Essaim · en veille");
    }
}
