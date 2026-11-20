# Guide d'Essaim

Tout ce que fait Essaim, en détail. Présentation : [README](../README.md) ; fonctionnement interne : [technique.md](technique.md).

## Ce que fait l'essaim

Surcouche réactive à la musique autour du pointeur de Windows : des comètes et un essaim de
satellites en orbite, qui réagissent au son et à la souris. Le pointeur lui-même reste celui de
Windows (voir « Curseurs »). Tout ce qui réagit passe par deux listes, gardées dans le preset :

- les **déclencheurs** : quand un événement arrive (un kick, une caisse claire, un drop, un
  clic…), une action (un éclat, une onde, des satellites, une fissure…) ;
- les **modulations** : un élément du son (la basse, la note grave, la montée…) fait varier un
  paramètre en continu (la vitesse, le rayon, la rotation du trajet, la teinte…).

Par défaut, elles font les réactions de toujours : chaque kick inverse le sens, fait un éclat
sous le pointeur, ajoute 2 à 3 satellites et illumine la bordure ; la basse accélère, une note
grave éloigne les orbites ; une montée resserre l'essaim et l'accélère, le drop le fait exploser ;
la caisse claire ouvre un carré, le charleston fait jaillir des étincelles des têtes, un break
continue de pulser au tempo, un changement de piste lance une rafale ; un clic fait une onde et une
fissure, un clic droit une onde qui se referme.

### Ce qu'Essaim entend

- **Kick** : reconnu par un petit modèle (`audio-commun`), en 25 ms environ.
- **Caisse claire** (et clap) : bande à 2 kHz qui l'emporte sur le grave et l'aigu. Le charleston
  et le clic d'attaque des kicks ne comptent pas ; une caisse claire jouée avec un kick (claps de
  la house), si.
- **Charleston** (attaques des aigus, cymbales) : passe-haut à 5 kHz, enveloppes rapide et lente.
- **Montée** et **drop** : roulement de caisse claire bien plus serré que d'habitude et son qui
  s'éclaircit (la tension monte) ; le drop est le premier kick après la montée, ou le retour
  brutal de l'énergie après un passage calme (au plus un toutes les 8 s).
- **Tempo** des kicks (affiché dans l'en-tête des réglages, « 128 BPM », quand il est régulier) :
  chaque **temps**, chaque **contretemps** (milieu du temps), et le temps d'un **break** (la
  musique continue sans grosse caisse depuis 2 s alors que le tempo était connu).
