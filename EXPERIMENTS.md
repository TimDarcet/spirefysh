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
