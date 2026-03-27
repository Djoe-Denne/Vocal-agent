Oui. Voici un **plan d’implémentation complet, directement exploitable par Codex**, basé sur la structure réelle de `tempo-service/infra` et `tempo-service/domain`.

Le point d’entrée actuel est `TempoMatchAdapter`, qui construit un `TempoPipelineContext`, exécute `TempoPipelineEngine`, puis renvoie `context.samples` .
La pipeline par défaut est assemblée dans `builder.rs` avec les stages `audio_prepare -> segment_plan -> segment_extraction -> frame_analysis -> f0_estimation -> voiced_zone -> pitch_mark -> stretch_region -> grain_extraction -> synthesis_grid -> synthesis_mapping -> overlap_add -> unvoiced_handling -> recombination -> debug_export` .
Le contexte partagé contient aujourd’hui les buffers, timings, plans de segments, pitch data, régions voisées, pitch marks, grains, grilles de synthèse et plans de placement .

# Ce que Codex doit corriger en priorité

Le plan ci-dessous corrige les problèmes structurels que le code actuel a encore :

1. `segment_plan.rs` travaille mot par mot, pairé **par index**, et ne crée **aucun segment de gap/pause** ; il ne peut donc pas réaligner correctement une timeline qui diffère surtout par les pauses .
2. `synthesis_grid.rs` utilise pour `VoicedPsola` un `synth_period = mean_period / local_alpha`, ce qui **augmente mécaniquement la fréquence quand on étire** et explique les artefacts aigus .
3. `segment_extraction.rs` extrait avec marges, `overlap_add.rs` remplace ensuite `local_samples` par un buffer de longueur `target_duration_samples`, puis `recombination.rs` **re-trime encore les marges** ; cela crée une incohérence de repère et explique le décalage systématique de durée   .
4. `pitch_mark.rs` snappe les marques sur le **pic d’amplitude absolue**, ce qui favorise les alternances demi-période / polarité opposée et produit du jitter .
5. `unvoiced_handling.rs` ne stretch pas réellement les pauses : il atténue simplement les samples existants, donc il ne peut pas reconstruire le timing global .

---

# Plan à donner à Codex

## 1. Stabiliser le modèle de données avant toute correction DSP

### Fichiers à modifier

* `tempo-service/domain/src/entity.rs`
* `tempo-service/domain/src/pipeline.rs`

### Objectif

Rendre explicites :

* le **type de segment** : mot vs gap/pause
* le **repère temporel** utilisé : buffer d’analyse avec marges vs buffer utile rendu
* la **métadonnée de mapping temporel**

### Changements demandés

### 1.1 Ajouter un type de segment

Dans `entity.rs`, introduire :

* `SegmentKind`

  * `Word`
  * `Gap`

Puis étendre `SegmentPlan` avec :

* `kind: SegmentKind`
* `tts_start_ms: u64`
* `tts_end_ms: u64`
* `original_start_ms: u64`
* `original_end_ms: u64`
* `label: Option<String>`

But :

* distinguer explicitement les vrais mots des pauses inter-mots
* garder l’info utile pour le debug et le narrative export

### 1.2 Séparer clairement analyse et rendu

Refactorer `SegmentAudio` pour éviter l’ambiguïté actuelle entre :

* buffer extrait avec marges
* buffer utile réellement rendu

Je recommande de faire évoluer `SegmentAudio` vers quelque chose comme :

* `analysis_samples: Vec<f32>` : buffer extrait avec marges
* `rendered_samples: Vec<f32>` : buffer final utile sans marges
* `global_start_sample`
* `global_end_sample`
* `extract_start_sample`
* `extract_end_sample`
* `useful_start_in_analysis`
* `useful_end_in_analysis`
* `target_duration_samples`
* `alpha`
* `kind`

Le but est que :

* les stages d’analyse lisent `analysis_samples`
* les stages de synthèse écrivent `rendered_samples`
* `recombination` ne fasse plus de trimming implicite de marges sur un buffer déjà rendu

### 1.3 Ajouter un helper de coordonnées

Créer dans le domaine ou dans `convert.rs` des helpers explicites :

