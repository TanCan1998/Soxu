# soxu-git：本地项目身份、Git 探测与受控 Worktree

M3f 提供 Worktree 恢复审批预览、受控解锁、无残留结论与重启只读 journal 复核。M3e 提供显式 Worktree 新分支注册、只读诊断和固定提交的受控检出。M3d 提供已跟踪文件的四种只读 Diff，以及只接受已授权 Checkpoint 或完整本地提交的固定端点双端 Diff。M3c 提供 Git Worktree 只读注册清单。M3b 提供打开本地目录的项目身份。M3a 提供 Git 工作树的 HEAD 与状态快照。原有 `CheckpointSummary` 字段及 serde 契约保持不变。

## Worktree 只读恢复预览（M3 #57-2）

`GitProbe::recovery_preview(RecoveryPreviewRequest)` 为 `unlock`、`repair`、`prune`、移除注册与清理部分落盘五类恢复操作建立只读预览契约：输出候选身份、影响对象、证据缺口与不确定性，供宿主生成二次授权摘要。该入口只采集证据。能否写入由下节所述执行入口独立判定。

- **观察状态**：`recovery_preview` 复用 `open_local_project` + `list_worktrees` 建立快照，把设计文档第 5 节表格的全部观察状态分类为 `WorktreeObservationKind`：有效保护锁（清单 `locked` 且授权目标确认为可达目录）、疑似孤立锁（有锁但无目标/意图证据）、prunable 或路径不可达、卷离线与身份漂移、`Unavailable`/bare/HEAD 异常、`Prepared`/`Started` 结果未定、`Pending`/`Disabled` 检出。证据缺口用 `EvidenceGap` 逐项记录，不推断注册路径的存在性或健康度。
- **unlock 预览**：绑定唯一 linked 注册、原始锁理由字节与失去锁保护的影响说明。无 linked 注册、无锁或锁租约证据不足（未知所有者、卷离线、记录不一致）时分别拒绝，保留锁。
- **repair 预览**：原生命令不提供 dry-run 也不支持按候选过滤；预览只在「prunable 候选可绑定显式授权目录目标」时给出候选集合，多候选或存在未授权的 prunable 注册时以 `ScopeUnbounded` 明确拒绝范围界定。
- **prune 预览**：以固定默认过期策略执行 `worktree prune --dry-run --verbose`（禁 `--expire=now`），候选记录逐字节保留（实测 Git for Windows 2.56 把候选清单输出到 stderr、退出码 0），并附预览后完整清单供执行前漂移比对。存在无法确认在线状态的锁定注册时拒绝授权——锁定的失联注册不出现在 prune 候选中，须先裁决锁归属。
- **移除注册预览**：区分失联注册（只走受控 prune，禁止手工删除管理目录）与在线 linked 工作树；在线判定要求授权目标确认为目录，清洁度复用源仓库 `status` 快照，锁保护或清洁度无法证明时拒绝。
- **清理部分落盘预览**：绑定请求 ID，只读回查协调域意图与检出留痕；执行结果未定（`Prepared`/`Started`/`Unknown`）时禁止清理；目标缺失且无异议时才允许进入授权，目录存在或含其他内容时按归属无法证明拒绝。
- **可见性最小扩围（主控裁决放行）**：`worktree_lifecycle::read_intents` 改为 `pub(crate)`、`worktree_checkout` 新增 `pub(crate) read_checkout_journal` 只读访问器，供预览复用既有持久日志解码；不改持久格式、行为或签名语义。
- 验证：`tests/worktree_recovery_preview.rs` 在 Windows 以真实 Git 夹具覆盖有效保护锁、疑似孤立锁拒绝、prune 候选与锁定裁决拒绝、repair 范围拒绝、在线干净/失联移除分类、请求 ID 绑定与未知请求拒绝、相对路径/普通目录拒绝；每个预览前后对夹具整树快照断言零写入。Linux 专属 HEAD 损坏用例由 Unix 门控执行。

## Worktree 恢复执行与 journal（M3 #57-3）

宿主用 `GitProbe::prepare_recovery` 建立审批预览，展示候选、`digest()` 和 `execution_blocker()`。获得本机用户确认后，通过 `confirm(expires_at)` 生成绑定原执行 ID 的确认，再调用 `execute_recovery`。当前原生写命令为 `git worktree unlock -- <目标>`。清理部分落盘只有在同一注册意图复核得出目标缺失、注册缺失、分支 ref 缺失时，才记录 `NothingToClean`。

- 审批摘要使用工作区的 `sha2`，覆盖 Git 版本、预览内容、执行 ID、显式授权目标和协调域及仓库的原生目录身份。解锁另核验目标目录、linked 管理目录、`.git`、`gitdir` 和 `commondir` 的关联。协调域锁内重算预览，再在写 journal 和启动 Git 前复核保留的目录身份。协调域锁覆盖同一状态根内的 Soxu 写者。宿主须持续排除域外写者、活动任务及授权路径的并发替换。
- `prune` 保留 dry-run 候选。写入口缺少每个候选的管理目录身份、永久丢失裁决、未推送 ref、局部对象、reflog、备份证据和可验证的外部写入排除能力，故在取得协调锁前返回 `EvidenceUnavailable`。单候选适用同一门槛。`repair` 缺少移动前后映射与全部触及记录的证明。在线移除缺少目标本身的完整清洁度与恢复来源证明。失联移除依赖整仓 prune 的证据。上述操作均保留只读预览。
- 确认在等锁前、获锁后、写 journal 前和 Git 启动前检查过期与取消。有效期上限 10 分钟，审批预览同样受 10 分钟时限约束。锁等待中到期不会创建执行记录。首次尝试可能创建协调锁文件。若意图已落盘后才到期或取消，则保留该执行 ID 并记录 `NotStarted`，Git 写入保持为零。
- journal 在私有协调域内持久化 `Prepared`、`Started`、`Completed` 和 `Observed`。子任务在调用者退出后继续持锁到结局落盘。运行中的取消、超时或管道异常下，执行者尝试回收直接子进程，记录 `Unknown { stop_confirmed: false }` 并保留预约和接入阻断。重启读取兼容旧记录，旧记录里的直接子进程回收标记无法解除预约。`diagnose_recovery` 和 `recovery_execution_records` 只读回查留痕。
- WSL Linux 正向夹具覆盖受控解锁、并发重放及重启诊断。负向夹具覆盖单候选和多候选 prune、无关联 repair 目标、在线目标含忽略文件、同路径同字节目录替换及等锁过期。原生子进程夹具覆盖启动后的协作取消、超时和输出上限。Windows 协调域缺少稳定目录身份，打开时返回 `UnsupportedFilesystem`。

