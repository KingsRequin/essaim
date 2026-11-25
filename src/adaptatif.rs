//! Qualité adaptative (`[rendu] adaptatif`) : quand une image coûte cher à calculer (machine
//! chargée, carte graphique lente), la cadence est plafonnée par paliers (celle de l'écran → 120
//! → 60 → 30 images/s, jamais moins), puis relâchée palier par palier quand le calcul redevient
//! léger, ou à l'essai après un long moment sans seconde lourde. Nourrie chaque seconde par la
//! mesure de la boucle (temps de calcul moyen par image).

/// Plafonds successifs (images/s) ; avant le premier, aucun plafond.
pub const PALIERS: [u32; 3] = [120, 60, 30];
/// Calcul moyen au-delà duquel une seconde compte comme lourde (ms par image).
const LOURD_MS: f32 = 4.0;
/// Secondes lourdes de suite avant de descendre d'un palier.
const SECONDES_LOURDES: u32 = 2;
/// Calcul moyen en deçà duquel une seconde compte comme légère. La mesure est un temps par image,
/// qui ne baisse pas quand la cadence est plafonnée : un seuil trop bas (1,5 ms) laissait un
/// preset ordinaire (2,5 ms) bloqué à 30 images/s après une pointe de charge. Sous les trois
/// quarts du seuil lourd, l'image redevient légère.
const LEGER_MS: f32 = LOURD_MS * 0.75;
/// Secondes légères de suite avant de remonter d'un palier.
const SECONDES_LEGERES: u32 = 10;
/// Secondes de suite sans seconde lourde (entre les deux seuils, 3 à 4 ms) avant de remonter d'un
/// palier à l'essai : sans elles, un calcul moyen restait plafonné pour toujours. Si l'essai
/// redevient lourd, il redescend deux secondes plus tard, et l'attente avant l'essai suivant
/// double (40, 80, 160 s… jusqu'à `ESSAI_MAX`) : une machine qui tient tout juste le palier du
/// dessous n'oscille pas toutes les 20 s. Elle revient à 20 s après un essai tenu
/// (`SECONDES_SANS_LOURDE` sans seconde lourde au nouveau palier) ou une remontée par un calcul
/// vraiment léger.
const SECONDES_SANS_LOURDE: u32 = 20;
const ESSAI_MAX: u32 = 600;

/// Secondes calmes à attendre avant l'essai qui suit `rates` essais ratés d'affilée.
fn attente_essai(rates: u32) -> u32 {
    SECONDES_SANS_LOURDE.saturating_mul(1u32 << rates.min(16)).min(ESSAI_MAX)
}

#[derive(Default)]
pub struct Adaptatif {
    /// 0 = aucun plafond ; n = `PALIERS[n - 1]`.
    niveau: usize,
    lourdes: u32,
    legeres: u32,
    /// Secondes de suite qui ne sont pas lourdes (légères comprises).
    calmes: u32,
    /// Remontée à l'essai en cours (pas encore tenue), et essais ratés d'affilée.
    essai: bool,
    rates: u32,
}

impl Adaptatif {
    /// Plafond de cadence en cours (0 = aucun).
    pub fn plafond(&self) -> u32 {
        self.niveau.checked_sub(1).map_or(0, |i| PALIERS[i])
    }

    /// Mesure d'une seconde : calcul moyen par image (ms) et images dessinées par seconde.
    /// Renvoie vrai si le plafond change. Une seconde sans image (pause, veille) ne compte pas.
    pub fn mesure(&mut self, ms: f32, ips: f32) -> bool {
        if ips < 1.0 || !ms.is_finite() {
            return false;
        }
        (self.lourdes, self.legeres, self.calmes) = match ms {
            m if m > LOURD_MS => (self.lourdes + 1, 0, 0),
            m if m < LEGER_MS => (0, self.legeres + 1, self.calmes + 1),
            _ => (0, 0, self.calmes + 1),
        };
        if self.lourdes >= SECONDES_LOURDES && self.niveau < PALIERS.len() {
            // essai qui redevient lourd : raté, le suivant attendra deux fois plus
            if std::mem::take(&mut self.essai) {
                self.rates = self.rates.saturating_add(1);
            }
            // premier palier qui réduit vraiment : sous la cadence mesurée (un écran à 60 Hz
            // passe directement à 30)
            let n = (self.niveau + 1..=PALIERS.len()).find(|&n| (PALIERS[n - 1] as f32) < ips * 0.95).unwrap_or(PALIERS.len());
            return self.changer(n);
        }
        // essai tenu : l'attente revient à 20 s
        if self.essai && self.calmes >= SECONDES_SANS_LOURDE {
            (self.essai, self.rates) = (false, 0);
        }
        if self.niveau == 0 {
            return false;
        }
        if self.legeres >= SECONDES_LEGERES {
            (self.essai, self.rates) = (false, 0);
            return self.changer(self.niveau - 1);
        }
        if self.calmes >= attente_essai(self.rates) {
            self.essai = true;
            return self.changer(self.niveau - 1);
        }
        false
    }

    /// Plus de plafond (réglage coupé). Renvoie vrai s'il y en avait un.
    pub fn lever(&mut self) -> bool {
        (self.essai, self.rates) = (false, 0);
        self.changer(0)
    }

