# Analyse du fonctionnement de MatrixGold — dossier de preuves

> Méthode : **rétro-ingénierie documentaire (clean-room)**. Aucun binaire MatrixGold n'était
> disponible sur la machine d'analyse (`find / -iname "*matrix*gold*"` → aucun résultat) ; l'utilisateur
> a choisi l'analyse sur sources publiques. Aucune ligne de code ni aucun asset de MatrixGold
> n'a été copié dans OpenJewelleryBhy. Les observations ci-dessous viennent de documents officiels
> Stuller/Gemvision ; les inférences sont marquées **[INF]**, les inconnues **[?]**.

## 1. Sources (preuves primaires)

| # | Source | Nature | Empreinte |
|---|--------|--------|-----------|
| S1 | *MatrixGold Software Training Guide*, Gemvision/Stuller, 06.18.2019, 64 p. — `https://s3.amazonaws.com/gemvision.com/Manuals/MatrixGold+2019+Training+Guide.pdf` | Manuel officiel | SHA-256 `a19b0d4393f7710c99ca2e14b06b448753d90640987f439036e0f6828eedabd3` |
| S2 | Page produit `https://www.stuller.com/MatrixGold` | Officiel (prix, prérequis Rhino) | — |
| S3 | Communiqué *Gemvision releases MatrixGold 4.0* — `https://blog.stuller.com/press-release/gemvision-releases-matrixgold-4-0/` | Officiel | — |
| S4 | *Introducing MatrixGold 3.10* — `https://blog.stuller.com/introducing-matrixgold-3-10-the-efficiency-update/` | Officiel | — |
| S5 | *Gemvision introduces MatrixGold 3* — `https://instoremag.com/gemvision-introduces-matrixgold-3/` | Presse | — |
| S6 | Rhino blog, *MatrixGold 4.0 released* (2026-05-26) — `https://blog.rhino3d.com/2026/05/matrixgold-40-released.html` | Éditeur de Rhino | — |

Pour re-vérifier S1 :
```bash
curl -sSL -o guide.pdf "https://s3.amazonaws.com/gemvision.com/Manuals/MatrixGold+2019+Training+Guide.pdf"
sha256sum guide.pdf   # a19b0d43…eabd3
pdftotext -layout guide.pdf - | grep -n "Command: gv"
```

## 2. Architecture observée

