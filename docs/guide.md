# Guide d'Essaim

Tout ce que fait Essaim, en détail. Présentation : [README](../README.md) ; fonctionnement interne : [technique.md](technique.md).

## Ce que fait l'essaim

Surcouche réactive à la musique autour du pointeur de Windows : deux comètes et un essaim de
satellites en orbite, un éclat sous le pointeur à chaque kick, et des fissures qui s'ouvrent sous
chaque clic. Le pointeur lui-même reste celui de Windows (voir « Curseurs »).

- **Kick** : inverse le sens de rotation et ajoute 2 à 3 satellites (durée de vie 2,8 s).
- **Quantité de basse** : accélère la rotation.
- **Hauteur de la note de basse** : plus elle est grave, plus les orbites s'éloignent.
- **Aigus** (charleston, cymbales) : chaque attaque fait jaillir des étincelles des têtes
  (`[scintillement]`, passe-haut à 5 kHz, enveloppes rapide et lente).
- **Caisse claire** (`[musique] caisse_claire`, actif par défaut) : sur chaque caisse claire ou
  clap, un carré posé sur la pointe s'ouvre sous le pointeur (0,3 s). Le charleston et le clic
  d'attaque des kicks ne comptent pas ; une caisse claire jouée avec un kick (claps de la house),
  si.
