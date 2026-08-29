# Experimental log

## 2026-08-25 — V27 character-balanced winning replay

Experiment: Continue the accepted A0/+24 V27 policy from `v27-a0-b24-win-1h/000003.selected.pt`. Keep PPO, BCE, learning rate, epochs, gates, and replay volume unchanged; sample winning replay with approximately equal quotas across characters.

Results: Generations 1 and 2 accepted; generation 3 was rejected by the `+2` win-margin rule (`20→21/40`). Across all three gates, win discordances were `23 favorable / 14 unfavorable` (`p=0.094`), floor deltas were `22 positive / 5 negative` (`p=0.00076`), caps changed by `−1`, and character 3 improved in every gate for a cumulative `+4` wins and `+16.0` mean-floor points. Training fractions were `82–86%`.

Analysis: Three prior direct-win candidates regressed character 3 while it held only 4.5% of replay. Equalized replay sampling removed that repeated regression in the first controlled generation. The gate had 12 favorable versus 6 unfavorable win discordances (one-sided sign test `p=0.119`) and 10 positive versus 3 negative nonzero floor deltas (`p=0.046`): credible direction, not yet replication.

Conclusion: Character-balanced replay fixed the targeted character-3 failure and strongly improved floor progression, but did not establish a statistically convincing aggregate win gain under the precommitted test. Keep it as the V28 default because it removed a reproducible imbalance without regressions or excess caps.

Next: Stop V27 at the completed generation-3 gate. Build and verify V28, then restart from random initialization; retain V27 artifacts only as benchmarks.

## 2026-08-25 — V27 cross-stage Markov audit

Experiment: Independently compare observations and transitions for identical seeded games under curriculum bonus `0` and `+24`, and audit the remaining end-to-end value workflow.

Results: Combat observations were byte-identical because the bonus Strength/Dexterity was subtracted, but the same Strike produced different enemy HP. Card occurrence/raw-field/zone/shared-embedding requirements pass, but derived previews still omit some public damage modifiers, deterministic Entrench/Reboot/Flak mechanics, exact capped draw counts, Sealed Throne star-cost projection, and random-target outcomes. The transformer checkpoint also has no calibrated Rust export path yet.

Analysis: The active fixed-`+24` run remains a useful within-stage benchmark, but the hidden modifier aliases distinct transition kernels once one shared model crosses curriculum stages.

Conclusion: V27 must not be the cross-stage production model.

Next: After the live benchmark finishes, fix the bonus observation and preview gaps together under one new feature/model version, restart from random initialization, then implement frozen-policy calibration and transformer-native Rust export before final A10 acceptance.

## 2026-08-25 — V28 clean A0/+24 restart

Experiment: Randomly initialize one shared five-character V28 model and train for one hour at A0/+24. V28 exposes the public curriculum modifier and actual combat powers, completes deterministic public card previews, retains character-balanced winning replay, and otherwise keeps PPO/BCE settings fixed.

Results: Generation 1 accepted. Training used 131,072 decisions and produced 9/480 exploratory wins; mean/median terminal floor reached `27.31/28`, BCE `0.184`, observed KL `0.0052`, and no caps. The paired greedy gate improved wins `3→14/40` and mean floor `29.43→45.40`; per-character wins changed `[1,1,1,0,0]→[3,6,2,0,3]`. Training fraction was `84.9%`. Post-run verification passed `86/86` Rust tests, full-token hidden-resampling bisimulation, bonus-observation/outcome sensitivity, forced real-run progression through both A10 bosses, reset-cache invalidation, Python-feature checks, extension rebuild, and runtime smoke.

Analysis: This is the first clean run whose observation distinguishes every configured curriculum stage while retaining the measured V27 replay improvement.

Conclusion: V28 is a strong clean small-model baseline, but character 3 still has no greedy wins after one generation.

Next: Preserve this checkpoint as the 566k-parameter baseline. Per the new scaling request, compare fresh ~5M width-heavy and depth-heavy models before resuming a long curriculum run; do not migrate V27/V28 weights or replay.

## 2026-08-25 — V29 width versus depth at ~5M parameters

Experiment: Compare two fresh BF16 models under identical A0/+24 seeds and 32,768 training decisions: width-heavy `W224/L2/H4/FF448` (4,958,243 parameters) versus depth-heavy `W160/L13/H4/FF320` (4,963,843 parameters).

Results: Architecture/BF16 support is implemented. Probe results: wide `4,958,243` parameters, BF16 max output error `0.00064`, 104 MB allocated; deep `4,963,843` parameters, BF16 max output error `0.00098`, 141 MB allocated. At 32,768 decisions, wide achieved exploratory mean floor `22.44`, BCE `0.375`, KL `0.0044`, and `93.5` decisions/s; deep achieved floor `19.77`, BCE `0.422`, KL `0.0046`, and `53.5` decisions/s. On the same first four held-out seeds per character, wide reached `5/20` wins and mean floor `43.75`; deep reached `2/20` wins and mean floor `37.40`.

Analysis: Depth provided no quality advantage at equal parameter count and was `1.75×` slower per decision. Wide was better on training floors, BCE, held-out floors, and wins. The wide trial's initial 16-seed gate plus premature progress confirmation reduced its training fraction to `54%`; bootstrap confirmation is now disabled until 500k decisions. The deep trial used `81%` training compute.

Conclusion: Select the width-heavy `W224/L2/H4/FF448` BF16 architecture.

Next: Start a clean long width-heavy run from random initialization with the corrected validation schedule; preserve both A/B artifacts as benchmarks only.

## 2026-08-25 — V29 5M selected-wide run

Experiment: Train a fresh `W224/L2/H4/FF448` shared model in BF16 at A0/+24 for one hour, using four-rollout generations, 8 seeds/character gates, and no pre-500k progress confirmation.

Results: Stopped during the first PPO update, before a complete rollout or gate, when the requested production size changed to ~2M parameters.

Analysis: The width-over-depth conclusion remains valid, but a 5M production model slows iteration unnecessarily.

Conclusion: Do not continue the 5M run.

Next: Preserve width and reduce to a hardware-aligned model near 2M parameters.

## 2026-08-25 — V29 2M selected-wide run

Experiment: Train a fresh hardware-aligned shallow/wide shared model in BF16 at A0/+24, retaining the width-over-depth result while restoring faster iteration.

Results: `W136/L2/H4/FF272` (2,017,019 parameters) was stopped before its first report because 34-value attention heads and the old repeated-update batch schedule took over five minutes. The selected replacement is `W128/L2/H4/FF256` (1,813,379 parameters), with 32-value attention heads. The async trainer now runs to a decision or time limit, reports by decision count, and consumes every fresh queued row at most once; probe consumed 37/37 unique rows once and ended an exact 96-decision budget. In matched 32,768-decision async probes, LR `3e-4` gave mean floor `20.64`, KL `0.00021`, and 32,768/32,768 rows trained; LR `1e-3` gave floor `23.42`, KL `0.00311`, clip `2.83%`, and 32,768/32,768 rows trained.

Analysis: Parameter count alone was misleading: hardware alignment and the obsolete repeated-update schedule dominated. `W128` is 3.20× the original 566k model and 36.6% of the tested 5M model while preserving the superior shallow/wide shape. Under one-pass async training, LR `3e-4` under-updates badly; `1e-3` is closer to the requested KL range and improves early floor progress.

Conclusion: Select `W128/L2/H4/FF256`; use the continuous async learner rather than rollout/epoch terminology or control.

Next: Restart from random initialization at LR `1e-3`, report every 32,768 decisions, checkpoint every 262,144 decisions, use four-step sampler chunks, and keep at least 75% of wall-clock compute on learning; monitor KL before increasing further.

## 2026-08-25 — V29 1.8M continuous async run

Experiment: Fresh A0/+24 training of the selected `W128/L2/H4/FF256` model in BF16. A CPU actor streams four-step trajectory chunks; the accelerator learner consumes each eligible row once. Reports are written every 32,768 decisions, immutable checkpoints every 262,144 decisions, and fresh 128-seed/character evaluation is reserved for promotion.

Results: Warm-continuation retained the model, optimizer, and 2,944 winning rows after the loop refactor. Through 589,824 total decisions, the learner trained 393,166/393,216 streamed rows with only 50 ratio-filtered drops and policy lag `2.49`. Early report windows reached mean floors `41.74`, `38.68`, and `31.20`, then collapsed as progress beta annealed: the last two fixed-`β=0.1` windows reached only `12.44` and `14.27`, with `28/79` and `25/78` combat caps and no broad win recovery. BCE improved from `0.435` to `0.145`; observed KL remained small (`0.00047–0.00078`). Learning occupied about `92%` of elapsed time.

Analysis: The queue remained fresh and nearly lossless, while the sampler exposed a real policy collapse that loss/KL alone would have missed. Two windows after the 500k bootstrap boundary falsified recovery at the prescribed progress plateau. The low KL and controlled clipping do not indicate an oversized single update; the continuous `1e-3` stream nevertheless drifts too quickly for the sparse shared objective.

Conclusion: Keep the continuous architecture and current weights. The temporary LR reduction did not repair behavior; the actual defect was the non-bootstrapped four-step progress signal.

Next: A 131,072-decision continuation at LR `3e-4` with the slower β schedule did not recover (`12.9–15.9` mean floor), so LR returned to `1e-3` without rollback. Audit then found that PPO used only raw four-step floor increments as its “progress advantage,” discarding the bootstrapped progress critic; ordinary and looping actions therefore received almost no auxiliary credit. Progress PPO now uses proper γ=`0.99` GAE, and the anneal spans 5M decisions. The run reached 1.8M decisions with a final-window mean/median floor of `39.04/43`, `12/82` wins, p10 `23`, and zero caps. The preceding peak window reached mean/median `43.70/49`; rolling 32-episode samples contained wins for every character. Continue in place toward 3M decisions; promotion still requires the independent fresh 128-seed/character gate.

## 2026-08-25 — Native value finalization path

Experiment: Prepare the final A10/+0 value workflow without interrupting policy training.

Results: `train.py finalize` now collects disjoint frozen-greedy fit/calibration/test trajectories, fine-tunes only the BCE win head, fits scalar temperature and bias, reports Brier/ECE and per-character constant baselines, and exports a versioned binary. Rust loads and evaluates the token transformer directly with exact terminal overrides. Random-model parity was `2.98e-7`; trained-checkpoint parity was `3.17e-8`; all 86 Rust tests passed.

Analysis: Final calibration is now operationally ready but remains invalid until the curriculum reaches A10/+0 and the greedy policy is frozen.

Conclusion: Retain the implementation; do not run final calibration early.

Next: Finish policy curriculum, then run the full disjoint 1,000-seed/character finalization and acceptance workflow.

## 2026-08-25 — Continuous model-version training

Experiment: Retire candidate generations. Each model version now owns one continuously updated policy, optimizer, winning reservoir, checkpoint stream, and sampler-metric history. Fresh evaluation remains only as the curriculum-promotion gate.

Results: V29 resumed in place from decision `1,900,544`. The dashboard is still the single `target/dashboard.html`, but its selector is now model version rather than run/generation and its default x-axis is cumulative training decisions. Before the precision fix, observed KL stayed around `0.013`, rejecting 85–89% of fresh rows. The first matched-FP32 window trained all `32,768/32,768` rows with zero ratio/KL drops; observed KL fell to `4.6e-5`, proving the prior rejection was numerical rather than genuine policy drift.

Analysis: The asynchronous sampler already measures the deployed behavior throughout learning, so periodic candidate construction and rollback duplicate that signal and interrupt optimizer continuity. Independent fresh runs remain necessary only before changing curriculum difficulty and for final calibration/test acceptance. A queue audit found that `publish()` retained the oldest pending actor snapshot; changing the size-one queue to latest-wins reduced policy lag from about `1.1` to below `0.2`, but did not remove KL rejection. A same-weight comparison then exposed the dominant issue: CPU-FP32 behavior and MPS-BF16 learner likelihoods differed by KL `0.01344` before any policy update, above the `0.008` gate.

Conclusion: Train each model version continuously; treat reports and immutable decision checkpoints as observations of one trajectory, not generations.

Next: Continue the same V29 model toward 3M decisions with matched FP32 actor/learner likelihoods and LR `1e-3`; target observed KL `0.005–0.01` and recover terminal-floor performance. Run the fresh promotion gate only after every character reaches the configured rolling win threshold.

## 2026-08-25 — V29 stop audit

Experiment: Validate the corrected continuous learner for three FP32 sampler windows, then audit simulator and action-feature fidelity before spending the remaining budget.

Results: Matched FP32 eliminated KL rejection: `98,279/98,304` rows trained in the last three-window session. The final window reached mean/median floor `40.85/44`, `19/89` wins, zero ordinary caps, and wins in every rolling character sample. Two independent source audits then found that Fan of Knives rewrites Shiv to AoE in gameplay but not action previews, and map generation selects room-count profiles using content act IDs rather than the public act ordinal. The latter produces incorrect first/third-act lengths and explains V29 victories ending at global floor 49 instead of the correct A0 floor 51.

Analysis: V29 is a useful optimizer/architecture benchmark, but its sampled game distribution is not the intended game and its promised target-damage features are false for a reachable card/power combination. More V29 training would optimize the wrong MDP.

Conclusion: Stop V29 at decision `2,392,064` and preserve its artifacts only as benchmarks.

Next: Fix both semantics, bump to V30/feature 29, rebuild and verify the extension, then start one randomly initialized five-character continuous run at A0/+24 with matched FP32 actor/learner likelihoods.

## 2026-08-25 — V30 corrected-map clean restart

Experiment: Start a new randomly initialized shared five-character policy/value model after correcting act-map profiles and Fan-of-Knives Shiv previews. Use the selected `W128/L2/H4/FF256` architecture, FP32 actor and learner likelihoods, one-pass asynchronous PPO, LR `1e-3`, and continuous sampler reporting at A0/+24.

Results: Feature version 29 and model version 30 build cleanly. The complete Rust suite passes `88/88`; the Python probe reports `1,813,379` parameters, token permutation error `2.42e-8`, Rust/Python value parity `3.58e-7`, exact one-pass row consumption, and the corrected token layout. V30 started from random weights with empty optimizer/replay state and disjoint training seed namespace `1,800,000,000`. At 327,680 every character first had a sampled win. The 500k audit at 491,520 retained rolling wins for all five, raised the weakest-character mean to `30.16`, and reached aggregate mean/median `36.38/33` with `5/76` wins and one cap. The 500k–1M windows sustained aggregate means mostly between `35.2` and `40.1`, with negligible caps and no KL rejection. At 964,352 every character again had rolling wins; aggregate mean/median was `38.36/41`, the weakest-character mean was `32.97`, and observed KL was `0.00579`. This is the new immutable best. The continuous stream reached exactly 1,000,000 decisions with progress beta `0.820`; its final full rolling samples were `[0,5,2,1,2]` wins and mean floors `[33.56,44.22,37.09,30.72,42.72]`. Successful trajectories terminate at the corrected floor 51.

Analysis: V29 remains a useful systems benchmark, but V30 is the first run in this architecture trained on the corrected full-run map distribution with internally consistent policy likelihoods. The 1M audit shows broad but still sparse connectivity: every character repeatedly reaches floor 51, while C3 remains the persistent weakest task near floor 31. Character-balanced winning replay already samples equal quotas and has enough C3 rows, so changing replay weights would not address the measured bottleneck. KL, clipping, caps, critic loss and queue freshness remain controlled. A resume-time fresh-row filter briefly tried to index non-tensor card-group metadata; repacking only actually filtered rows fixed it, and a reproduced 32,768-decision interval then completed in about 55 seconds.

Conclusion: V30 supersedes V29 for policy quality and curriculum decisions; V29 artifacts remain benchmark-only. The 1M evidence does not justify changing LR, progress shaping or replay before a longer continuous sample.

Next: Continue the same uninterrupted V30 stream to 1.5M decisions and reassess C3 on non-overlapping sampler data. Do not spend a fresh promotion gate until every character's rolling sampler win rate approaches the configured 70% threshold.

## 2026-08-25 — V30 1.5M continuous audit

Experiment: Continue the same A0/+24 V30 policy from 1M to 1,500,032 decisions without rollback or promotion evaluation.

Results: The best aggregate window was at 1,262,144: mean/median floor `40.87/44.5`, `13/92` wins, zero caps, and rolling wins for every character. The all-character checkpoint selector advanced to 1,294,912 because its weakest adjusted character floor was higher. At 1,500,032, rolling wins were `[1,7,3,2,3]/32` and mean floors `[35.56,41.91,43.22,32.06,37.81]`. Across the second 500k decisions, weighted mean floor changed from about `37.92` to `37.01` and win rate from `6.9%` to `7.7%`. KL, clipping, replay balance and caps remained healthy; progress beta was still `0.731`.

Analysis: C0 and C3 remain connected across non-overlapping samples, so character-specific replay or adapters are not indicated. Optimization is stable, but the high progress weight is the clearest remaining reason deep runs are not converting into wins: floor-51 losses receive nearly all of the auxiliary signal without completing the terminal objective.

Conclusion: Preserve continuous training and all immutable checkpoints. Do not change LR, replay, architecture or curriculum yet; isolate the progress-weight hypothesis.

Next: Continue from the latest 1,500,032 checkpoint for 262,144 decisions with `progress_decisions=2,500,000`, reducing beta immediately to about `0.46` and then toward `0.37`. Compare sampler win/floor curves with the preceding non-overlapping windows; do not run a promotion gate unless every character approaches 70% wins.

## 2026-08-25 — Faster progress anneal rejected

Experiment: Continue V30 from 1,500,032 with only `progress_decisions` changed from 5M to 2.5M, dropping beta from `0.731` to about `0.45`.

Results: The first two independent 32,768-decision windows fell to mean floors `25.21` and `33.36`, versus roughly `37–41` before the change. In the second window C3 fell to mean floor `19.91`, C4 to `27.88`, and two combat caps appeared. KL, filtering and learner throughput remained healthy, isolating the behavioral regression to the progress-weight intervention rather than optimizer instability.

Analysis: At this stage the dense progress signal still carries essential predecessor competence. Reducing it abruptly did not convert deep runs into more wins; it disconnected early and mid-run behavior across several characters.

Conclusion: Reject the faster anneal after 65,536 decisions. Preserve those weights as part of the continuous trajectory, but restore the 5M schedule immediately rather than spending the full 262k test budget.

Next: Continue from latest at 1,565,568 with `progress_decisions=5,000,000` and verify floor recovery before considering another intervention.

## 2026-08-25 — Progress-schedule recovery

Experiment: Restore the original 5M progress anneal from the continuous 1,565,568 state after rejecting the abrupt beta reduction.

Results: The first recovery window remained weak at mean floor `26.75`, but the next recovered to `40.53`. At 1,696,640 all five characters again had rolling wins; mean floor was `38.57`, C3 reached `35.91`, caps were negligible, and the all-character checkpoint selector recorded a new best. The recovery block ended at 1,762,176 with aggregate mean `38.62`, rolling wins `[0,6,2,1,2]`, and beta `0.686`.

Analysis: Recovery after restoring beta, with no LR/replay/architecture change, confirms that the abrupt auxiliary reduction caused the broad regression. The new best also shows the continuous policy is still improving rather than irreversibly damaged by the short failed intervention.

Conclusion: Keep the 5M progress schedule. Progress remains necessary scaffolding at A0/+24; future annealing changes should be gradual and evidence-driven.

Next: Continue the same V30 stream unchanged to 2M decisions. Reassess trend and C3 there; promotion still requires the independent 70%-per-character gate.

## 2026-08-25 — V30 2M audit

Experiment: Continue the recovered V30 stream from 1,762,176 to 2M using the verified 5M progress schedule.

Results: Aggregate sampler windows remained stable around mean floor `37–39`, with negligible caps and no KL-dropped batches. The best checkpoint remains 1,696,640, where every character had wins and the weakest mean floor was `35.91`. At 2M, rolling wins were `[1,5,2,0,6]/32`, mean floors `[36.75,41.03,38.56,32.56,43.09]`, observed KL was `0.00174`, and beta was `0.641`. C3 had three roughly non-overlapping zero-win windows after the best checkpoint while other characters remained connected.

Analysis: C3 is the persistent bottleneck, but neither optimizer health nor replay balance explains it. An abrupt beta reduction already caused broad collapse, so another immediate schedule jump would confound lower terminal weighting with a second distribution shock.

Conclusion: Do not add a character-specific adapter or abruptly change beta yet. Preserve the shared architecture and let the verified schedule enter the same lower-beta regime smoothly.

Next: Continue unchanged to 2,500,032 decisions, where beta naturally reaches about `0.55`; then compare C3 wins and floor reach against the 1.5M–2M block.

## 2026-08-25 — V30 2.5M smooth-anneal audit

Experiment: Continue the same A0/+24 V30 stream from 2M to 2,500,032 decisions without changing the verified 5M progress schedule.

Results: Between the earlier and later complete windows, weighted mean floor improved `36.52→37.72` and win rate `8.1%→8.7%`. C3 first fell into the `26.9–29.4` mean-floor range, then recovered through `32.2`, `34.2`, and `36.1` with recurring wins. The block trained `499,686/500,032` rows, dropped no batches for KL, had only `3/583` caps, and ended at beta `0.551`.

Analysis: Smoothly entering the lower-beta regime did not reproduce the earlier abrupt-anneal collapse. Optimization, queue freshness, BCE/progress losses, and replay remained stable. C3 is still the weakest character, but its late recovery argues against changing LR, replay, architecture, or objective on this sample.

Conclusion: The current continuous schedule remains viable but is still far below the per-character promotion threshold.

Next: Continue unchanged to 3M decisions and reassess non-overlapping C3 and aggregate trends. Keep fresh greedy evaluation reserved for curriculum promotion.

## 2026-08-26 — V31 complete-trajectory restart

Experiment: Preserve V30 as a benchmark and start V31 from random initialization with the asynchronous sampler retaining partial episodes and sending only complete Won/Dead trajectories to the learner.

Results: V31 keeps the `W128/L2/H4/FF256` shared 1,813,379-parameter model and player-visible feature version 29, while model/value format advances to 31. Full-trajectory GAE resets exactly at trajectory boundaries; artificial step/combat/empty-action cuts are reported but excluded from learning rather than labeled as deaths. The probe verifies terminal returns, retained partial trajectories, excluded cuts, exact five-character batch quotas, one-pass row consumption, BF16 forward/backward viability, token permutation invariance, and Rust/PyTorch value parity `1.79e-7`. Rust tests pass `88/88`.

Analysis: Complete trajectories remove the short-chunk credit horizon while retaining row-wise behavior-policy versions, stale-ratio filtering, terminal-only BCE win targets, and separate progress shaping. V30's late collapse and optimizer history are intentionally not imported.

Conclusion: V31 is a clean model/training boundary; all V30 artifacts remain benchmark-only.

Next: Train continuously from scratch at A0/+24 with FP32 actor/learner likelihoods, LR `1e-3`, terminal-trajectory PPO, the 5M progress schedule, and no fresh greedy evaluation until the rolling per-character promotion condition is approached.

## 2026-08-26 — V31 first 262k decisions

Experiment: Run the fresh V31 complete-terminal-trajectory learner unchanged through its first immutable 262,144-decision checkpoint.

Results: Mean terminal floor rose `20.42→27.63→33.26→38.93→38.04→40.67→41.74→42.37`. The latest rolling samples have wins `[10,6,5,2,4]/32` and mean floors `[43.50,42.03,42.16,39.47,42.25]`; all five characters won, with zero caps. The learner trained `261,743/262,144` rows, dropped no batches for KL, and kept observed policy KL at `0.00182`. Winning replay used `5.55%` of rows with accepted categorical KL `0.00522`.

Analysis: Whole-run credit reaches terminal outcomes far earlier than V30's short-chunk learner. C3 remains weakest but is already connected, and there is no optimizer, sampler, or truncation failure to justify intervention.

Conclusion: Keep V31 unchanged; the trajectory redesign is strongly supported by early learning.

Next: Continue continuous A0/+24 training toward the 5M decision target and reserve fresh greedy evaluation for the 70%-per-character promotion condition.

## 2026-08-26 — V31 winning-replay re-anchoring

Experiment: Resume V31 from decision `786,432` after replacing permanent KL-stale winner eviction with per-row abstention and re-anchoring to the current policy. Keep replay capped below 10% and all other training settings unchanged.

Results: At `819,200`, the reservoir retained `2,849` rows while `325` stale proposals were skipped and refreshed; replay used `9.13%` of training rows. At `851,968`, the reservoir reached its `4,096`-row cap, replay remained `8.99%`, and accepted replay KL was `0.00111`. That window produced `37/81` wins, mean/median terminal floor `48.80/51`, and zero caps or empty-action states.

Analysis: The former collapse was replay bookkeeping, not a lack of winning data. Re-anchoring preserves rare public winning states while making stale rows ineligible for one update instead of deleting them. The accepted subset remains inside the KL screen and the 10% data cap.

Conclusion: Keep the re-anchored reservoir and continue the same V31 model. The repair restores stable winning replay without destabilizing PPO.

Next: Continue toward 5M decisions, monitor per-character wins and reservoir balance, and reserve fresh greedy evaluation for curriculum promotion.

## 2026-08-26 — V31 1.31M audit

Experiment: Continue the repaired V31 stream for 524,288 decisions and compare its 16 non-overlapping reports with the preceding equal-size block.

Results: Post-repair V31 produced `482/1,366` wins (`35.29%`), mean terminal floor `46.77`, and one cap, versus `399/1,328` wins (`30.05%`), mean floor `45.37`, and three caps before repair. The latest rolling character wins are `[12,14,15,11,11]/32`; the reservoir remains full and replay uses `8.62%` of rows with accepted KL `0.00155`.

