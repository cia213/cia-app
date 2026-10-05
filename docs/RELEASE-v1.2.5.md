# cia render v1.2.5

Cette version corrige l'affichage du rendu après interpolation et les échecs de durée en fin de capture NVIDIA.

- Après interpolation, la progression affiche les FPS du fichier réellement rendu : par exemple **360 → 25 FPS**. L'enchaînement automatique utilise aussi les bonnes valeurs.
- **SPEED** apparaît dans les détails du rendu, réglable de **×0,50 à ×2,00**, avec **×1,07 par défaut**. Choisir ×1,00 conserve la vitesse d'origine.
- L'accélération audio utilise **Rubber Band avec conservation de la hauteur des voix**. Durée, progression et aperçu suivent la vitesse sélectionnée, même si les réglages sont modifiés pendant un traitement.
- Si le flux vidéo fourni par Smoothie se termine trop tôt, la sortie maintient **la dernière image valide jusqu'à la durée prévue**. L'audio continue et n'est plus coupé par la fin prématurée de la vidéo.
- Le nombre d'images est borné à la cadence demandée et contrôlé avant publication du résultat. Les autres contrôles de durée, ratio, audio et intégrité restent actifs.

Le cas NVIDIA signalé a été reproduit : le rendu de **64,52 s** passe à **64,84 s à 25 FPS**, avec huit images de maintien en fin de vidéo. À ×1,07, vidéo et audio durent **60,60 s**. L'arrondi se fait à l'image la plus proche. [Détails et validation](https://github.com/cia213/cia-app/blob/v1.2.5/docs/CORRECTION-FIN-RENDU.md).

Le maintien d'image ne reconstitue pas des images perdues. Un fichier sans image décodable ou un échec explicite du moteur reste signalé.

Windows x64 ; Smoothie et les outils média sont inclus. RIFE reste facultatif. La signature de mise à jour utilise la même clé publique que les versions précédentes.
