# Bilan du patch qualité et performances

**Bilan final : 1er octobre 2026.** Le patch corrige les risques prioritaires identifiés sur la révision `241f350` et remplace le parcours RIFE avec intermédiaire vidéo par un flux RGB direct vers l'encodeur final. L'audit initial reste consultable dans [AUDIT-QUALITE-PERFORMANCES.md](AUDIT-QUALITE-PERFORMANCES.md) ; la [cartographie](CARTOGRAPHIE.md) décrit le code corrigé.

## Corrections et preuves

| Audit | Changement livré | Validation |
| --- | --- | --- |
| Q1 — historique et calcul simultané | Snapshot immuable du job, navigation indépendante, réservation globale backend avant préparation | Tests des handlers de production et du registre Rust |
| Q2 — écrasement d'intermédiaire | Suppression du MP4 intermédiaire et du glob ; temporaires possédés, publication sans écrasement | Tests no-clobber et de publication tardive ; fixtures avec noms entre crochets |
| Q3 — processus orphelins | Windows Job Object avec kill-on-close, spawn suspendu puis affectation, gardes de nettoyage, annulation pendant préparation | Test réel de terminaison d'un descendant suspendu et tests de réservation/annulation |
| Q4 — ratio Smoothie | Dimensions conservées, SAR transmis ; overrides d'encodage sans arrondi largeur/hauteur indépendant | Fixture réelle Smoothie avec SAR 4:3 |
| Q5 — réglages sans effet | Recette et modèle sélectionnés transmis, LUT configurée, activation du traitement couleur non neutre ; retrait des contrôles scène/blend non appliqués | Contrats Rust, modèle effectivement utilisé, fixture Smoothie couleur/LUT |
| Q6 — cadence et validation | FPS rationnels, cadence moyenne VFR, limites de paramètres, rotation et SAR ; contrat SDR explicite | Fixtures FPS fractionnaires, facteur3, VFR, rotation90 et SAR4:3 |
| Q7 — faux succès/troncature | Erreurs de décodage propagées, pipes bornés, worker et encodeur attendus, comptes de frames et métadonnées validés | Décodage intégral des résultats d'intégration ; échec d'encodeur simulé ; sortie Smoothie audio-seul refusée |
| Q8 — réponses obsolètes/aperçus | Identifiants de requête, annulation native, chargement au survol, cache borné, huit extractions au lieu de seize | Tests de réponses d'analyse obsolètes et vraie extraction des huit PNG |
| Q9 — configuration | Écritures sérialisées et atomiques avec temporaires uniques ; erreur propagée sans faux message de succès ni remplacement des données illisibles | Tests de sauvegarde UI ; vérification source des chemins atomiques |
| Q10 — chemins/sous-titres/couleur | Chemins explicites sans glob, sous-titres texte MP4 et retiming, rejet HDR/bitmap, profil SDR BT709 | Audio AAC copié par hashes de paquets, sous-titres et langues conservés, rejet HDR/no-clobber |
| Q11 — aperçu final | Extraction du fichier réellement produit | Test du handler de résultat et chemin de sortie réel |
| Entretien | Wrapper Python de développement unique, logs bornés, sliders sans RAF permanent, CI vérifiable sans payload de release | Tests, lint et builds |

La contre-revue du patch a aussi corrigé deux cas non couverts par l'audit initial : la sélection d'une couverture vidéo intégrée à la place de la piste réelle, et le chargement permissif des poids RIFE. Le worker utilise l'index du flux analysé et charge un checkpoint complet avec `strict=True`, en acceptant les clés normales ou préfixées `module.`. Un modèle incomplet/incompatible échoue avant le décodage ; aucun résultat calculé avec des poids partiellement chargés n'est présenté comme valide.

## Performances

