# Installed editor assets follow the canonical adapter, not printf spelling

## Status

Accepted, 2026-11-08.

## Context

The shared native-editor script is embedded in both local integration and remote installation assets. Adding a separate trailing-display event parameterized the emitter's event name without changing its default buffer-report event. An installation test still required the previous literal printf text, so valid generated assets failed the test on every native host.

## Evidence

Release run `42451419751`, source `0f1a2069`, failed the same `installed_shell_assets_include_the_native_editor_adapter` assertion on all four primary native hosts. Each suite had exactly one failing case; packages and asset aggregation passed, while publication correctly remained blocked. The original case also failed locally before repair. Selected prediction/editor tests had not covered this installation consumer.

The canonical adapter is [tty::connection_shell](../../../../../nebula_terminal/src/tty/mod.rs). The [remote installation tests](../../../../../nebula_app/src/ai_hook/remote/tests.rs) own whether generated Bash/zsh assets embed it, not how its internal printf is spelled.

## Decision

- Compare the installed asset against the complete canonical adapter. Keep producer protocol behavior covered by its editor/terminal tests rather than duplicating its implementation text in an installer assertion.
- Derive omission, UTF-8-safe truncation and internal-corruption negative controls from that same canonical body. Negative controls must actually alter the asset and be rejected; they must not introduce another dependency on a producer local-variable name or formatting choice.
- Trace shared adapter changes to installed assets as well as local editor consumers. Run the existing `scripts/ci_native_tests.py --runner nextest` host entry with the workflow-pinned runner before presenting selective tests as complete native validation.
- Keep publication blocked until the existing same-source native and packaging gates pass. A new source requires a fresh run, not a failed-only rerun of the old source or reuse of old artifacts.

## Rejected alternatives

- Delete/ignore the failing test, add a comment containing its old marker, or special-case a known emitter spelling: each hides the contract rather than testing it.
- Rewrite a working producer solely to retain an obsolete test substring: gives the installation layer authority over internal emitter formatting.
- Treat package success, production compilation or selected local tests as full native acceptance: the failed release demonstrates the missing coverage.

## Consequences

No production Shell, key routing, model request, installer behavior, dependency or release scope changes are needed. Installation integrity becomes stricter while equivalent canonical producer refactors no longer require editing an unrelated textual expectation. The assertion should change only if installation intentionally stops embedding the adapter verbatim.

## Validation

The exact baseline failure is retained as a red regression. Positive generated-asset and effective mutation-negative tests protect the repaired boundary. Full host and fresh hosted native results are separate release evidence; passing a focused filter alone is not a claim that every platform has passed.

## Supersedes

None. The native trailing-display protocol remains unchanged; this repairs the downstream installation test's implicit assumption.

## Revisit when

Installation intentionally transforms the adapter rather than embedding it verbatim, or another installer becomes a consumer of the shared native-editor script.
