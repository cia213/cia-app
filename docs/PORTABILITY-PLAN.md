# CIA RENDER — plan de portabilité et de publication

> Statut : **plan uniquement**. Ce document décrit la migration vers une
> installation reproductible et une publication GitHub. Il ne modifie ni le
> runtime actuel, ni les moteurs, ni l'interface. La page ENHANCE (D2) reste
> bloquée tant que les deux sorties Topaz D1 valides n'existent pas.

## 1. Décision de distribution

La première distribution portable ne doit pas encapsuler aveuglément la
machine de développement actuelle. Le produit sera séparé en trois couches :

1. **Installateur CIA RENDER** : application Tauri/Svelte/Rust, configuration
   vierge, notices de licence et documentation de première installation.
2. **Runtimes open-source optionnels, versionnés et vérifiés** : RIFE,
   Smoothie et, seulement après audit, FFmpeg. Ils peuvent être détectés sur la
   machine, installés par un mécanisme de téléchargement contrôlé, ou fournis
   comme archives optionnelles distinctes.
3. **Topaz Video AI** : dépendance propriétaire installée localement par son
   détenteur de licence. Elle est détectée, jamais embarquée, téléchargée,
   copiée, ou redistribuée par CIA RENDER.

Cette séparation évite un installateur de plusieurs gigaoctets, ne mélange pas
les licences tierces avec le code de l'application, et rend explicite ce que
l'utilisateur doit installer ou autoriser.

## 2. Inventaire constaté (2026-08-06)

Les tailles suivantes sont des mesures de la machine de développement. Elles
servent à décider du format de distribution, pas à promettre la taille d'une
release future.

| Élément observé | Emplacement actuel | Taille observée | État de licence / provenance | Décision de distribution |
| --- | --- | ---: | --- | --- |
| Application Tauri/Svelte | `C:\Users\cia\time-remap-ui` | projet source, hors builds ignorés | code CIA RENDER + dépendances listées ci-dessous | **Inclure** l'exécutable compilé et les assets construits ; publier le source après nettoyage d'identité. |
| Runtime Python RIFE | `C:\Users\cia\time-remap-app\venv` | 5 245 174 005 octets (≈ 4,89 Gio) | Python 3.11.9, torch 2.5.1+cu121, CUDA 12.1 ; licences transitives à inventorier | **Ne pas inclure** tel quel. Créer un runtime verrouillé et optionnel, ou une installation assistée vérifiée. |
| Script d'orchestration RIFE | `C:\Users\cia\time-remap-app\time_remap.py` | 6 823 octets | provenance à confirmer avant publication | **À auditer puis inclure** dans le dépôt/app si CIA RENDER en possède les droits ; sinon réécrire/documenter son origine. |
| Practical-RIFE | `C:\Users\cia\time-remap-app\Practical-RIFE` | code + modèle séparés | licence MIT, copyright hzwer 2021 | **Inclure le code/notices** si sa version est figée ; le modèle suit une piste séparée. |
| Poids RIFE `flownet.pkl` | `...\Practical-RIFE\train_log\flownet.pkl` | 24 636 301 octets (≈ 23,5 Mio) | origine/version présentes mais droits de redistribution du poids à vérifier | **Ne pas versionner ni intégrer** avant vérification ; téléchargement optionnel avec hash si autorisé. |
| Smoothie local | `C:\Users\cia\Music\smoothie1` | 194 173 673 octets (≈ 185 Mio) | bundle contenant smoothie-rs, FFmpeg/VapourSynth et composants Python ; licence globale non établie | **Externe / archive optionnelle** après audit complet, pas dans l'installateur V1. |
| VapourSynth au sein de Smoothie | `...\smoothie1\bin\Lib\site-packages\VapourSynth-70.dist-info` | inclus dans la ligne précédente | fichier `COPYING.LESSER` présent | **Audit obligatoire** avant toute redistribution ; notices à préserver. |
| LUT personnel | `C:\Users\cia\Music\colorcia.cube` | 1 232 166 octets | création/provenance personnelle non documentée | **Ne jamais inclure par défaut** ; import explicite par l'utilisateur ou chemin configuré. |
| FFmpeg découvert | `C:\Users\cia\scoop\shims\ffmpeg.exe` puis `C:\ffmpeg\bin\ffmpeg.exe` | externe | build 8.1.1 essentials avec `--enable-gpl`, `--enable-version3`, notamment x264/x265 | **Ne pas redistribuer ce build**. Utiliser un binaire externe configuré, ou sélectionner plus tard un build dont la licence/distribution est approuvée. |
| FFprobe | mêmes chemins via PATH | externe | même famille de distribution que FFmpeg | **Même règle que FFmpeg**. Le chemin doit venir de la configuration, jamais du PATH seul. |
| Topaz Video AI | `C:\Program Files\Topaz Labs LLC\Topaz Video AI` | 1 357 934 432 octets (≈ 1,26 Gio) | propriétaire, installation/licence locale | **Externe requis, interdit de bundle**. |
| Modèles Topaz | `C:\ProgramData\Topaz Labs LLC\Topaz Video AI\models` | 308 996 653 octets (≈ 295 Mio) | propriétaire | **Interdits de dépôt, release et installateur**. |
| Font IBM Plex Sans / Mono | `@fontsource-variable/ibm-plex-sans`, `@fontsource/ibm-plex-mono` | frontend | OFL-1.1 | **Inclure avec les notices OFL**. |
| Svelte / Vite | dépendances Node | frontend | MIT constaté | **Inclure seulement dans le build**, consigner les notices. |
| Tauri API/CLI | dépendances Node/Rust | application | Apache-2.0 OR MIT constaté | **Inclure seulement dans le build**, consigner les notices. |

