//! Tout ce qui touche à Windows : fenêtres superposées transparentes, curseur système, écrans,
//! icône de notification.

use std::cell::Cell;
use std::ffi::c_void;
use std::sync::Once;
use tiny_skia::Pixmap;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateBitmap, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetMonitorInfoW, MonitorFromPoint,
    MonitorFromWindow, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP, HDC, HGDIOBJ,
    EnumDisplayMonitors, HMONITOR, MONITORINFO, MONITOR_DEFAULTTONEAREST, AC_SRC_ALPHA, AC_SRC_OVER,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::Shell::{
    ShellExecuteW, Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW,
};
use windows::Win32::UI::WindowsAndMessaging::*;

// ---------------------------------------------------------------------------------------------
// Fenêtre superposée : transparente aux clics, toujours devant, hors barre des tâches.

static OVERLAY_CLASS: Once = Once::new();

extern "system" fn overlay_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == WM_NCHITTEST {
        return LRESULT(HTTRANSPARENT as isize);
    }
    unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
}

pub struct Surface {
    hwnd: HWND,
    dc: HDC,
    bmp: HBITMAP,
    old: HGDIOBJ,
    bits: *mut u8,
    w: i32,
    h: i32,
    shown: bool,
}

/// Fenêtre superposée transparente aux clics, toujours devant, hors barre des tâches, au
/// rectangle écran `rect`. `composition` : sans surface GDI, pour un contenu DirectComposition
/// (elle est alors affichée tout de suite et ne bouge plus).
pub fn overlay_window(rect: (i32, i32, i32, i32), composition: bool) -> windows::core::Result<HWND> {
    unsafe {
        let hinst = GetModuleHandleW(None)?;
        OVERLAY_CLASS.call_once(|| {
            let wc = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                lpfnWndProc: Some(overlay_proc),
                hInstance: hinst.into(),
                lpszClassName: w!("EssaimOverlay"),
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
            w!("EssaimOverlay"),
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
        // Sans cette propriété, l'Explorateur prend ce calque (toujours devant, de la taille de
        // l'écran) pour une appli plein écran : la barre des tâches masquée automatiquement perd
        // son « toujours devant » et ressort derrière les fenêtres.
        SetPropW(hwnd, w!("NonRudeHWND"), Some(windows::Win32::Foundation::HANDLE(std::ptr::without_provenance_mut(1))))?;
        if composition {
            // une fenêtre « layered » n'est montrée qu'avec des attributs ; l'opacité globale
            // reste pleine, la transparence vient du contenu DirectComposition
            SetLayeredWindowAttributes(hwnd, COLORREF(0), 255, LWA_ALPHA)?;
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        }
        Ok(hwnd)
    }
}

pub fn destroy_window(hwnd: HWND) {
    unsafe {
        let _ = DestroyWindow(hwnd);
    }
}

impl Surface {
    pub fn new() -> windows::core::Result<Self> {
        unsafe {
            let hwnd = overlay_window((0, 0, 1, 1), false)?;
            let dc = CreateCompatibleDC(None);
            Ok(Self { hwnd, dc, bmp: HBITMAP::default(), old: HGDIOBJ::default(), bits: std::ptr::null_mut(), w: 0, h: 0, shown: false })
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
        let full = resized || !self.shown || region.is_none();
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
            for (d, s) in dst[a..z].chunks_exact_mut(4).zip(src[a..z].chunks_exact(4)) {
                d[0] = s[2];
                d[1] = s[1];
                d[2] = s[0];
                d[3] = s[3];
            }
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
            let _ = UpdateLayeredWindowIndirect(self.hwnd, &info);
            if !self.shown {
                let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
                self.shown = true;
            }
        }
        Ok(())
    }

    pub fn hide(&mut self) {
        if self.shown {
            unsafe {
                let _ = ShowWindow(self.hwnd, SW_HIDE);
            }
            self.shown = false;
        }
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
// Curseur système

/// Recharge les curseurs choisis dans Windows (annule `cursors::apply`).
pub fn restore_cursors() {
    unsafe {
        let _ = SystemParametersInfoW(SPI_SETCURSORS, 0, None, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0));
    }
}

pub struct CursorState {
    pub pos: (i32, i32),
    /// Le curseur est affiché (les jeux et lecteurs vidéo le cachent).
    pub showing: bool,
    /// Poignée du curseur affiché.
    pub handle: isize,
}

pub fn cursor_state() -> Option<CursorState> {
    unsafe {
        let mut ci = CURSORINFO { cbSize: std::mem::size_of::<CURSORINFO>() as u32, ..Default::default() };
        GetCursorInfo(&mut ci).ok()?;
        Some(CursorState {
            pos: (ci.ptScreenPos.x, ci.ptScreenPos.y),
            showing: ci.flags.0 & CURSOR_SHOWING.0 != 0,
            handle: ci.hCursor.0 as isize,
        })
    }
}

/// Diagnostic : nom du curseur système de poignée `h` (« autre » pour un curseur d'application).
/// Les poignées des curseurs système ne changent pas quand `SetSystemCursor` les remplace.
pub fn cursor_name(h: isize) -> String {
    use crate::pointer::Role;
    Role::ALL
        .iter()
        .find(|r| unsafe { LoadCursorW(None, PCWSTR(r.id() as usize as *const u16)) }.is_ok_and(|x| x.0 as isize == h))
        .map_or_else(|| format!("autre ({h:#x})"), |r| format!("{r:?}"))
}

/// Diagnostic : exécutable de la fenêtre sous le point.
pub fn app_under(x: i32, y: i32) -> String {
    use windows::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION};
    unsafe {
        let w = WindowFromPoint(POINT { x, y });
        let mut pid = 0u32;
        GetWindowThreadProcessId(w, Some(&mut pid));
        let Ok(p) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else { return format!("pid {pid}") };
        let mut buf = [0u16; 260];
        let mut n = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(p, PROCESS_NAME_WIN32, windows::core::PWSTR(buf.as_mut_ptr()), &mut n).is_ok();
        let _ = windows::Win32::Foundation::CloseHandle(p);
        if !ok {
            return format!("pid {pid}");
        }
        let path = String::from_utf16_lossy(&buf[..n as usize]);
        path.rsplit('\\').next().unwrap_or(&path).to_string()
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

/// Taille du pointeur choisie dans Windows (32 = taille normale).
pub fn cursor_base_size() -> u32 {
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
    let mut v = 32u32;
    let mut n = std::mem::size_of::<u32>() as u32;
    let ok = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Control Panel\\Cursors"),
            w!("CursorBaseSize"),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut v as *mut u32 as *mut c_void),
            Some(&mut n),
        )
    };
    if ok.is_ok() && (16..=512).contains(&v) {
        v
    } else {
        32
    }
}

/// Échelle de l'écran sous le point (1.0 = 100 %).
pub fn scale_at(x: i32, y: i32) -> f32 {
    unsafe {
        let mon = MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONEAREST);
        let (mut dx, mut dy) = (96u32, 96u32);
        if GetDpiForMonitor(mon, MDT_EFFECTIVE_DPI, &mut dx, &mut dy).is_ok() {
            dx as f32 / 96.0
        } else {
            1.0
        }
    }
}

