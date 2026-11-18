# Essaim

Surcouche réactive à la musique autour du pointeur de Windows : deux comètes et un essaim de
satellites en orbite, un éclat sous le pointeur à chaque kick, et des fissures qui s'ouvrent sous
chaque clic. Le pointeur lui-même reste celui de Windows (voir « Curseurs »).

- **Kick** : inverse le sens de rotation et ajoute 2 à 3 satellites (durée de vie 2,8 s).
- **Quantité de basse** : accélère la rotation.
- **Hauteur de la note de basse** : plus elle est grave, plus les orbites s'éloignent.
- **Couleurs** : celles de l'éclairage RGB quand Prisme tourne (teinte dominante des LED, tenue
  4 s pendant les flashs blancs ou creux sombres), sinon une teinte de départ qui tourne en
  continu, ou reste fixe.
- **Bordure** : la bordure de la fenêtre au premier plan prend la couleur de l'essaim et
  s'illumine sur chaque kick (Windows 11).

Le son analysé est celui que joue Windows (capture loopback de la sortie par défaut) : Spotify,
navigateur, jeux… sans rien configurer.

## Curseurs

Essaim ne touche pas au pointeur : on installe un pack de curseurs par Windows (Paramètres →
Bluetooth et appareils → Souris → Autres paramètres de souris → Pointeurs, ou clic droit →
Installer sur le fichier `.inf` du pack). Le pack utilisé avec Essaim sera fourni dans ce dépôt,
avec le crédit de son auteur.

## Installation

`scripts\install.ps1` (sans droits administrateur) : compile, copie dans
`%LOCALAPPDATA%\Programs\Essaim`, lance Essaim à l'ouverture de session (clé Run de HKCU) et
ajoute au menu Démarrer « Essaim » et « Essaim - réglages ». `-NoBuild` reprend
la dernière compilation. Désinstaller : `scripts\uninstall.ps1` (réglages conservés).

## Utilisation

- Icône de notification : *Mettre en pause*, *Réglages…*, *Quitter*.
- **Fenêtre des réglages** (`essaim.exe --reglages`, ou *Réglages…*) : glissières et cases
  appliquées en direct (écrites dans `config.toml`, commentaires gardés). En haut, les
  **profils** : choisir un profil l'applique ; taper un nom puis *Enregistrer* garde les
  réglages actuels sous ce nom (`%APPDATA%\Essaim\profils\<nom>.toml`), *Supprimer* l'efface.
  Préréglages fournis : *Défaut*, *Discret*, *Intense*, *Point seul*. Les profils ne touchent
  pas à `[rendu]` (propre à la machine).
- Réglages : `%APPDATA%\Essaim\config.toml`, appliqués aussi dès l'enregistrement du fichier.
- Journal : `%APPDATA%\Essaim\essaim.log` (états, cadence et temps de calcul toutes les minutes).
- `essaim.exe --quitter` ferme l'instance en cours.
- `essaim.exe --restaurer-bordure` rend sa couleur à la fenêtre restée colorée si l'appli a été
  tuée de force (fermeture normale, plantage, pause et plein écran le font tout seuls, et le
  lancement suivant aussi).

## Fonctionnement

- **Pointeur** : celui de Windows, jamais dessiné par l'appli (il aurait une image de retard).
  L'appli dessine autour : sphères, éclat du kick sous le pointeur (`[eclat] taille`, 0 = aucun)
  et fissures.
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
- **Bordure** (`bordure.rs`, section `[bordure]`) : `DwmSetWindowAttribute(DWMWA_BORDER_COLOR)`
  sur la fenêtre au premier plan, lue par `GetForegroundWindow` à chaque image (pas de hook).
  Couleur = tête de l'essaim (teinte Prisme ou cycle propre) à la luminosité `repos` (0,5) ; sur
  un kick, montée immédiate vers la couleur pleine (`intensite`, 1) puis retombée en 0,35 s.
  Une seule fenêtre colorée : l'ancienne retrouve la couleur du système au changement de premier
  plan, à la pause, en plein écran et à la fermeture (plantage compris ; la fenêtre colorée est
  notée dans `bordure.hwnd` pour le cas où Essaim est tué). Ignorées : fenêtres sans cadre, outils
  (`WS_EX_TOOLWINDOW`), barre des tâches, bureau, fenêtres d'Essaim, plein écran. Couleur
  quantifiée, envoyée seulement si elle change (60 fois par seconde au plus) ; une fenêtre qui
  refuse (appli administrateur) est notée une fois au journal. Essai réel :
  `cargo test --release bordure_reelle -- --ignored --nocapture` avec un Bloc-notes ouvert.
