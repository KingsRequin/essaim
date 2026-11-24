//! Synchro de teinte avec Prisme (éclairage RGB) : client natif de `ws://127.0.0.1:28650`.
//! Essaim demande la prévisualisation des LED (`preview`, ~15 par seconde) et en tire la teinte
//! dominante : moyenne circulaire des teintes, pondérée par saturation × luminosité. LED
//! éteintes ou blanches → pas de teinte (Essaim reprend sa teinte qui tourne seule).
//!
//! Le client ne tourne que si la synchro est voulue, et se reconnecte toutes les 5 s si Prisme
//! est absent. Synchro coupée, une simple connexion TCP toutes les 5 s dit si Prisme est là
//! (`present`) : la fenêtre des réglages ne propose la synchro qu'alors.

use serde_json::Value;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tungstenite::stream::MaybeTlsStream;
use tungstenite::Message;

const URL: &str = "ws://127.0.0.1:28650";
const ADRESSE: ([u8; 4], u16) = ([127, 0, 0, 1], 28650);
/// Prisme joignable (dernier essai) : la synchro n'est proposée qu'alors.
static PRESENT: AtomicBool = AtomicBool::new(false);

pub fn present() -> bool {
    PRESENT.load(Ordering::Relaxed)
}
/// Dernière teinte nette tenue pendant ce délai (flashs blancs, creux sombres, LED éteintes).
const HOLD: Duration = Duration::from_secs(4);

#[derive(Default)]
pub struct PrismeHue {
    wanted: AtomicBool,
    connected: AtomicBool,
    hue: AtomicU32,
    stamp: Mutex<Option<Instant>>,
}

impl PrismeHue {
    pub fn set_wanted(&self, on: bool) {
        self.wanted.store(on, Ordering::Relaxed);
    }

    pub fn connected(&self) -> bool {
        self.connected.load(Ordering::Relaxed)
    }

    /// Teinte dominante de l'éclairage (degrés) : la dernière teinte nette, tenue `HOLD` (un
    /// flash blanc ou un creux sombre ne fait pas sauter la couleur).
    pub fn current(&self) -> Option<f32> {
        let fresh = self.stamp.lock().ok()?.is_some_and(|s| s.elapsed() < HOLD);
        (fresh && self.connected()).then(|| f32::from_bits(self.hue.load(Ordering::Relaxed)))
    }

    fn store(&self, hue: Option<f32>) {
        let Some(h) = hue else { return };
        self.hue.store(h.to_bits(), Ordering::Relaxed);
        if let Ok(mut s) = self.stamp.lock() {
            *s = Some(Instant::now());
        }
    }
}

pub fn start() -> Arc<PrismeHue> {
    let shared = Arc::new(PrismeHue::default());
    let s = shared.clone();
    let _ = std::thread::Builder::new().name("essaim-prisme".into()).spawn(move || loop {
        if !s.wanted.load(Ordering::Relaxed) {
            let ouvert = std::net::TcpStream::connect_timeout(&ADRESSE.into(), Duration::from_millis(200)).is_ok();
            PRESENT.store(ouvert, Ordering::Relaxed);
            // reprend vite si la synchro est demandée entre deux essais
            for _ in 0..5 {
                if s.wanted.load(Ordering::Relaxed) {
                    break;
                }
                std::thread::sleep(Duration::from_secs(1));
            }
            continue;
        }
        if let Err(e) = session(&s) {
            log::debug!("synchro Prisme : {e}");
        }
        PRESENT.store(false, Ordering::Relaxed);
        if s.connected.swap(false, Ordering::Relaxed) {
            log::info!("synchro Prisme : déconnecté");
        }
        std::thread::sleep(Duration::from_secs(5));
    });
    shared
}

fn session(s: &PrismeHue) -> anyhow::Result<()> {
    let (mut ws, _) = tungstenite::connect(URL)?;
    if let MaybeTlsStream::Plain(t) = ws.get_mut() {
        t.set_read_timeout(Some(Duration::from_millis(500)))?;
    }
    ws.send(Message::Text(r#"{"id":1,"cmd":"preview","enabled":true}"#.into()))?;
    s.connected.store(true, Ordering::Relaxed);
    PRESENT.store(true, Ordering::Relaxed);
    log::info!("synchro Prisme : connecté");
    while s.wanted.load(Ordering::Relaxed) {
        match ws.read() {
            Ok(Message::Text(t)) => {
                let v: Value = serde_json::from_str(t.as_str()).unwrap_or(Value::Null);
                if v["event"] == "preview" {
                    s.store(dominant_hue(&v["data"]["colors"]));
                }
            }
            Ok(Message::Close(_)) => return Ok(()),
            Ok(_) => {}
            Err(tungstenite::Error::Io(e)) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {}
            Err(e) => return Err(e.into()),
        }
    }
    let _ = ws.close(None);
    Ok(())
}

/// Teinte dominante d'une liste `[[r, g, b], …]` ; `None` si tout est éteint ou gris.
pub fn dominant_hue(colors: &Value) -> Option<f32> {
    let (mut x, mut y, mut n) = (0f32, 0f32, 0usize);
    for c in colors.as_array()? {
        let c = c.as_array()?;
        let [r, g, b] = [0, 1, 2].map(|i| c.get(i).and_then(Value::as_f64).unwrap_or(0.0) as f32 / 255.0);
        let (max, min) = (r.max(g).max(b), r.min(g).min(b));
        n += 1;
        if max <= 0.0 || max - min < 1e-3 {
            continue;
        }
        let d = max - min;
        let h = if max == r {
            ((g - b) / d).rem_euclid(6.0)
        } else if max == g {
            (b - r) / d + 2.0
        } else {
            (r - g) / d + 4.0
        } * 60.0;
        let w = (d / max) * max; // saturation × luminosité
        x += w * h.to_radians().cos();
        y += w * h.to_radians().sin();
    }
    // en dessous, l'éclairage est éteint, très sombre ou blanc : pas de teinte à suivre
    if n == 0 || (x * x + y * y).sqrt() / (n as f32) < 0.08 {
        return None;
    }
    Some(y.atan2(x).to_degrees().rem_euclid(360.0))
}

#[cfg(test)]
mod tests {
    use super::dominant_hue;
    use serde_json::json;

    #[test]
    fn teinte_dominante() {
        let h = dominant_hue(&json!([[255, 0, 0], [255, 10, 0], [200, 0, 20]])).unwrap();
        assert!(!(5.0..=355.0).contains(&h), "rouge : {h}");
        let h = dominant_hue(&json!([[0, 0, 255], [0, 20, 255], [10, 0, 0]])).unwrap();
        assert!((h - 240.0).abs() < 10.0, "bleu : {h}");
        assert!(dominant_hue(&json!([[0, 0, 0], [0, 0, 0]])).is_none(), "éteint");
        assert!(dominant_hue(&json!([[255, 255, 255], [200, 200, 200]])).is_none(), "blanc");
    }
}
