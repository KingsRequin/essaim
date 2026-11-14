//! Fabrication des vrais curseurs Windows (le point et ses variantes) et remplacement des
//! curseurs système. Windows les affiche lui-même : aucun retard sur la souris, et le pointeur
//! reste visible même si l'appli s'arrête.
//!
//! Chaque variante est dessinée par `pointer::draw`, encodée en `.cur` (image fixe) ou en `.ani`
//! (animation : rotation du chargement, teinte Prisme qui tourne), écrite dans
//! `%APPDATA%\Essaim\curseurs\<échelle>\` puis chargée à sa taille exacte (pas de mise à
//! l'échelle par Windows). Une taille par échelle d'écran : refaite quand la souris passe sur un
//! écran d'une autre échelle.

use crate::config::Config;
use crate::pointer::{self, Role};
use std::f32::consts::TAU;
use std::path::PathBuf;
use tiny_skia::Pixmap;
use windows::core::PCWSTR;
use windows::Win32::UI::WindowsAndMessaging::{
    DestroyCursor, LoadImageW, SetSystemCursor, HCURSOR, IMAGE_CURSOR, LR_LOADFROMFILE, SYSTEM_CURSOR_ID,
};

/// Demi-côté de l'image (à 100 %) : les glyphes s'étendent jusqu'à ~19 px du point chaud.
const HALF: f32 = 22.0;
/// Durée d'une image des animations de chargement, en 1/60 s.
const SPIN_JIFFIES: u32 = 6;
/// Nombre d'images du cycle de teinte des autres variantes.
const HUE_FRAMES: usize = 24;

enum Anim {
    Still,
    /// Teinte Prisme qui tourne.
    Hue,
    /// Élément qui tourne (et teinte qui tourne).
    Spin,
}

fn anim(role: Role, cfg: &Config) -> Anim {
    let hue = cfg.couleur.vitesse_teinte.abs() > 0.01;
    match role {
        Role::Busy | Role::Working => Anim::Spin,
        Role::Normal | Role::No => Anim::Still,
        _ if hue => Anim::Hue,
        _ => Anim::Still,
    }
}

/// Encode une image (RGBA prémultiplié) en fichier `.cur`, point chaud au centre.
fn cur_bytes(pm: &Pixmap) -> Vec<u8> {
    let (w, h) = (pm.width(), pm.height());
    let mask_row = w.div_ceil(32) * 4;
    let img_len = 40 + w * h * 4 + mask_row * h;
    let mut b = Vec::with_capacity(22 + img_len as usize);
    // ICONDIR + une entrée (type 2 = curseur, avec point chaud)
    b.extend_from_slice(&[0, 0, 2, 0, 1, 0]);
    b.push(if w >= 256 { 0 } else { w as u8 });
    b.push(if h >= 256 { 0 } else { h as u8 });
    b.extend_from_slice(&[0, 0]);
    b.extend_from_slice(&((w / 2) as u16).to_le_bytes());
    b.extend_from_slice(&((h / 2) as u16).to_le_bytes());
    b.extend_from_slice(&img_len.to_le_bytes());
    b.extend_from_slice(&22u32.to_le_bytes());
    // BITMAPINFOHEADER : hauteur doublée (image + masque)
    b.extend_from_slice(&40u32.to_le_bytes());
    b.extend_from_slice(&(w as i32).to_le_bytes());
    b.extend_from_slice(&((h * 2) as i32).to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&32u16.to_le_bytes());
    b.extend_from_slice(&[0u8; 24]);
    // pixels BGRA non prémultipliés, de bas en haut
    let data = pm.data();
    for y in (0..h).rev() {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            let a = data[i + 3] as u32;
            let un = |v: u8| if a == 0 { 0 } else { ((v as u32 * 255 + a / 2) / a).min(255) as u8 };
            b.extend_from_slice(&[un(data[i + 2]), un(data[i + 1]), un(data[i]), data[i + 3]]);
        }
    }
    // masque ET tout à 0 : la transparence vient du canal alpha
    b.resize(b.len() + (mask_row * h) as usize, 0);
    b
}

/// Assemble des images `.cur` en curseur animé `.ani` (RIFF « ACON »).
fn ani_bytes(frames: &[Vec<u8>], jiffies: u32) -> Vec<u8> {
    let mut fram = b"fram".to_vec();
    for f in frames {
        fram.extend_from_slice(b"icon");
        fram.extend_from_slice(&(f.len() as u32).to_le_bytes());
        fram.extend_from_slice(f);
        if f.len() % 2 == 1 {
            fram.push(0);
        }
    }
    let mut body = b"ACON".to_vec();
    body.extend_from_slice(b"anih");
    body.extend_from_slice(&36u32.to_le_bytes());
    let n = frames.len() as u32;
    // cbSize, nFrames, nSteps, iWidth, iHeight, iBitCount, nPlanes, iDispRate, bfAttributes (1 = images .cur)
    for v in [36, n, n, 0, 0, 0, 0, jiffies, 1] {
        body.extend_from_slice(&v.to_le_bytes());
    }
    body.extend_from_slice(b"LIST");
    body.extend_from_slice(&(fram.len() as u32).to_le_bytes());
    body.extend_from_slice(&fram);
    let mut out = b"RIFF".to_vec();
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(&body);
    out
}

