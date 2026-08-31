# Development and release checklist

## 1. Plan the change

- [ ] Write down the user-visible behavior and acceptance criteria.
- [ ] Decide whether the change is a bug fix, feature, or breaking change.
- [ ] Choose the expected SemVer bump: patch, minor, or major.
- [ ] Check existing issues and releases for duplicates or compatibility constraints.
- [ ] Identify affected CLI options, bookmark parsing rules, PDF behavior, and documentation.

## 2. Start development

- [ ] Update the local default branch: `git switch main && git pull --ff-only`.
- [ ] Confirm the worktree is clean with `git status --short`.
- [ ] Create a focused branch, for example `git switch -c fix/outline-pages`.
- [ ] Do not edit the package version manually; the Release workflow owns version changes.
- [ ] Keep generated PDFs, local test inputs, credentials, and `target/` out of Git.

## 3. Implement

- [ ] Make the smallest change that satisfies the acceptance criteria.
- [ ] Preserve the `sbm` executable name and existing CLI behavior unless intentionally changing them.
- [ ] Ignore malformed bookmark lines and out-of-range pages without corrupting the output PDF.
- [ ] Ensure nested entries remain attached to the correct parent bookmark.
- [ ] Return actionable errors for missing files, invalid PDFs, and output failures.
- [ ] Add or update focused tests for every changed behavior and important edge case.
- [ ] Update `README.md` when commands, options, formats, defaults, or installation steps change.

## 4. Validate locally

- [ ] Run `cargo fmt --all --check`.
- [ ] Run `cargo clippy --all-targets --all-features -- -D warnings`.
- [ ] Run `cargo test --all-targets --all-features --locked`.
- [ ] Run `cargo package --locked` from a clean worktree.
- [ ] Run `cargo run --bin sbm -- --help` and inspect the CLI output.
- [ ] Run `cargo run --bin sbm -- --version` and confirm the current development version.
- [ ] Test default paths with `sbm NAME` using a representative PDF and bookmark file.
- [ ] Test explicit `--input`, `--bookmarks`, and `--output` paths.
- [ ] Open the generated PDF in a reader and verify titles, pages, nesting, and output readability.
- [ ] Test malformed lines, page zero, pages beyond the PDF, tabs, four-space indentation, and skipped levels.
- [ ] Confirm the input PDF is unchanged and the output path is correct.

## 5. Review and merge

- [ ] Review `git diff` for accidental files, credentials, debug output, and unrelated changes.
- [ ] Commit with a concise description of the behavior change.
- [ ] Push the branch and open a pull request against `main`.
- [ ] Describe behavior, tests, compatibility impact, and the intended SemVer bump in the pull request.
- [ ] Wait for the GitHub CI workflow to pass.
- [ ] Resolve review feedback and rerun all affected checks.
- [ ] Merge only when CI passes and documentation is current.
- [ ] Confirm `main` is green after the merge.

## 6. Publish a new version

- [ ] Confirm repository **Settings > Actions > General > Workflow permissions** allows read and write access.
- [ ] Confirm branch rules allow `github-actions[bot]` to push the generated version commit and tag to `main`.
- [ ] Confirm the `crates.io` GitHub Environment contains `CARGO_REGISTRY_TOKEN`.
- [ ] Confirm the token can publish `simplebookmarker` and has not expired.
- [ ] Open GitHub **Actions**, select **Release**, and choose **Run workflow** on `main`.
- [ ] Select `patch` for compatible fixes, `minor` for compatible features, or `major` for breaking changes.
- [ ] Wait for the `prepare` job to update both Cargo files, run checks, commit, tag, and push.
- [ ] Approve the `crates.io` environment deployment if protection rules require approval.
- [ ] Wait for the `publish` job to upload the crate and create the GitHub Release.
- [ ] If `publish` fails after `prepare` succeeds, rerun only the failed job; do not run a new version bump.

## 7. Verify the release

- [ ] Confirm the new version appears at `https://crates.io/crates/simplebookmarker`.
- [ ] Confirm the matching `vX.Y.Z` tag and GitHub Release exist.
- [ ] Check that the release commit changed only `Cargo.toml` and `Cargo.lock`.
- [ ] Install the published version with `cargo install simplebookmarker --force`.
- [ ] Run `sbm --version` and confirm it reports the released version.
- [ ] Run one end-to-end PDF bookmark test using the installed binary.
- [ ] Confirm the generated PDF opens and its bookmarks navigate correctly.

## 8. Handle release problems

- [ ] Never overwrite or reuse a version already published to crates.io.
- [ ] For a code defect, fix it on a new branch and publish a new patch version.
- [ ] If a release must be discouraged, use `cargo yank --vers X.Y.Z simplebookmarker` and document why.
- [ ] Do not delete or move an existing release tag after publication.
- [ ] Rotate `CARGO_REGISTRY_TOKEN` immediately if it may have been exposed.
- [ ] Record the failure and prevention steps in the issue or pull request.