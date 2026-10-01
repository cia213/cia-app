# Paquet Windows v1.2.4

Le runtime publié est une copie propre de Smoothie `Nightly_2025.11.30_14-36`, commit `3da73de9df0c991801c4f8474b320445fb25a208`. L'archive portable a l'empreinte SHA-256 `a9e49e8638c0c1db90a86d4ddb4e5eae0681e773c5bd9177caab8eb5586c4a07`. Les binaires et scripts VapourSynth communs à l'installation locale testée sont identiques octet par octet.

`prepare-release.py` conserve les métadonnées `#{type: ...}` et le séparateur `:` des recettes Smoothie. La reconstruction par un sérialiseur INI standard supprimait ces métadonnées et provoquait les deux crashes constatés pendant la préparation. Ces paquets de test n'ont pas été publiés.

Les réglages livrés n'activent ni LUT, ni correction couleur, ni changement de vitesse. La sortie par défaut conserve le ratio et utilise x264/CRF18/medium, yuv420p et AAC. Les caches, médias, modèles, chemins personnels et `last_args.txt` sont exclus ; les notices et le fichier Python `distutils-precedence.pth` sont conservés.

Le paquet comprend FFmpeg/FFprobe `8.1.1-essentials_build-www.gyan.dev`, VapourSynth R70 et l'installateur officiel Python 3.11.9 pour le bootstrap facultatif. `RELEASE-PAYLOAD.json` décrit les tailles et SHA-256 de chaque fichier runtime, l'origine de Smoothie, la configuration FFmpeg et l'empreinte de l'installateur Python. Les sources amont Smoothie, VapourSynth et FFmpeg sont jointes à la release.

## Reproduire la préparation

Déposer l'archive portable officielle dans `src-tauri/target/release-kit-v1.2.4/smoothie-rs-nightly.zip`, ainsi que les outils FFmpeg et l'installateur Python aux emplacements décrits dans `RUNTIME-RELEASE-NOTES.md`. Le script refuse une archive ou un installateur Python inattendu.

```powershell
python scripts/prepare-release.py
$env:CIA_TEST_RUNTIME_ROOT=(Resolve-Path src-tauri/target/release-kit-v1.2.4/runtime).Path
cargo test --locked --manifest-path src-tauri/Cargo.toml bundled_smoothie_pipeline -- --ignored --nocapture
# Retirer les caches et sidecars générés par le moteur avant de construire.
python scripts/prepare-release.py
$env:TAURI_SIGNING_PRIVATE_KEY = '<chemin de la clé existante>'
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = '<mot de passe>'
node node_modules/@tauri-apps/cli/tauri.js build --config src-tauri/target/release-kit-v1.2.4/build.json
```

La configuration de release mappe le runtime propre vers `resources/runtime`, avec les scripts RIFE, le bootstrap Python, les notices et le manifeste de provenance. Le chemin générique `resources/runtime` de développement ne sert pas de source au paquet public.

## Validation

Les contrôles locaux comptent 19 tests frontend, 11 tests Python et 14 tests Rust, avec fmt/clippy sans erreur. Le test Smoothie réel couvre quatre variantes : neutre, correction couleur, correction + LUT et source sans profil renseigné. Il vérifie 320×200, SAR 4:3, une source à cadence fractionnaire, 30 images à 30 FPS, l'audio, les tags BT709, des pixels différents selon les réglages, huit aperçus, l'absence d'écrasement et le nettoyage.

La validation du pipeline utilise un runtime propre isolé ; elle ne simule pas l'installation NSIS et toute la navigation graphique sur un nouveau compte Windows. Les tests RIFE et mesures de performance du patch sont détaillés dans `PATCH-QUALITE-PERFORMANCES.md`.
