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
    moteur processeur (Direct2D découpe chaque trait lissé sur le processeur). Le lot est
    crénelé : la pastille est prérendue à 64, 32, 16, 8 et 4 px sur une planche, chaque sprite
    prend la taille qui lui va (`sourceRectangles`), et aucune n'a moins de 1 px de rayon (opacité
    réduite d'autant) : un trait fin reste continu et un petit point ne clignote pas en bougeant
    (essai relu : `cargo test --release traits_fins_relus -- --ignored`) ; 20 000 pastilles au
    plus par trait ou anneau (un trait démesuré garde sa longueur, pastilles espacées), 500 000
    par image en garde-fou de mémoire, dont 300 000 au plus pour les traînées de l'essaim
    (`Canvas::trainee`, comptées à part) : les têtes, particules, éclats, figures, l'étiquette et
    l'habillage (un halo sur une fenêtre agrandie en 4K en pose ~33 000) ont toujours au moins
    200 000 places et ne sont jamais coupés ; après une pointe, la place des listes de
    pastilles est rendue dès qu'elle dépasse 4 fois l'usage de l'image d'avant (et 100 000
    éléments) ; points non finis ignorés ; un anneau plus large
    que son diamètre devient un disque jusqu'à r + largeur / 2, comme le trait de tiny-skia, et
    un anneau plus large qu'un tiers de son rayon est découpé en rangées concentriques en
    quinconce (essais `anneau_epais_sans_rosace` et `anneau_epais_relu --ignored`) ;
  - **processeur** (secours, `layer.rs`) : tiny-skia + `UpdateLayeredWindowIndirect` avec zone
    modifiée ; le dessin est découpé à la zone (tracé dans une image de sa taille, puis recopié ;
    directement dans le calque, effacé, quand la zone le couvre entier : projecteur). Bascule automatique si la carte graphique échoue ; `[rendu] carte_graphique`.
  Les traînées sont en coordonnées écran et s'étirent derrière la souris. Leur durée utile est
  de cinq tours de leur trajet au plus (`render::vie_utile`, vitesse angulaire lissée ~0,5 s :
  au-delà, elle repasse sur elle-même sans rien montrer de plus ; un tour mordait sur des presets
  fournis, qui en font près de 4 basse à fond) et toutes ensemble comptent 60 000 points au plus, partagés
  également (`POINTS_TRAINEES`, 4 000 au plus par traînée ; les plus vieux points partent) entre
  toutes celles que le réglage permet (`render::part_trainee` : comètes × anneaux, maximum de
  satellites, permanents) : la part ne bouge pas quand une salve de satellites arrive, et les
  comètes ne raccourcissent pas.
  Le trajet de chaque groupe (anneau extérieur, satellite le plus écarté, avec son dépassement en
  mode ressorts, décalage et retard du centre compris) est ramené à moins de 1 200 px du pointeur
  (`render::borne_etendue`, rayon et décalage réduits ensemble) : au-delà, les points sont
  retirés (traînée laissée derrière par un saut du pointeur). Le point le plus loin de chaque
  forme vient de `render::etendue_forme` (cercle 1, hexagone 1,05… ellipse et astroïde 1,3 ;
  contours 1,2), pas d'un pire cas commun. Une forme de trajet changée, ou ses contours (autre
  icône au premier plan, autre image : `render::version_contours`, comptée seulement quand les
  contours changent vraiment), vide les traînées de son groupe (comètes ou satellites). Un preset
  appliqué (ou « Annuler », fin d'une règle automatique, « Surprends-moi ») les vide toutes :
  `profils::applications` est incrémenté sous le verrou d'écriture, juste avant l'écriture de
  `config.toml` et seulement si le fichier change (`profils::ecrire_d_un_coup`,
  `config::apply_keys_avant`) ; la boucle le compare à chaque relecture et appelle
  `Swarm::vider_trainees`, comme la fenêtre pour le stream (l'aperçu fait de même quand au moins
  deux valeurs changent d'un coup, `changementsReglages`, ou quand les contours suivis changent). Une modulation ajoutée, retirée ou déplacée remet à zéro les valeurs
  lissées sans toucher aux traînées ; une glissière déplacée ne vide rien (le trajet glisse). Cadence calée sur le
  compositeur (`DwmFlush`), plafonnable par `[rendu] cadence`. Un `DwmFlush` rendu en moins de
  2 ms, moins d'une demi-période après le précédent, n'a rien composé (écran éteint, pas de
  rafraîchissement) : la boucle attend alors elle-même la période de l'écran le plus rapide (le
  compositeur suit celui-là quand les écrans ont des fréquences différentes ; 60 Hz si inconnue)
  au lieu de tourner à vide. Avec une cadence plafonnée, la
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
  principal qui plante (une panique qui sortirait d'une procédure de fenêtre ferait `abort` :
  les procédures de fenêtre attrapent les paniques (`catch_unwind`)) ; une exception non gérée hors Rust passe par un filtre,
  `bordure::installer_filtre_plantage`, qui rend la couleur sans allouer ; seuls `abort` et
  `__fastfail` y échappent, le témoin sert alors au lancement suivant). Ignorées (`bordure::eligible`, aussi utilisé par
  `win::cadre_premier_plan` pour le halo, les équerres et les rebonds) : fenêtres sans cadre ni
  barre de titre (barres et menus superposés, comme ceux d'Horizon), outils
  (`WS_EX_TOOLWINDOW`), barre des tâches, bureau, calques et fenêtre de stream d'Essaim, plein
  écran ; la fenêtre des réglages d'Essaim se colore comme les autres. Avec `[rendu]
  masquer_captures`, aucune bordure n'est colorée (`Border::reset`) : DWM la dessine dans la
  fenêtre d'une autre appli, elle ne peut pas être exclue des captures et se verrait dans un
  partage d'écran. Couleur
  quantifiée, envoyée seulement si elle change (60 fois par seconde au plus) ; une fenêtre qui
  refuse (appli administrateur) est notée une fois au journal. Essai réel :
  `cargo test --release bordure_reelle -- --ignored --nocapture` avec un Bloc-notes ouvert.
- **Plein écran** (jeu, vidéo) sous le pointeur : tout se met en veille et la flèche d'origine
  revient. C'est la fenêtre survolée qui compte, pas celle qui a le clavier
  (`win::plein_ecran_sous`, toutes les 250 ms, une fois par seconde en veille profonde, et tout
  de suite quand le pointeur change d'écran ou sort de la veille profonde, avant de dessiner) :
  `EnumWindows` dans l'ordre Z, première fenêtre
  visible, non réduite, non masquée par DWM (`DWMWA_CLOAKED`), qui n'est ni un calque traversé par
  la souris (d'Essaim ou d'une autre surcouche), ni le bureau (`Progman`, `WorkerW`), ni une
  barre des tâches, ni une infobulle, ni une fenêtre de passage qui ne couvre pas l'écran
  (`win::de_passage` : menu `#32768`, `WS_EX_TOOLWINDOW`, `WS_EX_NOACTIVATE`, fenêtre possédée
  `GW_OWNER`, ou `WS_EX_LAYERED` + `WS_EX_TOPMOST` comme les surcouches de Discord, Steam ou
  NVIDIA), et dont le cadre contient le point. Hystérésis (`win::plein_ecran_retenu`) : la
  fenêtre en plein écran retenue reste tant qu'elle couvre l'écran du pointeur et que la fenêtre
  survolée n'est pas une vraie fenêtre (barre de titre ou bord de redimensionnement) ni la racine
  de la fenêtre au premier plan (`GetAncestor(GetForegroundWindow(), GA_ROOTOWNER)` : une appli
  sans cadre qu'on vient d'activer lève la pause). Un menu,
  une notification ou une bulle au-dessus d'une vidéo en plein écran ne lève donc pas la pause.
  Position du pointeur illisible un instant : l'état est gardé. Un jeu en plein écran
  sur un écran et le pointeur sur l'autre : l'essaim continue ; une vidéo en plein écran survolée
  pendant qu'on tape ailleurs : veille. Compte comme plein écran une fenêtre qui couvre son écran
  (cadre lu par `DWMWA_EXTENDED_FRAME_BOUNDS`) sans être agrandie ni avoir de barre de titre ou
  de bord de redimensionnement : une fenêtre agrandie, barre des tâches masquée, ne compte pas.
  En pause et en plein écran, la capture du son est arrêtée (sauf si la fenêtre des réglages ou la
  fenêtre pour le stream est ouverte).
  Le calque de l'écran reste en place, vide (aucune fenêtre ne disparaît sous la souris) : un
  calque vide reste ainsi au-dessus d'un jeu en plein écran sans bordure ; son effet sur la
  présentation du jeu n'est pas mesuré. Un calque vide depuis 10 s n'est retiré qu'à une image
  qui ne touche pas son écran et dont le pointeur est sur un autre écran (`backend::Image::garder`) :
  à la reprise, celui de l'écran du pointeur n'est jamais retiré puis recréé sous la souris.