- **Montée et drop** (`[musique] montee`, actif par défaut) : pendant une montée (roulement de
  caisse claire bien plus serré que d'habitude, son qui s'éclaircit), l'essaim se resserre (jusqu'à
  −45 %) et tourne plus vite (jusqu'à +120 %). Au drop (le premier kick après la montée, ou le
  retour brutal de l'énergie après un passage calme ; au plus un toutes les 8 s), il explose :
  éclat, satellites en double, grande onde et gerbe d'étincelles depuis le centre.
- **Contretemps** (`[musique] contretemps`) : quand le tempo des kicks est connu, les têtes
  grossissent un instant au milieu de chaque temps, et les orbites s'écartent un peu.
- **Stéréo** (`[musique] stereo`, 0 à 1, 0 par défaut) : l'essaim penche vers le côté qui joue le
  plus fort, jusqu'à 1,6 fois le rayon de l'orbite. Les blancs entre deux coups ne le ramènent pas
  au centre ; un vrai silence (0,5 s), si.
- **Trajet** : les comètes (`[orbite] nombre` par anneau, `anneaux` emboîtés qui tournent de
  moins en moins vite) suivent un trajet parmi quatorze formes (cercle, ellipse, triangle, carré,
  losange, pentagone, hexagone, étoile, astroïde, fleur, trèfle, cœur, huit, Lissajous ; `forme`),
  ou un trajet dessiné à la main (`forme = "trace"`, points dans `trace`, parcourus à vitesse
  constante), qui peut tourner sur lui-même (`rotation`, tours/min) ; les
  satellites suivent le même trajet, ou le leur (`[essaim] forme`) ; `[essaim] fixes` ajoute des
  satellites permanents, même sans musique. Entre deux images, les traînées suivent le vrai
  trajet (points intermédiaires) : elles restent lisses à grande vitesse.
- **Couleurs** : une teinte de départ qui tourne en continu ou reste fixe ; ou la couleur
  dominante de l'icône de l'application au premier plan (`[couleur] application`, une fois par
  exécutable ; les icônes grises gardent la teinte précédente) ; dans un navigateur, celle du
  site affiché (`[couleur] site` : couleur dominante du haut de la page, sous la barre d'adresse,
  mesurée à chaque changement de page et toutes les 3 s ; une page grise garde la teinte
  précédente) ; ou, si l'outil Prisme de l'auteur tourne (non fourni ici), celle de l'éclairage
  RGB (bascule *Synchro Prisme*, `[couleur] synchro_prisme`). Avec `[couleur] pochette`, la
  teinte dominante de la pochette du morceau qui joue (Spotify, navigateur… : tout ce que montre
  le panneau multimédia de Windows) passe avant l'application, le site et Prisme ; en pause ou
  sans morceau, on retombe sur eux, dans cet ordre. Avec `[couleur] musique`, la teinte glisse en
  plus avec la basse et la brillance du son.
- **Palette** (`[couleur] palette`, 1 à 3 teintes) : les comètes et les satellites se partagent
  plusieurs teintes, chacune décalée de `ecart_palette` degrés (120 : trois teintes à égale
  distance) de la teinte de base, qu'elle soit fixe, qui défile ou imposée (application, pochette…).
  Un satellite garde sa teinte toute sa vie.
- **Constellation** (`[essaim] constellation`) : de fines lignes relient les têtes (satellites et
  comètes) plus proches que 1,1 fois le rayon de l'orbite, d'autant plus vives qu'elles sont
  proches (160 lignes au plus).
- **Changement de piste** (`[musique] rafale_piste`, actif par défaut) : quand le titre change
  pendant la lecture, une rafale (éclat, satellites en double, grande onde) ; rien au lancement
  ni sur une pause.
- **Tempo** : le tempo des kicks est suivi (affiché dans l'en-tête des réglages, « 128 BPM »,
  quand il est régulier). Quand la musique continue sans grosse caisse depuis 2 s (break,
  couplet) alors que ce tempo était connu, l'éclat de l'essaim continue de pulser doucement à
  chaque temps (`[audio] pulsation_tempo`, bascule *Pulser au tempo pendant les breaks*).
- **Clics** : une onde de choc (`[clic] onde`) et, au choix, des éclats (`eclats`), en plus des
  fissures. C'est le bouton principal qui compte, même quand les boutons sont inversés (gauchers).
  Au clic droit (`[clic] droit`, actif par défaut ; le bouton secondaire), un anneau se referme sur
  le point du clic et douze étincelles y sont aspirées. Dans le grand aperçu des réglages, un clic
  droit le montre.
- **Sillage** : quand la souris file vite, une poussière reste derrière elle (`[sillage]`) ; on
  le voit aussi dans le grand aperçu des réglages quand la souris y file.
- **Inertie** (`[orbite] inertie`, 0 à 1, 0 par défaut) : le centre de l'orbite suit le pointeur
  comme au bout d'un ressort : il traîne derrière la souris quand elle file (jusqu'à six fois plus
  loin qu'à 0), la dépasse un peu à l'arrêt, puis revient sur elle.
- **Rebonds** (`[scintillement] rebonds`, actif par défaut) : les étincelles (aigus, éclats, drop,
  clic droit) et la poussière du sillage rebondissent sur les bords de la fenêtre au premier plan,
  dedans comme dehors, en perdant un peu de vitesse. Ni le bureau, ni la barre des tâches, ni une
  fenêtre réduite ou en plein écran.
- **Projecteur** (`[projecteur]`, actif par défaut) : secouer la souris (quelques allers-retours
  rapides de gauche à droite) ou Ctrl+Alt+F assombrit l'écran du pointeur autour d'un disque clair
  qui se resserre puis s'efface (`duree`, 1,2 s) ; le disque gonfle sur les kicks.
- **Réticule** (`[reticule]`) : deux fines lignes croisées centrées sur le pointeur (`taille`,
  `opacite`), vides au centre, qui s'illuminent sur le kick.
- **Anneau de spectre** (`[spectre]`) : des barres autour du pointeur, une par bande de
  fréquences (`bandes`, 8 à 32, de 40 Hz à 12 kHz), plus longues quand la bande est forte
  (`rayon`, `hauteur`).
- **Bordure** : la bordure de la fenêtre au premier plan prend la couleur de l'essaim et
  s'illumine sur chaque kick (Windows 11), sauf en mode sans flashs.

Le son analysé est celui que joue Windows (capture loopback de la sortie par défaut, ou de celle
choisie dans *Avancé › Son › Sortie son écoutée*, `[audio] sortie`, propre à l'ordinateur et hors
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
  flashs. L'éclat du kick (et des rafales et drops) est à moitié de sa force et au plus trois par
  seconde, sans halo blanc ; les ondes et les carrés sont plus pâles ; la bordure garde sa
  luminosité de repos. `"auto"` (par défaut, *Comme Windows* dans les réglages) : seulement quand
  les effets d'animation de Windows sont coupés (*Accessibilité › Effets visuels*), relu chaque
  seconde ; `"oui"` : toujours ; `"non"` : jamais.
- **Fenêtre pour le stream** (*Fenêtre pour le stream* dans le menu de l'icône, le bouton des
  réglages avancés, ou `essaim.exe --stream`) : une fenêtre ordinaire où l'essaim danse au centre,
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
  - *02 Réglages* : les réglages courants appliqués en direct, *+ Avancé* pour tous les autres.
    Un réglage dont l'effet est coupé (fissures, bordure, son, comètes…) est masqué. *Dessiné…*
    ouvre une toile où tracer le trajet d'un geste. Un clic sur l'aperçu y montre l'effet du
    clic. *Exporter en GIF* enregistre trois secondes de l'aperçu (320 px, 20 images/s) dans
    `Images\Essaim` et le montre dans l'Explorateur.
  - *04 Auto* : les règles du preset automatique (*Si … alors …*, avec le bouton *Artiste en
    cours* qui remplit la condition pour le morceau qui joue ; par application ; par heure), les
    pauses par application et les raccourcis.
  - Au premier lancement, un accueil en trois étapes (à revoir depuis *Avancé*).
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
  - Pause dans l'en-tête ; *Démarrer Essaim avec Windows* dans *Avancé*.
  - Clavier : `/` recherche, flèches dans la grille, Entrée applique, Ctrl+1 à 4 ou flèches
    (Début, Fin) sur les onglets, Ctrl+S enregistrer, Ctrl+Z annuler, Ctrl+P pause, Échap ferme
    une boîte.
  - Si Windows demande de réduire les animations, le grand aperçu, la loupe et la comparaison
    restent à l'arrêt, avec un bouton *Lecture*.
  - Préréglages fournis (21) : *Défaut*, *Discret*, *Intense*, *Point seul*, *Pollen*, *Spirale* ;
    un par forme du trajet : *Delta* (triangle), *Ruche* (hexagone), *Huit*, *Losange*, *Ellipse*,
    *Carré*, *Pentagone*, *Étoile*, *Fleur*, *Trèfle*, *Cœur*, *Lissajous*, *Astroïde* ; et
    *Constellation* (lignes entre les satellites, deux teintes) et *Festival* (contretemps, stéréo,
    inertie, palette de trois teintes). Chacun est refaisable à la main dans les réglages. Les
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
dans les réglages avancés, ou `[mise_a_jour] auto = false`. Les échecs (pas de réseau, fichier
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
preset trop long pour l'adresse (plus de 6 000 caractères, par exemple un long trajet dessiné)
est copié dans le presse-papiers : il suffit de le coller dans la page qui s'ouvre. Une entrée de
l'index mal formée (section qui n'est pas une table) est ignorée. Champ facultatif `apercu` :
chemin relatif d'une image PNG ou GIF du dépôt (`"apercus/Pollen.gif"`), montrée dans la vignette
quand l'aperçu est figé ; sans lui, ou si l'image ne se charge pas, l'aperçu est calculé.
