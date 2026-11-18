//! Preset automatique (`[automatique]`) : selon l'application au premier plan (prioritaire) ou
//! selon l'heure. Quand plus aucune règle ne s'applique, les réglages d'avant reviennent.

use crate::config::{self, Config, Horaire};
use crate::profils;
use std::time::{Duration, Instant};

#[derive(Default)]
pub struct Auto {
    /// Preset mis par une règle, et les réglages d'avant la première règle.
    courant: Option<String>,
    avant: Option<Config>,
    prochain: Option<Instant>,
}

/// Minutes depuis minuit d'une heure « HH:MM ».
fn minutes(h: &str) -> Option<u32> {
    let (a, b) = h.trim().split_once(':')?;
    let (a, b): (u32, u32) = (a.parse().ok()?, b.parse().ok()?);
    (a < 24 && b < 60).then_some(a * 60 + b)
}

/// Preset de la plage horaire qui contient `maintenant` (minutes depuis minuit) ; une plage
/// dont la fin précède le début passe minuit.
pub fn horaire(regles: &[Horaire], maintenant: u32) -> Option<&str> {
    regles.iter().find_map(|r| {
        let (d, f) = (minutes(&r.debut)?, minutes(&r.fin)?);
        let dedans = if d <= f { (d..f).contains(&maintenant) } else { maintenant >= d || maintenant < f };
        dedans.then_some(r.preset.as_str())
    })
}

fn heure_locale() -> u32 {
    let t = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    t.wHour as u32 * 60 + t.wMinute as u32
}

impl Auto {
    /// À appeler à chaque image ; ne regarde qu'une fois toutes les 0,7 s.
    pub fn tick(&mut self, cfg: &Config, now: Instant) {
        if self.prochain.is_some_and(|p| now < p) {
            return;
        }
        self.prochain = Some(now + Duration::from_millis(700));
        let a = &cfg.automatique;
        let cible = if a.actif {
            // fenêtre d'Essaim ou sans exécutable lisible : rien ne change
            let Some(exe) = crate::appli::nom_premier_plan() else { return };
            a.applis
                .iter()
                .find(|r| r.appli.trim().eq_ignore_ascii_case(&exe))
                .map(|r| r.preset.clone())
                .or_else(|| horaire(&a.horaires, heure_locale()).map(str::to_owned))
        } else {
            None
        };
        if cible == self.courant {
            return;
        }
        match &cible {
            Some(nom) => {
                if self.courant.is_none() {
                    self.avant = Config::read(&config::path()).ok();
                }
                match profils::appliquer(nom) {
                    Ok(()) => log::info!("automatique : preset « {nom} »"),
                    Err(e) => log::warn!("automatique : preset « {nom} » non appliqué ({e})"),
                }
            }
            None => {
                if let Some(avant) = self.avant.take() {
                    match profils::remettre(&avant) {
                        Ok(()) => log::info!("automatique : réglages d'avant remis"),
                        Err(e) => log::warn!("automatique : retour impossible ({e})"),
                    }
                }
            }
        }
        self.courant = cible;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plages_horaires() {
        let r = |d: &str, f: &str, p: &str| Horaire { debut: d.into(), fin: f.into(), preset: p.into() };
        let regles = [r("22:00", "07:00", "Nuit"), r("12:00", "14:00", "Midi"), r("25:00", "26:00", "Faux")];
        assert_eq!(horaire(&regles, 23 * 60), Some("Nuit"));
        assert_eq!(horaire(&regles, 6 * 60 + 59), Some("Nuit"));
        assert_eq!(horaire(&regles, 7 * 60), None);
        assert_eq!(horaire(&regles, 13 * 60), Some("Midi"));
        assert_eq!(horaire(&regles, 14 * 60), None);
    }
}