## 轮次 Checkpoint

调用方用 `CheckpointScope::new()` 创建工作区和会话的私有作用域，并把同一作用域用于该会话后续轮次。`GitProbe::capture_checkpoint(root, &options)` 接收工作树根、轮次和既有的应用独占状态目录。状态目录须在工作树及 Git 元数据之外。Linux 上目录须拒绝组和其他用户访问，Windows 上由宿主提供独占 ACL。同一应用使用同一状态目录协调工作树锁。调用方可从 `CheckpointRecord::summary()` 取得既有的四字段摘要，`changed_files` 统计基树与捕获树之间禁用 rename 配对后的变更路径数。

捕获固定 HEAD 对象，从独立临时 index 建树。未出生分支动态构建对应对象格式的空树。逐个审核跟踪路径及尊重 `.gitignore`、`info/exclude` 的未跟踪路径，再把已审核的文件字节送入 Git。归档提供 `raw-blob` 或 `deleted` 表示，省略 clean/process/LFS、工作树编码及 ident 转换路径。忽略路径、敏感路径或内容、特殊文件、预算耗尽均在 `CoverageManifest` 中可见。忽略目录只记录目录边界。遗漏的跟踪路径沿用基树条目，遗漏条目无恢复权威。预算默认在 1 MiB 提示、限制文本 8 MiB、授权二进制 32 MiB、合计 64 MiB、10,000 路径，并限制 Git 输出和运行时间。

持久化 Intent 后，使用对象格式对应的全零旧值创建 `refs/soxu/checkpoints/<workspace-id>/<session-id>/<checkpoint-id>`，再原子发布应用目录记录。原 HEAD、分支、主 index、工作树文件和普通 reflog 字节保留。`GitProbe::inspect_checkpoint(root, state_directory.join(checkpoint_id))` 在重启后只读核验 Intent、提交树与 ref。若私有 ref 已发布且目录记录尚未落地，返回核验后的发布状态。漂移可重试，CAS 冲突保留已有 ref。文件检查和应用锁提供并发漂移检测，独立进程仍可编辑文件，因而捕获只能提供这些观测支持的覆盖范围。

- Linux 通过 `O_DIRECTORY | O_NOFOLLOW`、`/proc/self/fd`、`dev/ino/ctime/nlink` 固定目录与验证文件。普通文件打开时使用 `O_NONBLOCK`。链接、设备、FIFO、嵌套仓库均不进入快照，原始非 UTF-8 路径以字节保存。
- Windows 从卷根持有未授予 `FILE_SHARE_DELETE` 的目录链，拒绝 reparse point、ADS、保留名、尾随点空格及大小写碰撞。直接从通过校验的文件句柄流式读取，并在前后复核目录链、HEAD、主 index 与已纳入路径。观测记录标注 Windows 纯 `std` 不提供跨重启 NTFS FileId 与 `nlink` 硬链接计数检测。Linux 与 Windows 的原语覆盖范围不同。
- 其他平台在读取候选文件和发布私有 ref 前返回 `UnsupportedPlatform`。

## 消费接口

- `GitProbe::new(absolute_git_executable)`：绑定调用方信任的原生 Git 绝对路径，Windows 拒绝脚本 shim。
- `GitProbe::discover()`：从 PATH 的绝对目录查找原生入口；发现不代表签名或可信性证明，安全敏感宿主应使用 `new`。
- `GitProbe::open_local_project(absolute_directory).await`：返回 `LocalProject::Directory { root }` 或 `LocalProject::GitWorktree(WorktreeIdentity { root, git_dir, common_dir })`。路径均规范化为绝对路径。同一工作树的根与子目录返回相同身份，linked worktree 保留自己的根和 `git_dir`，并与主工作树共享 `common_dir`。
- `ProjectOpenError`：分别报告 bare 仓库、元数据目录和可识别的损坏。输入错误、访问失败、其他 Git 运行错误和无法确定的探测结果保留在 `Git` 分支中。
- `GitProbe::probe(absolute_directory).await`：接受工作树根或其子目录，返回 `RepositorySnapshot` 的绝对 `root`、独立 `git_dir`、共享 `common_dir`、`head` 与 `status`。linked worktree 的两个 Git 目录分开返回。
- `HeadState::{Attached, Detached, Unborn}`：分支名保留原始字节；对象标识为完整 SHA-1 或 SHA-256，unborn 不伪造 HEAD。
- `parse_status(&[u8])`：纯函数，解析 `git status --porcelain=v2 -z`。`StatusEntry` 区分普通变更、重命名/复制、冲突、未跟踪与忽略文件；保留 XY、子模块四位状态、模式、对象标识与相似度。未知记录类型、非法字段或截断均返回 `ParseError`，不返回部分快照；同一流的头部、记录间及记录内 OID 长度必须一致，不能混用 SHA-1/SHA-256，零 OID 同样校验。未来头部字段保留原始键值。
- `GitPath::as_bytes()`：原始 Git 路径身份；serde 输出字节数组而非有损字符串。`to_path_buf()` 在 Unix 保留非 UTF-8，Windows 对非 UTF-8、特殊字符及可能归一化的保留名/尾随字符明确拒绝。该转换不是文件访问沙箱，不负责链接、大小写或路径竞态裁决。
- `GitError`：区分入口/目录错误、非仓库、无工作树、外部过滤器、子模块、命令失败、超时、输出超限、路径编码及解析失败。命令失败保留原始 stderr，Display 不输出仓库路径或 stderr。

