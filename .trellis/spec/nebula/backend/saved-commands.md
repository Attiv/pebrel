# Saved Commands and Built-in Catalogues

## Scenario: Extending the platform command manager

### 1. Scope / Trigger

Read this before editing `nebula_app/src/saved_commands*`, built-in templates,
command-manager grouping/search, or corresponding typed UI labels. Actual ownership
is defined by root/nebula_app AGENTS, docs/architecture.md and docs/internationalization.md.

### 2. Signatures

- `builtins::commands(language: UiLanguage) -> Vec<SavedCommand>` builds all hosts' templates.
- `builtins::system_groups() -> &'static [SystemGroup]` supplies the single system registry.
- `builtins::is_system_group(id: &str) -> bool` is shared by UI and store validation.
- `builtins::default_group(command_id: &str) -> Option<&'static str>` reads static metadata.
- `SavedCommands::builtin_commands(language) -> Vec<SavedCommand>` applies hidden IDs.
- `SavedCommands::group_for(command_id) -> Option<&str>` resolves explicit membership,
  including null, before catalogue defaults and unknown-builtin fallback.
- `insert_in_group`, `move_to_group`, `remove` and custom-group `remove_group` use
  the existing lock/reload/atomic-write transaction. System-group `remove_group`
  rejects before lock/I/O. Do not add a view-owned store implementation.

### 3. Contracts

- Catalogue rows: stable `builtin:` ID, typed `Message`, editable nonempty single-line
  text; instantiated `SavedCommand.append_enter` must always be false.
- Platform-specific data belongs in meaningful Linux/macOS/Windows modules. Shared
  operations occur once. Existing IDs/text remain unchanged when adding templates.
- System groups: `builtin`, `builtin-linux`, `builtin-macos`, `builtin-windows`.
  Labels localize, identity does not. Windows group states PowerShell.
- All platform groups are available regardless of focused shell/remote session.
- Version-1 store fields remain `commands`, `organization.groups`, membership map
  with optional group values, and deleted_builtins. Virtual groups never auto-seed
  or count against 40 custom commands / 40 custom groups.
- New en-US and zh-CN message IDs fully align; other locales use existing English
  fallback. Static lookup and the 256 KiB translation payload gate remain unchanged.
- View code never derives default platform from command text or host conditionals.
- An old binary cannot interpret newly saved system-group references; rollback needs
  the pre-change store or explicit cleanup. Merely browsing must not write the store.

### 4. Validation & Error Matrix

| Condition | Contract |
| --- | --- |
| Existing explicit group or null | Preserve it; it overrides the catalogue default |
| Unknown historical builtin ID | Legacy `builtin` default; preserve valid historical references |
| Deleted builtin | Filter from visible catalogue; stale moves fail without writing |
| Delete a system group | Reject with InvalidInput before lock/I/O; no parent-directory creation or mutation |
| Custom group impersonates a system ID | Reject invalid store |
| Stale/missing custom target | Reject atomically; retain existing bytes |
| Custom capacity exceeded | Reject; do not increase limits for built-in catalogue growth |
| Active shell lacks a recipe's tool | Do not auto-install/auto-submit; label requirement and insert for review |

### 5. Good / Base / Bad Cases

- Good: add one typed static Linux recipe, keep its stable ID, test default ownership,
  include matching Chinese/English purpose and distro/tool requirement.
- Base: an old v1 file with no organization loads without being rewritten, while
  new platform folders appear in memory.
- Bad: duplicate common commands per OS, allocate all recipes in group_for,
  classify platform via `starts_with("brew")` in GPUI, or seed folders on startup.

### 6. Tests Required

- Catalogue completeness (Linux >=60, macOS >=40, Windows >=40), unique IDs,
  single-line/no CR-LF-NUL, nonempty localized names/text, append_enter=false.
- Legacy ID/text fixture, representative platform ownership, shared uniqueness,
  useful category coverage and invalid duplicate/missing/auto-submit cases.
- Old store, explicit group/null overrides, hidden records, concurrent transactions,
  protected groups and unchanged custom limits.
- Real GPUI folder clicks and keyboard navigation, scoped/global search,
  virtualization, copy/edit-into-custom/hide controls, and custom-delete positive
  control for system-delete absence. Enable `gpui-test-support` for these tests.
- Independent i18n contract, actual gpui-shell product compile, fmt and architecture
  checks. Test-support drawing is not native visual acceptance.

### 7. Wrong vs Correct

Wrong: select templates from the current host in available_saved_commands; implement
separate lists of protected IDs in persistence and the UI.

Correct: the UI calls SavedCommands::builtin_commands(active_language), enumerates
system_groups for folder names, and calls is_system_group for delete eligibility.

See `architecture/notes/nebula_app/saved_commands/2026-10-12-platform-catalogues.md`
for rationale, rejected alternatives and the old-binary downgrade boundary.
