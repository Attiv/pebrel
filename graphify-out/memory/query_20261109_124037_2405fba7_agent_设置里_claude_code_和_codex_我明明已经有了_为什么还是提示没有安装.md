---
type: "query"
date: "2026-11-09T12:40:37.907423+00:00"
question: "agent 设置里 claude code 和 codex 我明明已经有了,为什么还是提示没有安装,而且hook 也安装点不了"
contributor: "graphify"
outcome: "useful"
source_nodes: ["installed_agents()", "discover()", "shell_executable_directories()"]
---

# Q: agent 设置里 claude code 和 codex 我明明已经有了,为什么还是提示没有安装,而且hook 也安装点不了

## Answer

Expanded via graph vocab: [agent cli shell path]. Fixed shared discovery in ai_hook/integrations.rs: npm-global fallback plus one platform login-shell PATH probe per batch for unresolved agents. platform/agent_path.rs owns the 2-second, 64-KiB bounded probe and byte-safe PATH decoding; provider brand inspection now uses installed_agents() rather than repeated single-agent subprocess discovery. Existing Hook guards remain. 18 focused production/GPUI/process regression tests passed, including a real zsh startup fixture; architecture, platform cfg budget, formatting, UTF-8 and signatures passed. Fresh isolated native GUI detected Claude npm and Codex shell-only prefixes and both real Hook switches completed installation; isolated JSON/TOML writes were verified. Installed freshly built, ad-hoc signed local Pebrel with original bundle ID/version and preserved original app backup. User configuration was not modified and existing user process was not restarted. Other platforms and end-to-end event delivery were not tested; no commit or public release was made. Graph incrementally updated; existing 16 parsing warnings and 205 self-loops remain, no missing/dangling endpoints.

## Outcome

- Signal: useful

## Source Nodes

- installed_agents()
- discover()
- shell_executable_directories()