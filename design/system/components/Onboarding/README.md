# Onboarding

First-run flow: language, PO-provider check, optional login, music folder, finish (sets `updates.onboarding_done`).

- `surface-container-high` card at `radius-extra-large`, a segmented progress bar where the current step is twice as wide, step label `label-medium`, title `headline-medium`.
- Choices as 56px `radius-large` tiles; selected = `secondary-container` with a check.
- Skip (text) and Next / Finish (filled); Esc never dismisses the last step.
- Slint: `OnboardingOverlay` in `pages/settings/onboarding.slint`.
