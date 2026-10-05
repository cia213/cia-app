# Maintien de la dernière image en fin de rendu

Une capture NVIDIA nominalement à 360 FPS a reproduit le refus de durée : source de 64,8533 s, rendu Smoothie de 64,52 s à 25 FPS. FFprobe a décodé les 23 316 images de la source ; la cadence et les durées des paquets sont variables. Le déficit temporel du rendu n'établit donc pas une corruption de la dernière seconde du fichier.

Le rendu calcule maintenant le nombre d'images cible depuis la durée source, la vitesse et les FPS demandés. Après le changement de vitesse, FFmpeg normalise la cadence, maintient la dernière image disponible avec `tpad=stop_mode=clone`, puis borne la sortie au nombre d'images cible. Les contrôles de durée, de géométrie, d'audio et de cadence restent actifs ; le nombre d'images attendu est aussi vérifié.

L'audio est borné indépendamment à la même durée. Il continue pendant l'arrêt sur image et reçoit du silence en fin de piste si nécessaire. Rubber Band reste utilisé pour la vitesse, avec `pitch=1.0`. Les pistes courtes ne provoquent plus la coupure générale qu'entraînait `-shortest`.

## Vérifications locales

| Essai sur la capture complète | Vidéo | Audio | Images à 25 FPS |
| --- | --- | --- | --- |
| Avant correction, ×1,00 | 64,52 s | 64,856 s | 1 613 |
| Avec correction, ×1,00 | 64,84 s | 64,84 s | 1 621 |
| Avec correction, ×1,07 | 60,60 s | 60,60 s | 1 515 |

Ces trois essais utilisent Smoothie et le FFmpeg embarqué avec NVENC. Les durées corrigées sont arrondies à l'image la plus proche ; à 25 FPS, la différence avec la durée théorique reste inférieure à 20 ms. L'empreinte SHA-256 de la capture originale est inchangée.

Le test `smoothie_encoder_clones_missing_tail_without_cutting_audio` utilise x264 sans perte pour vérifier que dix images ajoutées sont identiques à la dernière image valide, que les 35 images à 25 FPS donnent 1,4 s, que l'audio est conservé jusqu'au bout et que le cas sans audio termine aussi. Le test Smoothie complet couvre également correction couleur, LUT, ratio, vitesse ×1,07/×0,80, hauteur audio, absence d'écrasement et nettoyage. Les 16 tests Rust ordinaires et clippy passent.

Cette correction traite une fin de flux vidéo trop courte lorsqu'au moins une image valide est disponible. Un fichier sans image décodable ou un échec explicite du moteur reste une erreur ; aucune image originale manquante n'est reconstituée.