- **Plein écran** (jeu, vidéo) sur l'écran de la souris : tout se met en veille et la flèche
  d'origine revient.
- **Clics** : lus par `GetAsyncKeyState` à chaque image (pas de hook global).
- **Écrans** : l'essaim est dessiné sur chaque écran qu'il touche (la traînée passe la
  frontière) ; une traînée laissée à plus de 1200 px par un saut du pointeur est abandonnée
  (sinon elle resterait figée sur l'écran quitté). Calques refaits quand la configuration des
  écrans change (`WM_DISPLAYCHANGE`).
- **Kick** : un seul par coup, même long et ondulant (type 808) : un kick est une attaque, et deux
  attaques doivent être séparées d'au moins `[audio] delai_kick` (0,1 s ; la double pédale du metal
  descend vers 0,08 s).
- **Repos** : souris immobile et pas de musique depuis 2 s → `[rendu] cadence_repos` (30 i/s).
- **Carte graphique** : en cas d'erreur, bascule sur le processeur et nouvel essai 10 s plus
  tard, puis toutes les 30 s.

## Interface locale (widget Horizon)

WebSocket `ws://127.0.0.1:28670` (28642 = relais Discord, 28650 = Prisme, 28660 = hooks
d'Horizon), JSON, même forme que Prisme : `get_state`, `set_paused`, `set_config`
(`section`, `key`, `value` : écrit dans `config.toml` en gardant les commentaires, valeurs
bornées ; par exemple `bordure`/`intensite`), `open_config` ; événement `state` à chaque changement et au moins toutes les 2 s.
Pages web refusées (en-tête `Origin`), 8 clients au plus, 3 s pour la poignée de main. Le
widget Essaim d'Horizon s'en sert (pause au clic, réglages en direct dans son panneau).

## Consommation (mesurée le 2026-11-14)

Banc d'essai `cargo test --release mesure_moteurs -- --ignored --nocapture` (essaim plein,
simulation comprise) : **0,35 ms par image** avec la carte graphique, 1,09 ms avec le
processeur. Pistes écartées après mesure : compilation AVX2 (aucun gain, le coût de tiny-skia
est dans la préparation des tracés), horloge du compositeur `DCompositionWaitForCompositorClock`
(pas de gain, bugs connus).

## Analyse du son (`../audio-commun/src/kick.rs`)

La capture et l'analyse sont dans la bibliothèque partagée `audio-commun` (aussi utilisée par
Prisme et Horizon) ; `src/audio.rs` ne fait que publier les résultats pour l'affichage.

Tout est causal, échantillon par échantillon, décisions toutes les 5 ms.

- **Kick** : 5 bandes (biquads, pas de FFT) suivies tous les 5 ms ; chaque pic de montée
  d'énergie est jugé par un petit réseau appris sur de vrais morceaux, 10, 20 ou 30 ms après
  l'attaque selon sa netteté (délai médian mesuré : 25–30 ms). `sensibilite_kick` règle le dernier
  seuil ; sans kick depuis 4 s (intro, montée, reprise après une pause), le premier doit être franc.
  Détails et banc d'essai : README d'`audio-commun`, section « Kick » (F1 0,18 → 0,81 sur 107
  morceaux d'électro, dubstep, hardstyle, rap, pop et rock ; l'ancien détecteur, qui n'écoutait
  que le sub, avait 50 à 150 ms de retard en électro, où le sub arrive après l'attaque).
- **Niveau de basse** : énergie du grave (3 passe-bas à 90 Hz) en dB, rapportée au pic récent du
  morceau (plage de 20 dB).
- **Hauteur** : autocorrélation du grave sous-échantillonné à ~1 kHz, 25–83 Hz, mise à jour hors
  des kicks.
- Limites connues : une note de basse tenue ou une voix grave qui démarre sèchement peut encore
  compter comme un kick (la distinguer demanderait d'écouter plus de 30 ms) ; sur la batterie
  acoustique très propre, l'ancien détecteur faisait un peu mieux (F1 0,93 contre 0,84).

## Développement

Voir `CLAUDE.md`.
