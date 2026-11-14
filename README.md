# Essaim

Curseur animé réactif à la musique pour Windows : un point à la place de la flèche, deux comètes
et un essaim de satellites en orbite, et des fissures qui s'ouvrent sous chaque clic.

- **Kick** : inverse le sens de rotation et ajoute 2 à 3 satellites (durée de vie 2,8 s).
- **Quantité de basse** : accélère la rotation.
- **Hauteur de la note de basse** : plus elle est grave, plus les orbites s'éloignent.
- **Couleurs** : « Prisme » (la teinte tourne en continu).

Le son analysé est celui que joue Windows (capture loopback de la sortie par défaut) : Spotify,
navigateur, jeux… sans rien configurer.

## Utilisation

- Icône de notification : *Mettre en pause*, *Ouvrir les réglages*, *Quitter*.
- Réglages : `%APPDATA%\Essaim\config.toml`, appliqués dès l'enregistrement du fichier.
- Journal : `%APPDATA%\Essaim\essaim.log` (états, cadence et temps de calcul toutes les minutes).
- `essaim.exe --quitter` ferme l'instance en cours.
- `essaim.exe --restaurer-curseur` remet les curseurs Windows si l'appli a été tuée de force
  (fermeture normale, plantage et plein écran les remettent tout seuls).

## Fonctionnement

- **Curseur** : la flèche normale de Windows (`OCR_NORMAL`) est remplacée par une flèche
  invisible et le point est dessiné à sa place. Les autres curseurs (texte, lien,
  redimensionnement, attente) restent ceux de Windows ; l'essaim continue autour.
- **Rendu** : une petite fenêtre superposée (transparente aux clics, toujours devant) suit la
  souris ; chaque fissure a la sienne. Dessin par `tiny-skia`, affichage par
  `UpdateLayeredWindow`, cadence calée sur le compositeur (`DwmFlush`).
- **Plein écran** (jeu, vidéo) sur l'écran de la souris : tout se met en veille et la flèche
  d'origine revient.
- **Clics** : lus par `GetAsyncKeyState` à chaque image (pas de hook global).

## Analyse du son (`src/dsp.rs`)

Tout est causal, échantillon par échantillon, décisions toutes les 5 ms.

- **Kick** : grave isolé par 3 passe-bas biquad à 90 Hz (pas de FFT : sa fenêtre étale le corps
  de la caisse claire dans le grave). Attaque = énergie grave au-dessus de sa moyenne lente
  (~350 ms), seuil réglable (`sensibilite_kick`). Garde anti-caisse claire : l'énergie grave doit
  valoir au moins 0,3 × le pic récent (120 ms) de l'énergie 120–1000 Hz. Délai mesuré : 10–20 ms.
- **Niveau de basse** : énergie du même grave en dB, rapportée au pic récent du morceau (plage
  de 20 dB).
- **Hauteur** : autocorrélation du grave sous-échantillonné à ~1 kHz, 25–83 Hz, mise à jour hors
  des kicks.
- Limite connue : une note de basse qui démarre très sèchement (depuis le silence) peut compter
  comme un kick. Une attente de confirmation a été essayée et écartée : elle ratait la moitié des
  vrais kicks (un kick descend en hauteur et son énergie grave monte encore 20–40 ms).

## Développement

Voir `CLAUDE.md`.
