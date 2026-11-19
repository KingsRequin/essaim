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
- **Clics** : lus par `GetAsyncKeyState` à chaque image (pas de hook global), bouton principal et
  bouton secondaire (clic droit) ; boutons inversés (`SM_SWAPBUTTON`) : le principal est le droit.
- **Rebonds** : cadre affiché de la fenêtre au premier plan (`DWMWA_EXTENDED_FRAME_BOUNDS`, relu
  toutes les 250 ms avec le plein écran ; ni bureau, ni barre des tâches, ni fenêtre réduite, en
  plein écran ou d'Essaim). Une étincelle ou un grain de poussière qui traverse un bord entre deux
  pas est réfléchi (vitesse × 0,7).
- **Inertie** (`[orbite] inertie`) : le centre de l'orbite est un ressort amorti vers le pointeur
  (pulsation 14 → 4,9 rad/s, amortissement 0,45, Euler semi-implicite, stable aux pas de 0,05 s),
  retard borné à 30 px × (1 + 5 × inertie) ; à 0, l'ancien suivi (lissage à 14/s).
- **Sans flashs** (`[rendu] sans_flashs`) : `auto` lit `SPI_GETCLIENTAREAANIMATION` chaque seconde
  (`win::sans_flashs`). L'éclat ne remonte qu'à 0,5 et au plus toutes les 1/3 s (`Swarm::eclater`),
  sans halo blanc ; ondes, carrés et implosions à 45 % ; bordure sans illumination
  (`bordure::sans_flashs`, `intensite` = 0).
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
- **Fenêtre pour le stream** (`stream.rs`) : fenêtre ordinaire (classe `EssaimStream`), son
  propre fil, son propre `Swarm` et ses `Effets`, dessinés au processeur (tiny-skia) au centre d'une
  image de la taille de la zone cliente (échelle = petit côté / 205), sur fond uni, puis envoyés
  par `SetDIBitsToDevice` à chaque rafraîchissement (`DwmFlush`). Elle relit `config.toml` chaque
  seconde (`config::Watched`) et lit le son par le même `AudioShared` ; la capture du son continue
  tant qu'elle est ouverte. Pas de `WDA_EXCLUDEFROMCAPTURE` : elle reste capturable.
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
Toute connexion avec un en-tête `Origin` est refusée (pages web, mais aussi applis web locales :
Tauri, Electron) ; 8 clients au plus, 3 s pour la poignée de main, messages de 64 Ko au plus,
client fermé après 60 s sans message. `set_config` refuse les sections `mise_a_jour`,
`bibliotheque`, `automatique` et `pause` (elles ne changent que par la fenêtre des réglages ou
le fichier). Le widget Essaim d'Horizon s'en sert depuis son côté Rust (pause au clic, réglages
en direct dans son panneau).

## Consommation (mesurée le 2026-11-14)

Banc d'essai `cargo test --release mesure_moteurs -- --ignored --nocapture` (essaim plein,
simulation comprise) : **0,35 ms par image** avec la carte graphique, 1,09 ms avec le
processeur ; 0,31 ms et 1,10 ms le 2026-11-19, après les effets de la musique (caisse claire,
montée et drop, palette, rebonds…). Pistes écartées après mesure : compilation AVX2 (aucun gain, le coût de tiny-skia
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
- **Caisse claire** (`src/audio.rs`, `Caisse`, sans FFT) : deux passe-bande en série centrés sur
  2 kHz (bande 1–4 kHz), enveloppe rapide (3 ms) contre lente (100 ms). Attaque : rapide > 2,5 ×
  lente, > 0,004, et > 0,4 × l'enveloppe rapide de la bande 8–12 kHz (le charleston et les cymbales,
  presque tout dans l'aigu, sont écartés). Elle n'est comptée que si elle tient 25 ms plus tard
  (encore 30 % de son pic) : le clic d'attaque d'un kick s'éteint en quelques millisecondes, le bruit
  d'une caisse claire non ; une caisse claire jouée avec un kick compte donc. 50 ms au moins entre
  deux. Limite connue : un kick très distordu (hardstyle), riche et tenu dans les médiums, peut
  compter aussi.
- **Montée et drop** (`Montee`) : tension = 0,75 × accélération + 0,45 × éclaircie, bornée à 1,
  lissée (montée 0,6 s, retombée 1,5 s). Accélération : caisses claires par seconde sur ~1 s face à
  celles sur ~12 s (au moins 1,5), écart / 5. Éclaircie : brillance (bande 8–12 kHz face au son
  entier, en dB : −40 dB → 0) sur 0,5 s face à 8 s, écart / 0,12. Drop : premier kick après au moins
  1,2 s sans kick quand la tension a dépassé 0,45, ou retour de l'énergie (niveau sur 0,5 s face au
  niveau sur 12 s, comme `spectre` : intensité < 0,32 puis > 0,66 dans les 4 s), au plus un toutes
  les 8 s ; la tension retombe alors à 0. Puissance lissée sur 85 ms ; niveau long = moyenne de tout
  ce qui a été entendu pendant les 12 premières secondes (une moyenne glissante partie d'un mauvais
  début dérivait de 2,5 dB) ; silence = 250 ms sous −60 dBFS. Tests synthétiques : groove régulier
  sans montée ni drop, montée (roulement de 4 à 16 caisses par seconde) puis un seul drop, drop
  d'énergie après un passage 16 dB plus bas, caisses claires trouvées sans celles des kicks ; essai
  réel `cargo test --release musique_reelle -- --ignored --nocapture` (un morceau joué pendant 24 s).
  Calculé seulement si `[musique] montee` ou `caisse_claire` (`AudioShared::set_musique`).