- **Basse** (niveau), **note de la basse** (plus ou moins grave), **brillance** (part d'aigus).
- **Stéréo** : le côté qui joue le plus fort ; les blancs entre deux coups ne la ramènent pas au
  centre, un vrai silence (0,5 s), si.
- **Changement de piste** pendant la lecture (panneau multimédia de Windows ; rien au lancement ni
  sur une pause).

### Déclencheurs

`[declencheurs] liste` (24 au plus) : `{ quand = "kick", faire = "onde", force = 1 }`, dans
l'ordre de la liste.

- `quand` : `kick`, `caisse`, `charleston`, `drop`, `temps`, `break`, `contretemps`, `piste`,
  `clic`, `clic_droit` (au point du clic ; c'est le bouton principal qui compte, même quand les
  boutons sont inversés).
- `faire` : `eclat` (halo blanc et anneau sous le pointeur, qui font briller les têtes),
  `satellites` (de `nombre` à `nombre_max`, dans la limite de `[essaim] maximum`), `inversion` (du
  sens de rotation), `bordure` (la bordure de la fenêtre au premier plan s'illumine), `onde` (de
  choc), `eclats` (seize étincelles projetées), `fissure`, `implosion` (une onde qui se referme,
  douze étincelles aspirées), `carre` (posé sur la pointe, qui s'ouvre), `etincelles` (`nombre`,
  depuis les têtes), `explosion` (éclat, satellites, grande onde, gerbe d'étincelles depuis le
  centre), `rafale` (éclat, satellites, grande onde).
- `force` : pour un éclat ou la bordure, de 0 à 1 ; pour une onde, des éclats, une fissure, un
  carré, une implosion, de 0,2 à 3 (la taille). `delai` : secondes au moins entre deux fois ;
  `probabilite` : la chance de se déclencher à chaque événement (0,33 : un sur trois).
- Un événement ou une action inconnus (preset d'une version plus récente) ne font rien.

### Modulations

`[modulations] liste` (16 au plus) : `{ source = "drop", cible = "rotation", sur = "satellites",
quantite = 0.8 }`. Par défaut : `basse` → `vitesse` (0,275 : +11 radians/s basse à fond), `grave`
→ `rayon` (0,325 : +130 %), `montee` → `acceleration` (0,6 : vitesse × 2,2) et `montee` → `rayon`
(−0,1125 : −45 %).

- Sources, de 0 à 1 : `basse` (niveau), `grave` et `aigu` (note de la basse, quand elle est là),
  `kick`, `caisse`, `charleston`, `contretemps` (un coup qui retombe en quelques dixièmes de
  seconde), `montee` (la tension avant un drop), `drop` (1 au drop, retombe en deux secondes
  environ), `tempo` (1 sur chaque temps quand le tempo est régulier), `brillance` ; `stereo` va de
  -1 (gauche) à 1 (droite).
- Cibles, à fond (`quantite` = 1, source à 1) : `rotation` du trajet +90 tr/min, `vitesse` sur le
  trajet +40 radians/s (382 tr/min), `acceleration` (la vitesse multipliée : × 3), `rayon`
  +400 %, `taille` des têtes et des traînées +150 %, `teinte` +180° (`sur = "tout"` : tout ce
  qu'Essaim dessine, particules et bordure comprises ; comètes ou satellites : leurs têtes et
  traînées), `decalage` de l'orbite vers la droite (1,6 rayon ; vers la gauche en négatif),
  `luminosite` (la lueur des têtes, +200 %), `trainee` (+300 %), `opacite` (−1 : transparent).
- `sur` : `"tout"`, `"cometes"` ou `"satellites"`. `quantite` de -1 à 1 (négatif : dans l'autre
  sens). `seuil` (0 à 0,95) : rien sous ce niveau de la source, puis jusqu'au maximum. `lissage`
  (0 à 1) : 0 réagit aussitôt, 1 en douceur (0,4 s ; 0,35 par défaut ≈ 50 ms). `inverser` : la
  source à l'envers (1 − valeur ; stéréo : l'autre côté).
- Rotations, vitesses, teintes et décalages s'additionnent ; accélération, rayon, taille, lueur,
  traînée et opacité se multiplient (deux réactions qui resserrent l'orbite de moitié la
  resserrent au quart), dans des bornes lisibles ; la vitesse ne devient jamais négative.

Les réglages et presets d'avant ces listes (`[musique]`, `[clic]`, `[scintillement]`,
`[fissures] actives`, `[bordure] intensite`, `[couleur] musique`, `[audio] pulsation_tempo`,
`[orbite] vitesse_basse`, `eloignement_grave`, `inverse_au_kick`, `[essaim] par_kick_min` et
`par_kick_max`) sont convertis avec leurs valeurs ; `config.toml` est réécrit une fois au
lancement, commentaires gardés. Un preset d'avant garde son allure (essai
`presets_comme_avant`, sur les presets fournis jusqu'à la version 0.6).

### Les effets

