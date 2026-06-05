# Releasing

Releases are automated by [`.github/workflows/release.yml`](.github/workflows/release.yml),
triggered by pushing a `v*` tag.

## Cut a release

1. Bump the version in `Cargo.toml` and add a `CHANGELOG.md` entry. Commit.
2. Tag and push:

   ```sh
   git tag v0.2.0
   git push origin v0.2.0
   ```

3. The workflow then:
   - builds release binaries for `aarch64-apple-darwin`, `x86_64-apple-darwin`,
     and `x86_64-unknown-linux-gnu`;
   - packages each as a `.tar.gz` with a `.sha256`;
   - creates a GitHub Release with those assets and auto-generated notes;
   - regenerates the Homebrew formula in `turlockmike/homebrew-tap` with the new
     version, URLs, and checksums.

Once the release exists, `curl … | sh` and `brew install turlockmike/tap/taipan`
both pick up the new version automatically.

## One-time setup: the Homebrew tap token

The formula-update job pushes to a **separate** repo
(`turlockmike/homebrew-tap`), which the default `GITHUB_TOKEN` cannot do. Add a
repo secret named `TAP_GITHUB_TOKEN`:

1. Create a fine-grained Personal Access Token with **Contents: read & write**
   scoped to `turlockmike/homebrew-tap`.
2. Add it under **Settings → Secrets and variables → Actions** in
   `turlockmike/taipan-rs` as `TAP_GITHUB_TOKEN`.

If the secret is missing, the release still succeeds — the Homebrew-update job
skips itself, and you can update `Formula/taipan.rb` in the tap manually (bump
`version`, the three `url` tags, and the three `sha256` values from the
release's `.sha256` files).