    fn changer(&mut self, niveau: usize) -> bool {
        (self.lourdes, self.legeres, self.calmes) = (0, 0, 0);
        let change = niveau != self.niveau;
        self.niveau = niveau;
        change
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fait passer `n` secondes à `ms` par image et `ips` images/s ; renvoie les plafonds vus.
    fn secondes(a: &mut Adaptatif, n: usize, ms: f32, ips: f32) -> Vec<u32> {
        let mut vus = Vec::new();
        for _ in 0..n {
            if a.mesure(ms, ips) {
                vus.push(a.plafond());
            }
        }
        vus
    }

    #[test]
    fn descend_par_paliers_sans_passer_sous_30() {
        let mut a = Adaptatif::default();
        assert_eq!(a.plafond(), 0);
        // une seconde lourde ne suffit pas, deux de suite oui
        assert!(!a.mesure(5.0, 144.0));
        assert!(a.mesure(5.0, 144.0));
        assert_eq!(a.plafond(), 120);
        assert_eq!(secondes(&mut a, 2, 5.0, 120.0), [60]);
        assert_eq!(secondes(&mut a, 2, 5.0, 60.0), [30]);
        assert!(secondes(&mut a, 20, 9.0, 30.0).is_empty(), "jamais sous 30");
        assert_eq!(a.plafond(), 30);
    }

    #[test]
    fn saute_les_paliers_inutiles() {
        // écran à 60 Hz : plafonner à 120 ou 60 ne changerait rien
        let mut a = Adaptatif::default();
        assert_eq!(secondes(&mut a, 2, 6.0, 60.0), [30]);
    }

    #[test]
    fn remonte_apres_dix_secondes_legeres() {
        let mut a = Adaptatif::default();
        secondes(&mut a, 6, 5.0, 144.0);
        assert_eq!(a.plafond(), 30);
        assert!(secondes(&mut a, 9, 1.0, 30.0).is_empty());
        assert!(a.mesure(1.0, 30.0));
        assert_eq!(a.plafond(), 60);
        // une seconde moyenne remet le compte à zéro
        secondes(&mut a, 9, 1.0, 60.0);
        assert!(!a.mesure(3.5, 60.0));
        assert!(secondes(&mut a, 9, 1.0, 60.0).is_empty());
        assert_eq!(secondes(&mut a, 1, 1.0, 60.0), [120]);
        assert_eq!(secondes(&mut a, 10, 1.0, 120.0), [0]);
        assert!(secondes(&mut a, 30, 1.0, 144.0).is_empty());
    }

    #[test]
    fn remonte_apres_une_pointe_pour_un_preset_ordinaire() {
        // 2,5 ms par image d'habitude, une pointe de charge le fait descendre à 30 : il remonte
        let mut a = Adaptatif::default();
        secondes(&mut a, 6, 6.0, 144.0);
        assert_eq!(a.plafond(), 30);
        secondes(&mut a, 40, 2.5, 30.0);
        assert_eq!(a.plafond(), 0, "un calcul ordinaire relâche tous les paliers");
    }

    #[test]
    fn calcul_moyen_remonte_a_l_essai() {
        // 3,5 ms par image : ni lourd ni léger. Il remontait jamais ; il remonte après 20 s
        let mut a = Adaptatif::default();
        secondes(&mut a, 6, 5.0, 144.0);
        assert_eq!(a.plafond(), 30);
        assert!(secondes(&mut a, 19, 3.5, 30.0).is_empty());
        assert_eq!(secondes(&mut a, 1, 3.5, 30.0), [60]);
        // l'essai redevient lourd : il redescend, et ne réessaie que 40 s plus tard
        assert_eq!(secondes(&mut a, 2, 5.0, 60.0), [30]);
        assert!(secondes(&mut a, 39, 3.5, 30.0).is_empty());
        assert_eq!(secondes(&mut a, 1, 3.5, 30.0), [60]);
        // une seconde lourde isolée remet le compte à zéro ; l'essai tenu 20 s ramène l'attente
        // à 20 s
        secondes(&mut a, 10, 3.5, 60.0);
        assert!(!a.mesure(5.0, 60.0));
        assert!(secondes(&mut a, 19, 3.5, 60.0).is_empty());
        assert_eq!(secondes(&mut a, 1, 3.5, 60.0), [120]);
    }

    #[test]
    fn essais_rates_de_plus_en_plus_espaces() {
        assert_eq!((0..7).map(attente_essai).collect::<Vec<_>>(), [20, 40, 80, 160, 320, 600, 600]);
        assert_eq!(attente_essai(u32::MAX), ESSAI_MAX);
        // machine qui tient 30 mais pas 60 : chaque essai rate, l'attente double
        let mut a = Adaptatif::default();
        secondes(&mut a, 6, 5.0, 144.0);
        assert_eq!(a.plafond(), 30);
        for attente in [20, 40, 80, 160, 320, 600, 600] {
            assert!(secondes(&mut a, attente - 1, 3.5, 30.0).is_empty(), "attente {attente}");
            assert_eq!(secondes(&mut a, 1, 3.5, 30.0), [60]);
            assert_eq!(secondes(&mut a, 2, 5.0, 60.0), [30]);
        }
        // calcul redevenu vraiment léger : remontée, et l'attente repart de 20 s
        assert_eq!(secondes(&mut a, 10, 1.0, 30.0), [60]);
        assert_eq!(secondes(&mut a, 2, 5.0, 60.0), [30]);
        assert!(secondes(&mut a, 19, 3.5, 30.0).is_empty());
        assert_eq!(secondes(&mut a, 1, 3.5, 30.0), [60]);
    }

    #[test]
    fn secondes_sans_image_ignorees() {
        let mut a = Adaptatif::default();
        assert!(!a.mesure(5.0, 144.0));
        assert!(!a.mesure(0.0, 0.0), "pause : ne compte pas");
        assert!(!a.mesure(f32::NAN, 100.0));
        assert!(a.mesure(5.0, 144.0), "les deux secondes lourdes se suivent malgré la pause");
        assert!(a.lever());
        assert_eq!(a.plafond(), 0);
        assert!(!a.lever());
    }
}
