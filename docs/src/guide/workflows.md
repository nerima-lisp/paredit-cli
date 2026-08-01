# Refactor workflow

`paredit refactor` separates deciding, previewing, writing, and verifying so
that automation can stop at any gate. The full lifecycle for a cross-file
rename looks like this:

```sh
# 1. Plan: gates, risk summary, and an ordered step list.
paredit refactor plan --symbol old-name src/core.lisp src/util.lisp

# 2. Preview: exact rewrites, no files touched. --manifest-out writes the
#    manifest file and prints its hash, so no extra `status` call is needed.
paredit refactor preview --from old-name --to new-name \
  --manifest-out preview.json src/core.lisp src/util.lisp

# 3. Review the manifest without writing.
paredit refactor check --manifest preview.json
paredit refactor status --manifest preview.json
paredit refactor diff --manifest preview.json

# 4. Apply the reviewed manifest with hash guards (hash printed by step 2).
paredit refactor apply --manifest preview.json \
  --expect-manifest-hash "$HASH" --write

# 5. Verify post-conditions.
paredit refactor verify --symbol old-name --new-symbol new-name \
  --phase post src/core.lisp src/util.lisp
```

## Plan output is a contract

`paredit refactor plan` emits JSON with a `decision` block
(`status`, `next_action`, `safe_to_automate`), `gates` with
`blocks_automation` flags, a `risk_summary`, and an ordered `steps` array
whose entries carry runnable `command` strings. An agent can execute the plan
literally: run each step in order and stop when a gate blocks automation or a
step exits non-zero.

Policy flags such as `--require-definitions`, `--require-references`, and
`--fail-on-blocking-gate` turn advisory checks into hard failures, which makes
`plan` usable as a CI gate on its own.

## Preview manifests and hash guards

`preview` and `workspace-preview` print a manifest describing every byte-exact
edit. `apply` refuses to write when:

- the manifest hash does not match `--expect-manifest-hash`;
- a target file changed on disk since the preview was generated; or
- any rewritten output no longer parses.

This means a manifest can be produced in one CI job, reviewed as an artifact,
and applied in a separate controlled job without trusting the intermediate
steps.

## Direct refactorings still gate writes

Named refactorings such as `rename-function`, `extract-function`, or
`add-function-parameter` run in plan mode by default and only modify files
when `--write` is passed after their own validation gates pass. Symbol-oriented
rewrites never touch strings or comments, and Common Lisp scope-aware
refactors preserve `flet`/`labels`/`macrolet`/`symbol-macrolet` binding
boundaries.

## Workspace scope

For repository-wide changes, start with discovery and stay inside the same
lifecycle:

```sh
paredit inspect workspace --output json .
paredit refactor workspace-plan --symbol old-name .
paredit refactor workspace-preview --from old-name --to new-name . > preview.json
paredit refactor workspace-execute --from old-name --to new-name --write .
```

`workspace-execute` wraps preview gates and post-write verification into one
command for cases where a human already reviewed the plan.
