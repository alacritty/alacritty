# AUR packaging

Two packages live in the [Arch User Repository](https://aur.archlinux.org/):

- **`alacritree-git`** — VCS package. Builds from the latest `master`;
  `pkgver()` derives a monotonically-increasing version from `git rev-list
  --count` until the project starts tagging releases. Synced to AUR by
  [`.github/workflows/aur-publish.yml`](../../.github/workflows/aur-publish.yml)
  on every push to `master`. The deploy action no-ops when the PKGBUILD
  is byte-identical to what's already on AUR, so users only see commits
  when the recipe actually changes.

- **`alacritree-bin`** — prebuilt-binary package. Downloads the `x86_64`
  / `aarch64` Linux tarballs from the corresponding GitHub release. Synced
  to AUR by [`.github/workflows/aur-bin-publish.yml`](../../.github/workflows/aur-bin-publish.yml)
  on every `release: published` event. The workflow stamps the PKGBUILD
  with the release version and asset sha256 sums before handing it off
  to the deploy action.

## One-time setup

1. **Generate an SSH key dedicated to AUR** (no passphrase, so the Actions
   runner can use it non-interactively):

   ```sh
   ssh-keygen -t ed25519 -f ~/.ssh/aur -C 'aur@alacritree-ci' -N ''
   ```

2. **Register the public key on AUR**: copy `~/.ssh/aur.pub` into your
   account's *SSH Public Key* field at <https://aur.archlinux.org/account/>.

3. **Reserve both package names** by pushing an initial commit yourself
   (the GitHub Actions assume the AUR repos already exist). Repeat the
   block below for each of `alacritree-git` and `alacritree-bin`:

   ```sh
   pkg=alacritree-git   # then repeat with pkg=alacritree-bin
   GIT_SSH_COMMAND='ssh -i ~/.ssh/aur' \
     git clone ssh://aur@aur.archlinux.org/$pkg.git /tmp/aur-$pkg
   cp packaging/aur/$pkg/PKGBUILD /tmp/aur-$pkg/
   cd /tmp/aur-$pkg
   makepkg --printsrcinfo > .SRCINFO
   git add PKGBUILD .SRCINFO
   git commit -m 'Initial import'
   GIT_SSH_COMMAND='ssh -i ~/.ssh/aur' git push
   cd -
   ```

4. **Expose the private key to Actions** as a repository secret named
   `AUR_SSH_PRIVATE_KEY` (Settings → Secrets and variables → Actions).
   Paste the full contents of `~/.ssh/aur` including the BEGIN/END lines.

Subsequent updates are automatic:
- `alacritree-git` republishes whenever its PKGBUILD changes on `master`.
- `alacritree-bin` republishes whenever a GitHub release is published —
  the workflow refuses to publish if Linux assets are missing, so a
  release with a broken Windows-only build will still produce a valid
  `alacritree-bin` bump as long as both `x86_64-linux` and `aarch64-linux`
  tarballs are attached.