* `analysis_to_useful_local(sample_idx, useful_start_in_analysis) -> Option<usize>`
* `useful_to_analysis_local(sample_idx, useful_start_in_analysis) -> usize`

Codex doit supprimer toute hypothèse implicite sur les marges.

---

## 2. Corriger la planification temporelle globale

### Fichier principal

* `tempo-service/infra/src/stages/segment_plan.rs`

### Problème actuel

Le code zippe simplement `tts_timings` et `original_timings` par index, puis crée un segment par mot, avec `target_duration_samples` dérivé de la durée du mot original uniquement .
Cela ignore complètement les pauses de l’original.

### Changements demandés

### 2.1 Remplacer le plan “word-only” par une timeline alternée mot/gap

À partir des timings TTS et originaux :

* pour chaque paire de mots indexée `i`

  * créer éventuellement un segment `Gap` entre la fin du mot `i-1` et le début du mot `i`
  * créer le segment `Word` pour le mot `i`
* ajouter aussi :

  * gap initial éventuel
  * gap final éventuel

### 2.2 Règle de durée cible

Pour un `Word` :

* `target_duration = original_word_duration`

Pour un `Gap` :

* `target_duration = original_gap_duration`

### 2.3 Règle de durée source

Pour un `Word` :

* `source_duration = tts_word_duration`

Pour un `Gap` :

* `source_duration = tts_gap_duration`

### 2.4 Ne plus filtrer agressivement les gaps courts

Le seuil `MIN_SEGMENT_SAMPLES` peut rester pour les mots très courts, mais les gaps doivent être tolérés différemment.
Codex doit :

* garder les gaps significatifs
* fusionner les gaps microscopiques adjacents si nécessaire

### 2.5 Ajouter des tests

Ajouter des tests qui valident :

* création de segments alternés `Gap` / `Word`
* présence des pauses longues de l’original
* durée totale planifiée alignée avec la timeline originale

---

## 3. Corriger l’extraction et le repère temporel des segments

### Fichier principal

* `tempo-service/infra/src/stages/segment_extraction.rs`

### Problème actuel

Le stage extrait `local_samples` avec marges et transmet ce buffer brut à toute la suite, mais la suite ne garde pas un contrat clair sur ce que représentent ces indices .

### Changements demandés

### 3.1 Extraire deux vues logiques

Pour chaque segment :

* `analysis_samples = original[extract_start..extract_end]`
* `useful interval in analysis = [margin_left .. analysis_len - margin_right]`

### 3.2 Ne plus écrire les résultats de synthèse dans le même champ que le buffer d’analyse

Les stages aval doivent écrire dans `rendered_samples`, jamais dans `analysis_samples`.

### 3.3 Pour les segments `Gap`

Ne pas passer les gaps dans le pipeline PSOLA complet.
Ils doivent être rendus via une stratégie dédiée, plus simple :

* zéro-fill ou noise floor très bas si gap silencieux
* ou simple resampling si le TTS contient déjà du souffle utile

---

## 4. Fiabiliser la détection voisée et le F0

### Fichiers principaux

* `tempo-service/infra/src/stages/frame_analysis.rs`
* `tempo-service/infra/src/stages/f0_estimation.rs`
* `tempo-service/infra/src/stages/voiced_zone.rs`

### Objectif

Réduire les erreurs d’octave et les faux positifs sur fricatives.

### Changements demandés

### 4.1 Durcir la décision voisé/non voisé

Dans `frame_analysis.rs`, la décision actuelle est surtout énergie + ZCR .
Codex doit intégrer aussi :

* `periodicity` minimale pour considérer une frame comme vraiment voisée
* un seuil configurable

Suggestion :

* `is_voiced = energy > silence_threshold && zcr < zcr_threshold && periodicity > periodicity_threshold`

### 4.2 Réduire l’intervalle F0 plausible

Dans `f0_estimation.rs`, la plage 50–500 Hz est trop large pour ce pipeline parole TTS et favorise les erreurs d’octave hautes .

Codex doit :

* rendre les bornes F0 configurables
* utiliser par défaut quelque chose comme :

  * 60–350 Hz
    ou
  * 70–320 Hz si la voix cible est connue

