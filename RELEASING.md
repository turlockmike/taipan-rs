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
   - creates a GitHub Release with those assets and auto-generated notes.

Once the release exists, `curl … | sh` (and `cargo install --git …`) pick up the
new version automatically — `install.sh` always points at the latest release.