Analysis: Both halves of the repaired block remain strong, so the improvement is not a short transient. C3/C4 still limit promotion, but replay balance, freshness, entropy, clipping and throughput show no mechanism failure that supports changing the objective or optimizer.

Conclusion: Continue V31 unchanged. Promotion is still materially distant because every character needs at least `23/32` rolling wins before the fresh gate.

Next: Train continuously to 2M decisions, then compare another non-overlapping block before considering any intervention.

## 2026-08-26 — V31 2M audit

Experiment: Continue V31 unchanged from 1.31M to the `1,998,848` report and compare equal non-overlapping 524k-decision blocks.

Results: Win rate improved from `35.29%` (`482/1,366`) to `37.24%` (`502/1,348`), mean floor from `46.77` to `47.59`, and caps from one to zero. The latest five windows reached `179/422` wins (`42.42%`) and mean floor `47.93`. Floor-51 reach rose `72.2%→75.9%→78.7%`; conversion of floor-51 runs into wins rose `48.9%→49.1%→53.9%`. Latest rolling wins are `[13,19,10,14,13]/32`.

Analysis: The remaining failure is final-act conversion rather than reachability, and that conversion is already improving as progress beta anneals smoothly to `0.643`. Replay remains full at `8.77%` with accepted KL `0.00149`; entropy, policy freshness, clipping and caps are healthy.

Conclusion: Do not change LR, replay, auxiliary schedule or curriculum while both reach and conversion improve.

Next: Continue unchanged to 2.5M decisions, then reassess another non-overlapping block. Promotion still requires at least `23/32` rolling wins for every character before a fresh gate.

## 2026-08-26 — V31 2.5M audit

Experiment: Continue V31 unchanged through the one-hour boundary at `2,538,001` decisions while progress beta anneals naturally to about `0.54`.

Results: The latest equal 16-window block reached `559/1,376` wins (`40.62%`), mean floor `47.69`, floor-51 reach `78.27%`, and two caps. The final six windows improved to `202/459` wins (`44.01%`), mean floor `48.76`, floor-51 reach `85.4%`, and zero caps. Latest rolling wins are `[17,16,12,14,13]/32`; replay remains full at `8.79%` with accepted KL `0.00142`.

Analysis: Reachability is now strong and final-act conversion is the remaining bottleneck. Conversion is flat near `51.5%`, but connectivity and aggregate wins are still rising, so abruptly lowering progress weight would confound a healthy trend.

Conclusion: Continue unchanged; no LR, replay, auxiliary or curriculum intervention is supported.

Next: Resume to approximately 3M decisions, allowing beta to fall smoothly toward `0.46`, then reassess terminal conversion and per-character promotion deficits.

## 2026-08-26 — V31 3M audit

Experiment: Continue V31 unchanged from `2,538,001` to `3,000,363` decisions, with replay acceptance/re-anchoring measured per character.

Results: This 15-report session reached `542/1,197` wins (`45.28%`), mean floor `48.65`, floor-51 reach `83.46%`, final-state conversion `54.25%`, and one cap. The preceding equal block produced `41.06%` wins, floor `47.99`, reach `80.35%`, and conversion `51.10%`. Accepted replay counts were balanced `[9172,9155,9203,9191,9032]`; re-anchors were also balanced. Latest rolling wins are `[19,15,19,11,19]/32`.

Analysis: C3 is a final-boss conversion bottleneck rather than a reachability problem: at least 24/32 recent runs reached floor 51, while 11 won. C3 has varied between 9 and 16 wins rather than collapsing, and replay already supplies adequate balanced coverage. Progress beta reached `0.460`; entropy and KL remain healthy.

Conclusion: A C3-specific intervention is premature while aggregate and final-state conversion continue improving.

Next: Continue unchanged to 3.5M decisions, allowing beta to decline smoothly toward `0.37`, then reassess C3 conversion and promotion readiness.

## 2026-08-26 — V31 3.5M stop audit

Experiment: Continue V31 unchanged to `3,499,964` decisions and audit Regent-specific representation and simulator behavior after C3 remained the promotion bottleneck.

Results: The final block produced `585/1,275` wins (`45.88%`), mean floor `49.00`, floor-51 reach `86.51%`, and one cap. Latest rolling wins were `[14,15,14,11,14]/32`; C3 alone had p25 floor `43.5` while every other character had p25 floor `51`. Source audits then found that Seeking Edge/Sovereign Blade, Child of the Stars and Black Hole have incorrect action previews, Divine Destiny is inert, Orbit copies are merged instead of independent, and Mini Regent triggers once per combat rather than once per turn.

Analysis: V31 demonstrated that complete-trajectory PPO, re-anchored winning replay and the 1.8M-parameter shared model learn effectively. Its C3 result is not a valid basis for replay weighting, however, because both public action features and Regent gameplay differ from the intended game.

Conclusion: Stop V31 and preserve it only as a systems/learning benchmark. Do not reuse its weights after the semantic corrections.

Next: Land and verify the Regent fixes under feature 30/model 32, rebuild the extension, and start a randomly initialized shared V32 run.

## 2026-08-26 — V32 corrected-Regent clean restart

Experiment: Correct Regent action previews and gameplay, advance to feature 30/model 32, rebuild, and start one randomly initialized shared five-character policy/value model at A0/+24 with LR `3e-4` and fresh training/promotion seeds.

Results: Seeking Edge/Sovereign Blade AoE, Child of the Stars block, Black Hole damage, first-turn Divine Destiny Stars, and independent Orbit instances now match gameplay. Mini Regent's existing per-turn reset gained regression coverage. All `93/93` Rust tests, Python-feature checks, extension build, token probe and Rust/PyTorch parity pass. A complete-trajectory smoke trained `8,543/8,543` rows without drops or caps. The first live 32,768-decision window reached mean floor `21.05` with zero wins and zero caps.

Analysis: V31 proved the learning system but encoded and simulated reachable Regent decisions incorrectly. V32 is a necessary clean semantic boundary; no V31 weights, optimizer state, or winning trajectories are reused.

Conclusion: V32 is the first valid run for evaluating corrected Regent performance.

Next: Continue V32 from scratch, monitor all five characters and terminal connectivity, and reserve fresh evaluation for curriculum promotion.

## 2026-08-26 — V32 cost-bucket isolation

Experiment: Diagnose a learner update that remained CPU/MPS-bound for over four minutes after decision `163,840`, then resume from that intact checkpoint with strict token-cost batches.

Results: The sampler had selected one cost bucket but filled character shortages and winning replay from arbitrary buckets, allowing one high-action row to pad a 1,024-row update. Fresh sampling now stays entirely within one bucket, rare buckets drain in smaller batches, and replay rows must be no more expensive than the fresh bucket. The probe covers cross-bucket exclusion. V32 resumed through `360,448` decisions without another stall, reaching `12/83` wins, mean floor `42.27`, and zero caps.

Analysis: The failure was batching, not policy learning or the Regent fixes. Bucket-local selection preserves one-pass consumption and the 10% replay cap while bounding padded work.

Conclusion: Keep the corrected V32 checkpoint and continue training; no model or optimizer rollback is needed.

Next: Monitor slow-update diagnostics and memory while continuing the clean V32 run.

## 2026-08-26 — V32 learning-rate restart

Experiment: Train corrected V32 from scratch for 524,288 decisions at the originally specified LR `3e-4`, then compare its early block with the otherwise similar V31 `1e-3` run.

Results: V32 produced `140/1,760` wins (`7.95%`) and mean floor `34.19`; V31's first equal block produced `235/1,598` wins (`14.71%`) and mean floor `37.99`. V32 observed KL was only `0.00070` with zero observed clipping, well below the requested active-update regime. The cost-bucket fix remained stable through the checkpoint.

Analysis: The lower rate materially under-updates the policy. Warm continuation would retain a weak-policy reservoir and consume 524k of the progress/entropy schedules, confounding the comparison.

Conclusion: Preserve the LR `3e-4` run as a benchmark and restart V32 from random initialization at the previously stable LR `1e-3`, keeping architecture, seeds and all other settings fixed.

Next: Compare the fresh `1e-3` run at 524k decisions; require materially stronger win/floor learning with controlled KL and clipping.

## 2026-08-26 — V32 learning-rate comparison

Experiment: Compare clean, same-seed V32 runs at LR `3e-4` and `1e-3` through exactly 524,288 decisions.

Results: LR `3e-4` produced `140/1,760` wins (`7.95%`), mean floor `34.185`, zero caps and zero KL-dropped rows. LR `1e-3` produced `84/1,734` wins (`4.84%`), mean floor `34.205`, one cap and `13,275` KL-dropped rows after an observed-KL spike to `0.0308`.

Analysis: The higher rate did not improve reachability and materially harmed terminal conversion. The requested KL range is not a useful target when it conflicts with direct complete-trajectory outcomes in this one-pass learner.

Conclusion: Reject LR `1e-3`; preserve it as a benchmark. Resume the stronger LR `3e-4` V32 checkpoint rather than running another clean midpoint experiment.

Next: Continue LR `3e-4` unchanged to 1M decisions and measure its learning slope over the next independent 500k block.

## 2026-08-26 — V32 1M audit

Experiment: Continue the accepted LR `3e-4` V32 lineage from 524,288 to `1,000,166` decisions with strict cost-bucket batching.

Results: Win rate increased from `7.95%` in the first block to `19.84%` in the second, mean floor from `34.19` to `43.26`, floor-51 reach from `21.99%` to `48.25%`, and floor-51 conversion from `36.18%` to `41.13%`. Both blocks had zero caps. Latest rolling wins are `[5,10,6,4,6]/32`; observed KL is `0.00047` and beta `0.821`.

Analysis: Learning accelerated broadly despite low KL, while the matched `1e-3` experiment already showed that increasing update size harms terminal conversion. Corrected Regent is currently weakest but is not isolated from the rest of a still-bootstrapping policy.

Conclusion: Keep LR `3e-4`, replay, architecture and auxiliary schedule unchanged.

Next: Continue V32 to 1.5M decisions and reassess the non-overlapping learning slope and per-character connectivity.

## 2026-08-26 — V32 1.5M audit

Experiment: Continue the accepted LR `3e-4` V32 lineage from 1M to `1,500,151` decisions without changing replay, architecture or auxiliary schedules.

Results: Compared with the preceding block, win rate rose `19.84%→25.04%`, mean floor `43.26→44.18`, floor-51 reach `48.25%→55.58%`, and floor-51 conversion `41.13%→45.05%`; both blocks had zero caps. Latest rolling wins are `[6,8,8,6,10]/32`, with C3/C4 still weaker but connected.

Analysis: Both reach and conversion continue improving while beta anneals smoothly to `0.731`. No localized plateau or optimizer instability supports changing replay, learning rate or curriculum.

Conclusion: Continue the same V32 policy unchanged.

Next: Train to 2M decisions and reassess another non-overlapping block before considering intervention.

## 2026-08-26 — V32 2M audit

Experiment: Continue the accepted LR `3e-4` V32 lineage from 1.5M to `2,000,150` decisions with no changes to the model, replay, curriculum, or auxiliary schedules.

Results: The block produced `356/1,465` wins (`24.30%`), mean floor `44.58`, floor-51 reach `57.0%`, floor-51 conversion `42.63%`, and zero caps. The preceding block produced `25.04%` wins, mean floor `44.18`, `55.58%` floor-51 reach, and `45.05%` conversion. Latest rolling wins are `[7,13,7,11,8]/32`, with mean floors `[45.88,46.59,45.75,44.25,47.0]`. Observed KL is `0.00022`, beta `0.641`, and entropy `1.246`.

Analysis: Win rate and final-act conversion flattened slightly, but reach and mean floor still improved. This is one plateau block rather than a confirmed reversal; all characters remain connected, caps are absent, and optimization is stable. The controlled LR `1e-3` run already showed that increasing update size harms conversion.

Conclusion: Keep V32 at LR `3e-4` unchanged for one more matched block.

Next: Continue to 2.5M decisions. Change the optimizer, replay, or progress schedule only if the next block confirms a conversion decline or broader regression.

## 2026-08-26 — V32 stopped at 2.262M

Experiment: Continue V32 unchanged toward 2.5M while independently auditing representation and simulator correctness.

Results: Training reached `2,262,294` decisions with zero caps. The process then exposed a zero-action-token batching bug: an empty `EmbeddingBag` produced one phantom row and failed `index_add`. Terminal offsets plus `include_last_offset=True` fix the invariant; the MPS probe, zero-token forward/backward regression, and Rust parity pass. Independent audits found that single-player Neow can offer the multiplayer-only inert Massive Scroll, and that End of Days, Shrink, Adroit/Swift enchantments, Knockout Blow, and Seeker Strike have reachable incorrect public action previews.

Analysis: The crash is checkpoint-compatible, but continuing V32 would spend compute on a known-wrong transition distribution and false action mechanics. Its artifacts remain useful only as a benchmark.

Conclusion: Stop V32 at `2,262,294`; do not promote or resume it as the authoritative policy.

Next: Correct the simulator and preview errors, advance the feature/model boundary, rebuild and verify, then start one shared five-character model from random initialization.

## 2026-08-26 — V33 correctness boundary

Experiment: Remove the multiplayer-only Massive Scroll from single-player Neow, correct the six audited action-preview paths, and advance to feature 31/model 33 before restarting from scratch.

Results: Neow no longer offers Massive Scroll across a deterministic 128-seed regression. Public action previews now cover target-aware End of Days, Shrink ordering, Adroit/Swift derived effects, lethal Knockout Blow Stars and Black Hole damage, and Seeker Strike's one-card selection semantics. The zero-action-token batching path now uses terminal `EmbeddingBag` offsets. All `98/98` Rust tests, Cargo checks, formatting, extension rebuild, MPS probe, zero-token regression and Rust parity (`4.2e-7`) pass. A fresh 8,234-decision async smoke trained every row with zero caps or drops.

Analysis: These are reachable transition and representation corrections, so V32 weights and replay are incompatible evidence rather than a valid initialization. The 1.813M-parameter architecture itself remains healthy.

Conclusion: Preserve V32 only as a benchmark. V33 is the new clean semantic and model boundary.

Next: Start one randomly initialized shared V33 model at A0/+24 with LR `3e-4`, fresh disjoint seeds, and unchanged continuous complete-trajectory PPO settings.

## 2026-08-26 — V33 500k audit

Experiment: Train the authoritative V33 model from random initialization through 491,520 decisions at A0/+24 and compare its first matched block with V32 LR `3e-4`.

Results: V33 produced `157/1,574` wins (`9.97%`), mean floor `34.56`, floor-51 reach `25.35%`, and `39.35%` conversion after reaching floor 51. Matched V32 produced `129/1,669` wins (`7.73%`), mean floor `33.71`, `20.85%` floor-51 reach, and `37.07%` conversion. Latest V33 rolling wins are `[9,9,9,5,6]/32`. Training has zero caps or KL drops, observed KL `0.00101`, clip fraction `1.36%`, and a full self-generated winning reservoir.

Analysis: Different seed blocks prevent a paired claim, but all four aggregate outcomes favor V33. Regent remains weakest, yet it is connected and improving; the system is otherwise stable and V32 previously accelerated strongly between 500k and 1M.

Conclusion: Keep LR, replay, model, curriculum and auxiliary schedules unchanged.

Next: Continue V33 to 1M decisions and compare the second independent 500k learning slope before any intervention.

## 2026-08-26 — V33 1M audit

Experiment: Continue V33 unchanged from 524,288 through `1,015,808` decisions and aggregate the complete second learning block.

Results: The second block produced `279/1,228` wins (`22.72%`), mean floor `44.19`, floor-51 reach `54.80%`, and `41.46%` conversion after reaching floor 51. V32's matched second block produced `19.84%` wins, mean floor `43.26`, `48.25%` floor-51 reach, and `41.13%` conversion. Latest V33 rolling wins are `[10,9,9,7,5]/32`, with mean floors `[45.47,45.72,45.41,41.69,42.97]`. One empty-action trajectory was conservatively truncated; there were no KL drops and no repeated caps.

Analysis: V33 preserves its first-block advantage and reproduces the expected 500k→1M acceleration. The gain is primarily better connectivity, with conversion also slightly ahead. All characters win, while C3/C4 remain the weakest but are not disconnected. The one empty-action event is not reproducible from current aggregate artifacts and has not repeated.

Conclusion: Continue V33 unchanged; no learning-rate, replay, auxiliary or curriculum change is supported.

Next: Train to 1.5M decisions. Add concise deterministic empty-action trace logging before the next resumed session so any recurrence can be replayed exactly.

## 2026-08-26 — V33 1.5M audit and stop

Experiment: Continue V33 from `1,015,808` through `1,500,103` decisions, with deterministic seed/action tracing enabled for empty-action truncations.

Results: The block produced `326/1,217` wins (`26.79%`), mean floor `45.06`, floor-51 reach `60.64%`, and `44.17%` conversion after reaching floor 51. These improve on the preceding block's `22.72%`, `44.19`, `54.80%`, and `41.46%`. There were two caps in roughly 1,200 episodes. The empty-action trace reproduced exactly at seed `1880000744`: Stampede auto-played upgraded Dagger Throw, its draw created a mandatory discard choice, and `end_turn` continued processing until the hand was empty while leaving the stale choice active.

Analysis: Policy learning is healthy and still improving, but the reproduced transition is a real simulator error rather than benign truncation noise. Continuing V33 would train on an invalid public transition kernel.

Conclusion: Stop V33 at `1,500,103` and preserve it as the strongest benchmark so far; do not promote it.

Next: Make end-turn resolution pause immediately when an autoplay creates a choice, add a reachable regression, advance the feature/model boundary, rebuild, and restart the shared model from random initialization.

## 2026-08-26 — V34 end-turn continuation boundary

Experiment: Reproduce the traced Stampede→upgraded Dagger Throw deadlock, correct end-turn continuation semantics, and advance to feature 32/model 34.

Results: Seed `1880000744` and its recorded 584-action path reproduce the stale mandatory discard choice exactly. End turn now queues an internal continuation beneath Stampede, pauses with the hand and legal discard choice intact, and resumes the remaining end-turn sequence only after selection. The focused regression, all `99/99` Rust tests, Cargo checks, formatting, extension rebuild, zero-token probe and Rust parity (`3.6e-7`) pass.

Analysis: The fault was in simulator control flow, not the sampler or truncation logic. V33 learned effectively through 1.5M decisions, but its transition kernel included this reachable invalid state.

Conclusion: Preserve V33 only as a benchmark. V34 is the next authoritative clean boundary and must start from random initialization.

Next: Start one shared V34 model at A0/+24 with the same proven LR `3e-4`, fresh disjoint seeds, continuous complete-trajectory PPO, and deterministic empty-action tracing retained.

## 2026-08-26 — V34 500k audit

Experiment: Train V34 from random initialization through `491,520` decisions at A0/+24 and compare its first complete block with the V33 and V32 benchmarks.

Results: V34 produced `75/1,609` wins (`4.66%`), mean floor `34.74`, floor-51 reach `19.27%`, and `24.19%` conversion after reaching floor 51. V33 produced `9.97%`, `34.56`, `25.35%`, and `39.35%`; V32 produced `7.73%`, `33.71`, `20.85%`, and `37.07%`. Latest V34 rolling wins are `[3,8,3,2,2]/32`, with mean floors `[41.50,43.25,41.97,37.78,39.78]`. The block had one cap, no empty-action states, no KL drops, observed KL `0.00063`, and a full winning reservoir.

Analysis: Corrected end-turn semantics preserve broad floor progress but expose weaker terminal conversion than the invalid predecessor kernels. The weakness spans all characters, while learning remains numerically stable. Both V32 and V33 accelerated strongly during their second 500k block, so the first block is too early for a targeted intervention.

Conclusion: Keep V34 authoritative and unchanged at LR `3e-4`.

Next: Continue V34 to 1M decisions and compare the second independent learning block before changing the optimizer, replay, auxiliary schedule, or curriculum.

## 2026-08-26 — V34 1M audit

Experiment: Continue V34 unchanged from `524,288` through `1,015,808` decisions and measure the second complete learning block.

Results: The block produced `234/1,390` wins (`16.83%`), mean floor `43.22`, floor-51 reach `47.91%`, and `35.14%` conversion after reaching floor 51, with zero caps and zero empty-action states. The first block produced `4.66%`, `34.74`, `19.27%`, and `24.19%`. V32 and V33 reached `19.84%` and `22.72%` wins in their matched blocks, but both used invalid predecessor kernels.

Analysis: V34 accelerated strongly across wins, floor progress, connectivity, and conversion. Its connectivity now nearly matches V32, while lower terminal conversion is consistent with the corrected end-turn task rather than demonstrated optimizer failure. Latest character windows are uniformly noisy rather than showing one disconnected character.

Conclusion: Keep V34 unchanged at LR `3e-4`.

Next: Continue to 1.5M decisions and require another matched block before changing the optimizer, replay, auxiliary schedule, architecture, or curriculum.

## 2026-08-26 — V34 1.5M audit

Experiment: Continue V34 unchanged from `1,015,808` through `1,500,263` decisions, preserving the model, optimizer, winning reservoir, schedules, and seed progression across the one-hour session boundary.

Results: The block produced `278/1,380` wins (`20.14%`), mean floor `43.98`, floor-51 reach `51.74%`, and `38.94%` conversion after reaching floor 51. The preceding block produced `16.83%`, `43.22`, `47.91%`, and `35.14%`. There was one ordinary combat cap and zero empty-action states. Latest rolling wins are `[7,10,6,6,11]/32`, with mean floors `[45.16,43.78,43.19,43.22,44.16]`.

Analysis: Every aggregate outcome improved. All characters remain in the same broad learning regime, so neither targeted replay nor curriculum changes are supported. The invalid-kernel V33 benchmark remains stronger, but V34's own corrected-task slope is positive.

Conclusion: Keep V34 unchanged at LR `3e-4`.

Next: Continue to 2M decisions and reassess only after another complete matched block.

## 2026-08-26 — V34 2M audit and stop

Experiment: Continue V34 unchanged from `1,500,263` through `2,000,195` decisions while running independent public-state and action-preview audits.

Results: The block produced `294/1,440` wins (`20.42%`), mean floor `44.48`, floor-51 reach `55.07%`, and `37.07%` conversion after reaching floor 51, with zero caps and zero empty-action states. The preceding block produced `20.14%`, `43.98`, `51.74%`, and `38.94%`. Audits found a real hidden-state collision in future Wriggler parity and deterministic public action-preview omissions for play-trigger damage, block and draw, hand-moving effects, and per-enemy AoE outcomes.

Analysis: Connectivity continued improving, but win conversion flattened. More importantly, V34 does not yet satisfy the player-visible Markov or promised action-feature contract, so additional training would optimize an invalid representation.

Conclusion: Stop V34 at `2,000,195` and preserve it only as a benchmark; do not promote it.

Next: Fix future Wriggler parity using explicit public state, complete deterministic action previews and dashboard details, advance the feature/model boundary, rebuild and verify, then restart one shared model from random initialization.

## 2026-08-26 — V35 public-state and action-outcome boundary

Experiment: Eliminate the audited V34 hidden-state collision and complete deterministic public action outcomes before restarting training.

Results: Feature version `33`/model version `35` adds an explicit token for the parity of the next free Wriggler instance without exposing unrelated enemy instances. Action previews now include deterministic Helmet, Danse, Ash, Ornamental Fan, Kusarigama, Letter Opener, Iron Club and Tuning Fork outcomes; hand-bound MoveAll/Select effects; and per-enemy deterministic AoE vulnerability, block, cap, damage, actual HP loss and lethality rows. Historical and current dashboard versions now show their training configuration, entropy schedule, advantage definitions, losses and replay constraints. All `104/104` Rust tests, formatting, Python-feature checks, extension rebuild, MPS probe, token permutation invariance and Rust/Python parity (`3.6e-7`) pass. The shared model remains `1,813,379` parameters.

Analysis: These changes repair a real player-visible Markov violation and fields explicitly promised by the action representation. V34 weights and replay therefore cannot initialize V35.

Conclusion: Preserve V34 only as a benchmark. V35 is the new authoritative clean boundary.

Next: Run a short end-to-end smoke, then start one randomly initialized shared V35 model at A0/+24 with the unchanged proven PPO settings and fresh disjoint seeds.

## 2026-08-26 — V35 500k audit

Experiment: Train the authoritative V35 model from random initialization through `491,520` decisions at A0/+24 and compare its first matched block with V34.

Results: V35 produced `128/1,651` wins (`7.75%`), mean floor `34.82`, floor-51 reach `23.56%`, and `32.90%` conversion after reaching floor 51. V34 produced `4.66%`, `34.74`, `19.27%`, and `24.19%`. V35 had one ordinary combat cap and zero empty-action states. Latest rolling wins are `[4,5,6,2,6]/32`, with mean floors `[44.03,40.62,44.34,38.88,43.69]`.

Analysis: V35 materially improves terminal wins, late-act reach and conversion while preserving mean floor, isolating a real decision-quality gain from the corrected public representation rather than easier progression. C3 remains weakest but is connected and the model is still in broad bootstrap.

Conclusion: Keep V35 unchanged at LR `3e-4`.

Next: Continue to 1M decisions and compare the second complete learning block before any targeted intervention.

## 2026-08-26 — V35 1M audit

Experiment: Continue V35 unchanged from `524,288` through `1,015,808` decisions and aggregate the second complete learning block.

Results: The block produced `243/1,412` wins (`17.21%`), mean floor `42.31`, floor-51 reach `45.04%`, and `38.21%` conversion after reaching floor 51, with zero caps and zero empty-action states. The first block produced `7.75%`, `34.82`, `23.56%`, and `32.90%`. Matched V34 produced `16.83%`, `43.22`, `47.91%`, and `35.14%`.

Analysis: V35 more than doubled its win rate and nearly doubled late-act reach from its first block. Relative to V34 it trades modest floor reach for higher win rate and materially better terminal conversion, consistent with the intended representation correction rather than regression. C3 remains weakest but is connected.