- **Trajet** : les comètes (`[orbite] nombre` par anneau, `anneaux` emboîtés qui tournent de
  moins en moins vite) suivent un trajet parmi quatorze formes (cercle, ellipse, triangle, carré,
  losange, pentagone, hexagone, étoile, astroïde, fleur, trèfle, cœur, huit, Lissajous ; `forme`),
  ou les contours d'une image (`forme = "trace"`, points dans `contours`, 24 au plus, parcourus à
  vitesse constante, coins compris ; plusieurs contours : un anneau de comètes chacun, quel que
  soit `anneaux` ; un ancien trajet dessiné à la main, dans `trace`,
  marche toujours), ou ceux de l'icône de l'application au premier plan (`forme = "icone"`, qui
  change avec elle ; les réglages d'Essaim n'en changent pas), qui peut tourner sur lui-même (`rotation`, tours/min ; quand sa rotation, modulations comprises, retombe sous 2 tours/min, il se redresse en douceur ; `rotation_sur` : `"tout"`,
  `"cometes"` ou `"satellites"`, seul ce groupe tourne avec le trajet) ; les satellites suivent le
  même trajet, ou le leur (`[essaim] forme`) ; `[essaim] fixes` ajoute des satellites permanents,
  même sans musique. Entre deux images, les traînées suivent le vrai trajet (points
  intermédiaires) : elles restent lisses à grande vitesse.
- **Couleurs** : une teinte de départ qui tourne en continu ou reste fixe ; ou la couleur
  dominante de l'icône de l'application au premier plan (`[couleur] application`, une fois par
  exécutable ; les icônes grises gardent la teinte précédente) ; dans un navigateur, celle du
  site affiché (`[couleur] site` : couleur dominante du haut de la page, sous la barre d'adresse,
  mesurée à chaque changement de page et toutes les 3 s ; une page grise garde la teinte
  précédente) ; ou, si l'outil Prisme de l'auteur tourne (non fourni ici), celle de l'éclairage
  RGB (bascule *Synchro Prisme*, `[couleur] synchro_prisme`). Avec `[couleur] pochette`, la
  teinte dominante de la pochette du morceau qui joue (Spotify, navigateur… : tout ce que montre
  le panneau multimédia de Windows) passe avant l'application, le site et Prisme ; en pause ou
  sans morceau, on retombe sur eux, dans cet ordre. Pour que la teinte glisse avec la musique :
  une modulation de la teinte « sur tout » (idée *La teinte suit la musique*).
- **Palette** (`[couleur] palette`, 1 à 3 teintes) : les comètes et les satellites se partagent
  plusieurs teintes, chacune décalée de `ecart_palette` degrés (120 : trois teintes à égale
  distance) de la teinte de base, qu'elle soit fixe, qui défile ou imposée (application, pochette…).
  Un satellite garde sa teinte toute sa vie.
- **Constellation** (`[essaim] constellation`) : de fines lignes relient les têtes (satellites et
  comètes) plus proches que 1,1 fois le rayon de l'orbite, d'autant plus vives qu'elles sont
  proches (160 lignes au plus).
- **Particules** (`[particules]`) : `taille` des ondes, des éclats et des implosions (0,3 à 3) ;
  `rebonds` (oui par défaut) : les étincelles (aigus, éclats, drop, clic droit) et la poussière du
  sillage rebondissent sur les bords de la fenêtre au premier plan, dedans comme dehors, en perdant
  un peu de vitesse ; ni le bureau, ni la barre des tâches, ni une fenêtre réduite ou en plein
  écran. 400 particules vivantes au plus.
- **Fissures** (`[fissures] taille`, `duree`) : une fissure qui s'ouvre sous le pointeur
  (déclencheur `fissure`, au clic par défaut), s'éclaire au début et sur les kicks, puis s'efface.
- **Sillage** : quand la souris file vite, une poussière reste derrière elle (`[sillage]`) ; on
  le voit aussi dans le grand aperçu des réglages quand la souris y file.
- **Inertie** (`[orbite] inertie`, 0 à 1, 0 par défaut) : le centre de l'orbite suit le pointeur
  comme au bout d'un ressort : il traîne derrière la souris quand elle file (jusqu'à six fois plus
  loin qu'à 0), la dépasse un peu à l'arrêt, puis revient sur elle.
- **Projecteur** (`[projecteur]`, actif par défaut) : secouer la souris (quelques allers-retours
  rapides de gauche à droite) ou Ctrl+Alt+F assombrit l'écran du pointeur autour d'un disque clair
  qui se resserre puis s'efface (`duree`, 1,2 s) ; le disque gonfle sur les éclats.