## Worktree 注册清单

- `GitProbe::list_worktrees(absolute_directory).await`：可传 `WorktreeIdentity.root`，仅以输入目录运行原生 `worktree list --porcelain -z`；直接复用现有 `run_command`，不读取 status、不重写配置隔离、超时及输出边界。bare 输入可报告 bare 主条目，不改变 `open_local_project` 对 bare 的拒绝契约。
- `WorktreeInventory.entries` 保留 Git 输出顺序，官方规定首条为 `Main`，后续为 `Linked`；不按路径重排、不依据文件存在性猜身份。
- `WorktreeHead::{Bare, Attached, Detached, Unborn, Unavailable}` 保留完整 SHA-1/SHA-256 HEAD；零 OID 加 branch 表示 unborn；零 OID 无 branch（包括 Git 在缺失/损坏 HEAD 时输出的 detached）表示 `Unavailable { oid, detached }`，不推断提交存在性或健康。branch 保留完整 ref 原始字节，按 Git `check-ref-format` 规则拒绝非法 ref（含空白、控制字节、`..`、`@{`、保留字符、空成分及 `.lock`），合法非 UTF-8 不转码。
- `WorktreePath::as_bytes()` 与 `WorktreeAnnotation.reason` 保留原始字节；注解缺席、无原因、显式空原因分别表示。不裁剪空白、不解引号，locked/prunable 原因可含换行或非 UTF-8。`to_path_buf()` 不访问路径，Unix 保留非 UTF-8/换行；Windows 接受 Git 正斜杠盘符与 UNC 路径，对非 UTF-8、控制/保留字符及可能归一化的名称明确拒绝，不有损重建。
- `parse_worktrees(&[u8])` 全批读取 NUL 终止的属性，空属性结束记录；非法或截断、重复路径/属性、状态冲突、混合 OID 宽度及未知未来字段返回 `WorktreeParseError`，不返回半份清单。仅接受 Git 绝对规范路径（根路径合法，非根拒绝空/`.`/`..` 成分与尾随分隔符），不 canonicalize 清单条目。
- 清单是注册事实，不是健康报告；Git 自身可能检查注册路径以生成 prunable，Soxu 不额外遍历或打开注册路径，只报告 locked/prunable，不创建/删除/repair/prune，不声称路径存在或可安全删除。`WorktreeInventoryError` 分别保留原有 Git 错误与清单解析错误；Display 不泄露原始路径、原因或 stderr。

