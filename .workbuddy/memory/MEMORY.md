# 项目长期约定 — pebrel

## README 必须随功能更新（2026-11-03 确立）

- 权威规则写在仓库根 `AGENTS.md` 全局规则第 14 行，以及 `CONTRIBUTING.md` 的
  「Small, reviewable changes」第 5 条（英文）与「中文摘要」对应条目。
- 触发条件：新增用户可见功能，或改变用户可见行为。
- 落地要求：同一改动内同时更新 `README.md` 与 `README.zh-CN.md`，归入
  `One Workspace` / `一个工作区` 下已有的对应小节，两侧条目位置与数量对应。
  - Terminals and Sessions / 终端与会话
  - Intelligent Completion / 智能补齐
  - SSH and Files / SSH 与文件
  - AI CLI Workflows / AI CLI 工作流
  - A Native, Configurable Interface / 原生、可配置的界面
- 写法：只写用户看到的效果和前提条件，不写内部模块名、函数名、实现细节。
- 不写 README：纯内部重构、缺陷修复、依赖/构建/CI 改动。
- 现有小节放不下时，先与维护者确认是否新增小节，不硬塞进相邻小节。

## 本地禁止编译，构建走 GitHub Release（2026-11-11 确立）

- 权威规则：根 `AGENTS.md` 全局规则（README 规则之后那条）。
- 本机不跑 `cargo build/check/test/clippy/run/bench`、`rustc`，也不许用仓库外
  `CARGO_TARGET_DIR` 变通；构建/测试/打包由 `.github/workflows/release.yml`
  的 `workflow_dispatch` 或 `v*.*.*` tag 触发，产物从 GitHub Release 取。
- 本地允许：`cargo fmt --all -- --check`、Python 门禁脚本、git、文本处理。
- 要关闭 rust-analyzer 自动 check；发布验证产物写仓库外，不再写 `tmp/`。
- 背景：2026-11-11 仓库达 60 GB，其中 57 GB 是 `target/` 多套 profile 与
  增量缓存；清理后降到 197 MB。安装包备份在 `~/pebrel-release-artifacts-20261111/`。

## 工作区已改名为 pebrel_*（2026-11-16 随上游合并确立）

- 上游把 crate 与目录从 `nebula_*` 改为 `pebrel_*`：`pebrel_app`、`pebrel_terminal`、
  `pebrel_config`、`pebrel_config_derive`、`pebrel_settings`、`pebrel_split`、
  `pebrel-completions`、`pebrel_gpui`、`pebrel_hook`；app 包名由 `nebula` 改为 `pebrel`。
- 只改 crate/目录名，**内部标识符没改**：`nebula_settings.txt`、`NebulaWorkspace` 等
  仍是原名；`-p nebula` 只出现在 release.yml 的旧源码兼容分支。
- 说明文档镜像到 `architecture/notes/pebrel_*`；`architecture/notes/nebula_app/math/`
  是上游唯一保留在旧目录的一篇。
- `.config/nextest.toml` 的串行名单顺序被 `test_ci_native_tests.py` 逐字锁定，
  合并冲突时不要只做集合并集。
- 改名要扫到**打包入口与测试断言**：`grep -rn "\-p nebula\|'nebula,\|nebula/gpui-shell"`
  over `scripts/`、`.github/workflows/`。本轮两次红都是这类残留
  （`scripts/tests/build-windows-product.tests.ps1`、`scripts/package-macos-intel.sh`）。
- 本机无 `pwsh`，PowerShell 测试只能在 Windows runner 上验证；合并后靠读脚本
  比对 `-p` / `--bin` / `--features` 参数清单做静态判定。
