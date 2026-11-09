# Release preflight

Publishing first resolves the tag to a commit and requires the checkout/event SHA
to match it. Every workspace member version must equal the tag version. The latest
Linux, Windows, macOS and formatting workflow runs for that exact commit must all
be completed successfully. Runs for older commits or a prior successful attempt do
not satisfy a newer pending/failed run. A GitHub API failure stops publication.

Wait for these quality runs before tagging, or rerun a release after they complete.
No administrative bypass or automatic check cancellation is used. The preflight
step receives a read-only GitHub token; crates.io credentials remain scoped to
the publish step. Dependency audit retains its separate warning-only policy and
owners/exception review dates in DEPENDENCY_SECURITY.md.

The registry lookup and dependency evidence work are separate changes. This PR
does not claim that an absent advisory warning means the dependency graph is safe.
Run `python3 .github/scripts/test_release_preflight.py` for the offline contracts.

Only `push` runs qualify as exact-commit evidence. Pull-request runs check out synthetic merge trees and are excluded even when their API `head_sha` matches the tag.
