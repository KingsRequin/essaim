# Essaim

Curseur animé réactif à la musique pour Windows : un point à la place de la flèche, deux comètes
et un essaim de satellites en orbite, et des fissures qui s'ouvrent sous chaque clic.

- **Kick** : inverse le sens de rotation et ajoute 2 à 3 satellites (durée de vie 2,8 s).
- **Quantité de basse** : accélère la rotation.
- **Hauteur de la note de basse** : plus elle est grave, plus les orbites s'éloignent.
- **Couleurs** : « Prisme » (la teinte tourne en continu).

Le son analysé est celui que joue Windows (capture loopback de la sortie par défaut) : Spotify,
navigateur, jeux… sans rien configurer.

## Utilisation

- Icône de notification : *Mettre en pause*, *Ouvrir les réglages*, *Quitter*.
- Réglages : `%APPDATA%\Essaim\config.toml`, appliqués dès l'enregistrement du fichier.
- Journal : `%APPDATA%\Essaim\essaim.log` (états, cadence et temps de calcul toutes les minutes).
- `essaim.exe --quitter` ferme l'instance en cours.
- `essaim.exe --restaurer-curseur` remet tes curseurs Windows si l'appli a été tuée de force
  (fermeture normale, plantage, pause et plein écran les remettent tout seuls). Même tuée,
  le pointeur reste visible : ce sont de vrais curseurs.

## Fonctionnement

- **Curseur** : le point et ses 15 variantes (texte, lien, chargement, arrière-plan, précision,
  aide, interdit, déplacer, 4 redimensionnements, stylo, autre sélection) sont de **vrais
  curseurs Windows**, fabriqués par l'appli (`cursors.rs`, dessin `pointer.rs`) à la taille de
  l'écran sous la souris, puis mis en place par `SetSystemCursor` : aucun retard. Ceux qui
  bougent (chargement, teinte Prisme) sont des curseurs animés `.ani`. Fichiers dans
  `%APPDATA%\Essaim\curseurs\<échelle>\`. L'appli dessine par-dessus les sphères, l'éclat du
  kick sous le point et les fissures.
- **Rendu** : un calque transparent par écran (`layer.rs`), de la taille de l'écran et
  **immobile**, transparent aux clics et toujours devant. Essaim et fissures y sont dessinés ;
  seule la zone qui change est effacée, redessinée et envoyée (`UpdateLayeredWindowIndirect`
  avec rectangle modifié). Une fenêtre qui bouge ou apparaît sous la souris fait redemander leur
  curseur aux applications, et l'Explorateur répond alors « flèche » : la barre de texte et le
  redimensionnement redevenaient le point. Les traînées sont en coordonnées écran et s'étirent
  derrière la souris. Dessin par `tiny-skia`, cadence calée sur le compositeur (`DwmFlush`).
- **Plein écran** (jeu, vidéo) sur l'écran de la souris : tout se met en veille et la flèche
  d'origine revient.
- **Clics** : lus par `GetAsyncKeyState` à chaque image (pas de hook global).

## Consommation (mesurée le 2026-11-14)

~7 % d'un cœur (≈ 0,6 % du processeur, 12 cœurs logiques) à 144 images/s, 3,5 Mo de mémoire.
Une image avec l'essaim plein (16 satellites, écran à 150 %) se dessine en ~1,2 ms : les
traînées des satellites et les halos ne sont pas lissés (le lissage coûte ~3,5× plus cher par
tracé), les traînées sont tracées par morceaux. Mesure : `cargo test --release mesure_cout --
--ignored --nocapture`.

## Analyse du son (`src/dsp.rs`)

Tout est causal, échantillon par échantillon, décisions toutes les 5 ms.

- **Kick** : grave isolé par 3 passe-bas biquad à 90 Hz (pas de FFT : sa fenêtre étale le corps
  de la caisse claire dans le grave). Attaque = énergie grave au-dessus de sa moyenne lente
  (~350 ms), seuil réglable (`sensibilite_kick`). Garde anti-caisse claire : l'énergie grave doit
  valoir au moins 0,3 × le pic récent (120 ms) de l'énergie 120–1000 Hz. Délai mesuré : 10–20 ms.
- **Niveau de basse** : énergie du même grave en dB, rapportée au pic récent du morceau (plage
  de 20 dB).
- **Hauteur** : autocorrélation du grave sous-échantillonné à ~1 kHz, 25–83 Hz, mise à jour hors
  des kicks.
- Limite connue : une note de basse qui démarre très sèchement (depuis le silence) peut compter
  comme un kick. Une attente de confirmation a été essayée et écartée : elle ratait la moitié des
  vrais kicks (un kick descend en hauteur et son énergie grave monte encore 20–40 ms).

## Développement

Voir `CLAUDE.md`.