Conclusion: Keep V35 unchanged at LR `3e-4`.

Next: Continue to 1.5M decisions before considering any character-specific or objective intervention.

## 2026-08-26 — V35 curriculum and precision update

Experiment: After the 1.5M audit, lower the fresh greedy curriculum gate from 70% to strictly above 20% wins per character and switch subsequent training updates from FP32 to BF16.

Results: The promotion window and independent promotion gate continue to count caps as failures; with 32-run windows the threshold requires at least `7/32` wins per character, and with 128-run confirmation it requires at least `26/128`. The V35 checkpoint remains compatible: model, optimizer, reservoir, auxiliary schedule and seed progression are retained while autocast changes to BF16.

Analysis: The lower gate moves the policy toward the real A0 game sooner instead of overfitting the +24 bridge. Per-character gating still prevents one strong character from carrying a disconnected one. BF16 reduces training cost without changing the model or stored public-state schema.

Conclusion: Resume the same V35 policy in BF16 with a 20% per-character promotion threshold.

Next: Verify a short resumed segment, then continue continuously and allow the first qualifying fresh gate to advance from +24 to +20.

## 2026-08-26 — V35 1.5M audit and benchmark stop

Experiment: Continue V35 through `1,507,328` decisions before applying the requested curriculum and precision change.

Results: The third block produced `304/1,398` wins (`21.75%`), mean floor `44.19`, floor-51 reach `54.01%`, and `40.26%` conversion after reaching floor 51, with one ordinary combat cap and zero empty-action states. It exceeded matched V34 on all four outcomes. Latest rolling wins were `[11,10,9,5,4]/32`.

Analysis: V35 remained healthy and improving, but the worktree then advanced to an incompatible action-set representation. Its checkpoint cannot validly initialize that new model.

Conclusion: Preserve V35 as the strongest compatible benchmark; do not force-load it into the next version.

Next: Verify the new action-set boundary and start it from random initialization in BF16 with the 20% per-character curriculum gate.

## 2026-08-26 — V37 action-set boundary

Experiment: Replace independent legal-action scoring context with a shared one-layer, four-head action-set Transformer and move current reward/shop offer identities into action rows, while retaining the shared card/entity encoders and all V35 public-state fixes.

Results: Feature version `35`/model version `37` represents legal actions plus visible unavailable shop actions, masks unavailable rows from selection, and uses a permutation-invariant action STATE summary in both value heads. VAKUU now uses shared relic action identities without redundant raw globals. The shared model has `1,796,611` parameters, 460 globals, ten card-zone summaries, nine entity summaries and one action-set summary. All `106/106` Rust tests, Python-feature checks, extension rebuild, BF16 MPS gradients, card/action permutation invariance and Rust/Python parity (`1.9e-7`) pass.

Analysis: The model remains one shared policy/value network for all five characters. The boundary is intentionally incompatible with V35 and must start from random initialization. Dense action attention is the primary runtime risk and will be monitored through slow-update action-count diagnostics.

Conclusion: V37 is the new authoritative clean boundary.

Next: Run a short BF16 smoke, then train V37 from scratch at A0/+24 with fresh seeds and a >20% fresh per-character promotion gate.

## 2026-08-26 — V37 500k audit

Experiment: Train V37 from random initialization in BF16 through `491,520` decisions at A0/+24 and compare its first matched block with V35 and V34.

Results: V37 produced `106/1,632` wins (`6.50%`), mean floor `35.66`, floor-51 reach `24.08%`, and `26.97%` conversion after reaching floor 51, with zero caps and zero empty-action states. V35 produced `7.75%`, `34.82`, `23.56%`, and `32.90%`; V34 produced `4.66%`, `34.74`, `19.27%`, and `24.19%`. Latest observed KL was `0.00144` with zero clipping.

Analysis: V37 improves early connectivity over both controls and beats authoritative V34 on wins and conversion, but trails V35's conversion. The action-set Transformer is a newly initialized decision layer and needs a matched second block before its sample efficiency can be judged. BF16 and runtime correctness are stable.

Conclusion: Keep V37 unchanged at LR `3e-4` in BF16.

Next: Continue to 1M decisions before changing the action-set architecture, optimizer, replay, or curriculum.

## 2026-08-26 — V37 1M audit

Experiment: Continue V37 unchanged through `1,015,808` decisions in BF16 with the >20% per-character promotion trigger.

Results: The second block produced `240/1,552` wins (`15.46%`), mean floor `43.75`, floor-51 reach `47.36%`, and `32.65%` conversion after reaching floor 51, with one ordinary combat cap and zero empty-action states. Its first block produced `6.50%`, `35.66`, `24.08%`, and `26.97%`. Matched V35 produced `17.21%`, `42.31`, `45.04%`, and `38.21%`.

Analysis: V37 learned substantially and now exceeds V35's connectivity, but its terminal conversion still trails. The new action-set layer needs another matched block before changing optimization. The first promotion evaluation exposed only an artifact bug: NumPy floor integers were not JSON serializable; evaluated floors are now converted to plain integers, and a real BF16 checkpoint evaluation serializes successfully.

Conclusion: Keep V37 unchanged at LR `3e-4` in BF16.

Next: Continue to 1.5M decisions and let the >20% per-character gate run when every rolling character window qualifies.

## 2026-08-26 — V37 +24 promotion

Experiment: Continue V37 until every character exceeded 20% in its rolling 32-run window, then run the independent 128-run-per-character greedy gate.

Results: The extended third block through the trigger produced `566/2,663` wins (`21.25%`), mean floor `46.00`, floor-51 reach `61.36%`, and `34.64%` conversion. Two promotion-path bugs were fixed without changing weights: evaluated floors now serialize as Python integers, and empty action-token sets bypass an MPS `index_add` bug that produced NaNs. Exact failing-seed reproduction, the BF16 probe and checkpoint evaluation pass. The fresh gate produced `200/640` wins with per-character results `[46,45,44,32,33]/128`, all above 20%, and zero caps.

Analysis: The action-set model achieved strong connectivity and met the deliberately lower per-character gate. The independent result confirms the rolling trigger was not a transient aggregate artifact. Character 3 remains weakest but still passed with 25.0% wins.

Conclusion: Promote V37 from A0/+24 to A0/+20, clear the stage-scoped winning reservoir, and retain the shared model/optimizer.

Next: Train continuously at A0/+20 and apply the same >20% fresh per-character gate before moving to +16.

## 2026-08-26 — V37 +20 benchmark stop

Experiment: Continue the promoted V37 policy at A0/+20 while independently auditing public action outcomes and learner runtime.

Results: Through `2,002,247` total decisions, the +20 stage accumulated about 164k decisions and `75/568` wins (`13.20%`), mean floor `43.33`, floor-51 reach `47.01%`, and `28.09%` conversion, with zero caps and zero empty-action states. Replay, KL and freshness remained healthy. The final learner update then ran for more than ten minutes without emitting another report. A source audit found reachable false action previews, including starter-card Dualcast reporting zero damage despite two Lightning evocations.

Analysis: The stage had too little exposure to diagnose policy adaptation, but the incorrect public derived outcomes invalidate further exploitation of this feature boundary. The abnormally long padded update also blocks the intended fast experimental cadence.

Conclusion: Preserve V37 only as a benchmark and stop at its last completed checkpoint.

Next: Correct the public preview semantics, eliminate pathological padded batches, bump the feature/model boundary, and restart one shared five-character model from random initialization.

## 2026-08-26 — V38 correctness and batching boundary

Experiment: Correct V37's reachable simulator/action-preview errors and prevent a single action-rich row from padding an entire dense action-attention batch.

Results: Feature version `36` and model version `38` fix zero-slot channeling, sequential orb overflow/lethal handling, negative-Focus orb behavior, Dualcast and orb previews, Fisticuffs `LastDamage`, Misery, block modifiers and `NO_BLOCK`, duplicate-hand exhaust/select counts, and deterministic play/draw/exhaust hooks. Fresh batches now share both a token-cost bucket and a represented-action-count bucket; compatible winning replay obeys both bounds and remains at most 10%. The V37 reservoir had represented-action counts p50 `5`, p90 `10`, p99 `25`, max `99`, so mixing the maximum with p90 rows inflated dense attention pairs about `98×`. All `113/113` Rust tests and the BF16 MPS probe pass; Rust/Python value parity error is `1.34e-7`.

Analysis: The prior stall and false derived outcomes were independent correctness and efficiency failures. The new boundary retains the same 1,796,611-parameter shared architecture while making the simulator kernel, public action features and padded compute consistent.

Conclusion: V38 supersedes V37; no checkpoint migration is valid.

Next: Start one shared five-character V38 model from random initialization at A0/+24 in BF16 and apply the >20% fresh per-character curriculum gates.

## 2026-08-26 — V38 coarse action batching

Experiment: Train the fresh V38 model through `262,144` decisions, diagnose the first composite-bucket learner block, and replace exact token/action buckets with four action-attention bands: `≤16`, `17–32`, `33–64`, and `65–128` represented rows.

Results: The initial run reached `262,144` decisions with zero caps or empty-action states, but exact composite bucketing produced 839 updates by 229k decisions, only 196 successful rows per recent update, 35.7% recent KL rejection, and 173 decisions/s. The V37 reservoir showed action counts p50 `5`, p90 `10`, p99 `25`, max `99`; mixing the maximum with ordinary rows inflated dense attention pairs about `98×`. A copied-checkpoint continuation with coarse action bands trained `8,586/8,586` rows with zero stale, ratio or KL drops, about 715 rows/update, 6.87% replay, and 503 decisions/s.

Analysis: Exact total-cost partitioning eliminated pathological padding but fragmented the common data into tiny batches, accelerating policy-version drift and wasting trajectories. Coarse action bands isolate the quadratic outliers while allowing ordinary token variation through linear/ragged paths.

Conclusion: Keep the V38 model/optimizer/reservoir and resume from the immutable 262,144 checkpoint with coarse action bands only.

Next: Continue the same V38 A0/+24 policy to a matched 500k+ block before changing its objective, optimizer, replay, or architecture.

## 2026-08-26 — V38 557k audit

Experiment: Continue the same V38 model after the 262,144-decision batching correction and compare the clean post-fix interval with matched V35/V37 windows.

Results: The whole run through `557,056` decisions produced `158/1,943` wins (`8.13%`), mean floor `34.73`, floor-51 reach `23.93%`, and `33.98%` conversion. The clean `262,144–557,056` interval produced `131/877` wins (`14.94%`), floor `40.98`, reach `40.14%`, and conversion `37.22%`, with one cap. It trained `294,404/294,805` accepted rows, averaged about 787 rows/update, dropped only 81 rows for KL, 320 for ratio and 107 for staleness, and ran around 701 decisions/s. Matched V35 was `14.13%` wins and `34.23%` conversion; matched V37 was `11.64%` and `27.72%`.

Analysis: The coarse action bands restored sample efficiency and throughput. V38 already matches or exceeds the valid controls on win rate and terminal conversion, while its modest floor deficit is consistent with a still-young policy. Character 3 is weakest in the latest 32-run slice, but one clean partial block is insufficient evidence for a targeted intervention.

Conclusion: Keep V38 unchanged at LR `3e-4` in BF16.

Next: Continue to 1M decisions before changing the architecture, objective, replay, or character balance.

## 2026-08-26 — V38 +24 promotion

Experiment: Continue V38 unchanged until every character exceeded 20% wins in its rolling 32-run window, then evaluate greedily on 128 fresh runs per character.

Results: The trigger fired at `755,347` decisions with rolling wins `[10,10,12,7,7]/32`. The disjoint fresh gate produced `313/640` wins with per-character results `[75,63,59,52,64]/128`, corresponding to `[58.6%,49.2%,46.1%,40.6%,50.0%]` before rounding from the stored exact counts. Two runs capped and counted as failures; no character was near the 20% threshold.

Analysis: V38 reached a much stronger +24 gate than required, including 40.6% for the weakest character. The corrected orb kernel, public action outcomes, BF16 learner and coarse action batching therefore support a connected five-character policy rather than an aggregate-only success.

Conclusion: Promote V38 from A0/+24 to A0/+20, retain the shared model/optimizer, and clear stage-specific winning replay.

Next: Train continuously at A0/+20 and require the same fresh per-character gate before moving to +16.

## 2026-08-26 — V38 +20 promotion

Experiment: Continue V38 at A0/+20 until the rolling per-character trigger fired, then evaluate the frozen greedy policy on 128 new runs per character.

Results: The gate at `1,183,476` decisions produced `258/640` wins. Per-character counts were `[51,59,57,42,49]/128`, or `[39.8%,46.1%,44.5%,32.8%,38.3%]` with caps counted as failures. Character 4 had the only cap; all characters exceeded 20% by a wide margin. The several-minute pause before the result was the complete gate evaluation, not a learner stall.

Analysis: V38 retained connected full-run behavior after removing four points of training bonus. Character 3 remains the weakest, but its fresh 32.8% result is well above the curriculum threshold and does not justify delaying exposure to the harder stage.

Conclusion: Promote V38 from A0/+20 to A0/+16, retain model and optimizer state, and clear stage-specific winning replay.

Next: Resume continuous training at A0/+16 with the same BF16 learner and fresh per-character promotion gate.

## 2026-08-26 — V38 +16 benchmark stop

Experiment: Continue the promoted V38 policy at A0/+16 while the worktree remained on feature 36/model 38.

Results: Through `1,609,460` total decisions, the +16 stage accumulated `203/1,183` wins (`17.16%`), mean floor `42.99`, floor-51 reach `49.11%`, `34.94%` conversion, and zero caps. The last report had rolling wins `[7,5,8,3,3]/32`. A slow-update diagnostic then exposed a JSON-only `numpy.int64` serialization error; the value was cast to a native integer and Python compilation passed. Before resume, the shared worktree advanced to incompatible feature `37`/model `39`.

Analysis: V38 remained connected and healthy at +16, but had not reached its next fresh gate. The new V39 representation changes model inputs and architecture, so loading the V38 checkpoint would be invalid regardless of the diagnostic fix.

Conclusion: Preserve V38 as a benchmark and do not migrate its weights into V39.

Next: Audit V39 end to end, then start a new shared five-character model from random initialization if the boundary is valid.

## 2026-08-26 — V39 unified-state 100k pilot

Experiment: Start a fresh shared V39 policy with unified typed state and candidate-action Transformers, BF16, four CPU samplers and batch 256; switch the MPS state path from dense padded attention to the ragged kernel at 65,536 decisions.

Results: The pilot completed `100,598` decisions with `3/464` wins, mean floor `26.12`, floor-51 reach `2.37%`, zero caps and zero empty-action states. Dense pre-fix windows ran at `71–82` decisions/s and retained roughly 50 GiB of Metal allocations. Ragged post-fix windows ran at `176–211` decisions/s with process memory around 3–5 GiB. The final session attempted 35,062 rows but trained 17,871; 16,945 were rejected by the KL gate, while stale and ratio drops were negligible. Cleaning redundant numbered/superseded checkpoints recovered 19.5 GiB directly and allowed APFS to report 152 GiB free while preserving every `latest.pt` and referenced best checkpoint.

Analysis: Ragged attention resolves the architecture's padding/memory failure and more than doubles throughput. Batch 256 now becomes the limiting factor: many small optimizer steps advance the learner faster than actors refresh, wasting nearly half the complete-trajectory data through KL rejection.

Conclusion: V39 is mechanically trainable, but batch 256 is not an efficient long-run configuration.

Next: Test the ragged model with a larger batch to reduce policy-version churn and KL rejection before launching the long curriculum run.

## 2026-08-26 — V40 leak-free ragged boundary

Experiment: Remove private card-instance dependence from queued continuation hashes, bump to feature `38`/model `40`, and test the unified architecture with ragged MPS attention, four samplers and batch 1024.

Results: Pending effects now hash only normalized effects; semantic context and queued/active cards remain explicit public tokens. A reachable full-observation renumbering regression passes, as do all `114/114` Rust tests. The rebuilt V40 MPS probe reports `667,395` shared parameters, state/action permutation errors around `3e-8`, Rust parity `8.94e-8`, and finite BF16 updates. The batch-1024 functional smoke trained all `8,583` accepted rows in 11 updates with zero stale, ratio or KL drops, lag `3.14`, KL `0.000759`, and `234.3` decisions/s. Peak RSS was `1.57` GB and peak unified/Metal footprint `12.64` GB.

Analysis: V40 removes the last confirmed private-instance leak. Ragged attention fixes V39's dense-padding memory failure, while batch 1024 reduces optimizer-version churn and eliminates the 48% KL rejection observed with batch 256.

Conclusion: V40 is the first valid unified-transformer training boundary; V39 remains a mechanics/performance benchmark only.

Next: Run a fresh 100k A0/+24 V40 pilot with batch 1024 and four samplers, then decide whether to extend unchanged.

## 2026-08-26 — V39 unified decision representation

Experiment: Replace the dense V38 observation with typed state tokens, explicit candidate and legality masks, bounded continuation frames, and one state Transformer followed by one decision Transformer over the state and every candidate action.

Results: Feature 37/model 39 has 667,395 parameters. The probe observed state shape `389 × 32`, three candidates with independent presence and legality masks, state permutation error `1.86e-8`, action permutation error `1.68e-8`, and Rust export parity error `8.94e-8`. All 113 Rust tests passed, including unavailable shop offers, pending choices, continuation order, resume payloads, uncapped continuations, and action-token alignment.

Analysis: The policy now receives exactly the public decision boundary: typed state, typed pending context, and the complete candidate set. Automatic resolution remains internal; queued effects are exposed only when a player choice blocks resolution. Offer-specific state zones and the legal-prefix convention are gone. Existing deterministic card-effect previews remain as derived candidate features, but no longer require the legacy dense action vector.

Conclusion: Accept V39 as the next incompatible representation generation and initialize it from random weights.

Next: Start shared five-character V39 training and compare sample efficiency with the preserved V38 benchmark.

## 2026-08-26 — V40 batch-1024 pilot

Experiment: Train the first leak-free V40 unified model from random initialization at A0/+24 for 100k decisions, using BF16, four CPU samplers, ragged state/action attention, and batch 1024.

Results: The pilot completed `100,408` decisions in `496.2` seconds. It trained `100,407/100,408` rows with one ratio rejection and zero stale or KL rejections, made 128 optimizer updates, and reached two wins in 478 completed episodes. Mean terminal floor was `25.49`; eight episodes reached floor 51. The final observed KL was `0.000828`, cumulative clipping was `1.10%`, mean policy lag was `12.56`, and there were zero caps or empty-action states. Process memory stayed around 3–4 GB rather than V39's dense-path 50 GiB allocation.

Analysis: Batch 1024 fixes V39 batch-256 policy churn: essentially every complete-trajectory row is used while throughput remains about 200 decisions/s. The two wins also populate a small self-generated winning reservoir without destabilizing replay. At only 100k decisions, policy quality is still a bootstrap signal rather than a useful architecture verdict.

Conclusion: Accept the V40 learner configuration and continue the same model, optimizer, critic and reservoir unchanged.

Next: Extend V40 to 500k total decisions and reassess learning slope, character balance, KL, replay and throughput before changing architecture or optimization.

## 2026-08-26 — V40 500k continuation

Experiment: Continue the accepted V40 checkpoint unchanged from `100,408` to `501,188` decisions at A0/+24, retaining the optimizer, progress critic and self-generated winning reservoir.

Results: The full run produced `104/1,774` wins (`5.86%`), mean floor `34.32`, floor-51 reach `18.83%`, and `31.14%` conversion, with zero caps and zero empty-action states. Excluding the 100k bootstrap, it produced `102/1,296` wins (`7.87%`) and floor `37.58`. The later `297k–494k` block accelerated to `66/583` wins (`11.32%`), floor `40.47`, floor-51 reach `33.62%`, and `33.67%` conversion. The learner trained `488,226/501,015` attempted rows (`97.45%`); KL, ratio and stale rejection were `2.36%`, `0.19%`, and `0.035%`. Replay remained about `8.1%`, every character was represented in its full reservoir, and training consumed `84.8%` of measured training-plus-validation time.

Analysis: V40 trails some valid prior boundaries over the entire bootstrap, but its clean late block exceeds their first-500k aggregate win rates and is still accelerating. The unified model therefore needs more exposure rather than an optimizer or objective change. Rolling wins `[5,5,8,4,3]/32` remain below the required seven for characters 0, 1, 3 and 4, so a fresh promotion gate is premature.

Conclusion: Keep the same V40 model, optimizer, replay, BF16 precision and asynchronous sampling configuration.

Next: Continue V40 to one million total decisions and require the existing fresh per-character >20% gate before promotion.

## 2026-08-26 — V40 optimizer overshoot diagnosis

Experiment: Continue V40 beyond 500k, stop after an abrupt all-character training-window collapse, and compare the last healthy and collapsed checkpoints greedily on the same 32 fresh seeds per character.

Results: Checkpoint `730,564` scored `68/160` greedy wins with per-character counts `[13,17,17,7,14]`. Checkpoint `796,100` scored only `33/160`, with `[5,15,7,2,4]`. Before the collapse, observed KL spiked to `0.01667`, clip fraction reached `16.3%`, and `12,218/32,768` rows were rejected; the next training window fell to `0/183` wins and mean floor `26.16`. Across 512 fixed public winning states, the two checkpoints differed by mean categorical KL `0.0830` with `21.3%` greedy-action flips. Entropy, progress beta and bounded replay remained healthy.

Analysis: The paired greedy result proves a real policy regression rather than noisy exploratory reporting. The sequence and parameter drift identify an accepted optimizer step overshoot; the pre-update KL filter can reject later stale rows but cannot undo the update that caused the jump.

Conclusion: Reject the collapsed continuation and preserve checkpoint `730,564` as the recovery source.

Next: Run exactly 131,072 fresh decisions from `730,564` at learning rate `1.5e-4`, with every other setting unchanged; require all observed-KL reports to remain below `0.008` and retain the 40–44 floor regime before extending.

## 2026-08-26 — V40 hard trust region and +24 promotion

Experiment: Compare a half-rate continuation with a hard post-update trust region, both starting from immutable checkpoint `730,564`, then run the predeclared 128-seed-per-character promotion gate on the stable source policy.

Results: LR `1.5e-4` merely delayed the failure: its third window reached observed KL `0.00873`, rejected `16,366` rows afterward, and collapsed to `0/151` wins with mean floor `31.75`. At LR `3e-4`, post-step rollback restored model and Adam state whenever realized KL exceeded `0.008`. It rejected `18,560` rows across 132k attempted decisions but retained window floors `39.95`, `44.78`, `42.89`, and `39.44`; at the exact prior failure point it scored `8/98` instead of `0/151`. The trust branch did not improve greedy conversion: its best checkpoint scored `53/160` versus `68/160` for the source. The source then passed the fresh promotion gate with `234/640` wins and per-character counts `[46,66,46,33,43]/128`, one cap, and all characters above 20%.

Analysis: Learning-rate reduction does not bound an Adam update. A post-update rollback is necessary and successfully prevents catastrophic actor publication, but bounded local drift alone does not guarantee better terminal conversion. The stable source policy already clears the user-requested fresh greedy threshold, so further +24 continuation is unnecessary.

Conclusion: Keep the post-update trust-region mechanism, promote the original `730,564` checkpoint to A0/+20, clear stage-specific winning replay, and disable the completed progress curriculum.

Next: Train continuously at A0/+20 from the promoted stable checkpoint, retaining LR `3e-4`, BF16, batch 1024, four samplers, and the hard KL rollback.

## 2026-08-26 — V40 zero-shot +20 promotion

Experiment: Test whether the stable A0/+24 policy already satisfies A0/+20 before committing to harder-stage updates; compare this with two guarded +20 training windows.

Results: Guarded +20 training regressed immediately from `2/115` wins and floor `37.17` to `1/126` and floor `34.52`, despite post-step rollback. The unmodified checkpoint scored `46/160` on diagnostic +20 seeds with every character above 20%, then passed the official fresh gate at `188/640` wins. Per-character gate counts were `[43,49,32,27,37]/128`; all exceeded 20%, with zero caps.

Analysis: The stable policy already solved +20. Training before gating was unnecessary and harmful, even with bounded local updates. Difficulty gates should therefore be attempted before stage-specific training whenever the preceding-stage checkpoint has plausible transfer.

Conclusion: Reject the +20 training branch and promote the unmodified stable checkpoint directly to A0/+16.

Next: A diagnostic +16 gate produced `[3,12,5,3,6]/32`, so train at +16 from the stable source with empty stage replay and hard post-update KL rollback.

## 2026-08-26 — V40 +16 guarded adaptation

Experiment: Train from the stable `730,564` checkpoint at A0/+16 with BF16, batch 1024, four samplers, LR `3e-4`, empty stage replay, no progress auxiliary, and hard post-update KL rollback; then compare the branch best against its source on the same 32 seeds per character.

Results: Three 32,768-decision windows produced `3/378` sampled wins with mean floors `34.17`, `34.30`, and `33.04`, zero caps, and `19,574/98,304` rows rejected by pre- or post-update KL checks. The branch best scored `22/160` greedy wins at +16 with per-character counts `[4,7,3,3,5]`; the source scored `29/160` on the identical seeds with `[3,12,5,3,6]`.

Analysis: The trust region prevented a catastrophic jump but did not produce a positive learning slope. The candidate raised several character floor means while losing seven terminal wins, especially on characters 1 and 2, so floor connectivity did not convert into better play.

Conclusion: Reject the +16 branch and preserve the unmodified `730,564` source.

Next: Change one collection or optimization variable that addresses sparse long-horizon winning trajectories, then rerun a bounded +16 adaptation from the same source.

## 2026-08-26 — V40 +16 lower-temperature collection

Experiment: Repeat the +16 adaptation from the same stable source for 131,072 decisions, changing only policy temperature from `1.0` to `0.5` consistently in collection, PPO, KL, entropy, and winning replay.