- **Réticule** (`[reticule]`) : deux fines lignes croisées centrées sur le pointeur (`taille`,
  `opacite`), vides au centre, qui s'illuminent sur les éclats.
- **Anneau de spectre** (`[spectre]`) : des barres autour du pointeur, une par bande de
  fréquences (`bandes`, 8 à 32, de 40 Hz à 12 kHz), plus longues quand la bande est forte
  (`rayon`, `hauteur`).
- **Bordure** (`[bordure] active`, `repos`) : la bordure de la fenêtre au premier plan prend la
  couleur de l'essaim (Windows 11) et s'illumine sur ses déclencheurs (`bordure`, au kick par
  défaut), même pendant la frappe ; pas en mode sans flashs.

Le son analysé est celui que joue Windows (capture loopback de la sortie par défaut, ou de celle
choisie dans *Écoute › Avancé › Sortie son écoutée*, `[audio] sortie`, propre à l'ordinateur et hors
presets ; si elle est débranchée, Essaim écoute la sortie par défaut en attendant son retour) :
Spotify, navigateur, jeux… sans rien configurer.

## Curseurs

Essaim ne dessine pas le pointeur : il reste un vrai curseur Windows. On installe un pack de
curseurs comme d'habitude (clic droit → Installer sur le fichier `.inf` du pack), puis on le
choisit dans l'onglet *03 Curseurs*, qui liste les jeux installés (ceux de l'utilisateur et
ceux de Windows) avec un aperçu, détaille les 17 curseurs du jeu en place et les met en place
comme le panneau de la souris (`src/souris.rs`). Les curseurs animés (`.ani`) sont animés dans
la fenêtre, à leur cadence. Un preset garde le jeu en place quand on l'enregistre, et son
curseur est dessiné au centre de son aperçu ; avec *Installer les curseurs du preset choisi*,
appliquer ce preset remet aussi ce jeu.

Le jeu conseillé avec Essaim est fourni dans `curseurs/Dotter` : **Dotter**, par
[Verticablets](https://www.deviantart.com/verticablets/art/Dotter-856489042), sous licence
CC BY-NC 3.0 (pas d'utilisation commerciale ; voir `curseurs/Dotter/CREDITS.md`). Son pointeur est
un simple point, discret au milieu de l'essaim. Clic droit sur `_install.inf` → *Installer*.

## Utilisation

- Icône de notification : un clic (ou double-clic) ouvre les réglages ; clic droit, le menu :
  *Mettre en pause*, *Réglages…*, *Fenêtre pour le stream*, *Presets* (celui en place coché),
  *Preset suivant* / *précédent*, *Quitter*, avec leurs raccourcis quand ils sont actifs. L'infobulle donne le preset en
  place et l'état (pause, veille) ; l'icône est grisée en pause.
- **Veille** : souris immobile, ni son ni clic depuis `[rendu] veille_apres` secondes (60 ;
  0 = jamais) : l'essaim s'efface, puis plus rien n'est dessiné jusqu'au prochain mouvement, son
  ou clic. En pause et en plein écran, la capture du son s'arrête aussi (sauf si la fenêtre des
  réglages est ouverte).
- **Pauses automatiques** (`[pause]`, propres à la machine, hors presets) :
  - `ne_pas_deranger` (oui par défaut) : pause quand Windows ne veut pas être dérangé (appli ou
    jeu en plein écran au premier plan, mode présentation), reprise ensuite ; le calque
    transparent d'une autre surcouche, même de la taille d'un écran, ne compte pas ;
  - `batterie` : sur batterie ou avec l'économiseur de batterie, `"rien"`, `"reduire"` (par
    défaut : 30 images/s au plus) ou `"pause"` ;
  - `applis` : pause tant que l'une de ces applications est au premier plan (onglet *04 Auto*,
    avec la dernière application utilisée à ajouter d'un clic).
  La raison s'affiche sous le bouton Pause.
