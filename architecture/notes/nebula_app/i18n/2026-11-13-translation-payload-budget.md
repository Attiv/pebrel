# Translation payload budget re-baselined to 288 KiB

## Status

Implemented; static verification passed. Maintainer review: approved in the current
task (chose raising the budget over trimming catalogues).

## Context

`nebula_app/tests/i18n_contract.rs` asserts that the embedded UTF-8 translation
payload stays under a fixed budget. The check exists so translation growth cannot
silently inflate the initial binary payload; the number came from upstream when the
fork was created and had never been revisited.

This fork ships more user-visible strings than upstream because it owns extra
features (tab badges, SSH forwarding settings, provider/agent settings, backup
flows). By the 2.5.6 alignment the catalogue total was 262130 bytes against a
262144-byte (256 KiB) ceiling: 14 bytes of headroom. The 2.5.7 upstream merge added
one message (`commands.rename_group`: 12 bytes of English, 15 bytes of Simplified
Chinese), which pushed the total to 262157 and failed the contract on macOS,
Windows x64 and Windows ARM64 native test jobs.

## Evidence

- `nebula_app/build/i18n.rs` computes `TRANSLATED_BYTES` as the sum of every
  language catalogue value's UTF-8 length, so the budget covers all eleven
  languages rather than English plus Chinese only.
- Measured totals: `0c08f287` (pre-merge) 262130; `HEAD` after merging
  `09e5eb47` 262157; upstream `main` 248770; upstream `v2.5.7` 252046.
  The fork therefore carries roughly 13 KB of feature messages upstream does not.
- The same build script requires English and Simplified Chinese to have identical
  message ids (`english.keys().eq(chinese.keys())`), so catalogue entries cannot be
  dropped from those two locales to buy headroom. Partially translated locales are
  allowed, but their entries are real translations rather than filler.
- CI failure: run `43364160295`, job `Native tests (macos-26)` reported
  `assertion failed: i18n::TRANSLATED_BYTES < 256 * 1024` at
  `nebula_app/tests/i18n_contract.rs:107`; the Windows jobs failed identically.

## Decision

Raise the ceiling to 288 KiB and keep it as a named constant
(`TRANSLATED_BYTE_BUDGET`) next to the test, with a comment explaining why the fork
diverges from upstream's 256 KiB. `docs/internationalization.md` states the same
number in both languages.

The budget still fails the build when the payload grows past 288 KiB, so it keeps
its purpose: it is a fork-sized ceiling, not a disabled check.

## Rejected alternatives

- **Trim redundant catalogue entries.** English-identical values exist in `zh-CN`
  (19 entries, 252 bytes), but the build script requires exact key parity with
  English, so those entries cannot be removed. Dropping entries from other locales
  would either delete real translations or make coverage tests
  (`tab_context_menu_has_translations_for_every_supported_language`) fail.
- **Shorten the new message text.** Would fit this release by 15 bytes while
  diverging the fork's UI wording from upstream for every future merge, and would
  break again on the next upstream message.
- **Delete the assertion.** Removes the only guard against unbounded payload
  growth, which is what the contract exists to prevent.

## Consequences

- The fork accepts up to 32 KiB more embedded translation text than upstream's
  ceiling; at the current total (262157) the build has roughly 32 KiB of headroom,
  enough for several upstream releases before this needs revisiting.
- Any future re-baseline must state the measured total and the reason, exactly as
  this note does, so the number stays a decision rather than an accident.

## Validation

- `python3` recomputation of `TRANSLATED_BYTES` from the eleven catalogues gives
  262157, which is under the new 294912-byte ceiling.
- `cargo fmt --all -- --check` reports no diff; `scripts/check_architecture.py` and
  `scripts/check_platform_cfg.py` pass.
- The contract test itself runs on the three native-test jobs of the release
  workflow; a green run is the acceptance evidence for this change.

## Supersedes

None. The 256 KiB value had no separate rationale note.

## Revisit when

- The measured payload passes 280 KiB, which leaves too little room for a single
  upstream release cycle.
- A language catalogue is dropped or added, since both change the total by more
  than any single message.
- Upstream re-baselines or removes the payload assertion, in which case the fork
  should follow upstream's contract instead of keeping a local number.