Results: Four complete windows produced `8/481` sampled wins and mean floor `35.09`, versus `3/378` and `33.83` in the temperature-1 branch. Only `69,398/131,967` attempted rows trained; pre-KL, post-KL, and ratio rejection removed `16,510`, `43,194`, and `2,865` rows. There were zero caps or empty-action states. The planned greedy comparison became invalid when the shared tree advanced to incompatible feature `39`/model `41` before evaluation.

Analysis: Lower temperature improved sampled wins and floor, supporting excessive exploration as part of the sparse-win problem, but doubled sensitivity to policy movement and cut row utilization to `52.6%`. The result is directional rather than sufficient for adoption because no compatible paired greedy gate remains possible.

Conclusion: Preserve policy temperature as an explicit training parameter with default `1.0`; do not migrate V40 weights or make `0.5` the V41 default.

Next: Validate V41, compare width-heavy and depth-heavy models near two million parameters, then start fresh continuous training with the faster configuration.

## 2026-08-26 — V41 compact representation and 2M scaling choice

Experiment: Validate feature `39`/model `41`, then compare matched BF16 models near two million parameters: width-heavy `W192/L2/H6/FF384` and depth-heavy `W128/L9/H4/FF256`, each trained for 8,192 fresh A0/+24 decisions with identical seeds and optimizer settings.

Results: A mixed active/inactive sampler smoke exposed and fixed short inactive metadata rows; a second smoke exposed and fixed BF16-to-FP32 sparse encoder writes. The final Rust suite passed `115/115` and the Python probe passed with permutation errors below `3e-8` and Rust parity `5.96e-8`. The width-heavy model has `2,001,219` parameters and trained at `368.5` decisions/s; the depth-heavy model has `1,966,595` parameters and trained at `285.0` decisions/s. Both used every attempted row with no caps, stale, ratio, or KL drops. Their mean floors were `17.38` and `17.82`; neither produced a win in this short bootstrap.

Analysis: Width is about `29%` faster at the same parameter budget, while the deep model's observed KL was nearly zero at the same learning rate. The width-heavy configuration therefore offers faster iteration and stronger early policy movement without changing the shared-model design.

Conclusion: Use `W192/L2/H6/FF384` as the V41 production architecture, BF16 forward with FP32 parameters and optimizer.

Next: Start fresh continuous V41 training at A0/+24 with batch 1024, four samplers, LR `3e-4`, policy temperature `1.0`, and fresh per-character promotion above 20%.

## 2026-08-26 — V41 continuous training start

Experiment: Start the selected `W192/L2/H6/FF384` shared model from random weights at A0/+24 with BF16, batch 1024, four samplers, LR `3e-4`, policy temperature `1.0`, and automatic fresh promotion gates above 20% per character.

Results: Through `65,536` decisions, mean terminal floor rose from `21.32` to `27.60`; the second window produced the first win. Throughput rose from `356.8` to `440.4` decisions/s, all `65,536` attempted rows trained, observed KL remained `0.00083–0.00233`, and there were zero caps, empty-action states, or freshness drops.

Analysis: The corrected compact representation and width-heavy model are stable under asynchronous BF16 training. Early progression is improving without sacrificing row utilization or triggering the hard trust-region guard.

Conclusion: Continue V41 unchanged.

Next: Let the continuous run accumulate enough decisions for win conversion and per-character promotion evidence; do not tune from two bootstrap windows.

## 2026-08-26 — V41 representation audit stop

Experiment: Continue V41 while independently auditing the compact state and candidate encodings against transition behavior.

Results: Training reached `229,376` decisions with window floor rising to `38.73`, `7/96` wins, more than 99% row utilization, and zero caps. The audit then found exact collisions: structurally identical Event options lacked their visible index, Tinker Time offer identities were absent, and queued AutoPlay card tokens did not join their continuation effects because their positions used incompatible orderings.

Analysis: The improving metrics do not validate a representation that maps different legal transitions to identical inputs. The checkpoint is useful only as a speed/learning benchmark.

Conclusion: Stop V41 and fix all three collisions under a new incompatible boundary.

Next: Start V42 from random weights after focused transition regressions and full Rust/Python verification.

## 2026-08-26 — V42 corrected compact boundary

Experiment: Encode every Event action index, attach visible Tinker offer identity to state and candidates, and align queued AutoPlay semantic-card positions with continuation effects.

Results: Feature `40`/model `42` passes `117/117` Rust tests, including duplicate-event, swapped-Tinker-offer, and queued-AutoPlay regressions. The rebuilt BF16 probe passes with the selected `2,001,219`-parameter `W192/L2/H6/FF384` model, state/action permutation errors below `2e-8`, and Rust parity `5.96e-8`.

Analysis: V42 removes the concrete V41 action/state collisions without changing the chosen model scale or training algorithm.

Conclusion: V42 is the next valid clean-training boundary; V41 remains benchmark-only.

Next: Train V42 continuously from random weights at A0/+24 with fresh seeds and promote only through fresh per-character gates above 20%.

## 2026-08-26 — V43 token-only cleanup boundary

Experiment: Remove the obsolete dense observation API and layout offsets, export named token routes, rename power decay state, and replace hashed continuation payloads with exact typed payload bytes folded into one effect representation.

Results: Feature `41`/model `43` is intentionally checkpoint-incompatible. All `117/117` Rust tests pass; the BF16 MPS probe passes with state/action permutation errors below `2.1e-8` and Rust parity `5.96e-8`. The bridge builds and loads a packaged V43 checkpoint through the packed token path.

Analysis: The cleanup removes deprecated parallel representations and makes the token contract the sole training and export path without changing its semantics.

Conclusion: V43 is the next clean-training boundary; V42 processes already in memory may finish unchanged, while newly started processes use V43 automatically.

Next: Start fresh V43 training from random weights; existing V42 processes remain valid only within their already-loaded code.

## 2026-08-26 — V41 compact semantic state

Experiment: Replace the flat unified state with semantic summary tokens, per-relic state, per-power amount/value embeddings, folded enemy history and effect context, and a dedicated one-layer map encoder.

Results: Feature `39`/model `41` reduces the probe's main state-attention length to `36–38` tokens from `139–163` raw tokens. Remaining relics, events and encounters each become one learned summary token; duplicate maps are encoded once per batch and cached across CPU inference calls. All `114/114` Rust tests pass, BF16 MPS forward/backward is finite, and Rust/Python value parity error is `5.96e-8`.

Analysis: The compact semantic state substantially reduces attention cost, but it still requires a transition-level completeness audit before long training.

Conclusion: V41 is the next incompatible training boundary and must start from random weights.

Next: Run the compact-boundary state/action collision audit before treating V41 metrics as valid evidence.

## 2026-08-26 — V42 491k benchmark

Experiment: Train the corrected `W192/L2/H6/FF384` BF16 model continuously from scratch at A0/+24 through 491,520 decisions.

Results: Successive 163,840-decision blocks improved from `6/677` wins and floor `28.21`, to `34/461` and `37.24`, then `46/452` and `40.22`. Overall results were `86/1,590` wins, floor `34.24`, floor-51 reach `17.80%`, and conversion `30.39%`. The learner trained `486,493/490,929` attempted rows, used `81.5%` of measured collect/update compute for updates, and recorded one cap, no empty-action failures, and observed KL `0.00421`.

Analysis: V42 learned monotonically and slightly exceeded matched V40, but no character met the all-character promotion condition yet. Feature `41`/model `43` became the source-of-truth boundary at the audit point, so further V42 compute would produce an unresumable checkpoint.

Conclusion: Keep one V42 checkpoint as a benchmark and restart V43 from random weights.

Next: Run V43 with the same architecture, optimizer, BF16, async samplers, and promotion settings; reassess after a matched 491,520 decisions.

## 2026-08-26 — V43 architecture compliance stop

Experiment: Start feature `41`/model `43` with the compact two-million-parameter architecture, then audit it against the full requested representation before committing a long run.

Results: At `65,536` decisions it trained `65,535/65,536` rows, reached mean floor `24.00`, observed KL `0.00425`, and had no caps or empty-action failures. The audit found that it used one global W192/H6 state Transformer, dense learned metadata summaries, and a 192-dimensional action-set Transformer instead of separate shared W64/H4 card-zone summaries, direct globals, and 64-dimensional state/action scoring. Visible offer cards were incomplete and float16 storage could alias raw flag bitmasks.

Analysis: Training mechanics were healthy, but the checkpoint could not become the requested model through continued optimization or export.

Conclusion: Stop V43 and retain its `65,536` checkpoint only as a systems benchmark.

Next: Build feature `42`/model `44` with one lossless token per visible card occurrence, 13 shared card zones, tokenized sparse collections, direct public globals, and the exact W64/H4 architecture; restart from random weights.

## 2026-08-26 — V44 requested architecture restored

Experiment: Replace the compact global Transformer with the requested per-zone architecture and make card/state encoding lossless before restarting training.

Results: Feature `42`/model `44` uses one 34-value token per visible card occurrence, including categorical raw and turn flags; 13 card zones share one W64/L2/H4/FF128 Transformer with distinct learned state tokens. Ten non-card collections are independently pooled, map tokens have a dedicated summary, actions are 64-dimensional, and policy/value heads consume the direct 725-float public vector plus collection summaries without a fusion Transformer. The model has `2,003,380` parameters. Rust passed `117/117`; BF16 probe passed with card-zone permutation error `2.61e-8` and Rust value parity below `4e-7`; an 8,919-decision async smoke trained every row with no caps, empty actions, or KL rejection.

Analysis: The old ~3,100 direct-feature estimate included sparse map/set fields that are now variable tokens; retaining them would duplicate the requested tokenized representation. The nonduplicated direct remainder is 725 normalized values. V44 now matches the semantic architecture while retaining the verified async PPO system.

Conclusion: V44 is the first clean training boundary for the requested model.

Next: Train V44 continuously from random initialization at A0/+24 and reassess at 491,520 decisions before changing any optimizer or objective setting.

## 2026-08-26 — V44 491k architecture validation

Experiment: Train the requested two-million-parameter per-zone W64 BF16 model continuously from scratch at A0/+24 through 491,520 decisions.

Results: Successive 163,840-decision blocks improved from `12/730` wins and floor `27.20`, to `37/473` and `39.34`, then `59/451` and `41.84`. Overall results were `108/1,654` wins, floor `34.66`, floor-51 reach `20.62%`, and conversion `31.67%`, with zero caps. The learner trained `491,274/491,346` attempted rows, rejected no step for KL, and ended with observed KL `0.00049`. Latest per-character rolling wins were `[7,5,7,3,4]/32`.

Analysis: V44 beats leak-free V42 and V40 at the same budget while preserving essentially complete row utilization and stable optimization. C3 is the weakest win converter but reaches floor `41.28`, so the current evidence favors continued shared training over a targeted intervention.

Conclusion: Keep the architecture, optimizer, objective, and replay unchanged.

Next: Continue V44 to 1,000,000 decisions, then reassess promotion readiness and per-character conversion.

## 2026-08-26 — V44 state-completeness stop

Experiment: Continue V44 while independently auditing the production state/value input against visible choice transitions.

Results: V44 reached `557,056` decisions; its final complete window produced `15/97` wins at floor `40.61`, with zero caps or empty-action failures. The audit reproduced a state collision: changing a visible dynamic Event page or relic offer changed the legal transitions but not the state/value input because those identities existed only on action rows.

Analysis: Action-conditioned policy scores could distinguish these choices, but the public win/progress critics could not distinguish visibly different menus. More V44 training would optimize an incomplete value state.

Conclusion: Stop V44 and retain only its selected `524,288`-decision checkpoint as a benchmark.

Next: Add current Event options and visible offer payloads to state, restore the requested approximately 3,100 direct globals, bump the incompatible boundary, and restart from random weights.

## 2026-08-26 — V45 complete public choice state

Experiment: Add visible dynamic choice payloads to state and restore the direct public map grid while preserving the per-zone W64 architecture.

Results: Feature `43`/model `45` encodes every current Event option, ancient relic offer, paired Relic Trader offer, and Toy Box provenance on the state side. Direct globals are `3,605`: five character indicators, 24 normalized singleton-state blocks, and the `18×8×20` public map grid. The model remains near two million parameters (`2,004,787`) with state width `5,141` and head width `112`. Rust passed `120/120`; the BF16 probe passed with state/action permutation errors below `1.4e-8` and Rust parity `2.38e-7`. An 8,489-decision async smoke trained every accepted row with no caps, empty actions, freshness drops, or KL rejection.

Analysis: V45 removes the remaining reproduced state/value collision and now satisfies the literal direct-global scale without removing variable map-node tokens. The smaller automatically tuned policy/value hidden width keeps iteration cost near the requested two-million-parameter budget.

Conclusion: V45 is the next clean training boundary; V44 remains benchmark-only.

Next: Start fresh continuous V45 training at A0/+24 with the validated BF16 async PPO configuration and reassess at 491,520 decisions.

## 2026-08-26 — V45 491k validation

Experiment: Train the complete-public-state V45 model continuously from scratch at A0/+24 through 491,520 decisions.

Results: Three successive 163,840-decision blocks improved from `6/785` wins and floor `25.96`, to `34/527` and `37.60`, then `66/445` and `41.71`. The latest block reached floor 51 in `41.12%` of runs and converted `36.07%` of those reaches into wins. The learner trained `491,121/491,141` attempted rows, with no KL rollback, caps, or empty-action failures. Latest rolling wins were `[6,5,6,4,8]/32`.

Analysis: V45 had a slower initial block than V44, but its latest equal block exceeds V44 on win rate, floor-51 reach, and terminal conversion. C3 remains weakest but is connected and receives balanced replay; optimization is exceptionally clean.

Conclusion: Keep V45 architecture, optimizer, objective, and replay unchanged.

Next: Continue V45 to 1,000,000 decisions before attempting promotion or targeted changes.

## 2026-08-27 — V45 one-million-decision audit

Experiment: Continue V45 unchanged through two matched 491,520-decision blocks, resuming only from immutable periodic checkpoints after terminal-session interruptions.

Results: The first block produced `106/1,757` wins (`6.03%`), floor `33.44`, and `18.90%` floor-51 reach. The second produced `185/1,378` wins (`13.43%`), floor `42.02`, and `41.36%` reach. Its successive 163,840-decision thirds were `10.94%`, `14.91%`, and `14.47%` wins while floor rose `40.78→42.45→42.86`. All resumed sessions trained essentially every attempted row, with no KL rollback or caps. Latest rolling wins were `[7,7,5,5,2]/32` and floors `[45.25,42.47,43.69,44.09,42.44]`.

Analysis: Win rate more than doubled and connectivity continues improving. Replay sampling remains balanced even though C4 is the current conversion bottleneck. Low KL and negligible rejection show no optimizer instability.

Conclusion: Continue unchanged; one weak C4 rolling window is insufficient evidence for a targeted intervention.

Next: Train to 1,474,560 decisions, unless the fresh automatic promotion gate triggers first.

## 2026-08-27 — V45 1.475M audit

Experiment: Continue V45 unchanged through a third matched 491,520-decision block.

Results: Equal blocks improved monotonically: `6.03%` wins/floor `33.44`, then `13.43%`/`42.02`, then `16.94%`/`42.97`. The third block reached floor 51 in `47.29%` of runs and converted `35.83%` of those reaches into wins; its final two thirds held near `18%` wins while reach rose to `56.78%`. Across all sessions, `1,474,131/1,474,155` attempted rows trained, with zero KL rollback, caps, or empty actions. Latest rolling wins were `[3,7,8,2,8]/32`.

Analysis: Both connectivity and terminal conversion continue improving. C0 and C3 block promotion but remain well connected, and character-balanced replay samples them normally despite fewer stored C3 wins.

Conclusion: Keep V45 unchanged; no targeted replay or optimizer change is supported.

Next: Continue to 1,966,080 decisions, allowing the automatic fresh promotion gate to run earlier only if every character exceeds 20%.

## 2026-08-27 — V45 1.966M audit

Experiment: Continue V45 unchanged through a fourth matched 491,520-decision block.

Results: The four blocks progressed from `6.03%` to `13.43%`, `16.94%`, and `21.21%` wins. Mean floor rose `33.44→42.02→42.97→44.54`; floor-51 reach rose `18.90%→41.36%→47.28%→54.79%`; conversion rose `31.93%→32.46%→35.83%→38.72%`. Latest rolling wins were `[7,17,7,4,5]/32`, with zero caps or empty actions. Across all sessions, `1,965,650/1,965,675` attempted rows trained and no KL step was rejected.

Analysis: Aggregate performance now exceeds 20%, but promotion correctly remains blocked because C3 and C4 are below the per-character threshold. Both reach deep floors, making terminal conversion—not connectivity or replay starvation—the remaining limitation.

Conclusion: Continue the shared policy unchanged; targeted changes remain premature while every matched block improves.

Next: Train to 2,457,600 decisions, allowing the automatic fresh promotion gate to run earlier if all five rolling windows exceed 20%.

## 2026-08-27 — V47 compact pools and fast process sampling

Experiment: Continue the V45 policy through the compatible V47 compact-pool migration, using four process samplers, 512 environments, FP16, batch 2,048, and publish-every-32 updates.

Results: Excluding the first startup-censored window after each sampler restart, decisions `2,162,688–2,457,600` produced `218/876` wins (`24.89%`), mean floor `46.21`, floor-51 reach `61.76%`, and `40.30%` conversion. The learner trained `352,622/352,623` attempted rows, with one ratio drop, no KL rollback, caps, or empty actions. The latest full window ran at `1,918` decisions/s; the validated optimization benchmark averaged `2,116` across its final two windows. Latest rolling wins were `[12,8,8,6,6]/32`.

Analysis: Compact set storage and process-isolated actors materially improve throughput without degrading steady-state outcomes. Restart windows are biased toward short deaths because successful trajectories complete later and must not be compared directly with steady-state windows. C3 and C4 are each one win short of the promotion trigger; replay remains balanced.

Conclusion: Keep the model, optimizer, objective, replay, and sampler configuration unchanged.

Next: Continue to `2,621,440` decisions, allowing the automatic fresh promotion gate to run sooner only if every character exceeds 20%.

## 2026-08-27 — V47 A0/+24 promotion

Experiment: Let the automatic trigger run the declared fresh 128-seed greedy gate for every character after all rolling windows crossed 20%.

Results: The gate passed with `316/640` wins and zero caps. Per-character wins were `[70,59,70,55,62]/128`, or `42.97–54.69%`; mean floors were `[49.50,48.41,49.65,48.48,48.27]`.

Analysis: Every character clears the strict `>20%` criterion by a wide margin, so the rolling-window trigger was conservative rather than a false positive.

Conclusion: Promote from A0/+24 to A0/+20 and clear the stage-scoped winning reservoir.

Next: Continue uninterrupted at A0/+20 and treat its first report as startup-censored rather than steady-state evaluation.

## 2026-08-27 — V47 A0/+20 benchmark and stop

Experiment: Continue the promoted V47 policy at A0/+20 while auditing its compact pool representation against the requested V45 architecture.

Results: Excluding the startup-censored first report, the first complete 491,520-decision block produced `197/1,399` wins (`14.08%`), floor `42.51`, floor-51 reach `44.10%`, and `31.93%` conversion, with no caps or empty actions. The learner trained every attempted row and sustained about `1,950` decisions/s. The audit found that separately summing pool ID and kind embeddings aliases transition-relevant relic, event, and encounter assignments. It also confirmed that V47 had reduced the requested model from `3,605` direct globals and `2,004,787` parameters to `725` globals and `1,102,387` parameters.

Analysis: V47 is a useful throughput benchmark, but both the public-state collision and the unintended architecture reduction invalidate it as the production continuation.

Conclusion: Stop V47 at decision `3,193,910`; preserve its reports and latest checkpoint only as a benchmark.

Next: Restore the exact V45 state architecture under model version 48 while retaining only lossless compact transport and process-sampler optimizations.

## 2026-08-27 — V48 architecture-preserving transport

Experiment: Restore the V45 model shape and public state while retaining V47's compact queue rows, process-isolated samplers, and FP16 execution path.

Results: Feature `43`/model `48` again uses `3,605` direct globals, state width `5,141`, and `2,004,787` parameters. Relic, event, and encounter items use the normal nonlinear per-token encoder before collection pooling; compact queue rows reconstruct those exact tokens before training. Rust passed `123/123` tests. The MPS probe passed with state, zone, and action permutation errors below `2.3e-8` and Rust parity `2.38e-7`. A warm-resume smoke from the immutable V45 `1,048,576` checkpoint restored the optimizer and reservoir, trained `8,486/8,486` attempted rows with no freshness, ratio, or KL drops, and had no caps or empty actions.

Analysis: V48 preserves the selected architecture and V45 learning state. The compact representation is now transport-only rather than a learned architectural shortcut.

Conclusion: V48 is the valid production boundary; V47 remains benchmark-only.

Next: Continue V48 from the V45 `1,048,576` checkpoint at A0/+24 with the validated 512-environment, four-process FP16 sampler configuration.

## 2026-08-27 — V48 first matched continuation block

Experiment: Continue the restored V45 architecture under the V48 transport/runtime boundary and measure a complete uncensored 491,520-decision block.

Results: Six consecutive uncensored 491,520-decision blocks reached win rates of `15.02%`, `15.99%`, `19.56%`, `19.91%`, `17.77%`, and `22.46%`. Mean floor progressed from `43.01` to `44.42`, floor-51 reach from `45.57%` to `54.20%`, and conversion from `32.97%` to `41.44%`. The sixth block's thirds were uniformly strong at `22.71%`, `22.59%`, and `22.10%` wins. The learner recorded no pre-update or post-update KL rejection, zero empty actions, and two ordinary caps across these blocks; recent windows run around `1,400–1,600` decisions/s.

Analysis: The architecture restoration did not erase the valid V45 learning signal. Connectivity, conversion, and aggregate wins improved across matched blocks while optimizer and replay behavior remained controlled. The first report after each process restart is excluded because short failures complete before surviving trajectories.

Conclusion: Continue V48 unchanged; neither promotion nor an optimizer/objective intervention is supported yet.

Next: Accumulate another matched block and allow the existing fresh per-character promotion trigger to run only when every rolling window exceeds 20%.

## 2026-08-27 — V48 five-million-decision audit

Experiment: Continue V48 unchanged through seven complete uncensored blocks and a partial eighth block at A0/+24.

Results: Blocks six and seven held `22.46%` and `22.59%` wins with floors `44.42` and `44.62`. The partial eighth block produced `249/1,232` wins (`20.21%`), floor `44.60`, and `54.87%` floor-51 reach; its successive chunks improved `19.24%→20.61%→20.97%`. At decision `5,000,805`, rolling wins were `[11,9,7,4,3]/32`. The learner trained `2,959,140/2,959,141` attempted rows with one ratio drop, no KL rollback, caps, or empty actions.

Analysis: The late decline is within sampling noise and is not accelerating. C3 and C4 remain terminal-conversion bottlenecks, while beta has only just reached its `0.1` floor and has not yet received a complete fixed-beta training block.

Conclusion: Preserve the objective and optimizer; collect one matched fixed-beta block before testing a character-targeted intervention.

Next: Resume the verified latest checkpoint for `524,288` decisions, exclude the first startup-censored report, and reassess the following `491,520` decisions.

## 2026-08-27 — V48 A0/+24 promotion

Experiment: Run the declared fresh 128-seed greedy gate after every character crossed the rolling `>20%` trigger.

Results: The gate passed with `263/640` wins and zero caps. Per-character wins were `[53,41,71,48,50]/128`, or `32.0–55.5%`; mean floors were `[49.32,48.05,49.80,48.17,48.27]`.

Analysis: The weaker rolling C3/C4 estimates were sampling noise rather than a persistent failure. Every character clears the promotion threshold by at least 12 percentage points.

Conclusion: Promote the validated checkpoint from A0/+24 to A0/+20 and reset stage-local replay.

Next: Continue uninterrupted at A0/+20, excluding its first startup-censored report from steady-state comparisons.

## 2026-08-27 — V48 first A0/+20 block

Experiment: Train the promoted checkpoint for one exact 491,520-decision block at A0/+20, excluding the first report after each sampler restart.

Results: The block produced `204/1,344` wins (`15.18%`), floor `43.72`, floor-51 reach `47.69%`, and `31.83%` conversion, with zero caps or empty actions. Its five-report thirds were `19.91%`, `11.43%`, and `14.55%` wins; the final third restored floor and reach to `44.73` and `53.99%` but converted only `26.96%`. All `229,376` rows in the final session were trained with no freshness, ratio, or KL drops. Latest rolling wins were `[5,4,2,4,5]/32`.

Analysis: The harder stage preserves late-run connectivity but has not stabilized terminal conversion. The result is non-monotonic rather than a continuing collapse, and the stage-local winning reservoir is still only `1,728/4,096` rows despite balanced replay sampling.

Conclusion: Do not promote or change the optimizer from this single mixed block.

Next: Use the next measured block to determine whether conversion recovers as the stage-local reservoir fills; isolate any intervention to the demonstrated bottleneck.

## 2026-08-27 — V48 second A0/+20 block

Experiment: Continue A0/+20 unchanged for a second exact 491,520-decision block while the stage-local winning reservoir matures.

Results: The block produced `232/1,329` wins (`17.46%`), floor `44.07`, floor-51 reach `52.45%`, and `33.29%` conversion, improving every aggregate measure over the first block. Its thirds were `21.79%`, `14.16%`, and `16.55%` wins. The learner trained `507,923/507,926` attempted rows with three ratio drops, no KL rollback, caps, or empty actions. The reservoir reached `3,590/4,096`, with character rows `[824,847,839,408,672]`; latest rolling wins were `[9,5,13,0,4]/32`.

