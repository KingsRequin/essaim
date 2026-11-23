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
  un déclencheur `bordure` (au kick par défaut), montée immédiate vers la couleur pleine (sa
  `force`, 1) puis retombée en 0,35 s. La bordure écoute le son elle-même (`render::Ecoute`,
  `Border::reagir`, avec délai et chance) : elle s'illumine aussi pendant la frappe, quand
  l'essaim est masqué.
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
- **Trajet d'une image** (`contours.rs`) : valeur de forme de chaque pixel (l'opacité si l'image a
  de la transparence, sinon la luminosité, du côté du seuil d'Otsu qui n'est pas le fond que
  montrent les bords de l'image ; le niveau est le milieu des moyennes des deux classes), flou
  1-2-1, puis isocontour au niveau entre les centres des pixels (marching squares, cols tranchés
  par la moyenne de la cellule, marge hors de la forme autour de l'image) : le bord est placé au
  sous-pixel, les côtés droits restent droits. Les contours sont orientés : les trous (aire de
  l'autre signe) et les formes de moins de 0,025 % de l'image sont écartés, les plus grandes
  d'abord, 24 au plus ; simplifiés (Ramer-Douglas-Peucker sur contour fermé, ¼ px), 512 points au
  plus, sans lissage (les coins restent des coins). La page envoie les pixels (`cmd: "contours"`,
  image ramenée à 512 px ; un texte ou un emoji est dessiné dans un canevas, cadré sur son encre)
  et lit un fichier choisi en adresse `data:` (la page refuse les `blob:`, CSP de `reglages.rs`) ;
  une image glissée sur la fenêtre est lue par `reglages.rs` et renvoyée à la page. Le moteur
  (`render::set_traces`) met les contours à la même échelle autour du milieu de leur cadre et les
  parcourt à l'abscisse curviligne (sommets gardés, coins atteints exactement) ; chaque image est
  découpée en pas selon la longueur du contour. Avec plusieurs contours, un anneau par contour
  (quel que soit `anneaux`, sans le rétrécissement des anneaux), le satellite suit le contour de
  sa couleur. `forme = "icone"` : `appli::IconeAppli` lit l'icône
  (`PrivateExtractIconsW`, 128 px) de la dernière application au premier plan qui n'est pas Essaim,
  une fois par exécutable.
- **Inertie** (`[orbite] inertie`) : le centre de l'orbite est un ressort amorti vers le pointeur
  (pulsation 14 → 4,9 rad/s, amortissement 0,45, Euler semi-implicite, stable aux pas de 0,05 s),
  retard borné à 30 px × (1 + 5 × inertie) ; à 0, l'ancien suivi (lissage à 14/s).
- **Sans flashs** (`[rendu] sans_flashs`) : `auto` lit `SPI_GETCLIENTAREAANIMATION` chaque seconde
  (`win::sans_flashs`). L'éclat ne remonte qu'à 0,5 et au plus toutes les 1/3 s (`Swarm::eclater`),
  sans halo blanc, quel que soit le déclencheur (toutes les actions `eclat`, `explosion`,
  `rafale` passent par là) ; ondes, carrés et implosions à 45 % ; bordure sans illumination (ses
  déclencheurs ne font rien).
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
  `QUNS_QUIET_TIME`, première heure d'un nouveau compte ; `QUNS_BUSY` seulement si une vraie
  fenêtre couvre un écran, `win::plein_ecran_quelque_part` : Windows le renvoie aussi pour le
  calque transparent d'une autre surcouche), `GetSystemPowerStatus` toutes les 5 s
  ou sur `WM_POWERBROADCAST`, exécutable au premier plan chaque seconde. Traitées comme le plein
  écran (bordure rendue, son coupé), notées au journal ; raison publiée (`charge::publier_raison`).
- **Qualité adaptative** (`adaptatif.rs`) : nourrie chaque seconde par le calcul moyen par
  image ; > 4 ms deux secondes de suite → palier suivant (120, 60, 30 ; ceux qui ne réduiraient
  rien sont sautés), < 1,5 ms dix secondes → palier précédent.
- **Masquer des captures** : `SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)` sur chaque
  calque (`win::masquer_captures`). Les essais par capture `BitBlt` ne voient alors plus l'essaim.
- **Fenêtre pour le stream** (`stream.rs`) : fenêtre ordinaire (classe `EssaimStream`), son
  propre fil, son propre `Swarm` et ses `Effets`. Affichage : périphérique Direct3D 11 (carte
  graphique, sinon WARP), chaîne d'échange DXGI à retournement (`FLIP_DISCARD`, présentée à chaque
  rafraîchissement), dessin Direct2D dans son tampon avec le `Pinceau` de `gpu.rs` (lot de
  sprites). Le tampon garde la transparence, prémultipliée (fond `"transparent"`, effacé à 0) : la
  *Capture de jeu* d'OBS le reprend sur la carte graphique (*Autoriser la transparence*, *Valeurs
  alpha prémultipliées*) ; DWM l'affiche sans transparence, sur noir. Essai réel relu sur la carte
  graphique : `cargo test --release image_transparente_relue -- --ignored`.
  Taille : l'essaim est simulé à 100 % autour de (0, 0) puis agrandi par la transformation
  Direct2D (petit côté / 2 − 4 px) / portée. La portée = plus grand demi-côté de `Swarm::bounds`
  (et du réticule, du spectre) sur 12 s de musique synthétique (kicks, caisses claires, montée,
  drop, changement de piste), mesurée à l'ouverture et à chaque changement de `config.toml`
  (1 à 10 ms) ; si la vraie musique pousse plus loin, la portée vue en direct (oubliée en ~20 s)
  prend le relais : l'essaim rapetisse en ~0,1 s, regrandit en ~2 s. Test : chaque preset fourni
  tient dans sa portée sur une autre musique. Elle relit `config.toml` chaque
  seconde (`config::Watched`) et lit le son par le même `AudioShared` ; la capture du son continue
  tant qu'elle est ouverte. Pas de `WDA_EXCLUDEFROMCAPTURE` : elle reste capturable.