### 2.1 Ce n'est pas un logiciel autonome : c'est un plugin Rhino
- S2 : *« MatrixGold requires a Rhino license to run »* ; vendu 5 995 $ avec Rhinoceros 8.
- S1 p.4 : *Import/Export … for all file types supported by Rhino* ; p.6 : Osnaps, Gumball, Record History, SmartTrack = vocabulaire et fonctions natives de Rhino.
- S6 / S4 : MatrixGold 3.10+ exige Rhino 8.
- **[INF]** Le moteur géométrique (NURBS, booléens, maillage, rendu des vues) est **celui de Rhino (openNURBS/RhinoCommon)** ; MatrixGold ajoute une couche « métier bijouterie » : ses propres commandes, une UI de type ruban, un historique paramétrique et une base de contenus.
- Conséquence : **Windows uniquement** (Rhino 7 pour Windows : x64 Intel/AMD, pas d'ARM, OpenGL 4.1).

### 2.2 Toutes les commandes métier portent le préfixe `gv` (= GemVision)
Le manuel S1 donne le nom interne exact de chaque commande. Liste complète extraite (`grep "Command: gv"`) :

| Catégorie (S1) | Commandes |
|---|---|
| Tools | `gvRingRail`, `gvOutsideRingRail`, `gvBypassRingRail`, `gvBypassOutsideRingRail`, `gvCathedralRingRail`, `gvProfilePlacer`, `gvSplitShank`, `gvTextObjects`, `gvTextOnCurve`, Profile Cap |
| Gems | `gvGem`, `gvGemsOnCurve`, `gvGemOnRingRail`, `gvGemOnSurface`, `gvGemGuides`, `gvGemOffsetCurve`, `gvHalo`, `gvOrientToGem` |
| Settings | `gvHead`, `gvBezel`, `gvUnderBezel`, `gvGalleryRail`, `gvProngPlacer`, `gvProngOnSurface`, `gvPearlCup`, `gvBail` |
| Cutters | `gvGemCutter`, `gvChannelCutter`, `gvScallopCutter`, `gvMicroProngCutter` (rangé dans Transform) |
| Transform | `gvPolarArray`, `gvDynamicMirror`, `gvShear`, `gvTaper`, `gvTwist`, `gvSmartFlow`, `gvAutoBase` |
| Solid / Surface / Curves | `gvBoolean`, `gvPipe`, `gvRope`, `gvObjectOnCurve`, `gvProfileSweep1`, `gvProfileSweep2`, `gvPullPush`, `gvOffset`, `gvFillet`, `gvBlendCurves`, `gvCurveFrom2Views`, `gvExtractIsoCurvefromSurface` |

### 2.3 Le cœur : « Dynamic Commands » = historique paramétrique en arbre (DAG)
- S1 p.10 : *« All commands in this subcategory exhibit Parametric History … changes to a design … automatically update all dynamic commands that are tied to it »* et *« think of it as a tree. There is a base object … any changes … will affect the other commands that are tied to the base object »*.
- S1 p.5 et p.9 : la fenêtre *Dynamic Commands* liste les commandes, la plus récente en haut ; survol = surbrillance cyan ; menu ⋮ : Style, Hide, Show, Rename, Delete ; **F5** rouvre l'édition d'un objet doté d'historique.
- Deux façons de régler les paramètres : (1) *Viewport Control Handles* dans la vue 3D, (2) panneau *Dynamic Command Window* (S1 p.10).
- **StyleSheets** : chaque commande peut sauvegarder ou charger un jeu de paramètres réutilisable (S1 p.9-10).
- *Save Small* enregistre la géométrie sans l'historique (S1 p.4). **[INF]** L'historique est donc sérialisé à côté de la géométrie dans le .3dm, sous forme de *user data* du plugin.
- **[INF] Modèle de données** : chaque commande est un nœud `{type, paramètres, références d'entrée}` ; modifier un nœud fait réévaluer ses descendants dans l'ordre de dépendance. Exemples de références vus dans S1 : *Ring Rail Input Box*, *Gem Input Box*, *Curve Input Box*, *Target Object Input Box*.

### 2.4 Conventions géométriques (observées)
- Unités en millimètres ; une maille de grille = 1 mm (S1 p.7).
- Repère : +X à droite, +Y vers l'arrière, +Z vers le haut (S1 p.7).
- Le ring rail (cercle de doigt) est centré à l'origine (S1 p.52 *« Sets a ring rail at the center »*). Les épaisseurs sont exprimées « à 12 h, 3 h/9 h, 6 h » (S1 p.16, 18, 40). **[INF]** Le rail est donc dans le plan XZ (vue Front) et le doigt suit Y.
- Gemme : L = longueur sur Y, W = largeur sur X, D = profondeur sur Z ; % = D/W (S1 p.30). Placement par rapport à la *table*, au *rondiste* (girdle) ou à la *colette* (culet) (S1 p.29).

### 2.5 Tailles de bague (S1 p.52)
Régions prises en charge : US, UK, CH (Chine), DE, EU, FI, FR, HK, JP, PT, TW, plus diamètre mm/pouce et circonférence mm/pouce, avec un curseur. **[INF]** En interne, la grandeur canonique est le diamètre (ou la circonférence) intérieur, converti selon une table par région.

### 2.6 Gemmes (S1 p.30)
- Tailles : *Cabochon*, *Diamond* (facettée), *Gem* (facettée, table plate), *Pearl*.
- Formes (Diamond) : Round, Princess, Oval, Cushion, Cushion Square, Pear, Marquise, Hexagon, Emerald, Custom Emerald, Emerald Square, Octagon, Radiant, Radiant Square, Coffin, Heart, Trillion Curved, Trillion Straight, Triangle, Calf, Half Moon, Baguette, Custom Baguette. La version 4.0 ajoute Lozenge, Hexagon et Trapezoid (S3).
- Saisie manuelle L/W/D avec **estimation du poids en carats**, ou liste de tailles préréglées (carat, L, W, D).
- MG3 : *Custom Gems Builder* (gemme à partir de n'importe quelle courbe fermée, réglage table/couronne/rondiste/pavillon/colette), *Proxy Gem*, *Gems Placer* anti-chevauchement (S5).

### 2.7 Constructeurs paramétriques principaux (paramètres officiels, S1)
- **Head** (p.33-34) : nombre et disposition des griffes, hauteur au-dessus du rondiste, hauteur totale, angle X, courbure (*bend*), profils des griffes, *Base Style* (normal / pointe / coupe à la colette), nombre de *gallery rails*. Quatre niveaux d'édition : tête entière, toutes les griffes, griffe individuelle, rail individuel.
- **Bezel** (p.13) : décalage au rondiste, position, angle, hauteur, épaisseur haute et basse, angle, hauteur et longueur du siège, dôme.
- **Halo** (p.31-32) : petites pierres (rondes ou princesse) autour d'une gemme centrale, espacement, canal, cadre métal, griffes (Prong / Bar / Auto), griffes d'angle.
- **Gems On Curve** (p.27) : position début et fin, nombre, taille de départ, espacement, méthode *Minimum / Minimum Scaled / Fixed*, dégressif (*taper*) au milieu et en fin, orientation *Origin / Y-axis / Down*, roulis, inclinaison, miroir.
- **Prong Placer** (p.50-51) : 6 dispositions (griffes partagées, en quinconce, 4 par pierre…), diamètre, dôme, *taper*, *nudge*, griffes d'extrémité.
- **Profile Placer + Sweep** (p.45-48) : profils placés à une position 0-100 % le long du rail, largeur et hauteur ; balayage fermé (*Auto Sweep*).
- **Gem Cutter** (p.24) : longueur haute, décalage X à la table, épaisseur au rondiste, longueur du siège, longueur basse. C'est l'outil soustrait par **Boolean** (p.15 : Union, Difference, Intersection).
- MG 4.0 : **Scoop** (*« reduce metal weight while maintaining structural integrity »*), **Tech Report**, Multi-Prong Edit, Award (signet) Ring Builder, Curve on Object (S3). MG 3.10 : *Azure (honeycomb) cutters* (S4).

### 2.8 Production et gestion
- Fenêtre **Metal Weights** : poids en grammes ou en pennyweights (dwt) (S1 p.5).
- **Gem Report** : informations sur les gemmes sélectionnées (S1 p.5).
- **Projects / Job Bags / Collections** : client, designer, statut (couleur), n° de job, temps CAD (S1 p.7-8), dans `Documents\MatrixGold\Projects`.
- **Render Studio** (rendu photoréaliste) ; **Clayoo** (modélisation organique SubD) (S2).

## 3. Ce qui reste inconnu sans binaire
- [?] Formats internes exacts (user data .3dm, StyleSheets, base de gemmes préréglées).
- [?] Formules de carat exactes utilisées par MG (on suppose les formules standard de l'industrie).
- [?] Tables de densité des alliages utilisées par *Metal Weights*.
- [?] Algorithmes internes (offset, balayage, booléens) : ce sont ceux de Rhino.

Pour lever ces inconnues, il faudrait analyser les assemblies .NET du plugin (`.rhp`) d'une installation sous licence, avec `inspect_managed_artifact` de REA. Cette analyse est possible si un binaire est fourni.

## 4. Correspondance MatrixGold → OpenJewelleryBhy

| MatrixGold | OpenJewelleryBhy | État |
|---|---|---|
| Rhino (NURBS) comme moteur | Noyau maison en Rust : maillages f64, balayages, booléens BSP + classification par rayons | réécrit (maillage, pas NURBS) |
| Dynamic Commands (arbre d'historique) | `Document` = DAG de nœuds, réévaluation incrémentale par hachage, niveaux en parallèle (rayon) | ✅ |
| StyleSheets | Presets JSON par type de nœud | ✅ |
| gvRingRail + 14 systèmes de tailles | `RingRail` : US, UK, EU/ISO, FR, DE, CH/IT/ES, JP/CN, diamètre/circonférence mm/in | ✅ |
| gvProfilePlacer + gvProfileSweep1 / OutsideRingRail | `Shank` : stations de profil (position, largeur, épaisseur, profil), symétrie, interpolation | ✅ |
| gvGem (+ Custom Gem Builder) | `Gem` : 20 formes, coupes Brilliant / Step / Cabochon / Pearl, proportions, carat (maillage + formules) | ✅ |
| gvGemOnRingRail | Placement `OnRail` | ✅ |
| gvGemsOnCurve (sur rail) | `GemsOnRail` : Fixed / Minimum, dégressif, miroir, éternité | ✅ |
| gvHead | `Head` : griffes, courbure, base, gallery rails | ✅ |
| gvBezel | `Bezel` | ✅ |
| gvHalo | `Halo` (pierres, cadre métal, griffes partagées) | ✅ |
| gvProngPlacer | `Prongs` (griffes partagées / 2 / 4 par pierre) | ✅ |
| gvGemCutter | `GemCutter` | ✅ |
| gvBoolean | `Boolean` (Difference / Union / Intersection) | ✅ |
| Scoop (MG 4.0) | `Scoop` (allègement intérieur de l'anneau) | ✅ |
| gvPolarArray / gvDynamicMirror | `PolarArray` / `Mirror` | ✅ |
| Metal Weights (g / dwt) | Rapport technique : volume par union de rayons, 14 alliages | ✅ |
| Gem Report / Tech Report | `TechReport` (Markdown / JSON) | ✅ |
| Export Rhino | STL binaire, OBJ, 3MF | ✅ |
| gvTextObjects / gvTextOnCurve | `Text3d` / `TextOnRail` (gravé ou en relief, intérieur ou extérieur) | ✅ |
| Award Ring Builder (MG 4.0) | `Signet` (plateau, dôme, bordure, monogramme) | ✅ |
| gvSplitShank / gvBypassRingRail | `SplitShank` (Split / Bypass) | ✅ |
| gvChannelCutter / gvScallopCutter | `ChannelCutter` / `ScallopCutter` | ✅ |
| Azure Cutters (MG 3.10) | `Azure` (nid d'abeille) | ✅ |
| gvPearlCup / gvBail | `PearlCup` / `Bail` | ✅ |
| gvRope / gvTwist / gvTaper | `Rope` / `Deform` (Twist, Taper) | ✅ |
| gvGemOnSurface (+ Gems Placer MG3) | `GemsOnSurface` (clic + remplissage automatique) | ✅ |
| Viewport Control Handles | poignées par builder (`ops::handles`) | ✅ |
| Render Studio | Render Studio (lancer de rayons, dispersion) | ✅ |
| Import (formats Rhino) | Import STL / OBJ | ✅ (pas de .3dm) |
| Clayoo (SubD), NURBS, .3dm | — | non prévu (noyau maillé) |