Les anciennes estimations par composant (FP16, nombre de processus d'aperçu et encodeur) **ne sont pas des gains cumulables**. La première série exploratoire du patch chevauchait une compilation Rust ; elle est conservée comme preuve mais exclue du bilan de gains.

Les mesures définitives sont consignées dans [PATCH-PERFORMANCE.json](PATCH-PERFORMANCE.json), avec la révision initiale gelée dans [BASELINE-PATCH.json](BASELINE-PATCH.json). Le protocole compare des durées de processus entiers, incluant démarrage Python, chargement du modèle, décodage, interpolation, audio et encodage. L'ordre avant/après est alterné ; chaque résultat est contrôlé par FFprobe et décodage intégral. Les hashes du code, de la source et du modèle sont conservés.

Mesures locales sur **RTX 3050 Laptop, 4 Gio VRAM**, source 1680 × 1050 / 120 FPS avec audio, boost ×2. Encodage comparé : x264 CRF18/**medium** ; option NVENC CQ18/p5.

| Profil | 48 images : médiane de 3 essais | Réduction vs ancien | 120 images : 1 essai | Réduction vs ancien |
| --- | ---: | ---: | ---: | ---: |
| Avant : FP32/x264 | 27.07 s | 0.0% | 50.52 s | 0.0% |
| Patch : FP32/x264 | 21.14 s | 21.9% | 47.65 s | 5.7% |
| Patch : FP16/x264 | 17.74 s | 34.5% | 34.59 s | 31.5% |
| Patch : FP16/NVENC | 17.40 s | 35.7% | 32.41 s | 35.8% |

Les 16 sorties chronométrées passent les contrôles de cadence, durée, audio et décodage intégral. La série de 120 images est une vérification supplémentaire unique ; elle ne fournit pas de dispersion statistique. Le gain FP32 varie ici de **5,7 à 21,9 %** ; celui de FP16/NVENC reste proche de **36 %**. Ces valeurs décrivent uniquement ces essais locaux.

| Comparaison des sorties | SSIM 48 images | PSNR 48 images | SSIM 120 images | PSNR 120 images |
| --- | ---: | ---: | ---: | ---: |
| Patch FP32 vs ancien FP32 | 0.969939 | 38.13 dB | 0.970412 | 38.26 dB |
| Patch FP16/x264 vs patch FP32/x264 | 0.973620 | 38.82 dB | 0.973574 | 38.88 dB |
| Patch FP16/NVENC vs patch FP32/x264 | 0.975968 | 39.28 dB | 0.974627 | 39.17 dB |

Les comparaisons d'images utilisent SSIM/PSNR sur les indices communs, sans constituer une évaluation subjective de qualité. L'ancien pipeline ajoutait une compression MPEG-4 intermédiaire ; le patch encode directement les frames interpolées. La tenue de la dernière frame complète N×facteur images, contrairement à l'ancienne fin plus courte. FP16 peut modifier les valeurs calculées ; **CQ18 NVENC n'équivaut pas à CRF18 x264**.

Les extraits courts incluent une part importante de chargement du modèle. Ils ne permettent pas d'annoncer un débit stabilisé sur plusieurs minutes, un gain universel, ni un gain en 4K. Les huit aperçus divisent le nombre de processus par deux, sans annoncer un gain de temps de 51% : la luminance locale n'était pas incluse dans ce microbenchmark initial.

## Validation et reproduction

**Résultats finaux :** 19 tests frontend, 11 tests Python, 14 tests Rust et 4 contrôles Python supplémentaires réussis ; 10 cas RIFE réels ou de rejet avant lancement réussis. Test Smoothie réel réussi dans quatre variantes, avec changements de pixels vérifiés. `cargo fmt --check` et Clippy sans avertissement passent. Build Vite et build natif Tauri en release sans bundle réussis ; JS 133,34 kB / gzip 41,48 kB. Les vérifications Rust sans payload utilisent le même override de ressources que la CI.

Vérifications source seules :

```powershell
npm test
npm run build
python -m unittest discover -s tests -v
$env:TAURI_CONFIG = '{"bundle":{"resources":["resources/time_remap.py","resources/rife_worker.py","resources/bootstrap/bootstrap-rife.ps1"]}}'
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --locked --manifest-path src-tauri/Cargo.toml
Remove-Item Env:TAURI_CONFIG
```

Les tests réels supplémentaires nécessitent le runtime local et le venv CUDA :

```powershell
..\time-remap-app\venv\Scripts\python.exe scripts\test_pipeline_integration.py --run
cargo test --manifest-path src-tauri/Cargo.toml bundled_smoothie_pipeline -- --ignored --nocapture
..\time-remap-app\venv\Scripts\python.exe scripts\benchmark-patch.py --run --frames 48 --repeats 3 --profiles --preset medium --label final-48-clean
..\time-remap-app\venv\Scripts\python.exe scripts\benchmark-patch.py --run --frames 120 --repeats 1 --profiles --preset medium --label final-120-clean
```

Les scripts de mesures conservent les artefacts dans un dossier unique de `time-remap-app/audit-benchmark`. L'ancien moteur reçoit uniquement des copies de médias et du runtime possédées par le benchmark, pour ne pas réintroduire son risque d'écrasement.

## Limites et travaux futurs

- Le profil de livraison actuel est SDR H.264 8 bits. HDR nécessite une vraie conversion de transfert/tone mapping ; les dimensions impaires sont refusées par le backend actuel.
- Le runtime Smoothie local exige des métadonnées BT709 cohérentes pour sa conversion LUT ; une source non renseignée ou incompatible doit être refusée avant ce filtre. Ce comportement a été découvert par un essai réel.
- Les sous-titres texte sont convertis en `mov_text` : les styles ASS avancés ne sont pas conservés. Les sous-titres bitmap sont refusés par RIFE. Le parcours Smoothie ne promet pas de conserver les pistes de sous-titres.
- La production valide les métadonnées et les fins de processus ; elle ne relance pas systématiquement un décodage intégral du résultat. Les fixtures et benchmarks, eux, exécutent ce décodage pour vérifier les vidéos produites.
- Les handlers UI sont testés avec une frontière Tauri simulée ; l'inspection visuelle navigateur couvre ABOUT et l'accès aux paramètres avancés. Aucun test automatisé complet de l'interface native sur une installation Windows fraîche n'est revendiqué.
- Aucun installateur signé ni release n'est publié par ce patch. La compilation native ne remplace pas la validation du payload de distribution et de son installateur.
- L'extraction des monolithes Svelte/Rust, un worker RIFE persistant, TensorRT, les optimisations 4K et une mesure longue avec RAM/VRAM restent des travaux distincts. Ils ne sont pas présentés comme livrés ni inclus dans les gains.

Les auxiliaires d'entraînement `teacher.*` et `caltime.*` absents du réseau d'inférence sont exclus ; tous les paramètres du réseau sont chargés strictement, avec vérification des dimensions par PyTorch.
