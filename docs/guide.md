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
  centre), `rafale` (éclat, satellites, grande onde), `rebond` (le rayon de l'orbite part vers
  l'extérieur, dépasse et revient, comme au bout d'un ressort), `poussee` (la rotation prend de
  la vitesse, +5 radians/s à la force 1, qui retombe en un quart de seconde), `frein` (la
  rotation perd d'un coup jusqu'à 80 % de sa vitesse, puis repart).
- `force` : pour un éclat, la bordure ou le frein, de 0 à 1 ; pour une onde, des éclats, une
  fissure, un carré, une implosion, de 0,2 à 3 (la taille) ; pour un rebond ou une poussée, de
  0,2 à 3 (l'élan). `delai` : secondes au moins entre deux fois ;
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
- `sur` : `"tout"` (les comètes et les satellites), `"cometes"`, `"satellites"`, `"figures"`
  (toutes les figures autour du pointeur) ou une figure : `"arcs"`, `"quatre_temps"`,
  `"anneau"`, `"orbe"`, `"voix"`, `"trame"`, `"pinceau"` (le trait), `"pendule"`, `"gouttes"`.
  Sur une figure, `rayon` l'agrandit tout entière, `taille` épaissit ses traits, `rotation` et
  `vitesse` la font tourner autour du pointeur (la vitesse dix fois moins que pour l'essaim),
  `decalage` la pousse de côté (48 px à fond), `opacite` et `luminosite` la pâlissent ou la
  ravivent, `teinte` la colore, `acceleration` multiplie sa rotation ; `trainee` n'y fait rien.
  Le trait et le pendule ne tournent, ne grandissent ni ne se déplacent. `quantite` de -1 à 1 (négatif : dans l'autre
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
  qui se resserre puis s'efface (`duree`, 1,2 s) ; le disque gonfle sur les éclats. *Sensibilité à
  la secousse* (`sensibilite`, 1 à 10, 5 par défaut) : à 1, il faut un geste ample et vif
  (allers d'au moins 104 px) ; à 10, un petit geste plus lent suffit (30 px). S'il part tout
  seul, la baisser ; `secousse = false` (*Projecteur lancé par › La secousse de la souris*) ne
  garde que le raccourci. Ces deux réglages, propres à la main et à la souris, ne changent pas
  avec le preset. Sous *+ Avancé* : *Taille du projecteur*, le rayon du disque à la fin de l'effet
  (`taille`, 30 à 300, 60 par défaut), et *Obscurité du projecteur*, l'assombrissement de l'écran
  (`opacite`, 20 à 95 %, 62 % par défaut).
- **Réticule** (`[reticule]`) : deux fines lignes croisées centrées sur le pointeur (`taille`,
  `opacite`), vides au centre, qui s'illuminent sur les éclats. Sous *+ Avancé* : l'épaisseur
  des lignes (`epaisseur`, 0,5 à 6, 1,2 par défaut) et le vide autour du pointeur (`vide`, 4 à
  60, 12 par défaut).
- **Anneau de spectre** (`[spectre]`) : des barres autour du pointeur, une par bande de
  fréquences (`bandes`, 8 à 32, de 40 Hz à 12 kHz), plus longues quand la bande est forte
  (`rayon`, `hauteur`). Avec `masquer_sans_musique` (oui par défaut, *Masquer sans musique*),
  l'anneau s'efface après 1,5 s sans aucun son et revient dès que la musique reprend.
- **Arcs** (`[arcs]`) : deux arcs fins autour du pointeur, comme les anneaux d'une montre
  (`rayon` du grand, 10 à 80, 20 par défaut ; `epaisseur`, 0,5 à 6). Le grand s'ouvre avec la
  basse, le petit avec les aigus et la montée ; chaque kick fait avancer le grand d'un cran et
  gonfler son rayon, la caisse claire fait reculer le petit, un drop les ferme en cercles.
- **Quatre temps** (`[quatre_temps]`) : quatre points sous le pointeur, un par temps (`taille`,
  1 à 5 ; `ecart`, 4 à 20). Celui du temps en cours saute sur le kick et s'étire en tiret sur la
  caisse claire ; pendant une montée ils se rapprochent, au drop ils sautent en vague et le
  compte repart du premier (Essaim connaît les temps, pas le premier de la mesure). Ils
  s'effacent en moins d'une seconde quand le tempo n'est plus régulier.
- **Anneau suiveur** (`[anneau]`) : un anneau fin (`rayon`, 6 à 60, 13 par défaut ;
  `epaisseur`, 0,5 à 6) qui suit le pointeur avec un temps de retard et s'allonge dans le
  mouvement. Il bat avec le kick, devient carré un instant sur la caisse claire, s'épaissit avec
  la basse, se resserre pendant une montée et lâche un écho au drop ; un point y fait un tour par
  mesure de quatre temps quand le tempo est régulier.
- **Orbe** (`[orbe]`) : le spectre sans les barres, en une seule ligne fermée autour du
  pointeur (`rayon`, 10 à 60, 21 par défaut ; `epaisseur`, 0,5 à 5). Les bandes de fréquences la
  déforment (`relief`, 2 à 30 : graves en haut, aigus en bas, les deux moitiés en miroir) ; chaque
  kick en détache un écho qui garde la forme du son, un drop trois ; pendant une montée elle se
  resserre et frise. Elle demande l'analyse en bandes, comme le spectre (16 bandes s'il est coupé).
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
  chaque kick y fait glisser une perle de la tête à la queue, un drop le fait durer plus longtemps.
- **Pendule** (`[pendule]`) : un petit poids (`taille`, 1 à 6) pendu au pointeur par un fil
  (`longueur`, 8 à 60, 22 par défaut). Il se balance quand la souris bouge ; chaque kick le fait
  sauter (le fil se détend puis se retend), la caisse claire lui donne une pichenette, à gauche
  puis à droite ; le fil s'allonge avec la basse et se raccourcit pendant une montée.
- **Gouttes** (`[gouttes]`) : une ligne liquide autour du pointeur. Un noyau et quatre perles en
  orbite, dont on ne voit que le contour commun (`epaisseur`, 1 à 4) : la ligne se soude quand
  deux gouttes se touchent et se sépare quand elles s'éloignent ; l'intérieur reste vide, on voit
  l'écran à travers. Le noyau s'élargit avec la basse, chaque kick projette les perles qui
  reviennent se souder, une montée fond tout en un seul anneau, un drop lâche cinq petites bulles.
  `taille` (0,6 à 1,6) agrandit ou réduit l'ensemble (à 1, les perles tournent jusqu'à 35 px du
  pointeur).
  Ces neuf figures prennent les couleurs de l'essaim (tête et queue des traînées), laissent le
  pointeur dégagé, et s'utilisent seules ou avec l'essaim : ce sont des alternatives aux
  comètes, aussi réglables qu'elles. Chacune a, en plus de ses réglages propres, son **allure**
  (mêmes clés dans sa section) : `echelle` (0,5 à 2), `opacite` (0,1 à 1), `teinte` (degrés
  ajoutés à celle de l'essaim, -180 à 180), `rotation` (tours par minute autour du pointeur, -30
  à 30), `decalage_x` et `decalage_y` (sa place par rapport au pointeur, -80 à 80 px), et la
  force de chacune de ses réactions à la musique, de 0 (aucune) à 2 (double) :
  `reaction_kick`, `reaction_caisse`, `reaction_charleston`, `reaction_basse`,
  `reaction_montee`, `reaction_drop`. Le trait et le pendule n'ont ni échelle, ni rotation, ni
  décalage. Les modulations peuvent viser chaque figure (`sur`, ci-dessus). Dans la fenêtre des
  réglages, *Copier sur toutes les figures* (sous *+ Avancé* d'une figure) donne sa teinte, son
  opacité et ses réactions aux huit autres (Ctrl+Z pour revenir) ; tant que le panneau d'une
  figure active est affiché, le grand aperçu montre une croix au pointeur et, en pointillés, le
  cadre de sa zone une fois posée (échelle, décalage), absents de l'export GIF.
- **Habillage de la fenêtre et de l'écran** : comme la bordure, les équerres, le halo et le
  liseré ne sont pas autour du pointeur ; ils accompagnent l'essaim et prennent ses couleurs. Ils ne font
  pas partie des presets : une fois allumés, ils restent quand on change de preset (la bordure,
  elle, est réglée par le preset). Comme elle, ils restent affichés quand la frappe masque le
  pointeur et quand l'essaim s'éteint (veille) ; ils s'effacent en pause et en plein écran.
- **Équerres** (`[equerres]`) : quatre petites équerres aux coins de la fenêtre au premier plan,
  à l'extérieur de son cadre (`bras`, 4 à 30, 10 par défaut ; `epaisseur`, 1 à 5) : rien autour du
  pointeur, rien sur le contenu. Elles s'écartent sur le kick puis reviennent, leurs bras
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
  après 1,5 s sans son. `tous_ecrans = true` le met au bord bas de chaque écran : les éclats
  partent toujours de l'aplomb du pointeur et passent d'un écran à l'autre.
  Ces trois figures agrandissent la zone que le moteur redessine (un seul rectangle par écran,
  du pointeur jusqu'à elles) : les équerres seulement quand elles bougent, le halo et le liseré
  tant que la musique joue. C'est léger avec la carte graphique, lourd avec le dessin par le processeur.
- **Étiquette du morceau** (`[etiquette]`) : quand le morceau change, son titre et, dessous, son
  artiste s'écrivent à côté du pointeur (à droite et sous la flèche ; à gauche ou au-dessus près
  d'un bord de l'écran), en blanc cerné de sombre pour rester lisibles sur tout fond, soulignés
  d'un filet à la couleur de l'essaim qui bat avec le kick. Ils restent `duree` secondes (1,5 à
  8, 3 par défaut) puis s'effacent ; le reste du temps, rien. `taille` règle le texte (9 à 20).
  Un titre de plus de 48 lettres est coupé. C'est une information, pas une allure de l'essaim :
  comme l'habillage, l'étiquette ne fait pas partie des presets. Il faut que le lecteur annonce
  son morceau à Windows (comme pour l'infobulle).
- **Bordure** (`[bordure] active`, `repos`) : la bordure de la fenêtre au premier plan prend la
  couleur de l'essaim (Windows 11) et s'illumine sur ses déclencheurs (`bordure`, au kick par
  défaut), même pendant la frappe ; pas en mode sans flashs. La fenêtre des réglages d'Essaim
  se colore comme les autres.

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

- À chaque ouverture de la fenêtre des réglages, le logo s'assemble, puis le nom va prendre sa place dans l'en-tête pendant que la fenêtre se dessine
  (trois secondes ; un clic ou une touche l'abrège ; rien si Windows demande moins d'animations).
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
  la vue *Application* des réglages, ou `essaim.exe --stream`) : une fenêtre ordinaire où l'essaim danse au centre,
  sur le fond `[stream] fond` (propre à la machine) : `"transparent"` (par défaut) ou un fond uni,
  `"vert"`, `"bleu"`, `"magenta"` ou `"noir"`. Transparent, dans OBS : *Capture de jeu* en mode
  *Capturer une fenêtre spécifique* sur « Essaim — stream », avec *Autoriser la transparence* et
  *Valeurs alpha prémultipliées* (sans cette dernière, les lueurs ont un bord sombre) ; à l'écran,
  la fenêtre paraît sur fond noir. Fond uni : *Capture de fenêtre*, puis un filtre *Incrustation
  chromatique* (ou, sur fond noir, un mode de fusion *Écran*). Elle suit le son et les
  réglages, pas le pointeur, et continue quand l'essaim du pointeur est en pause, en plein écran ou
  masqué par la frappe (le son reste écouté tant qu'elle est ouverte). Elle reste capturable même
  avec *Masquer des captures*. L'essaim y grandit avec la fenêtre et y tient toujours entier, éclats
  et ondes compris, quelle que soit sa forme.
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
  - *02 Réglages* : des sous-onglets rangés par familles repliables (seule celle du panneau
    ouvert est dépliée d'office ; l'en-tête compte les effets actifs) : *L'essaim* (*Orbite*,
    *Comètes*, *Rubans*, *Ressorts*, *Satellites*, *Couleur*), *Les effets* (*Éclat*,
    *Particules*, *Fissures*, *Réticule et projecteur*), *Figures* (*Spectre*, *Arcs*,
    *Quatre temps*, *Anneau suiveur*, *Orbe*, *Ligne de voix*, *Trame*, *Trait*, *Pendule*,
    *Gouttes* ; sous *+ Avancé*, leur teinte, rotation, place et réactions, et un bouton qui
    leur ajoute une modulation), *Fenêtre et écran* (*Bordure*, *Équerres*, *Halo de fenêtre*, *Liseré*),
    *Réactions* (*Modulations*, *Déclencheurs*), *Écoute* (*Écoute*, *Étiquette du morceau*) et
    *Cet ordinateur* (le dessin de l'essaim sur ce PC, hors presets) ; chacun avec ses réglages courants et *+ Avancé*
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
  - *Application* (l'engrenage, à droite des onglets) : ce qui tient à Essaim sur ce PC et non
    à l'allure de l'essaim, hors presets. À gauche, la version installée, où en est la mise à
    jour, *Chercher une mise à jour*, *Mises à jour automatiques* et *Ouvrir config.toml* ; à
    droite, le démarrage avec Windows, l'accueil à revoir, les pauses (*ne pas déranger*, sur
    batterie), les captures et le stream.
  - Au premier lancement, un accueil en trois étapes (à revoir depuis *Application*).
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
  - *Surprends-moi* tire des réglages au hasard (Ctrl+Z pour revenir) ; une fois sur trois
    environ, une ou deux figures (jamais plus) d'allure tirée dans des bornes sages (échelle 0,8
    à 1,3, réactions 50 à 150 %), parfois avec une modulation sur l'une d'elles.
  - Pause dans l'en-tête ; *Démarrer Essaim avec Windows* dans *Application*.
  - Clavier : `/` recherche, flèches dans la grille, Entrée applique, Ctrl+1 à 5 ou flèches
    (Début, Fin) sur les onglets, Ctrl+S enregistrer, Ctrl+Z annuler, Ctrl+P pause, Échap ferme
    une boîte.
  - Si Windows demande de réduire les animations, le grand aperçu, la loupe et la comparaison
    restent à l'arrêt, avec un bouton *Lecture*.
  - Préréglages fournis (56), chacun avec sa forme, ses couleurs, ses effets et ses propres
    modulations et déclencheurs (toutes les sources et toutes les cibles de modulation y servent,
    essai `modulations_variees`) : *Défaut* ; *Discret* (petit, lent, sans fissure) ; *Intense*
    (deux anneaux, beaucoup de satellites, ondes, éclats, fissures) ; *Zen* (trois anneaux qui
    respirent au tempo) ; *Viseur* (réticule) ; *Spectre* (anneau de spectre) ; *Comète* (une
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
    Chacun est refaisable à la main dans les réglages. Les
    presets ne touchent pas à `[rendu]`, `[stream]` (propres à la machine), à l'option des
    curseurs, ni à la secousse du projecteur et à sa sensibilité.
- Réglages : `%APPDATA%\Essaim\config.toml`, appliqués aussi dès l'enregistrement du fichier.
- Journal : `%APPDATA%\Essaim\essaim.log` (états, cadence et temps de calcul toutes les minutes).
- `essaim.exe --quitter` ferme l'instance en cours ; `essaim.exe --stream` ouvre la fenêtre pour
  le stream (celle de l'instance en cours s'il y en a une).
- `essaim.exe --restaurer-bordure` rend sa couleur à la fenêtre restée colorée si l'appli a été
  tuée de force (fermeture normale, plantage, pause et plein écran le font tout seuls, et le
  lancement suivant aussi).

## Mises à jour

La version installée est écrite en petit à côté du titre de la fenêtre des réglages (« v0.11.1 »),
en grand dans la vue *Application*, et au début du journal à chaque lancement.

Trente secondes après le lancement, puis toutes les six heures, Essaim regarde la dernière
version publiée sur GitHub (`KingsRequin/essaim`, *Releases*). Si elle est plus récente :

1. une notification annonce le téléchargement de `essaim.exe` ;
2. son empreinte SHA-256 est comparée à celle publiée par GitHub, et sa signature à la clé
   de l'auteur intégrée à Essaim : un fichier abîmé ou qui ne vient pas de l'auteur est refusé ;
3. le nouveau fichier prend la place de l'exécutable en cours, et l'ancien devient
   `essaim.exe.ancien` (effacé au lancement suivant) ;
4. une seconde notification propose de redémarrer. Clic dessus, ou *Redémarrer avec la version…*
   dans le menu de l'icône. Sans clic, la nouvelle version démarre avec la session suivante.

Sans attendre : *Chercher une mise à jour*, dans la vue *Application* des réglages, lance la
même recherche tout de suite, même si les mises à jour automatiques sont coupées. Le résultat
s'affiche au-dessus du bouton : *À jour* avec l'heure de la recherche, la raison d'un échec, ou
la version téléchargée avec un bouton *Redémarrer*.

Le dossier de l'exécutable doit être modifiable (c'est le cas de
`%LOCALAPPDATA%\Programs\Essaim`, pas de `Program Files`). Couper : *Mises à jour automatiques*
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