### Contraintes relevées dans le code actuel

- Rust contient quatre chemins propres à la machine : Python, `time_remap.py`,
  le dossier Smoothie et `smoothie-rs.exe` (`src-tauri/src/lib.rs`).
- `ffprobe` est invoqué par son simple nom dans Rust. `time_remap.py` appelle
  aussi `ffprobe`/`ffmpeg` via le shell : le rendu dépend donc actuellement du
  `PATH` de l'utilisateur et d'une résolution non déterministe.
- Smoothie référence le LUT absolu
  `C:/Users/cia/Music/colorcia.cube` dans `recipe.ini`.
- Les préférences UI vivent dans `localStorage` sous `rife_auto_render`,
  `rife_settings` et `smoothie_settings`. Elles ne constituent pas une
  configuration installable ni inspectable par Rust.
- Le résultat RIFE est aujourd'hui créé à côté de l'entrée sous la forme
  `<stem>-<factor>x-RIFE-4.26-<fps>fps.mp4`, puis Smoothie produit
  `<stem>_smoothie.mp4`. Cette convention validée doit être préservée par
  défaut.

## 3. Cible : un unique fichier de configuration utilisateur

### Emplacement et règles

La cible est `%APPDATA%\CIA RENDER\config.json` sur Windows. Le fichier est
créé au premier lancement à partir d'un modèle inclus dans l'application ; il
n'est jamais écrit dans le répertoire d'installation, le dépôt, une release ou
un journal de support sans consentement.

Rust le lit et le valide au lancement. Le frontend interroge Rust pour obtenir
un état de capacités, au lieu de reconstruire des chemins ou de dépendre de
`localStorage`. L'enregistrement passe aussi par Rust afin d'écrire de manière
atomique et de préserver le numéro de schéma.

Proposition de structure (schéma à versionner) :

```json
{
  "schema_version": 1,
  "rife": {
    "python_executable": "C:/CIA RENDER/runtimes/rife/venv/Scripts/python.exe",
    "script": "C:/CIA RENDER/runtimes/rife/time_remap.py",
    "model_file": "C:/CIA RENDER/runtimes/rife/Practical-RIFE/train_log/flownet.pkl"
  },
  "smoothie": {
    "root": "C:/CIA RENDER/runtimes/smoothie",
    "executable": "C:/CIA RENDER/runtimes/smoothie/bin/smoothie-rs.exe",
    "recipe": "C:/CIA RENDER/runtimes/smoothie/recipe.ini",
    "lut_file": null
  },
  "media_tools": {
    "ffmpeg": null,
    "ffprobe": null
  },
  "topaz": {
    "enabled": false,
    "ffmpeg": null
  },
  "output": {
    "policy": "alongside_input"
  },
  "ui": {
    "auto_render_smoothie": false,
    "rife_settings": {},
    "smoothie_settings": {}
  }
}
```

