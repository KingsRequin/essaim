//! Synchro de teinte avec Prisme (éclairage RGB) : client natif de `ws://127.0.0.1:28650`.
//! Essaim demande la prévisualisation des LED (`preview`, ~15 par seconde) et en tire la teinte
//! dominante : moyenne circulaire des teintes, pondérée par saturation × luminosité. LED
//! éteintes ou blanches → pas de teinte (Essaim reprend sa teinte qui tourne seule).
//!
//! Le client ne tourne que si la synchro est voulue, et se reconnecte toutes les 5 s si Prisme
//! est absent. Synchro coupée, une simple connexion TCP toutes les 30 s dit si Prisme est là
//! (`present`) : la fenêtre des réglages ne propose la synchro qu'alors. Une synchro demandée
//! entre deux essais est prise dans la seconde.
//!
//! Ce qui vient du port est lu avec méfiance : connexion et poignée de main bornées dans le
//! temps, messages de 64 Ko au plus, composantes ramenées entre 0 et 255.

use anyhow::anyhow;
use serde_json::Value;
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tungstenite::handshake::HandshakeError;
use tungstenite::protocol::WebSocketConfig;
use tungstenite::Message;

const URL: &str = "ws://127.0.0.1:28650";
const ADRESSE: ([u8; 4], u16) = ([127, 0, 0, 1], 28650);
/// Plus gros message ou fragment accepté de Prisme (octets) : une prévisualisation de plusieurs
/// milliers de LED y tient largement.
const MESSAGE_MAX: usize = 64 * 1024;
/// Synchro coupée : intervalle entre deux sondes de présence.
const SONDE: Duration = Duration::from_secs(30);
/// Délai de connexion, et d'attente de chaque réponse pendant la poignée de main.
const CONNEXION: Duration = Duration::from_millis(500);
const POIGNEE: Duration = Duration::from_secs(2);
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
        let Some(h) = hue.filter(|h| h.is_finite()) else { return };
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
            for _ in 0..SONDE.as_secs() {
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
    let tcp = TcpStream::connect_timeout(&ADRESSE.into(), CONNEXION)?;
    // un serveur qui se tait pendant la poignée de main ne garde pas le fil
    tcp.set_read_timeout(Some(POIGNEE))?;
    tcp.set_write_timeout(Some(POIGNEE))?;
    let limites = WebSocketConfig::default().max_message_size(Some(MESSAGE_MAX)).max_frame_size(Some(MESSAGE_MAX));
    let (mut ws, _) = tungstenite::client::client_with_config(URL, tcp, Some(limites)).map_err(|e| match e {
        HandshakeError::Failure(e) => anyhow::Error::from(e),
        HandshakeError::Interrupted(_) => anyhow!("poignée de main sans réponse"),
    })?;
    ws.get_mut().set_read_timeout(Some(Duration::from_millis(500)))?;
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

/// Teinte dominante d'une liste `[[r, g, b], …]` ; `None` si tout est éteint ou gris. Une
/// composante absente, non numérique ou hors de 0 à 255 est ramenée dans ces bornes.
pub fn dominant_hue(colors: &Value) -> Option<f32> {
    let (mut x, mut y, mut n) = (0f32, 0f32, 0usize);
    for c in colors.as_array()? {
        let c = c.as_array()?;
        let [r, g, b] = [0, 1, 2].map(|i| c.get(i).and_then(Value::as_f64).filter(|v| v.is_finite()).unwrap_or(0.0).clamp(0.0, 255.0) as f32 / 255.0);
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
    let h = y.atan2(x).to_degrees().rem_euclid(360.0);
    h.is_finite().then_some(h)
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

    #[test]
    fn composantes_hors_bornes() {
        // composantes énormes ou négatives : ramenées entre 0 et 255 (rouge franc ici)
        let h = dominant_hue(&json!([[1e300, -5e300, 0], [f64::MAX, 0, -1]])).unwrap();
        assert!(h.is_finite() && !(5.0..=355.0).contains(&h), "{h}");
        let h = dominant_hue(&json!([[0, 0, 1e9]])).unwrap();
        assert!((h - 240.0).abs() < 1.0, "{h}");
        // tout au maximum : blanc, pas de teinte
        assert!(dominant_hue(&json!([[1e308, 1e308, 1e308]])).is_none());
        // NaN et infinis ne passent pas par JSON ; non numériques ou absentes : zéro
        assert!(dominant_hue(&json!([["x", null, {}], []])).is_none());
        assert!(dominant_hue(&json!([[255, "x"]])).unwrap() < 1.0);
        // pas une liste : rien
        assert!(dominant_hue(&json!({"r": 255})).is_none());
        assert!(dominant_hue(&json!([[255, 0, 0], 7])).is_none());
    }

    #[test]
    fn teinte_non_finie_refusee() {
        let p = super::PrismeHue::default();
        p.connected.store(true, std::sync::atomic::Ordering::Relaxed);
        p.store(Some(f32::NAN));
        assert!(p.current().is_none());
        p.store(Some(f32::INFINITY));
        assert!(p.current().is_none());
        p.store(Some(120.0));
        assert_eq!(p.current(), Some(120.0));
    }
}
