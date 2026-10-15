# Public alpha implementation plan

Status: in progress. Target: Linux x86-64 alpha with Claude Code and Codex as primary integrations, and optional experimental Antigravity and Muse integrations.

## Preservation and rollback

- Develop in a separate checkout. Preserve the existing checkout, installed binary, configuration and database.
- A private backup contains a source snapshot including uncommitted edits, Git history, the stable binary and an integrity-checked SQLite backup.
- Use a separate preview application identifier and separate `code-ui-preview` configuration/data directories for native verification. Never install over the stable binary during preparation.
- Keep release preparation on its own branch in the existing private remote. Create the public repository from an explicitly reviewed snapshot, without private history.
- Publication is the final review step after code, documentation, CI and downloadable artifacts are concrete.

## Stages and acceptance gates

1. **Permissions and privacy** (implemented and verified)
   - Ask by default for new installations; preserve explicit saved permissions.
   - Approving a plan enters Ask mode rather than silently granting full access.
   - Secret answers are excluded from retained interaction values and diagnostics.
   - App data and diagnostic files use owner-only permissions.
   - Verify redaction, legacy-answer handling, plan transition and isolated preview paths.
2. **Settings and onboarding** (implemented and locally verified)
   - Choose/create a projects folder, enabled agents, default agent/permission and optional history discovery.
   - Support executable overrides, repeatable detection and recovery from invalid settings.
   - Require explicit trust before a newly added project's first agent turn.
   - Preserve existing settings and conversations. Verify fresh and migrated profiles.
3. **Compatibility and portability** (implemented; native CI verification follows in stage 5)
   - Work with zero agents, Claude only, Codex only and optional agents.
   - Respect CLI data-directory overrides and valid discovery working directories.
   - Make Linux desktop notifications and the GPU workaround optional.
   - Publish tested versions and truthful feature limitations. Test process cleanup with fake CLIs.
4. **Public tooling and documentation** (implemented and locally verified)
   - Portable screenshot tooling, documented optional probes, toolchain pins and standard check commands.
   - README, setup/troubleshooting, architecture, security/privacy, contributing, licenses and roadmap.
   - Neutral demo data and screenshots. Exclude private plans, old logs, local reference material, temporary files and build artifacts from the public snapshot.
5. **CI and packaging** (implemented; hosted verification in progress)
   - Locked clean builds and checks on a documented Linux baseline.
   - Generate AppImage and Debian packages, checksums and bundled third-party notices.
   - Verify local artifacts and hosted CI. Record any native or clean-machine checks that remain unavailable.
6. **Fresh-history repository and alpha**
   - Scan and inspect the allowlisted public snapshot, initialize new history and create the new repository privately.
   - Push source, run hosted CI and prepare a draft prerelease with verified assets.
   - Present the exact repository and draft release for final public visibility/release review.

## Scope

Keep the current architecture and dark theme. Provider installation, provider login, automatic updates, macOS/Windows support and new agent protocols are outside this alpha. Installed CLI detection does not establish authentication. Automated checks must use synthetic fixtures/fake CLIs rather than paid model calls.

## Progress

- Preservation complete: source archive, binary checksum, SQLite integrity and Git bundle verified.
- Baseline from the audit: 58 Rust tests, 19 frontend tests, frontend build, formatting and Clippy passed; npm audit reported no known vulnerabilities.

- Stage 1: 64 Rust tests and 19 frontend tests passed; frontend build, formatting and Clippy passed. Added regression checks for legacy secret redaction, trace suppression and owner-only permissions.

- Stages 2–3: 71 Rust tests, 21 frontend tests, frontend build and Clippy passed. Browser checks passed setup, one-agent defaults, project trust/cancel, persistence, no-agent behavior, minimum-size footer visibility and console checks. Preview storage now follows the compiled identifier even without an environment variable. Native WebDriver verification will run in CI because this shell has no display server.

- Stage 4: added public documentation, synthetic screenshots, pinned toolchains, portable probes and screenshot tooling, strict bundled license generation and an allowlisted fresh-history export. The exported snapshot passed personal-path, credential-pattern, retired-reference, local-file and metadata checks. Browser screenshot checks reported no console errors, undersized text or horizontal overflow across six states.
- Stage 5: added Ubuntu 22.04 checks, dependency security scans, browser and isolated native fake-CLI checks, AppImage/Debian packaging, artifact inspection and checksums. The release workflow creates a draft prerelease. Hosted results are pending.