- **Projecteur, réticule, spectre** (`effets.rs`) : le réticule et le spectre s'ajoutent à la
  zone de l'essaim ; le projecteur ajoute l'écran entier du pointeur aux zones à redessiner, le
  temps de l'effet seulement (voile : dégradé radial Direct2D, ou remplissage ligne par ligne sur
  le processeur). Secousse : trois changements de sens horizontaux francs (allers ≥ 60 px,
  < 0,35 s, plutôt horizontaux) en moins de 0,6 s, lus sur les positions de chaque image. Ces
  seuils sont ceux de `[projecteur] sensibilite = 5` (`effets::secousse_exigence`) : la course
  minimale double tous les 5 crans en moins (104 px à 1, 30 px à 10), les deux durées
  s'allongent comme l'inverse de sa racine (0,49 s et 0,85 s à 10). `secousse` et `sensibilite`
  sont hors presets (`profils::in_profile`, `HORS_PRESET` dans `ui/app.js`).
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
bornées ; par exemple `bordure`/`repos`), `list_presets` (presets fournis puis ceux de
l'utilisateur : `name`, `builtin`, `active`, relus à chaque demande), `apply_preset` (`name` :
comme le menu de l'icône, curseurs du preset compris), `open_settings` (fenêtre des réglages,
ouverte par la boucle principale), `open_config` ; événement `state` à chaque changement et au
moins toutes les 2 s, avec `preset` : le nom du preset en place, `null` si les réglages ne sont
ceux d'aucun (recherché à chaque relecture de `config.toml` ; un preset enregistré ou supprimé
sans autre changement ne le fait pas bouger, `list_presets` reste juste).
Clés d'avant les déclencheurs, encore lues et écrites par Horizon : `orbite.vitesse_basse` et
`orbite.eloignement_grave` (les modulations de base basse → vitesse et grave → rayon, calculées à
la publication, réécrites, ajoutées ou retirées par `set_config`) et `fissures.actives` (le
déclencheur clic → fissure) ; `ipc::config_publiee`, `ALIAS`.
Toute connexion avec un en-tête `Origin` est refusée (pages web, mais aussi applis web locales :
Tauri, Electron) ; 8 clients au plus, 3 s pour la poignée de main, messages de 64 Ko au plus,
client fermé après 60 s sans message. `set_config` refuse les sections `mise_a_jour`,
`bibliotheque`, `automatique` et `pause` (elles ne changent que par la fenêtre des réglages ou
le fichier). Le plugin Essaim d'Horizon s'en sert (pause au clic ; dans son panneau, choix du
preset et ouverture des réglages).

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