### 4.3 Ajouter une correction d’octave par continuité

Quand une frame voisée a un F0 qui saute brutalement :

* si `f0_current > 1.8 * f0_prev`
* tester `f0_current / 2`
* choisir la version la plus proche de la continuité locale

Même logique pour un saut vers le bas :

* tester `f0_current * 2`

### 4.4 Renforcer `VoicedZoneStage`

Dans `voiced_zone.rs`, n’autoriser `VoicedPsola` ensuite que si :

* la stabilité est suffisante
* la zone contient assez de frames voisées
* la variance F0 n’est pas extrême

---

## 5. Corriger les pitch marks

### Fichier principal

* `tempo-service/infra/src/stages/pitch_mark.rs`

### Problème actuel

Le snapping au pic d’amplitude absolue favorise les alternances de polarité et le jitter demi-période .

### Changements demandés

### 5.1 Snap sur pics de polarité cohérente

Codex doit remplacer `snap_to_peak` par une recherche plus robuste :

* choisir une polarité de référence au seed
* ensuite chercher les pics de **même polarité** autant que possible
* fallback sur amplitude absolue seulement si aucun candidat valide

### 5.2 Contraindre les espacements inter-marks

Lors de la propagation gauche/droite :

* accepter uniquement un prochain mark dans `[0.8 * T0, 1.2 * T0]`
* rejeter tout candidat trop proche ou trop loin
* si aucun candidat acceptable, utiliser la position théorique ou arrêter la propagation

### 5.3 Utiliser la période locale du frame le plus proche

Le code le fait déjà partiellement via `local_period_at` , mais Codex doit :

* en faire la règle principale
* ne plus trop dépendre du `mean_period` de région

### 5.4 Ajouter des diagnostics

Exporter :

* histogramme des écarts inter-marks
* ratio `delta_mark / local_period`
* pourcentage de marks hors plage acceptable

---

## 6. Corriger la grille de synthèse : c’est le cœur du bug de pitch

### Fichier principal

* `tempo-service/infra/src/stages/synthesis_grid.rs`

### Problème actuel

Le code utilise :

* `synth_period = mean_period / local_alpha`
  pour `VoicedPsola` .

C’est la cause principale des sons aigus sur les stretches.

### Règle correcte à implémenter

Pour garder la **hauteur constante** :

* l’espacement des **synthesis marks** doit rester proche du **pitch period source**
* c’est le **mapping des marks d’analyse vers les marks de synthèse** qui doit créer duplication/saut
* pas l’inverse

### Changements demandés

### 6.1 Pour `VoicedPsola`, ne plus diviser la période par `alpha`

Codex doit remplacer :

* `synth_period = mean_period / alpha`

par une logique du type :

* `synth_period ~= local_period_of_mapped_analysis_mark`

En première approximation :

* utiliser `mean_period` inchangé
* puis raffiner avec la période locale du mark mappé

### 6.2 Construire la grille comme une timeline de sortie à pitch constant

Algorithme recommandé pour une région `VoicedPsola` :

* définir `region_output_len = region_input_len * local_alpha`
* initialiser `out_pos = output_cursor`
* tant que `out_pos < region_output_end`

  * calculer la position d’entrée correspondante :

    * `input_pos = region.start + (out_pos - output_cursor) / local_alpha`
  * choisir le mark d’analyse le plus proche de `input_pos`
  * récupérer sa période locale `T0`
  * pousser `SynthesisMark { out_pos, mapped_analysis_mark_index }`
  * avancer `out_pos += T0`

Cette logique :

* garde la densité temporelle de sortie compatible avec le pitch
* crée automatiquement répétition ou skipping via le mapping d’entrée

### 6.3 Pour `KeepNearConstant`

Ne pas utiliser PSOLA.
Construire une petite stratégie de time-warp simple :

* copie proportionnelle
  ou
* mini-resample local

### 6.4 Pour `Pause`

Ne pas reposer des marks pitchés s’il n’y en a pas.
Le stage doit pouvoir créer une région de sortie même sans analysis marks.

---

## 7. Corriger le mapping analyse -> synthèse