- **Qualité adaptative** (`[rendu] adaptatif`, oui par défaut) : si une image demande plus de
  4 ms de calcul, la cadence baisse par paliers (120, 60, 30 images/s au plus) et remonte quand
  le calcul redevient léger.
- **Masquer des captures** (`[rendu] masquer_captures`) : l'essaim reste à l'écran mais
  n'apparaît plus dans les captures, enregistrements et partages d'écran (Windows 10 2004 et plus).
- **Sans flashs** (`[rendu] sans_flashs`, propre à la machine) : pour les personnes sensibles aux
  flashs. Tout éclat (kick, rafale, drop, et tout déclencheur `eclat`, quels que soient sa force et
  son événement) est à moitié de sa force et au plus trois par seconde, sans halo blanc ; les ondes
  et les carrés sont plus pâles ; la bordure garde sa luminosité de repos (ses déclencheurs ne font
  rien). `"auto"` (par défaut, *Comme Windows* dans les réglages) : seulement quand
  les effets d'animation de Windows sont coupés (*Accessibilité › Effets visuels*), relu chaque
  seconde ; `"oui"` : toujours ; `"non"` : jamais.
- **Fenêtre pour le stream** (*Fenêtre pour le stream* dans le menu de l'icône, le bouton de
  *Cet ordinateur* dans les réglages, ou `essaim.exe --stream`) : une fenêtre ordinaire où l'essaim danse au centre,
  sur un fond uni (`[stream] fond` : `"vert"` par défaut, `"bleu"`, `"magenta"` ou `"noir"` ; propre
  à la machine). Dans OBS : *Capture de fenêtre* sur « Essaim — stream », puis un filtre
  *Incrustation chromatique* (ou, sur fond noir, un mode de fusion *Écran*). Elle suit le son et les
  réglages, pas le pointeur, et continue quand l'essaim du pointeur est en pause, en plein écran ou
  masqué par la frappe (le son reste écouté tant qu'elle est ouverte). Elle reste capturable même
  avec *Masquer des captures*. L'essaim y grandit avec la fenêtre.
- **Raccourcis globaux** (`[raccourcis] actif`) : Ctrl+Alt+→ / ← preset suivant / précédent
  (une bulle de l'icône le nomme), Ctrl+Alt+P pause, Ctrl+Alt+F projecteur. Ils passent par `RegisterHotKey` : Windows
  prévient Essaim de ces seules touches, sans hook sur le clavier.
- **Preset automatique** (`[automatique]`) : des règles « si … alors … » (`regles`,
  prioritaires), puis un preset par application au premier plan (nom de l'exécutable), puis par
  plage horaire (qui peut passer minuit). Quand plus aucune règle ne s'applique, les réglages
  d'avant reviennent, sauf ceux retouchés pendant la règle, qui sont gardés (curseurs compris).
  Renommer ou supprimer un preset met à jour ou retire les règles qui le citent. Les fenêtres
  d'Essaim ne comptent pas.
  - Une règle : `{ si = "artiste = Skrillex", alors = "Delta" }`. Clés : `artiste` et `titre` (du
    morceau qui joue, pas en pause), `appli` (exécutable au premier plan, `.exe` facultatif), `bpm`
    (tempo des kicks, quand il est stable), `heure` (« HH:MM »). Opérateurs : `=`, `!=`, `~`
    (contient), et pour `bpm` et `heure` `<`, `>`, `<=`, `>=`. Plusieurs conditions : « et »
    (`bpm > 140 et heure >= 22:00`). Majuscules ignorées ; une valeur peut être entre guillemets
    (`artiste = "Simon et Garfunkel"`).
  - `artiste =` reconnaît aussi un des artistes d'un champ « A, B & C feat. D » ; `bpm =` est vrai
    à 2 BPM près. Une donnée absente (rien ne joue, tempo inconnu) rend la condition fausse.
  - La première règle vraie l'emporte. Un changement dû à la musique (artiste, titre, tempo)
    attend qu'elle soit stable 2 s : un tempo qui hésite autour d'un seuil ne fait pas clignoter
    les presets. Une règle illisible est ignorée et notée au journal.
- **Fenêtre des réglages** (`essaim.exe --reglages`, *Réglages…*, ou « Essaim - réglages » dans
  le menu Démarrer). Elle est claire ou sombre selon le thème des applications de Windows.
  - *01 Presets* : la grille des presets, chacun animé comme l'essaim autour du pointeur et au
    rythme du vrai son (en silence, sur un kick de démonstration). Recherche, filtre par source
    (fournis, mes presets, communauté), aperçus *Direct*, *Survol* ou *Figé*. Un clic applique ;
    le cadre rouge marque le preset actif. *Enregistrer le preset* garde les réglages actuels
    (`%APPDATA%\Essaim\profils\<nom>.toml`) et peut les proposer à la bibliothèque. Filtre par
    forme ; un arrêt sur une vignette l'affiche en grand à côté ; *Comparer* sur deux presets les
    montre côte à côte avec la liste de leurs réglages différents.
  - *02 Réglages* : des sous-onglets rangés par familles : *L'essaim* (*Orbite*, *Comètes*,
    *Satellites*, *Couleur*), *Les effets* (*Éclat*, *Particules*, *Fissures*, *Bordure*,
    *Spectre*, *Réticule et projecteur*), *Réactions* (*Modulations*, *Déclencheurs*), *Écoute* et
    *Cet ordinateur* (réglages propres au PC) ; chacun avec ses réglages courants et *+ Avancé*
    pour les autres ; flèches haut et bas pour passer de l'un à l'autre. Un voyant dit si l'effet
    est là (bascule *Actif* en tête du panneau, ou au moins un déclencheur) ; un effet coupé, ou un
    réglage sans effet (densité du sillage sans sillage…), est estompé mais reste réglable. Chaque
    effet montre ses déclencheurs (*Quand* : un bouton par événement, × pour le retirer, un choix
    pour en ajouter). *Déclencheurs* : une ligne par déclencheur (quand → action, force ou taille,
    nombre ; *+* : délai minimal, chance ; *Tester* le joue dans le grand aperçu) ; *Modulations* :
    une ligne par modulation (source → paramètre, sur qui, quantité, jauge de la source en direct ;
    *+* : seuil, lissage, à l'envers) ; des idées à ajouter d'un clic. Sous l'aperçu, *Simuler* :
    une montée suivie du drop (4 s), un kick, une caisse claire, un temps, un changement de piste,
    pour voir les réactions sans attendre la musique ; tant qu'un morceau joue, l'aperçu suit le
    vrai son, même entre deux coups. *Image…* ouvre
    *Trajet d'une image* : une image (choisie, ou glissée sur la fenêtre : PNG, JPEG, GIF, WebP,
    BMP, SVG, ICO, 20 Mo au plus), ou un texte, un emoji ; ses contours s'affichent sur l'image et
    l'essaim les parcourt dans un aperçu. *Seuil* (calculé : *Auto*), *Inverser* (la forme est de
    l'autre côté du seuil), *Le plus grand* ou *Tous, un par anneau* (24 au plus : chaque anneau
    suit le sien, à sa place ; un texte les garde tous, une lettre chacune). Une image à fond
    transparent donne le plus net. Un clic sur l'aperçu y montre l'effet du
    clic. *Exporter en GIF* enregistre trois secondes de l'aperçu (320 px, 20 images/s) dans
    `Images\Essaim` et le montre dans l'Explorateur.
  - *04 Auto* : les règles du preset automatique (*Si … alors …*, avec le bouton *Artiste en
    cours* qui remplit la condition pour le morceau qui joue ; par application ; par heure), les
    pauses par application et les raccourcis.
  - Au premier lancement, un accueil en trois étapes (à revoir depuis *Cet ordinateur*).
  - En-tête : ce que l'essaim entend. Le témoin *Kick* rougit sur chaque kick ; *Basse* donne le
    niveau du grave (faible → forte) et *Note* sa hauteur (25–85 Hz, grave → aiguë) ; le témoin
    *Aigus* s'allume sur les attaques des aigus. Sous *Pause*, l'état de l'écoute : *Son capté*,
    *Son coupé* (pause, plein écran, son désactivé) ou *Son : erreur*, *En veille*, et la raison
    d'une pause automatique ; le titre du morceau en cours (titre et artiste au survol, aussi
    dans l'infobulle de l'icône) et le tempo des kicks (BPM) quand il est régulier.
  - *Puissance utilisée* : temps de calcul par image × images par seconde, mesuré en direct
    (part d'un cœur du processeur), avec des pistes pour alléger quand elle est élevée.
  - Une valeur de réglage se tape au clavier (clic dessus, Entrée) ; double-clic sur une
    glissière : valeur par défaut.
  - À côté de chaque réglage, dix cases notent son impact sur la puissance (1 à 10), mesuré par
    le banc `mesure_impact_reglages` (coût d'une image au minimum puis au maximum du réglage) ;
    quelques réglages hors du banc ont un score estimé, signalé comme tel. Ce qui pèse : la
    carte graphique (10), la cadence (7), le rayon (4) ; le reste compte peu.
  - Presets : favoris, tri (nom, récents), menu *Gérer* (dupliquer ; pour les tiens : renommer,
    exporter vers l'Explorateur, supprimer), import d'un fichier `.toml` (bouton ou glisser sur
    la fenêtre). Un preset appliqué s'annule par *Annuler* dans le message ou Ctrl+Z. Le preset
    dont les réglages en cours s'écartent de peu est marqué *modifié* (cadre pointillé).
    Installer ou importer un preset ne remplace jamais un preset différent du même nom (à la
    casse près), et les noms réservés de Windows (CON, NUL, COM1…) sont refusés.
  - *Surprends-moi* tire des réglages au hasard (Ctrl+Z pour revenir).
  - Pause dans l'en-tête ; *Démarrer Essaim avec Windows* dans *Cet ordinateur*.
  - Clavier : `/` recherche, flèches dans la grille, Entrée applique, Ctrl+1 à 4 ou flèches
    (Début, Fin) sur les onglets, Ctrl+S enregistrer, Ctrl+Z annuler, Ctrl+P pause, Échap ferme
    une boîte.
  - Si Windows demande de réduire les animations, le grand aperçu, la loupe et la comparaison
    restent à l'arrêt, avec un bouton *Lecture*.
  - Préréglages fournis (22), chacun avec son caractère : *Défaut* ; *Discret* (petit, lent, sans
    fissure, pour travailler) ; *Intense* (deux anneaux, beaucoup de satellites, ondes, éclats et
    fissures) ; *Zen* (trois anneaux lents qui respirent au tempo, peu de réactions) ; *Viseur*
    (réticule et têtes serrées, pour jouer) ; *Spectre* (anneau de spectre) ; *Comète* (une seule
    comète qui traîne derrière la souris, sillage) ; *Néon* (Lissajous magenta et vert, tout s'allume
    au kick) ; *Arcade* (carré vert phosphore, carrés sur la caisse claire et au clic) ; *Orage*
    (astroïde bleu, fissures sur des kicks au hasard et au drop) ; *Delta* (triangle qui tourne,
    s'emballe à l'envers au drop) ; *Ruche* (hexagone ambre, satellites permanents reliés) ; *Huit*
    (penche du côté qui joue le plus fort) ; *Spirale* (quatre anneaux qui tournent, satellites sur
    une ellipse) ; *Mandala* (trois anneaux de fleur reliés, trois teintes) ; *Battement* (cœur qui
    bat au tempo) ; *Constellation* (lignes entre les satellites, deux teintes) ; *Festival*
    (contretemps, stéréo, inertie, trois teintes) ; *Tourbillon* (le trajet des satellites, seul à
    tourner, s'emballe au drop) ; *Pollen* (sans comètes : des grains qui flottent, sillage) ;
    *Pochette* (teinte de la pochette du morceau, trèfle) ; *Caméléon* (forme et couleur de l'icône
    de l'application au premier plan). Chacun est refaisable à la main dans les réglages. Les
    presets ne touchent pas à `[rendu]`, `[stream]` (propres à la machine) ni à l'option des
    curseurs.
- Réglages : `%APPDATA%\Essaim\config.toml`, appliqués aussi dès l'enregistrement du fichier.
- Journal : `%APPDATA%\Essaim\essaim.log` (états, cadence et temps de calcul toutes les minutes).
- `essaim.exe --quitter` ferme l'instance en cours ; `essaim.exe --stream` ouvre la fenêtre pour
  le stream (celle de l'instance en cours s'il y en a une).
- `essaim.exe --restaurer-bordure` rend sa couleur à la fenêtre restée colorée si l'appli a été
  tuée de force (fermeture normale, plantage, pause et plein écran le font tout seuls, et le
  lancement suivant aussi).

## Mises à jour

Trente secondes après le lancement, puis toutes les six heures, Essaim regarde la dernière
version publiée sur GitHub (`KingsRequin/essaim`, *Releases*). Si elle est plus récente :

1. une notification annonce le téléchargement de `essaim.exe` ;
2. son empreinte SHA-256 est comparée à celle publiée par GitHub, et sa signature à la clé
   de l'auteur intégrée à Essaim : un fichier abîmé ou qui ne vient pas de l'auteur est refusé ;
3. le nouveau fichier prend la place de l'exécutable en cours, et l'ancien devient
   `essaim.exe.ancien` (effacé au lancement suivant) ;
4. une seconde notification propose de redémarrer. Clic dessus, ou *Redémarrer avec la version…*
   dans le menu de l'icône. Sans clic, la nouvelle version démarre avec la session suivante.

Le dossier de l'exécutable doit être modifiable (c'est le cas de
`%LOCALAPPDATA%\Programs\Essaim`, pas de `Program Files`). Couper : *Mises à jour automatiques*
dans *Cet ordinateur*, ou `[mise_a_jour] auto = false`. Les échecs (pas de réseau, fichier
refusé) sont notés dans le journal, et Essaim réessaie six heures plus tard.

## Bibliothèque de presets (GitHub)

Les presets partagés vivent dans un dépôt GitHub public, `[bibliotheque] depot` dans
`config.toml` (par défaut `KingsRequin/essaim-presets`). L'appli lit son `index.json` (branche
`main`) :

```json
{"version": 1, "presets": [
  {"nom": "Pollen", "auteur": "pseudo-github", "etiquettes": ["dense", "jaune"],
   "fichier": "presets/Pollen.toml",
   "reglages": {"orbite": {"rayon": 26}, "essaim": {"maximum": 40}, "couleur": {"teinte": 50, "vitesse_teinte": 0}}}
]}
```

`reglages` reprend les sections de `config.toml` (sauf `[rendu]`) ; les clés absentes prennent
les valeurs du preset *Défaut*. *Installer* copie le preset dans Mes presets. *Publier* (case du
panneau d'enregistrement) ouvre GitHub sur la création de `presets/<nom>.toml` dans le dépôt :
GitHub en fait une copie et une pull request, à relire avant d'ajouter le preset à l'index. Un
preset trop long pour l'adresse (plus de 6 000 caractères, par exemple le trajet d'une image)
est copié dans le presse-papiers : il suffit de le coller dans la page qui s'ouvre. Une entrée de
l'index mal formée (section qui n'est pas une table) est ignorée. Champ facultatif `apercu` :
chemin relatif d'une image PNG ou GIF du dépôt (`"apercus/Pollen.gif"`), montrée dans la vignette
quand l'aperçu est figé ; sans lui, ou si l'image ne se charge pas, l'aperçu est calculé.