Analysis: Aggregate connectivity and conversion improved, but C3 is now a repeated character-specific conversion failure despite reaching late floors. Balanced replay sampling prevents simple starvation, though C3 has roughly half the stored winning examples of C0–C2.

Conclusion: Keep the optimizer and global objective fixed; decide the next block solely on whether a character-targeted replay adjustment is supported.

Next: Compare unchanged continuation against a C3-focused replay intervention only if the independent audit confirms the imbalance is actionable.

## 2026-08-27 — V48 full-reservoir A0/+20 audit

Experiment: Continue A0/+20 unchanged for a third exact 491,520-decision block after the stage-local winning reservoir reached capacity.

Results: The block produced `238/1,333` wins (`17.85%`), floor `44.50`, floor-51 reach `53.64%`, and `33.29%` conversion, with one ordinary cap and no empty actions. This is a small aggregate improvement over block two (`17.46%`, floor `44.07`) but no conversion gain. Latest rolling wins were `[8,4,11,3,4]/32`. Replay was full and character-balanced. A focused audit then reproduced two Regent action-preview errors: Regalite's block on generated cards and Galactic Dust's threshold block were omitted.

Analysis: Reservoir maturation did not remove the recurrent C3 conversion weakness. The concrete Regent-specific false action features are a more targeted explanation than replay starvation or optimizer instability.

Conclusion: Stop V48 as the valid benchmark and correct the preview semantics under a new incompatible feature/model boundary.

Next: Verify V49 regressions and parity, then restart the shared five-character model from random initialization rather than carrying weights across changed input semantics.

## 2026-08-27 — V49 Regent outcome correction

Experiment: Correct the two reproduced Regent action-preview errors under a hard feature/model boundary and verify the complete production stack before restarting.

Results: Feature `44`/model `49` adds Regalite block for deterministic generated-card effects and Galactic Dust threshold block. Cloned-step regressions cover AddCard, AddFlaggedCard, AddUpgradedCard, Forge, and threshold-crossing star spend. Rust passed `125/125`; the FP16 MPS probe passed with `2,004,787` parameters, permutation errors below `3.0e-8`, and Rust value parity `2.38e-7`. V48 checkpoint loading is no longer permitted.

Analysis: These were concrete C3-specific false action features observed alongside a persistent Regent conversion deficit. Retaining V48 weights or replay would preserve learning under the incomplete feature semantics.

Conclusion: V49 must restart from random initialization with empty optimizer, counters, stage state, and winning reservoir; V48 remains benchmark-only.

Next: Launch one uninterrupted five-million-decision V49 run at A0/+24 with the validated production configuration and fresh disjoint seed namespaces.

## 2026-08-27 — V49 first two bootstrap blocks

Experiment: Train V49 continuously from random initialization for two exact 491,520-decision blocks at A0/+24.

Results: Block one produced `7/2,475` wins (`0.28%`), floor `23.95`, and `1.25%` floor-51 reach. Block two improved to `48/1,710` wins (`2.81%`), floor `34.50`, and `13.45%` reach. Its thirds improved monotonically: wins `2.18%→2.96%→3.33%`, floor `32.77→34.68→36.23`, and reach `8.40%→14.63%→17.74%`. The learner trained `964,716/965,461` attempted rows with no KL rollback, zero empty actions, and controlled clipping.

Analysis: V49 bootstraps more slowly than V42/V45 because batch 2,048 produces fewer optimizer updates, but it is still accelerating. By the second block it has recovered the prior controls' floor range, while terminal conversion and winning replay remain immature.

Conclusion: Do not change learning rate, batch size, or objective during the monotonic bootstrap.

Next: Continue unchanged through a third matched block and reassess whether win conversion closes the remaining gap.

## 2026-08-27 — V49 third bootstrap block

Experiment: Continue the fresh V49 run unchanged through its third exact 491,520-decision block.

Results: The three blocks progressed from `0.28%` to `2.81%` and `6.42%` wins. Mean floor rose `23.95→34.50→38.03`, while floor-51 reach rose `1.25%→13.45%→25.00%`. The third block itself improved across every third: wins `4.85%→6.00%→8.49%`, floor `36.98→38.13→39.02`, and conversion `20.61%→24.43%→31.47%`. No KL rollback, caps, or empty actions occurred.

Analysis: The initially slow random seed is now accelerating coherently. Replay remains only partly bootstrapped (`1,272/4,096` rows), so altering batch cadence or the objective would interrupt a still-improving regime.

Conclusion: Keep the V49 architecture and training settings unchanged.

Next: Continue through 1,966,080 decisions and compare the fourth equal block.

## 2026-08-27 — V49 blocks four through six

Experiment: Continue V49 unchanged through three more matched bootstrap blocks, allowing winning replay to reach capacity.

Results: Win rate advanced `8.98%→12.23%→15.14%`; mean floor advanced `40.34→41.84→43.22`; floor-51 reach advanced `32.68%→40.49%→48.49%`. Block six was stable across thirds at `14.69%`, `14.48%`, and `16.26%` wins. The reservoir reached `4,096/4,096`; latest rolling wins were `[7,4,8,3,2]/32`. Optimization remained clean with no KL rollback or empty-action failures.

Analysis: All primary metrics improved monotonically across six blocks. C1, C3, and C4 are conversion-limited rather than disconnected, and saturated replay has not yet operated for a full block.

Conclusion: Continue the same model and hyperparameters; no targeted replay or optimizer change is justified.

Next: Train through block seven at 3,440,640 decisions and measure the first complete full-reservoir interval.

## 2026-08-27 — V49 promotion and first A0/+20 block

Experiment: Continue V49 through its A0/+24 gate, then measure one exact clean block after promotion to A0/+20.

Results: The A0/+24 gate passed with `307/640` wins; per-character wins were `[64,76,64,53,50]/128`, with one cap counted as failure. The first clean +20 block produced `192/1,447` wins (`13.27%`), floor `44.25`, floor-51 reach `50.17%`, and `26.45%` conversion. Its thirds improved `10.88%→11.74%→17.26%`; the final third reached floor `45.67` and `55.58%` floor-51 reach. No training rows were dropped in the final session.

Analysis: V49 generalizes cleanly to +20 and improves rapidly within the stage. C3 remains the weakest conversion character, while the stage-local reservoir is only `1,600/4,096` rows.

Conclusion: Keep the optimizer, objective, and balanced replay unchanged while the +20 reservoir matures.

Next: Resume for one second clean 491,520-decision +20 block, excluding the restart-censored report.

## 2026-08-27 — V49 +20 blocks two and three

Experiment: Continue V49 at A0/+20 through two more exact clean blocks while stage-local replay reaches capacity.

Results: Block two produced `14.76%` wins, floor `44.90`, reach `54.26%`, and conversion `27.20%`. Block three improved to `18.47%`, floor `44.79`, reach `56.89%`, and conversion `32.47%`; its thirds were stable at `18.87%`, `18.16%`, and `18.41%`. Latest rolling wins were `[4,5,9,4,2]/32`. The learner recorded no KL rollback or empty actions, and the reservoir reached `4,096/4,096`.

Analysis: Terminal conversion improved broadly without sacrificing connectivity. Every character reaches late floors, but rolling per-character wins remain too uneven for promotion.

Conclusion: Keep training unchanged; no character-specific or optimizer intervention is supported.

Next: Measure one complete +20 block with saturated replay throughout.

## 2026-08-27 — V49 full-reservoir blocks seven through ten

Experiment: Continue V49 through four full-reservoir blocks as the auxiliary progress weight anneals toward its `0.1` floor.

Results: Block win rates were `19.07%`, `16.96%`, `20.08%`, and `18.93%`. Mean floor stayed `43.79–44.72`; floor-51 reach rose to a new high of `56.00%`. Block ten was stable across thirds at `18.11%`, `19.56%`, and `19.16%` wins. At 5,001,793 decisions, rolling wins were `[7,10,6,3,4]/32`; no KL rollback, empty actions, or final-window caps occurred.

Analysis: V49 has reached a stable aggregate plateau with improving connectivity but incomplete late-game conversion, especially for C3 and C4. Beta only reached its `0.1` floor at the end, so every completed block still mixed changing policy objectives.

Conclusion: Do not roll back to the lexicographic best checkpoint or alter replay yet; first measure a complete fixed-beta block from the latest policy.

Next: Resume unchanged for 524,288 decisions, exclude the startup-censored report, and evaluate the following 491,520 decisions at fixed beta `0.1`.

## 2026-08-27 — V49 fourth A0/+20 block

Experiment: Continue V49 unchanged for a fourth exact clean 491,520-decision block at A0/+20 with the stage-local reservoir full throughout.

Results: The block produced `281/1,420` wins (`19.79%`), floor `45.92`, floor-51 reach `60.63%`, and `32.64%` conversion, with zero caps or empty actions. Its thirds were stable at `19.11%`, `20.37%`, and `19.87%` wins; the final third reached floor `46.86` and `65.60%` floor-51 reach. Latest rolling wins were `[9,12,3,6,8]/32`. A hung sampler tail was replaced from the immutable `7,201,147` checkpoint; the restart-censored report was excluded. The recovered segment trained `131,072/131,072` rows with no freshness, ratio, KL, or post-step drops and reached `2,076` decisions/s.

Analysis: This is the strongest +20 aggregate and improves block three's wins, floor, reach, and conversion. The remaining failures are volatile per-character conversion rather than lost connectivity or optimization instability. C3 is one rolling win short of the trigger; C2 is currently the larger blocker despite a high mean floor.

Conclusion: Keep the architecture, optimizer, objective, and balanced replay unchanged. Add a per-output process lock so accidental duplicate trainers cannot share an output directory.

Next: Resume from `7,332,219` for 524,288 decisions, exclude the first restart-censored report, and allow the automatic promotion gate only when every character exceeds 20%.

## 2026-08-27 — V49 fifth A0/+20 block

Experiment: Continue V49 unchanged for another exact clean 491,520-decision block at A0/+20 with full stage-local replay.

Results: The block produced `303/1,387` wins (`21.85%`), floor `46.56`, floor-51 reach `65.61%`, and `33.30%` conversion, with zero caps or empty actions. Its thirds were `24.44%`, `21.24%`, and `20.00%` wins; conversion remained stable at `33.64%`, `33.00%`, and `33.22%`. Latest rolling wins were `[9,7,6,6,5]/32`. The learner trained `513,183/513,202` attempted rows with no KL or post-step rollback, and replay remained full at `9.42%`.

Analysis: This improves block four's wins, floor, reach, and conversion. The declining third-by-third win rate reflects fewer final-act arrivals rather than worsening conversion, and the final third still meets the aggregate curriculum threshold. C2 and C3 are each one rolling win short; C4 is two short.

Conclusion: Continue the same policy, optimizer, objective, and balanced replay. The promotion criterion is not yet satisfied for every character.

Next: Resume from the actual `7,857,946` checkpoint for 524,288 decisions, exclude the first restart-censored report, and allow automatic promotion if every character exceeds 20%.

## 2026-08-27 — V49 sixth A0/+20 block

Experiment: Continue V49 unchanged for a sixth exact clean 491,520-decision block at A0/+20.

Results: The block produced `356/1,433` wins (`24.84%`), floor `46.37`, floor-51 reach `65.46%`, and `37.95%` conversion, with zero caps or empty actions. Its thirds were consistent at `25.96%`, `24.24%`, and `24.36%` wins, with conversion `37.42–38.76%`. Latest rolling wins were `[7,7,11,3,8]/32`; four characters passed the trigger. The learner trained `515,857/515,896` attempted rows with no KL or post-step rollback, and replay remained full at `9.59%`.

Analysis: This is a broad gain over block five, especially in terminal conversion. Regent is the only remaining rolling blocker, but its floor `46.75` shows that the deficit is late-game conversion rather than connectivity or data starvation.

Conclusion: Keep the shared policy and training settings unchanged; the automatic promotion gate must wait for Regent to exceed 20% as well.

Next: Follow the measured continuation recommendation from the completed block without weakening the all-character gate.

## 2026-08-27 — V49 seventh A0/+20 block

Experiment: Continue V49 unchanged for a seventh exact clean 491,520-decision block at A0/+20.

Results: The block produced `348/1,411` wins (`24.66%`), floor `47.17`, floor-51 reach `68.04%`, and `36.25%` conversion, with zero caps or empty actions. Its thirds were `24.68%`, `21.55%`, and `27.84%` wins; the final third reached floor `48.19`, `73.66%` reach, and `37.79%` conversion. Latest rolling wins were `[7,13,7,10,5]/32`. The recovered segment trained all `131,072` rows with no drops and reached `3,167` decisions/s.

Analysis: The block confirms the prior gain while improving floor and late-run reach. C0–C3 now satisfy the rolling trigger. C4 alone remains two wins short, despite enough connectivity to keep learning terminal conversion.

Conclusion: Continue unchanged and preserve the all-character promotion rule.

Next: Resume from `8,941,592` for 524,288 decisions and allow the automatic fresh gate immediately when C4 reaches `7/32`.

## 2026-08-27 — V49 A0/+20 promotion

Experiment: Continue V49 unchanged until every character crossed the rolling `>20%` trigger, then run the declared fresh 128-run-per-character greedy gate.

Results: The trigger fired at decision `9,447,751` with rolling wins `[11,9,7,9,8]/32`. The fresh gate passed with `229/640` wins. Per-character wins were `[39,55,43,43,49]/128`; mean floors were `[47.96,46.95,46.84,48.55,46.45]`. Evaluation caps `[15,9,15,8,9]` were counted as failures, and no empty-action state occurred.

Analysis: Every character clears the required 26 wins by a wide margin, including Regent and Necrobinder. The rolling volatility was sampling noise around a policy with strong greedy performance across all five characters.

Conclusion: Promote V49 from A0/+20 to A0/+16 and reset stage-local winning replay.

Next: Discard the short startup-censored +16 tail and measure the first complete clean A0/+16 block before changing training settings.

## 2026-08-27 — V49 first A0/+16 block

Experiment: Train the promoted V49 checkpoint for one exact clean 491,520-decision block at A0/+16, excluding the startup-censored report.

Results: The block produced `189/1,411` wins (`13.39%`), floor `44.07`, floor-51 reach `50.67%`, and `26.43%` conversion, with zero caps or empty actions. Its thirds were stable at `13.75%`, `13.58%`, and `12.87%` wins; conversion improved from `23.94%` to `28.09%` and held at `27.60%`. Latest rolling wins were `[4,5,7,3,5]/32`. The learner trained `510,562/510,689` attempted rows with no KL or post-step rollback. The stage-local reservoir reached `1,512/4,096` and replay `8.51%`.

Analysis: The harder stage reduces aggregate conversion but does not collapse connectivity. Four characters remain below the trigger, while replay has not yet accumulated enough +16 winners for a mature comparison.

Conclusion: Keep the optimizer, objective, and character-balanced replay unchanged while the stage reservoir fills.

Next: Resume from `9,991,441` for 524,288 decisions, exclude the first restart-censored report, and measure the second clean +16 block.

## 2026-08-27 — V49 second A0/+16 block

Experiment: Continue V49 unchanged for a second exact clean 491,520-decision block at A0/+16 while stage-local replay matures.

Results: The block produced `201/1,426` wins (`14.10%`), floor `44.39`, floor-51 reach `51.40%`, and `27.42%` conversion, with zero caps or empty actions. Its thirds were `12.74%`, `15.15%`, and `14.38%` wins. Latest rolling wins were `[4,10,3,5,7]/32`. The learner trained `512,954/513,066` attempted rows with no KL or post-step rollback. The reservoir reached `3,120/4,096` and replay `9.66%`.

Analysis: The second block improves aggregate wins and conversion while preserving connectivity. The policy remains below promotion for several characters, but there is no optimizer failure or monotonic regression.

Conclusion: Continue unchanged until replay has operated at full capacity for a complete block.

Next: Follow the measured continuation recommendation from the completed second +16 block.

## 2026-08-27 — V49 full-reservoir A0/+16 block

Experiment: Continue V49 unchanged for a third exact clean A0/+16 block after the stage-local winning reservoir reached capacity.

Results: The block produced `248/1,401` wins (`17.70%`), floor `45.04`, floor-51 reach `55.46%`, and `31.92%` conversion, with zero caps or empty actions. Its thirds improved monotonically: wins `15.37%→17.56%→20.26%` and conversion `27.65%→32.54%→35.63%`. Latest rolling wins were `[7,8,6,3,6]/32`. The learner trained `511,782/511,923` attempted rows with no KL or post-step rollback; replay was full at `9.34%`.

Analysis: Mature replay coincides with a clear aggregate and within-block improvement. Regent remains the weakest rolling character, but all characters retain late-floor connectivity.

Conclusion: Continue unchanged; the learning slope is positive and no character-specific intervention is warranted.

Next: Follow the measured continuation recommendation from this full-reservoir block.

## 2026-08-27 — V49 fourth A0/+16 block

Experiment: Continue V49 unchanged for a fourth exact clean A0/+16 block with saturated replay.

Results: The block produced `241/1,393` wins (`17.30%`), floor `45.42`, floor-51 reach `59.08%`, and `29.28%` conversion, with zero caps or empty actions. Its thirds declined from `21.65%` to `15.70%` and `14.75%` wins; conversion declined `31.91%→28.46%→26.98%`, while final-third reach remained `54.66%`. Latest rolling wins were `[3,9,5,6,8]/32`. The learner trained `513,157/513,499` attempted rows with no KL or post-step rollback.

Analysis: Aggregate performance remains near block three and connectivity is stronger, but the within-block conversion decline is the first sustained softening after mature replay. Optimizer diagnostics remain healthy, so any next change should isolate the terminal-conversion bottleneck.

Conclusion: Pause automatic continuation long enough to choose one controlled next experiment; do not change several training levers together.

Next: Follow the measured recommendation based on blocks three and four.

## 2026-08-27 — V49 A0/+16 promotion

Experiment: Continue V49 unchanged until the rolling all-character trigger fired, then run the fresh 128-run-per-character A0/+16 gate.

Results: The gate passed with `265/640` wins. Per-character wins were `[46,58,60,49,52]/128`; mean floors were `[47.82,48.85,48.34,49.01,49.15]`. Seven total caps were counted as failures, and no empty-action state occurred.

Analysis: Every character clears the required 26 wins by at least 20 wins. The preceding rolling volatility again underestimated a broadly capable greedy policy.

Conclusion: Promote V49 from A0/+16 to A0/+12 and reset stage-local replay.

Next: Complete the first clean 491,520-decision +12 block before evaluating adaptation or changing training settings.

## 2026-08-27 — V49 first A0/+12 block

Experiment: Train the promoted V49 checkpoint for one exact clean 491,520-decision block at A0/+12, excluding two startup-censored reports around the process boundary.

Results: The block produced `117/1,428` wins (`8.19%`), floor `41.91`, floor-51 reach `38.31%`, and `21.39%` conversion, with zero caps or empty actions. Its thirds improved monotonically: wins `5.92%→7.88%→10.87%`; the final third reached floor `42.87` and `47.83%` floor-51 reach. Latest rolling wins were `[2,2,0,0,5]/32`. The recovery trained `229,135/229,376` attempted rows with no stale, KL, or post-step drops. The reservoir reached `984/4,096`.

Analysis: The aggregate reflects early-stage adaptation rather than a plateau. The within-block learning slope is positive, while Regent remains the weakest character and terminal replay is still immature.

Conclusion: Continue unchanged while +12 winner coverage grows.

Next: Resume from `12,322,715` for 524,288 decisions, exclude the first restart-censored report, and measure the second clean +12 block.

## 2026-08-27 — V49 second A0/+12 block

Experiment: Continue V49 unchanged for a second exact clean A0/+12 block while the stage-local winning reservoir grows.

Results: The block produced `145/1,374` wins (`10.55%`), floor `43.51`, floor-51 reach `48.33%`, and `21.84%` conversion, with one ordinary cap and no empty actions. Its thirds cooled from `13.04%` to `10.21%` and `8.57%` wins, while final-third reach remained `45.40%`. Latest rolling wins were `[3,5,0,1,3]/32`. The learner trained `508,484/509,366` attempted rows with no KL or post-step rollback. The reservoir reached `2,144/4,096`.

Analysis: The block improves every aggregate measure over block one, but its within-block conversion softens. C2 and especially C3 remain weak; replay is still only half full, so this does not yet isolate a mature terminal-learning plateau.

Conclusion: Keep training unchanged while the +12 winning reservoir approaches capacity.

Next: Resume from `12,847,326` for 524,288 decisions, exclude the first restart-censored report, and measure block three.

## 2026-08-27 — V49 third A0/+12 block

Experiment: Continue V49 unchanged for a third exact clean A0/+12 block as stage-local replay approaches capacity.

Results: The block produced `193/1,371` wins (`14.08%`), floor `43.77`, floor-51 reach `49.96%`, and `28.18%` conversion, with zero caps or empty actions. Its thirds held `13.56%`, `13.49%`, and `15.20%` wins while conversion improved `25.42%→28.64%→30.67%`. Latest rolling wins were `[8,5,9,2,6]/32`. The learner trained `508,700/508,923` attempted rows with no KL or post-step rollback. The reservoir reached `3,688/4,096`.

Analysis: This is a broad gain over block two and ends with a positive conversion slope. C3 remains the main character bottleneck, but no optimizer or connectivity failure is present.

Conclusion: Continue unchanged through the first complete saturated-replay block.

Next: Resume from `13,372,327` for 524,288 decisions, exclude the first restart-censored report, and measure block four.

## 2026-08-27 — V49 fourth A0/+12 block

Experiment: Continue V49 unchanged for the first complete A0/+12 block with saturated stage-local replay.

Results: The block produced `178/1,358` wins (`13.11%`), floor `44.16`, floor-51 reach `50.81%`, and `25.80%` conversion, with zero caps or empty actions. Its thirds were `13.26%`, `14.00%`, and `12.10%` wins. The learner recovery trained `65,218/65,536` attempted rows with no KL or post-step rollback, and replay remained full.

Analysis: This is slightly below block three's `14.08%` wins and `28.18%` conversion, but floor and reach remain healthy. The result is a flat-to-soft block rather than a systems or connectivity collapse.

Conclusion: Choose one controlled next experiment; do not combine optimizer, replay, and exploration changes.

Next: Follow the measured recommendation from the two mature-replay +12 blocks.

## 2026-08-27 — V49 fifth A0/+12 block

Experiment: Continue V49 unchanged for another exact clean A0/+12 block with saturated replay.

Results: The block produced `171/1,364` wins (`12.54%`), floor `43.85`, floor-51 reach `50.66%`, and `24.75%` conversion, with zero caps or empty actions. Its thirds were `13.70%`, `11.49%`, and `12.50%` wins, with conversion steady around `24–25%`. Latest rolling wins were `[4,4,4,3,4]/32`. The learner trained `507,766/507,932` attempted rows with no KL or post-step rollback; replay remained full at `9.41%`.

Analysis: Blocks three through five now show a modest mature-replay plateau: wins `14.08%→13.11%→12.54%` and conversion `28.18%→25.80%→24.75%`, while floor and reach remain stable. The bottleneck is terminal conversion rather than connectivity or optimizer stability.

Conclusion: Select one isolated conversion-focused experiment before spending another full unchanged block.

Next: Follow the measured recommendation from the three mature-replay +12 blocks.

## 2026-08-27 — V49 doubled winning-replay loss

Experiment: Continue A0/+12 for one exact clean block with only the winning-replay loss coefficient increased from `0.1` to `0.2`; keep replay rows capped at 10% and leave PPO, learning rate, entropy, and trust-region settings unchanged.

Results: The block produced `167/1,363` wins (`12.25%`), floor `44.42`, floor-51 reach `52.46%`, and `23.36%` conversion, with zero caps or empty actions. Its thirds were `12.58%`, `10.32%`, and `13.91%` wins. The learner trained `509,124/509,518` attempted rows with no KL or post-step rollback; observed KL was `0.00146`, replay was `9.36%`, and the reported loss weight was `0.2`.

Analysis: Doubling replay weight did not improve the targeted metric. Recent `0.1` blocks produced `12.54–14.08%` wins and `24.75–28.18%` conversion. The `0.2` block slightly improved floor/reach but reduced terminal conversion.

Conclusion: Reject the doubled replay-loss coefficient as the next production setting.

Next: Restore `0.1` and select the next isolated conversion experiment from the measured plateau.

## 2026-08-27 — V49 replay-weight restoration

Experiment: Continue the current policy for one exact clean A0/+12 block after restoring winning-replay loss weight from `0.2` to `0.1`; keep every other setting fixed.

Results: The block produced `197/1,355` wins (`14.54%`), floor `44.64`, floor-51 reach `54.46%`, and `26.69%` conversion, with zero caps or empty actions. Its thirds were `15.83%`, `15.27%`, and `12.56%` wins. The learner trained `509,196/509,773` attempted rows with no KL or post-step rollback. Replay remained full at `9.36%`.

Analysis: Restoring `0.1` recovered wins and conversion over the `0.2` test (`12.25%`, `23.36%`) and returned performance to the prior mature-replay range. The doubled coefficient is causally disfavored.

Conclusion: Retain winning-replay loss weight `0.1`.

Next: Select one different conversion-focused experiment without changing replay weight, volume, or optimizer stability controls.

## 2026-08-27 — V49 replay-weight recovery confirmation

Experiment: Continue A0/+12 for a second exact clean block after restoring winning-replay loss weight to `0.1`.

Results: The block produced `218/1,353` wins (`16.11%`), floor `44.73`, floor-51 reach `57.58%`, and `27.98%` conversion, with zero caps or empty actions. Its thirds were `14.61%`, `17.74%`, and `15.88%` wins. The learner trained `507,231/508,036` attempted rows with no KL or post-step rollback; replay remained full at `9.26%`.

Analysis: This exceeds every preceding mature +12 block and confirms that restoring `0.1` produced a real recovery rather than a single-window fluctuation. The `0.2` replay-loss experiment remains rejected.

