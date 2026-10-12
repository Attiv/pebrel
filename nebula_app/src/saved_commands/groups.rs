//! Group metadata shares the command store's lock/reload/atomic-write transaction.

use std::collections::{BTreeMap, HashSet};

use super::*;

pub(crate) const BUILTIN_GROUP_ID: &str = "builtin";
const MAX_GROUPS: usize = 40;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct CommandGroup {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Organization {
    #[serde(default)]
    pub groups: Vec<CommandGroup>,
    // Absence uses the default. Explicit null keeps a builtin outside its default group.
    #[serde(default)]
    pub membership: BTreeMap<String, Option<String>>,
}

impl SavedCommands {
    pub(crate) fn groups(&self) -> &[CommandGroup] {
        &self.organization.groups
    }

    pub(crate) fn group_for<'a>(&'a self, command_id: &str) -> Option<&'a str> {
        match self.organization.membership.get(command_id) {
            Some(group) => group.as_deref(),
            None if command_id.starts_with("builtin:") => {
                Some(builtins::default_group(command_id).unwrap_or(BUILTIN_GROUP_ID))
            },
            None => None,
        }
    }

    pub(crate) fn create_group(&mut self, name: &str) -> io::Result<()> {
        let (next, ()) = mutate_store(&self.path, |store| store.add_group(name).map(|_| ()))?;
        *self = next;
        Ok(())
    }

    fn add_group(&mut self, name: &str) -> io::Result<String> {
        let (name, _) = normalize_fields(name, "group")?;
        if self.groups().len() >= MAX_GROUPS || self.groups().iter().any(|g| g.name == name) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "duplicate name or group limit reached",
            ));
        }
        let id = new_id(&name, "group");
        self.organization.groups.push(CommandGroup { id: id.clone(), name });
        Ok(id)
    }

    pub(crate) fn move_to_group(
        &mut self,
        command_id: &str,
        group: Option<&str>,
    ) -> io::Result<()> {
        let (next, ()) = mutate_store(&self.path, |store| store.assign_group(command_id, group))?;
        *self = next;
        Ok(())
    }

    pub(super) fn assign_group(&mut self, command_id: &str, group: Option<&str>) -> io::Result<()> {
        if !self.has_command(command_id) || !self.has_group(group) {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "command or group no longer exists",
            ));
        }
        self.organization.membership.insert(command_id.to_owned(), group.map(str::to_owned));
        Ok(())
    }

    pub(crate) fn remove_group(&mut self, group_id: &str) -> io::Result<()> {
        // The protected registry is immutable; reject before any lock or filesystem work.
        if is_system_group(group_id) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "system groups cannot be deleted",
            ));
        }
        let (next, ()) = mutate_store(&self.path, |store| store.delete_group(group_id))?;
        *self = next;
        Ok(())
    }

    fn delete_group(&mut self, group_id: &str) -> io::Result<()> {
        let Some(index) = self.organization.groups.iter().position(|group| group.id == group_id)
        else {
            return Err(io::Error::new(io::ErrorKind::NotFound, "group no longer exists"));
        };
        self.organization.groups.remove(index);
        for group in self.organization.membership.values_mut() {
            if group.as_deref() == Some(group_id) {
                *group = None;
            }
        }
        Ok(())
    }

    fn has_command(&self, id: &str) -> bool {
        // Unknown historical builtin ids remain valid compatibility inputs.
        (is_builtin_id(id) && !self.deleted_builtins.contains(id))
            || self.commands.iter().any(|command| command.id == id)
    }

    fn has_group(&self, id: Option<&str>) -> bool {
        id.is_none_or(|id| is_system_group(id) || self.groups().iter().any(|group| group.id == id))
    }

    pub(super) fn validate_groups(&self) -> io::Result<()> {
        let invalid =
            || io::Error::new(io::ErrorKind::InvalidData, "invalid command group metadata");
        if self.groups().len() > MAX_GROUPS {
            return Err(invalid());
        }
        let mut ids = HashSet::new();
        for group in self.groups() {
            if group.id.is_empty()
                || group.id.len() > MAX_ID_CHARS
                || is_system_group(&group.id)
                || !ids.insert(&group.id)
            {
                return Err(invalid());
            }
            normalize_fields(&group.name, "group").map_err(|_| invalid())?;
        }
        for (command, group) in &self.organization.membership {
            if !self.has_command(command) || !self.has_group(group.as_deref()) {
                return Err(invalid());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_defaults_and_explicit_overrides_do_not_rewrite_on_browse() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(STORE_FILE);
        let bytes = br#"{"version":1,"commands":[],"organization":{"membership":{"builtin:git_install_mac":"builtin","builtin:git_install_windows":null}}}"#;
        std::fs::write(&path, bytes).unwrap();
        let mut saved = SavedCommands::load_from(&path).unwrap();
        assert_eq!(saved.group_for("builtin:git_install_ubuntu"), Some("builtin-linux"));
        assert_eq!(saved.group_for("builtin:git_install_mac"), Some(BUILTIN_GROUP_ID));
        assert_eq!(saved.group_for("builtin:git_install_windows"), None);
        assert_eq!(saved.builtin_commands(crate::i18n::UiLanguage::EnUs).len(), 146);
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        saved.move_to_group("builtin:git_install_ubuntu", Some("builtin-macos")).unwrap();
        saved.move_to_group("builtin:git_install_windows", Some("builtin-windows")).unwrap();
        let saved = SavedCommands::load_from(&path).unwrap();
        assert_eq!(saved.group_for("builtin:git_install_ubuntu"), Some("builtin-macos"));
        assert_eq!(saved.group_for("builtin:git_install_windows"), Some("builtin-windows"));
    }

    #[test]
    fn system_groups_are_virtual_protected_and_cannot_be_impersonated() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(STORE_FILE);
        let mut saved = SavedCommands::load_from(&path).unwrap();
        for group in system_groups() {
            assert!(saved.remove_group(group.id).is_err());
            assert!(!path.exists(), "reject deletion without creating a store");
            let spoof = serde_json::json!({"version":1, "commands":[], "organization":{
                "groups":[{"id":group.id, "name":"Spoof"}]
            }});
            assert!(
                SavedCommands::from_bytes(&path, &serde_json::to_vec(&spoof).unwrap()).is_err()
            );
            let command =
                saved.insert_in_group("Example", "echo example", false, Some(group.id)).unwrap();
            assert_eq!(
                SavedCommands::load_from(&path).unwrap().group_for(&command.id),
                Some(group.id)
            );
            assert!(saved.groups().is_empty(), "system groups do not consume user capacity");
            let before = std::fs::read(&path).unwrap();
            assert!(saved.remove_group(group.id).is_err());
            assert_eq!(std::fs::read(&path).unwrap(), before);
            saved.remove(&command.id).unwrap();
            std::fs::remove_file(&path).unwrap();
        }
    }

    #[test]
    fn rejecting_system_group_deletion_does_not_create_store_directories() {
        let dir = tempfile::tempdir().unwrap();
        let parent = dir.path().join("missing").join("commands");
        let path = parent.join(STORE_FILE);
        let mut saved = SavedCommands::load_from(&path).unwrap();
        for group in system_groups() {
            assert_eq!(
                saved.remove_group(group.id).unwrap_err().kind(),
                io::ErrorKind::InvalidInput
            );
            assert!(!parent.exists(), "reject protected deletion before any filesystem writes");
        }
    }

    #[test]
    fn rejecting_system_group_deletion_does_not_depend_on_store_lock() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(STORE_FILE);
        let mut saved = SavedCommands::load_from(&path).unwrap();
        let _lock = crate::atomic_file::try_lock(&path).unwrap().unwrap();
        for group in system_groups() {
            assert_eq!(
                saved.remove_group(group.id).unwrap_err().kind(),
                io::ErrorKind::InvalidInput
            );
            assert!(!path.exists());
        }
    }

    #[test]
    fn platform_hidden_records_survive_other_window_edits_and_reload() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(STORE_FILE);
        let mut saved = SavedCommands::load_from(&path).unwrap();
        let mut other_window = saved.clone();
        saved.move_to_group("builtin:linux_pwd", Some("builtin-windows")).unwrap();
        saved.remove("builtin:linux_pwd").unwrap();
        other_window
            .insert_in_group("Example", "echo example", false, Some("builtin-linux"))
            .unwrap();
        let saved = SavedCommands::load_from(&path).unwrap();
        assert!(
            saved
                .builtin_commands(crate::i18n::UiLanguage::ZhCn)
                .iter()
                .all(|row| row.id != "builtin:linux_pwd")
        );
        assert!(!saved.organization.membership.contains_key("builtin:linux_pwd"));
        let before = std::fs::read(&path).unwrap();
        assert!(other_window.move_to_group("builtin:linux_pwd", None).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }

    #[test]
    fn expanded_catalogue_does_not_expand_custom_store_limits() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(STORE_FILE);
        let mut saved = SavedCommands::load_from(&path).unwrap();
        for index in 0..MAX_GROUPS {
            saved.create_group(&format!("Group {index}")).unwrap();
        }
        for index in 0..MAX_COMMANDS {
            saved.insert(&format!("Example {index}"), "echo example", false).unwrap();
        }
        let before = std::fs::read(&path).unwrap();
        assert!(saved.create_group("One too many").is_err());
        assert!(saved.insert("One too many", "echo example", false).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
        saved.move_to_group(&saved.commands()[0].id.clone(), Some("builtin-linux")).unwrap();
        assert_eq!(saved.groups().len(), MAX_GROUPS);
        assert_eq!(saved.commands().len(), MAX_COMMANDS);
    }

    #[test]
    fn inserting_inside_a_folder_is_atomic_and_rejects_a_deleted_folder() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(STORE_FILE);
        let mut saved = SavedCommands::load_from(&path).unwrap();
        saved.create_group("Work").unwrap();
        let group = saved.groups()[0].id.clone();
        let command = saved.insert_in_group("Build", "cargo build", true, Some(&group)).unwrap();
        assert_eq!(
            SavedCommands::load_from(&path).unwrap().group_for(&command.id),
            Some(group.as_str())
        );
        saved.remove_group(&group).unwrap();
        let before = std::fs::read(&path).unwrap();
        assert!(saved.insert_in_group("Lost", "echo lost", false, Some(&group)).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }

    #[test]
    fn old_store_defaults_and_explicit_builtin_removal_survive_reload() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(STORE_FILE);
        std::fs::write(&path, r#"{"version":1,"commands":[]}"#).unwrap();
        let old = SavedCommands::load_from(&path).unwrap();
        assert_eq!(old.group_for("builtin:docker_ps"), Some(BUILTIN_GROUP_ID));
        let (moved, ()) =
            mutate_store(&path, |store| store.assign_group("builtin:docker_ps", None)).unwrap();
        assert_eq!(moved.group_for("builtin:docker_ps"), None);
        assert_eq!(SavedCommands::load_from(&path).unwrap(), moved);
    }

    #[test]
    fn group_transactions_preserve_other_windows_and_reject_stale_targets() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(STORE_FILE);
        let (_, first) = mutate_store(&path, |store| store.add_group("Work")).unwrap();
        let (_, second) = mutate_store(&path, |store| store.add_group("Servers")).unwrap();
        mutate_store(&path, |store| store.assign_group("builtin:docker_ps", Some(&first))).unwrap();
        let (saved, ()) = mutate_store(&path, |store| store.delete_group(&first)).unwrap();
        assert_eq!(saved.groups()[0].id, second);
        assert_eq!(saved.group_for("builtin:docker_ps"), None);
        let bytes = std::fs::read(&path).unwrap();
        assert!(
            mutate_store(&path, |store| store.assign_group("builtin:docker_ps", Some(&first)))
                .is_err()
        );
        assert!(
            mutate_store(&path, |store| store.assign_group("missing-command", Some(&second)))
                .is_err()
        );
        assert!(mutate_store(&path, |store| store.add_group("Servers")).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
    }
}