`null` signifie « non configuré » et non « rechercher arbitrairement dans le
PATH ». Les chemins peuvent être absolus au départ ; une évolution ultérieure
pourra accepter une racine de runtime avec chemins relatifs afin de déplacer
un pack optionnel sans le reconfigurer.

### Validation au démarrage

La validation doit être non destructive et explicite :

1. vérifier le `schema_version`, canonicaliser les chemins existants et tester
   fichier/dossier attendu ;
2. exécuter les sondes minimales non coûteuses nécessaires (`--version` ou
   équivalent), jamais un rendu ;
3. produire des capacités indépendantes : `rife_ready`, `smoothie_ready`,
   `media_tools_ready`, `topaz_detected` ;
4. afficher une carte « Runtime setup required » avec bouton de sélection du
   chemin manquant, plutôt qu'un crash ou un fallback PATH silencieux ;
5. empêcher seulement l'action dépendante du composant absent. RIFE et
   Smoothie doivent rester utilisables sans Topaz ; Topaz absent correspond à
   l'état propre déjà prévu pour ENHANCE.

Les sorties restent **à côté du média d'entrée** tant que
`output.policy = "alongside_input"`. Une option future `chosen_folder` devra
demander un dossier et conserver les noms de sorties déjà validés. Aucun
composant ne doit écrire dans le dossier d'installation.

### Migration des préférences existantes

Lors du premier lancement de la version qui introduira la configuration :

1. lire une seule fois les trois clés `localStorage` actuelles ;
2. reporter leurs valeurs valides dans `ui` ;
3. écrire `config.json` de façon atomique ;
4. conserver les clés existantes une version de transition, puis les supprimer
   seulement après confirmation de migration réussie.

Les chemins locaux actuels ne doivent **pas** être copiés automatiquement dans
une release : ils peuvent être proposés à l'utilisateur de développement, puis
validés comme tout autre chemin.

## 4. Stratégie des runtimes RIFE, CUDA et modèles

Le `venv` actuel représente environ 4,89 Gio et contient notamment
`torch==2.5.1+cu121` et `torchvision==0.20.1+cu121`, avec CUDA utilisable sur
la machine de référence. Le réutiliser tel quel dans un installateur rendrait
la release énorme, peu reproductible et difficile à auditer.

Ordre de préférence proposé :

1. **V1 de publication : runtime RIFE externe/optionnel.** L'application
   détecte un runtime explicitement sélectionné et fournit une procédure de
   mise en place versionnée. C'est le plus sûr tant que les licences et hashes
   ne sont pas figés.
2. **Pack GPU optionnel versionné.** Après audit, construire depuis un lock
   file un runtime minimal (Python, Torch CUDA, dépendances RIFE, script et
   modèle autorisé), publié comme artefact séparé avec SHA-256. Il n'est ni un
   asset Git normal ni un téléchargement opaque.
3. **Téléchargement de premier lancement.** Seulement après avoir fixé des
   URLs officielles, versions, SHA-256, taille annoncée, licences et gestion
   des échecs/reprises. L'utilisateur doit confirmer avant le téléchargement.
4. **Bundling intégral.** À rejeter pour la V1 ; ne le reconsidérer qu'avec
   une chaîne de build reproductible, un audit de licences complet et une
   raison produit forte.

Le dépôt doit contenir un fichier de verrouillage Python dédié au runtime,
avec hashes, mais aucun `venv`, cache pip, poids `.pkl/.pt/.onnx` ou binaire
CUDA. Avant de choisir un pack GPU, documenter les prérequis NVIDIA/driver et
le comportement exact en absence de CUDA. Ne pas promettre de fallback CPU
tant qu'il n'est pas mesuré.