- **Clics** : lus par `GetAsyncKeyState` à chaque image (pas de hook global), bouton principal et
  bouton secondaire (clic droit) ; boutons inversés (`SM_SWAPBUTTON`) : le principal est le droit.
  En plus du bit fort (enfoncé), le bit faible (appuyé depuis la dernière lecture) compte un
  clic appuyé et relâché entre deux lectures (tap du pavé tactile ; `main::nouveau_clic`). Ce bit
  est partagé entre processus : une autre appli qui le lit l'efface, ce clic-là reste manqué ;
  il ne crée pas de faux clic. Il est lu une fois avant la boucle pour oublier les appuis
  antérieurs au lancement, et ignoré à la lecture qui suit une position illisible ou une pause de
  Windows (session verrouillée, écran éteint : `pauses::Etat::systeme`) : il garderait les clics
  faits sur le bureau sécurisé (invite de l'UAC, écran de connexion).
- **Rebonds** : cadre affiché de la fenêtre au premier plan (`DWMWA_EXTENDED_FRAME_BOUNDS`, relu
  à chaque image par `main::lire_fenetre` ; mêmes fenêtres que la bordure, `bordure::eligible` :
  ni bureau, ni barre des tâches, ni barre ou menu sans cadre, ni fenêtre réduite ou en plein
  écran, ni calque d'Essaim). Une étincelle ou un grain de poussière qui traverse un bord entre deux
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
  `rafale` passent par là) ; ondes, carrés, implosions et cœur blanc des étincelles à 45 % ;
  bordure sans illumination (ses déclencheurs ne font rien). L'habillage le suit (`Effets::doux`,
  donné par la boucle à chaque tour, `Loin::doux`) : filets (halo, liseré) sans embrasement au
  drop, un éclat à moitié de force toutes les 1/3 s au plus (`loin::Filet::avance`), équerres
  allumées à +25 % au lieu de +50 %. La fenêtre pour le stream fait de même. L'aperçu
  (`ui/apercu.js`, `sansFlashs`) suit `[rendu] sans_flashs`, et pour `auto`
  `prefers-reduced-motion`, que WebView2 tire du même réglage de Windows, par une seule requête
  qui écoute ses changements.
- **Éclat du kick** : le halo blanc est un anneau qui laisse un vide de 6 px (× échelle,
  `render::HALO_VIDE`, réduit à la moitié du halo pour un petit éclat) sous la pointe : le texte visé n'est pas voilé à chaque kick.
- **Écrans** : l'essaim est dessiné sur chaque écran qu'il touche (la traînée passe la
  frontière) ; une traînée laissée à plus de 1200 px par un saut du pointeur est abandonnée
  (sinon elle resterait figée sur l'écran quitté). Quand la configuration des écrans change
  (`WM_DISPLAYCHANGE`, `WM_DPICHANGED` ; pas la zone de travail, que la barre des tâches fait
  bouger), seuls les calques des écrans disparus, déplacés ou redimensionnés sont refaits
  (`Backend::ecrans_changes`) : un calque détruit sous la souris lui ferait perdre son curseur.
  Pour la même raison, un calque n'est jamais caché : effacé, il reçoit une image transparente.
  Écrans d'échelles différentes : l'échelle de l'essaim (simulation et dessin autour du
  pointeur) rejoint celle du nouvel écran en ~0,15 s (`main::lisser_echelle`) au lieu de sauter
  de taille ; après une pause ou la frappe, elle repart de celle de l'écran.
- **Kick** : un seul par coup, même long et ondulant (type 808) : un kick est une attaque, et deux
  attaques doivent être séparées d'au moins `[audio] delai_kick` (0,1 s ; la double pédale du metal
  descend vers 0,08 s).
- **Repos** : souris immobile et pas de musique depuis 2 s → `[rendu] cadence_repos` (30 i/s).
  **Veille** : rien de neuf depuis `[rendu] veille_apres` (60 s) → fondu de 1 s, puis plus aucun
  dessin ; souris, son et clics restent lus toutes les 40 ms (réveil en 0,25 s au plus), le plein
  écran une fois par seconde (relu dès le réveil, avant la première image). Les coups entendus
  pendant la veille profonde sont lus et jetés (`oublier_coups`) : pas de salve au réveil.
