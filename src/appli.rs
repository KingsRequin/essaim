//! Teinte de l'application au premier plan : la couleur dominante de l'icône de son exécutable
//! (`[couleur] application`). Calculée une fois par exécutable, relue seulement quand la fenêtre
//! au premier plan change (`GetForegroundWindow` à chaque image, sans hook).

use std::collections::HashMap;
use std::ffi::c_void;
use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::{CloseHandle, HWND};
use windows::Win32::Graphics::Gdi::{
    DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
};
use windows::Win32::System::Threading::{
    GetCurrentProcessId, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Shell::ExtractIconExW;
use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetForegroundWindow, GetIconInfo, GetWindowThreadProcessId, HICON, ICONINFO};

#[derive(Default)]
pub struct TeinteAppli {
    fenetre: isize,
    teinte: Option<f32>,
    cache: HashMap<String, Option<f32>>,
}

impl TeinteAppli {
    /// Teinte (degrés) de l'application au premier plan. Essaim lui-même, une application sans
    /// icône ou à l'icône grise gardent la teinte précédente.
    pub fn courante(&mut self) -> Option<f32> {
        let fg = unsafe { GetForegroundWindow() };
        if fg.0 as isize == self.fenetre {
            return self.teinte;
        }
        self.fenetre = fg.0 as isize;
        let Some(exe) = executable(fg) else { return self.teinte };
        let teinte = *self.cache.entry(exe.clone()).or_insert_with(|| {
            let t = icone(&exe).and_then(|px| teinte_dominante(&px));
            log::info!("teinte de {} : {}", exe.rsplit('\\').next().unwrap_or(&exe), t.map_or("grise, ignorée".into(), |h| format!("{h:.0}°")));
            t
        });
        if teinte.is_some() {
            self.teinte = teinte;
        }
        self.teinte
    }
}

/// Chemin de l'exécutable de la fenêtre (aucun pour les fenêtres d'Essaim).
fn executable(hwnd: HWND) -> Option<String> {
    unsafe {
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 || pid == GetCurrentProcessId() {
            return None;
        }
        let p = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 512];
        let mut n = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(p, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut n).is_ok();
        let _ = CloseHandle(p);
        ok.then(|| String::from_utf16_lossy(&buf[..n as usize]))
    }
}

/// Pixels BGRA de la grande icône de l'exécutable.
fn icone(exe: &str) -> Option<Vec<u8>> {
    let wide: Vec<u16> = exe.encode_utf16().chain(Some(0)).collect();
    unsafe {
        let mut grande = HICON::default();
        if ExtractIconExW(PCWSTR(wide.as_ptr()), 0, Some(&mut grande), None, 1) == 0 || grande.is_invalid() {
            return None;
        }
        let mut info = ICONINFO::default();
        let lu = GetIconInfo(grande, &mut info).is_ok();
        let _ = DestroyIcon(grande);
        if !lu {
            return None;
        }
        let px = (|| {
            let mut bm = BITMAP::default();
            if info.hbmColor.is_invalid()
                || GetObjectW(info.hbmColor.into(), std::mem::size_of::<BITMAP>() as i32, Some(&mut bm as *mut _ as *mut c_void)) == 0
            {
                return None;
            }
            let (w, h) = (bm.bmWidth, bm.bmHeight);
            let mut bi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: w,
                    biHeight: -h,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut px = vec![0u8; (w * h * 4) as usize];
            let dc = GetDC(None);
            let n = GetDIBits(dc, info.hbmColor, 0, h as u32, Some(px.as_mut_ptr() as *mut c_void), &mut bi, DIB_RGB_COLORS);
            ReleaseDC(None, dc);
            (n > 0).then_some(px)
        })();
        let _ = DeleteObject(info.hbmColor.into());
        let _ = DeleteObject(info.hbmMask.into());
        px
    }
}

/// Teinte dominante (degrés) de pixels BGRA : histogramme de 36 cases pondéré par la saturation
/// et la luminosité, puis moyenne circulaire autour de la case la plus lourde. Rien si l'image
/// est presque grise.
pub fn teinte_dominante(bgra: &[u8]) -> Option<f32> {
    // vieilles icônes sans alpha : tout est opaque
    let sans_alpha = bgra.chunks_exact(4).all(|p| p[3] == 0);
    let mut cases = [0f32; 36];
    let mut pixels: Vec<(f32, f32)> = Vec::new();
    for p in bgra.chunks_exact(4) {
        let a = if sans_alpha { 1.0 } else { p[3] as f32 / 255.0 };
        let (b, g, r) = (p[0] as f32 / 255.0, p[1] as f32 / 255.0, p[2] as f32 / 255.0);
        let (max, min) = (r.max(g).max(b), r.min(g).min(b));
        let s = if max > 0.0 { (max - min) / max } else { 0.0 };
        if a < 0.5 || s < 0.25 || max < 0.2 {
            continue;
        }
        let d = max - min;
        let h = if max == r { 60.0 * ((g - b) / d).rem_euclid(6.0) } else if max == g { 60.0 * ((b - r) / d + 2.0) } else { 60.0 * ((r - g) / d + 4.0) };
        let poids = s * max * a;
        cases[(h / 10.0) as usize % 36] += poids;
        pixels.push((h, poids));
    }
    let total: f32 = cases.iter().sum();
    let n = (bgra.len() / 4) as f32;
    if total < n * 0.03 {
        return None;
    }
    let (best, _) = cases.iter().enumerate().fold((0, 0.0), |m, (i, &w)| if w > m.1 { (i, w) } else { m });
    let centre = best as f32 * 10.0 + 5.0;
    let (mut x, mut y) = (0f32, 0f32);
    for (h, w) in pixels {
        let d = (h - centre + 540.0).rem_euclid(360.0) - 180.0;
        if d.abs() <= 20.0 {
            x += w * h.to_radians().cos();
            y += w * h.to_radians().sin();
        }
    }
    Some(y.atan2(x).to_degrees().rem_euclid(360.0))
}

#[cfg(test)]
mod tests {
    use super::teinte_dominante;

    fn image(pixels: &[([u8; 3], usize)]) -> Vec<u8> {
        pixels.iter().flat_map(|&([r, g, b], n)| std::iter::repeat_n([b, g, r, 255], n).flatten()).collect()
    }

    #[test]
    fn teinte_d_une_icone() {
        // icône Spotify : vert dominant, un peu de noir et de blanc
        let h = teinte_dominante(&image(&[([30, 215, 96], 500), ([0, 0, 0], 300), ([255, 255, 255], 224)])).unwrap();
        assert!((h - 141.0).abs() < 4.0, "{h}");
        // rouge, de part et d'autre de 0°
        let h = teinte_dominante(&image(&[([230, 20, 30], 400), ([230, 30, 20], 400)])).unwrap();
        assert!(!(10.0..350.0).contains(&h), "{h}");
        // grise : rien
        assert!(teinte_dominante(&image(&[([128, 128, 130], 1024)])).is_none());
    }
}