## 5. Smoothie, FFmpeg et LUT

### Smoothie

Smoothie local fait environ 185 Mio et sa recette locale combine plusieurs
outils. La présence de `VapourSynth-70.dist-info/COPYING.LESSER` et de nombreux
composants Python impose un inventaire de licences avant redistribution. La
V1 doit donc le traiter comme runtime externe détectable ; une archive
optionnelle ne sera envisagée qu'après :

- identification de la release source de `smoothie-rs` ;
- lecture des licences de tous les binaires/plugins fournis ;
- conservation des notices requises ;
- version et SHA-256 figés ;
- test sur une machine vierge.

Le fichier `recipe.ini` est une configuration utilisateur/runtime, pas un
asset implicite du binaire CIA RENDER. Le futur écran de setup doit autoriser
son emplacement ou permettre de générer une copie locale contrôlée.

### FFmpeg / FFprobe

Les appels actuels au PATH devront devenir des chemins explicitement validés
dans `media_tools`. Le build local observé est GPL/version3, donc il ne doit
pas être repris dans une release sans stratégie de conformité. Les deux voies
acceptables sont :

- demander à l'utilisateur de pointer vers son installation FFmpeg ; ou
- sélectionner un build redistribuable après revue de sa configuration, de sa
  licence et des obligations de notices/source.

`time_remap.py` devra ensuite remplacer ses commandes `shell=True` par des
arguments discrets et les chemins fournis par la configuration. C'est une
phase d'implémentation future, non réalisée par ce plan.

### LUT

Le LUT `colorcia.cube` est propre à l'utilisateur et non attribué dans le
projet. Le défaut portable doit être `lut_file: null`, avec un réglage qui
propose une importation locale. Il ne doit apparaître ni dans Git, ni dans les
assets d'installation, ni dans les releases tant que son droit de distribution
n'est pas consigné.

## 6. Topaz : frontière non négociable

Topaz Video AI et ses modèles restent strictement externes. La fonction D0 ne
fait que détecter un runtime local compatible ; D1 est documentée dans
`docs/ENHANCE-STATUS.md` et n'a pas obtenu les deux sorties non vides exigées.
Par conséquent :

- aucune page ENHANCE (D2) n'est implémentée dans cette phase ;
- aucun modèle, exécutable, installateur, clé, cache ou log Topaz n'entre dans
  le dépôt, une release ou l'installateur ;
- la configuration ne mémorise qu'un chemin optionnel choisi par l'utilisateur
  et le valide en lecture seule ;
- si Topaz est absent/incompatible, le produit affiche son état propre sans
  modifier RIFE/Smoothie ni proposer de faux fallback.

## 7. Identité CIA RENDER avant toute release

L'interface affiche CIA RENDER, mais l'identité de build provient encore du
projet Dollrunner/TimeRemap :

| Surface | Valeur actuelle | Cible de migration |
| --- | --- | --- |
| `package.json` | `dollrunner` | `cia-render` |
| Rust package/lib et `main.rs` | `dollrunner` / `dollrunner_lib` | `cia_render` / `cia_render_lib` |
| Tauri `productName` et titre de fenêtre | `TimeRemap` | `CIA RENDER` |
| Tauri identifier | `com.timeremap.app` | `com.ciarender.app` après vérification de propriété du namespace |
| `index.html` | titre Dollrunner + ressources Google/`model-viewer` historiques | titre CIA RENDER, retirer les ressources inutiles après audit |
| `README.md` | documentation Dollrunner/C# | documentation CIA RENDER réellement exécutable |

Cette migration doit être un commit isolé, avec vérification que le changement
d'identifiant ne perd ni configuration ni accès aux données. Le premier
lancement de CIA RENDER doit offrir une migration explicite, jamais supposer
que l'ancien espace applicatif TimeRemap est sûr à effacer.

## 8. Publication GitHub et releases

### Ce qui peut aller dans le dépôt source

