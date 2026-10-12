//! windows templates and tool-specific requirements.

use super::Recipe;
use crate::i18n::Message;

pub(super) const RECIPES: &[Recipe] = &[
    (
        "builtin:docker_install_windows",
        Message::CommandsBuiltinDockerInstallWindows,
        "winget install --exact --id Docker.DockerDesktop",
    ),
    (
        "builtin:conda_install_windows",
        Message::CommandsBuiltinCondaInstallWindows,
        "winget install --exact --id Anaconda.Miniconda3",
    ),
    (
        "builtin:python_install_windows",
        Message::CommandsBuiltinPythonInstallWindows,
        "winget install --exact --id Python.Python.3.14",
    ),
    (
        "builtin:git_install_windows",
        Message::CommandsBuiltinGitInstallWindows,
        "winget install --exact --id Git.Git",
    ),
    ("builtin:windows_pwd", Message::CommandsWindowsPwd, "Get-Location"),
    ("builtin:windows_list", Message::CommandsWindowsList, "Get-ChildItem -Force"),
    (
        "builtin:windows_mkdir",
        Message::CommandsWindowsMkdir,
        "New-Item -ItemType Directory -Path '.\\project\\src' -Force",
    ),
    (
        "builtin:windows_copy",
        Message::CommandsWindowsCopy,
        "Copy-Item -LiteralPath '.\\example.txt' -Destination '.\\example-copy.txt' -Confirm",
    ),
    (
        "builtin:windows_move",
        Message::CommandsWindowsMove,
        "Move-Item -LiteralPath '.\\example.txt' -Destination '.\\renamed.txt' -Confirm",
    ),
    (
        "builtin:windows_find_name",
        Message::CommandsWindowsFindName,
        "Get-ChildItem -Path . -Recurse -File -Filter '*.log'",
    ),
    ("builtin:windows_disk", Message::CommandsWindowsDisk, "Get-PSDrive -PSProvider FileSystem"),
    (
        "builtin:windows_hash",
        Message::CommandsWindowsHash,
        "Get-FileHash -LiteralPath '.\\example.txt' -Algorithm SHA256",
    ),
    (
        "builtin:windows_grep",
        Message::CommandsWindowsGrep,
        "Get-ChildItem -Path . -Recurse -File -Filter '*.log' | Select-String -Pattern 'ERROR'",
    ),
    (
        "builtin:windows_head",
        Message::CommandsWindowsHead,
        "Get-Content -LiteralPath '.\\example.txt' -TotalCount 40",
    ),
    (
        "builtin:windows_follow",
        Message::CommandsWindowsFollow,
        "Get-Content -LiteralPath '.\\app.log' -Tail 100 -Wait",
    ),
    (
        "builtin:windows_unique",
        Message::CommandsWindowsUnique,
        "Get-Content -LiteralPath '.\\example.txt' | Group-Object -CaseSensitive -NoElement | Select-Object Count, Name",
    ),
    (
        "builtin:windows_zip",
        Message::CommandsWindowsZip,
        "Compress-Archive -Path '.\\project' -DestinationPath '.\\archive.zip'",
    ),
    (
        "builtin:windows_unzip",
        Message::CommandsWindowsUnzip,
        "Expand-Archive -LiteralPath '.\\archive.zip' -DestinationPath '.\\extracted'",
    ),
    (
        "builtin:windows_system",
        Message::CommandsWindowsSystem,
        "Get-CimInstance Win32_OperatingSystem | Select-Object Caption, Version, LastBootUpTime",
    ),
    (
        "builtin:windows_memory",
        Message::CommandsWindowsMemory,
        "Get-CimInstance Win32_OperatingSystem | Select-Object TotalVisibleMemorySize, FreePhysicalMemory",
    ),
    (
        "builtin:windows_processes",
        Message::CommandsWindowsProcesses,
        "Get-Process | Sort-Object WorkingSet64 -Descending | Select-Object -First 15 Id, ProcessName, WorkingSet64",
    ),
    (
        "builtin:windows_process_detail",
        Message::CommandsWindowsProcessDetail,
        "$processId = 12345; Get-Process -Id $processId",
    ),
    (
        "builtin:windows_stop_process",
        Message::CommandsWindowsStopProcess,
        "$processId = 12345; Stop-Process -Id $processId -Confirm",
    ),
    (
        "builtin:windows_addresses",
        Message::CommandsWindowsAddresses,
        "Get-NetIPAddress -AddressFamily IPv4",
    ),
    ("builtin:windows_routes", Message::CommandsWindowsRoutes, "Get-NetRoute -AddressFamily IPv4"),
    (
        "builtin:windows_ports",
        Message::CommandsWindowsPorts,
        "Get-NetTCPConnection -State Listen | Select-Object LocalAddress, LocalPort, OwningProcess",
    ),
    (
        "builtin:windows_ping",
        Message::CommandsWindowsPing,
        "Test-Connection -ComputerName 'example.com' -Count 4",
    ),
    ("builtin:windows_dns", Message::CommandsWindowsDns, "Resolve-DnsName -Name 'example.com'"),
    (
        "builtin:windows_http",
        Message::CommandsWindowsHttp,
        "Invoke-WebRequest -Uri 'https://example.com' -Method Head -UseBasicParsing | Select-Object StatusCode, Headers",
    ),
    (
        "builtin:windows_acl",
        Message::CommandsWindowsAcl,
        "Get-Acl -LiteralPath '.\\example.txt' | Format-List",
    ),
    ("builtin:windows_identity", Message::CommandsWindowsIdentity, "whoami /all"),
    (
        "builtin:windows_services",
        Message::CommandsWindowsServices,
        "Get-Service | Where-Object Status -eq 'Running'",
    ),
    (
        "builtin:windows_service_status",
        Message::CommandsWindowsServiceStatus,
        "Get-Service -Name 'Spooler'",
    ),
    (
        "builtin:windows_service_restart",
        Message::CommandsWindowsServiceRestart,
        "Restart-Service -Name 'Spooler' -Confirm",
    ),
    (
        "builtin:windows_logs",
        Message::CommandsWindowsLogs,
        "Get-WinEvent -LogName System -MaxEvents 100",
    ),
    (
        "builtin:windows_log_errors",
        Message::CommandsWindowsLogErrors,
        "Get-WinEvent -FilterHashtable @{LogName='System'; Level=2} -MaxEvents 50",
    ),
    (
        "builtin:windows_winget_search",
        Message::CommandsWindowsWingetSearch,
        "winget search --name PowerShell",
    ),
    ("builtin:windows_winget_list", Message::CommandsWindowsWingetList, "winget list"),
    (
        "builtin:windows_winget_install",
        Message::CommandsWindowsWingetInstall,
        "winget install --exact --id Microsoft.PowerShell",
    ),
    ("builtin:windows_winget_upgrades", Message::CommandsWindowsWingetUpgrades, "winget upgrade"),
];
