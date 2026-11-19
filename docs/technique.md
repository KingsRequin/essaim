# Essaim : fonctionnement

Comment Essaim dessine, écoute et reste léger. Usage : [guide.md](guide.md).

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
  compositeur (`DwmFlush`), plafonnable par `[rendu] cadence`. Avec une cadence plafonnée, la
  boucle attend l'image suivante sur une minuterie haute résolution
  (`CreateWaitableTimerExW` + `MsgWaitForMultipleObjects`, réveil au moins toutes les 30 ms pour
  lire les clics) au lieu de se réveiller à chaque rafraîchissement. Au-delà de 0,05 s entre
  deux images, la simulation avance en plusieurs pas (pointeur interpolé, 0,25 s rattrapées au
  plus). Sur processeur, les deux passes fixes d'une fissure sont tracées une fois dans une image
  recopiée ensuite.
- **Bordure** (`bordure.rs`, section `[bordure]`) : `DwmSetWindowAttribute(DWMWA_BORDER_COLOR)`
  sur la fenêtre au premier plan, lue par `GetForegroundWindow` à chaque image (pas de hook).
  Couleur = tête de l'essaim (teinte Prisme ou cycle propre) à la luminosité `repos` (0,5) ; sur
  un kick, montée immédiate vers la couleur pleine (`intensite`, 1) puis retombée en 0,35 s.
  Une seule fenêtre colorée : l'ancienne retrouve la couleur du système au changement de premier
  plan, à la pause, en plein écran et à la fermeture (plantage compris ; la fenêtre colorée est
  notée dans `bordure.hwnd` pour le cas où Essaim est tué ; ce témoin n'est réécrit que pour une
  nouvelle fenêtre colorée, et le hook de plantage ne rend la bordure que si c'est le fil
  principal qui plante). Ignorées : fenêtres sans cadre, outils
  (`WS_EX_TOOLWINDOW`), barre des tâches, bureau, fenêtres d'Essaim, plein écran. Couleur
  quantifiée, envoyée seulement si elle change (60 fois par seconde au plus) ; une fenêtre qui
  refuse (appli administrateur) est notée une fois au journal. Essai réel :
  `cargo test --release bordure_reelle -- --ignored --nocapture` avec un Bloc-notes ouvert.
- **Plein écran** (jeu, vidéo) sur l'écran de la souris : tout se met en veille et la flèche
  d'origine revient. Compte comme plein écran une fenêtre qui couvre l'écran (cadre lu par
  `DWMWA_EXTENDED_FRAME_BOUNDS`) sans être agrandie ni avoir de barre de titre ou de bord de
  redimensionnement : une fenêtre agrandie, barre des tâches masquée, ne compte pas. En pause et
  en plein écran, la capture du son est arrêtée (sauf fenêtre des réglages ouverte).
- **Clics** : lus par `GetAsyncKeyState` à chaque image (pas de hook global).
- **Écrans** : l'essaim est dessiné sur chaque écran qu'il touche (la traînée passe la
  frontière) ; une traînée laissée à plus de 1200 px par un saut du pointeur est abandonnée
  (sinon elle resterait figée sur l'écran quitté). Calques refaits quand la configuration des
  écrans change (`WM_DISPLAYCHANGE`).
- **Kick** : un seul par coup, même long et ondulant (type 808) : un kick est une attaque, et deux
  attaques doivent être séparées d'au moins `[audio] delai_kick` (0,1 s ; la double pédale du metal
  descend vers 0,08 s).
- **Repos** : souris immobile et pas de musique depuis 2 s → `[rendu] cadence_repos` (30 i/s).
  **Veille** : rien de neuf depuis `[rendu] veille_apres` (60 s) → fondu de 1 s, puis plus aucun
  dessin ; souris, son et clics restent lus toutes les 40 ms (réveil en 0,25 s au plus).
- **Carte graphique** : en cas d'erreur, bascule sur le processeur et nouvel essai 10 s plus
  tard, puis toutes les 30 s.