### Fichier principal

* `tempo-service/infra/src/stages/synthesis_mapping.rs`

### Objectif

Transformer la grille corrigée en placements de grains cohérents.

### Changements demandés

### 7.1 Conserver la monotonie stricte

Le stage est déjà monotone , mais Codex doit ajouter :

* détection des longues répétitions du même grain
* alerte si le même `source_grain_index` est utilisé plus de `N` fois de suite

### 7.2 Ajouter un garde-fou anti-buzz

Si plus de `K` répétitions consécutives apparaissent :

* soit avancer d’un grain si possible
* soit signaler une anomalie
* soit lisser le mapping

---

## 8. Garder `GrainExtractionStage`, mais le rendre plus strict

### Fichier principal

* `tempo-service/infra/src/stages/grain_extraction.rs`

### Changements demandés

### 8.1 Garder une fenêtre ~2 périodes, mais configurable

Le principe actuel est correct .
Codex doit simplement :

* rendre `PERIOD_MULTIPLIER` configurable
* logguer les grains trop courts

### 8.2 Associer chaque grain à son repère utile

Ajouter si utile :

* `left_sample`
* `right_sample`
  dans les métadonnées de debug, pas forcément dans le modèle public

---

## 9. Refondre le traitement des pauses et non-voisés

### Fichiers principaux

* `tempo-service/infra/src/stages/stretch_region.rs`
* `tempo-service/infra/src/stages/unvoiced_handling.rs`

### Problème actuel

Le stage de stretch classe déjà `Pause`, `VoicedPsola`, `KeepNearConstant` , mais `unvoiced_handling.rs` n’effectue pas de vrai stretch des pauses : il se contente d’atténuer les samples existants .

### Changements demandés

### 9.1 Pour les segments `Gap`

Les traiter comme des segments autonomes :

* si le gap source est quasi silencieux : générer directement un silence de la bonne durée
* sinon : time-resample léger du gap

### 9.2 Pour les régions `Pause` à l’intérieur d’un segment mot

Au lieu de multiplier par `0.05`, appliquer une vraie stratégie :

* duplication / suppression simple avec crossfades
  ou
* resampling local si acceptable pour un POC

### 9.3 Pour `KeepNearConstant`

Faire une vraie stratégie conservative :

* garder le contenu
* appliquer seulement un faible resampling local si nécessaire
* surtout ne pas le laisser “au même index” quand la durée du segment change

---

## 10. Corriger `OverlapAddStage`

### Fichier principal

* `tempo-service/infra/src/stages/overlap_add.rs`

### Changements demandés

### 10.1 Ne plus écraser `local_samples`

Écrire le résultat dans `rendered_samples`.

### 10.2 Ne plus remplir les trous avec `original[i]` brut sans mapping temporel

Le gap-fill actuel copie le sample original au même index, ce qui n’est pas correct si la durée change .

Codex doit :

* soit ne pas faire de gap-fill brut
* soit faire un gap-fill basé sur le mapping local entrée -> sortie
* soit laisser `KeepNearConstant` être rendu par son propre sous-pipeline

### 10.3 Garder la normalisation par poids

La logique générale est bonne, mais il faut mesurer :

* pourcentage de samples non couverts
* poids min / max / mean
* zones de trous

---

## 11. Simplifier et corriger `RecombinationStage`

### Fichier principal

* `tempo-service/infra/src/stages/recombination.rs`

### Problèmes actuels

Le stage :

* relit `local_samples`
* re-trime encore selon `margin_left` / `margin_right`
* applique un boundary fade dont la branche droite ne fait pratiquement rien (`1.0 - weight + weight`) 

### Changements demandés

### 11.1 Recombiner uniquement `rendered_samples`

Le stage ne doit plus trim les marges heuristiquement.
Il doit simplement :

* copier l’audio non traité avant segment
* insérer `rendered_samples`
* copier l’audio non traité après segment

### 11.2 Corriger le crossfade

Remplacer `apply_boundary_fade` par un vrai crossfade entre :

* les `N` derniers samples déjà écrits
* les `N` premiers samples du bloc entrant

Avec une formule standard :

