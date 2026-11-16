---
type: "query"
date: "2026-11-09T11:23:46.872725+00:00"
question: "agent 设置里 claude code 和 codex 已安装却未检测到，Hook 不能点击"
contributor: "graphify"
outcome: "useful"
source_nodes: ["executable_directories()", "find_executable()", "can_toggle()"]
---

# Q: agent 设置里 claude code 和 codex 已安装却未检测到，Hook 不能点击

## Answer

Expanded from original query via vocab: [agent cli claude codex hook installed detect path environment settings]. integrations::executable_directories searches process PATH and fixed fallbacks but omits ~/.npm-global/bin. Production-source probe detects both installed CLIs with terminal PATH, returns None for both with /usr/bin:/bin:/usr/sbin:/sbin, and detects both again when only ~/.npm-global/bin is prepended. can_toggle requires an installed hook or detected executable plus config footprint and helper; false discovery blocks initial hook installation. Installed Pebrel bundle helper and both config footprints exist. No production source or user hook config changed; GUI process environment not yet observed.

## Outcome

- Signal: useful

## Source Nodes

- executable_directories()
- find_executable()
- can_toggle()