- **Pauses automatiques** (`pauses.rs`) : `SHQueryUserNotificationState` chaque seconde (pause
  sur `QUNS_BUSY`, `QUNS_RUNNING_D3D_FULL_SCREEN`, `QUNS_PRESENTATION_MODE` ; pas
  `QUNS_QUIET_TIME`, première heure d'un nouveau compte), `GetSystemPowerStatus` toutes les 5 s
  ou sur `WM_POWERBROADCAST`, exécutable au premier plan chaque seconde. Traitées comme le plein
  écran (bordure rendue, son coupé), notées au journal ; raison publiée (`charge::publier_raison`).
- **Qualité adaptative** (`adaptatif.rs`) : nourrie chaque seconde par le calcul moyen par
  image ; > 4 ms deux secondes de suite → palier suivant (120, 60, 30 ; ceux qui ne réduiraient
  rien sont sautés), < 1,5 ms dix secondes → palier précédent.
- **Masquer des captures** : `SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)` sur chaque
  calque (`win::masquer_captures`). Les essais par capture `BitBlt` ne voient alors plus l'essaim.
- **Projecteur, réticule, spectre** (`effets.rs`) : le réticule et le spectre s'ajoutent à la
  zone de l'essaim ; le projecteur ajoute l'écran entier du pointeur aux zones à redessiner, le
  temps de l'effet seulement (voile : dégradé radial Direct2D, ou remplissage ligne par ligne sur
  le processeur). Secousse : trois changements de sens horizontaux francs (allers ≥ 60 px,
  < 0,35 s, plutôt horizontaux) en moins de 0,6 s, lus sur les positions de chaque image.
- **Morceau en cours** (`media.rs`) : `GlobalSystemMediaTransportControlsSessionManager` sur le
  fil « essaim-media » (MTA, appels asynchrones attendus par `join`), réveillé par
  `CurrentSessionChanged`, `MediaPropertiesChanged` et `PlaybackInfoChanged`, et au moins
  toutes les secondes. Miniature décodée en 48 × 48 (`BitmapDecoder`) puis
  `appli::teinte_dominante`. Priorité des teintes : pochette (pendant la lecture) > application
  ou site > Prisme. Sans l'API (Windows N), une ligne au journal et rien ne change. Message
  `charge` : `morceau`, `lecture`, `pistes`.

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
(pas de gain, bugs connus). Banc `mesure_fissures` (2026-11-19) : 12 fissures sur processeur,
2,7 → 1,1 ms par image depuis que leurs passes fixes sont précalculées. Effets (banc
`mesure_effets`) : réticule et spectre négligeables ; projecteur, pendant l'effet seulement,
+0,13 ms par image avec la carte graphique, +9 ms avec le processeur (envoi de l'écran entier).

## Analyse du son (audio-commun, `src/kick.rs`)

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
- **Côté Essaim** (`src/audio.rs`) : échantillons non finis remis à 0 avant tout filtre ;
  compteurs de kicks et d'aigus monotones (une réouverture du flux ne crée pas de faux kicks) ;
  brillance qui retombe en silence ; si la capture ne démarre pas, Essaim continue sans son et
  réessaie toutes les 30 s.
- **Tempo** (`audio-commun`, `src/pulsation.rs`, sans FFT) : d'après les seuls instants des
  kicks des 8 dernières secondes. Chaque tempo de 70 à 180 BPM est noté par l'alignement des
  kicks sur sa grille × la part des temps marqués (ce produit écarte le double et la moitié du
  tempo), puis la grille est ajustée par moindres carrés. Tempo annoncé seulement si les kicks
  sont réguliers (8 au moins, 80 % sur la grille, écart type sous 4 % de la période, 60 % des
  temps marqués) et après 3 estimations concordantes ; gardé pendant un break tant que la
  musique joue (30 s au plus), oublié après 3 s de silence. Relais : tempo connu, musique, aucun
  kick depuis 2 s → `pulsation_tempo` (éclat 0,35 × (1 − phase)⁴).
- **Sortie écoutée** : `capture::start_sur` d'`audio-commun` (sortie par défaut ou identifiant
  Windows) ; si la sortie choisie disparaît, repli sur la sortie par défaut, et retour sur elle
  dès qu'elle revient (vérification toutes les 2 s).