* `out = left * fade_out + right * fade_in`

---

## 12. Ajuster la construction de régions de stretch

### Fichier principal

* `tempo-service/infra/src/stages/stretch_region.rs`

### Changements demandés

### 12.1 Garder le principe actuel

La logique de distribution de `alpha` par poids est bonne en principe .

### 12.2 Mais ajouter une contrainte de cap local

Empêcher des `local_alpha` délirants sur de très petites régions :

* clamp par exemple dans `[0.5, 2.0]` pour une région interne
* reporter l’excès sur les pauses ou sur d’autres régions

### 12.3 Pour les segments `Gap`

Ne pas passer par ce stage de la même manière que les mots.

---

## 13. Étendre le debug export

### Fichier principal

* `tempo-service/infra/src/stages/debug_export.rs`

Le stage existe déjà et exporte toute la pipeline plus un `narrative.md` .

### Changements demandés

Ajouter dans le dump :

* `segment kind` (`Word` / `Gap`)
* repères `analysis_len`, `rendered_len`, `useful_len`
* stats sur le spacing des pitch marks
* stats sur le spacing des synthesis marks
* ratio `output_step / local_period`
* nombre de répétitions consécutives du même grain
* nombre de corrections d’octave
* couverture overlap-add réelle

Et dans le `narrative.md`, faire apparaître explicitement :

* les gaps créés
* la durée cumulée des gaps
* les segments à risque
* les segments où `VoicedPsola` a été bypassé

---

# Ordre d’implémentation recommandé pour Codex

## Phase A — refactor structurel

1. Modifier `entity.rs`
2. Modifier `SegmentPlanStage`
3. Modifier `SegmentExtractionStage`
4. Adapter `pipeline.rs` si besoin

## Phase B — correction pitch / marks

5. Corriger `frame_analysis.rs`
6. Corriger `f0_estimation.rs`
7. Corriger `voiced_zone.rs`
8. Corriger `pitch_mark.rs`

## Phase C — correction synthèse

9. Corriger `synthesis_grid.rs` en priorité
10. Ajuster `synthesis_mapping.rs`
11. Ajuster `grain_extraction.rs`
12. Corriger `overlap_add.rs`

## Phase D — pauses / recombinaison

13. Refondre `unvoiced_handling.rs`
14. Corriger `recombination.rs`
15. Étendre `debug_export.rs`

---

# Critères de validation à demander à Codex

Codex doit ajouter ou mettre à jour des tests pour vérifier :

* la création de segments `Gap`
* l’égalité approximative entre durée cumulée cible et durée de sortie
* l’absence de trimming parasite de 20 ms
* pour une voyelle synthétique pure :

  * `alpha = 1.0` -> sortie quasi identique
  * `alpha = 1.25` -> durée plus longue mais pitch inchangé
* la monotonie des synthesis marks
* la borne des écarts `delta_mark / T0`
* la baisse forte des répétitions absurdes du même grain
* la présence d’artefacts debug supplémentaires

---

# Instruction synthétique à donner à Codex

Travaille uniquement dans `tempo-service/domain` et `tempo-service/infra`.
Ne change pas l’API publique du port sauf nécessité absolue.
Conserve l’architecture pipeline actuelle (`TempoMatchAdapter` -> `TempoPipelineEngine` -> stages)  , mais refactore les données et les stages pour que :

* la timeline contienne **mots + gaps**
* les coordonnées analyse / rendu soient explicites
* les pitch marks soient plus stables
* les synthesis marks PSOLA gardent une **période de sortie compatible avec le pitch source**
* les pauses soient réellement stretched
* la recombinaison n’enlève plus de durée par trimming incohérent
* le debug export montre clairement spacing, mapping et anomalies

Le changement le plus urgent est dans `synthesis_grid.rs` :
pour `VoicedPsola`, **ne plus faire `period / alpha`**.
Le pitch doit rester constant ; c’est le mapping entrée/sortie qui doit créer duplication ou skipping, pas une compression artificielle de la période de sortie .

Si tu veux, je peux maintenant te transformer ça en **prompt Codex prêt à coller**, en anglais, avec ton architecture et les tâches ordonnées.
