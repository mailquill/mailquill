# Contributing to Mailquill

Thanks for wanting to contribute! Mailquill is licensed under
**AGPL-3.0-or-later** (see [LICENSE](LICENSE)). A few things to know before you
open a pull request.

## Developer Certificate of Origin (required)

Mailquill stays **AGPL-3.0-or-later** — there is no relicensing and no CLA.
Instead, every commit must be **signed off** under the
[Developer Certificate of Origin](DCO.txt) (DCO 1.1). The sign-off is your
statement that you wrote the change, or otherwise have the right to submit it
under the project's license.

How it works:

1. Add a `Signed-off-by` line to each commit by committing with `-s`:

   ```bash
   git commit -s -m "fix: ..."
   ```

   This appends `Signed-off-by: Your Name <your@email>` using your git
   `user.name` / `user.email` — which must be a real identity.

2. Already committed without it? Add sign-offs to your branch:

   ```bash
   git rebase --signoff <base-branch>   # e.g. origin/main
   git push --force-with-lease
   ```

A CI check verifies that every non-merge commit in your PR is signed off; PRs
cannot be merged until it passes.

## Development

See [docs/dev-setup.md](docs/dev-setup.md) for getting the stack running and
[docs/](docs/) for architecture and data-layout docs.

Before opening a PR:

- **Build:** `cargo build` (backend) and `bun run build` (frontend) succeed.
- **Tests:** `cargo test` passes. Rust tests live in `tests/` integration files,
  not inline `#[cfg(test)]` modules.
- **Validation:** validate every API input server-side (see the backend
  validation layer) — never trust client input.
- Keep changes focused; match the style and conventions of the surrounding code.

## Reporting security issues

Please do **not** open a public issue for security vulnerabilities. Email
**fg@fgehann.de** instead.