- **Bandes du spectre** (`src/audio.rs`, `Bandes`) : seulement si `[spectre] actif` ; 8 à 32
  passe-bande (deux biquads f32 en série chacun) répartis en échelle log de 40 Hz à 12 kHz,
  enveloppes 5 ms / 120 ms, niveaux en dB sous le pic récent (plage 36 dB). 0,9 % d'un cœur à
  32 bandes.
- Limites connues : une note de basse tenue ou une voix grave qui démarre sèchement peut encore
  compter comme un kick (la distinguer demanderait d'écouter plus de 30 ms) ; sur la batterie
  acoustique très propre, l'ancien détecteur faisait un peu mieux (F1 0,93 contre 0,84).

## Interface : technique et charte

- La fenêtre est une page web (`ui/`, intégrée à l'exécutable) affichée par WebView2 (fourni
  avec Windows 10 et 11) via la crate `wry`. Elle n'a accès qu'à ses fichiers et à
  `raw.githubusercontent.com` (index de la bibliothèque et images d'aperçu) ; elle ne parle à l'appli que par messages (`src/reglages.rs`).
- Charte graphique « Essaim » : `ui/style.css` en est la référence (jetons clair/sombre,
  typographie, composants). Une seule police, Archivo (Omnibus-Type, licence SIL OFL 1.1,
  `ui/polices/OFL.txt`).
- `ui/apercu.js` reproduit `render.rs` jusqu'au détail : ordre de la mise à jour, points
  intermédiaires (`sous_pas`), sillage, dessin des traînées par morceaux de 4 segments.
- Réglages (`config.rs`) : écritures atomiques (fichier `.tmp` puis renommage) sous un verrou
  unique partagé par la fenêtre, Horizon et le preset automatique ; un `config.toml` illisible
  n'est jamais écrasé (seul un fichier absent est recréé) ; relecture sur date et taille, forcée
  après chaque écriture de l'appli. Fenêtre réduite : vue cachée et envoi du son arrêté. Le
  message `charge` porte aussi `capture` (`active`, `coupee`, `erreur`) et `veille`.

## Mises à jour et publication

`src/maj.rs` : fil « mise à jour », WinHTTP (proxy et certificats du système, aucune dépendance
de plus). Il lit `releases/latest` de l'API GitHub, compare le tag à `CARGO_PKG_VERSION`,
télécharge l'élément `essaim.exe` (adresse limitée à `github.com/KingsRequin/essaim/releases/download/`),
vérifie l'en-tête `MZ` et l'empreinte `digest` (SHA-256 par `BCryptHash`), puis renomme
l'exécutable en cours en `.ancien` et met le nouveau à sa place (Windows autorise le renommage
d'un exécutable lancé). Redémarrer : la boucle s'arrête, la bordure est rendue, le verrou
d'instance unique est fermé, puis le même chemin est relancé. Une compilation de travail (debug)
ne vérifie rien.

Publier une version : monter `version` dans `Cargo.toml`, commiter, puis pousser un tag
`vX.Y.Z` identique. Le workflow `.github/workflows/publication.yml` (Windows) vérifie le tag,
lance les tests, compile, et crée la version GitHub avec `essaim.exe` (nom attendu par les
mises à jour) et `curseur-Dotter.zip`.

## Développement

`cargo build --release` suffit : l'analyse du son vient de la bibliothèque
[audio-commun](https://github.com/KingsRequin/audio-commun) (tag `v0.2.0`), partagée avec Horizon.
Pour la modifier en même temps, la cloner à côté et la désigner dans `.cargo/config.toml` (non
suivi par git) :

```toml
[patch."https://github.com/KingsRequin/audio-commun"]
audio-commun = { path = "../audio-commun" }
```

Le même fichier peut fixer le dossier de compilation (`[build] target-dir`). Consignes de
travail : `CLAUDE.md`.

### Images du README

`python scripts/images-readme/rendre.py` refait les animations de `docs/images/` : les aperçus de
`ui/apercu.js` (calculés comme l'essaim), rendus dans Edge sans fenêtre sur un son simulé à
120 BPM, avec les réglages des presets fournis (test `exporter_presets`). Les captures de la
fenêtre (`presets.png`, `reglages.png`, `curseurs.png`) sont faites à la main, fenêtre en
1440 × 900.
