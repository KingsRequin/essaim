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
  RGB. Avec `[couleur] musique`, la teinte glisse en plus avec la basse et la brillance du son.
- **Clics** : une onde de choc (`[clic] onde`) et, au choix, des éclats (`eclats`), en plus des
  fissures.
- **Sillage** : quand la souris file vite, une poussière reste derrière elle (`[sillage]`).
- **Bordure** : la bordure de la fenêtre au premier plan prend la couleur de l'essaim et
  s'illumine sur chaque kick (Windows 11).

Le son analysé est celui que joue Windows (capture loopback de la sortie par défaut) : Spotify,
navigateur, jeux… sans rien configurer.

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

- Icône de notification : *Mettre en pause*, *Réglages…*, *Presets* (celui en place coché),
  *Quitter*.
- **Raccourcis globaux** (`[raccourcis] actif`) : Ctrl+Alt+→ / ← preset suivant / précédent
  (une bulle de l'icône le nomme), Ctrl+Alt+P pause. Ils passent par `RegisterHotKey` : Windows
  prévient Essaim de ces seules touches, sans hook sur le clavier.
- **Preset automatique** (`[automatique]`) : un preset par application au premier plan (nom de
  l'exécutable, prioritaire) ou par plage horaire (qui peut passer minuit). Quand plus aucune
  règle ne s'applique, les réglages d'avant reviennent. Les fenêtres d'Essaim ne comptent pas.
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
  - *04 Auto* : les règles du preset automatique et les raccourcis.
  - Au premier lancement, un accueil en trois étapes (à revoir depuis *Avancé*).
  - En-tête : ce que l'essaim entend. Le témoin *Kick* rougit sur chaque kick ; *Basse* donne le
    niveau du grave (faible → forte) et *Note* sa hauteur (25–85 Hz, grave → aiguë) ; le témoin
    *Aigus* s'allume sur les attaques des aigus.
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
  - *Surprends-moi* tire des réglages au hasard (Ctrl+Z pour revenir).
  - Pause dans l'en-tête ; *Démarrer Essaim avec Windows* dans *Avancé*.
  - Clavier : `/` recherche, flèches dans la grille, Entrée applique, Ctrl+1 à 4 onglets,
    Ctrl+S enregistrer, Ctrl+Z annuler, Ctrl+P pause.
  - Préréglages fournis : *Défaut*, *Discret*, *Intense*, *Point seul*, *Delta*, *Ruche*,
    *Huit*, *Pollen*, *Losange*, *Spirale*, chacun refaisable à la main dans les réglages. Les
    presets ne touchent pas à `[rendu]` (propre à la machine) ni à l'option des curseurs.
- Réglages : `%APPDATA%\Essaim\config.toml`, appliqués aussi dès l'enregistrement du fichier.
- Journal : `%APPDATA%\Essaim\essaim.log` (états, cadence et temps de calcul toutes les minutes).
- `essaim.exe --quitter` ferme l'instance en cours.
- `essaim.exe --restaurer-bordure` rend sa couleur à la fenêtre restée colorée si l'appli a été
  tuée de force (fermeture normale, plantage, pause et plein écran le font tout seuls, et le
  lancement suivant aussi).

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
GitHub en fait une copie et une pull request, à relire avant d'ajouter le preset à l'index.
