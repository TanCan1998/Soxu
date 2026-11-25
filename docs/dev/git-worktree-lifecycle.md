# M3e：Worktree 受控创建与只读诊断

本文交付 #47 的设计与证据。后续实现分为 Git 注册、文件检出、工作区接入三个阶段。首个写入 PR 只开放宿主显式发起的**新分支注册**，其成功状态为「已注册，待检出」。Agent 启动、工作区就绪与 `direct` MCP 挂载都需要后续阶段的独立验收。

## 现有能力

设计以 #42（PR #45，`4c5d706`）为基线。现有能力足够建立两类独立证据。

| 入口 | 可证明的事实 | 使用边界 |
| --- | --- | --- |
| [`GitProbe::list_worktrees`](../../crates/soxu-git/src/worktrees.rs) | 从指定绝对目录运行 `git worktree list --porcelain -z`，首项为主工作树，其余为 linked。全批解析 `path`、HEAD、分支与 `locked` / `prunable` 可选原因 | 路径和原因保留原始字节。重复、截断、未知属性或混合 OID 宽度整批失败。路径无法无损转换时保留展示证据，禁止据此定位写入。Git 自身会检查注册路径，Soxu 不额外逐项访问 |
| [`GitProbe::open_local_project`](../../crates/soxu-git/src/project.rs) | 从一个具体目录确认 `WorktreeIdentity { root, git_dir, common_dir }`，或返回普通目录及明确错误 | 根与子目录可以归于同一工作树。核验目标时要求根恰好是目标。`git_dir` 区分工作树，`common_dir` 绑定共享仓库。身份探测不读取完整状态 |
| [`GitProbe` 进程边界](../../crates/soxu-git/src/probe.rs) | 受信任的原生 Git、独立参数、受控环境、有界输出与超时 | 现有只读入口保持原契约。新写入执行器使用独立的命令白名单、权限和结果模型。`GIT_OPTIONAL_LOCKS=0` 只关可选锁，Git 更新所需的锁照常生效 |

## 首版请求与授权

- 宿主确认一个已打开的 `GitWorktree`、一处专用本地工作区根、一个显式基准、一个新分支和一次请求标识。授权绑定源根与 `common_dir` 的规范路径和目录身份、工作区根身份、具体目标、完整基准 OID、完整分支 ref，以及新增 linked 注册及新分支所需的共享 Git 元数据写入。普通目录、bare、身份未知或已漂移时停止。源工作树的文件、HEAD 和 index 不在授权写入范围内。
- 工作区根必须已存在，归宿主管理且与源工作树、其他已知工作树及 Git 元数据目录互不嵌套。首版限权限可验证的专用本地目录。由服务在根内生成一个扁平、便携、唯一的目标目录名，展示名独立保存。拒绝根或祖先的符号链接、junction、reparse point、路径别名、共享写权限或无法验证的文件系统。用逐段路径与文件系统身份比较，覆盖 Windows 大小写、卷边界与保留名称，避免字符串前缀判定。
- 目标必须缺失。目录、文件、链接、空目录、已有 Git 注册（含 `locked`、`prunable`）或未结请求均构成占用。按平台原生等价关系检查注册路径，从注册表中发现不明确的别名时保守拒绝，不尝试打开列表里的外部路径。预检至启动之间再次核对根与祖先身份、目标缺失和预约。Git 通过路径重新打开文件，单次 `canonicalize` 或进程内锁无法约束外部改动。无法维持根的受控写入条件时禁用创建。
- 基准只接收仓库内完整 OID 或 `refs/heads/…`、`refs/remotes/…`、`refs/tags/…` 中的显式引用。先用 `git rev-parse --verify --end-of-options <基准>^{commit}` 固定为仓库对象格式对应的完整非零 OID。拒绝歧义简写、隐式 HEAD、用户给的修订表达式、无提交基准和缺失对象。引用经宿主确认后如已移动，返回冲突并重新确认。实际执行仍传固定 OID，避免二次解析更晚的分支值。全程不 fetch。
- 新分支采用受控便携命名，如 `soxu/<slug>`。用 `git check-ref-format refs/heads/<分支>` 检查完整 ref，同时约束传给 `-b` 的短名。用精确 ref 查询和分支清单判定已存在、大小写碰撞、目录/叶节点前缀冲突及工作树占用。`git show-ref --exists` 的 0 表示存在、2 表示缺失、1 表示其他查询错误。`-b`、Git 的 ref 锁及失败回读共同约束竞态。

