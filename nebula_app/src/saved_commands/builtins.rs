//! Built-in templates inserted for review, never auto-executed.

mod linux;
mod macos;
mod windows;

use super::{BUILTIN_GROUP_ID, SavedCommand};
use crate::i18n::{Message, UiLanguage};

// Stable identifier, typed purpose label, editable shell text.
type Recipe = (&'static str, Message, &'static str);

pub(crate) struct SystemGroup {
    pub id: &'static str,
    pub name: Message,
    recipes: &'static [Recipe],
}

pub(crate) fn system_groups() -> &'static [SystemGroup] {
    &[
        SystemGroup { id: BUILTIN_GROUP_ID, name: Message::CommandsBuiltinGroup, recipes: SHARED },
        SystemGroup {
            id: "builtin-linux",
            name: Message::CommandsLinuxGroup,
            recipes: linux::RECIPES,
        },
        SystemGroup {
            id: "builtin-macos",
            name: Message::CommandsMacosGroup,
            recipes: macos::RECIPES,
        },
        SystemGroup {
            id: "builtin-windows",
            name: Message::CommandsWindowsGroup,
            recipes: windows::RECIPES,
        },
    ]
}

pub(crate) fn is_system_group(id: &str) -> bool {
    system_groups().iter().any(|group| group.id == id)
}

pub(super) fn default_group(command_id: &str) -> Option<&'static str> {
    system_groups()
        .iter()
        .find(|group| group.recipes.iter().any(|row| row.0 == command_id))
        .map(|group| group.id)
}

pub(crate) fn commands(language: UiLanguage) -> Vec<SavedCommand> {
    system_groups()
        .iter()
        .flat_map(|group| group.recipes)
        .map(|&(id, name, command)| SavedCommand {
            id: id.into(),
            name: language.text(name).into(),
            command: command.into(),
            append_enter: false,
        })
        .collect()
}

const SHARED: &[Recipe] = &[
    (
        "builtin:docker_logs",
        Message::CommandsBuiltinDockerLogs,
        "docker logs --follow --tail 100 container_name",
    ),
    (
        "builtin:docker_exec",
        Message::CommandsBuiltinDockerExec,
        "docker exec -it container_name sh",
    ),
    ("builtin:compose_up", Message::CommandsBuiltinComposeUp, "docker compose up -d --build"),
    (
        "builtin:conda_create",
        Message::CommandsBuiltinCondaCreate,
        "conda create -n dev python=3.12",
    ),
    (
        "builtin:conda_export",
        Message::CommandsBuiltinCondaExport,
        "conda env export --from-history > environment.yml",
    ),
    (
        "builtin:conda_restore",
        Message::CommandsBuiltinCondaRestore,
        "conda env create -f environment.yml",
    ),
];

#[cfg(test)]
mod tests;
