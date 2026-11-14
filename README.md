# Essaim

Curseur animé réactif à la musique pour Windows : un point à la place de la flèche, deux comètes
et un essaim de satellites en orbite, et des fissures qui s'ouvrent sous chaque clic.

- **Kick** : inverse le sens de rotation et ajoute 2 à 3 satellites (durée de vie 2,8 s).
- **Quantité de basse** : accélère la rotation.
- **Hauteur de la note de basse** : plus elle est grave, plus les orbites s'éloignent.
- **Couleurs** : « Prisme » (la teinte tourne en continu).

Le son analysé est celui que joue Windows (capture loopback de la sortie par défaut) : Spotify,
navigateur, jeux… sans rien configurer.

## Installation

`scripts\install.ps1` (sans droits administrateur) : compile, copie dans
`%LOCALAPPDATA%\Programs\Essaim`, lance Essaim à l'ouverture de session (clé Run de HKCU) et
ajoute au menu Démarrer « Essaim » et « Essaim - restaurer les curseurs ». `-NoBuild` reprend
la dernière compilation. Désinstaller : `scripts\uninstall.ps1` (réglages conservés).

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
- **Rendu** : un calque **immobile** par écran, de la taille de l'écran, transparent aux clics
  et toujours devant (une fenêtre qui bouge ou apparaît sous la souris fait redemander leur
  curseur aux applications, et l'Explorateur répondait « flèche »). Seule la zone qui change
  est effacée et redessinée. Deux moteurs (`backend.rs`), même interface de dessin
  (`canvas.rs`) :
  - **carte graphique** (par défaut, `gpu.rs`) : DirectComposition + surface virtuelle Direct2D,
    tout dessiné en un **lot de sprites** (une pastille ronde recopiée des milliers de fois ; un
    trait = une rangée de pastilles). Tracer chaque trait avec `DrawLine` coûtait plus que le
    moteur processeur (Direct2D découpe chaque trait lissé sur le processeur) ;
  - **processeur** (secours, `layer.rs`) : tiny-skia + `UpdateLayeredWindowIndirect` avec zone
    modifiée. Bascule automatique si la carte graphique échoue ; `[rendu] carte_graphique`.
  Les traînées sont en coordonnées écran et s'étirent derrière la souris ; cadence calée sur le
  compositeur (`DwmFlush`), plafonnable par `[rendu] cadence`.
- **Plein écran** (jeu, vidéo) sur l'écran de la souris : tout se met en veille et la flèche
  d'origine revient.
- **Clics** : lus par `GetAsyncKeyState` à chaque image (pas de hook global).

## Consommation (mesurée le 2026-11-14)

Banc d'essai `cargo test --release mesure_moteurs -- --ignored --nocapture` (essaim plein,
simulation comprise) : **0,35 ms par image** avec la carte graphique, 1,09 ms avec le
processeur. Pistes écartées après mesure : compilation AVX2 (aucun gain, le coût de tiny-skia
est dans la préparation des tracés), horloge du compositeur `DCompositionWaitForCompositorClock`
(pas de gain, bugs connus).

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
