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

- **Kick** : reconnu par un petit modèle (`audio-commun`), en 25 ms environ. Le modèle garde le
  contexte en mémoire (rythme, coups récents) : une voix grave ou une contrebasse seules ne
  comptent presque plus comme des kicks. Il est appris sur des morceaux de 42 styles et juge le son
  par rapport à son niveau récent : le volume du lecteur ne change rien (après une forte baisse, il
  lui faut une vingtaine de secondes pour s'y faire).
- **Caisse claire** (et clap) : reconnue par un petit modèle appris sur les mêmes morceaux que le
  kick, en 20 à 30 ms. Le charleston et les kicks ne comptent pas ; une caisse claire jouée avec
  un kick (claps de la house), si.
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

`[declencheurs] liste` (40 au plus) : `{ quand = "kick", faire = "onde", force = 1 }`, dans
l'ordre de la liste.

- `quand` : `kick`, `caisse`, `charleston`, `drop`, `temps`, `break`, `contretemps`, `piste`,
  `clic`, `clic_droit` (au point du clic ; c'est le bouton principal qui compte, même quand les
  boutons sont inversés).
- `faire` : `eclat` (halo blanc, qui laisse libre la pointe, et anneau sous le pointeur, qui font
  briller les têtes),
  `satellites` (de `nombre` à `nombre_max`, dans la limite de `[essaim] maximum`), `inversion` (du
  sens de rotation ; une seule par image : deux kicks ou un kick et une caisse claire arrivés
  ensemble ne s'annulent pas), `bordure` (la bordure de la fenêtre au premier plan s'illumine), `onde` (de
  choc), `eclats` (seize étincelles projetées), `fissure`, `implosion` (une onde qui se referme,
  douze étincelles aspirées), `carre` (posé sur la pointe, qui s'ouvre), `etincelles` (`nombre`,
  depuis les têtes), `explosion` (éclat, satellites, grande onde, gerbe d'étincelles depuis le
  centre), `rafale` (éclat, satellites, grande onde), `rebond` (le rayon de l'orbite part vers
  l'extérieur, dépasse et revient, comme au bout d'un ressort), `poussee` (la rotation prend de
  la vitesse, +5 radians/s à la force 1, qui retombe en un quart de seconde), `frein` (la
  rotation perd d'un coup jusqu'à 80 % de sa vitesse, puis repart) ; et les **gestes des figures**,
  que chaque figure fait à sa façon (voir « Les effets ») : `battement` (le geste du kick : les
  arcs avancent, les perles des gouttes sont projetées, le pendule saute…), `accent` (celui de la
  caisse claire : une goutte se détache, le pendule reçoit une pichenette…), `scintillement`
  (celui des aigus) et `apogee` (celui du drop : écho, bulles, grande vague…). `sur` (gestes
  seulement) : la figure qui le fait (`"gouttes"`…) ; absent, `"tout"` ou `"figures"` : toutes
  celles qui font ce geste. Les réactions d'une figure aux coups du son sont ces déclencheurs,
  ajoutés quand on l'allume (`{ quand = "kick", faire = "battement", sur = "gouttes" }`…) : on les
  change, les retire ou les met sur d'autres événements comme les autres. Ce qu'une modulation de
  la figure sait déjà faire n'est pas un geste, pour ne pas le régler à deux endroits : le
  battement et le carré de l'anneau (modulations de son rayon au kick et de sa forme à la caisse
  claire), les arcs qui se ferment en cercles et le trait qui dure plus longtemps au drop
  (modulations de l'ouverture et de la durée) sont des modulations, ajoutées aussi quand on allume
  la figure. Des réglages d'avant la 0.28 qui en faisaient des gestes sont convertis.
- `force` : pour un éclat, la bordure ou le frein, de 0 à 1 ; pour une onde, des éclats, une
  fissure, un carré, une implosion, de 0,2 à 3 (la taille) ; pour des étincelles, de 0,2 à 3 (leur
  vitesse : leur taille ne change pas) ; pour une explosion ou une rafale, de 0,2 à 3 (la taille
  de la grande onde seule) ; pour un rebond ou une poussée, de 0 à 3 (l'élan) ; pour un geste
  des figures, de 0 à 2 (le geste de toujours à 1, le double à 2). `nombre` et
  `nombre_max` (satellites, étincelles, et ceux de l'explosion et de la rafale) : de 0 à 40, le
  nombre tiré entre les deux à chaque fois. `delai` : secondes au moins entre deux fois (0 à 10) ;
  `probabilite` : la chance de se déclencher à chaque événement (0 à 1 ; 0,33 : un sur trois).
- Un événement ou une action inconnus (preset d'une version plus récente) ne font rien.
- Jusqu'à la version 0.25, les réactions d'une figure étaient fixes, réglées par `reaction_kick`,
  `reaction_caisse`, `reaction_charleston`, `reaction_basse`, `reaction_montee` et
  `reaction_drop` : un fichier ou un preset d'alors (sans `[declencheurs] figures = true`) reçoit,
  pour chaque figure allumée, les déclencheurs de ses gestes à la force de sa réaction d'alors
  (aucun à 0) ; ces clés sont retirées (celles de la basse et de la montée sans équivalent).

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
  `luminosite` (la lueur des têtes, +200 %), `trainee` (+300 %), `opacite` (−1 : transparent ;
  une quantité positive est sans effet seule, l'opacité étant déjà au plus, mais compense une
  autre modulation qui la baisse). En négatif, ces parts multipliées s'arrêtent à leur borne :
  le rayon et la traînée à −90 % (× 0,1), l'accélération, la taille et la lueur à −100 % ; une
  figure, elle, reste entre la moitié et deux fois et demie sa taille (échelle × rayon, de × 0,5
  à × 2,5 : elle ne couvre jamais l'écran). Quel que soit le réglage, l'essaim reste à moins de
  1 200 px du pointeur (rayon et décalage réduits ensemble au-delà, selon le point le plus loin
  de sa forme : un cercle va plus loin qu'une ellipse de même rayon), et une traînée ne fait pas
  plus de cinq tours de son trajet (au-delà, elle repasserait sur elle-même). Les aperçus
  montrent cette borne à l'échelle 100 % : sur un écran à 150 ou 200 %, l'essaim, plus grand,
  l'atteint plus tôt. La
  fenêtre des réglages affiche cet effet borné, source à fond, et son infobulle dit d'où la
  source à fond ne change plus rien (−22,5 % pour le rayon, −30 % pour la traînée, −50 % pour
  l'accélération et la lueur, −66,7 % pour la taille) ; la glissière va quand même de −100 à
  100 % : la quantité est multipliée par la source, et une source à mi-course distingue encore
  ce que la source à fond confond.
- `sur` : `"tout"` (les comètes et les satellites), `"cometes"`, `"satellites"`, `"figures"`
  (toutes les figures autour du pointeur) ou une figure : `"arcs"`, `"quatre_temps"`,
  `"anneau"`, `"orbe"`, `"voix"`, `"trame"`, `"pinceau"` (le trait), `"pendule"`, `"gouttes"`.
  Sur une figure, `rayon` l'agrandit tout entière (la trame : sa loupe ; le pendule : son fil,
  de × 0,5 à × 2,5), `taille` épaissit ses traits (la trame et les quatre temps : leurs points ;
  le pendule : son poids), `decalage` la pousse de côté (48 px à fond), `opacite` la pâlit,
  `teinte` la colore. Seuls les arcs, l'anneau, l'orbe et les gouttes tournent (`rotation`,
  `vitesse`, dix fois moins que pour l'essaim, et `acceleration`, qui multiplie leur rotation) :
  les quatre temps et la ligne de voix sont sous la flèche, la trame est fixe à l'écran ; le trait
  et le pendule ne grandissent ni ne se déplacent. `luminosite` y vaut une opacité de plus et
  `trainee` n'y fait rien (la fenêtre ne les propose pas). Chaque figure a en plus ses cibles
  propres, des parts ajoutées (quantité × source) : `ouverture` (arcs : 1 = grands ouverts),
  `ecart` (quatre temps, perles des gouttes : de × 0,3 à × 2), `forme` (anneau : 1 = carré),
  `relief` (orbe, ligne de voix : de plat à × 2), `duree` (trait : de × 0,3 à × 2,5), `nombre`
  (gouttes : 1 = 4 perles de plus, 8 au plus) et `noyau` (gouttes : 1 = +8 px). `quantite` de -1 à 1 (négatif : dans l'autre
  sens). `seuil` (0 à 0,95) : rien sous ce niveau de la source, puis jusqu'au maximum. `lissage`
  (0 à 1) : 0 réagit aussitôt, 1 en douceur (0,4 s ; 0,35 par défaut ≈ 50 ms). `inverser` : la
  source à l'envers (1 − valeur ; stéréo : l'autre côté) ; une source nulle en silence agit donc
  à fond tant que rien ne joue.
- Rotations, vitesses, teintes et décalages s'additionnent ; accélération, rayon, taille, lueur,
  traînée et opacité se multiplient (deux réactions qui resserrent l'orbite de moitié la
  resserrent au quart), dans des bornes lisibles. Avec le réglage, la rotation reste entre −180
  et 180 tr/min (celle d'une figure aussi) et la vitesse sur le trajet entre 0 et 60 radians/s.
  Une figure ne descend pas sous la moitié de sa taille : plus petite, elle se ramasserait sur
  le pointeur.

Les réglages et presets d'avant ces listes (`[musique]`, `[clic]`, `[scintillement]`,
`[fissures] actives`, `[bordure] intensite`, `[couleur] musique`, `[audio] pulsation_tempo`,
`[orbite] vitesse_basse`, `eloignement_grave`, `inverse_au_kick`, `[essaim] par_kick_min` et
`par_kick_max`) sont convertis avec leurs valeurs ; `config.toml` est réécrit une fois au
lancement, commentaires gardés. Un preset sans `[declencheurs]` dont la liste de modulations est
vide (`liste = []`) la garde vide, sauf pour les réactions d'avant écrites dans le preset (une
réaction non écrite y compte comme coupée, même la montée, active par défaut : ni ses
modulations ni l'explosion au drop). Un preset d'avant garde son allure (essai
`presets_comme_avant`, sur les presets fournis jusqu'à la version 0.6).

### Les effets

- **Trajet** : les comètes (`[orbite] cometes`, oui par défaut ; `nombre` par anneau, 1 à 8, 2
  par défaut ; `anneaux` emboîtés, 1 à 4, qui tournent de moins en moins vite) suivent un trajet
  parmi quatorze formes (cercle, ellipse, triangle, carré, losange, pentagone, hexagone, étoile,
  astroïde, fleur, trèfle, cœur, huit, Lissajous ; `forme`), ou les contours d'une image
  (`forme = "trace"`, points dans `contours`, 24 au plus, parcourus à vitesse constante, coins
  compris ; plusieurs contours : un anneau de comètes chacun, quel que soit `anneaux` ; un ancien
  trajet dessiné à la main, dans `trace`, marche toujours), ou ceux de l'icône de l'application au
  premier plan (`forme = "icone"`, qui change avec elle, les traînées repartant de zéro sur la
  nouvelle forme ; les réglages d'Essaim n'en changent pas). Autres réglages : `rayon` (4 à 200 px
  à 100 %, 22 par défaut, sans basse), `vitesse_base` (vitesse de rotation sans basse, 0 à 60
  radians/s, 3,5 par défaut) et `trainee` (durée de la traînée des comètes, 0,01 à 3 s, 0,3 par
  défaut). Le trajet peut tourner sur lui-même (`rotation`, tours/min ; quand sa rotation,
  modulations comprises, retombe sous 2 tours/min, il se redresse en douceur ; `rotation_sur` :
  `"tout"`, `"cometes"` ou `"satellites"`, seul ce groupe tourne avec le trajet ; une autre valeur
  vaut `"tout"`). Les satellites suivent le même trajet, ou le leur (`[essaim] forme`) ; `[essaim]
  maximum` borne ceux qui naissent en même temps (0 à 64, 16 par défaut), chacun vivant `duree`
  secondes (0,1 à 30, 2,8 par défaut) ; `fixes` ajoute des satellites permanents (0 à 32), même
  sans musique. Entre deux images, les traînées suivent le vrai trajet (points intermédiaires) :
  elles restent lisses à grande vitesse.
- **Couleurs** : une teinte de départ (`[couleur] teinte`, 0 à 360°, 0 par défaut) qui tourne en
  continu (`vitesse_teinte`, degrés par seconde, 0 à 720, 60 par défaut) ou reste fixe (0) ; un
  écart de teinte entre la tête et la queue des traînées (`ecart_teinte`, 0 à 720°, 140 par
  défaut) ; ou la couleur dominante de l'icône de l'application au premier plan (`[couleur] application`, une fois par
  exécutable ; les icônes grises gardent la teinte précédente) ; dans un navigateur, celle du
  site affiché (`[couleur] site` : couleur dominante du haut de la page, sous la barre d'adresse,
  mesurée à chaque changement de page et toutes les 3 s ; une page grise garde la teinte
  précédente) ; ou, si l'outil Prisme de l'auteur tourne (non fourni ici), celle de l'éclairage
  RGB (bascule *Synchro Prisme*, `[couleur] synchro_prisme`, montrée quand Prisme répond, ou
  tant qu'elle est cochée, pour pouvoir la décocher). Avec `[couleur] pochette`, la
  teinte dominante de la pochette du morceau qui joue (Spotify, navigateur… : tout ce que montre
  le panneau multimédia de Windows) passe avant l'application, le site et Prisme ; en pause ou
  sans morceau, on retombe sur eux, dans cet ordre. Le passage de l'une à l'autre, et vers ou
  depuis la teinte propre de l'essaim, se fait toujours en glissant, sans saut (une teinte qui
  disparaît rejoint la teinte propre par un écart qui s'amortit en ~250 ms) ; au changement de
  morceau, la teinte de l'ancienne pochette reste le temps que la nouvelle soit lisible. Pour que la teinte glisse avec la musique :
  une modulation de la teinte « sur tout » (idée *La teinte suit la musique*).
- **Luminosité et saturation** (`[couleur] luminosite`, 0 = noir, 0,66 = vif par défaut, 1 =
  blanc ; `saturation`, 0 = gris, 1 par défaut) : celles de tout ce qu'Essaim dessine (essaim,
  figures à la couleur de l'essaim, particules, habillage, bordure), quelle que soit la teinte,
  même prise de l'application, du site ou de la pochette : un essaim sombre, jusqu'au noir, ou
  pâle. Les reflets blancs (cœur des têtes et des étincelles, halo de l'éclat) s'assombrissent
  avec elle. Dans la fenêtre des réglages, une pastille du nuancier donne une couleur vive ; la
  dernière, en arc-en-ciel, ouvre le choix de couleur de Windows : n'importe quelle couleur, qui
  donne teinte, saturation et luminosité. Dans les deux cas la couleur devient fixe (elle ne
  défile plus et ne suit plus l'application). Rallumer une source (défilement, application, site,
  pochette, Prisme) depuis une couleur fixe rend des couleurs vives : on l'assombrit ensuite avec
  la luminosité. Un double-clic sur une glissière la remet à sa valeur par défaut. Derrière un
  essaim sombre, les aperçus s'éclaircissent.
- **Palette** (`[couleur] palette`, 1 à 3 teintes) : les comètes et les satellites se partagent
  plusieurs teintes, chacune décalée de `ecart_palette` degrés (120 : trois teintes à égale
  distance) de la teinte de base, qu'elle soit fixe, qui défile ou imposée (application, pochette…).
  Un satellite garde sa teinte toute sa vie.
- **Rubans** (`[rubans]`, éteints par défaut) : les traînées des comètes deviennent des rubans,
  larges au milieu (`largeur`, 1 à 14, 4 par défaut), affinés aux deux bouts, sans tête. Leur
  largeur ondule le long de la traînée comme un ruban qui tourne (`vrille`, 0 = plat, 1 par
  défaut), et chaque caisse claire fait avancer cette vrille d'un cran. Leur longueur est la
  traînée des comètes, leurs teintes celles de la palette (une teinte fixe et un *écart
  tête-queue* donnent un dégradé le long du ruban) ; une modulation de la taille les fait enfler.
- **Ressorts** (`[ressorts]`, éteints par défaut) : tout prend de l'élan. Le rayon de l'orbite
  passe par un ressort (`raideur`, 6 à 30, 19 par défaut ; `amorti`, 0,1 = rebondit longtemps,
  1 = revient sans dépasser, 0,26 par défaut) : ce que les modulations lui demandent, il le fait
  en dépassant un peu, et il tremble pendant une montée. Les têtes s'étirent à l'opposé de leur
  vitesse (`etirement`, 0 à 2), les satellites naissent au centre et rejoignent leur orbite chacun
  au bout de son ressort, entrent en dépassant leur taille et sortent en fondant ; l'éclat devient
  un anneau épais qui sort en dépassant puis s'affine, sans halo blanc. Les traînées des
  satellites sont d'un tiers plus courtes (sans inversion du sens, elles restent déployées). Les
  actions `rebond`, `poussee` et `frein` des déclencheurs marchent aussi sans ce mode.
- **Constellation** (`[essaim] constellation`) : de fines lignes relient les têtes (satellites et
  comètes) plus proches que 1,1 fois le rayon de l'orbite, d'autant plus vives qu'elles sont
  proches (160 lignes au plus).
- **Éclat** (`[eclat] taille`, 0 à 20, 3,4 par défaut ; 0 : aucun) : la taille du halo et de
  l'anneau que lance l'action `eclat`, sous le pointeur.
- **Particules** (`[particules]`) : `taille` des ondes, des éclats et des implosions (0,3 à 3) ;
  `rebonds` (oui par défaut) : les étincelles (aigus, éclats, drop, clic droit) et la poussière du
  sillage rebondissent sur les bords de la fenêtre au premier plan, dedans comme dehors, en perdant
  un peu de vitesse ; ni le bureau, ni la barre des tâches, ni une fenêtre réduite ou en plein
  écran. 400 particules vivantes au plus.
- **Fissures** (`[fissures] taille`, `duree`) : une fissure qui s'ouvre sous le pointeur
  (déclencheur `fissure`, au clic par défaut), s'éclaire au début et sur les kicks, puis s'efface.
  Sa taille est celle du réglage fois la force du déclencheur, ses branches à 400 px au plus du
  point (à 100 %).
- **Sillage** (`[sillage]`, éteint par défaut : `actif`) : quand la souris file vite, une
  poussière reste derrière elle, plus ou moins dense (`densite`, 1 à 10, 5 par défaut) ; on le
  voit aussi dans le grand aperçu des réglages quand la souris y file.
- **Inertie** (`[orbite] inertie`, 0 à 1, 0 par défaut) : le centre de l'orbite suit le pointeur
  comme au bout d'un ressort : il traîne derrière la souris quand elle file (jusqu'à six fois plus
  loin qu'à 0), la dépasse un peu à l'arrêt, puis revient sur elle.
- **Projecteur** (`[projecteur]`, actif par défaut) : secouer la souris (quelques allers-retours
  rapides de gauche à droite) ou Ctrl+Alt+F assombrit l'écran du pointeur autour d'un disque clair
  qui se resserre puis s'efface (`duree`, 1,2 s) ; le disque gonfle sur les éclats. *Sensibilité à
  la secousse* (`sensibilite`, 1 à 10, 5 par défaut) : à 1, il faut un geste ample et vif
  (allers d'au moins 104 px) ; à 10, un petit geste plus lent suffit (30 px). Un va-et-vient
  bouton enfoncé (sélection, fenêtre traînée) ne compte pas. S'il part tout seul, la baisser ;
  `secousse = false` (*Projecteur lancé par › La secousse de la souris*) ne garde que le
  raccourci. Ces trois réglages (le projecteur lui-même, la secousse et la sensibilité), propres
  à la machine, ne changent pas avec le preset. Sous *+ Avancé* : *Taille du projecteur*, le rayon du disque à la fin de l'effet
  (`taille`, 30 à 300, 60 par défaut), et *Obscurité du projecteur*, l'assombrissement de l'écran
  (`opacite`, 20 à 95 %, 62 % par défaut).
- **Réticule** (`[reticule]`) : deux fines lignes croisées centrées sur le pointeur (`taille`,
  `opacite`), vides au centre, qui s'illuminent sur les éclats. Sous *+ Avancé* : l'épaisseur
  des lignes (`epaisseur`, 0,5 à 6, 1,2 par défaut) et le vide autour du pointeur (`vide`, 4 à
  60, 12 par défaut).
- **Anneau de spectre** (`[spectre]`, éteint par défaut : `actif`) : des barres autour du
  pointeur, une par bande de fréquences (`bandes`, 8 à 32, 16 par défaut, de 40 Hz à 12 kHz),
  plus longues quand la bande est forte (`rayon` intérieur, 10 à 150, 40 par défaut ; `hauteur`
  à pleine énergie, 4 à 100, 24 par défaut). Avec `masquer_sans_musique` (oui par défaut, *Masquer sans musique*),
  l'anneau s'efface après 1,5 s sans aucun son et revient dès que la musique reprend.
- **Arcs** (`[arcs]`) : deux arcs fins autour du pointeur, comme les anneaux d'une montre
  (`rayon` du grand, 10 à 80, 20 par défaut ; `epaisseur`, 0,5 à 6). Le grand s'ouvre avec la
  basse, le petit avec les aigus et la montée ; chaque kick fait avancer le grand d'un cran et
  gonfler son rayon, la caisse claire fait reculer le petit ; un drop les ferme en cercles (une
  modulation de leur ouverture).
- **Quatre temps** (`[quatre_temps]`) : quatre points sous le pointeur, un par temps (`taille`,
  1 à 5 ; `ecart`, 4 à 20). Celui du temps en cours saute sur le kick et s'étire en tiret sur la
  caisse claire ; pendant une montée ils se rapprochent, au drop ils sautent en vague et le
  compte repart du premier (Essaim connaît les temps, pas le premier de la mesure). Ils
  s'effacent en moins d'une seconde quand le tempo n'est plus régulier.
- **Anneau** : un anneau qui suit le pointeur avec un temps de retard et s'étire quand il file,
  d'autant plus qu'il est loin derrière (`etirement`, 0 à 0,7, 0,45 par défaut ; 0 : il reste
  rond), vers le pointeur même quand il tourne. Deux formes, seules ou ensemble (deux anneaux ; panneau
  *Anneau*, cases *Lisse* et *Spectre*) :
  - *Lisse* (`[anneau]`) : un trait net (`rayon`, 6 à 60, 13 par défaut ; `epaisseur`, 0,5 à
    6). Il bat avec le kick et devient carré un instant sur la caisse claire (deux modulations,
    de son rayon et de sa forme), s'épaissit avec la basse, se resserre pendant une montée et lâche un écho au drop ; un point y fait un tour par
    mesure de quatre temps quand le tempo est régulier.
  - *Spectre* (`[orbe]`) : le spectre sans les barres, en une seule ligne fermée autour du
    pointeur, étirée comme la forme lisse (`rayon`, 10 à 60, 21 par défaut ; `epaisseur`, 0,5 à
    5 ; `etirement`, le sien). Les bandes de fréquences la déforment (`relief`, 2 à 30 : graves en haut, aigus en bas,
    les deux moitiés en miroir) ; chaque kick en détache un écho qui garde la forme du son, un
    drop trois ; pendant une montée elle se resserre et frise. Elle demande l'analyse en bandes,
    comme le spectre (16 bandes s'il est coupé).
- **Ligne de voix** (`[voix]`) : une petite onde sous le pointeur, comme celle d'un message vocal
  (`largeur`, 20 à 120 ; `hauteur`, 2 à 20 ; `epaisseur`). Les grandes ondulations suivent la
  basse et le kick, les moyennes la caisse claire, les fines le charleston et la montée ; un drop
  l'élargit un instant. Elle ne passe jamais sur le pointeur et s'efface après 1,5 s sans son.
- **Trame** (`[trame]`) : une grille de points fixe à l'écran (`pas`, 6 à 24), visible seulement
  autour du pointeur, comme sous une loupe (`rayon`, 20 à 120, 46 par défaut ; `taille` des
  points, 0,3 à 3) ; un vide de 7 px est gardé sous le pointeur. La basse agrandit la loupe ; les
  coups la traversent en vagues, ronde pour le kick, en losange pour la caisse claire, plus grande
  au drop ; le charleston fait scintiller quelques points ; une montée les aspire vers le centre.
- **Trait** (`[pinceau]`) : rien au repos. Quand la souris file (plus de 120 px par seconde à
  100 %), elle laisse un trait de pinceau, plus épais quand elle va vite (`largeur` à pleine
  vitesse, 2 à 12), qui s'efface en `duree` (0,2 à 1 s, 0,42 par défaut). La basse l'élargit,
  chaque kick y fait glisser une perle de la tête à la queue, un drop le fait durer plus longtemps
  (une modulation de sa durée).
  Il prend le dégradé de sa couleur de la tête à la queue : celle de l'essaim, ou une teinte à lui
  (`couleur = "fixe"`, ci-dessous).
- **Pendule** (`[pendule]`) : un petit poids (`taille`, 1 à 6) pendu au pointeur par un fil
  (`longueur`, 8 à 60, 22 par défaut). Il se balance quand la souris bouge, plus ou moins
  longtemps (`frottement`, 0,3 = longtemps à 6 = il s'arrête vite, 1,8 par défaut) ; chaque kick le fait
  sauter (le fil se détend puis se retend), la caisse claire lui donne une pichenette, à gauche
  puis à droite ; le fil s'allonge avec la basse et se raccourcit pendant une montée.
- **Gouttes** (`[gouttes]`) : une ligne liquide autour du pointeur. Un noyau et ses perles en
  orbite (`perles`, 0 à 8, 4 par défaut ; `vitesse` sur leur orbite, 0 à 3), dont on ne voit que
  le contour commun (`epaisseur`, 1 à 4) : la ligne se soude quand deux gouttes se touchent et se
  sépare quand elles s'éloignent ; l'intérieur reste vide, on voit l'écran à travers. Comme un
  liquide, chaque goutte traîne derrière la souris au bout de son propre ressort (`souplesse`, 0 =
  d'un bloc, 1 = très liquide, 0,5 par défaut) : l'ensemble s'étire, se sépare, les gouttes
  libres restent un instant en arrière, puis tout revient se souder. Le noyau s'élargit avec la
  basse, une montée fond tout en un seul anneau ; ses gestes : le battement projette les perles,
  l'accent détache une goutte d'une perle, l'apogée lâche cinq petites bulles. `taille` (0,6 à
  1,6) agrandit ou réduit l'ensemble (à 1, les perles tournent jusqu'à 35 px du pointeur).

Ces neuf figures laissent le pointeur dégagé et s'utilisent seules ou avec l'essaim : ce sont des
alternatives aux comètes, aussi réglables qu'elles. Chacune a, en plus de ses réglages propres,
son **allure** (mêmes clés dans sa section) : `couleur` (`"essaim"` par défaut : les couleurs de
l'essaim, tête et queue des traînées, qui défilent ou suivent l'application avec lui, `teinte`
degrés plus loin, -180 à 180 ; `"fixe"` : une couleur à elle, `teinte_fixe`, 0 à 360, `saturation` et
`luminosite`, 0 à 1 (1 et 0,66 par défaut ; 0 : noire), choisies aussi par la pastille *Couleur
fixe* de son panneau, sa queue `degrade` degrés plus loin, 0 à 180, 40 par défaut ; `"blanc"`), `echelle` (0,5 à 2), `opacite`
(0,1 à 1), `rotation` (tours par minute autour du pointeur, -30 à 30 : arcs, anneau, orbe et
gouttes), `decalage_x` et `decalage_y` (sa place par rapport au pointeur, -80 à 80 px). Le trait
et le pendule n'ont ni échelle, ni rotation, ni décalage. Ses réactions aux coups du son sont
ses gestes, des déclencheurs (voir « Déclencheurs ») ; la basse et la montée, elle les entend
elle-même, et les modulations peuvent viser chaque figure (`sur`, ci-dessus), avec ses cibles
propres. Dans la fenêtre des réglages, *Copier sur toutes les figures* (sous *+ Avancé* d'une
figure) donne sa couleur et son opacité aux huit autres (Ctrl+Z
pour revenir, tant qu'aucun preset n'a été appliqué depuis, ici ou ailleurs) ; tant que le
panneau d'une figure active est affiché, le grand aperçu montre une croix au pointeur et, en
pointillés, le cadre de sa zone une fois posée (échelle, décalage), absents de l'export GIF.

- **Habillage de la fenêtre et de l'écran** : comme la bordure, les équerres, le halo et le
  liseré ne sont pas autour du pointeur ; ils accompagnent l'essaim et prennent ses couleurs. Ils
  ne font pas partie des presets : une fois allumés, ils restent quand on change de preset (la bordure,
  elle, est réglée par le preset). Comme elle, ils restent affichés quand la frappe masque le
  pointeur et quand l'essaim s'éteint (veille) ; ils s'effacent en pause et en plein écran.
- **Équerres** (`[equerres]`) : quatre petites équerres aux coins de la fenêtre au premier plan,
  à l'extérieur de son cadre (`bras`, 4 à 30, 10 par défaut ; `epaisseur`, 1 à 5) : rien autour du
  pointeur, rien sur le contenu. Elles ne sortent jamais de la zone de travail de l'écran de la
  fenêtre : quand la place manque dehors (fenêtre agrandie, ancrée au bord, qui passe sous la
  barre des tâches), l'équerre de ce coin se retourne dans le cadre, plutôt que de tomber sur
  l'écran voisin ou la barre des tâches. Équerres et halo sont à l'échelle de l'écran que Windows
  attribue à la fenêtre. Elles s'écartent sur le kick puis reviennent, leurs bras
  s'allongent avec la basse, la caisse claire allume une diagonale puis l'autre, une montée les
  serre contre la fenêtre, un drop les écarte en grand. Le cadre est relu à chaque image : les
  équerres et le halo suivent une fenêtre qu'on déplace ou redimensionne. Ils habillent les mêmes
  fenêtres que la bordure : des fenêtres à cadre ou à barre de titre, la fenêtre des réglages
  d'Essaim comprise ; jamais le bureau, la barre des tâches, une fenêtre en plein écran, ni une
  barre ou un menu sans cadre (la barre d'Horizon et son menu du clic droit, par exemple).
- **Halo de fenêtre** (`[halo]`) : la lumière court le long du cadre de la fenêtre au premier
  plan (`epaisseur` du trait, 1 à 4). Elle entre par le bord le plus proche du pointeur,
  s'élargit avec la basse ; chaque kick en fait partir deux éclats le long du cadre, la caisse
  claire le fait frissonner, une montée le charge, un drop l'embrase en entier. Elle déborde vers
  l'intérieur de la fenêtre, en nappes de plus en plus pâles, jamais au-delà de `profondeur`
  (6 à 30, 14 par défaut) ; rien ailleurs sur le contenu. Elle s'efface après 1,5 s sans son.
  Les équerres en sont la version économe : le halo fait redessiner toute la fenêtre.
- **Liseré** (`[lisere]`) : un filet de lumière au bord bas de l'écran du pointeur (`epaisseur`,
  1 à 8, 3 par défaut), à son aplomb. Il s'élargit avec la basse, chaque kick y court vers la
  gauche et la droite, une montée charge tout le bord, un drop l'allume en entier. Il s'efface
  après 1,5 s sans son. `tous_ecrans = true` le met au bord bas de chaque écran, à l'épaisseur
  de son échelle : les éclats partent toujours de l'aplomb du pointeur et passent d'un écran à
  l'autre.
  Équerres, halo et liseré agrandissent la zone que le moteur redessine (un seul rectangle par
  écran, du pointeur jusqu'à elles) : les équerres seulement quand elles bougent, le halo et le
  liseré tant que la musique joue. C'est léger avec la carte graphique, lourd avec le dessin par
  le processeur.
- **Étiquette du morceau** (`[etiquette]`) : quand le morceau change, son titre et, dessous, son
  artiste s'écrivent à côté du pointeur (à droite et sous la flèche ; à gauche ou au-dessus près
  d'un bord de l'écran), en blanc cerné de sombre pour rester lisibles sur tout fond, soulignés
  d'un filet à la couleur de l'essaim qui bat avec le kick. Ils restent `duree` secondes (1,5 à
  8, 3 par défaut) puis s'effacent ; le reste du temps, rien. `taille` règle le texte (9 à 20).
  Une ligne plus large que 480 px (à 100 %) est coupée, avec « … ». C'est une information, pas une allure de l'essaim :
  comme l'habillage, l'étiquette ne fait pas partie des presets. Il faut que le lecteur annonce
  son morceau à Windows (comme pour l'infobulle). Un morceau changé pendant que l'essaim n'est pas
  dessiné (frappe, pause, veille) n'est pas montré après coup. Pointeur agrandi dans Windows
  (*Accessibilité › Pointeur de la souris*) : l'étiquette, comme les quatre temps et la ligne de
  voix sous le pointeur, s'en écarte d'autant.
- **Bordure** (`[bordure] active`, `repos`) : la bordure de la fenêtre au premier plan prend la
  couleur de l'essaim (Windows 11) et s'illumine sur ses déclencheurs (`bordure`, au kick par
  défaut), même pendant la frappe ; pas en mode sans flashs. La fenêtre des réglages d'Essaim
  se colore comme les autres. Avec *Masquer des captures*, elle n'est pas colorée (elle se
  verrait dans un partage d'écran).

Le son analysé est celui que joue Windows (capture loopback de la sortie par défaut, ou de celle
choisie dans *Écoute › Avancé › Sortie son écoutée*, `[audio] sortie`, propre à l'ordinateur et hors
presets ; si elle est débranchée, Essaim écoute la sortie par défaut en attendant son retour) :
Spotify, navigateur, jeux… sans rien configurer. Pour n'écouter qu'une ou quelques applications
(ta musique, sans les notifications ni la vidéo d'à côté), coche-les dans *Écoute › Avancé ›
Applications écoutées* (`[audio] applis = ["spotify.exe"]`, propre à l'ordinateur et hors
presets ; aucune cochée : tout le son). La liste montre les applications qui ont du son ouvert,
celles qui jouent en ce moment en tête, marquées « en lecture »
(*Actualiser* la relit) ; une appli lancée plus tard est écoutée dès qu'elle démarre, un
navigateur avec tous ses onglets. Il faut Windows 10 2004 ou plus récent ; sinon, tout le son
est écouté.

Autres réglages de l'écoute (`[audio]`, panneau *Écoute*) : `actif` (oui par défaut ; non,
Essaim n'écoute plus rien et les aperçus restent muets), `sensibilite_kick` (1 à 10, 5 par défaut :
1 peu sensible, 10 très sensible) et `delai_kick` (écart minimal entre deux kicks, 0,05 à 1 s,
0,1 par défaut).

## Curseurs

Essaim ne dessine pas le pointeur : il reste un vrai curseur Windows. On installe un pack de
curseurs comme d'habitude (clic droit → Installer sur le fichier `.inf` du pack), puis on le
choisit dans l'onglet *03 Curseurs*, qui liste les jeux installés (ceux de l'utilisateur et
ceux de Windows) avec un aperçu, détaille les 17 curseurs du jeu en place et les met en place
comme le panneau de la souris (`src/souris.rs`). Les curseurs animés (`.ani`) sont animés dans
la fenêtre, à leur cadence. Un preset garde le jeu en place quand on l'enregistre, et son
curseur est dessiné au centre de son aperçu ; avec *Installer les curseurs du preset choisi*,
appliquer ce preset remet aussi ce jeu. En réglages : `[souris] jeu` est le nom du jeu de Windows
choisi (vide : Essaim n'y touche pas ; un preset l'enregistre avec lui) et `avec_preset` (oui par défaut ; propre
à la machine, hors presets) l'option *Installer les curseurs du preset choisi*.

Le jeu conseillé avec Essaim est fourni dans `curseurs/Dotter` : **Dotter**, par
[Verticablets](https://www.deviantart.com/verticablets/art/Dotter-856489042), sous licence
CC BY-NC 3.0 (pas d'utilisation commerciale ; voir `curseurs/Dotter/CREDITS.md`). Son pointeur est
un simple point, discret au milieu de l'essaim. Clic droit sur `_install.inf` → *Installer*.

## Utilisation

- À chaque ouverture de la fenêtre des réglages, le logo s'assemble, puis le nom vient se poser
  dans l'en-tête pendant que la fenêtre se dessine (trois secondes ; un clic ou une touche
  l'abrège ; rien si Windows demande moins d'animations).
- Icône de notification : un clic (ou double-clic) ouvre les réglages ; clic droit, le menu :
  *Mettre en pause*, *Réglages…*, *Fenêtre pour le stream*, *Presets* (celui en place coché),
  *Preset suivant* / *précédent*, *Quitter*, avec leurs raccourcis quand ils sont actifs.
  L'infobulle donne le preset en place et l'état (pause, veille) ; l'icône est grisée en pause.
- **Veille** : souris immobile, ni son ni clic depuis `[rendu] veille_apres` secondes (60 ;
  0 = jamais) : l'essaim s'efface, puis plus rien n'est dessiné jusqu'au prochain mouvement, son
  ou clic. En pause et en plein écran, la capture du son s'arrête aussi (sauf si la fenêtre des
  réglages ou la fenêtre pour le stream est ouverte).
- **Plein écran** : l'essaim se met en veille quand la fenêtre **sous le pointeur** est en plein
  écran (jeu, vidéo), qu'elle ait le clavier ou non ; avec un jeu en plein écran sur un écran et
  le pointeur sur l'autre, il continue. Un menu, une notification, une bulle ou la surcouche
  d'une autre appli (Discord, Steam, NVIDIA) au-dessus d'une vidéo en plein écran ne la font pas
  reprendre ; une vraie fenêtre (à barre de titre) passée devant, si, comme une appli sans cadre
  qu'on vient d'activer (cliquée ou choisie par Alt+Tab).
- **Pauses automatiques** (`[pause]`, propres à la machine, hors presets) :
  - `ne_pas_deranger` (oui par défaut) : pause quand Windows ne veut pas être dérangé (appli en
    plein écran sur l'écran du pointeur, jeu Direct3D en plein écran exclusif, mode
    présentation), reprise ensuite ; le calque transparent d'une autre surcouche, même de la
    taille d'un écran, ne compte pas ;
  - `texte` (oui par défaut) : l'essaim s'efface tant que le pointeur est le curseur de texte de
    Windows (zone de saisie, mais aussi texte d'une page web), pour voir ce qu'on écrit même
    quand l'option de Windows « Masquer le pointeur pendant la frappe » est coupée ; la bordure et
    l'habillage restent, comme pendant la frappe. Une appli qui dessine son propre curseur de
    texte n'est pas reconnue ;
  - `batterie` : sur batterie ou avec l'économiseur de batterie, `"rien"`, `"reduire"` (par
    défaut : 30 images/s au plus) ou `"pause"` ;
  - `applis` : pause tant que l'une de ces applications est au premier plan (onglet *04 Auto*,
    avec la dernière application utilisée à ajouter d'un clic). Une appli du Store compte sous
    son propre exécutable ; `ApplicationFrameHost.exe`, qui les héberge, les désigne toutes.
  Toujours, en plus : session verrouillée, déconnectée de la console ou ouverte à distance, et
  écran éteint (« Écran éteint » : délai d'extinction de Windows, la session restant ouverte ;
  pas quand il est seulement atténué).
  La raison s'affiche sous le bouton Pause.
- **Cadence et dessin** (`[rendu]`, propres à la machine, hors presets) : `cadence`, les images
  par seconde de l'essaim (0 à 500 ; 0 par défaut : celle de l'écran, 144… ; 72 : deux fois moins
  de calcul qu'à 144) ; `cadence_repos` (0 à 500, 30 par défaut) : la cadence quand la souris est
  immobile et que rien ne joue depuis 2 s, 0 pour ne jamais ralentir ; `carte_graphique` (oui par
  défaut) : le dessin se fait par la carte graphique, non : par le processeur, plus lourd (en cas
  d'erreur de la carte, Essaim bascule seul sur le processeur et réessaie plus tard).
- **Qualité adaptative** (`[rendu] adaptatif`, oui par défaut) : si une image demande plus de
  4 ms de calcul, la cadence baisse par paliers (120, 60, 30 images/s au plus) et remonte quand
  le calcul redevient léger, ou à l'essai après 20 s sans image lourde ; un essai qui redevient
  lourd double l'attente du suivant (40 s, 80 s… 10 min au plus). *Allégé* ne s'affiche
  sous le bouton Pause que si ce plafond limite vraiment (pas sur un écran à 60 Hz plafonné à
  120, par exemple).
- **Masquer des captures** (`[rendu] masquer_captures`) : l'essaim reste à l'écran mais
  n'apparaît plus dans les captures, enregistrements et partages d'écran (Windows 10 2004 et plus).
  La bordure colorée, elle, est dessinée par Windows dans la fenêtre de l'autre application et
  ne peut pas être cachée des captures : tant que ce réglage est actif, la bordure n'est pas
  colorée.
- **Sans flashs** (`[rendu] sans_flashs`, propre à la machine) : pour les personnes sensibles aux
  flashs. Tout éclat (kick, rafale, drop, et tout déclencheur `eclat`, quels que soient sa force et
  son événement) est à moitié de sa force et au plus trois par seconde, sans halo blanc ; les ondes,
  les carrés et le cœur blanc des étincelles sont plus pâles ; la bordure garde sa luminosité de
  repos (ses déclencheurs ne font rien). L'habillage aussi : le halo et le liseré ne s'embrasent
  plus au drop, leurs éclats sont à moitié de force et trois par seconde au plus, les équerres
  s'allument moitié moins au kick. `"auto"` (par défaut, *Comme Windows* dans les réglages) : seulement quand
  les effets d'animation de Windows sont coupés (*Accessibilité › Effets visuels*), relu chaque
  seconde ; `"oui"` : toujours ; `"non"` : jamais.
- **Fenêtre pour le stream** (*Fenêtre pour le stream* dans le menu de l'icône, le bouton de
  la vue *Application* des réglages, ou `essaim.exe --stream`) : une fenêtre ordinaire où l'essaim
  danse au centre, autour d'un petit point blanc qui tient lieu de pointeur (celui de Windows n'y
  est pas), sur le fond `[stream] fond` (propre à la machine) : `"transparent"` (par défaut) ou
  un fond uni, `"vert"`, `"bleu"`, `"magenta"` ou `"noir"`. Transparent, dans OBS : *Capture de
  jeu* en mode *Capturer une fenêtre spécifique* sur « Essaim — stream », avec *Autoriser la transparence* et
  *Valeurs alpha prémultipliées* (sans cette dernière, les lueurs ont un bord sombre) ; à l'écran,
  la fenêtre paraît sur fond noir. Fond uni : *Capture de fenêtre*, puis un filtre *Incrustation
  chromatique* (ou, sur fond noir, un mode de fusion *Écran*). Elle suit le son et les
  réglages, pas le pointeur, et continue quand l'essaim du pointeur est en pause, en plein écran ou
  masqué par la frappe (le son reste écouté tant qu'elle est ouverte). Elle reste capturable même
  avec *Masquer des captures*. L'essaim y grandit avec la fenêtre et y tient toujours entier, éclats
  et ondes compris, quelle que soit sa forme. Sans `[rendu] cadence`, elle tourne à 60 images/s
  au moins sans suivre tout un écran plus rapide (72 à 144 Hz, 60 à 240 Hz) ; avec une cadence
  fixée, elle la tient (jamais en dessous sur un écran assez rapide, au plus 10 % au-dessus quand
  l'écran ne s'y divise pas juste : des images régulières valent mieux qu'une attente qui en
  sauterait une sur deux).
- **Raccourcis globaux** (`[raccourcis] actif`) : Ctrl+Alt+→ / ← preset suivant / précédent
  (une bulle de l'icône le nomme), Ctrl+Alt+P pause, Ctrl+Alt+F projecteur. Ils passent par
  `RegisterHotKey` : Windows prévient Essaim de ces seules touches, sans hook sur le clavier.
  Pour Windows, AltGr vaut Ctrl+Alt : sur un clavier où AltGr+touche écrit un caractère (AltGr+F donne « [ » en tchèque,
  hongrois, croate…), le raccourci de cette touche n'est pas pris, pour que le caractère s'écrive
  toujours ; la fenêtre des réglages le signale (*04 Auto*), et le projecteur reste à la secousse.
  Une disposition de clavier ajoutée ou retirée est vue dans la seconde.
- **Preset automatique** (`[automatique]`, `actif`, coupé par défaut) : des règles « si … alors … »
  (`regles`, prioritaires), puis un preset par application au premier plan (`applis` :
  `{ appli = "jeu.exe", preset = "Discret" }`, nom de l'exécutable), puis par plage horaire
  (`horaires` : `{ debut = "22:00", fin = "07:00", preset = "Discret" }`, qui peut passer
  minuit). Quand plus aucune règle ne s'applique, les réglages
  d'avant reviennent, sauf ceux retouchés pendant la règle, qui sont gardés (curseurs compris).
  Ils sont gardés dans `auto-avant.toml` tant qu'une règle agit : un arrêt d'Essaim ou du PC ne
  les perd pas.
  Renommer ou supprimer un preset met à jour ou retire les règles qui le citent ; renommer le
  preset qu'une règle a mis en place ne le réapplique pas (tes retouches restent). Les fenêtres
  d'Essaim ne comptent pas. Une application du Store compte sous son propre exécutable
  (`CalculatorApp.exe`) ; une règle écrite sur `ApplicationFrameHost.exe`, qui les héberge, vaut
  pour toutes.
  - Une règle : `{ si = "artiste = Skrillex", alors = "Delta" }`. Clés : `artiste` et `titre` (du
    morceau qui joue, pas en pause), `appli` (exécutable au premier plan, `.exe` facultatif), `bpm`
    (tempo des kicks, quand il est stable), `heure` (« HH:MM »). Opérateurs : `=`, `!=`, `~`
    (contient), et pour `bpm` et `heure` `<`, `>`, `<=`, `>=`. Plusieurs conditions : « et »
    (`bpm > 140 et heure >= 22:00`). Majuscules ignorées ; une valeur peut être entre guillemets
    (`artiste = "Simon et Garfunkel"`).
  - `artiste =` reconnaît aussi un des artistes d'un champ « A, B & C feat. D » ; `bpm =` est vrai
    à 2 BPM près. Une donnée absente (rien ne joue, tempo inconnu) rend la condition fausse.
  - La première règle vraie l'emporte. Un changement dû à la musique (artiste, titre, tempo)
    attend qu'elle soit stable 2 s, tout autre changement 1 s : un tempo qui hésite autour d'un
    seuil ou un Alt+Tab aller-retour ne font pas clignoter les presets. En pause (le son n'est
    plus écouté), le tempo garde sa dernière valeur. Un preset appliqué par une règle sur le
    tempo ne change pas `[audio]` (sinon il changerait la mesure du tempo qui l'a choisi). Une
    règle illisible est ignorée et notée au journal.
- **Fenêtre des réglages** (`essaim.exe --reglages`, *Réglages…*, ou « Essaim - réglages » dans
  le menu Démarrer). Elle est claire ou sombre selon le thème des applications de Windows.
  - *01 Presets* : la grille des presets, chacun animé comme l'essaim autour du pointeur et au
    rythme du vrai son (en silence, sur un kick de démonstration). Recherche, filtre par source
    (fournis, mes presets, communauté), aperçus *Direct*, *Survol* ou *Figé*. Un clic applique ;
    le cadre rouge marque le preset actif. *Enregistrer le preset* garde les réglages actuels
    (`%APPDATA%\Essaim\profils\<nom>.toml`) et peut les proposer à la bibliothèque ; le nom d'un
    de ses presets demande de confirmer le remplacement. Filtre par
    forme ; un arrêt sur une vignette l'affiche en grand à côté ; *Comparer* sur deux presets les
    montre côte à côte avec la liste de leurs réglages différents.
  - *02 Réglages* : des sous-onglets rangés par familles repliables (seule celle du panneau
    ouvert est dépliée d'office ; l'en-tête compte les effets actifs) : *L'essaim* (*Orbite*,
    *Comètes*, *Rubans*, *Ressorts*, *Satellites*, *Couleur*), *Les effets* (*Éclat*,
    *Particules*, *Fissures*, *Réticule et projecteur*, *Barres de spectre*), *Figures* (*Arcs*,
    *Quatre temps*, *Anneau* (formes lisse et spectre), *Ligne de voix*, *Trame*, *Trait*, *Pendule*,
    *Gouttes* : leurs réglages, leur couleur, et leurs gestes dits par ce qu'ils leur font, « Les
    perles sont projetées quand [Kick] » ; sous *+ Avancé*, leur échelle, rotation et place, et un
    bouton qui leur ajoute une modulation ; seuls les paramètres qui agissent sur la figure sont
    proposés ; allumer une figure lui donne ses gestes de toujours si aucun déclencheur ne la vise), *Fenêtre et écran* (*Bordure*, *Équerres*, *Halo de fenêtre*, *Liseré*),
    *Réactions* (*Modulations*, *Déclencheurs*), *Écoute* (*Écoute*, *Étiquette du morceau*) et
    *Cet ordinateur* (le dessin de l'essaim sur ce PC ; liens vers les pauses, les captures et le
    stream, et les pauses par application) ; les panneaux hors presets le disent dans leur
    en-tête ; chacun avec ses réglages courants et *+ Avancé* pour les autres ; flèches haut et
    bas pour passer de l'un à l'autre. Un voyant dit si l'effet
    est là (bascule *Actif* en tête du panneau, ou au moins un déclencheur) ; un effet coupé, ou un
    réglage sans effet (densité du sillage sans sillage…), est estompé mais reste réglable. Chaque
    effet montre ses déclencheurs (*Quand* : un bouton par événement, × pour le retirer, un choix
    pour en ajouter). *Déclencheurs* : une ligne par déclencheur (quand → action, force ou taille,
    nombre ; *+* : délai minimal, chance ; *Tester* le joue dans le grand aperçu) ; *Modulations* :
    une ligne par modulation (source → paramètre, sur qui, quantité, jauge de la source en direct ;
    *+* : seuil, lissage, à l'envers) ; des idées à ajouter d'un clic. Les listes ne proposent que
    ce qui est allumé (le choix *sur* : l'essaim, ses comètes, ses satellites, chaque figure
    allumée ; le paramètre : ce qui agit sur ce groupe, avec le nom qu'il y prend, « Nombre de
    perles », « Longueur du fil »…) ; une modulation ou un geste qui vise un effet éteint est caché,
    compté sous la liste (*Montrer* les affiche), et reprend dès que l'effet revient ; un geste
    d'une figure se règle aussi pour une seule figure ou pour toutes. Sous l'aperçu, *Simuler* :
    une montée suivie du drop (3 s), un kick, une caisse claire, un temps, un changement de piste,
    pour voir les réactions sans attendre la musique ; tant qu'un morceau joue, l'aperçu suit le
    vrai son, même entre deux coups ; écoute coupée, les aperçus restent muets. *Image…* ouvre
    *Trajet d'une image* : une image (choisie, ou glissée sur la fenêtre : PNG, JPEG, GIF, WebP,
    BMP, SVG, ICO, 20 Mo au plus), ou un texte, un emoji ; ses contours s'affichent sur l'image et
    l'essaim les parcourt dans un aperçu. *Seuil* (calculé : *Auto*), *Inverser* (la forme est de
    l'autre côté du seuil), *Le plus grand* ou *Tous, un par anneau* (24 au plus : chaque anneau
    suit le sien, à sa place ; un texte les garde tous, une lettre chacune). Une image à fond
    transparent donne le plus net. Un clic sur l'aperçu y montre l'effet du
    clic. *Exporter en GIF* enregistre trois secondes de l'aperçu (320 px, 20 images/s) dans
    `Images\Essaim` (sans écraser un GIF du même nom) et le montre dans l'Explorateur ; quitter la
    vue l'abandonne.
  - *04 Auto* : les règles du preset automatique (*Si … alors …*, avec le bouton *Artiste en
    cours* qui remplit la condition pour le morceau qui joue ; par application ; par heure ;
    estompées quand le preset automatique est coupé), les pauses par application et les
    raccourcis (ceux qu'une autre application a déjà pris, ou dont la touche écrit un caractère
    avec AltGr sur ce clavier, sont signalés).
  - *Application* (l'engrenage, à droite des onglets) : ce qui tient à Essaim sur ce PC et non
    à l'allure de l'essaim, hors presets. À gauche, la version installée, où en est la mise à
    jour, *Chercher une mise à jour*, *Mises à jour automatiques* et *Ouvrir config.toml* ; à
    droite, le démarrage avec Windows, l'accueil à revoir, les pauses (*ne pas déranger*, sur
    batterie), les captures et le stream.
  - Au premier lancement, un accueil en trois étapes (à revoir depuis *Application*) ; dans *À
    savoir*, un raccourci que Windows n'a pas enregistré est marqué, avec sa raison.
  - En-tête : ce que l'essaim entend. Le témoin *Kick* rougit sur chaque kick ; *Basse* donne le
    niveau du grave (faible → forte) et *Note* sa hauteur (25–83 Hz, grave → aiguë) ; le témoin
    *Aigus* s'allume sur les attaques des aigus. Sous *Pause*, l'état de l'écoute : *Son capté*,
    *Son coupé* (écoute désactivée : bascule *Actif* du panneau *Écoute* éteinte ; fenêtre des
    réglages ouverte, l'écoute continue même en pause ou derrière un plein écran) ou *Son :
    erreur*, *En veille*, et la raison d'une pause automatique ; le titre du morceau en cours (titre et artiste au survol, aussi
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
  - *Surprends-moi* tire des réglages au hasard, en partant des réglages par défaut (pas de
    ceux en place : un réglage avancé changé à la main n'y survit pas ; Ctrl+Z pour revenir) ;
    l'habillage et les réglages de la machine restent. Une fois sur trois
    environ, une ou deux figures (jamais plus) d'allure tirée dans des bornes sages (échelle 0,8
    à 1,3, une fois sur quatre une teinte fixe, gestes de 50 à 150 %), parfois avec une
    modulation sur l'une d'elles.
  - Pause dans l'en-tête ; *Démarrer Essaim avec Windows* dans *Application* (une entrée coupée
    dans le Gestionnaire des tâches est montrée inactive ; la réactiver ici lève la coupure). Une
    entrée qui lance un autre `essaim.exe` (copie déplacée, autre dossier) est montrée non cochée,
    avec une note « pointe vers … » : un clic y met l'exécutable en cours (le même fichier écrit
    autrement, nom court ou chemin sans guillemets suivi d'options, compte comme le même). Activé depuis
    Téléchargements ou le dossier temporaire, le message demande de ranger d'abord `essaim.exe`
    dans un dossier où il restera.
  - Clavier : `/` recherche, flèches dans la grille, Entrée applique, Ctrl+1 à 5 ou flèches
    (Début, Fin) sur les onglets, Ctrl+S enregistrer, Ctrl+Z annuler, Ctrl+P pause, Échap ferme
    une boîte (boîte ouverte : Ctrl+1 à 5 et Ctrl+S n'agissent pas). Ctrl+plus et Ctrl+moins
    agrandissent et réduisent la page, Ctrl+0 la remet à sa taille (Ctrl+molette aussi). Fenêtre
    étroite ou zoomée : colonnes resserrées, puis une seule colonne qui défile.
  - Si Windows demande de réduire les animations, le grand aperçu, la loupe et la comparaison
    restent à l'arrêt, avec un bouton *Lecture*.
  - Préréglages fournis (56), chacun avec sa forme, ses couleurs, ses effets et ses propres
    modulations et déclencheurs (toutes les sources et toutes les cibles de modulation y servent,
    essai `modulations_variees`) : *Défaut* ; *Discret* (petit, lent, sans fissure) ; *Intense*
    (deux anneaux, beaucoup de satellites, ondes, éclats, fissures) ; *Zen* (trois anneaux qui
    respirent au tempo) ; *Viseur* (réticule) ; *Spectre* (barres de spectre) ; *Comète* (une
    comète qui traîne derrière la souris) ; *Néon* (Lissajous magenta et vert) ; *Arcade* (carré
    vert phosphore) ; *Orage* (astroïde bleu, fissures au hasard) ; *Delta* (triangle qui pivote au
    kick) ; *Ruche* (hexagone ambre relié) ; *Huit* (penche du côté qui joue) ; *Spirale* (quatre
    anneaux qui tournent) ; *Mandala* (fleur sur trois anneaux, reliée) ; *Battement* (cœur qui bat
    au tempo) ; *Constellation* ; *Festival* (contretemps, stéréo, ressort) ; *Tourbillon* (le
    trajet des satellites s'emballe au drop) ; *Pollen* (grains qui flottent) ; *Pochette* (teinte
    de la pochette) ; *Caméléon* (forme et couleur de l'icône de l'appli) ; *Aurore* (voiles verts
    et bleus, effacés sans basse) ; *Pulsar* (une onde à chaque temps) ; *Métronome* (balance à
    gauche sur le temps, à droite sur le contretemps) ; *Typhon* (tourne de plus en plus vite
    pendant la montée) ; *Bulles* (gonflent au kick, éclatent sur la caisse claire) ; *Lave*
    (fleur lourde qui jaunit pendant la montée) ; *Glace* (flocon, scintillements, tout ralentit au
    drop) ; *Ricochet* (éclats qui rebondissent, secoué par la caisse) ; *Radar* (faisceau autour
    du réticule) ; *Séisme* (tremble sur les coups, fissures) ; *Disco* (change de couleur à
    chaque temps) ; *Fantôme* (presque invisible sans basse) ; *Anaglyphe* (comètes rouges et
    satellites cyan écartés par la stéréo) ; *Sub* (tout suit la basse) ; *Nuée* (des dizaines de
    satellites) ; *Vortex* (implosions, tout est aspiré) ; *Artifice* (gerbes d'éclats) ;
    *Hypnose* (astroïdes lents, traînées arc-en-ciel) ; *Élastique* (pend au bout d'un ressort) ;
    *Ressorts* (l'essaim par défaut, mais l'orbite bondit au kick sans changer de sens et freine
    sur la caisse claire) ; *Rubans* (trois rubans vrillés, sans tête, dans trois teintes fixes) ;
    *Arcs*, *Quatre temps*, *Anneau*, *Orbe*, *Ligne de voix*, *Trame*, *Trait*, *Pendule* et
    *Gouttes* (chacun une petite figure seule, sans comètes ni satellites) ; *Montre* (des arcs
    qui tournent autour des quatre temps), *Respiration* (une orbe qui enfle sur la basse et vire
    au drop), *Planète* (un anneau décalé et son pendule, qui suit la stéréo) et *Duo* (une
    trame et la ligne de voix) : des figures combinées, avec leur allure et leurs modulations.
    Chacun est refaisable à la main dans les réglages. Les presets ne touchent pas à `[rendu]`,
    `[stream]` (propres à la machine), à l'option des curseurs, ni au projecteur (`actif`,
    `secousse`, `sensibilite`).
- Réglages : `%APPDATA%\Essaim\config.toml`, appliqués aussi dès l'enregistrement du fichier.
- Journal : `%APPDATA%\Essaim\essaim.log` (états, cadence et temps de calcul toutes les minutes ;
  au-delà de 1 Mo, l'ancien devient `essaim.old.log`).
- `essaim.exe --quitter` ferme l'instance en cours ; `essaim.exe --stream` ouvre la fenêtre pour
  le stream (celle de l'instance en cours s'il y en a une). Relancer `essaim.exe` sans option
  pendant qu'il tourne (double-clic) ouvre toujours les réglages de l'instance en cours. Le
  démarrage avec Windows (clé Run de l'utilisateur) lance `essaim.exe --demarrage` : rien ne
  s'ouvre. Une ancienne clé sans cet argument est complétée par l'instance 30 s après son
  lancement ; d'ici là, un second lancement sans option dans ces 30 s est ignoré.
- `essaim.exe --restaurer-bordure` rend sa couleur à la fenêtre restée colorée si l'appli a été
  tuée de force (fermeture normale, plantage, pause et plein écran le font tout seuls, et le
  lancement suivant aussi).

## Mises à jour

La version installée est écrite en petit à côté du titre de la fenêtre des réglages (« vX.Y.Z »),
en grand dans la vue *Application*, et au début du journal à chaque lancement.

Trente secondes après le lancement, puis toutes les six heures, Essaim regarde la dernière
version publiée sur GitHub (`KingsRequin/essaim`, *Releases*). Si elle est plus récente :

1. une notification annonce le téléchargement de `essaim.exe` ;
2. son empreinte SHA-256 est comparée à celle publiée par GitHub, et sa signature à la clé
   de l'auteur intégrée à Essaim : un fichier abîmé ou qui ne vient pas de l'auteur est refusé ;
3. le nouveau fichier prend la place de l'exécutable en cours, et l'ancien devient
   `essaim.exe.ancien` (effacé au lancement suivant, dans les secondes qui suivent) ;
4. une seconde notification propose de redémarrer. Clic dessus, ou *Redémarrer avec la version…*
   dans le menu de l'icône. Sans clic, la nouvelle version démarre avec la session suivante. À
   son premier lancement, une notification « Essaim X.Y.Z installé » le confirme ; redémarrée
   depuis la fenêtre des réglages, elle la rouvre.

Si la mise en place échoue après la première notification (réseau coupé, fichier refusé,
exécutable tenu), une notification le dit avec la cause, une fois par version.

Sans attendre : *Chercher une mise à jour*, dans la vue *Application* des réglages, lance la
même recherche tout de suite, même si les mises à jour automatiques sont coupées. Le résultat
s'affiche au-dessus du bouton : *À jour* avec l'heure de la recherche, *Recherche impossible*
(GitHub injoignable…) ou *Mise à jour vers la version… impossible* (version trouvée mais pas
mise en place) avec la raison, ou la version téléchargée avec un bouton *Redémarrer*.

Le dossier de l'exécutable doit être modifiable (c'est le cas de
`%LOCALAPPDATA%\Programs\Essaim`, pas de `Program Files`) : sinon, rien n'est téléchargé et
*Application* le dit. Couper : *Mises à jour automatiques*
dans *Application*, ou `[mise_a_jour] auto = false`. Les échecs (pas de réseau, fichier
refusé) sont notés dans le journal et affichés dans *Application*, et Essaim réessaie six heures
plus tard.

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
GitHub en fait une copie et une pull request ; une fois celle-ci relue et fusionnée, le dépôt
reconstruit `index.json` tout seul (workflow *Index*) et le preset apparaît dans la bibliothèque
quelques minutes plus tard. Le fichier arrive déjà rempli. Un preset trop long pour l'adresse (plus de 8 000 caractères une fois
encodé, par exemple le trajet d'une image très détaillée) est copié dans le presse-papiers : le
fichier ouvert le rappelle, il suffit de l'y coller (Ctrl+A, puis Ctrl+V). Une entrée de
l'index mal formée (section qui n'est pas une table) est ignorée. Champ facultatif `apercu` :
chemin relatif d'une image PNG ou GIF du dépôt (`"apercus/Pollen.gif"`), montrée dans la vignette
quand l'aperçu est figé ; sans lui, ou si l'image ne se charge pas, l'aperçu est calculé.
