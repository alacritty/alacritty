# AUR packaging

`alacritree-git` is the VCS package pushed to the
[Arch User Repository](https://aur.archlinux.org/) — it builds from the
latest `master`, with `pkgver()` deriving a monotonically-increasing
version from `git rev-list --count` until the project starts tagging
releases.

The GitHub Actions workflow at `.github/workflows/aur-publish.yml` syncs
this PKGBUILD to `ssh://aur@aur.archlinux.org/alacritree-git.git` on every
push to `master`. When the PKGBUILD is byte-identical to what's already
on AUR, the action regenerates `.SRCINFO` but skips pushing if nothing
actually changed — so users only see commits when the recipe changes.

## One-time setup

1. **Generate an SSH key dedicated to AUR** (no passphrase, so the Actions
   runner can use it non-interactively):

   ```sh
   ssh-keygen -t ed25519 -f ~/.ssh/aur -C 'aur@alacritree-ci' -N ''
   ```

2. **Register the public key on AUR**: copy `~/.ssh/aur.pub` into your
   account's *SSH Public Key* field at <https://aur.archlinux.org/account/>.

3. **Reserve the package name** by pushing an initial commit yourself
   (the GitHub Action assumes the AUR repo already exists):

   ```sh
   GIT_SSH_COMMAND='ssh -i ~/.ssh/aur' \
     git clone ssh://aur@aur.archlinux.org/alacritree-git.git /tmp/aur-alacritree-git
   cp packaging/aur/alacritree-git/PKGBUILD /tmp/aur-alacritree-git/
   cd /tmp/aur-alacritree-git
   makepkg --printsrcinfo > .SRCINFO
   git add PKGBUILD .SRCINFO
   git commit -m 'Initial import'
   GIT_SSH_COMMAND='ssh -i ~/.ssh/aur' git push
   ```

4. **Expose the private key to Actions** as a repository secret named
   `AUR_SSH_PRIVATE_KEY` (Settings → Secrets and variables → Actions).
   Paste the full contents of `~/.ssh/aur` including the BEGIN/END lines.

Subsequent updates are automatic: any push to `master` that changes the
PKGBUILD triggers a fresh AUR commit.
