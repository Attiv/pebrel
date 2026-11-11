# Nebula 项目代理指引

## 全局规则

- 严禁利用 PR、Issue 及其评论进行广告宣传、商业推广或引流；核实任何未经授权的推广 PR 或 Issue 后直接关闭，无需再次确认。商业需求须通过项目邮箱沟通，已获批准的赞助按批准范围处理。完整范围、邮箱与审阅边界以 [贡献指南](CONTRIBUTING.md#commercial-promotion-policy) 为唯一权威说明；首次贡献者身份、改动小或 CI 通过不能代替需求审查。
- 修改前阅读 `CONTRIBUTING.md`、`docs/architecture.md` 和 `docs/project-constraints.md`；新 UI 文案同时遵循 `docs/internationalization.md`。
- 规则按目录分层：先遵守本文件，再读取目标文件路径上最近的 `AGENTS.md`。模块细节留在模块目录，不回填到根规则。
- 行数预算与依赖方向由 `architecture/` 声明，使用 `python3 scripts/check_architecture.py --base <PR-base-commit>` 验证；800 行仅提示，2000 行是既有防灾上限。
- 禁止靠抬高预算、删测试、压缩格式、机械分片或静默排除目录让门禁通过。规范存在缺陷时，先给可复现反例，补正反测试并记录经维护者审查的规则修订。
- 核心规则保持单一权威实现；按职责和生命周期拆分，不因文件夹名称或历史模块别名推断耦合。新增长期依赖、跨层接口、持久化和线程模型变更须给出设计依据。
- 普通翻译查询维持已测零分配合同；参数格式化和冷路径另行评估。不能把单机微基准写成普遍性能保证。
- 不得声称仅加入 workflow 或 `CODEOWNERS` 就已启用 GitHub 强制保护；服务端设置及负例 PR 验证须另行获准并核验。
- `docs/` 记录经过核验的当前事实；非平凡改动的决策、被否定方案和事故因果写入与代码职责同路径的 `architecture/notes/`。遵循 [note 规则](architecture/notes/AGENTS.md)，普通样式修改和单文件修复不写 note。
- 用户可见的新功能或可见行为变化，必须在**同一改动**内写进 `README.md` 与 `README.zh-CN.md`：按主题归入 `One Workspace` / `一个工作区` 下已有的对应小节（Terminals and Sessions / 终端与会话、Intelligent Completion / 智能补齐、SSH and Files / SSH 与文件、AI CLI Workflows / AI CLI 工作流、A Native, Configurable Interface / 原生、可配置的界面），两侧条目位置与数量保持对应。只写用户看到的效果和前提条件，不写内部模块名、函数名或实现细节；现有小节都放不下时先与维护者确认是否新增小节，不硬塞进相邻小节。纯内部重构、缺陷修复、依赖/构建/CI 改动不写 README。
- 本地一律不做 Rust 编译与打包：禁止在本机执行 `cargo build/check/test/clippy/run/bench`、`rustc` 或任何会生成 `target/` 的命令，包括把 `CARGO_TARGET_DIR` 指向仓库外的变通做法；构建、测试和安装包统一走 GitHub Actions（`.github/workflows/release.yml` 的 `workflow_dispatch`，或推送 `v*.*.*` tag 触发），产物从 GitHub Release 获取。本地只做不产出构建目录的检查：`cargo fmt --all -- --check`、`scripts/check_architecture.py` 等 Python 门禁、git 与文本处理。同时关闭编辑器 / rust-analyzer 的自动 `cargo check`；发布验证下载的日志、安装包和临时文件一律写到仓库外，不得再写回 `tmp/`。发现 `target/` 重新出现时先查触发来源再清除，不要当成正常缓存保留。

## 分层入口

- 应用、窗口和 UI：遵循 [`nebula_app/AGENTS.md`](nebula_app/AGENTS.md)。
- 终端核心、PTY、VT 和输入：遵循 [`nebula_terminal/AGENTS.md`](nebula_terminal/AGENTS.md)。
- 设置、持久化和语言注册：遵循 [`nebula_settings/AGENTS.md`](nebula_settings/AGENTS.md)。
- 检查器、门禁和治理脚本：遵循 [`scripts/AGENTS.md`](scripts/AGENTS.md)。
- 文档事实与操作手册：遵循 [`docs/AGENTS.md`](docs/AGENTS.md)。
- 版本号、构建、打包、自动更新、GitHub Release、Changelog 或 Release Note：先完整阅读 [`packaging/AGENTS.md`](packaging/AGENTS.md)、[`docs/release-notes/AGENTS.md`](docs/release-notes/AGENTS.md) 和本地 `.agents/memory.md` 的发布章节。
