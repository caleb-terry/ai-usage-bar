---
name: ship-pr
description: End-to-end "ship this branch" workflow — commit & push everything, open a PR, wait for CI + automated reviews, resolve any review findings, then merge to main, sync main, and delete the merged branches (local + remote). Trigger on requests like "ship this", "ship the PR", "open a PR and merge it", "commit push PR wait and merge", or any ask for the full create-PR-through-merge-and-cleanup cycle.
---

# ship-pr — full branch ship & cleanup cycle

Runs the whole lifecycle of getting the current branch's work merged: commit →
push → PR → wait for review → resolve findings → merge → sync → clean up.
Designed so the user can just say "ship it" instead of typing every step.

## Preconditions / sanity checks

1. **Refuse on `main`.** Never run this with `main` checked out — there's
   nothing to PR. If on `main`, stop and tell the user to switch to a feature
   branch.
2. Confirm `gh auth status` is logged in. If not, stop and tell the user to run
   `gh auth login`.
3. Identify the default branch: `gh repo view --json defaultBranchRef -q .defaultBranchRef.name`
   (this repo: `main`). Use that everywhere "main" appears below.

## Step 1 — Commit & push everything

- `git status --short`. If there are uncommitted changes, commit them. Use a
  conventional-commit message that describes the *why*; match the repo's commit
  style (see `git log`). End the message with the Co-Authored-By trailer this
  session uses.
- If the working tree is already clean, say so — don't fabricate an empty commit.
- Push: `git push -u origin <branch>`.
- Before pushing, run the repo's fast local checks if the diff touches their
  domain (this project: `cargo fmt --check`, `cargo clippy -- -D warnings`,
  `cargo test`, `pnpm build`; and `scripts/win-check.sh` if any
  `#[cfg(windows)]` code changed — CI does NOT compile Windows). Don't push
  known-red code.

## Step 2 — Open the PR

- `gh pr create --base main --head <branch> --title "<title>" --body "<body>"`.
- Title: summarize the branch's theme (look at `git log main..HEAD`).
- Body: bullet the notable changes + a "Validation" line listing what was run.
  End the body with the Claude Code generated-by footer.
- Capture the PR URL/number from the output.

## Step 3 — Wait ~15 min, then check for reviews

The user typically wants a wait so automated reviewers (and CI) have time to run.

- Use `ScheduleWakeup` (dynamic `/loop`) or sleep-in-background to wait the
  requested interval (default 15 min). Pick `delaySeconds` per the ScheduleWakeup
  guidance — for a 15-min wait, two ~450s legs keeps it simple; a single 900s is
  fine too. Don't busy-poll every 60s for 15 min.
- While waiting / on wake, gather review state:
  - CI: `gh pr checks <num>` (rerun until checks finish, not just once).
  - Human/bot review comments: `gh pr view <num> --json reviews,comments`
  - Inline code-review threads (these are the ones bots like the cloud reviewer
    post): `gh api repos/{owner}/{repo}/pulls/<num>/comments`
  - Top-level issue comments: `gh api repos/{owner}/{repo}/issues/<num>/comments`
- Summarize every finding for the user.

## Step 4 — Resolve findings

- For each actionable review finding, make the fix on the branch, re-run the
  relevant local checks, then `git commit` + `git push`.
- Re-check that CI goes green after the push.
- Skip / push back (with reasoning) on findings that are wrong or out of scope —
  don't blindly apply bot suggestions. State which you skipped and why.
- If there are NO findings, say so explicitly and proceed to merge.

## Step 5 — Merge to main

- Confirm CI is green and the PR is mergeable: `gh pr view <num> --json mergeable,mergeStateStatus,statusCheckRollup`.
- Merge. Default to squash unless the repo clearly prefers merge commits:
  `gh pr merge <num> --squash --delete-branch`.
  `--delete-branch` removes the remote branch as part of the merge.

## Step 6 — Refresh main & clean up branches

- `git checkout main && git pull --ff-only origin main`.
- Prune remote-tracking refs: `git fetch --prune`.
- Delete the merged local feature branch: `git branch -d <branch>` (use `-d`,
  not `-D`, so git refuses if it's somehow unmerged — investigate rather than
  force).
- Clean up any *other* local branches whose upstream is gone (merged & remote
  deleted). Identify them:
  `git branch -vv | grep ': gone]' | awk '{print $1}'`
  Confirm each is merged into main before deleting with `git branch -d`. There
  is a project skill/command `clean_gone` that does exactly this — prefer it if
  available.
- Report final state: current branch (`main`), the merge commit, and which
  branches were deleted.

## Notes

- **Idempotency:** re-running after a partial run should detect existing state
  (PR already open, branch already pushed, tree already clean) and not duplicate
  work. Always check before acting.
- **Don't force-push or hard-reset** to resolve conflicts without surfacing them
  to the user first.
- Keep the user informed at each phase boundary; this is a long-running,
  outward-facing sequence (it publishes a PR and merges to main).