Conclusion: Keep winning-replay loss weight at `0.1`.

Next: Continue only if the measured block-level trend supports it; do not revisit the heavier replay coefficient.

## 2026-08-27 — V49 post-recovery A0/+12 block

Experiment: Continue A0/+12 unchanged at winning-replay loss weight `0.1` for another exact clean block.

Results: The block produced `196/1,344` wins (`14.58%`), floor `44.71`, floor-51 reach `55.58%`, and `26.24%` conversion, with zero caps or empty actions. Its thirds declined from `18.56%` to `14.14%` and `11.16%` wins; conversion declined from `28.99%` to `21.49%`. Latest rolling wins were `[5,11,2,1,3]/32`. The learner trained `509,444/509,818` attempted rows with no KL or post-step rollback.

Analysis: Aggregate performance remains within the recovered range, but the third-by-third conversion decline is now repeated despite full replay and stable optimization. Connectivity remains adequate.

Conclusion: Choose one isolated terminal-conversion experiment rather than another indefinite unchanged block.

Next: Follow the measured recommendation from the mature +12 sequence.

## 2026-08-27 — V49 policy-temperature 0.8

Experiment: Continue A0/+12 for one exact clean block with only actor and PPO policy temperature reduced from `1.0` to `0.8`; keep replay weight `0.1` and all other settings fixed.

Results: The block produced `230/1,324` wins (`17.37%`), floor `45.55`, floor-51 reach `61.63%`, and `28.19%` conversion, with zero caps or empty actions. Its thirds were `20.90%`, `14.10%`, and `17.42%` wins. Latest rolling wins were `[6,6,6,5,7]/32`. The learner trained `509,752/510,449` attempted rows with no KL or post-step rollback.

Analysis: Temperature `0.8` exceeds the mature temperature-`1.0` mean (`14.16%`) and its previous best (`16.11%`) while improving floor and reach. The gain is not caused by easier optimization or reduced coverage.

Conclusion: Temperature `0.8` is the preferred continuation setting, pending one confirmation block.

Next: Repeat one clean block at temperature `0.8`, allowing the automatic promotion gate if every character exceeds 20%.

## 2026-08-27 — V49 policy-temperature 0.8 replication

Experiment: Repeat one exact clean A0/+12 block at policy temperature `0.8` and replay loss weight `0.1`.

Results: The replication produced `223/1,356` wins (`16.45%`), floor `44.54`, floor-51 reach `56.56%`, and `29.07%` conversion, with zero caps or empty actions. Combined with the first `0.8` block, the setting produced `453/2,680` wins (`16.90%`). The learner trained `509,064/509,821` attempted rows with no KL or post-step rollback.

Analysis: Both `0.8` blocks outperform the six mature temperature-`1.0` controls, whose mean win rate was `14.16%`; conversion also remains stronger than the latest control. The declining thirds show volatility but do not overturn the replicated aggregate gain.

Conclusion: Accept policy temperature `0.8` as the production setting and retain replay loss weight `0.1`.

Next: Continue unchanged at temperature `0.8`, allowing the automatic all-character promotion gate when every rolling window exceeds 20%.

## 2026-08-27 — V49 continued policy-temperature 0.8

Experiment: Continue A0/+12 for a third exact clean block at policy temperature `0.8` and replay loss weight `0.1`.

Results: The block produced `213/1,325` wins (`16.08%`), floor `45.42`, floor-51 reach `59.70%`, and `26.93%` conversion, with zero caps or empty actions. Its thirds were `20.72%`, `15.00%`, and `12.89%` wins. Latest rolling wins were `[4,4,3,6,3]/32`. The learner trained `509,226/510,036` attempted rows with no KL or post-step rollback.

Analysis: Three temperature-`0.8` blocks now produce `17.37%`, `16.45%`, and `16.08%` wins, remaining above the mature temperature-`1.0` baseline. The declining thirds show persistent terminal-conversion volatility, not a loss of floor connectivity.

Conclusion: Retain temperature `0.8`; any subsequent experiment should target conversion without reverting the validated sampling improvement.

Next: Follow the measured recommendation from the three temperature-`0.8` blocks.

## 2026-08-27 — V49 policy-temperature 0.6

Experiment: Continue A0/+12 for one exact clean block with only policy temperature reduced from `0.8` to `0.6`; keep replay weight `0.1` and all other settings fixed.

Results: The block produced `265/1,340` wins (`19.78%`), floor `45.41`, floor-51 reach `60.60%`, and `32.64%` conversion, with zero caps or empty actions. Its thirds were `25.00%`, `19.43%`, and `15.08%` wins. The learner trained `505,444/510,637` attempted rows with no KL or post-step rollback; ratio filtering rose to about `1.0%`, observed KL was `0.00325`, and throughput was `1,405` decisions/s.

Analysis: Temperature `0.6` is the strongest +12 aggregate and improves conversion over all temperature-`0.8` blocks. The declining thirds, higher ratio filtering, and lower throughput require replication before acceptance.

Conclusion: Treat temperature `0.6` as provisionally positive.

Next: Repeat one clean block at temperature `0.6` before changing any other lever.

## 2026-08-27 — V49 policy-temperature 0.6 replication

Experiment: Repeat one exact clean A0/+12 block at policy temperature `0.6` and replay loss weight `0.1`.

Results: The replication produced `230/1,318` wins (`17.45%`), floor `45.23`, floor-51 reach `61.61%`, and `28.33%` conversion, with zero caps or empty actions. Its thirds were `19.81%`, `15.57%`, and `17.09%` wins, with conversion stable at about `28%`. Combined, the two temperature-`0.6` blocks average about `18.6%` wins. The learner trained `506,977/508,442` attempted rows with no KL or post-step rollback.

Analysis: Temperature `0.6` replicates above the three temperature-`0.8` blocks, which average about `16.6%`, but increases filtering and lowers throughput. The result supports better exploitation without destabilizing the trust region.

Conclusion: Accept temperature `0.6` as the stronger policy-sampling setting.

Next: Continue at temperature `0.6` unless a measured promotion or plateau criterion supports a new isolated change.

## 2026-08-27 — V49 continued policy-temperature 0.6

Experiment: Continue A0/+12 for another exact clean block at accepted policy temperature `0.6` and replay loss weight `0.1`.

Results: The block produced `264/1,315` wins (`20.08%`), floor `45.59`, floor-51 reach `61.75%`, and `32.51%` conversion, with zero caps or empty actions. Its thirds were `22.35%`, `17.26%`, and `20.72%` wins; final-third conversion recovered to `34.07%`. Latest rolling wins were `[4,11,4,6,7]/32`. The learner trained `506,537/508,269` attempted rows with no KL or post-step rollback.

Analysis: Three temperature-`0.6` blocks now produce `19.78%`, `17.45%`, and `20.08%` wins. The third block restores the initial gain and ends positively, confirming the setting over temperature `0.8` and `1.0`.

Conclusion: Retain policy temperature `0.6`; the remaining work is all-character consistency, not global policy quality.

Next: Continue unchanged and allow the automatic promotion gate only when every character exceeds 20%.

## 2026-08-27 — V49 sustained policy-temperature 0.6

Experiment: Continue A0/+12 for a fourth exact clean block at policy temperature `0.6`.

Results: The block produced `259/1,282` wins (`20.20%`), floor `45.81`, floor-51 reach `62.56%`, and `32.29%` conversion, with zero caps or empty actions. Its thirds were `23.56%`, `18.86%`, and `18.31%` wins. Latest rolling wins were `[5,12,5,4,5]/32`. The learner trained `504,953/505,939` attempted rows with no KL or post-step rollback.

Analysis: Four temperature-`0.6` blocks now produce `19.78%`, `17.45%`, `20.08%`, and `20.20%` wins. The setting is consistently better than warmer controls; promotion remains blocked by rolling per-character alignment rather than aggregate quality.

Conclusion: Keep temperature `0.6` and the existing all-character gate.

Next: Follow the measured continuation recommendation without weakening the curriculum criterion.

## 2026-08-27 — V49 policy-temperature 0.5

Experiment: Continue A0/+12 for one exact clean block with only policy temperature reduced from `0.6` to `0.5`.

Results: The block produced `244/1,309` wins (`18.64%`), floor `45.57`, floor-51 reach `61.42%`, and `30.35%` conversion, with zero caps or empty actions. Its thirds declined `25.42%→16.23%→14.77%`. The learner trained `507,862/509,206` attempted rows with no KL or post-step rollback; observed KL rose to `0.00411` and throughput fell to `1,190` decisions/s.

Analysis: Temperature `0.5` does not exceed the four-block temperature-`0.6` mean of `19.37%` and shows sharper within-block decay. It increases policy movement and compute cost without a demonstrated outcome gain.

Conclusion: Reject temperature `0.5` and restore the accepted `0.6` setting.

Next: Continue from the current policy at temperature `0.6`, changing no other setting.

## 2026-08-27 — V49 A0/+12 promotion

Experiment: Continue V49 at temperature `0.6` until the rolling all-character trigger fired, then run the fresh 128-run-per-character A0/+12 gate.

Results: The gate passed with `192/640` wins. Per-character wins were `[38,49,38,31,36]/128`; mean floors were `[47.31,47.88,46.32,46.46,47.90]`. Thirty-four caps were counted as failures, and no empty-action state occurred.

Analysis: Every character clears the required 26 wins despite the harder stage and conservative cap handling. Temperature `0.6` supplied enough all-character consistency to advance.

Conclusion: Promote V49 from A0/+12 to A0/+8 and reset stage-local replay.

Next: Complete the first clean 491,520-decision +8 block before evaluating adaptation.

## 2026-08-27 — V49 first A0/+8 block

Experiment: Train the promoted V49 checkpoint for one exact clean 491,520-decision block at A0/+8, excluding startup-censored reports around the process boundary.

Results: The block produced `50/1,453` wins (`3.44%`), floor `38.55`, floor-51 reach `23.40%`, and `14.71%` conversion, with zero caps or empty actions. Its thirds were `4.18%`, `2.59%`, and `3.59%` wins; the final third recovered floor and reach to `39.00` and `24.74%`. Latest rolling wins were `[2,7,1,0,1]/32`. The recovery trained `195,259/196,608` attempted rows with no stale, KL, or post-step drops. The reservoir reached `416/4,096`.

Analysis: This is expected early-stage adaptation rather than collapse. C3 is the weakest character, while terminal replay is still in its initial bootstrap.

Conclusion: Continue unchanged at temperature `0.6` and replay weight `0.1` while +8 winner coverage grows.

Next: Resume from `22,006,223` for 524,288 decisions, exclude the first restart-censored report, and measure the second clean +8 block.

## 2026-08-27 — V49 second A0/+8 block

Experiment: Continue V49 unchanged at temperature `0.6` and replay weight `0.1` for a second clean A0/+8 block.

Results: The block produced `41/1,444` wins (`2.84%`), floor `38.26`, floor-51 reach `22.09%`, and `12.85%` conversion, with zero caps or empty actions. Its thirds were `2.45%`, `1.88%`, and `4.20%` wins; the final third reached floor 51 in `25.21%` of episodes. The learner trained `502,231/507,368` attempted rows; observed KL was `0.00463`, and the reservoir reached `744/4,096`.

Analysis: Results are slightly below block 1 but floors remain flat and the final third rebounds. Terminal winner coverage remains immature; the small post-step rejection rate shows the trust region is working.

Conclusion: The evidence supports continued +8 adaptation, not a policy or optimizer change.

Next: Resume unchanged from `22,531,640` to `23,055,928`, exclude the first restart-censored report, and evaluate block 3.

## 2026-08-27 — V49 third A0/+8 block

Experiment: Continue V49 unchanged at temperature `0.6` and replay weight `0.1` for a third clean A0/+8 block.

Results: The block produced `50/1,442` wins (`3.47%`), floor `38.20`, floor-51 reach `22.95%`, and `15.11%` conversion, with zero caps or empty actions. Its thirds were `3.09%`, `2.70%`, and `4.63%` wins; final-third reach and conversion improved to `25.05%` and `18.49%`. The learner trained `499,681/507,428` attempted rows; observed KL was `0.00459`, and the reservoir reached `1,144/4,096`.

Analysis: Block 3 recovers block 1's win rate after block 2's small dip, while the strongest results again occur in the final third. Replay remains well short of saturation.

Conclusion: +8 learning is stable but still terminal-data limited.

Next: Continue unchanged until the reservoir and full-block trend provide a mature comparison.

## 2026-08-27 — V49 fourth A0/+8 block

Experiment: Continue V49 unchanged at temperature `0.6` and replay weight `0.1` for a fourth clean A0/+8 block.

Results: The block produced `43/1,458` wins (`2.95%`), floor `38.09`, floor-51 reach `22.29%`, and `13.23%` conversion, with zero caps or empty actions. Its thirds were `2.45%`, `3.50%`, and `2.90%` wins. The learner trained `486,577/507,852` attempted rows; observed KL was `0.00570`, and the reservoir reached `1,488/4,096`.

Analysis: Four +8 blocks remain tightly grouped at `2.84–3.47%` wins and floors `38.09–38.55`. This is a low plateau so far, but terminal replay remains well below capacity.

Conclusion: Do not change the objective before obtaining mature +8 winner coverage.

Next: Test policy temperature `0.8` for one clean block, keeping replay weight `0.1` and every other setting fixed.

## 2026-08-27 — V49 A0/+8 policy-temperature 0.8

Experiment: Test one clean A0/+8 block with only policy temperature increased from `0.6` to `0.8`.

Results: The block produced `40/1,461` wins (`2.74%`), floor `37.50`, floor-51 reach `20.60%`, and `13.29%` conversion, with zero caps or empty actions. Its thirds were `2.43%`, `3.23%`, and `2.54%` wins. The learner trained `503,331/507,389` attempted rows with no KL or post-step rollback; observed KL was `0.00550`, entropy `0.725`, and the reservoir reached `1,808/4,096`.

Analysis: Temperature `0.8` underperforms the four temperature-`0.6` controls in wins, floor, and reach. More stochastic sampling did not relieve the +8 bottleneck.

Conclusion: Reject temperature `0.8` for A0/+8.

Next: Restore the validated temperature `0.6` setting before testing any other lever.

## 2026-08-27 — V49 A0/+8 fixed progress beta 0.1

Experiment: Restore policy temperature `0.6` and re-enable only the public progress advantage at fixed `β=0.1` for one clean A0/+8 block.

Results: The block produced `51/1,411` wins (`3.61%`), floor `38.27`, floor-51 reach `24.24%`, and `14.91%` conversion, with zero caps or empty actions. Its thirds were `3.51%`, `3.90%`, and `3.44%` wins; reach rose `19.63%→25.76%→27.53%`. The learner trained `498,339/506,412` attempted rows with observed KL `0.00431`; the reservoir reached `2,216/4,096`.

Analysis: The trial exceeds the four no-progress controls' `3.18%` mean win rate and improves late-floor reach without destabilizing optimization. One block is insufficient to separate the small gain from variance.

Conclusion: Fixed progress `β=0.1` is provisionally positive at +8.

Next: Replicate the isolated setting before accepting or rejecting it.

## 2026-08-27 — V49 A0/+8 fixed progress beta 0.1 replication

Experiment: Replicate one clean A0/+8 block at temperature `0.6`, replay weight `0.1`, and fixed progress `β=0.1`.

Results: The block produced `75/1,416` wins (`5.30%`), floor `38.71`, floor-51 reach `24.72%`, and `21.43%` conversion, with zero caps or empty actions. Its thirds were stable at `5.00%`, `5.47%`, and `5.42%` wins; final-third reach rose to `29.07%`. The learner trained `501,004/504,667` attempted rows with no KL or post-step rollback. The reservoir reached `2,816/4,096`.

Analysis: The replication materially exceeds both the first beta trial and the four no-progress controls. The gain comes from stronger conversion without sacrificing reach or optimizer stability.

Conclusion: Accept fixed progress `β=0.1` for A0/+8.

Next: Continue at the accepted setting while the stage-local winning reservoir reaches capacity.

## 2026-08-27 — V49 continued A0/+8 fixed progress beta 0.1

Experiment: Continue the accepted A0/+8 configuration for another clean block at temperature `0.6`, replay weight `0.1`, and fixed progress `β=0.1`.

Results: The block produced `60/1,417` wins (`4.23%`), floor `38.38`, floor-51 reach `26.61%`, and `15.92%` conversion, with zero caps or empty actions. Its thirds were `3.27%`, `4.94%`, and `4.56%` wins. The learner trained `493,912/505,121` attempted rows; observed KL was `0.00651`, and the reservoir reached `3,296/4,096`.

Analysis: Three beta-enabled blocks now produce `3.61%`, `5.30%`, and `4.23%` wins, clearly above the no-progress mean of `3.18%`; reach continues to improve. The independent state audit nevertheless found an exact random-generation preview omission affecting Regent actions.

Conclusion: Accept fixed progress `β=0.1`, but stop V49 at this boundary because its action representation is incomplete.

Next: Fix random generated-card `CardGenerated` previews, create a new feature/model boundary, and restart from random initialization.

## 2026-08-27 — V50 random-generation preview boundary

Experiment: Create FEATURE45/MODEL50 and restart from random initialization at A0/+24. Random and distinct card generators now preview only the public number of `CardGenerated` triggers, without sampling or exposing generated identities.

Results: Cloned-step regressions cover Bundle of Joy, White Noise, Jack of All Trades, Jackpot, Cosmic Concoction, and character-specific random/distinct generation. Rust tests pass `127/127`; the MPS probe passes with `2,004,787` parameters, Rust parity `2.38e-7`, and permutation errors below `2e-8`.

Analysis: V49 remains a useful benchmark, but its weights and replay were trained with incomplete Regent action outcomes and cannot seed the corrected experiment.

Conclusion: V50 requires a fresh policy, optimizer, curriculum, and winning reservoir.

Next: Launch one continuous V50 run at A0/+24 with fresh disjoint seeds and the validated production architecture.

## 2026-08-27 — V50 first A0/+24 block

Experiment: Train the corrected FEATURE45/MODEL50 model from random initialization for its first 491,520 decisions at A0/+24.

Results: The block produced `7/2,440` wins (`0.29%`), floor `24.28`, floor-51 reach `1.76%`, and `16.28%` conversion, with zero caps or empty actions. Its thirds improved from `0%` wins and floor `20.01` to `0.94%` wins and floor `30.19`. The learner trained `487,856/488,419` attempted rows with no KL or post-step rollback; observed KL was `0.00341` and throughput `2,589` decisions/s.

Analysis: V50 matches V49's early win rate while improving floor and floor-51 reach. The monotonic within-block acceleration shows normal scratch bootstrap; the winning reservoir contains only `56/4,096` rows.

Conclusion: Continue the corrected model unchanged.

Next: Evaluate the second equal block while the winning reservoir and terminal conversion begin to develop.

## 2026-08-27 — V50 second A0/+24 block

Experiment: Continue fresh V50 unchanged through its second exact 491,520-decision block.

Results: The block produced `40/1,641` wins (`2.44%`), floor `34.37`, floor-51 reach `12.07%`, and `20.20%` conversion, with zero caps or empty actions. Its thirds improved from `1.38%` wins and floor `32.80` to `3.06%` wins and floor `35.68`. The learner trained `963,735/964,721` attempted rows with no KL or post-step rollback; observed KL was `0.00239` and throughput `2,220` decisions/s.

Analysis: Every primary metric improves sharply over block 1, and the second block continues improving internally. Replay remains early at `376/4,096` rows.

Conclusion: V50 is learning normally from scratch with no intervention indicated.

Next: Continue unchanged through the third equal block.

## 2026-08-27 — V50 third A0/+24 block

Experiment: Continue fresh V50 unchanged through its third exact 491,520-decision block.

Results: The block produced `103/1,409` wins (`7.31%`), floor `38.70`, floor-51 reach `25.76%`, and `28.37%` conversion, with zero caps or empty actions. Its thirds were `6.98%`, `7.13%`, and `7.82%` wins. The learner trained `1,443,274/1,444,304` attempted rows with no KL or post-step rollback; observed KL was `0.00237`, and the reservoir reached `1,200/4,096`.

Analysis: V50 improves monotonically across all three blocks, and block 3 is stable internally rather than driven by one window. C3 remains the weakest floor character, but all characters are learning.

Conclusion: Continue the scratch run unchanged.

Next: Evaluate block 4 while terminal replay approaches meaningful coverage.

## 2026-08-27 — V50 fourth A0/+24 block

Experiment: Continue fresh V50 unchanged through its fourth exact 491,520-decision block.

Results: The block produced `129/1,355` wins (`9.52%`), floor `40.15`, floor-51 reach `32.77%`, and `29.05%` conversion, with zero caps or empty actions. Its thirds were `11.45%`, `7.96%`, and `9.13%` wins; floor rose throughout to `40.57`. The learner trained `1,926,590/1,927,626` attempted rows with no KL or post-step rollback; observed KL was `0.00404`, and the reservoir reached `2,232/4,096`.

Analysis: V50 continues its monotonic block-level improvement in wins, floor, and late-game reach. The middle-third variance does not persist into the final third.

Conclusion: Keep the fresh run unchanged.

Next: Evaluate block 5 with a nearly mature winning reservoir.

## 2026-08-27 — V50 fifth A0/+24 block

Experiment: Continue fresh V50 unchanged through its fifth exact 491,520-decision block.

Results: The block produced `142/1,337` wins (`10.62%`), floor `41.34`, floor-51 reach `36.80%`, and `28.86%` conversion, with zero caps or empty actions. Its thirds stayed between `10.38%` and `11.09%` wins while floor and reach increased to `42.19` and `39.49%`. The learner trained `2,409,718/2,410,764` attempted rows with no KL or post-step rollback; observed KL was `0.00178`, and the reservoir reached `3,200/4,096`.

Analysis: Wins, floor, and reach improve for the fifth consecutive block. Conversion is stable while more trajectories reach the final floor.

Conclusion: Continue unchanged; no optimization intervention is supported.

Next: Evaluate block 6 with the winning reservoir expected to reach capacity.

## 2026-08-27 — V50 sixth A0/+24 block

Experiment: Continue fresh V50 unchanged through its sixth exact 491,520-decision block.

Results: The block produced `182/1,300` wins (`14.00%`), floor `42.52`, floor-51 reach `42.15%`, and `33.21%` conversion, with zero caps or empty actions. Its thirds produced `13.23%`, `17.25%`, and `11.59%` wins. The learner trained `2,876,626/2,877,693` attempted rows with no KL or post-step rollback; observed KL was `0.00151`, and the reservoir reached `3,216/4,096`.

Analysis: Every block-level outcome has improved monotonically through block 6. The final-third dip follows a strong middle third without a corresponding connectivity collapse.

Conclusion: Continue unchanged while replay reaches full capacity.

Next: Evaluate block 7 and allow the standard promotion trigger only when every character clears 20%.

## 2026-08-27 — V50 seventh A0/+24 block

Experiment: Continue fresh V50 unchanged through its seventh exact 491,520-decision block.

Results: The block produced `185/1,315` wins (`14.07%`), floor `42.65`, floor-51 reach `41.29%`, and `34.07%` conversion, with one cap and no empty actions. Its thirds stayed tightly between `13.76%` and `14.25%` wins. The learner trained `3,360,056/3,361,149` attempted rows with no KL or post-step rollback; observed KL was `0.00287` and replay was `8.14%`.

Analysis: Block 7 consolidates block 6 rather than regressing. Floor depth and conversion improve slightly while the win rate remains stable.

Conclusion: The run is healthy but needs another block to distinguish a plateau from renewed growth.

Next: Continue unchanged through block 8, allowing the standard all-character gate if triggered.

## 2026-08-27 — V50 eighth A0/+24 block

Experiment: Continue fresh V50 unchanged through its eighth exact 491,520-decision block.

Results: The block produced `181/1,319` wins (`13.72%`), floor `43.11`, floor-51 reach `45.26%`, and `30.32%` conversion, with zero caps or empty actions. Its thirds were `12.33%`, `16.17%`, and `12.73%` wins. The learner trained `3,845,729/3,846,857` attempted rows with no KL or post-step rollback; observed KL was `0.00191` and replay was `8.34%`.

Analysis: Wins are statistically flat with block 7, while floor and late-game reach improve materially. The remaining variation is terminal conversion rather than lost connectivity.

Conclusion: Do not intervene on a single flat win block while reach is still increasing.

Next: Continue unchanged through block 9.

## 2026-08-27 — V50 ninth A0/+24 block

Experiment: Continue fresh V50 unchanged through its ninth clean 491,520-decision block, excluding one sampler-restart startup report.

Results: The block produced `218/1,341` wins (`16.26%`), floor `43.21`, floor-51 reach `45.64%`, and `35.62%` conversion, with zero caps or empty actions. Its thirds produced `17.78%`, `16.44%`, and `14.60%` wins while conversion remained near `35–36%`. The resumed learner trained `441,683/442,932` attempted rows with no KL or post-step rollback; observed KL was `0.00138`, and the reservoir reached `4,096/4,096`.

Analysis: Block 9 breaks the two-block plateau and establishes a new win-rate peak. The late-third softness is connectivity-driven while terminal conversion stays stable.

Conclusion: Continue unchanged through the final planned A0/+24 block.

Next: Evaluate block 10 at fixed minimum progress beta before changing any setting.

## 2026-08-27 — V50 tenth A0/+24 block

Experiment: Continue V50 unchanged through its tenth clean 491,520-decision block as progress beta reaches its `0.1` floor.

Results: The block produced `200/1,346` wins (`14.86%`), floor `43.33`, floor-51 reach `45.47%`, and `32.68%` conversion, with zero caps or empty actions. Its thirds produced `12.72%`, `15.99%`, and `15.86%` wins; conversion rose to `36.18%` in the final third. The learner trained `906,163/907,564` attempted rows in the resumed session with no KL or post-step rollback; observed KL was `0.00159`, replay `9.47%`, and the reservoir remained full.