- **Kick** : 10 bandes (biquads, pas de FFT ; celles sous 4 kHz sur le son sous-échantillonné)
  suivies tous les 5 ms ; chaque pic de montée d'énergie est jugé par un petit réseau appris sur de
  vrais morceaux, 10, 20 ou 30 ms après l'attaque selon sa netteté (délai médian mesuré :
  20–30 ms). Le réseau lit la forme de l'attaque et le contexte : l'état d'un petit réseau
  récurrent (GRU) qui écoute les bandes en continu et retient le rythme, les coups récents et la
  nature du son (musique, voix seule, basse tenue) ; il est oublié après 2 s de silence.
  `sensibilite_kick` règle le dernier seuil ; sans kick depuis 4 s (intro, montée, reprise après
  une pause), le premier doit être franc. Détails et banc d'essai : README d'`audio-commun`,
  section « Kick et caisse claire ». En validation croisée (morceaux jamais appris) : F1 0,89 sur
  107 morceaux d'électro, dubstep, hardstyle, rap, pop et rock, 0,88 sur 93 morceaux d'autres
  genres, 0,84 sur 277 morceaux de 42 styles (tableau par style dans ce README), 0,78 sur MDB
  Drums ; sans le contexte (versions jusqu'à 0.9.0), 0,84, 0,79 et 0,64 sur les trois jeux
  d'alors. Le seuil de silence suit le niveau récent (52 dB sous le pic, entre -100 et -60 dB) :
  un lecteur réglé 48 dB plus bas donne encore 0,90 là où le seuil fixe d'avant la 0.10.0 donnait
  0,75. Le premier détecteur, qui n'écoutait que le sub, avait 50 à 150 ms de
  retard en électro, où le sub arrive après l'attaque. Calcul : 0,26 % d'un cœur.
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
  kick depuis 2 s → événement `break` sur chaque temps (par défaut : éclat de force 0,35).
- **Caisse claire** (et clap ; `audio-commun`, `kick::Analyzer::caisses`) : les mêmes attaques
  que le kick, jugées par trois réseaux à elle, appris sur les mêmes morceaux (caisses claires
  transcrites par ADTOF, annotations de MDB Drums). Une attaque peut être un kick et une caisse
  claire à la fois. 50 ms au moins entre deux. En validation croisée : F1 0,78 sur les 107
  morceaux, 0,82 sur les 93 autres, 0,76 sur les 277 de 42 styles, 0,59 sur MDB Drums (0,68, 0,70
  et 0,39 sans le contexte sur les trois jeux d'alors) ;
  délai médian 20 ms. Elle remplace un détecteur à seuils (bande à 2 kHz, enveloppes rapide et
  lente, tenue à 25 ms) qui comptait aussi les kicks distordus du hardstyle. `Musique` lit le compteur de l'analyse du kick après chaque paquet ; elle
  ne calcule plus elle-même que la brillance de la montée (`Eclat`).
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
  Calculé seulement si une modulation (sources caisse, montée, drop) ou un déclencheur (caisse,
  drop) l'écoute (`Config::analyse_musicale`, `AudioShared::set_musique`).
- **Stéréo** : la capture d'`audio-commun` remet après chaque paquet le niveau efficace de chaque
  canal (`Sink::stereo`, depuis le commit qui suit la v0.2.0). Équilibre = (D − G) / (D + G), lissé
  sur 0,25 s de son audible ; les paquets muets (blancs entre deux coups) ne le changent pas, et il
  revient au centre après 0,5 s de silence. Essai réel : groove joué tout à droite → +1,00.
- **Temps, contretemps** (`render::Ecoute`) : la phase du tempo des kicks (`pulsation`) qui
  repasse par 0 (temps), ou passe 0,5 (contretemps).
- **Sortie écoutée** : `capture::start_sur` d'`audio-commun` (sortie par défaut ou identifiant
  Windows) ; si la sortie choisie disparaît, repli sur la sortie par défaut, et retour sur elle
  dès qu'elle revient (vérification toutes les 2 s).
- **Bandes du spectre** (`src/audio.rs`, `Bandes`) : seulement si `[spectre] actif` ; 8 à 32
  passe-bande (deux biquads f32 en série chacun) répartis en échelle log de 40 Hz à 12 kHz,
  enveloppes 5 ms / 120 ms, niveaux en dB sous le pic récent (plage 36 dB). 0,9 % d'un cœur à
  32 bandes.
- Limites connues : sur un son sans batterie (voix seule, piano, orgue, contrebasse), il reste
  environ un faux kick toutes les deux minutes (dix par minute avant le contexte) ; le kick
  effleuré du jazz acoustique reste mal reconnu ; les notes fantômes de caisse claire de la
  batterie acoustique sont souvent manquées (rappel 0,68 sur MDB Drums).

## Interface : technique et charte

- La fenêtre est une page web (`ui/`, intégrée à l'exécutable) affichée par WebView2 (fourni
  avec Windows 10 et 11) via la crate `wry`. Elle n'a accès qu'à ses fichiers et à
  `raw.githubusercontent.com` (index de la bibliothèque et images d'aperçu) ; elle ne parle à l'appli que par messages (`src/reglages.rs`).
- Charte graphique « Essaim » : `ui/style.css` en est la référence (jetons clair/sombre,
  typographie, composants). Une seule police, Archivo (Omnibus-Type, licence SIL OFL 1.1,
  `ui/polices/OFL.txt`).
- **Déclencheurs** (`[declencheurs]`, `render.rs`) : `Ecoute::lire` donne les événements du son
  depuis l'image précédente (`Coups` : kicks, caisses claires, drops, attaques des aigus, au plus 4
  chacun ; temps, break, contretemps) ; `Swarm::update` les passe à `declencher` dans un ordre
  fixe (kicks, retombée de l'éclat, temps et break, caisse claire, drop, contretemps, aigus), et
  la boucle principale passe les événements de la souris et du morceau (`Swarm::evenement` :
  clic, clic droit, piste). Chaque déclencheur de l'événement agit dans l'ordre de la liste, après
  son délai minimal ; sa chance n'est tirée que si elle est sous 1 (les presets d'avant gardent
  ainsi la même suite de tirages : satellites et particules identiques, essai
  `presets_comme_avant`). `agir` fait l'action ; une fissure est demandée à la boucle principale
  (`Swarm::fissures`), la bordure suit ses déclencheurs elle-même.
- **Modulations** (`modulation.rs`) : `Sources` tient les enveloppes des coups (kick, caisse
  claire, charleston, contretemps : 1 au coup, retombée de 7, 8, 10 et 10 par seconde ; drop :
  0,9 par seconde) et suit la montée. Pour chaque modulation, `visee` donne quantité × source
  (seuil, à l'envers), lissée à son rythme dans `Swarm::update` (`part_lissage` : constante de
  temps 0,4 × lissage² s) ; `effets` en fait l'effet sur les comètes et sur les satellites
  (`Effet` : tours/min de rotation, radians/s de vitesse, degrés de teinte et rayons de décalage
  additionnés ; accélération, rayon, taille, lueur, traînée et opacité multipliés) et la teinte
  « sur tout », que la boucle principale passe à `render::set_decalage` (tout ce qui est dessiné
  la prend, bordure comprise). L'angle du trajet de chaque groupe s'accumule ; quand sa rotation
  (réglage plus modulations) retombe sous 2 tours/min, il revient vers 0 par le plus court
  (`render::pas_rotation`, taux 3/s pondéré par la lenteur), sinon une modulation retirée ou un
  drop retombé le laisseraient de travers. La vitesse du trajet de chaque groupe est lissée (~100 ms) vers la
  vitesse de base × son accélération. Chaque groupe a sa `Pose` (rotation, rayon et décalage au
  début et à la fin de l'image) : le trajet des comètes et celui des satellites tournent chacun
  de leur côté (`[orbite] rotation_sur`). Une modulation de la taille élargit la marge de
  `Swarm::bounds`.
- **Réglages d'avant** (`migration.rs`) : les réactions fixes d'Essaim 0.3 et 0.4 (`[musique]`,
  `[clic]`, `[scintillement]`, `[fissures] actives`, `[bordure] intensite`, `[couleur] musique`,
  `[audio] pulsation_tempo`, `[orbite] vitesse_basse`, `eloignement_grave`, `inverse_au_kick`,
  `[essaim] par_kick_*`) sont lues dans `Ancien` (avec leurs valeurs par défaut d'alors) et
  deviennent des modulations et des déclencheurs : à la lecture (`config::de_texte`, presets
  compris), et dans `config.toml` une fois au lancement (`config::migrer_fichier`, commentaires
  gardés). Sans `[declencheurs]`, les réactions d'avant deviennent les déclencheurs et s'ajoutent
  aux modulations ; sans `[modulations]` (0.3), la basse aussi. Les réglages par défaut en
  sortent (`Ancien::default`). La page fait de même pour un preset de la communauté
  (`reactionsAvant`, `fusion`). Essai `presets_comme_avant` : chaque preset fourni jusqu'à la
  0.6 (`prereglages::presets_d_avant`, écrits avec les réglages d'avant) contre les mesures de l'ancien moteur (`tests/donnees/rendu-presets.json`, même
  son d'essai) : sens, satellites et particules identiques, distance des têtes à 8 % près par
  seconde (0,1 % en médiane).
- `ui/apercu.js` reproduit `render.rs` jusqu'au détail : ordre de la mise à jour (écoute des
  événements, déclencheurs, modulations lissées une à une, vitesse accélérée par groupe), points
  intermédiaires (`sous_pas`), sillage, dessin des traînées par morceaux de 4 segments ; seules
  les fissures y sont plus simples (quelques branches brisées). Vérifié hors navigateur contre le
  moteur : même sens de rotation et distance des comètes à 5 % près, preset par preset.
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

L'avancement tient dans `maj::Etat` (jamais, recherche, à jour, prête, échec), que la fenêtre
des réglages envoie à la page quand il change (message `maj`, vue Application). Son bouton
*Chercher une mise à jour* (`chercher_maj` → `maj::chercher`) lance la même recherche sur un
fil à part, même si `[mise_a_jour] auto` est coupé ; une seule recherche à la fois (`EN_COURS`),
et plus aucune une fois qu'une version attend le redémarrage. *Redémarrer* (`redemarrer`) envoie
`CMD_REDEMARRER` à la boucle principale, comme le menu de l'icône.

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
`v0.5.0` : `rev` dans `Cargo.toml`), partagée avec Horizon.
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
120 BPM, avec les réglages des presets fournis (test `exporter_presets`). `stream.png` est la
vraie image de la fenêtre pour le stream (Direct2D, test `exporter_image_stream`) posée sur un damier
(`rendre.py stream` ne refait qu'elle). Les captures de la
fenêtre (`presets.png`, `reglages.png`, `curseurs.png`) sont faites à la main, fenêtre en
1440 × 900.