- code Svelte, Rust et scripts dont les droits sont établis ;
- fichiers de build et manifests verrouillés (`package-lock`, Cargo lock,
  lockfile Python dédié avec hashes) ;
- icônes, documentation, tests sans médias propriétaires ;
- `THIRD_PARTY_NOTICES.md` et copies/attributions requises ;
- modèle de configuration sans chemins personnels.

### Ce qui ne doit pas y aller

- `node_modules`, `dist`, `src-tauri/target`, logs, vidéos de test et sorties ;
- `venv`, caches, poids RIFE et tout modèle `.pkl/.pth/.pt/.onnx` avant
  autorisation explicite ;
- `smoothie-rs.exe`, plugins et bundle Smoothie avant audit ;
- LUT personnel ;
- Topaz Video AI, ses modèles, ses exécutables, licences, journaux ou caches ;
- chemins `C:\Users\cia\...`, tokens, réglages ou médias personnels.

Le `.gitignore` actuel couvre déjà les builds, vidéos, environnements Python,
modèles et anciens binaires ; il doit rester en place et être contrôlé par
`git status --ignored` avant chaque ajout massif.

### Git LFS et assets

Git LFS n'est pas une solution à une licence inconnue. Il ne doit être utilisé
que pour un asset dont la redistribution est autorisée, qui est indispensable
au source et dont la taille justifie LFS. Pour les modèles et runtimes, la voie
préférée est un artefact de release séparé avec version, SHA-256, licence et
source officielle. Topaz ne va ni dans Git LFS ni dans une release.

### Chaîne de release proposée

1. CI : lint/build/test de l'UI et du Rust sans dépendre des répertoires
   personnels ni de Topaz ; utiliser des doublures de test pour les runtimes.
2. Build Windows signé lorsque la signature et l'identité sont prêtes.
3. Génération des notices de licences et vérification de leur inclusion.
4. Publication de l'installateur CIA RENDER léger, accompagné de checksums.
5. Publication séparée et facultative d'un runtime open-source seulement après
   audit juridique/technique, ou guide de configuration manuelle autrement.
6. Test d'installation sur une VM Windows vierge : installation, premier
   lancement, composants manquants, configuration RIFE/Smoothie, rendu court,
   désinstallation et conservation/suppression documentée des données.

## 9. Découpage d'implémentation ultérieur

L'ordre protège les acquis RIFE/Smoothie et évite de mêler une refonte de
distribution au travail Topaz :

1. terminer/archiver D1 Topaz sans D2 tant que les critères ne sont pas verts ;
2. commit isolé d'identité CIA RENDER et remplacement du README historique ;
3. introduire `config.json`, lecture/validation Rust et écran de setup sans
   changer les commandes de rendu ;
4. faire migrer les préférences depuis `localStorage` ;
5. enlever les chemins absolus et le PATH implicite, avec tests de chemins et
   d'erreurs ;
6. figer/auditer les runtimes et leurs licences ;
7. rendre la CI indépendante des runtimes locaux, puis produire l'installateur
   et tester une machine vierge ;
8. seulement après validation D1 par les sorties et les yeux utilisateur,
   reprendre D2 dans un changement séparé.

## 10. Critères d'acceptation avant une première release

- aucun chemin utilisateur n'est compilé dans l'application ;
- un lancement sans runtime ouvre un setup lisible, sans console visible et
  sans crash ;
- un lancement avec runtimes configurés vérifie chaque capacité avant rendu ;
- les sorties validées gardent leur nom et leur dossier par défaut ;
- aucune donnée personnelle, vidéo, modèle non autorisé ou binaire Topaz n'est
  suivie par Git ;
- les licences/notices sont complètes pour chaque composant redistribué ;
- l'identité affichée, les métadonnées du paquet et l'identifiant Tauri sont
  tous CIA RENDER ;
- un test d'installation sur Windows vierge est passé avec les dépendances
  réellement annoncées ;
- D2 n'est entrepris que lorsque D1 dispose de deux sorties non vides,
  techniquement inspectées et soumises à la validation visuelle utilisateur.
