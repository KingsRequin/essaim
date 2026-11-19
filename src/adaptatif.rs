//! Qualité adaptative (`[rendu] adaptatif`) : quand une image coûte cher à calculer (machine
//! chargée, carte graphique lente), la cadence est plafonnée par paliers (celle de l'écran → 120
//! → 60 → 30 images/s, jamais moins), puis relâchée palier par palier quand le calcul redevient
//! léger. Nourrie chaque seconde par la mesure de la boucle (temps de calcul moyen par image).

/// Plafonds successifs (images/s) ; avant le premier, aucun plafond.
pub const PALIERS: [u32; 3] = [120, 60, 30];
/// Calcul moyen au-delà duquel une seconde compte comme lourde (ms par image).
const LOURD_MS: f32 = 4.0;
/// Secondes lourdes de suite avant de descendre d'un palier.
const SECONDES_LOURDES: u32 = 2;
/// Calcul moyen en deçà duquel une seconde compte comme légère.
const LEGER_MS: f32 = 1.5;
/// Secondes légères de suite avant de remonter d'un palier.
const SECONDES_LEGERES: u32 = 10;

#[derive(Default)]
pub struct Adaptatif {
    /// 0 = aucun plafond ; n = `PALIERS[n - 1]`.
    niveau: usize,
    lourdes: u32,
    legeres: u32,
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
        (self.lourdes, self.legeres) = match ms {
            m if m > LOURD_MS => (self.lourdes + 1, 0),
            m if m < LEGER_MS => (0, self.legeres + 1),
            _ => (0, 0),
        };
        if self.lourdes >= SECONDES_LOURDES && self.niveau < PALIERS.len() {
            // premier palier qui réduit vraiment : sous la cadence mesurée (un écran à 60 Hz
            // passe directement à 30)
            let n = (self.niveau + 1..=PALIERS.len()).find(|&n| (PALIERS[n - 1] as f32) < ips * 0.95).unwrap_or(PALIERS.len());
            return self.changer(n);
        }
        if self.legeres >= SECONDES_LEGERES && self.niveau > 0 {
            return self.changer(self.niveau - 1);
        }
        false
    }

    /// Plus de plafond (réglage coupé). Renvoie vrai s'il y en avait un.
    pub fn lever(&mut self) -> bool {
        self.changer(0)
    }

    fn changer(&mut self, niveau: usize) -> bool {
        (self.lourdes, self.legeres) = (0, 0);
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
        assert!(!a.mesure(2.5, 60.0));
        assert!(secondes(&mut a, 9, 1.0, 60.0).is_empty());
        assert_eq!(secondes(&mut a, 1, 1.0, 60.0), [120]);
        assert_eq!(secondes(&mut a, 10, 1.0, 120.0), [0]);
        assert!(secondes(&mut a, 30, 1.0, 144.0).is_empty());
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