- **Stéréo** : la capture d'`audio-commun` remet après chaque paquet le niveau efficace de chaque
  canal (`Sink::stereo`, depuis le commit qui suit la v0.2.0). Équilibre = (D − G) / (D + G), lissé
  sur 0,25 s de son audible ; les paquets muets (blancs entre deux coups) ne le changent pas, et il
  revient au centre après 0,5 s de silence. Essai réel : groove joué tout à droite → +1,00.
- **Contretemps** (`render.rs`) : la phase du tempo des kicks (`pulsation`) qui passe 0,5.
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
vérifie l'en-tête `MZ`, l'empreinte `digest` (SHA-256 par `BCryptHash`) et la signature
`essaim.exe.sig` : ECDSA P-256 (r puis s, 64 octets) sur « essaim.exe <version> <sha256> », par
l'une des deux clés publiques de `CLES` (principale, secours), vérifiée par BCrypt. Sans
signature valide, rien n'est installé : GitHub seul (compte, workflow, dépendance de
compilation piégée) ne peut pas pousser un exécutable chez les utilisateurs. Il renomme ensuite
l'exécutable en cours en `.ancien` et met le nouveau à sa place (Windows autorise le renommage
d'un exécutable lancé). Redémarrer : la boucle s'arrête, la bordure est rendue, le verrou
d'instance unique est fermé, puis le même chemin est relancé. Une compilation de travail (debug)
ne vérifie rien.

Publier une version :

1. monter `version` dans `Cargo.toml`, commiter, puis pousser un tag `vX.Y.Z` identique ;
2. le workflow `.github/workflows/publication.yml` vérifie le tag, lance les tests, compile
   (job sans droit d'écriture ni jeton conservé), puis un second job, qui ne compile rien, crée
   la version **en brouillon** avec `essaim.exe` (nom attendu par les mises à jour) et
   `curseur-Dotter.zip` ;
3. sur le PC de l'auteur, `pwsh scripts\signer-version.ps1` vérifie que le tag de GitHub est
   le commit local, recompile ce commit dans une copie propre (sans le `[patch]` local), signe
   cet exe et le dépose à la place de celui du brouillon avec `essaim.exe.sig`, puis publie.
   L'exe compilé par GitHub ne sert qu'à valider la version : il n'est jamais signé.

Clés (`scripts\creer-cles-signature.ps1`, une seule fois) : la principale est dans
`%USERPROFILE%\.essaim-signature\principale.cle`, chiffrée par DPAPI pour la session Windows de
l'auteur ; la clé de secours est rangée hors du PC. Si la principale est perdue : signer avec
`-Secours <fichier>` une version dont `CLES` contient une nouvelle clé principale.

## Développement

`cargo build --release` suffit : l'analyse du son vient de la bibliothèque
[audio-commun](https://github.com/KingsRequin/audio-commun) (figée par commit, celui du tag
`v0.2.0` : `rev` dans `Cargo.toml`), partagée avec Horizon.
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