格式依据：[官方 git-worktree](https://git-scm.com/docs/git-worktree#_porcelain_format)、[官方 git-check-ref-format](https://git-scm.com/docs/git-check-ref-format)，以及 Git `builtin/worktree.c` 的 `show_worktree_porcelain`。真实临时夹具覆盖主/linked、unborn、detached、bare、locked（有/无原因）、移动目录形成的 prunable、空格/Unicode；探测前后逐字节及修改时间快照相同。纯解析器覆盖 NUL、换行、非 UTF-8、截断、非法 ref/路径和未来字段；Unix 专属真实换行及非 UTF-8 路径测试不等同于本次 Windows 已执行验证。

## Worktree 受控注册与只读诊断

- `GitProbe::register_new_worktree(&coordinator, WorktreeRegistrationRequest)` 执行宿主显式授权的新分支 linked Worktree 注册，终态是「已注册，待检出」。请求绑定持久请求 ID、源 `WorktreeIdentity`、专用工作区根、直接目标名、完整 `refs/heads/soxu/…` 分支和固定完整基准 OID。请求中没有状态目录字段。协议、Daemon、项目脚本和 MCP 接入属于后续阶段。
- 预检逐段拒绝链接和 reparse point，绑定规范路径与平台原生目录身份。源、共享 Git 目录、状态根和授权工作区在启动前复验。专用工作区可包含多个直接子目录中的 linked Worktree，当前目标必须独立且缺失。工作区、目标和注册条目存在真实重叠时拒绝。
- 可信宿主以 `WorktreeRegistrationCoordinator::open(state_root, legacy_state_roots)` 打开全域协调能力。所有可能触达同仓或同目标的进程及其重启实例共用同一个预先建立的私有持久目录。目录必须和仓库、Git 管理目录、工作区互相隔离。宿主负责同域配置、旧写者停用及旧状态根清点，确保状态根、锁文件、目录项、授权路径及其祖先持续受控。请求参数无法选择协调域。Unix 要求协调根只允许所有者访问，并要求本地文件系统提供有效的跨进程文件锁、原生目录身份、原子重命名及目录同步。域外写入者、可替换的路径或存在交叠的文件系统别名需要宿主排除。
- 域内固定的 `registration.lock` 串行化预检、意图写入、Git 执行与回读。仓库和 ref 预约使用共享 `common_dir` 原生身份，ref 覆盖目录前缀冲突。目标预约使用工作区根原生身份与等价目标名，跨仓检查，并保留原路径占用保护。执行者持锁至直接子进程回收、复核和记录完成。若重启时仍无法确认原进程已停止，留痕继续保留目标和 ref 预约，并阻止该仓库的新注册。
- 预检与启动由私有写执行任务串接：先持久化 `Prepared`，任务内最后刷新全部授权目录、配置/对象边界并复验目标缺失，再切换 `Started` 并启动 Git。Unix 夹具覆盖“意图已落盘后出现目标”的竞态；Windows 当前在授权入口即 fail-closed，不会到达该屏障。
- `SOXUWT3` 持久化状态根、工作区、源目录和 Git 目录的设备号、inode 与原生创建时间。初次复核成功时再记录新目标及其管理目录身份。无法取得原生创建时间时拒绝注册。身份和参数一致的重放返回原执行记录与时间戳。独立写执行者记录启动失败、退出码、超时、输出限额和直接子进程回收结果。调用者取消后执行者继续持锁。
- 启用统一域前，宿主须停用旧写者并把所有旧状态根交给 `open`。发现其它根有意图或临时留痕时拒绝启用，原路径仍可对身份完整的记录做只读诊断。原根就是统一域时，状态根身份一致的 `SOXUWT3` 可继续读取和预约。域内缺少身份的 `SOXUWT2` 对相同绑定返回原记录和待复核状态，并阻止新注册。带有异域身份的记录同样阻止新注册。旧记录保留原位及原字节，迁移或解除预约需要宿主另行裁决。
- 写入命令固定为受信任 Git 上的 `worktree add --no-checkout --no-track -b <分支> -- <目标> <OID>`，禁用钩子、替换对象、延迟获取和自动维护。回读要求前后两次观察的目标注册和 ref 一致，并再次核对源和已授权目标身份。目录单独存在也作为足迹报告，归属取决于独立的注册和身份复核。分段读取期间发生漂移时保留原执行结果，整体返回待复核。
- `GitProbe::diagnose_registration(state_root, request_id, source_root)` 默认只读源身份、完整注册清单、ref/locked/prunable 和意图留痕，`TargetState::NotExamined` 表示未触碰目标。`diagnose_registration_with_target(..., target_authorization)` 只在目标与持久意图和授权工作区精确绑定后才读取单个目标。未知请求返回 `None`；源根不匹配、状态域漂移、意图损坏、Git 失败或身份漂移失败关闭。诊断不写意图、不扫描未授权外部路径、不执行修复。
- 状态文件读取上限 64 KiB，意图文件与累计字节另有上限。Windows 注册仍返回 `UnsupportedFilesystem`。启用前需要从持有的 NTFS 目录句柄取得卷及文件 ID，核验 DACL、祖先及子项写权限，并由宿主持有可撤销的持续授权。平台封装的依赖及宿主接入范围由主控另行协调。临时夹具留在测试临时根。
- 本轮 Windows 在 Rust/Cargo 1.99.1、Node 26.13.1、pnpm 12.21.0 的 `mise exec` 环境执行专项与七项门禁，缓存隔离为 `target/soxu-49-pi`。生命周期集成仅运行 Windows fail-closed 用例；执行器另用原生测试二进制实测成功、spawn 失败、退出码、输出超限和超时回收，不证明 Windows 目录授权、合法创建或 DACL 可用。Unix 注册、崩溃和重放用例本轮未执行：本机 WSL 启动报 `ERROR_BAD_EXE_FORMAT`，须由可用 Unix 环境补验。
- 参考固定 HEAD：paseo `7bcd598f` 的 `validateWorktreePath` 与创建后注册回读、codeg `028d30ed` 的 `check_name` 和占用拒绝、padu `fbe97a38` 的 `create_in` 专用根及平铺目标。Soxu 保留严格新目标、固定 OID、失败留痕与待检出语义，不采用自动清理或宽松已有路径。参考 AOCI 仅执行 `aoci status` 与读取参考索引文件 + 源码核实，未做参考仓完整认知校验；CodeGraph 精确 `node --path` 查询用于导航，paseo 图仍有已知漂移/解析缺口。
- 注册入口先检查请求 ID、目标目录名和分支引用的格式。Windows 在打开协调能力时即拒绝，且不会创建锁、意图或目标。Unix 专项可运行 `cargo test --locked -p soxu-git --test worktree_lifecycle`，其中独立测试进程覆盖同仓跨父目录串行、跨仓目标冲突、注册所有者终止后的持久预约及只读重放。宿主接入须在启用前验证部署所用文件系统与统一域边界。

## Worktree 受控检出

- `GitProbe::checkout_registered_worktree(&coordinator, &registration).await` 在原协调域中重读 #49 的注册意图。仅接受目标目录只含原 Git 标记、目标分支与 HEAD 仍指向固定完整 OID、管理目录尚无 index 的登记。分支需保留可直接核验的 loose ref。
- 进程只读取有限的本地对象副本与受控配置。在私有暂存目录运行 `read-tree <OID>` 与 `checkout-index --all`，校验 index 树条目。拒绝 gitlink、符号链接、路径别名、过滤器、外部属性来源、稀疏检出、工作树专属配置、外部或缺失对象。仅支持受控的内建文本和换行属性。
- 采用目录句柄约束发布路径。index 和各工作文件以独占创建方式发布，随后核对工作文件、index、目录身份及 HEAD。单次检出的持久记录先于写入，失败、取消或失联后保留阶段、原因及可能的文件残留，原请求拒绝再次检出。暂存留痕由受信任宿主管理。
- `GitProbe::checkout_status(&coordinator, request_id)` 可读取 `Pending`、`Disabled` 或 `Verified`。`Verified` 对应本次检出完毕时的复核，宿主登记、当前工作区接入、Agent 启动和 `direct` MCP 仍由第三阶段负责。
- 当前文件发布使用 Linux 的目录句柄与 `/proc/self/fd`。Windows 保留 #49 的 `UnsupportedFilesystem`；预检、Git 子进程约束及持久状态在 Windows 通过独立夹具验证。缺少稳定目录身份、句柄路径或同卷 index 发布能力时拒绝执行。

## 已跟踪文件只读 Diff

`GitProbe::readonly_diff(absolute_worktree_root, DiffRequest).await` 接受精确路径字节列表，可以直接使用 `GitPath::as_bytes()`。列表为空时返回空元数据，空授权请求正文则报错。调用方可通过 `DiffOutput::Review(path)` 审阅授权集合中的一个文件。

| `DiffBaseline` | 比较方向 |
| --- | --- |
| `Unstaged` | index → 工作树 |
| `Staged` | HEAD → index。unborn 时返回全部已授权暂存新增 |
| `Uncommitted` | HEAD → 工作树净变化。unborn 时返回 `UnbornHead` |
| `BaseToWorktree(ref)` | 显式本地提交 → 工作树。支持 `HEAD`、完整 SHA-1 OID、`refs/heads/`、`refs/tags/` 和 `refs/remotes/` 下的完整引用 |

- 路径须精确存在于 index 或所选基准树。基准树独有的文件可显示为删除。检查祖先、叶节点与两端目录前缀，拒绝目录、内嵌仓库、符号链接和 reparse point。Git 使用 literal pathspec 与独立 `--` 参数，NUL 元数据回读时再核对授权集合。
- `DiffChange` 保留原始路径、增删改、模式、Git 提供的 OID 与二进制标志。Git 的 raw 输出可能含 stat 变化的候选，正文实际抵消时不列入净变化。二进制审阅只返回 `DiffReview::Binary`。文本 patch 保留原始字节，可能带 Git 的内建换行或编码转换。Git 为选中路径计算 `numstat` 时可能读取文件正文。
- 请求最多授权 128 个文件、合计 8 KiB 路径字节。单命令 15 秒、每流 16 MiB，请求总计 45 秒、64 MiB 子进程输出。超限失败，无部分结果。Git 子进程超时后终止并等待直接子进程。请求取消沿用 `kill_on_drop`，系统调用耗时和 Git 内部峰值内存仍受宿主控制。
- 使用可信原生 Git 和受信任本地仓库，由宿主串行化更新。配置预检在卷根以显式非仓库 `GIT_DIR` 和 `config --file --no-includes` 读取选定配置。Git 的 `config` 在解析子命令参数之前会执行仓库发现，必须隔离其运行上下文。拒绝 includes、clean/process、外部对象库和 partial clone。禁用 external diff、textconv、替换对象、自动刷新 index 与用户/系统 attributes。Git 可读取工作树、index 或仓库内的 attributes。
- 拒绝冲突、intent-to-add、特殊索引标志、sparse/split index、SHA-256、gitlink 及相关路径的不支持类型。缺失对象或命令失败直接返回错误。复核工作树身份来源、配置、index 字节、选中路径元数据和解析后的提交。`DiffObservation` 记录观察时间与解析端点，文件系统竞态仍可能逃过复核。部署时由宿主负责仓库信任与隔离。

Windows 专项夹具验证四种基准、unborn、linked/detached 独立 index、基准树独有删除、二进制、CRLF 与非 UTF-8 文本。独占锁令未授权文件正文确实无法读取，同次选择其他文件仍能得到审阅结果。50,000 行夹具的元数据查询成功，超过 16 MiB 的审阅整个失败。配置、index、对象库及其余夹具文件的字节和 mtime 前后相同。Unix 真实换行、非 UTF-8 路径和链接替换用例由 Unix 门禁运行。

## 受控仓库 Init 与本地源 Clone

`RepositoryCreateCoordinator` 绑定宿主私有协调域；`RepositoryCreateRequest` 是一次性结构化授权：请求 ID、操作、已打开受控父根、直接目标名与固定参数。`GitProbe::create_repository` 只在协调域暂存目录内执行两种 Git 写入：`git init --template= --initial-branch=main -- <暂存仓库>` 与本地源 `git clone --no-local --no-checkout --no-recurse-submodules --template= -- <源> <暂存仓库>`，随后由本进程把结果逐段句柄锚定并独占发布到目标；`GitProbe::diagnose_repository_create` 只读回查，不接受调用方另行提供的目标路径。

- Init：默认只接受缺失目标；`InitMode::VerifiedEmptyDirectory` 显式接纳经持锁复核仍为空的纯空目录（含隐藏项全量枚举）。拒绝非空目录、现有仓库、bare 结构、损坏 gitfile、祖先仓库、链接与 reparse point。任何失败都保留用户预建的原空目录。
- Clone：首版只开放本地绝对的普通仓库来源；`--no-checkout` 收下元数据后由 `repository_create/checkout` 用 `git ls-tree -r -z` 预检、再用 `git cat-file --batch` 逐对象发布工作树，从不调用 `git checkout` / `checkout-index`，因此不执行钩子、Git LFS、子模块、外部过滤器或项目脚本。拒绝 bare、关联工作树、UNC、alternates、promisor、shallow 与含 `120000`/`160000` 的提交。受控 HTTPS 与任何 `ext::`、`file://`、`git://`、`ssh://`、scp 风格来源一律以 `Unsupported` / `UnsupportedSource` 关闭。
- 私有暂存与发布：Git 只在协调域内的宿主私有暂存目录（`sandbox/<请求 ID>/work/repo`）执行 `git init` / `git clone`，从不直接写最终目标；目标侧由本进程按「逐段句柄锚定 + 独占创建 + 不覆盖」发布 `.git` 与工作树。每一级目录都先在上一级句柄的锚定路径下创建（Linux `/proc/self/fd/<fd>`，Windows 持有不共享删除的目录句柄），随后立刻打开该级句柄并入句柄链，写入只发生在已核对身份的那一级目录内，不用「单一根句柄 + 多段字符串拼接」。目标缺失时在受控父根句柄下原子 `create_dir`；显式空目录沿用预检取得的句柄直到复核结束。
- 源私有快照：本地源先经句柄链核验（源根与 `.git` 都由上级句柄派生，拒绝 bare、关联工作树、链接、reparse、shallow 与 `objects/info/alternates`），再在持有源句柄期间把 `.git` 的允许布局（`HEAD`、`config`、`packed-refs`、`refs/`、`objects/`、`info/`、`branches/`、`description/`、`hooks/`、`logs/`、`index`、`COMMIT_EDITMSG`、`ORIG_HEAD`）逐段复制到协调域暂存；未列出的顶层条目、链接、reparse、特殊文件一律 `UnsupportedSource`，条目 20 万、单文件 64 MiB、总量 512 MiB 预算按实际流量执行。捕获前后各核一次源身份，`git clone` 的输入只能是快照，捕获完成后子进程不回读原源。
- 预检与发布顺序：`ls-tree` 枚举与路径校验通过后，先做 `cat-file --batch-check` 的对象类型/体积预检与全部 `.gitattributes` 内容校验，再开始任何写盘；任一步失败都不在目标留下任何工作树文件或 index。
- 发布预算与平台语义：目录与文件同计 20 万条目、单文件 64 MiB、总量 2 GiB；复制按实际流量 `take(limit)` 限长、超限探测只读并 `sync_all`，Unix 保留可执行位，Windows 无对应语义。复制不依赖硬链接，同卷与跨卷走同一路径；跨卷未在本轮实测，不声称已验证。
- 发布后复核：Clone 在最终目标重新解析 HEAD、比对 `ls-files --stage` 与 `ls-tree -r` 的 mode/OID/路径，并要求 `diff-index --cached --quiet` 与 `status --porcelain=v2 --untracked-files=all` 均为空；因此正常克隆后独立原生 `git status` 是干净的；无法解出已固定 HEAD 或身份不一致时不授予 `Verified`。
- 留痕：协调域内 `SOXURC1` 逐行 `key=value` 文本记录（单文件 ≤ 64 KiB、原子 `tmp` → `rename` 加目录同步、字段 `%` 转义）。阶段为 `Prepared` → `Started` → `Observed`，终态为 `NotStarted` / `Verified` / `PartialObserved` / `Unknown`。只有 `SpawnFailed`（确证未启动）才记 `NotStarted`；命令已启动但无 Git 元数据、或成功却看不到 `.git` 均记 `PartialObserved` 并保留观察事实；复核出错记 `Unknown`。同 ID 变参报 `RequestConflict`；`result` 或终态为 `Unknown` 时拒绝重放；从不自动删除、覆盖或迁移目标。记录保留精确源路径用于变参比较，`notes` 只写固定分类标记，不写源路径或原始错误串。
- 运行隔离：受控 Git 在协调域沙箱内运行，清除继承的 `GIT_*`、`SSH_*`、`GPG_*`、`LC_*`、`XDG_*`、代理与 home/临时目录变量，隔离 HOME/XDG/临时目录与全局配置，用宿主私有空目录覆盖 `core.hooksPath`，`protocol.allow=never` 且只放行本次操作声明的协议；单命令 120 秒、输出 16 MiB 上限。只读回查与重复请求使用 `prepare_readonly`：不建目录、不写 `gitconfig`（缺失时 `GIT_CONFIG_GLOBAL` 指向空设备），不产生任何写盘。
- 回收分型：超时、输出超限与 IO 失败会终止并等待直接子进程，`direct_child_reaped` 只在 `wait()` 成功后 `try_wait()` 确证已回收时为真；该字段只覆盖直接子进程，不覆盖其派生的孙进程（例如本地克隆的 upload-pack），记录不会把直接子进程回收等同于整棵进程树已回收。
- 配置与属性白名单：来源仓库与产物配置拒绝 `include`/`includeIf`/`filter`/`submodule`/`lfs`/`alias` 段，拒绝非空的 `hooksPath`、`askPass`、`sshCommand`、`fsmonitor`、`editor`、`pager`、`worktree`、`alternates`、`external`、`textconv`、`driver`、`uploadPack`、`receivePack`、`worktreeConfig`、`partialClone`、`preciousObjects`；提交树内的属性文件一旦声明 `filter=`、`textconv` 或 `working-tree-encoding` 即拒绝发布。

平台边界（如实披露）：Linux 是受支持的写入平台，用 `O_DIRECTORY | O_NOFOLLOW` 从 `/` 句柄开始逐级打开（每级用上一级 fd 的 `/proc/self/fd/<fd>/<name>`，等价 `openat`），目录身份为 `dev:ino` 加创建时间；宿主侧写入用 `/proc/self/fd/<fd>` 锚定，Git 子进程经 `/proc/<host_pid>/fd/<fd>` 访问已授权暂存。授权前提是可证明的宿主私有根：属主必须等于 `/proc/self/status` 的**有效 UID**、`mode & 0o077 == 0`，且文件系统必须在白名单（`ext2/ext3/ext4/xfs/btrfs/f2fs/tmpfs/ramfs/zfs/overlay/drvfs`）内——通过目录句柄的 `mnt_id` 与 `/proc/self/mountinfo` 绑定判定，未知类型与无法定位挂载一律拒绝。同 UID 的其他进程与具特权进程不在威胁模型内。Windows 缺少目录创建与打开的原子锚定原语（无 `openat` 等价接口；`FSCTL_SET_REPARSE_POINT` 允许对有 `FILE_WRITE_ATTRIBUTES` 的空目录就地改写 reparse 属性），`GitProbe::create_repository` 在写入入口直接 `UnsupportedPlatform` fail-closed，早于加锁、留痕与沙箱创建；只读 `diagnose_repository_create` 仍可用。重新开放 Windows 需要：相对父句柄的原子创建（`NtCreateFile` + `RootDirectory`）、创建后 file id 前后比对、以及阻断空目录 reparse 就地改写的隔离或原生接口；在获得这些证据前不授予 `Verified`。其余平台（macOS、BSD）按同一契约返回 `UnsupportedPlatform`/`UnsupportedFilesystem`。验证：`tests/repository_create.rs` 在 WSL Ubuntu 26.04.1 LTS 通过 27 项正向用例，在 Windows 通过 2 项拒绝用例（写入零残留、只读诊断零写入）；`cargo test -p soxu-git --locked` 在 WSL 通过 25 + 8 + 12 + 7 + 9 + 5 + 27 + 7 + 6 + 17 + 11 项，在 Windows 通过 22 + 8 + 13 + 7 + 8 + 5 + 2 + 7 + 1 + 1 + 10 项；两平台 `cargo clippy -- -D warnings` 零告警。Windows 不列为正向验收。其余 Unix 未验证。

## 固定端点只读 Diff

`GitProbe::readonly_fixed_diff(root, FixedDiffRequest).await` 比较两个**已授权固定端点**。它不扩展 `DiffBaseline` 的四个基准，也不接受任意 ref 字符串：端点只有两种可验证来源。

| `FixedDiffEndpoint` | 解析与校验 |
| --- | --- |
| `AuthorizedCheckpoint { state_directory, scope, checkpoint_id }` | 只接受服务端已发布记录：`inspect_checkpoint` 复核 Intent/记录，工作树 `root`、`git_dir`、`common_dir` 与目录身份必须与本工作树一致，`workspace_id`/`session_id`/`checkpoint_id` 必须属于给定作用域，私有 ref 不得为符号引用且其 OID 必须与记录一致，提交、树与 `worktree_tree_oid` 必须真实存在 |
| `AuthorizedBaseCommit(oid)` | 完整 40 位 SHA-1 或 64 位 SHA-256 十六进制 OID。缩写、`HEAD`、`refs/*` 等任意引用一律 `InvalidCommit`；对象必须可从本地分支、标签、远端跟踪分支或 HEAD 到达，私有 `refs/soxu` 提交返回 `UnreachableCommit` |

`GitProbe::readonly_checkpoint_diff(root, checkpoint, paths, output)` 是「已授权 Checkpoint → 当前工作树」的封装：先固定完整提交 OID，再复用同一受控只读比较。结果中 head 端 `mutable = true` 且 `commit_oid`/`tree_oid` 为空，显式标注为可变观察，不伪造历史身份。

- `FixedDiffResult` 携带两端 `FixedDiffIdentity`（工作树身份、会话归属、提交/树 OID、观察时间）、两端 `FixedDiffCoverage`、`selected_paths`、逐文件 `FixedDiffChange`、可选 `review` 与 `incomplete_reason`。
- `FixedDiffChange` 支持新增、删除、修改与重命名（`old_path`），保留模式、完整 OID、二进制标记与 `numstat` 行统计。二进制审阅返回 `DiffReview::Binary`，未变更返回 `Unchanged`，文本补丁上限 1 MiB。
- `FixedDiffIncomplete` 记录 `BaseOmissions`、`HeadOmissions`、`PathSelection`、`PathBudget`、`OutputBudget`、`ReviewBudget`、`UntrackedWorktreePaths`。任一端 Checkpoint 存在遗漏、或命中任何路径/输出/审阅预算时，`is_complete()` 必为 `false`；空 `changes` 不能覆盖这些事实。
- 工作树端的未跟踪与被忽略路径（`ls-files --others --exclude-standard`、`--ignored`）只记录在 `head_coverage.untracked_paths`，不读取内容，也不进入比较集合。因此含未跟踪路径的工作树比较永不声称完整零差异。
- 请求路径为空时在两端已跟踪集合内枚举；显式授权时按 `native_file_path` 校验、拒绝重复与越界，且仍以 `PathSelection` 标记「非全仓库」语义。预算沿用 128 文件 / 8 KiB 路径字节，超限记录 `PathBudget` 并保留已选前缀。
- 沿用既有只读边界：`--no-ext-diff`、`--no-textconv`、`--ignore-submodules=all`、`--no-replace-objects`、literal pathspec、禁用自动刷新 index 与 optional locks、`GIT_NO_LAZY_FETCH=1`，关闭 rename 之外的启发式（固定 `--diff-algorithm=myers`、`--no-indent-heuristic`）。不比固定端点组合时不做 worktree 戳记；工作树比较前记录 index 与选中路径元数据，结束后复核并要求完全一致，否则 `Changed`。
- 固定端点单独允许 SHA-256 对象格式，四个既有基准仍拒绝 SHA-256。比较前后复核 `Layout`、两端解析结果与工作树戳记，不建临时分支、不改 HEAD、主 index 或工作树。

专项夹具 `tests/fixed_diff.rs` 覆盖 Checkpoint→Checkpoint、AuthorizedBaseCommit→Checkpoint、Checkpoint→工作树、跨会话与跨工作树拒绝、私有 ref 删除与 OID 漂移、缩写与任意引用拒绝、含遗漏检查点的 `incomplete_reason`、路径选择与预算、SHA-256、unborn、linked worktree、二进制、重命名，并断言比较与审阅前后全仓库文件字节与 mtime 不变。

## 只读与限制

所有子进程直接传参数并设置独立 `current_dir`，不启动 shell，不改变全局 cwd 或环境。子进程清除继承的 `GIT_*`，忽略系统/全局 Git 配置，保留仓库自身的语义配置；禁用 optional locks、fsmonitor、untracked cache、自动维护、子模块摘要和按需 fetch，关闭交互。单命令超时 15 秒，stdout/stderr 各上限 16 MiB；任一流读到上限加一字节立即返回 `OutputLimit`，不等待 EOF 或自然退出而退化成 `Timeout`。超限、超时或读取失败会终止并等待回收直接子进程，不承诺回收恶意入口派生的整个进程树。

`open_local_project` 只运行 `rev-parse`。只有 Git 明确报告没有仓库，并且沿途没有发现疑似被跳过的 Git 元数据，才返回普通目录。现存的 `.git` 文件、目录或链接，以及 `objects`、`refs` 与 `HEAD`、`config` 形成的独立 Git 目录特征，或成对出现的 `commondir` 和 `gitdir` 文件，都按疑似损坏保守拒绝。若 Git 跳过损坏的内层 `.git` 并找到外层工作树，入口也会拒绝打开。损坏判定限于发现身份所需的元数据。索引、对象库和引用完整性不在此检查范围内。已观察到的访问失败和文件系统边界等无法确定归属的诊断直接返回错误。

`status` 可能经 clean/process 过滤器或子模块配置执行外部程序，因此遇到仓库外部内容过滤器或索引 gitlink 时提前失败关闭。未初始化子模块也明确不支持；纯解析器仍完整支持子模块标记。过滤器查询显式使用 `config --includes`，覆盖本地配置、已启用 `extensions.worktreeConfig` 的 linked worktree 配置及其 include/生效的 includeIf；系统/用户配置不参与。配置查询失败会拒绝探测，不自动降级为忽略配置继续运行。探测没有 OS 级沙箱，不承诺面对恶意 Git 可执行文件、配置读取与启动之间的篡改竞态或并发 Git 变更时提供原子快照；调用方应串行化更改，并信任所绑定原生入口。

`probe` / `open_local_project` 暂不支持 bare 仓库和 `.git` 元数据目录；`list_worktrees` 可报告 bare 主注册条目。忽略文件只在纯解析器输入中支持，探测默认不枚举。无新第三方依赖，临时夹具使用标准库，构建缓存与 AOCI 运行资产沿用已有忽略规则。

## 验证与后续

`mise exec -- cargo test -p soxu-git --locked` 覆盖 unborn/attached/detached、子目录、linked worktree、暂存/未暂存、Unicode/空格、冲突、目录错误、fsmonitor 与过滤器拒绝，以及解析器的换行、非 UTF-8、复制、七类冲突、SHA-256 和非法输入。临时内嵌 Git 工作树实测 `--untracked-files=all` 仍返回带单个 `/` 后缀的 `?` 目录记录；忽略枚举返回对应 `!` 目录记录。仅这两类允许单个目录后缀并保留原始字节，跟踪、重命名源/目标和冲突路径仍拒绝目录后缀；所有类型均拒绝内部空分量、双斜线、绝对路径及 `.`/`..`。探测前后比较整个临时仓库的文件列表与字节，确保索引和对象未变。Unix 专项真实路径用例只在 Unix 执行；Windows 上执行纯字节解析与拒绝转换用例。

`tests/repository_create.rs` 覆盖缺失目标与显式空目录 Init、空目录被写入后的拒绝且保留原内容、现有仓库 / bare / gitfile / 祖先仓库 / 链接拒绝、预置 `GIT_*`、`GIT_CONFIG_*` 与外部 `init.templateDir` 不生效、unborn `refs/heads/main` 复核，以及本地源 `--no-local` 克隆、已存在空目录与源目标重叠拒绝、非法或未开放协议拒绝、空远端、symlink / gitlink / 外部过滤器 / alternates 源阻断发布、同 ID 变参与同参重放、命令失败留痕与只读诊断。新增回归覆盖：同参重复请求（Init 与 Clone）对协调域与目标做目录树快照并断言完全不变；失败且无残留必须记 `PartialObserved` 而非 `NotStarted`；`Unknown` 结果拒绝重放且拒绝重放本身零写入；Clone 发布后 `.git/index` 存在、`ls-files --stage` 与 `ls-tree -r HEAD` 逐条一致、`git status --porcelain=v2` 为空；不安全提交树在目标不留下任何工作树文件；源 `.git` 内层链接被 `UnsupportedSource` 拒绝且不创建目标；捕获后修改源（新增提交）不影响快照与已发布目标，终态对照捕获时的固定 HEAD。Windows 专用：写入请求 `UnsupportedPlatform` 且目标、`sandbox/`、`records/` 与本机协调域零残留；只读诊断零写入。全部夹具位于系统临时目录内，不访问外网，不留孤儿进程。

副作用夹具在临时根中放置可写 marker 脚本，逐一验证 clean/process/fsmonitor 的本地、include、条件 include、linked worktree 与 worktree include 共 15 种组合：探测后 marker 不存在且仓库字节不变；原生 Git diff（过滤器）或 status（fsmonitor）正向对照必须写出 marker，避免以不存在的命令冒充安全证据。输出夹具复用原生 Rust 测试二进制，以较小限额验证 stdout/stderr 超限后仍存活的进程返回 `OutputLimit` 而非 `Timeout`、精确上限合法、空闲进程超时，均检查直接子进程已被等待回收。

`mise exec -- cargo test -p soxu-git --locked --test project` 使用真实 Git 临时夹具验证项目身份、错误分类与只读边界。打开前后比较目录项、文件字节和修改时间。损坏索引与外部过滤器分别验证身份读取独立于状态查询。Windows 用独占文件句柄复现访问失败，Unix 在文件权限实际受限时断言拒绝。

显式 Init 与本地源 Clone 已由 `create_repository` 交付，HTTPS 克隆、Worktree 池与损坏恢复、direct MCP 上下文挂载、Checkpoint Diff 与 Safety Stash、拆分式提交及预检。Daemon 与协议接线另行协调。
