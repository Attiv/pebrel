use super::*;
use std::collections::HashSet;

fn adequate_catalogue(rows: &[SavedCommand]) -> bool {
    let ids: HashSet<_> = rows.iter().map(|row| &row.id).collect();
    ids.len() == rows.len()
        && rows.iter().all(|row| {
            !row.append_enter
                && !row.name.is_empty()
                && !row.command.is_empty()
                && !row.command.contains(['\r', '\n', '\0'])
                && super::super::normalize_fields(&row.name, &row.command).is_ok()
                && default_group(&row.id).is_some()
        })
        && [("builtin-linux", 60), ("builtin-macos", 40), ("builtin-windows", 40)].into_iter().all(
            |(id, min)| rows.iter().filter(|row| default_group(&row.id) == Some(id)).count() >= min,
        )
}

#[test]
fn platform_catalogues_cover_everyday_tasks() {
    for language in [UiLanguage::EnUs, UiLanguage::ZhCn] {
        let rows = commands(language);
        assert!(adequate_catalogue(&rows));
        for group in system_groups().iter().skip(1) {
            for category in [
                "Files",
                "Text",
                "Archive",
                "System",
                "Resources",
                "Processes",
                "Network",
                "Permissions",
                "Services",
                "Logs",
                "Packages",
            ] {
                assert!(
                    group
                        .recipes
                        .iter()
                        .any(|row| UiLanguage::EnUs.text(row.1).starts_with(category)),
                    "missing {category} in {}",
                    group.id
                );
            }
        }
        assert_eq!(rows.iter().filter(|row| row.id == "builtin:compose_up").count(), 1);
        assert_eq!(rows.iter().filter(|row| row.id == "builtin:conda_create").count(), 1);
    }
}

#[test]
fn recipes_are_unique_reviewable_and_platform_specific() {
    let rows = commands(UiLanguage::EnUs);
    assert!(adequate_catalogue(&rows));
    for row in &rows {
        if row.command.starts_with("winget ") {
            assert_eq!(default_group(&row.id), Some("builtin-windows"));
        }
        if row.command.starts_with("brew ") {
            assert_eq!(default_group(&row.id), Some("builtin-macos"));
        }
        let chinese =
            commands(UiLanguage::ZhCn).into_iter().find(|other| other.id == row.id).unwrap();
        assert_ne!(row.name, chinese.name);
    }
    assert!(UiLanguage::EnUs.text(Message::CommandsWindowsGroup).contains("PowerShell"));
    assert_eq!(default_group("builtin:linux_journal"), Some("builtin-linux"));
    assert_eq!(default_group("builtin:macos_services"), Some("builtin-macos"));
    assert_eq!(default_group("builtin:windows_services"), Some("builtin-windows"));
    assert_eq!(default_group("builtin:unknown"), None);
}

#[test]
fn incomplete_duplicate_unassigned_and_autoexecuting_catalogues_are_rejected() {
    let rows = commands(UiLanguage::EnUs);
    let mut invalid = rows.clone();
    invalid.retain(|row| default_group(&row.id) != Some("builtin-linux"));
    assert!(!adequate_catalogue(&invalid));
    for defect in 0..5 {
        let mut invalid = rows.clone();
        match defect {
            0 => invalid.push(invalid[0].clone()),
            1 => invalid[0].append_enter = true,
            2 => invalid[0].command.push('\n'),
            3 => invalid[0].name.clear(),
            _ => invalid[0].id = "builtin:unassigned".into(),
        }
        assert!(!adequate_catalogue(&invalid));
    }
}

#[test]
fn legacy_ids_and_text_remain_exact() {
    let rows = commands(UiLanguage::EnUs);
    for (id, text) in [
        ("builtin:docker_install_windows", "winget install --exact --id Docker.DockerDesktop"),
        ("builtin:docker_install_mac", "brew install --cask docker"),
        (
            "builtin:docker_install_ubuntu",
            "sudo apt-get update && sudo apt-get install docker.io docker-compose-v2",
        ),
        ("builtin:docker_install_fedora", "sudo dnf install moby-engine docker-compose"),
        ("builtin:docker_logs", "docker logs --follow --tail 100 container_name"),
        ("builtin:docker_exec", "docker exec -it container_name sh"),
        ("builtin:compose_up", "docker compose up -d --build"),
        ("builtin:conda_install_windows", "winget install --exact --id Anaconda.Miniconda3"),
        ("builtin:conda_install_mac", "brew install --cask miniconda"),
        (
            "builtin:conda_install_linux",
            "curl -fL -o miniconda-installer.sh \"https://repo.anaconda.com/miniconda/Miniconda3-latest-Linux-$(uname -m).sh\" && bash miniconda-installer.sh",
        ),
        ("builtin:conda_create", "conda create -n dev python=3.12"),
        ("builtin:conda_export", "conda env export --from-history > environment.yml"),
        ("builtin:conda_restore", "conda env create -f environment.yml"),
        ("builtin:python_install_windows", "winget install --exact --id Python.Python.3.14"),
        ("builtin:python_install_mac", "brew install python"),
        (
            "builtin:python_install_ubuntu",
            "sudo apt-get update && sudo apt-get install python3 python3-pip python3-venv",
        ),
        ("builtin:python_install_fedora", "sudo dnf install python3 python3-pip"),
        ("builtin:git_install_windows", "winget install --exact --id Git.Git"),
        ("builtin:git_install_mac", "brew install git"),
        ("builtin:git_install_ubuntu", "sudo apt-get update && sudo apt-get install git"),
        ("builtin:git_install_fedora", "sudo dnf install git"),
    ] {
        assert_eq!(rows.iter().find(|row| row.id == id).unwrap().command, text);
    }
}

#[test]
fn powershell_unique_lines_preserves_case_and_process_examples_avoid_reserved_pid() {
    let rows = commands(UiLanguage::EnUs);
    let unique = rows.iter().find(|row| row.id == "builtin:windows_unique").unwrap();
    assert!(unique.command.contains("Group-Object -CaseSensitive -NoElement"));
    for id in ["builtin:windows_stop_process", "builtin:windows_process_detail"] {
        let row = rows.iter().find(|row| row.id == id).unwrap();
        assert!(row.command.contains("$processId = "));
        assert!(!row.command.to_lowercase().contains("$pid = "));
    }
}
