# Categorical critic

- Add `canonical_progress(state) -> 0..52`:
  - Act 1: `run.floor`.
  - Act 2 with Golden Compass: `15 + run.floor`.
  - Normal Act 2: `15 + run.floor + (run.floor >= 7) + (run.floor >= 10)`, skipping canonical slots 7 and 11.
  - Act 3: `34 + run.floor`.
  - Assert that the result remains in `0..52`.
- Label each completed trajectory with `Z = 53` on victory and `Z = max(canonical_progress)` on loss.
- Replace the scalar win/floor heads with one 54-logit categorical critic.
- Build detached critic targets backward over each complete trajectory:
  - `q[T - 1] = one_hot(Z)`.
  - `q[t] = (1 - lambda) * stop_gradient(p(s[t + 1])) + lambda * q[t + 1]`.
- Train with soft-target categorical cross-entropy: `L[t] = -sum_z q[t, z] * log p(s[t], z)`.
- Keep policy GAE at `1.0`; test critic lambda values `1.0`, `0.99`, and `0.97`.
- Lightly balance critic loss across floors / phases / characters: maintain EMA frequencies for critic rows that enter optimization, before weighting, using separate character, phase, and canonical-floor marginals.
- Weight each row by the geometric mean of the marginal inverse-square-root frequencies, clip initially to `[0.25, 4]`, and renormalize minibatch weights to mean one.
- Report pre-weight category counts, post-weight loss mass, and effective sample size: `ESS = sum(w)^2 / sum(w^2)`.
- Add a critic-only learning-rate multiplier and test `1x`, `2x`, and `4x` while keeping policy-head and trunk learning rates fixed.
- Select the critic configuration, then rerun the matched expectimax-Q expert-iteration experiment using its expected categorical value.