/// Une fenêtre en plein écran occupe l'écran où se trouve le point (jeu, vidéo…).
pub fn fullscreen_under(x: i32, y: i32) -> bool {
    unsafe {
        let fg = GetForegroundWindow();
        if fg.is_invalid() {
            return false;
        }
        let mut class = [0u16; 64];
        let n = GetClassNameW(fg, &mut class) as usize;
        let class = String::from_utf16_lossy(&class[..n]);
        if matches!(class.as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd" | "EssaimOverlay") {
            return false; // bureau (Wallpaper Engine), barre des tâches
        }
        if MonitorFromWindow(fg, MONITOR_DEFAULTTONEAREST) != MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONEAREST) {
            return false;
        }
        covers_monitor(fg)
    }
}

/// La fenêtre couvre tout son écran (plein écran : jeu, vidéo…).
pub fn covers_monitor(hwnd: HWND) -> bool {
    unsafe {
        let mon = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        let mut r = RECT::default();
        if !GetMonitorInfoW(mon, &mut mi).as_bool() || GetWindowRect(hwnd, &mut r).is_err() {
            return false;
        }
        r.left <= mi.rcMonitor.left && r.top <= mi.rcMonitor.top && r.right >= mi.rcMonitor.right && r.bottom >= mi.rcMonitor.bottom
    }
}

// ---------------------------------------------------------------------------------------------
// Icône de notification

pub const CMD_PAUSE: usize = 1;
pub const CMD_CONFIG: usize = 2;
pub const CMD_QUIT: usize = 3;
const WM_TRAY: u32 = WM_APP + 1;
pub const TRAY_CLASS: PCWSTR = w!("EssaimTray");

thread_local! {
    static COMMAND: Cell<usize> = const { Cell::new(0) };
    static PAUSED: Cell<bool> = const { Cell::new(false) };
    static TASKBAR_CREATED: Cell<u32> = const { Cell::new(0) };
    static ICON: Cell<HICON> = const { Cell::new(HICON(std::ptr::null_mut())) };
    static DISPLAY_CHANGED: Cell<bool> = const { Cell::new(false) };
}

/// Vrai une fois après un changement de la configuration des écrans (résolution, échelle,
/// écran branché ou débranché).
pub fn take_display_changed() -> bool {
    DISPLAY_CHANGED.with(|d| d.replace(false))
}

/// Commande choisie dans le menu depuis le dernier appel (0 = aucune).
pub fn take_command() -> usize {
    COMMAND.with(|c| c.replace(0))
}

pub fn set_paused_label(paused: bool) {
    PAUSED.with(|p| p.set(paused));
}

