# cia render v1.2.4

Cette version corrige le suivi des traitements et remplace l'intermédiaire RIFE par un flux direct vers FFmpeg.

- Un seul traitement actif, paramètres conservés pendant la navigation, arrêt des descendants à l'annulation et à la fermeture.
- Sorties publiées sans écrasement ; contrôle de durée, cadence, dimensions, ratio et audio avant succès.
- RIFE : modèle sélectionné réellement chargé, FPS fractionnaires/VFR, rotation, sous-titres texte et audio en boost/slowmo.
- Smoothie : ratio conservé, recette sélectionnée et réglages couleur/LUT effectivement appliqués. LUT limitée aux sources SDR BT709 renseignées.
- Aperçu final tiré du résultat, aperçus supplémentaires au survol, cache borné et animations de curseurs arrêtées au repos.
- Options FP16 et NVENC explicites ; FP32/x264 restent les valeurs par défaut.

Les essais locaux RIFE montrent **6–22 % de temps en moins en FP32/x264** et environ **36 % en FP16/NVENC**, sur des extraits de 48 et 120 images. Ces mesures incluent le chargement du modèle ; les profils accélérés ne produisent pas des images ou une compression identiques. [Méthode et mesures](https://github.com/cia213/cia-app/blob/main/docs/PATCH-QUALITE-PERFORMANCES.md).

Profil actuel : SDR H.264 8 bits ; HDR et sous-titres bitmap refusés pour éviter une conversion implicite incorrecte. Les styles ASS avancés ne sont pas conservés lors de la conversion en sous-titres MP4.

Le paquet Windows contient Smoothie et les outils média. RIFE reste une installation facultative nécessitant un GPU NVIDIA CUDA. L'installateur et le manifeste de mise à jour utilisent la clé publique déjà configurée dans les versions précédentes.
