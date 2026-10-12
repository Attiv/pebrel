//! macos templates and tool-specific requirements.

use super::Recipe;
use crate::i18n::Message;

pub(super) const RECIPES: &[Recipe] = &[
    (
        "builtin:docker_install_mac",
        Message::CommandsBuiltinDockerInstallMac,
        "brew install --cask docker",
    ),
    (
        "builtin:conda_install_mac",
        Message::CommandsBuiltinCondaInstallMac,
        "brew install --cask miniconda",
    ),
    ("builtin:python_install_mac", Message::CommandsBuiltinPythonInstallMac, "brew install python"),
    ("builtin:git_install_mac", Message::CommandsBuiltinGitInstallMac, "brew install git"),
    ("builtin:macos_pwd", Message::CommandsMacosPwd, "pwd"),
    ("builtin:macos_list", Message::CommandsMacosList, "ls -lah"),
    ("builtin:macos_mkdir", Message::CommandsMacosMkdir, "mkdir -p ./project/src"),
    ("builtin:macos_copy", Message::CommandsMacosCopy, "cp -i ./example.txt ./example-copy.txt"),
    ("builtin:macos_move", Message::CommandsMacosMove, "mv -i ./example.txt ./renamed.txt"),
    ("builtin:macos_find_name", Message::CommandsMacosFindName, "find . -type f -name \"*.log\""),
    ("builtin:macos_disk", Message::CommandsMacosDisk, "df -h"),
    ("builtin:macos_size", Message::CommandsMacosSize, "du -h -d 1 ."),
    ("builtin:macos_grep", Message::CommandsMacosGrep, "grep -Rn \"ERROR\" ./logs"),
    ("builtin:macos_head", Message::CommandsMacosHead, "head -n 40 ./example.txt"),
    ("builtin:macos_follow", Message::CommandsMacosFollow, "tail -F ./app.log"),
    ("builtin:macos_unique", Message::CommandsMacosUnique, "sort ./example.txt | uniq -c"),
    (
        "builtin:macos_tar_create",
        Message::CommandsMacosTarCreate,
        "tar -czf ./archive.tar.gz ./project",
    ),
    (
        "builtin:macos_zip",
        Message::CommandsMacosZip,
        "ditto -c -k --sequesterRsrc --keepParent ./project ./archive.zip",
    ),
    ("builtin:macos_unzip", Message::CommandsMacosUnzip, "ditto -x -k ./archive.zip ./extracted"),
    ("builtin:macos_version", Message::CommandsMacosVersion, "sw_vers"),
    (
        "builtin:macos_hardware",
        Message::CommandsMacosHardware,
        "system_profiler SPHardwareDataType",
    ),
    ("builtin:macos_uptime", Message::CommandsMacosUptime, "uptime"),
    ("builtin:macos_memory", Message::CommandsMacosMemory, "vm_stat"),
    (
        "builtin:macos_processes",
        Message::CommandsMacosProcesses,
        "ps aux | sort -nr -k 4 | head -n 15",
    ),
    ("builtin:macos_top", Message::CommandsMacosTop, "top -o cpu"),
    ("builtin:macos_stop_process", Message::CommandsMacosStopProcess, "kill -TERM 12345"),
    ("builtin:macos_ports", Message::CommandsMacosPorts, "lsof -nP -iTCP -sTCP:LISTEN"),
    (
        "builtin:macos_interfaces",
        Message::CommandsMacosInterfaces,
        "networksetup -listallhardwareports",
    ),
    ("builtin:macos_dns", Message::CommandsMacosDns, "networksetup -getdnsservers \"Wi-Fi\""),
    ("builtin:macos_ping", Message::CommandsMacosPing, "ping -c 4 example.com"),
    ("builtin:macos_http", Message::CommandsMacosHttp, "curl -I https://example.com"),
    ("builtin:macos_permissions", Message::CommandsMacosPermissions, "ls -le ./example.txt"),
    ("builtin:macos_chmod", Message::CommandsMacosChmod, "chmod u+x ./script.sh"),
    ("builtin:macos_identity", Message::CommandsMacosIdentity, "id"),
    ("builtin:macos_services", Message::CommandsMacosServices, "launchctl list"),
    (
        "builtin:macos_logs",
        Message::CommandsMacosLogs,
        "/usr/bin/log show --last 1h --style compact --predicate 'process == \"sshd\"'",
    ),
    (
        "builtin:macos_log_stream",
        Message::CommandsMacosLogStream,
        "/usr/bin/log stream --style compact --predicate 'process == \"sshd\"'",
    ),
    ("builtin:macos_brew_search", Message::CommandsMacosBrewSearch, "brew search nginx"),
    ("builtin:macos_brew_list", Message::CommandsMacosBrewList, "brew list --versions"),
    ("builtin:macos_brew_install", Message::CommandsMacosBrewInstall, "brew install nginx"),
];