相关官方文档：[git-rev-parse](https://git-scm.com/docs/git-rev-parse)、[git-check-ref-format](https://git-scm.com/docs/git-check-ref-format)、[git-show-ref](https://git-scm.com/docs/git-show-ref)。

## 执行、并发和失败留痕

1. **先记意图。** 将请求 ID、授权绑定、源身份、目标、ref、基准 OID、验证时间及执行阶段原子写入宿主受控状态目录。以 `common_dir` 目录身份串行化同仓创建，并跨进程预约规范目标和 ref。不同仓库也不能预约同一目标。首次执行前记录目标和 ref 原先均缺失。同一 ID 与同一参数返回已保存进度。同一 ID 携带不同参数时拒绝。并发不同 ID 命中同一预约时返回占用。Soxu 预约与 Git `locked` 是两套独立状态。
2. **执行前复验。** 持有预约重查源身份、创建授权、配置边界、目标、基准和 ref。以受信任 Git 的绝对可执行文件传入独立参数，从已核验源根执行 `git worktree add --no-checkout --no-track -b <分支> -- <目标绝对路径> <完整提交OID>`。首版固定参数组合。禁用继承的 Git 重定向、交互、外部钩子和自动维护，以进程级覆盖处理配置，避免改共享配置。配置来源、元数据路径、对象可用性或离线约束无法核验时拒绝启动。退出、超时、取消、输出超限都收集为执行结果。回收子进程后才开始复核。进程未确认停止时保留未知状态和预约。
3. **退出后回读。** 从源头读取完整注册清单和精确 ref，分别确认分支存在性与值、目标存在性和注册。对唯一匹配且仍在授权根内的目标才调用 `open_local_project`。要求 `Linked`、目标根相同、目标 `git_dir` 与源不同且属于同一仓库受控管理目录、`common_dir` 身份相同，并复核目标 `HEAD` 的符号 ref 与 OID。注册、ref、HEAD、根和管理目录证据均一致且 Git 成功退出时，进入「已注册，待检出」。比较前后观测与预约，记录 `ref`、注册和目录分别有无。回读结果不是跨 Git 进程的原子快照，变动或不可读时进入待复核。
4. **保留部分结果。** 命令退出非零也可能留下分支、注册、目录或检出文件。记录阶段、退出码、超时/取消类型和已确认的足迹，外部日志仅给出脱敏摘要。启动前失败可明确标记未执行。命令已启动但回读无足迹，只报告「当前观察未见残留」。有足迹报告部分完成。进程中断、身份分歧或观察不足时报告结果未知。即使非零退出后各证据吻合，也记录为「已观测注册，原命令失败」，由显式后续处置决定能否接纳。进程重启后从持久意图做只读复核，禁止自动重发、接管、回退或复用同一目标。

禁止的自动动作包括强制占用、重置分支、锁定或解锁、`repair`、`prune`、移动、移除及清理。仓库与路径都可能在检查间被外部进程改动，后置复核用于发现漂移，不能撤销已经发生的写入。

相关官方文档：[git-worktree](https://git-scm.com/docs/git-worktree)、[git-config](https://git-scm.com/docs/git-config)、[githooks](https://git-scm.com/docs/githooks)。

## 检出与工作区接入顺序

1. **注册。** `--no-checkout` 可产生具备分支、HEAD 和管理目录的 linked worktree，文件与 index 尚未就绪。该选项跳过 `post-checkout`，ref 更新仍须用进程级设置禁用钩子。阶段验收使用上述注册回读，禁止在此阶段运行项目脚本、交给 Agent 或挂载 MCP。
2. **检出。** 后续写入阶段只接受已复核的 Soxu 请求及其固定提交。重新核对目录身份、HEAD、注册与待检出前置状态，目标仅含预期 Git 标记且没有用户文件。候选路径是对新工作树执行 `read-tree <OID>`，再以 `checkout-index --all` 填充，不传 `--force`。上线前须验证目标树、index 一致性、路径冲突、子模块、符号链接、过滤器及属性来源、稀疏/工作树专属配置、缺失对象和平台行为。存在无法约束的 smudge/process、gitlink、路径风险或外部配置来源时首版拒绝。仅检查 `post-checkout` 不能约束检出时的属性过滤。失败后保留阶段与影响范围，禁止借重试覆盖已产生或用户新增的文件。
3. **接入。** 验证目标 index 与固定提交的树条目相符，工作文件按已验证的检出语义可用，并重新确认 Git 身份。宿主随后登记可用工作区。按 ADR-003，仅对已验证的工作区准备目录敏感的 `direct` MCP 上下文。路由失败单独报告，可在重新验证工作区后单独重试；保留既有 Git 结果。`StandaloneNative` 按原生独立模式开放已检出的工作区，无须全局配置改动。仓库提供的 setup 脚本不随裸 Git 注册自动执行。

相关官方文档：[git-read-tree](https://git-scm.com/docs/git-read-tree)、[git-checkout-index](https://git-scm.com/docs/git-checkout-index)、[gitattributes](https://git-scm.com/docs/gitattributes)；[ADR-003](../ops/DECISIONS.md#adr-003magpie-深度自动发现与原生独立降级)。

## 只读诊断与修复授权

默认报告源身份、观测时间、完整注册清单及可选请求留痕。每项注明证据来源是 Git 注册、显式路径核验或 Soxu 留痕。只对调用者明确选择且已授权检查的单个目标追加路径核验，并验证目标属于同一仓库。拒绝外部目标越权核验、无损转换失败及路径竞态。诊断不写请求记录、刷新 index、扫描工作状态、探测所有外部注册路径或执行修复。失败信息保留错误类型与影响范围，附注原因安全转义并限长。

| 观测 | 可报告 | 需要另行授权的处置 |
| --- | --- | --- |
| `locked` 有值或无原因 | Git 注册带锁，`lock` 用于保护注册免遭清理并限制移动或删除，例如工作树位于暂时离线卷 | 解锁须确认目标、锁的归属和理由，避免解除他人的保护。不会自动执行 |
| `prunable` 有值或无原因 | Git 将注册元数据标记为可清理，保留 Git 提供的原因。也可能只是卷暂时离线 | 先恢复可访问性并复查。手工移动导致关联失效时可另审 `repair`。确认永久遗留后才单独评估清理范围、预览与恢复依据。不可将 `prune` 当成单路径操作 |
| `Unborn`、`Detached`、`Bare`、`Unavailable` | 原样报告 HEAD 类型。零 OID 的 `Unavailable` 只表明 Git 未给出可用 HEAD。正常 OID 不证明对象仍在本地 | 补对象、改 HEAD、恢复配置或重建关联须单独审阅，诊断入口不执行 |
| 目标缺失、无法访问、身份不符或清单整体错误 | 若已授权核验则区分具体错误。整批解析失败不返回不完整清单，留痕可单独标为历史信息 | 人工检查源、目标及共享仓库身份。依赖权限变更、修复关联或写入仓库时先授权 |
| 只剩分支、仅有目录或已注册但未检出 | 分开报告观测结果，列出与请求一致和不一致的字段。不能仅凭路径或 OID 相同认领外部资源 | 继续检出、接纳已有结果、删除分支/目录或移除注册是不同操作。逐项核验归属、用户数据、影响范围和回退条件后显式授权 |

## 后续实施范围与验收

建议按下表拆单。路径为后续 Issue 的拟定白名单。共同协议或依赖有变更时先走独立契约 PR。

| 顺序 | 拟定 Owned Paths | 可交付验收 |
| --- | --- | --- |
| 1. 注册与诊断 | `crates/soxu-git/src/lib.rs`、新建 `crates/soxu-git/src/worktree_lifecycle.rs` 和 `crates/soxu-git/tests/worktree_lifecycle.rs`、`crates/soxu-git/README.md`、必要 AOCI 资产 | 以调用者提供的已授权本地根与状态目录支持预检、预约、注册及复核。覆盖大小写/别名/链接越界、基准移动、目录与 ref 占用、同名并发与重启重放、非零退出残留、取消超时、解析失败及拒绝外部路径探测。失败不自动修复。现有清单与身份 API 及错误契约兼容 |
| 2. 安全检出 | 新建 `crates/soxu-git/src/worktree_checkout.rs` 和 `crates/soxu-git/tests/worktree_checkout.rs`，以及 `crates/soxu-git/src/lib.rs`、`crates/soxu-git/src/worktree_lifecycle.rs`、`crates/soxu-git/README.md`、必要 AOCI 资产 | 仅在阶段 1 产出的可验证目标使用固定提交。用夹具证明无外部钩子、过滤器、递归子模块或网络启动，覆盖用户文件出现、路径竞态、所需对象缺失和部分文件落盘。失败禁用 Agent 入口并保留可诊断状态 |
| 3. 宿主接入与 `direct` MCP | 新建 `crates/soxu-daemon/src/state/worktrees.rs`、`crates/soxu-daemon/tests/worktree_lifecycle.rs`、`crates/soxu-magpie/src/worktree_context.rs`、`crates/soxu-magpie/tests/worktree_context.rs`，以及 `crates/soxu-daemon/src/state.rs`、`crates/soxu-magpie/src/lib.rs`、必要 AOCI 资产 | 注册待检出时无工作区启动。检出通过后以单次请求登记工作区，双态和挂载失败分别可观察。只在已授权工作区写入路由资产。消息传输、权限及移动端接入如需更改公共协议，先单独冻结契约 |

## 证据矩阵

本机 Git `2.56.0.windows.4` 核对了 `worktree`、`rev-parse`、`check-ref-format`、`show-ref`、`read-tree`、`checkout-index` 的 `-h`。新增验证在新建的临时仓库及其同级临时目标内执行，禁用系统/全局 Git 配置及交互。夹具以显式空钩子目录运行，钩子对照仅使用返回 37 的无副作用脚本。保留夹具，没有执行清理或修复。

| 验收点 | 已获得证据 | 实施后还需证明 |
| --- | --- | --- |
| 基准和新分支 | Windows 夹具可从完整 ref 解析出 40 位提交 OID。树 OID、缺失 ref 和非法分支被拒绝；`--no-track` 后未设置上游 | SHA-256 创建、歧义输入拒绝、基准变动和 ref 前缀/大小写冲突 |
| 授权根、注册及真实身份 | 空格与 Unicode 目标在 `--no-checkout` 后目录只有 `.git`、index 无条目。注册项的 ref 与 OID 一致，独立 `git_dir`、相同 `common_dir`、精确目标根均复核。只读复核前后夹具路径与文件字节摘要一致 | 根的权限模型、Windows 别名和 reparse point、外部进程路径替换及授权撤销的阻断 |
| 目标占用与并发 | Windows 夹具中 Git 接纳已有空目录。非空目标退出 128，原文件保留，分支已经创建且没有目标注册。两次同分支、不同目标的并发 `-b` 退出 0 和 255，仅一项注册 | Soxu 预检拦截一切已有目标。同目标不同分支、多仓库和多进程预约、重启复核均需要新测试 |
| 注册、检出及失败留痕 | `read-tree` 后文件仍缺失，`checkout-index --all` 后普通跟踪文件落盘。`--no-checkout` 跳过对照钩子。默认检出因对照钩子退出 37 后，分支、注册与文件仍存在。整个夹具的源 HEAD、index 与种子文件字节保持一致 | 部分检出后的重试限制、取消中断、日志脱敏及过滤器/配置防护。当前证据只涉及普通文件与良性钩子 |
| 锁与可清理状态 | 本轮 `--lock --reason` 的理由出现在清单。#42 的现有夹具覆盖 `prunable`、缺失 HEAD、原始注解、拒绝畸形记录及探测前后仓库字节对比 | 将 Git 标记映射为诊断证据，验证读路径不越权、不误判并且不启动恢复命令 |
| 工作区接入与修复 | ADR-003 和现有 `soxu-magpie` 只读发现源码用于界定阶段 | 新注册不得触发 Agent、MCP 或 setup。独立验证实际挂载、降级、脱敏诊断及修复授权。未运行 Unix 的创建、并发或挂载实测 |
