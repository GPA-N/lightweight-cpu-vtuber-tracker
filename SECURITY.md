# Security policy

## What this project does and does not touch

- Runs entirely on your machine. No network calls at runtime, no telemetry, no accounts.
- Webcam frames stay in memory. Nothing is sent anywhere. Only the opt-in debug variables (such as `AVATAR_SHOT`) write an image, and only when you set them.
- Models are downloaded once by `scripts/fetch_instanthmr.ps1`; they are not in the repository.

## Automated checks

Every push and pull request runs, and a weekly schedule repeats the security ones:

- Build and test with `--locked` (dependencies pinned by `Cargo.lock`).
- RustSec advisory audit of all dependencies (`cargo audit`).
- Secret scan of the full git history (gitleaks).
- CodeQL static analysis.
- Dependabot opens pull requests for outdated Cargo and GitHub Actions dependencies.

These find known advisories, leaked secrets and common code flaws. They do not prove the code is free of bugs. Read the source if you need more than that.

## Reporting a vulnerability

Use GitHub's private reporting: the repository's Security tab, then "Report a vulnerability". Please do not open a public issue for a vulnerability. Expect a first answer within a week.