Analysis: Block 10 is below block 9's win peak but ends with strengthening conversion and no loss of floor depth. The scheduled progress coefficient has only now reached its minimum.

Conclusion: The A0/+24 result is not promotion-ready and needs a fixed-beta continuation before introducing a new lever.

Next: Follow the measured recommendation for the first full block at minimum progress beta.

## 2026-08-27 — V50 fixed-beta A0/+24 block

Experiment: Resume V50 unchanged for one clean block entirely at the scheduled minimum progress `β=0.1`.

Results: The block produced `229/1,339` wins (`17.10%`), floor `44.26`, floor-51 reach `51.46%`, and `33.24%` conversion, with zero caps or empty actions. Its thirds produced `21.58%`, `13.12%`, and `16.93%` wins. Latest rolling wins were `[6,7,9,4,5]/32`. The learner trained `509,829/509,899` attempted rows with no KL or post-step rollback; observed KL was `0.00242`, replay `9.68%`, and the reservoir remained full.

Analysis: The first full fixed-beta block establishes a new aggregate peak over blocks 9 and 10 while preserving high floor depth and reach. C3 and C4 remain the promotion blockers.

Conclusion: Continue unchanged; no additional lever is justified.

Next: Replicate one fixed-beta block and allow the normal fresh promotion gate if all characters clear 20%.

## 2026-08-27 — V50 fixed-beta A0/+24 replication

Experiment: Replicate one clean A0/+24 block at the scheduled minimum progress `β=0.1`.

Results: The block produced `253/1,307` wins (`19.36%`), floor `45.30`, floor-51 reach `57.23%`, and `33.82%` conversion, with zero caps or empty actions. Its thirds produced `22.57%`, `15.67%`, and `20.14%` wins; final-third conversion reached `38.03%`. Latest rolling wins were `[8,4,8,4,5]/32`. The learner trained `509,118/509,157` attempted rows with no KL or post-step rollback; observed KL was `0.00108`, replay `9.56%`, and the reservoir remained full.

Analysis: Fixed-beta performance improves from `17.10%` to `19.36%` with materially higher floor and reach. C1, C3, and C4 still block the all-character gate despite strong connectivity.

Conclusion: Keep V50 unchanged at the minimum beta.

Next: Continue until the normal all-character promotion trigger fires or another full block establishes a plateau.

## 2026-08-27 — V50 third fixed-beta A0/+24 block

Experiment: Continue V50 unchanged for a third clean block at progress `β=0.1`.

Results: The block produced `250/1,283` wins (`19.49%`), floor `45.41`, floor-51 reach `56.43%`, and `34.53%` conversion, with zero caps or empty actions. Its thirds produced `20.05%`, `16.82%`, and `21.70%` wins; final-third conversion reached `38.82%`. Latest rolling wins were `[5,7,10,4,7]/32`. The learner trained `505,469/505,535` attempted rows with no KL or post-step rollback; observed KL was `0.00216`, replay `9.58%`, and the reservoir remained full.

Analysis: Three fixed-beta blocks improve `17.10%→19.36%→19.49%`, with stable connectivity and a strong final third. C0 and C3 alone block the rolling trigger.

Conclusion: Continue unchanged; the current setting remains productive.

Next: Allow the standard promotion gate when all five rolling character windows exceed 20%.

## 2026-08-27 — V50 A0/+24 promotion

Experiment: Continue V50 at fixed progress `β=0.1` until the rolling all-character trigger fired, then run the fresh 128-run-per-character A0/+24 promotion gate.

Results: The gate passed with `327/640` wins. Per-character wins were `[65,61,74,58,69]/128`; mean floors were `[47.89,47.58,48.44,45.84,47.12]`. One cap was counted as a failure, and no empty-action state occurred.

Analysis: Every character clears the required 26 wins by a wide margin. The stage-local reservoir was reset, and the auxiliary progress advantage was disabled for the next stage as designed.

Conclusion: Promote V50 from A0/+24 to A0/+20.

Next: Complete the first clean +20 block before changing any learning setting.

## 2026-08-27 — V50 first A0/+20 block

Experiment: Train the promoted V50 checkpoint through its first clean 491,520-decision A0/+20 block, excluding promotion and restart startup reports.

Results: The block produced `191/1,284` wins (`14.88%`), floor `43.76`, floor-51 reach `45.79%`, and `32.48%` conversion, with zero caps or empty actions. Its thirds improved from `12.41%` and `12.72%` wins to `19.80%`; final-third floor and reach were `45.14` and `57.21%`. The resumed learner trained `229,322/229,376` attempted rows with no stale, KL, or post-step rollback; observed KL was `0.00299`, replay `9.59%`, and the stage-local reservoir reached `1,560/4,096`.

Analysis: V50 adapts to +20 without collapse and finishes the block substantially stronger than it began. C3 remains the rolling bottleneck, while the reservoir is still rebuilding.

Conclusion: Continue A0/+20 unchanged.

Next: Evaluate a second clean +20 block before changing any setting.

## 2026-08-27 — V50 second A0/+20 block

Experiment: Continue V50 unchanged through its second clean A0/+20 block.

Results: The block produced `248/1,286` wins (`19.28%`), floor `44.70`, floor-51 reach `52.57%`, and `36.69%` conversion, with zero caps or empty actions. Its thirds produced `22.78%`, `18.91%`, and `16.28%` wins. Latest rolling wins were `[5,10,6,2,4]/32`. The learner trained `505,449/505,539` attempted rows with no KL or post-step rollback; observed KL was `0.00324`, replay `9.68%`, and the stage reservoir reached `3,544/4,096`.

Analysis: The second block materially improves every aggregate metric over the first. The within-block cooling is not accompanied by lost connectivity, and replay is not yet fully mature.

Conclusion: Continue A0/+20 unchanged.

Next: Evaluate one full block with a saturated stage-local reservoir.

## 2026-08-27 — V50 third A0/+20 block

Experiment: Continue V50 unchanged through its first full A0/+20 block with a saturated winning reservoir.

Results: The block produced `195/1,290` wins (`15.12%`), floor `44.35`, floor-51 reach `49.77%`, and `30.37%` conversion, with zero caps or empty actions. Its thirds declined from `17.92%` to `14.97%` and `12.61%` wins. Latest rolling wins were `[7,6,7,2,4]/32`. The learner trained `505,794/506,064` attempted rows with no KL or post-step rollback; observed KL was `0.00211`, replay `9.44%`, and the reservoir was full.

Analysis: The block retreats from block 2 despite mature replay, with the largest weakness in C3. One declining block is not sufficient to select a new intervention.

Conclusion: Require a confirming block before changing the objective or sampling policy.

Next: Follow the measured recommendation from the saturated-replay comparison.

## 2026-08-27 — V50 fourth A0/+20 block

Experiment: Continue V50 unchanged through a second full A0/+20 block with saturated replay.

Results: The block produced `210/1,281` wins (`16.39%`), floor `44.94`, floor-51 reach `51.44%`, and `31.87%` conversion, with zero caps or empty actions. Its thirds produced `19.38%`, `14.45%`, and `15.46%` wins. Latest rolling wins were `[3,8,7,7,2]/32`. The learner trained `504,707/505,824` attempted rows with no KL or post-step rollback; observed KL was `0.00488`, replay `9.28%`, and the reservoir remained full.

Analysis: The block partially recovers the prior decline while retaining high floor depth. C0 and C4, rather than global optimization, now block the rolling promotion trigger.

Conclusion: Continue unchanged until the per-character signal is less volatile.

Next: Follow the measured recommendation from the four +20 blocks.

## 2026-08-27 — V50 fifth A0/+20 block

Experiment: Continue V50 unchanged through a third full A0/+20 block with saturated replay.

Results: The block produced `103/1,461` wins (`7.05%`), floor `41.18`, floor-51 reach `27.52%`, and `25.62%` conversion, with zero caps or empty actions. Its thirds recovered from `5.95%` to `7.10%` and `8.16%` wins. Latest rolling wins were `[1,7,3,3,0]/32`. The learner trained `512,900/513,586` attempted rows with no KL or post-step rollback; observed KL was `0.00232`, replay `9.03%`, and the reservoir remained full.

Analysis: This is a material regression from the preceding mature blocks, affecting both connectivity and conversion. The within-block recovery means the direction is not yet a monotonic collapse.

Conclusion: Do not promote or change multiple variables; isolate the cause before continuing broadly.

Next: Follow the measured recovery recommendation from this regression.

## 2026-08-27 — V50 A0/+20 fixed progress beta 0.1

Experiment: Re-enable only the public progress advantage at fixed `β=0.1` for one clean A0/+20 block after the connectivity regression.

Results: The block produced `170/1,389` wins (`12.24%`), floor `43.39`, floor-51 reach `38.66%`, and `31.66%` conversion, with zero caps or empty actions. Its thirds stayed between `11.99%` and `12.55%` wins. The learner trained `511,235/511,470` attempted rows with no KL or post-step rollback; observed KL was `0.00197`, replay `9.44%`, and the reservoir remained full.

Analysis: Fixed progress recovers substantially from the immediately preceding `7.05%` block, but remains below the earlier mature no-progress results. The block is stable rather than accelerating.

Conclusion: The recovery is incomplete; require a replication before accepting the auxiliary at +20.

Next: Resume the latest checkpoint with only target KL tightened from `0.008` to `0.004` for one clean recovery block.

## 2026-08-27 — V50 A0/+20 tighter-KL recovery

Experiment: Continue the collapsed +20 checkpoint with only target KL tightened from `0.008` to `0.004`, retaining fixed progress `β=0.1`.

Results: The block produced `45/1,715` wins (`2.62%`), floor `38.18`, floor-51 reach `10.85%`, and `24.19%` conversion, with zero caps or empty actions. Its thirds produced `2.47%`, `1.36%`, and `4.10%` wins. The learner trained `361,098/524,037` attempted rows; `116,174` rows failed the pre-update KL gate and `44,897` failed the post-update gate.

Analysis: Tightening the gate prevents further oversized updates but does not repair the already-damaged policy. The retained stage-1 checkpoint is therefore not a useful recovery source.

Conclusion: Reject continuation from the collapsed checkpoint and recover from an earlier robust immutable checkpoint.

Next: Preserve stage-specific best checkpoints, then branch from the last robust V50 checkpoint rather than restarting from the damaged state.

## 2026-08-27 — Stage-specific checkpoint retention

Experiment: Audit V50 best selection and pruning after the A0/+20 collapse.

Results: `best.json` tracked only the global best, so its A0/+24 checkpoint at `5,525,286` decisions displaced every later stage from best selection. Pruning happened to leave only that checkpoint and the latest A0/+20 periodic checkpoint at `11,009,180`; earlier robust A0/+20 checkpoints cannot be reconstructed. Best selection now runs independently per curriculum stage, preserves every stage's immutable numbered checkpoint during pruning, records SHA-256, and migrates retained checkpoints from existing runs. V50 metadata now records both retained stages.

Analysis: A global lexicographic score cannot represent checkpoint quality across changing difficulty, and latest-periodic retention is not a substitute for a durable stage best.

Conclusion: Use one immutable scored best per stage while retaining the legacy global fields for compatibility.

Next: Choose the V50 recovery source from the checkpoints that still exist; future stage regressions will retain a scored recovery checkpoint automatically.

## 2026-08-27 — V51 hierarchical public-state architecture

Experiment: Replace V50's flat concatenated state with the requested W128/H4 hierarchy before any new training: shared lossless token encoding; query-only second layers for card zones, map and fusion; actor-scoped tuple pools; nonlinear unordered pools; ordered continuation frames; and linear represented-candidate context.

Results: Feature `46`/model `51` uses five character floats only for replay bookkeeping, never as model input. Thirteen card zones share an L2/FF256 Transformer, the map is `[MAP]` plus 18 floor summaries through a second L2 Transformer, and ragged L2 fusion emits `h∈R128`. Every represented legal or visible-illegal candidate emits `a∈R128`; one `h`-conditioned attention query emits `m∈R128`. Policy scoring is `[h,a,h×a,m] 512→128→1`; the shared critic is `[h,m] 256→128` with separate win and progress outputs. The model has `1,350,019` parameters. The rebuilt MPS probe passed exact categorical round-trip, finite BF16 forward/backward, exact eval-cache invalidation, query-only/full-query output and gradient equivalence, state/card/action permutation errors below `2.7e-8`, and Rust value parity below `1.5e-7`.

Analysis: The hierarchy preserves public state and action-set context while eliminating dense global projection and quadratic candidate self-attention. Exact-count nonlinear pooling distinguishes multiplicity, actor ownership prevents cross-actor collisions, and query-only terminal layers avoid computing unused outputs.

Conclusion: Accept V51 as the next clean architecture boundary; V50 artifacts remain unchanged and incompatible.

Next: Complete the focused Rust suite and the planned batch-1024 runtime gates before starting V51 training.

## 2026-08-27 — V51 late-game runtime gate

Experiment: Benchmark the final V51 implementation on 8,192 real A0/+24 states generated by the simulator teacher, balanced across all five characters and restricted to floor 35 or later. Compare BF16 batches 1,024 and 2,048 after 32-update warm-up, and measure CPU actor supply from repeated final-boss states.

Results: The corpus contained `[1639,1639,1639,1636,1639]` character rows, median floor `43`, `991` final-boss states, p95/max state tokens `499/573`, candidates `18/102`, candidate tokens `5/25`, and continuation tokens `7/27`. Batch 1,024 accepted all 64 updates, sustained `1,737` learner rows/s, had `0.81s` p95 updates, `0.17 GiB` peak live MPS allocation and `6.33 GiB` peak driver allocation; driver growth over the second 32 updates was `0.03 GiB`. Four CPU actors supplied an estimated `9,139` final-boss rows/s. Batch 2,048 accepted all 32 updates but reached only `1,698` rows/s, with `1.35s` p95 updates and `12.54 GiB` peak driver allocation. The represented-candidate corpus generator also exposed and fixed teacher indices being returned in the legal-only order instead of represented order. Final validation passed all `129/129` Rust tests, Python compilation/diff checks, the MPS probe, and Rust parity `1.19e-7`.

Analysis: Batch 1,024 clears every runtime gate with large margin. Batch 2,048 is slower rather than the required 15% faster and doubles cached Metal allocation, so it is rejected. Actor supply exceeds learner consumption by more than fivefold. The stable live allocation and negligible second-half driver growth show no sustained leak.

Conclusion: Launch V51 from random initialization with batch 1,024 and BF16. Preserve all V50 artifacts and do not migrate weights, optimizer state, counters, stage state, or replay.

Next: Start a fresh shared A0/+24 V51 run with new training and promotion seed namespaces while retaining the existing PPO objective, replay, and curriculum settings.

## 2026-08-27 — V51 production bootstrap

Experiment: Launch V51 from random initialization at A0/+24 with BF16, batch 1,024, 512 environments, four samplers, and the unchanged PPO objective. Test the inherited publication cadence and learning rate independently against the runtime utilization gate.

Results: Publishing every 32 updates left actors at policy version zero while the learner stalled at 30 accepted updates; by 65,536 decisions only `30,720/65,536` attempted rows had trained. Publishing every four updates bounded first-window lag but LR `3e-4` still trained only `23,552/32,768` rows because eight steps were rolled back. The final fresh launch keeps publication at four and halves only LR to `1.5e-4`. Its first two windows trained `65,536/65,536` attempted rows with zero stale, ratio, pre-KL, or post-KL drops. Throughput was `382` then `654` decisions/s, observed KL `0.00351` then `0.00155`, and mean floor rose `14.47→20.98` with no caps or empty actions.

Analysis: The initial failures were asynchronous bootstrap and update-size defects exposed by V51, not representation or architecture failures. Frequent publication alone fixed lag but not oversized early steps; the half-rate launch clears the required utilization and throughput gates while leaving the objective, replay, curriculum, and architecture unchanged.

Conclusion: The authoritative V51 run is `target/v51-hierarchical-a0-b24` with BF16, batch 1,024, publication every four updates, and LR `1.5e-4`. The two rejected bootstrap directories remain small systems traces and must not seed training.

Next: Continue the authoritative fresh run unchanged through the first matched 491,520-decision block before considering any further optimization or policy change.

## 2026-08-27 — V50 guarded recovery branch

Experiment: Branch from the robust A0/+24 checkpoint at `5,525,286` decisions with fixed progress `β=0.1`, target KL `0.004`, and weights published every four accepted updates. Keep every other V50 production setting unchanged. Promotion now writes a separate immutable stage-entry checkpoint before training the next stage.

Results: The startup-censored report was `0/132`, then the first clean `491,520` decisions produced `259/1,313` wins (`19.73%`) and floor `44.76`. Policy versions passed the prior failure region without collapse. One unsafe update was rolled back; cumulative drops were `2,048` pre-update and `12,287` post-update rows, while later windows remained viable.

Analysis: Faster publication prevents the stale-actor starvation seen when tightening KL only after collapse. The branch is healthy enough to continue and must re-earn the A0/+24 promotion normally.

Conclusion: Keep the guarded recovery settings and preserve the exact A0/+20 entry if promotion succeeds.

Next: Continue the branch unchanged until the fresh promotion gate fires or the declared decision target is reached.

## 2026-08-27 — V50 recovery temperature 0.8 and promotion

Experiment: After five stable guarded A0/+24 blocks, change only policy temperature from `1.0` to `0.8`, retaining target KL `0.004`, publication every four updates, fixed progress `β=0.1`, and replay weight `0.1`.

Results: The new temperature quickly produced rolling wins `[10,15,12,9,10]/32`. The fresh `128`-run gate passed with wins `[73,68,62,63,72]`, floors `[50.08,49.61,49.89,49.81,49.91]`, and zero caps. Promotion occurred at `8,263,682` decisions. The exact A0/+20 entry was saved before new-stage training as `stage-entries/01-000008263682.pt`; its SHA-256 matched `latest.pt` at promotion.

Analysis: Lower sampling temperature resolved the stage-0 conversion variance while the guarded learner remained stable. The separate entry checkpoint removes the earlier inability to recover a healthy promoted policy.

Conclusion: Accept temperature `0.8` and continue from the promoted A0/+20 state.

Next: Measure a complete clean A0/+20 block before changing another setting.

## 2026-08-27 — V50 recovery first A0/+20 block

Experiment: Continue the promoted recovery branch at A0/+20 with temperature `0.8`, target KL `0.004`, publication every four updates, and the existing optimizer/replay settings.

Results: Excluding each sampler session's startup-censored report and terminal partial report, the first `491,520` decisions produced `216/1,357` wins (`15.92%`), floor `45.15`, floor-51 reach `53.28%`, and `29.88%` conversion, with zero caps or empty actions. Thirds were `16.14%`, `13.22%`, and `18.53%` wins; the final third recovered floor `45.86`, reach `56.70%`, and conversion `32.68%`.

Analysis: The stage adapted without collapse and finished stronger than its middle third. The stage-local winning reservoir is still rebuilding, so this is not yet a mature plateau.

Conclusion: Keep the guarded recovery configuration unchanged.

Next: Follow the measured recommendation from the complete first A0/+20 block.

## 2026-08-27 — V51 live audit and V52 correction

Experiment: Audit the live V51 bootstrap, checkpoint integrity, hierarchy routing, candidate coverage, and dashboard fidelity after its first 262,144 decisions.

Results: V51 reached `262,144` decisions with zero caps or empty actions, mean floor `23.86`, no ratio/KL rollback, and about `550–740` decisions/s after warm-up. Cumulative stale rejection was `10,819/262,144`; the final window retained about `93%` of handled rows as mean policy lag reached `47.4`. Its immutable checkpoint, `latest.pt`, metadata, and SHA-256 agree. The audit found that STATE owner also held phase room, Enchant card type, choice filter, and reward subtype, incorrectly routing non-actors into actor pools; represented candidates also omitted several visible illegal actions. Feature `47`/model `52` reserves owner for actors, relocates those fields losslessly, routes only actor STATE kinds `18–22/25`, and represents all visibility-defined candidates while retaining a separate legality mask. An empty-actor MPS backward edge was also fixed. The rebuilt V52 probe passes exact categorical routing, finite BF16 forward/backward, query-only output/gradient equivalence, permutation/cache checks, and Rust parity `1.19e-7`; all `131/131` Rust tests pass.

Analysis: V51 was mechanically stable but semantically invalid, so its learning metrics are systems evidence only. The dashboard architecture and nested bands were correct; asynchronous trajectory dots used sampled-iteration coordinates on a training-decision axis, and the live process retained an obsolete pipeline sentence. Dashboard generation now anchors async dots to their report step and normalizes that historical V51 sentence.

Conclusion: Preserve V51 only as a benchmark. V52 is the corrected incompatible boundary.

Next: Run the batch-1,024 V52 runtime gate, then start fresh training only if it passes.

## 2026-08-27 — V53 exact typed-domain architecture

Experiment: Replace V52 with feature 48/model 53 before training: lossless u32/i32 domain schemas, ten shared card-zone Transformers, actor DeepSets, typed public collections, ordered recursive continuations, a sparse reverse-topological map DAG, ragged state fusion, simulator-derived candidate outcomes, and independent policy/value candidate menus.

Results: The Python model has 1,786,879 parameters at W128/H4/FF256. Exact integer fields remain u32/i32 through observation packing and replay, then use field-specific four-byte embeddings before autocast. Fusion receives one current-node context after map entry or the virtual entry root beforehand, plus one global map summary. The MPS/BF16 probe passes finite forward/backward, pack/replay equivalence, card and fusion query-only output/gradient equivalence, candidate permutation equivariance, ordered continuation sensitivity, unordered relic and map-renumbering invariance, edge sensitivity, illegal-present versus absent-padding separation, card-cache invalidation, and candidate-conditioned Rust value parity below `3e-7`.

Analysis: Domain-specific schemas remove the shared-token collisions and float identity loss. The value critic now conditions on the full visible menu independently of the policy menu; legality is encoded for both and applied as a policy mask only after scoring. Path candidates reuse their encoded destination node.

Conclusion: Accept V53 as the next incompatible representation boundary, but do not train it yet.

Next: Complete the full Rust semantic suite and the batch-1,024 late-game throughput, latency, memory, and cache gates; launch only after all pass.

## 2026-08-27 — V53 simulator-owned action expectations

Experiment: Derive every Play and Potion preview from a cloned simulator transition. Enumerate public random choices exactly up to 64 leaves; fall back to the existing symbolic fields for unsupported, explosive, paused, turn-changing, or combat-ending outcomes.

Results: Deterministic block, draw, discard, exhaust, and enemy HP loss now match `Game::step`. Hidden draws and random selections are probability-weighted without sampling, while the richer random-target distribution remains symbolic. All 136 Rust tests and the MPS probe pass, with Rust/Python parity `1.19e-7`. A 512-environment early-combat observation benchmark rose from `32.31` to `39.28 µs/env` (`+21.6%`); the V51 runtime gate measured more than fivefold actor headroom.

Analysis: The simulator now owns outcome semantics, removing learning-side guesses and avoiding seeded samples that would look exact while being noisy. The branch cap keeps preview cost bounded and Unknown preserves truthful fallback behavior.

Conclusion: Accept feature `48`/model `53` as an incompatible boundary. V52 weights must not seed V53.

Next: Run the V53 runtime gate, then start fresh training only if it passes.

## 2026-08-28 — V54 recursive and actor identity fix

Experiment: Audit V53 for structural aliases before training. Make continuation frame, parent, and path fields semantic exact inputs; retain canonical actor owner slots in actor encodings; and reserve `ACTION.u4 = u32::MAX` for every non-Path candidate so only Path actions receive destination-node context.

Results: Feature `49`/model `54` has `1,793,071` parameters. The focused probe distinguishes synthetic continuation trees that differ only in parent/path structure and actor sets that differ only in slot assignment, preserves these fields exactly through replay, and verifies Path adapter presence exactly matches Path candidates. The full MPS/BF16 probe passes with candidate permutation error `4.77e-7` and Rust/Python candidate-conditioned value parity `1.79e-7`; all `148/148` Rust library tests pass.

Analysis: V53 sorted continuation records before permutation-invariant fusion but omitted the structural fields from their encoder, so different recursive trees could collide. It also used zero as the default Path node, accidentally adding entry-node context to every non-Path action. V54 removes both collisions without changing the typed wire widths or hierarchy.

Conclusion: V53 is invalid for training; V54 is the corrected incompatible boundary.

Next: Run the V54 batch-1,024 late-game throughput, latency, memory, and cache gates before launch.

## 2026-08-28 — V55 semantic-domain model

Experiment: Replace byte-limb learning with one shared namespaced semantic table, multiplicative field-role scales, domain-specific normalized float channels, bottom-up continuation pooling, ordered power tuples, actor-target and Path joins, count/log-count pooling, and simulator-derived 20-float candidate outcomes. Keep W128/H4/FF256, sparse maps, ragged query-only fusion, independent policy/value menus, BF16, and batch 1,024. Add optional same-version bootstrapped segments for a later matched staleness test; full trajectories remain the default.

Results: Feature 50/model 55 has 1,890,947 parameters and a 3,353-row semantic vocabulary. The full probe passes exact typed pack/flat replay, within-namespace field-swap separation and gradient isolation, recursive continuation non-aliasing, candidate permutation, illegal-present versus padding, unordered collection and map checks, target/Path joins, cache invalidation, query-only output/gradient, finite MPS BF16 backward, and candidate-conditioned Rust parity at `3.28e-7`. Candidate permutation error is `4.17e-7`; all 161 Rust library tests pass. Runtime pilots also caught and fixed noncombat HP previews, duplicated potion-replacement actions, invalid continuation child depths, shifted crystal-item semantics, fixed-card-reward namespace routing, and position overflow through the configured 512-step combat horizon.