- **Carte graphique** : en cas d'erreur, bascule sur le processeur ; nouvel essai 10 s plus tard,
  puis attente triplée à chaque échec (10 min au plus, remise à 10 s après 5 min sans erreur).
  Les calques processeur ne sont remplacés qu'après une image carte graphique réussie ; un calque
  carte graphique est montré après son premier `Commit` réussi. Périphérique perdu (pilote mis
  à jour, réinitialisé : `D2DERR_RECREATE_TARGET`, `DXGI_ERROR_DEVICE_REMOVED`…) : un nouveau `Gpu`
  est créé et les mêmes fenêtres y sont rattachées (aucune fenêtre ne disparaît sous la souris) ;
  bascule sur le processeur seulement si cela échoue, pour une autre erreur, ou pour une seconde
  perte en moins de 10 s.
- **Pauses automatiques** (`pauses.rs`) : `SHQueryUserNotificationState` chaque seconde (pause
  sur `QUNS_BUSY`, `QUNS_RUNNING_D3D_FULL_SCREEN`, `QUNS_PRESENTATION_MODE` ; pas
  `QUNS_QUIET_TIME`, première heure d'un nouveau compte ; `QUNS_BUSY` seulement si une vraie
  fenêtre couvre l'écran du pointeur, `win::plein_ecran_sur_ecran_du` : Windows le renvoie aussi
  pour le calque transparent d'une autre surcouche, et pour un plein écran sur n'importe quel
  écran ; `QUNS_RUNNING_D3D_FULL_SCREEN` et `QUNS_PRESENTATION_MODE` valent pour tous les
  écrans), `GetSystemPowerStatus` toutes les 5 s
  ou sur `WM_POWERBROADCAST` (la sortie de veille, `PBT_APMRESUMEAUTOMATIC`, relit aussi les
  écrans), exécutable au premier plan chaque seconde (une appli du Store, hébergée par
  `ApplicationFrameHost.exe`, est lue dans sa fenêtre enfant `Windows.UI.Core.CoreWindow` ;
  `appli::premier_plan`, aussi pour le preset automatique ; une règle écrite sur
  `applicationframehost.exe`, du temps où l'hôte était donné, vaut encore pour toute appli du
  Store, `AppliVue::noms`). La teinte et l'icône de l'appli suivent la même lecture ; tant que
  l'exécutable lu est l'hôte (au lancement, le contenu n'est pas encore attaché au cadre), il est
  relu toutes les 500 ms (`appli::Suivie`) ; session verrouillée, déconnectée ou à
  distance (`WTSRegisterSessionNotification` → `WM_WTSSESSION_CHANGE`, `SM_REMOTESESSION`),
  prioritaire sur les autres, puis écran éteint, session ouverte (« Écran éteint » :
  `RegisterPowerSettingNotification` de `GUID_CONSOLE_DISPLAY_STATE` sur la fenêtre de l'icône,
  `PBT_POWERSETTINGCHANGE` ; 0 éteint, l'état atténué ne compte pas). Traitées comme le plein
  écran (bordure rendue, son coupé), notées au journal ; raison publiée (`charge::publier_raison`).
  « Allégé » (plafond adaptatif) n'est dit que s'il limite vraiment : pas quand il est au-dessus
  de la cadence choisie, de repos, de la batterie ou de la fréquence de l'écran du pointeur
  (`win::frequence_ecran`, `EnumDisplaySettingsW`, relue au changement d'écran) ; fréquence
  inconnue : la cadence mesurée décide (dit à 90 % du plafond, retiré sous 75 %, pas de
  clignotement entre les deux).
- **Qualité adaptative** (`adaptatif.rs`) : nourrie chaque seconde par le calcul moyen par
  image ; > 4 ms deux secondes de suite → palier suivant (120, 60, 30 ; ceux qui ne réduiraient
  rien sont sautés), < 3 ms dix secondes, ou vingt secondes sans seconde > 4 ms (calcul entre
  3 et 4 ms : remontée à l'essai, qui redescend en 2 s s'il redevient lourd) → palier précédent.
  Chaque essai raté double l'attente avant le suivant (20, 40, 80… 600 s au plus) ; elle revient
  à 20 s après un essai tenu 20 s ou une remontée par un calcul vraiment léger (< 3 ms).
- **Raccourcis globaux** (`win::raccourcis`) : `RegisterHotKey` (Ctrl+Alt+touche), sans hook.
  AltGr vaut Ctrl+Alt pour Windows : avant chaque enregistrement, Ctrl+Alt+touche est traduit
  sur chaque disposition installée (`GetKeyboardLayoutList`, `ToUnicodeEx` avec VK_CONTROL et
  VK_MENU enfoncés, drapeau 4 : l'état des touches mortes n'est pas touché) ; s'il écrit un
  caractère (AltGr+F = « [ » en tchèque), le raccourci n'est pas enregistré, sinon il volerait ce
  caractère dans toutes les applications (`win::caractere_vole`). La liste des dispositions est
  relue chaque seconde (`WM_INPUTLANGCHANGE` ne vient qu'à la fenêtre active), triée (Windows
  la rend dans un autre ordre quand la disposition active change), et les raccourcis réévalués
  si elle change. Message `etat` de la page : `raccourcis_refuses` (libellés) et
  `raccourcis_raisons` (`raccourci`, `raison` : pris par une autre application, ou produit un
  caractère sur ce clavier).
- **Masquer des captures** : `SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)` sur chaque
  calque (`win::masquer_captures`). Les essais par capture `BitBlt` ne voient alors plus l'essaim.
  La bordure colorée, dessinée par DWM dans une fenêtre d'une autre appli, n'est pas excluable :
  elle n'est pas colorée tant que le réglage est actif.
- **Fenêtre pour le stream** (`stream.rs`) : fenêtre ordinaire (classe `EssaimStream`), son
  propre fil, son propre `Swarm` et ses `Effets`. Affichage : périphérique Direct3D 11 (carte
  graphique, sinon WARP), chaîne d'échange DXGI à retournement (`FLIP_DISCARD`), présentée tous
  les `n` rafraîchissements de l'écran de la fenêtre (`stream::intervalle_synchro` :
  `n = fréquence / c` en division entière, de 1 à 4, `c` = `[rendu] cadence` si fixée, sinon
  60 : la cadence obtenue est la plus proche de `c` sans passer dessous ; 90 Hz → 90 i/s, 144 Hz
  → 72 i/s, 240 Hz → 60 i/s, 60 Hz pour 30 → 30 i/s. Quand elle dépasse une cadence fixée de plus
  de 10 % (144 Hz pour 100), une attente la tient (`stream::periode_min`) ; à 10 % près, aucune
  attente, qui ferait sauter un rafraîchissement sur deux ; fréquence relue chaque seconde, la
  fenêtre peut changer d'écran), dessin Direct2D dans son tampon avec le `Pinceau` de `gpu.rs` (lot de
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
  tant qu'elle est ouverte. Pas de `WDA_EXCLUDEFROMCAPTURE` : elle reste capturable. Une seule
  ouverture à la fois (place réservée avant le fil) ; un échec d'ouverture est dit par une bulle
  de l'icône ; sa place est rendue même si son fil panique. Réduite, écran éteint
  (`GUID_CONSOLE_DISPLAY_STATE`) ou session retenue : pas de dessin (essai `DXGI_PRESENT_TEST`
  toutes les 100 ms, coups de la musique jetés pour ne pas partir en rafale) ; seulement
  recouverte, elle reste présentée pour OBS. Affichage perdu : ancien rendu libéré, nouvel essai
  après 1, 2, 4… 30 s, une ligne de journal par minute au plus.
- **Projecteur, réticule, spectre** (`effets.rs`) : le réticule et le spectre s'ajoutent à la
  zone de l'essaim ; le projecteur ajoute l'écran entier du pointeur aux zones à redessiner, le
  temps de l'effet seulement (voile : dégradé radial Direct2D, ou remplissage ligne par ligne sur
  le processeur). Secousse : trois changements de sens horizontaux francs (allers ≥ 60 px,
  < 0,35 s, plutôt horizontaux) en moins de 0,6 s, lus sur les positions de chaque image ; un
  bouton de la souris enfoncé (glissement : sélection, fenêtre traînée) efface le geste en cours
  (`Effets::bouton`). Un Ctrl+Alt+F reçu en pause, en plein écran ou en pause automatique est
  oublié (`Effets::oublier_demande`) ; reçu pendant que Windows masque le pointeur (frappe), il
  attend son retour. Ces
  seuils sont ceux de `[projecteur] sensibilite = 5` (`effets::secousse_exigence`) : la course
  minimale double tous les 5 crans en moins (104 px à 1, 30 px à 10), les deux durées
  s'allongent comme l'inverse de sa racine (0,49 s et 0,85 s à 10). `actif`, `secousse` et
  `sensibilite` sont hors presets (`profils::in_profile`, `HORS_PRESET` dans `ui/app.js`).
- **Étiquette du morceau** (`etiquette.rs`, tenue par la boucle principale) : un changement de
  piste (`media::pistes`) lance l'affichage. Le texte est écrit une fois par morceau dans une
  image (GDI : `CreateFontW` Segoe UI, `ExtTextOutW` en blanc sur une section DIB, la plus forte
  composante de chaque pixel donne sa couverture), puis cerné de sombre (plus forte couverture
  dans un disque de 2 px). Chaque ligne tient dans 480 px × l'échelle (`GetTextExtentExPointW`),
  coupée entre deux groupes de caractères visibles (jamais au milieu d'un emoji composé, d'un
  drapeau ou d'une lettre et de ses accents) avec « … » ; un texte de droite à gauche est écrit
  avec `ETO_RTLREADING` et aligné à droite. Un artiste sans titre est montré à la place du titre. Les moteurs recopient cette image par `Canvas::vignette` : tiny-skia
  la pose, Direct2D la téléverse une fois (`CreateBitmap`, gardée tant que son numéro ne change
  pas) et la dessine après le lot de pastilles (`DrawBitmap`). Posée à droite et sous le
  pointeur, à gauche ou au-dessus faute de place dans l'écran ; le côté pris est gardé et n'est
  quitté pour la droite ou le dessous que s'il y tient avec 48 px de marge en plus
  (`etiquette::HYSTERESE`) : un pointeur qui hésite au bord ne la fait pas sauter. Sa zone
  s'ajoute à celle de l'essaim. Hors des presets, comme l'habillage. Un changement de piste
  survenu pendant que rien n'est dessiné (frappe, pause, veille profonde) est oublié
  (`Etiquette::oublier_pistes`) : jamais d'étiquette en retard à la reprise. Elle prend
  l'échelle de l'écran telle quelle (pas l'échelle lissée de l'essaim : son texte serait réécrit à
  chaque image du glissement).
- **Pointeur agrandi** (Accessibilité, « Taille du pointeur ») : `souris::relire_taille_pointeur`
  lit chaque seconde `HKCU\Control Panel\Cursors\CursorBaseSize` (lecture seule, 32 par défaut) ;
  le décalage de l'étiquette (`etiquette::DECALAGE`) et la place des figures sous le pointeur
  (`figures::sous_le_pointeur` : quatre temps, ligne de voix) sont multipliés par taille / 32
  (`souris::echelle_pointeur`), pour rester hors de la flèche. L'aperçu, qui dessine le pointeur
  du jeu à sa taille normale, n'en tient pas compte.
- **Morceau en cours** (`media.rs`) : `GlobalSystemMediaTransportControlsSessionManager` sur le
  fil « essaim-media » (MTA ; chaque appel asynchrone attendu 2 s au plus, puis annulé et le tour
  sauté : un lecteur figé ne bloque pas le fil), réveillé par
  `CurrentSessionChanged`, `MediaPropertiesChanged` et `PlaybackInfoChanged`, et au moins
  toutes les 5 s (chaque seconde tant qu'une pochette ou une reprise de lecture est attendue). Pochette ignorée au-delà de 16 Mo ou de
  4096 px de côté (vu dans l'en-tête, avant le décodage) ; miniature décodée en 48 × 48
  (`BitmapDecoder`) puis
  `appli::teinte_dominante`. Nouvelle pochette pas encore lisible : relue jusqu'à 3 fois, la
  teinte de la précédente restant en attendant. Priorité des teintes : pochette (pendant la
  lecture) > application ou site > Prisme. Teinte imposée suivie en douceur
  (`main::glisser_teinte`) : elle part de la teinte propre quand elle apparaît et y revient en
  glissant quand elle disparaît, par un écart amorti (~250 ms), lâché sous 0,5°
  (`TEINTE_REJOINTE`). Sans l'API (Windows N), une ligne au journal et rien ne change. Message
  `charge` : `morceau`, `lecture`, `pistes`.

## Interface locale (widget Horizon)

WebSocket `ws://127.0.0.1:28670` (28642 = relais Discord, 28650 = Prisme, 28660 = hooks
d'Horizon), JSON, même forme que Prisme : `get_state`, `set_paused`, `set_config`
(`section`, `key`, `value` : écrit dans `config.toml` en gardant les commentaires, valeurs
hors bornes refusées ; par exemple `bordure`/`repos`), `list_presets` (presets fournis puis ceux de
l'utilisateur : `name`, `builtin`, `active`, relus à chaque demande), `apply_preset` (`name` :
comme le menu de l'icône, curseurs du preset compris ; `ok: true` dès que les réglages sont
écrits, avec `"warning"` si seul le jeu de curseurs n'a pas pu être installé), `open_settings` (fenêtre des réglages,
ouverte par la boucle principale), `open_config` ; événement `state` (`{"event": "state",
"data": …}`, sans `id`) à chaque changement et au moins toutes les 2 s. Ses champs, les mêmes
que la réponse à `get_state` (`main.rs`, publiés par `ipc::Shared::publish`) :
- `paused` : pause demandée par l'utilisateur (icône, raccourci, `set_paused`) ;
- `fullscreen` : en veille pour une fenêtre en plein écran sous la souris ;
- `active` : l'essaim est affiché (ni en pause, ni en plein écran, ni en pause automatique) ;
- `gpu` : vrai avec le moteur carte graphique, faux avec le moteur processeur (secours) ;
- `prisme` : connecté à Prisme ;
- `config` : les réglages en vigueur (`Config` entière, plus les clés d'avant calculées,
  `ipc::config_publiee`) ;
- `preset` : le nom du preset en place, `null` si les réglages ne sont ceux d'aucun (recherché à
  chaque relecture de `config.toml` par `profils::en_place`, dans la liste gardée en mémoire avec
  les réglages de chaque preset déjà mis à plat : seuls les réglages en place sont sérialisés ;
  un preset enregistré ou supprimé sans autre changement ne le fait pas bouger, `list_presets`
  reste juste).

Avant la première publication de la boucle principale (juste après le lancement), `get_state`
répond `{"ok": true, "data": null}` : un client attend l'événement `state` ou redemande.
Clés d'avant les déclencheurs, encore lues et écrites par Horizon : `orbite.vitesse_basse` et
`orbite.eloignement_grave` (les modulations de base basse → vitesse et grave → rayon, calculées à
la publication, réécrites, ajoutées ou retirées par `set_config`) et `fissures.actives` (le
déclencheur clic → fissure) ; `ipc::config_publiee`, `ALIAS`.
Toute connexion avec un en-tête `Origin` est refusée (pages web, mais aussi applis web locales :
Tauri, Electron) ; 8 clients au plus, 3 s au total pour la poignée de main (comptées depuis
l'acceptation : un client qui envoie un octet de temps en temps ne garde pas sa place), messages
de 64 Ko au plus, client fermé après 60 s sans rien recevoir de lui (les événements `state` qu'il
reçoit ne comptent pas) : un client qui ne fait qu'écouter envoie une requête (`get_state`) ou
un ping WebSocket de temps en temps, moins d'une minute d'écart. `open_config` passe une fois toutes les
5 s au plus ; `set_config` et `apply_preset` sont étalés à 20 par seconde, tous clients
confondus (au-delà, la demande attend son tour jusqu'à 2 s, puis « trop de demandes »). Un
jeton aléatoire est écrit au premier lancement dans `%APPDATA%\Essaim\ipc.jeton` ; il n'est pas
encore demandé (Horizon ne l'envoie pas). Le port est ouvert par le fil `essaim-ipc`, 10 essais
à 0,5 s d'écart (l'instance d'avant le tient encore juste après une mise à jour). `set_config`
refuse les sections `mise_a_jour`, `bibliotheque`, `automatique` et `pause` (elles ne changent
que par la fenêtre des réglages ou le fichier). Le plugin Essaim d'Horizon s'en sert (pause au
clic ; dans son panneau, choix du
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
Prisme et Horizon) ; `src/audio.rs` ne fait que publier les résultats pour l'affichage. La
capture suit `[audio] sortie` et `[audio] applis` (`capture::Ecoute::depuis_reglages`) : avec des
applications choisies, `audio-commun` ouvre une capture « process loopback » par application
(racine de son arbre de processus, enfants compris), les mélange et revérifie les processus
toutes les 2 s (README d'`audio-commun`, « Son de certaines applications »). `main::Son` relance
la capture quand la sortie ou les applications changent ; la fenêtre des réglages demande la
liste (`Demande::ApplisSonores` → `capture::applis_sonores_detail`). Si Windows refuse l'écoute
par application, toute la sortie par défaut est écoutée en attendant (nouvel essai toutes les 30 s
dans un fil à part, sans couper l'écoute ni l'analyse ; jamais sur un Windows trop ancien) :
`AudioShared::repli_applis`, montré dans la fenêtre des réglages, effacé à chaque nouvelle
capture. Chaque capture a sa génération : une capture remplacée (fil détaché après 300 ms)
n'écrit plus rien. Un flux rouvert à la même fréquence après moins de 3 s garde l'analyse et le
tempo. Un fil de capture qui panique le dit (`lost`) et est relancé 30 s plus tard.

Tout est causal, échantillon par échantillon, décisions toutes les 5 ms.

- **Kick** : 10 bandes (biquads, pas de FFT ; celles sous 4 kHz sur le son sous-échantillonné) ;
  chaque pic de montée d'énergie est jugé par un petit réseau appris sur de vrais morceaux, 10, 20
  ou 30 ms après l'attaque selon sa netteté (délai médian mesuré : 20–30 ms). Le réseau lit la
  forme de l'attaque et le contexte : l'état d'un petit réseau
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
  réessaie toutes les 30 s. Au-dessus de 48 kHz, le son des aigus, des bandes, de la montée et
  des silences est ramené à 48 kHz (passe-bas d'ordre 6 puis un échantillon sur 2, 4 ou 8) ; le
  kick garde la pleine fréquence.
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
  délai médian 20 à 30 ms. Elle remplace un détecteur à seuils (bande à 2 kHz, enveloppes rapide
  et lente, tenue à 25 ms) qui comptait aussi les kicks distordus du hardstyle. `Musique` lit le
  compteur de l'analyse du kick après chaque paquet ; elle ne calcule plus elle-même que la
  brillance de la montée (`Eclat`) et le niveau du son (le drop par retour de l'énergie).
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
  Calculé seulement si une modulation (sources caisse, montée, drop), un déclencheur (caisse,
  drop), une figure ou l'habillage l'écoute (`Config::analyse_musicale`,
  `AudioShared::set_musique`).
- **Stéréo** : la capture d'`audio-commun` remet après chaque paquet le niveau efficace de chaque
  canal (`Sink::stereo`). Équilibre = (D − G) / (D + G), lissé
  sur 0,25 s de son audible ; les paquets muets (blancs entre deux coups) ne le changent pas, et il
  revient au centre après 0,5 s de silence. Essai réel : groove joué tout à droite → +1,00.
- **Temps, contretemps** (`render::Ecoute`) : la phase du tempo des kicks (`pulsation`) qui
  repasse par 0 (temps), ou passe 0,5 (contretemps).
- **Sortie écoutée** : `capture::start_ecoute` d'`audio-commun`, avec `Ecoute::depuis_reglages`
  (sortie par défaut ou identifiant Windows, applications choisies) ; si la sortie choisie
  disparaît, repli sur la sortie par défaut, et retour sur elle
  dès qu'elle revient (vérification toutes les 2 s).
- **Bandes du spectre** (`src/audio.rs`, `Bandes`) : seulement si l'écoute est active et que
  `[spectre] actif` (ou, à 16 bandes, que la forme *Spectre* de l'anneau, `[orbe]`, est active) ;
  8 à 32 passe-bande (deux biquads f32 en série chacun) répartis en échelle log de 40 Hz à 12 kHz,
  enveloppes 5 ms / 120 ms, niveaux en dB sous le pic récent (plage 36 dB). 0,9 % d'un cœur à
  32 bandes.
- Limites connues : sur un son sans batterie (voix seule, piano, orgue, cordes, contrebasse,
  ASMR), le modèle livré donne environ 1 faux kick par minute sur les 59 sons du banc, qu'il a
  appris ; en validation croisée, 4,1 par minute, dont 3,0 viennent d'un seul son (des
  tapotements d'ASMR) ; 11 par minute sur 31 autres sons sans batterie (chœurs, clavecin, pluie,
  train…). Le kick effleuré du jazz acoustique reste mal reconnu ; les notes fantômes de caisse
  claire de la batterie acoustique sont souvent manquées (rappel 0,46 sur MDB Drums en validation
  croisée).

## Interface : technique et charte

- La fenêtre est une page web (`ui/`, intégrée à l'exécutable) affichée par WebView2 (préinstallé
  sur Windows 11 et sur la plupart des Windows 10 à jour ; sinon, runtime WebView2 Evergreen à
  installer depuis le site de Microsoft ; sans lui, Essaim le dit par une bulle au lieu d'ouvrir la
  fenêtre) via la crate `wry`. Son cache est dans `%APPDATA%\Essaim\webview\`. Elle n'a accès qu'à ses fichiers et à
  `raw.githubusercontent.com` (index de la bibliothèque et images d'aperçu) ; elle ne parle à l'appli que par messages (`src/reglages.rs`).
- Charte graphique « Essaim » : `ui/style.css` en est la référence (jetons clair/sombre,
  typographie, composants). Une seule police, Archivo (Omnibus-Type, licence SIL OFL 1.1,
  `ui/polices/OFL.txt`).
- La fenêtre naît cachée et ne se montre que lorsque la page se dit dessinée (message `prete`,
  envoyé par `ui/app.js` après le premier état, la police et deux images ; au bout de 3 s sinon) :
  montrée tout de suite, elle resterait blanche le temps que WebView2 rende sa première image.
  Son propre fond est peint à la couleur de la page (`WM_ERASEBKGND`) : sur un grand écran
  (4K à 150 %), WebView2 ne présente sa première image que quelques images après l'apparition,
  et c'est ce fond qu'on voit entre-temps. Une ouverture se contrôle par capture de l'écran
  (`BitBlt`), sur chaque écran : `PrintWindow` fait rendre la page et ne montre pas ce blanc.
- **Ouverture animée** (`ui/lancement.js`) : à chaque ouverture de la fenêtre, la page naît sous
  le voile `#ouverture` (`style.css` ; absent si Windows demande moins d'animations). Le logo s'y
  assemble en orbes (SVG, soudure par un filtre de flou durci), puis le nom ESSAIM vient se poser
  dans l'en-tête, sur le titre, mesuré dans la page (même police, même interlettrage : il s'y
  superpose lettre pour lettre), son filet
  devient celui de l'en-tête et les blocs de la fenêtre entrent en décalé (Web Animations) ; le
  voile est alors retiré. L'animation part quand la fenêtre se montre (événement `essaim-prete`
  de la page). Un clic ou une touche l'abrège (un changement de taille aussi, une fois
  le nom parti) ; une erreur le retire. Ni bibliothèque ni fichier en ligne : le temps est mené par des segments et des courbes
  écrits dans le fichier. La même animation, avec sa sortie, fait les GIF du logo du README.
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
  additionnés ; accélération, rayon, taille, lueur, traînée et opacité multipliés, effet à fond et
bornes dans `modulation::multipliee`, que la page reprend pour afficher l'effet réel ; totaux
bornés à ±180 tr/min et 60 rad/s (`[orbite] vitesse_base` va de 0 à 60) ; échelle des figures
de 0,5, pour que le pointeur reste dégagé, à 2,5, pour qu'une figure ne couvre pas l'écran (sa
zone : demi-côté × échelle, plus son trait le plus large × (traits − 1)) ; au plus une inversion
du sens par image) et la teinte
  « sur tout », que la boucle principale passe à `render::set_decalage` (tout ce qui est dessiné
  la prend, bordure comprise). L'angle du trajet de chaque groupe s'accumule ; quand sa rotation
  (réglage plus modulations) retombe sous 2 tours/min, il revient vers 0 par le plus court
  (`render::pas_rotation`, taux 3/s pondéré par la lenteur), sinon une modulation retirée ou un
  drop retombé le laisseraient de travers. La vitesse du trajet de chaque groupe est lissée (~100 ms) vers la
  vitesse de base × son accélération. Chaque groupe a sa `Pose` (rotation, rayon et décalage au
  début et à la fin de l'image) : le trajet des comètes et celui des satellites tournent chacun
  de leur côté (`[orbite] rotation_sur`). Une modulation de la taille élargit la marge de
  `Swarm::bounds`.
- **Ressorts** (`[ressorts]`, `render.rs`) : le rayon de chaque groupe est son rayon modulé plus
  l'écart d'un ressort amorti (`render::ressort` : Euler semi-implicite en petits pas,
  pulsation × pas ≤ 0,25, stable jusqu'à 20 images/s au ressort le plus raide). L'action `rebond`
  lui donne un élan (6,67 rayons/s à la force 1 : +25 % de rayon au sommet avec les réglages par
  défaut ; 20 rayons/s au plus, plusieurs rebonds dans la même image compris). En mode ressorts (`actif`), le ressort suit la cible elle-même (modulations, plus un
  tremblement de 5 % à 46 rad/s pendant une montée) : tout changement de rayon dépasse puis
  revient. Sans rebond ni mode ressorts, l'écart reste exactement nul : les presets d'avant ne
  changent pas (`presets_comme_avant`). `poussee` ajoute 5 radians/s par unité de force (retombée
  4 par seconde), `frein` retire jusqu'à 80 % de la vitesse (retombée 9 par seconde) ; tous deux
  passent par `Swarm::vitesse`. En mode ressorts encore : chaque satellite naît au centre, au
  bout d'un ressort à lui (pulsation tirée de 10 à 16 rad/s, seulement dans ce mode pour ne pas
  décaler la suite des tirages des autres presets) ; sa taille entre avec dépassement en 0,3 s,
  fond de 60 % sur sa vie et sort en 0,35 s ; les têtes sont des traits aux bouts ronds, longs de
  20 ms de trajet (8,7 px au plus à 100 %, fois `etirement`) ; l'éclat est un anneau de 5 tailles
  d'éclat qui sort avec dépassement, dont l'avancée se lit sur `env` (de 1 à 0,02 en 0,42 s).
- **Rubans** (`[rubans]`, `render::draw_ruban`) : la traînée d'une comète tracée par morceaux de
  deux segments, chacun de la largeur de son milieu : `largeur` × sin(π k^0,75)^0,8 (k : 0 en
  queue, 1 en tête), fois la vrille 1 − 0,58 × `vrille` × (1 − onde), onde = ½ + ½ sin(5,5 k +
  phase). La phase défile à 2,4 rad/s et saute de 1,4 rad à chaque caisse claire (rejointe en
  douceur). La tête n'est pas dessinée (elle compte encore pour la constellation). La largeur suit
  la modulation de la taille et tient dans la marge de `Swarm::bounds`.
- **Petites figures** (`figures.rs` : arcs, quatre temps, anneau suiveur) : `Effets` possède
  `Figures`, lui passe le son (`AudioShared::view`) et les coups (sa propre `render::Ecoute`), et
  ajoute leur zone à celle du pointeur. Leur centre est au bout d'un ressort accroché au pointeur
  (14 rad/s, amortissement 0,55), jamais à plus de 40 px de lui : la zone (`Figures::rect`) est
  prise autour de ce centre. Tout ce qui bouge passe par des ressorts bornés
  (`Ressort::vers` : en butée, l'élan est perdu), si bien que la zone contient toujours le dessin
  (essai `les_figures_tiennent_dans_leur_zone`, quatre kicks par image à 20 images/s). Les temps
  se comptent depuis le dernier drop ; la présence du tempo monte en 0,15 s et s'efface en 0,8 s.
  Une figure active demande l'analyse musicale (`Config::analyse_musicale`).
  `figures/plus.rs` en ajoute cinq. L'orbe reçoit les bandes lissées du spectre (`Effets` les
  demande à 16 bandes quand le spectre est coupé) ; son contour est recalculé à chaque image, et
  chaque écho en garde une copie. La trame parcourt les cases de la grille que couvre la loupe
  (alignée sur l'écran : les points ne bougent pas avec le pointeur) ; la somme des vagues est
  bornée, donc aussi la taille et le déplacement d'un point. Le trait garde le trajet du pointeur
  (instant, x, y) : sa zone est le cadre de ce trajet, et un point à plus de 1200 px du pointeur
  est oublié (saut d'écran). Le pendule est une masse sous gravité au bout d'un fil qui ne
  s'étire pas : après chaque pas, le poids est ramené à la longueur du fil et sa vitesse
  recalculée d'après son vrai déplacement (le fil tendu l'entraîne), stable de 20 à 144 images/s
  (essai `le_pendule_reste_au_bout_de_son_fil`).
  Les gouttes en contour (`figures/gouttes.rs`) sont un champ : chaque goutte (noyau, perles,
  bulles) y ajoute r² / (d² + 1), calculé sur une grille de 73 × 73 nœuds (pas de 2 px à 100 %),
  réduite à la partie que les gouttes peuvent atteindre. La ligne où ce champ vaut 1 est tirée
  case par case (marching squares, coupure placée par interpolation entre deux nœuds), en petits
  segments dessinés comme des traits : ni image ni flou, les deux moteurs dessinent comme
  d'habitude. Les ressorts des perles sont bornés et le bord de la grille est toujours hors du
  contour : la ligne reste dans la zone redessinée.
  **Allure et modulations des figures** : chaque figure lit son allure (`config::Allure`,
  aplatie dans sa section par `serde(flatten)`) et ses effets de modulation
  (`modulation::Effets::figures`, gardés par `Swarm` et passés par `Effets::moduler_figures`).
  `Partage::pour` lui donne la basse, la montée, le charleston et la caisse claire à la force de
  ses réactions (plafonnés à 1 : les zones restent bornées), `Elan::de` ses coups (kicks ×
  force, et 0 ou la force pour le kick, la caisse claire, le charleston et le drop) ; chaque
  impulsion est multipliée par cette force. `Figures::poser` en fait une `Pose` par figure :
  angle accumulé (rotation propre plus modulations, en tr/min, plus vitesse × 0,1, le tout ×
  accélération), échelle (`echelle` × rayon modulé), épaisseur des traits (taille modulée),
  décalage (`decalage_x` + décalage modulé × 30 px, `decalage_y`), opacité (`opacite` × opacité ×
  lueur modulées) ; la teinte (`teinte` + teinte modulée) décale ses couleurs
  (`render::prisme_decale`). Le dessin passe par `Posee`, un `Canvas` qui transforme les points
  autour du centre, multiplie les rayons par l'échelle et les épaisseurs par celle des traits ;
  une pose neutre dessine directement (au bit près comme avant). La zone de chaque figure est son
  carré autour du centre, déplacé, agrandi par l'échelle et les traits, et par √2 si elle a
  tourné. Le trait et le pendule, posés sur le trajet du pointeur, ne prennent ni échelle, ni
  rotation, ni décalage.
- **Équerres, halo et liseré** (`loin.rs`) : habillage de la fenêtre et de l'écran, hors des
  presets (`profils::SECTIONS` ne contient ni `equerres`, ni `halo`, ni `lisere` : appliquer un
  preset n'y touche pas, et aucun preset fourni ne les allume). Comme la bordure, il reste quand l'essaim n'est
  pas dessiné : pointeur masqué par la frappe et veille profonde passent par
  `main::habillage_seul`, une image sans zone d'essaim (`layer::AUCUNE`) où seul `Loin` avance
  et dessine ; il n'est jamais estompé avec l'essaim (dessiné hors du fondu de veille). Pause et
  plein écran effacent tout : la boucle le dit à `Loin` (`Effets::oublier_loin`, aussi quand les
  calques sont refaits), qui redonne alors toutes ses zones ; en filet, les équerres au repos
  les redonnent toutes les 3 s (un calque sans zone depuis 10 s est retiré, sauf celui de l'écran
  du pointeur). `Effets` possède
  `Loin`, lui passe le son, le cadre de la fenêtre au premier plan (`win::cadre_premier_plan`,
  relu à chaque image dessinée par `main::lire_fenetre`, aussi pour les rebonds) et l'écran du
  pointeur. `loin::FenetreLue` y ajoute l'écran de la fenêtre (son DPI : halo et équerres y
  sont à sa taille, même si le pointeur est ailleurs) et la zone de travail de l'écran de chacun
  de ses coins (`MonitorFromPoint` par coin : une fenêtre à cheval sur deux écrans a ses coins
  sur l'un et l'autre) ; ces appels à Windows ne sont refaits que quand la fenêtre ou son cadre
  change, au moins chaque seconde, et tout de suite quand les écrans changent
  (`Effets::oublier_loin` vide ce cache). Une équerre qui sortirait de la zone de
  travail de son coin (fenêtre agrandie, ancrée au bord, barre des tâches) se retourne dans le
  cadre. Les éclats du halo sont vidés quand la fenêtre change ; chaque bande du
  liseré a l'épaisseur de son écran. Leurs zones s'ajoutent à celles
  des fissures (`Effets::zones_loin`), et ils sont dessinés sur tous les écrans
  (`Effets::dessiner_loin`). `Backend::frame` redessine, par écran, un seul rectangle qui englobe
  toutes les zones : une zone loin du pointeur l'agrandit d'autant. Les équerres ne donnent donc
  leurs zones que quand leur dessin change (cadre, écart, bras, opacités et couleur comparés à
  ceux de l'image précédente, au quart de pixel près ; anciennes et nouvelles zones ensemble) : au
  repos, plus rien n'est redessiné, et ce qui est affiché reste en place. Une teinte qui glisse
  ne compte que quand l'écart se voit (`EQUERRE_NUANCE`) : une dizaine de fois par seconde. Le
  liseré donne sa bande à chaque image tant qu'il y a du son, puis une dernière fois pour
  s'effacer. Le halo et le liseré sont le même filet de lumière (`Filet` : lueur près du
  pointeur, éclats qui en partent au kick, embrasement au drop, présence qui s'efface sans son),
  posé sur un segment pour l'un, sur le tour du cadre pour l'autre (`Cadre` : abscisse le long
  du cadre, point le plus proche du pointeur, chemin le plus court entre deux abscisses). Le
  moteur n'a pas de pastille floue : la lueur du halo est faite de quatre nappes de traits de
  plus en plus pâles vers l'intérieur. Ses zones sont quatre bandes le long du cadre.
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
compilation piégée) ne peut pas pousser un exécutable chez les utilisateurs. Le nouveau fichier
(`essaim.exe.nouveau`, effacé puis créé à neuf, jamais réécrit à travers un fichier existant)
est écrit puis forcé sur le disque ; l'exécutable en cours est déplacé en `.ancien` (Windows
autorise le renommage d'un exécutable lancé) et le nouveau mis à sa place, chaque fois par
`MoveFileExW` (`REPLACE_EXISTING | WRITE_THROUGH`) avec cinq nouveaux essais espacés (un
antivirus qui tient le fichier) ; à défaut, le nouveau est copié, sinon l'ancien revient : il
reste toujours un `essaim.exe`. Le déplacement de l'exécutable en cours attend que `.nouveau`
puisse s'ouvrir en exclusif (antivirus fini) ; la sortie d'Essaim attend un remplacement en
cours (`maj::attendre_remplacement`, au plus `maj::duree_remplacement_max` : quatre séries
d'attentes complètes, ~15,4 s, plus 5 s de marge) et aucun ne commence pendant la fermeture
(remplacement refusé pour cette raison : `maj::Fermeture`, ni bulle ni état « impossible », la
version est reprise au lancement suivant).
Avant tout téléchargement, le dossier de l'exécutable doit accepter un nouveau fichier
(`maj::inscriptible`, un `essaim.exe.essai-<numéro du processus>` créé puis effacé ; un reste de
ce nom déjà là ne compte pas comme un refus, un effacement raté est noté au journal) : sinon
(Program Files…), rien n'est téléchargé et l'état le dit. Un échec après la bulle
« Téléchargement… » (réseau coupé, empreinte ou signature refusée, exécutable tenu) en ouvre une
seconde avec la cause, une fois par version (`BULLES_ECHEC`). Redémarrer : la boucle s'arrête, la bordure est rendue, le verrou
d'instance unique est fermé, puis le même chemin est relancé, avec `--reglages` si la fenêtre
des réglages était ouverte au moment de la demande. Au lancement, `.ancien` est effacé, et
réessayé sur le fil de mise à jour 2, 7 puis 17 s plus tard (l'instance d'avant le tient le temps
de finir de sortir). La dernière version lancée est notée dans `%APPDATA%\Essaim\version` : une
autre (ou le fichier absent hors premier lancement) donne la bulle « Essaim X.Y.Z installé. »
(`maj::annoncer_version`). Une compilation de travail (debug) ne vérifie rien.

L'avancement tient dans `maj::Etat` (jamais, recherche, à jour, prête, échec de la recherche,
mise en place impossible avec la version trouvée), que la fenêtre
des réglages envoie à la page quand il change (message `maj`, vue Application : « Recherche
impossible » ou « Mise à jour vers la version X impossible »). Son bouton
*Chercher une mise à jour* (`chercher_maj` → `maj::chercher`) lance la même recherche sur un
fil à part, même si `[mise_a_jour] auto` est coupé ; une seule recherche à la fois (`EN_COURS`),
et plus aucune une fois qu'une version attend le redémarrage. *Redémarrer* (`redemarrer`) envoie
`CMD_REDEMARRER` à la boucle principale, comme le menu de l'icône.

Publier une version :

1. monter `version` dans `Cargo.toml`, commiter, puis pousser un tag `vX.Y.Z` identique ;
2. le workflow `.github/workflows/publication.yml` vérifie le tag, l'interface
   (`node scripts/verifier-ui.mjs`), lance les tests et Clippy, compile
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
[audio-commun](https://github.com/KingsRequin/audio-commun) (figée par commit, version 0.7.3 d'après
`Cargo.lock` : `rev` dans `Cargo.toml`), partagée avec Horizon.
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
(`rendre.py stream` ne refait qu'elle). `logo.gif` et `logo-nom.gif` sont l'ouverture animée de
la fenêtre (`ui/lancement.js`), jouée sur fond noir par `scripts/images-readme/logo.html` et
photographiée image par image (`rendre.py logo`). Les captures de la
fenêtre (`presets.png`, `reglages.png`, `curseurs.png`) sont faites à la main, fenêtre en
1440 × 900.

Pour juger l'allure d'un preset fourni dans le vrai moteur, sans lancer l'appli : le test
`exporter_planche` dessine six instants du son d'essai (kick, groove, montée, drop, après le
drop, break) à 200 % sur fond sombre :
`ESSAIM_PLANCHE=<dossier> ESSAIM_PRESET=Rubans cargo test --release exporter_planche -- --ignored`.