fn nid(hwnd: HWND) -> NOTIFYICONDATAW {
    let mut n = NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: 1,
        uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
        uCallbackMessage: WM_TRAY,
        hIcon: ICON.with(|i| i.get()),
        ..Default::default()
    };
    for (d, s) in n.szTip.iter_mut().zip("Essaim".encode_utf16()) {
        *d = s;
    }
    n
}

extern "system" fn tray_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        if msg == WM_TRAY {
            let ev = (lp.0 & 0xFFFF) as u32;
            if ev == WM_RBUTTONUP || ev == WM_LBUTTONUP {
                if let Ok(menu) = CreatePopupMenu() {
                    let paused = PAUSED.with(|p| p.get());
                    let _ = AppendMenuW(menu, MF_STRING, CMD_PAUSE, if paused { w!("Reprendre") } else { w!("Mettre en pause") });
                    let _ = AppendMenuW(menu, MF_STRING, CMD_CONFIG, w!("Réglages…"));
                    let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);
                    let _ = AppendMenuW(menu, MF_STRING, CMD_QUIT, w!("Quitter"));
                    let mut pt = POINT::default();
                    let _ = GetCursorPos(&mut pt);
                    let _ = SetForegroundWindow(hwnd);
                    let _ = TrackPopupMenu(menu, TPM_RIGHTBUTTON, pt.x, pt.y, None, hwnd, None);
                    let _ = DestroyMenu(menu);
                }
            }
            return LRESULT(0);
        }
        if msg == WM_COMMAND {
            COMMAND.with(|c| c.set(wp.0 & 0xFFFF));
            return LRESULT(0);
        }
        if msg == WM_DISPLAYCHANGE || msg == WM_DPICHANGED || (msg == WM_SETTINGCHANGE && wp.0 == SPI_SETWORKAREA.0 as usize) {
            DISPLAY_CHANGED.with(|d| d.set(true));
        }
        if msg == WM_CLOSE || msg == WM_ENDSESSION {
            COMMAND.with(|c| c.set(CMD_QUIT));
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
            ICON.with(|i| i.set(make_icon().unwrap_or_else(|| LoadIconW(None, IDI_APPLICATION).unwrap_or_default())));
            if !Shell_NotifyIconW(NIM_ADD, &nid(hwnd)).as_bool() {
                log::warn!("icône de notification : ajout impossible");
            }
            Ok(Self { hwnd })
        }
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
        unsafe {
            let _ = Shell_NotifyIconW(NIM_DELETE, &nid(self.hwnd));
            let _ = DestroyWindow(self.hwnd);
        }
    }
}

/// Icône de l'appli (pour une fenêtre), dessinée à la volée.
pub fn app_icon() -> Option<HICON> {
    make_icon()
}

/// Icône 32×32 dessinée à la volée : le point et deux comètes.
fn make_icon() -> Option<HICON> {
    use tiny_skia::{Color, FillRule, Paint, PathBuilder, Transform};
    let mut pm = Pixmap::new(32, 32)?;
    let mut fill = |x: f32, y: f32, r: f32, c: (u8, u8, u8)| {
        let mut p = Paint::default();
        p.set_color(Color::from_rgba8(c.0, c.1, c.2, 255));
        p.anti_alias = true;
        if let Some(path) = PathBuilder::from_circle(x, y, r) {
            pm.fill_path(&path, &p, FillRule::Winding, Transform::identity(), None);
        }
    };
    fill(16.0, 16.0, 4.5, (255, 255, 255));
    fill(26.0, 10.0, 3.5, (122, 162, 247));
    fill(6.0, 22.0, 3.5, (187, 154, 247));
    fill(9.0, 6.0, 2.0, (125, 207, 255));
    unsafe {
        let dc = CreateCompatibleDC(None);
        let bi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: 32,
                biHeight: -32,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits: *mut c_void = std::ptr::null_mut();
        let color = CreateDIBSection(Some(dc), &bi, DIB_RGB_COLORS, &mut bits, None, 0).ok()?;
        let dst = std::slice::from_raw_parts_mut(bits as *mut u8, 32 * 32 * 4);
        for (d, s) in dst.chunks_exact_mut(4).zip(pm.data().chunks_exact(4)) {
            let a = s[3] as u32;
            let un = |v: u8| if a == 0 { 0 } else { ((v as u32 * 255 + a / 2) / a).min(255) as u8 };
            d[0] = un(s[2]);
            d[1] = un(s[1]);
            d[2] = un(s[0]);
            d[3] = s[3];
        }
        let mask = CreateBitmap(32, 32, 1, 1, Some([0u8; 128].as_ptr() as *const c_void));
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
            // pas d'appli associée aux .toml : Bloc-notes
            let _ = ShellExecuteW(None, w!("open"), w!("notepad.exe"), PCWSTR(wide.as_ptr()), None, SW_SHOWNORMAL);
        }
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