fn frame(role: Role, side: u32, hue_t: f64, spin: f32, cfg: &Config, scale: f32) -> Vec<u8> {
    let mut pm = Pixmap::new(side, side).expect("taille de curseur valide");
    let c = side as f32 / 2.0;
    pointer::draw(&mut pm, role, c, c, hue_t, spin, cfg, scale);
    cur_bytes(&pm)
}

/// Fichier du curseur `role` à l'échelle `scale` : (contenu, extension).
fn encode(role: Role, cfg: &Config, scale: f32) -> (Vec<u8>, &'static str, u32) {
    let side = (2.0 * (HALF * scale).ceil()) as u32;
    // période du cycle de teinte (s) ; arrondie pour que la rotation boucle sans saut
    let period = if cfg.couleur.vitesse_teinte.abs() > 0.01 { (360.0 / cfg.couleur.vitesse_teinte.abs()).clamp(1.0, 30.0) } else { 1.0 };
    let bytes = match anim(role, cfg) {
        Anim::Still => return (frame(role, side, 0.0, 0.0, cfg, scale), "cur", side),
        Anim::Hue => {
            let jiffies = ((period * 60.0) / HUE_FRAMES as f32).round().max(1.0) as u32;
            let frames: Vec<_> = (0..HUE_FRAMES)
                .map(|k| frame(role, side, (k as f32 * jiffies as f32 / 60.0) as f64, 0.0, cfg, scale))
                .collect();
            ani_bytes(&frames, jiffies)
        }
        Anim::Spin => {
            // un tour par seconde, nombre entier de tours par cycle de teinte
            let n = ((period * 60.0) / SPIN_JIFFIES as f32).round().max(1.0) as usize;
            let turns = (period.round()).max(1.0);
            let frames: Vec<_> = (0..n)
                .map(|k| {
                    let ph = k as f32 / n as f32;
                    frame(role, side, (ph * period) as f64, (ph * turns * TAU).rem_euclid(TAU), cfg, scale)
                })
                .collect();
            ani_bytes(&frames, SPIN_JIFFIES)
        }
    };
    (bytes, "ani", side)
}

fn dir(scale: f32) -> PathBuf {
    crate::config::dir().join("curseurs").join(format!("{}", (scale * 100.0).round() as u32))
}

/// Fabrique les curseurs de `roles` à l'échelle `scale` et les met à la place des curseurs
/// système. Renvoie les variantes réellement appliquées.
pub fn apply(roles: &[Role], cfg: &Config, scale: f32) -> Vec<Role> {
    let d = dir(scale);
    if let Err(e) = std::fs::create_dir_all(&d) {
        log::warn!("curseurs : dossier {} impossible : {e}", d.display());
        return Vec::new();
    }
    let mut done = Vec::new();
    for &role in roles {
        let (bytes, ext, side) = encode(role, cfg, scale);
        let path = d.join(format!("{}.{ext}", role.id()));
        if let Err(e) = std::fs::write(&path, &bytes) {
            log::warn!("curseurs : écriture de {} impossible : {e}", path.display());
            continue;
        }
        let wide: Vec<u16> = path.as_os_str().to_string_lossy().encode_utf16().chain(Some(0)).collect();
        unsafe {
            match LoadImageW(None, PCWSTR(wide.as_ptr()), IMAGE_CURSOR, side as i32, side as i32, LR_LOADFROMFILE) {
                Ok(h) => {
                    let cur = HCURSOR(h.0);
                    match SetSystemCursor(cur, SYSTEM_CURSOR_ID(role.id())) {
                        Ok(()) => done.push(role),
                        Err(e) => {
                            let _ = DestroyCursor(cur);
                            log::warn!("curseurs : remplacement de {role:?} impossible : {e}");
                        }
                    }
                }
                Err(e) => log::warn!("curseurs : chargement de {} impossible : {e}", path.display()),
            }
        }
    }
    log::info!("curseurs : {} variantes appliquées à {:.0} %", done.len(), scale * 100.0);
    done
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fichier_cur_bien_forme() {
        let pm = Pixmap::new(44, 44).unwrap();
        let b = cur_bytes(&pm);
        assert_eq!(&b[0..6], &[0, 0, 2, 0, 1, 0]);
        assert_eq!(u16::from_le_bytes([b[10], b[11]]), 22, "point chaud x au centre");
        let len = u32::from_le_bytes([b[14], b[15], b[16], b[17]]) as usize;
        assert_eq!(b.len(), 22 + len);
    }

    #[test]
    fn fichier_ani_bien_forme() {
        let cfg = Config::default();
        let (b, ext, _) = encode(Role::Busy, &cfg, 1.0);
        assert_eq!(ext, "ani");
        assert_eq!(&b[0..4], b"RIFF");
        assert_eq!(&b[8..12], b"ACON");
        assert_eq!(u32::from_le_bytes([b[4], b[5], b[6], b[7]]) as usize + 8, b.len());
        let frames = u32::from_le_bytes([b[24], b[25], b[26], b[27]]);
        assert!(frames >= 10, "{frames} images");
    }

    #[test]
    fn variantes_fixes_et_animees() {
        let cfg = Config::default();
        assert_eq!(encode(Role::Normal, &cfg, 1.5).1, "cur");
        assert_eq!(encode(Role::Text, &cfg, 1.5).1, "ani");
        assert_eq!(encode(Role::Normal, &cfg, 1.5).2, 66);
    }
}
