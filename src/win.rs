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
    MONITORINFO, MONITOR_DEFAULTTONEAREST, AC_SRC_ALPHA, AC_SRC_OVER,
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

impl Surface {
    pub fn new() -> windows::core::Result<Self> {
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
            let hwnd = CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                w!("EssaimOverlay"),
                w!("Essaim"),
                WS_POPUP,
                0,
                0,
                1,
                1,
                None,
                None,
                Some(hinst.into()),
                None,
            )?;
            let dc = CreateCompatibleDC(None);
            Ok(Self { hwnd, dc, bmp: HBITMAP::default(), old: HGDIOBJ::default(), bits: std::ptr::null_mut(), w: 0, h: 0, shown: false })
        }
    }

    fn ensure_size(&mut self, w: i32, h: i32) -> windows::core::Result<()> {
        if w == self.w && h == self.h && !self.bits.is_null() {
            return Ok(());
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
        Ok(())
    }

    /// Copie l'image (RGBA prémultiplié) dans la surface (BGRA prémultiplié).
    pub fn upload(&mut self, pm: &Pixmap) -> windows::core::Result<()> {
        self.ensure_size(pm.width() as i32, pm.height() as i32)?;
        let n = (self.w * self.h * 4) as usize;
        let dst = unsafe { std::slice::from_raw_parts_mut(self.bits, n) };
        for (d, s) in dst.chunks_exact_mut(4).zip(pm.data().chunks_exact(4)) {
            d[0] = s[2];
            d[1] = s[1];
            d[2] = s[0];
            d[3] = s[3];
        }
        Ok(())
    }

    /// Affiche la dernière image envoyée, coin haut-gauche en (x, y), opacité globale `alpha`.
    pub fn present(&mut self, x: i32, y: i32, alpha: u8) {
        if self.bits.is_null() {
            return;
        }
        let blend = windows::Win32::Graphics::Gdi::BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: alpha,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        unsafe {
            let _ = UpdateLayeredWindow(
                self.hwnd,
                None,
                Some(&POINT { x, y }),
                Some(&SIZE { cx: self.w, cy: self.h }),
                Some(self.dc),
                Some(&POINT { x: 0, y: 0 }),
                COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            );
            if !self.shown {
                let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
                self.shown = true;
            }
        }
    }

    pub fn hide(&mut self) {
        if self.shown {
            unsafe {
                let _ = ShowWindow(self.hwnd, SW_HIDE);
            }
            self.shown = false;
        }
    }

    /// Repasse devant les autres fenêtres « toujours devant » ouvertes depuis.
    pub fn raise(&self) {
        if self.shown {
            unsafe {
                let _ = SetWindowPos(
                    self.hwnd,
                    Some(HWND_TOPMOST),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOOWNERZORDER,
                );
            }
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
}

pub fn cursor_state() -> Option<CursorState> {
    unsafe {
        let mut ci = CURSORINFO { cbSize: std::mem::size_of::<CURSORINFO>() as u32, ..Default::default() };
        GetCursorInfo(&mut ci).ok()?;
        Some(CursorState { pos: (ci.ptScreenPos.x, ci.ptScreenPos.y), showing: ci.flags.0 & CURSOR_SHOWING.0 != 0 })
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
        let mon = MonitorFromWindow(fg, MONITOR_DEFAULTTONEAREST);
        if mon != MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONEAREST) {
            return false;
        }
        let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        let mut r = RECT::default();
        if !GetMonitorInfoW(mon, &mut mi).as_bool() || GetWindowRect(fg, &mut r).is_err() {
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
                    let _ = AppendMenuW(menu, MF_STRING, CMD_CONFIG, w!("Ouvrir les réglages"));
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