Analysis: Semantic identity is shared without losing field roles, numeric ratios remain float32 until autocast, and exact routing integers never become learned inputs. Same-version segment bootstraps avoid mixing a newer critic with older behavior rows; arrival lag and stale-step exposure are now reported.

Conclusion: V55 passes correctness and parity gates. V54 artifacts remain immutable and V55 must start fresh.

Next: Compare full trajectories with segmented bootstraps from one identical V55 step-zero checkpoint, then launch the selected mode while continuing representative runtime profiling.

## 2026-08-28 — V55 matched staleness pilot

Experiment: Run full trajectories and 128-decision bootstrapped segments from the same fresh V55 initialization for 131,072 decisions each, with identical sampler seeds and all optimizer settings fixed.

Results: Full trajectories trained `129,024/131,072` attempted rows (`98.44%`) versus `120,832/131,072` (`92.19%`) for segmented rollouts. Full trajectories had zero pre-KL and `2,048` post-KL drops; segmentation had `8,192` pre-KL and `2,048` post-KL drops. Full training ran at about `974` decisions/s with `1.08s` p95 updates and `2.74GB` peak MPS driver allocation, versus `795` decisions/s, `1.22s`, and `5.00GB` for segmentation. Segmentation reduced mean arrival lag `86.34→59.58`, maximum arrival lag `123→89`, and final tail discard `29.9%→19.3%`, but both modes had zero stale rows at the matched boundary. Four-window weighted mean floor was `19.77` for full trajectories and `18.64` for segmentation; both had zero wins and no caps or empty-action failures.

Analysis: The shorter segments improved freshness, but the full-trajectory baseline already stayed below the lag cutoff during the matched interval. The added bootstraps increased KL rejection, memory use, and wall time without demonstrating a learning benefit.

Conclusion: Keep `--segment-steps 0`. The instrumentation remains enabled so segmentation can be retested if long-run stale loss becomes material.

Next: Launch a fresh V55 A0/+24 run with BF16, batch 1,024, LR `1.5e-4`, target KL `.004`, temperature `.8`, publish every four updates, and progress shaping held through all A0 stages.

## 2026-08-28 — V55 fresh A0/+24 launch

Experiment: Start `target/v55-semantic-a0-b24` from fresh weights, optimizer, replay, and seed namespaces with BF16, batch 1,024, four 128-environment samplers, full trajectories, LR `1.5e-4`, target KL `.004`, temperature `.8`, and publish every four updates.

Results: The run is live. Through 65,536 decisions it has zero caps, empty actions, or stale rows; `61,440/65,536` attempted rows trained (`93.75%`). The latest report is about `980` decisions/s end to end, `4,092` actor decisions/s, `1,178` learner rows/s, `2.53GB` MPS driver allocation, observed KL `.00103`, and mean floor `21.55`.

Conclusion: The new boundary is operational under production settings. Continue training unchanged and retain the staleness instrumentation.

## 2026-08-28 — V56 integrated architecture

Experiment: Replace the invalid V55 boundary with feature 51/model 56. Keep exact public semantic IDs and numeric quantities separate, repair card lineage, public pile order, replay-oracle leakage, encounter depletion, event associations, and delayed-effect metadata, then factor policy as verb → public object → actor target. Stabilize PPO with replay-capacity normalization, pre-backward KL rejection, verified backtracked steps, sampler watchdogs, bounded transport, stage-local shaping, and independent held-out evaluation.

Results: V56 has `1,997,061` parameters at W128/H4/FF256. Policy and value menus attend target-cardinality-invariant verb summaries; three conditional scorers produce each leaf probability. The full Python probe passes hierarchy, permutation, cache, query-only attention, finite precision, packing, and Rust value parity at `1.19e-7`. All `175/175` Rust tests pass. The final 8,192-decision hierarchical smoke trained all `7,285` actionable rows, excluded `907` forced rows from PPO, accepted all eight full updates, had no stale/ratio/KL drops or worker restarts, used `2.54GB` peak MPS driver memory, and recorded `2.45s` p95 updates. Earlier flat-policy artifacts are preserved under `target/v56-rejected-flat-*` and are not valid training seeds.

Analysis: V55's long-run collapse was dominated by replay-amplified policy jumps and discarded updates, compounded by a wedged sampler, flat-leaf multiplicity bias, and observation aliases/leakage. V56 excludes forced single-action rows from PPO while retaining their critic targets, republishes critic-only updates at the unchanged policy version, reconstructs critic gradients after rejected proposals, and initializes restarted samplers at the current policy version. Progress shaping resets per stage, remains active through A0/+0, and stops on promotion to A1.

Conclusion: V56 is the new incompatible boundary. No V54/V55 weights, optimizer state, or replay migrate.

Next: Launch `target/v56-semantic-hierarchical-a0-b24` from fresh seed namespaces at BF16, batch 1,024, LR `1e-4`, target KL `.004`, temperature `.8`, and publication every accepted update.

## 2026-08-28 — MODEL57 flat-candidate correction

Experiment: Keep the FEATURE51 public observation schema, but restore the approved flat decision path under MODEL57: both menus attend every complete represented candidate and one shared MLP scores `[hπ, a, hπ×a]`. Progress beta uses the run-global auxiliary-decision clock while entropy remains stage-local.

Result: The Python model is 1,898,243 parameters. All V56 checkpoints and traces are benchmark-only because V56 was used for incompatible flat and hierarchical policies; none may seed MODEL57.

Next: Rebuild the MODEL57 Rust extension, pass the full correctness/parity/runtime gates, then launch fresh weights, optimizer, replay, and seeds with BF16, batch 1,024, LR `1.5e-4`, target KL `.004`, temperature `.8`, four-update publication, and the approved four-sampler/two-Torch-thread/one-sampler-thread topology.

## 2026-08-28 — MODEL57 fresh launch

Experiment: Rebuild FEATURE51/MODEL57, run the full MPS probe, and start a new A0/+24 run with random weights, fresh optimizer/replay, and disjoint seed namespaces. Use the V56 benchmark-selected entropy schedule `.003 → .001`; migrate no checkpoint state.

Results: The rebuilt probe passes at `1,898,243` parameters with candidate permutation error `3.58e-7` and Rust/Python value parity `5.96e-8`. The fresh run records `source=null` and `optimizer_restored=false`. Its first report trained every attempted row with no KL, ratio, stale, cap, or empty-action failures. A watchdog respawn then exposed two cleanup bugs: the replacement actor could retain an unpickleable MPS tensor, and shutdown joined a process whose spawn had failed. Respawn now constructs a fresh CPU `Agent` from a CPU state dictionary, and shutdown considers only successfully started workers; the probe pickles that exact actor snapshot.

A later reachable `Effect::Select` exposed a semantic-slot bug: its two integer counts occupy `u14/u15`, while its booleans occupy `u16/u17`. The encoder had incorrectly treated the counts as Boolean values and panicked when a count exceeded one. A regression now exercises counts `[2,3]` and both Boolean toggles.

The patched run resumed from its report-backed step `32,768`. Fixed development mean floor improved from `2.09` at step `294,912` to `21.44` at `557,056`; caps fell from `21` to `14`, and every character escaped the floor-2 attractor. The run still had no deterministic boss entries or wins at that checkpoint.

At step `819,200`, fixed development reached `2/160` wins, mean floor `35.09`, and `19` boss entries spread across all five characters; caps fell to one. This is broad greedy-policy learning rather than a stochastic-only improvement.

At step `1,081,344`, fixed development reached `8/160` wins, mean floor `36.71`, `44` boss entries, and zero caps. Four characters recorded wins; Silent remained the sole conversion bottleneck with four boss entries and no win.

At step `1,343,488`, fixed development reached `19/160` wins, mean floor `39.00`, `59` boss entries, `32.2%` boss conversion, and zero caps. All five characters recorded wins; the weakest per-character counts were three of 32.

Conclusion: MODEL57 is benchmark-only because its learned continuation semantics are incompatible with the corrected mapping.

Next: Continue unchanged to step `1,605,632`; watch the weakest characters move toward the `20%` promotion threshold.

## 2026-08-28 — MODEL58 Select semantics correction

Experiment: Keep FEATURE51 and the flat MODEL57 architecture, correct `Effect::Select` Boolean routing from count slots `u14/u15` to flag slots `u16/u17`, and assign the incompatible learned semantics a new MODEL58 identity.

Results: The focused regression covers counts `[2,3]` plus both Boolean toggles. All `175/175` Rust tests, Python compilation, and the rebuilt MPS probe pass with `1,898,243` parameters, candidate permutation error `3.58e-7`, and Rust/Python value parity `5.96e-8`. Fresh production started with `source=null` and `optimizer_restored=false`; its first report at step `32,768` accepted all 32 policy updates with zero KL, ratio, stale, cap, empty-action, or sampler-wedge failures.

At the first immutable checkpoint, step `262,144`, the fixed development panel reached mean floor `11.23` with zero wins, zero boss entries, and `14` caps. The matching training window reached stochastic floor `20.67`, one boss entry, `96.0%` row utilization, observed/post KL `.00078/.00063`, `8,627` pre-KL rejects, and `5,513` stale rows.

A separate 16×128 FP32 cached-head gate was not adopted. Its startup-censored report reached actor `3,713` decisions/s, learner `1,279` rows/s, p95 update `1.106s`, and 100% utilization, but only `519` end-to-end decisions/s because host-side actor work contended with MPS submission. Production resumed with the canonical BF16 4×128 topology.

The canonical step-`524,288` development panel was terminated after more than twelve minutes of active evaluation; its immutable checkpoint SHA is `d7879347ad78a8799620cb5aaa01711c4cd45fdcaca48af24c860c3235573c8e`, but the panel is incomplete. A bounded 12×128 FP32 CPU-learner gate from that checkpoint was also not adopted: its warm window reached `1,105` end-to-end decisions/s, actor `3,278`, learner `1,947`, p95 update `.926s`, and 100% utilization with zero drops, still below the required sustained throughput. Policy-only evaluation remained exact and measured `2.43×` faster off-tree.

A sustained 12×128 FP32 native-ragged-attention gate from report-backed step `819,200` was also not adopted. It reached actor supply around `3,700` decisions/s, learner throughput up to `3,321` trained rows/s, p95 updates near `.33s`, and one warm window at `3,016` end-to-end decisions/s. However, attempted-row utilization fell from 100% to `39%` as pre-KL rejection accumulated, and later end-to-end windows returned near `2,450` decisions/s. The kernel remains promising but lacks a stable >3,000 end-to-end result under the production trust-region workload.

A follow-up four-thread CPU/head-factorization gate from immutable step `1,081,344` was invalid before sampling: no sampler child started, no report was produced, and termination cleaned leaked multiprocessing semaphores. It is excluded from performance evidence and was not adopted.

The corrected four-thread/native-attention/head-factorization gate from report-backed step `1,245,184` was also not adopted. Its first `65,536`-decision window was startup-censored at `579` end-to-end decisions/s despite actor `3,465` and learner `3,848`; the next complete window reached only `621` end-to-end because the accepted attempt-64 trunk update invalidated every queued cached feature and the learner waited for new-version complete trajectories. Exact on-demand refresh of selected old-version rows is the next off-tree target.

Conclusion: MODEL58 is the sole current model. Every MODEL57 checkpoint remains an immutable learning benchmark and must not seed MODEL58.

Next: Continue canonical MODEL58 unchanged from report-backed step `819,200`. Keep the native attention and evaluator work off-tree until a sustained end-to-end gate passes without sacrificing attempted-row utilization.

## 2026-08-28 — FEATURE52/MODEL59 public-state correction

Experiment: Start a fresh incompatible boundary that removes private card-instance/deck-lineage identity from learned card, Nightmare, and continuation features while retaining exact routing fields; adds total, known-top, and known-bottom exact/log counts to every card-zone summary; and derives incoming intent damage from executable move effects with the simulator's enemy-to-player modifiers and per-hit caps.

Results: Focused Rust regressions prove private lineage changes leave model-visible observations and identical-copy successors unchanged while exact master routing remains present, draw summaries expose all six count channels, and modified/capped incoming damage equals the synchronized transition. Python compilation, `cargo check --all-targets --features python`, and all `177/177` Rust tests pass. The rebuilt MPS probe passes with `1,898,627` parameters, candidate permutation error `2.38e-7`, and Rust/Python value parity `1.34e-7`. Fresh production records `source=null` and `optimizer_restored=false`; its first report trained all `27,523` attempted rows with zero KL, ratio, stale, cap, or empty-action failures.

Conclusion: FEATURE52/MODEL59 is incompatible with MODEL58. Every MODEL58 artifact and its matched LR branches remain benchmark-only. MODEL59 started from fresh weights, optimizer, replay, and disjoint seeds.

Next: Continue MODEL59 unchanged to its first immutable checkpoint and fixed development gate.

## 2026-08-29 — FEATURE53/MODEL60 direct-state restoration

Experiment: Replace MODEL59's W128 learned fusion state with the requested literal geometry: 3,605 direct public float32 globals, 13 distinct card-zone summaries from one shared W64/L2/H4/FF128 Transformer, ten typed W64 entity summaries, one W64 sparse-map summary, and a fixed 5,141-dimensional concatenation. Encode complete actions at W64 and score each candidate directly from `[state, action]` through a 112-wide head. Preserve MODEL59's typed semantic/numeric fields, exact replay transport, public target/path joins, continuation tree, and no-private-lineage corrections.

Results: Feature 53/model 60 has `2,195,507` parameters. The observation digest now binds the 3,605 direct globals, padded and flat observations round-trip identically, and the exported Rust critic consumes the same 5,141 coordinates with no fusion or candidate-menu weights. All `178/178` Rust tests pass. The rebuilt FP32/MPS-BF16 probe passes direct-global prefix equality, 13-zone count channels, semantic and numeric sensitivity, continuation structure, contextual targets, candidate permutation, legality/padding equality, replay corruption rejection, finite backward, cache reuse, and Rust/Python value parity at `1.04e-7`; candidate permutation error is `1.19e-7`.

The fresh first block reached immutable step `262,144` (SHA256 `86b2d8f30d8266f370d7201b47da68f57e27a4c39fcfee4e111408f5fbe49a5c`) and drained to actual step `262,849` (SHA256 `4aeb251da74a6690deb95696b1d0431675c9ae5ef1dc658a41e89e524b0ca9f5`). It trained `184,744/213,795` attempted rows (`86.41%`), with `28,801` pre-KL, `250` ratio, `6,351` stale, and zero post-KL drops; final observed/post KL was `.003912/.003690`. The fixed 32-run-per-character development panel produced `15/160` wins, mean floor `36.01`, `41` boss entries, and zero caps. Every character won; per-character wins were `[4,4,5,1,1]`.

Conclusion: MODEL59 artifacts remain immutable learning benchmarks and cannot seed MODEL60. MODEL60 is the fresh incompatible boundary.

Next: Start MODEL60 from random weights, optimizer, and replay with disjoint seeds, then evaluate the first immutable 262,144-decision checkpoint before changing the requested LR `3e-4`, clip `.2`, grad clip `.5`, entropy schedule, or full-trajectory PPO settings.

## 2026-08-29 — MODEL60 publication and fused-head gates

Experiment: From immutable step `951,085`, compare full-trajectory publication every four updates with publication every update for `131,072` decisions per arm. Separately gate an FP32-only algebraic fusion of the three direct heads and factor the policy state projection without changing the model or PPO schedule.

Results: Publish-4 and publish-1 both dropped zero stale rows. Publish-1 slightly increased policy-span/arrival maxima (`104/108` versus `100/104`), added `811` post-KL drops, and reduced clean actor/end-to-end throughput by about `3–5%`. Its fixed panel had `70/160` wins, floor `47.39`, and `127` boss entries versus `63/160`, `48.31`, and `136` for publish-4; the mixed learning result does not establish a freshness benefit. The fused FP32 head passed full-model output/gradient parity at `2.38e-7/3.81e-6` and improved isolated B1024 forward/backward `1.690x`, but sustained full-current PPO reached only `771` median end-to-end decisions/s, with learner throughput `287–358` rows/s. BF16 fusion failed parity and was rejected.

The publish-1 terminal panel also reproduced a queue-tail deadlock: the shutdown watchdog terminated samplers blocked flushing packets, then `Queue.get_nowait()` blocked on a partial pipe record. Terminal conditions now bypass watchdog termination so the main loop drains healthy producers, and final cleanup no longer reads the shared queue after forced termination. The feeder-backed shutdown regression, Python compilation, diff check, and full MPS probe pass; MODEL60 remains `2,195,507` parameters with candidate permutation error `1.19e-7` and Rust parity `1.04e-7`.

Conclusion: Keep publish every four updates. Reject the fused-only FP32 configuration and retain canonical MODEL60 learning settings.

Next: Resume publish-4 from step `1,082,656` and allow the configured rolling trigger and fresh promotion evaluation to decide the A0/+24 transition.

## 2026-08-29 — FEATURE54/MODEL61 positional correction

Experiment: Preserve MODEL60 as a benchmark and add the missing bounded public position embeddings without changing the W64 direct-state architecture. Known draw-top/bottom cards receive a role-bound rank embedding; map nodes receive distinct floor and lane embeddings; continuation rows receive distinct branch, path/depth, list, and sibling-order embeddings. Exact frame and parent-frame IDs remain structural references only, and all existing normalized coordinates remain.

Results: Card, continuation, and map-node semantic widths are `73`, `80`, and `7`. Focused Rust coverage proves top/bottom distinction, hidden/non-draw exclusion, bounded saturation, map role swaps, all four continuation roles, and frame/parent relabel invariance. All `179/179` Rust tests, Python compilation, diff checks, and the rebuilt full MPS probe pass. MODEL61 has `2,195,955` parameters, candidate permutation error `1.19e-7`, and Rust/Python value parity `5.96e-8`.

MODEL60's final benchmark at step `1,213,728` trained `103,958/104,775` attempted rows with zero stale and post-KL drops. Its stochastic window reached `23/85` wins and floor `44.59`; fixed development reached `46/160` wins, floor `46.96`, `127` boss entries, and zero caps. A schedule-preserving fast-trust optimization passed FP32/BF16 parity but improved accepted-first timing only `3%` and slowed forced retries, so it was not adopted.

Conclusion: FEATURE54/MODEL61 is the corrected incompatible boundary. No MODEL60 weights, optimizer state, replay, or seed namespace migrate.

Next: Start MODEL61 fresh at A0/+24 with BF16, batch `1,024`, LR `3e-4`, clip `.2`, target KL `.004`, temperature `.8`, publish every four updates, full trajectories, and the unchanged 5M progress/entropy schedule. Use step `262,144` as the first immutable 160-run development gate.

## 2026-08-29 — MODEL61 fresh A0/+24 launch

Experiment: Train FEATURE54/MODEL61 from random weights, a fresh optimizer and replay, and new seed namespaces with the unchanged MODEL60 PPO settings.

Results: The manifest records `source=null`, `optimizer_restored=false`, BF16, batch `1,024`, LR `3e-4`, target KL `.004`, publish every four updates, and full trajectories. At immutable step `262,144`, fixed development reached `37/160` wins, mean floor `40.74`, `78` boss entries, and seven step caps; every character won. MODEL60's comparable first gate was `15/160`, floor `36.01`, and `41` boss entries. Training reached stochastic floor `26.91` with one win, trained `171,152/214,831` attempted rows (`79.67%`), and dropped `5,200` stale rows (`1.98%` of seen rows), `634` ratio rows, `40,469` pre-KL rows, and `2,576` post-KL rows. Final observed/post KL was `.003703/.003820`; no training cap, empty-action, or sampler-wedge failure occurred.

The unchanged continuation reached step `393,581` with `107,413/107,414` attempted rows trained, zero stale, pre-KL, or post-KL drops, one ratio drop, and observed/post KL `.003156/.001891`. Its fixed panel produced `36/160` wins, mean floor `42.63`, `74` boss entries, and zero caps. Terminal shutdown then exposed an accounting-only queue race: a cleanly exited worker could leave a complete packet undrained after the queue-empty check, while progress reconciliation covered only forcibly terminated workers. Shutdown now reconciles every worker against its authoritative shared progress counter without reading a possibly partial queue record. A bounded continuation reproduced overshoot and terminal drain and exited with exact one-pass accounting.

The repaired full continuation drained cleanly to step `525,390` with `107,631/107,631` attempted rows trained and zero stale, ratio, pre-KL, or post-KL drops. The final complete stochastic window reached `9/95` wins, mean floor `39.26`, and `27` boss entries with no cap. Fixed development improved to `62/160` wins, mean floor `43.13`, `106` boss entries, and zero caps; per-character wins were `[15,11,14,11,11]` and mean floors `[44.00,39.53,42.38,46.16,43.59]`.

Conclusion: The positional correction is learning better than the MODEL60 benchmark. The marginal utilization miss and fixed-panel caps warrant another unchanged evidence block, not a confounded hyperparameter change.

Next: Keep the canonical rolling trigger authoritative. Do not manually promote from the development panel; continue unchanged until it launches the fresh 256-run-per-character promotion gate.

## 2026-08-29 — FEATURE55/MODEL62 compact action-aware state

Experiment: Replace MODEL61's 5,141-dimensional direct state with a compact typed state. Keep only deck, hand, draw, discard, and exhaust card zones. Encode powers and statuses through their existing domain encoders and pool them as unified owner-bound actor effects. Pool player with optional Osty and pool enemies separately. Replace the global map summary with a whole-map-aware contextual current or entry node. Remove raw globals and offers from state; summarize every visible legal or visible-illegal action through explicit target-independent object IDs and one state-conditioned attention query.

The resulting model has `920,611` parameters. Seventeen independently normalized 64-dimensional blocks form a `1,088`-dimensional base state: five card zones; run, phase, party, enemies, relics, potions, orbs, events, encounters, crystal, and continuations; and the current map node. A 64-dimensional action-object menu produces the final `1,152`-dimensional state. Card, collection, continuation, candidate, target, and object counts use `count/64` and `log1p(count)/5` features.

Results: The first adversarial review found unbalanced state blocks, premature verb-level menu averaging, a state residual that overwhelmed menu content, target-dependent grouping, missing object counts, off-route Winged Boots map aliases, and incomplete active or potion-suspended reward state. The corrected design normalizes every base block, attends directly over target-averaged objects, removes the menu query residual, uses simulator-supplied object IDs, includes count features, lets the selected node query every public contextual map node, and preserves ordered active and suspended rewards. Actor-side duplicate digest validation was also removed while learner-boundary validation remains mandatory, reducing a measured heterogeneous 128-state actor cycle from about `268 ms` to `174 ms`.

All `181/181` Rust library tests pass. The full MPS probe passes with candidate permutation error `1.19e-7` and Rust/Python value parity `5.96e-8`. A second adversarial pass found no leakage, public-state collision, indexing, parity, or scaling blocker; an A10 sweep covered `8,192` live decisions and 131 resets. A fresh FP32 training smoke reached 719 accepted decisions, 12 accepted optimizer updates, 100% attempted-row utilization, zero KL/ratio/stale/post-KL drops, finite losses, and a `91.6%` graph-cache hit rate.

Conclusion: FEATURE55/MODEL62 is a fresh incompatible architecture and is ready for a new training run. MODEL61 artifacts remain immutable benchmarks and cannot seed MODEL62.

Next: Start MODEL62 from random weights, optimizer, replay, and disjoint seeds. Treat partial graph-cache hits, removal of the obsolete candidate-token construction path, richer minibatch cost estimates, and Rust allocation reductions as performance follow-ups rather than launch blockers.

## 2026-08-29 — MODEL62 throughput work

Experiment: Preserve MODEL62 and PPO exactly while removing redundant legacy action-token construction, reusing partial actor graph caches, coalescing CPU-to-MPS transfers, buffering eight sampler packets, and bucketing variable training tensors. A fused Metal semantic encoder replaces the equivalent embedding, numeric projection, normalization, and ReLU graph. Release-mode Rust is required for training.

Results: The first isolated canonical 32,768-decision gate, before shape bucketing and semantic fusion, reached `523.7` end-to-end decisions/s, actor `3,266.2`, learner `625.2`, and `100%` row utilization, with zero stale, ratio, pre-KL, or post-KL drops. It was `1.60x` the prior isolated `328` baseline. The complete patch reached `653.0` end-to-end, actor `2,650.0`, and learner `882.9` decisions/s with `100%` utilization and no drops. Focused exact BF16 updates reached `1,699` actionable rows/s at batch `2,048` with `0.36/1.81 GiB` current/driver MPS memory, and `2,330` at batch `4,096` with `0.43/2.96 GiB`. Post-step KL verification no longer computes an unused detached entropy diagnostic. Padded and unpadded MPS outputs are identical; maximum gradient difference is `7.5e-9`. All `181/181` Rust tests and the full MPS probe pass with candidate permutation error `1.49e-7` and Rust/Python value parity `5.96e-8`.

The isolated two-window batch-`4,096` gate reached `659.4` end-to-end decisions/s in the complete-trajectory startup window and `1,228.9` in the second window. The second window added eight accepted updates at `100%` utilization with zero stale, ratio, pre-KL, post-KL, or proposal-retry drops. Cumulative actor and learner rates were `2,722.5` sampled decisions/s and `1,364.6` actionable rows/s. Observed, mean pre-step, and mean post-step KL were `6.56e-6`, `7.69e-6`, and `7.94e-6`. Peak MPS allocation was `0.348 GiB` current and `4.117 GiB` driver.

Conclusion: The patch is semantically clean and clears the `1,000` end-to-end decisions/s gate at batch `4,096`. A custom Metal scatter-sum path was rejected because it matched native `index_add` throughput.

Next: Use batch `4,096` and a release Rust extension for MODEL62 training. Keep the 32,768-decision report window; treat its first complete-trajectory window as startup and require subsequent windows to remain above `1,000` end-to-end decisions/s without utilization or trust-region regression.
