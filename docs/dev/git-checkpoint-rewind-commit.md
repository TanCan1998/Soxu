# M3：轮次 Checkpoint、安全 Rewind 与结构化提交引擎

> 设计审定稿，对应 #58；不代表引擎已经实现。基线为 `63882d1`。主控于 2026-11-26 审定本文件的首版边界。

## 1. 现状、范围与不可变约束

现有源码证据：

- `crates/soxu-git/src/lib.rs` 只有四字段 `CheckpointSummary { checkpoint_id, turn_index, commit_oid, changed_files }`；本设计不增加、删除或改义这些字段。遗漏、覆盖范围、安全快照、生命周期放入独立记录。
- `probe.rs`、`project.rs` 提供可信 Git 进程和工作树身份；`status.rs` 已解析 porcelain v2 的 Unmerged、普通变更、重命名及原始路径。
- `diff.rs` 已提供 `Unstaged`、`Staged`、`Uncommitted`、`BaseToWorktree` 四基准；后者接受完整 OID，但不接受任意私有 ref 字符串。
- `worktree_lifecycle.rs`、`worktree_checkout.rs` 的身份重核、持久 Intent、阶段状态和失败留痕可复用设计原则；不能把“注册成功”当成已有 Checkpoint/Rewind 授权。
- `soxu-protocol/src/domain.rs` 的 `CheckpointAnchor` 是消息锚点，另有 `diff_stat`；它不是快照目录、授权票据或写入 RPC。现有 core/daemon 消费不能被描述为已经接入新引擎。

首版只承诺：轮次快照、明确覆盖范围的 Diff、HEAD 不动的工作树 Rewind，以及文件级选中集提交。hunk、自动合并冲突、submodule 内部状态恢复、任意过滤器兼容和 provider 对话历史回退另行排期。

### 1.1 安全不变量

1. 快照只写受控对象、私有 index、应用记录和私有 ref，不移动用户分支 HEAD、不替换主 index、不依赖用户常规 reflog。
2. Rewind 之前必须持久化独立 Safety Snapshot：工作树树、原 index 树与必要原始 index/元数据。不能完整保存将被改数据就阻断。
3. Rewind 恢复工作树与恢复原暂存区是两个可分别确认的动作；两者都不移动 HEAD。
4. 提交必须先预览、再授权、再执行；文件全集合不等于用户选中集，工作树状态也不等于暂存状态。
5. 不执行无保护的 `git reset --hard`、`git clean -fd`，不绕过 hooks、签名或隐私验证。失败不得自动改用宽泛路径或强制选项重试。
6. 私有 ref 是命名隔离，不是安全保险箱：对象仍在本地共享对象库，显式推送或镜像备份仍可能携带它们。敏感内容不能因“没有普通分支”而自动获准落盘。

## 2. 身份、计划、权限与资源预算

所有读写沿用现有可信 Git 可执行文件、受限参数、输出/超时上限及环境清理；外部 diff、textconv、fsmonitor、分页器、交互提示、过滤器执行与可选 index 刷新按既有只读边界处理。写入口另行授权，不能复用只读命令白名单。

计划绑定以下观察值，而不是仅绑定目录字符串：

| 绑定项 | 用途 |
| --- | --- |
| canonical worktree root、worktree git-dir、common-dir、工作树注册身份 | 区分 linked worktree、目录替换及跨工作树串线 |
| HEAD 类型、symbolic ref、完整 OID（或明确 unborn） | 阻止分支切换、HEAD 漂移、错误父提交 |
| 主 index 存在性、字节摘要、stage-0 树、受支持扩展/标志 | 保护暂存状态，识别空 index 与不存在的区别 |
| 路径原始字节、类型、文件身份、大小、内容摘要、属性摘要 | 防路径替换、符号链接穿越和预览后编辑 |
| 配置/attributes/ignore/策略版本、选中路径全集、目标 OID | 阻止过滤器或忽略规则漂移改变结果 |
| operation_id、授权范围、计划摘要、期限、幂等键 | 防重放、跨任务误用和重复发布 |

每工作树只允许一个 Soxu 写操作；common-dir 的私有 ref 发布与目录分配另有协调。应用锁不能排斥编辑器、原生 Git 或其他进程，必须在破坏点前重核，并保留部分应用的现实状态。HEAD CAS 也不保护工作树文件。

路径以现有 GitPath 的原始路径能力为基础：NUL 分隔、参数数组、显式 `--` 或受支持的 NUL pathspec 输入，禁 shell 拼接；不以换行拆文件名。拒绝越界、绝对路径、`..`、元数据目录及 Windows ADS、大小写/规范化冲突、重解析点等歧义。路径组件必须逐级 no-follow 核查；只做一次 canonicalize 不足以抵御 TOCTOU。缺少相应平台安全原语时，该平台写能力关闭，不能宣称跨平台等价。

建议的首版预算如下，均为待实现的可配置策略，不是既有常量：

| 项目 | 普通 Checkpoint | Rewind Safety |
| --- | --- | --- |
| 单文件提示 | 1 MiB 起提示 | 同样提示，不构成跳过许可 |
| 文本单文件 | 超过 8 MiB 可遗漏并记录 | 必须保存；超过硬上限则整次阻断 |
| 二进制 | 默认不自动纳入，可授权纳入至 32 MiB | 将被改的二进制不得遗漏 |
| 总量/路径数 | 64 MiB / 10,000，超过标记不完整 | 建议 512 MiB / 10,000、单文件 128 MiB；超过则阻断，不降级 |
| 凭据/隐私命中 | 不自动保存，返回明细 | 无合法本地保护方式或授权时阻断回退 |

枚举、读取、哈希、Git 输出与对象增长均计预算；读取过程中增长仍需截停。禁止仅靠扩展名判定二进制或敏感性，也不能把警告阈值当作“安全证明”。

## 3. Checkpoint 存储与捕获

### 3.1 私有命名与记录

私有 ref 建议为 `refs/soxu/checkpoints/<workspace-id>/<session-id>/<checkpoint-id>`，Safety 使用不同的 `refs/soxu/safety/<workspace-id>/<operation-id>/{worktree,index}`。所有段由受控 ID 生成并经 check-ref-format 校验，不接受用户提供完整 ref。common-dir 共享 ref，必须显式包含工作树身份并核对归属。

私有 index 位于应用拥有、非工作树可枚举的操作目录中，随机独占创建其父目录；index 文件初始应不存在，由 Git 创建。禁止指向主 index、符号链接或共享可写临时文件。linked worktree 的原 index 应经 Git 解析，不能假定是根目录的 `.git/index`。

独立目录记录建议包含：schema_version、工作树/会话/轮次身份、checkpoint_id、父 OID、工作树树 OID、commit_oid、捕获前后观察摘要、覆盖 manifest、遗漏原因、策略版本、创建时间及生命周期状态。摘要四字段保留原样；`changed_files` 定义为快照树相对捕获基树的变更路径数，重命名按明确的统计规则归一，不声称包含所有遗漏变化。

### 3.2 捕获步骤

以下是实现顺序，不是让用户在真实仓库复制执行的命令脚本：

1. 核查仓库身份、HEAD、index、冲突/合并状态及资源预算，取得稳定枚举。
2. 解析并固定父 OID；普通 HEAD 从该树初始化私有 index。unborn 用私有 index 的 `read-tree --empty` 建空树，根快照不带父提交；不伪造零 HEAD，不硬编码 SHA-1 空树。
3. 逐个纳入已批准路径：tracked 的修改/删除，以及尊重 ignore 后被显式策略允许的 untracked。必要时用无过滤器的原始 blob 加 stage-0 entry；不能在未知仓库直接 `add -A .`。
4. `write-tree` 得到树；`commit-tree` 创建内部快照对象，使用明确的内部身份与消息，不篡改用户全局身份。Checkpoint 不是用户提交；若组织政策要求内部对象签名，也必须遵守。
5. 核对捕获前后身份、HEAD、主 index 及路径内容摘要；检测漂移时废弃未发布候选，返回可重试的新计划，不发布撕裂快照。不能把“最终 stat 没变”当作所有文件同一瞬间的证明。
6. 先持久化 Intent，再以旧值 CAS 创建私有 ref，最后原子发布完整目录记录。对象和 ref 均可重新解析且记录持久化后才返回成功。

普通捕获初版对强一致性没有文件系统级全局快照保证；必须阻止本机 agent 同时写入，检测外部漂移，并将无法稳定捕获报告为失败。不得静默宣称任意并发编辑下的时间点快照。

### 3.3 纳入、遗漏与特殊对象

- tracked 删除只在“确认不存在且非临时读取失败”时删除树 entry；拒绝读取、占用、类型竞态不能当作删除。
- untracked 必须来自受控枚举，默认尊重 `.gitignore`、仓库 exclude 与既定忽略策略；敏感目录、嵌套仓库和元数据目录永不递归扫描。普通快照不自动添加 ignored 文件。
- 存在 clean/process filter、LFS 或无法忠实表达的编码转换时，首版不执行该过滤器。受影响路径遗漏或整次阻断；原始内容与 Git 规范 blob 不等价时必须记录表示方式，不能把原始 blob 冒充可正常提交内容。
- tracked symlink/gitlink 可从基树继承，但不跟随目标、不捕获 submodule 内部工作树；发生变化时记录 unsupported。新增 symlink、重解析点、设备/FIFO 等首版不纳入；若计划将覆盖它们，Safety 阶段必须阻断。
- 普通快照允许不完整：遗漏的 tracked 路径保留基树 entry，遗漏的 untracked 不进入树；manifest 明确其不是当前工作树真值。不得根据保留的旧 blob 得出“该路径没有变化”。
- 使用目标树进行 Rewind 时，遗漏路径没有恢复权威；全工作树回退到不完整快照应被拒绝，不能自动恢复为旧基树内容。

### 3.4 持久性、reflog 与保留

私有 ref 更新显式关闭普通 reflog 自动创建，不使用 create-reflog；若命名空间已有意外 reflog，需检查并停止而非悄悄覆盖。应用以自己的追加记录追踪发布和恢复，不依赖 HEAD reflog。

提交对象、工作树/index Safety ref 使用同一 ref transaction 的 CAS 发布；更新前持久化预期旧值、目标 OID 和恢复 Intent。平台必须提供记录及备份所需的 flush/原子替换保证；做不到就不能进入破坏阶段。ref 已提交但目录记录未完成时，按 Intent 重建发布状态，不能把重启后看不到列表当作没有快照。

保留策略分别管理普通轮次与 Safety：普通快照可按用户策略裁剪，Safety 在用户确认可丢弃或明确保留期限到达并授权前持续保留。自动 GC 不得回收仍被恢复记录引用的 ref。删除 ref 不保证底层敏感对象立即擦除，必须如实提示；禁止顺带执行激进 prune 或改全局 GC 配置。

## 4. Checkpoint 基准 Diff 的只读语义

| 比较 | 接入方式 | 约束 |
| --- | --- | --- |
| index → worktree | 现有 Unstaged | 不重新定义 |
| HEAD → index | 现有 Staged | 延续现有 unborn 处理 |
| HEAD → worktree | 现有 Uncommitted | 无 HEAD 则明确报错 |
| 固定基准 → worktree | 现有 BaseToWorktree | 服务端从已授权 Checkpoint 解析完整 commit OID；不放开私有 ref 任意输入 |
| Checkpoint A → B | 新的双端固定 OID 比较契约 | 两端核对工作树/会话归属、对象可达性、覆盖 manifest，不能套 BaseToWorktree |

消费 Checkpoint 时先经目录记录解析 ID，验证私有 ref 与 OID 一致并固定端点；客户端传来的 OID 不能单独构成访问其他会话快照的授权。BaseToWorktree 的工作树一端仍会移动，结果携带观察时间/身份；只有双固定树端点才能称作固定历史比较。

保持现有 metadata/review、路径预算、二进制/删除/重命名表示与 no-ext-diff/no-textconv 边界。不为了 Diff 临时移动 HEAD、主 index 或创建用户分支；只读路径不生成新快照。未跟踪内容的展示沿用现有只读约束，不能把尚未纳入的 untracked 假装成基准树中的文件。

独立结果 envelope 携带两端身份、coverage、omissions、incomplete_reason；存在遗漏时显示“已覆盖路径的差异”，不能标成完整零差异。即使展示读取不足，固定 OID 仍保留供后续按策略读取，不擅自扩大资源预算。

## 5. 安全 Rewind：先保存、后修改

### 5.1 计划与授权

Rewind 不等于 Git 分支回退，也不包含 provider 对话历史回退。首版不修改 HEAD、分支或当前会话消息。预览列出每条路径的新增/替换/删除/保持、目标 blob、原内容摘要、暂存变化影响、冲突障碍和 Safety 预计占用。

明确两种独立授权：

- **恢复工作树**：按目标 Checkpoint 的完整覆盖域恢复文件，原 index 保持不动；界面必须显示将出现的 staged/unstaged 差异。
- **恢复原暂存区**：针对某次已保存 Safety 的 index 树/原始 index 状态单独预览确认；不能从 Checkpoint 的工作树树推测用户原来暂存了什么。两步都选也必须各有明确范围。

重命名必须包含源/目标双方及障碍检查。首版拒绝 Unmerged、进行中的 merge/rebase/cherry-pick/revert、unsupported sparse/split index、intent-to-add、skip-worktree/assume-unchanged 等无法保证恢复的状态；不能仅保存 stage-0 树后声称完整恢复了这些状态。

### 5.2 Safety 的零遗漏定义

Safety 覆盖本次操作声明的恢复域：当前 tracked 内容、允许范围内当前 untracked、原暂存状态，以及所有将被替换/删除的路径和碰撞障碍。ignored 文件即使不属于普通快照，也不能被回退覆盖或清理；若目标路径与其相撞，必须合法完整备份，否则阻断。

至少保存以下不可变产物：

1. 当前工作树的完整保护树及必要的受控旁路备份；
2. 原 index 树对应的独立 commit/ref，以及原 index 字节备份、存在性与受支持元数据；
3. 路径清单、模式/属性、摘要、原 HEAD/工作树身份、目标 OID、拟执行动作和操作日志。

树对象不包含 ACL、ADS、全部时间戳、目录属性或 OS 锁。首版明确不是通用文件系统备份；遇到将受影响而无法忠实保存/恢复的属性、特殊对象或编码，拒绝执行，不能用“Git 已存 blob”掩盖信息丢失。硬链接及非普通文件亦应拒绝。忽略且完全不触碰的域明确不在保护承诺中，不进行目录级清理。

Safety 不允许沿用普通 Checkpoint 的跳过策略：大小、敏感策略、读取失败、磁盘不足、对象校验失败或持久化失败均导致整次回退不启动。两个树 ref 与恢复记录均可验证后才进入 SafetyDurable。Safety 独立保留，成功后也不自动删除。

### 5.3 执行状态机与失败

建议状态：Planned → AwaitingApproval → Revalidating → CapturingSafety → SafetyDurable → Applying → Verifying → Completed；另有 Rejected、Invalidated、FailedBeforeApply、PartialNeedsRecovery。

每个动作再次核对路径组件、文件身份、内容与占用；用同目录受控临时文件准备内容，在平台支持的安全句柄/原子替换边界应用。删除只能逐条命中已授权且已备份的路径。不能把批量 restore 的退出码当作文件级原子事务。

- 有未保存编辑、外部写入或被独占锁占用时，暂停/失败并保留 Safety；不自动关闭编辑器、杀进程、抢锁或重试强制写。
- 路径在核查后变为 symlink/reparse point 或指向其他卷时停止。无法以平台原语封闭这类竞态时禁止写入，而非继续跟随。
- 部分失败记录已完成、未完成、结果未知的每条动作。超时或取消可能已经产生写入，必须检查现场，不能直接返回“已取消且未修改”。
- 不自动执行破坏性逆操作：回退到 Safety 同样需要新的预览、身份重核和授权；当前又有新修改时还需新的保护。
- 工作树完成而 index 恢复失败时，按两个动作分别报告，保留两个原始产物，不伪称整个操作已原子回滚。

重启只进行只读核验并展示恢复入口。锁文件存在不能证明持锁者已退出；未经明确裁决不删除原生 Git 的 index.lock 或其他进程文件。

## 6. 结构化提交：预检、隔离构树与发布

### 6.1 预检与不可变提交计划

初版只支持可核验的已存在 HEAD、当前 symbolic branch、stage-0 普通 index 和文件级选择。detached/unborn 提交、合并提交、冲突解决、hunk 选择另设能力，不隐式建分支或选父提交。

只读预检输出：父 OID、目标 ref、选中完整路径集合、内容来源、增删改/重命名影响、大小/二进制/敏感风险、已暂存但未选中内容、将替代的选中文件旧暂存内容、hooks/签名要求及主 index 同步计划。明确提示“选中完整文件”将提交该文件当前批准版本，不仅是其已暂存部分。

凭据、私钥、本机配置、个人路径和超预算内容默认阻断或进入独立策略裁决；警告批准不授予绕过组织硬性规则的权限。任何 Unmerged 或进行中的合并/变基状态都拦截。内容、HEAD、index、分支或策略漂移均使批准失效。

### 6.2 私有选中树

从固定父树 H 初始化私有 index，只加入授权文件最终版本，删除也仅限授权路径。没有选中的 staged 内容不得因为原 index 已暂存而混入新提交。

若 H 为父树、S 为文件全集合，则新树 C 在 S 外必须逐 entry 等于 H；S 内等于已批准内容。重命名按源删/目标增成对确认。用树级验证再次证明该不变量，而不是只检查命令返回码。

实际主 index 的后继 I' 另行从原 I 复制构造：S 内更新为 C 的 entry，S 外保留 I 的原始暂存内容与受支持标志。原 I 的已暂存选中内容可与工作树不同，必须在预览中说明并持久备份；不能未经说明丢弃。

### 6.3 hooks、签名与验证不能被 plumbing 绕过

`commit-tree` 不会自行执行常规 commit 的 pre-commit、prepare-commit-msg、commit-msg、post-commit 流程；提前跑一次全仓门禁也不能证明这次提交满足相同约束。

首版能力探测必须识别 core.hooksPath、适用 hooks、签名配置及组织验证策略：

- 未发现适用 hooks 也必须完成受控隐私/提交信息检查；发现 hooks 而没有获审定的适配器时失败关闭，不能设置空 hooksPath、使用 no-verify 或换工具绕过。
- 受控适配器需保持适用的顺序、退出语义、提交消息来源和私有 index 环境；运行 pre-commit → prepare-commit-msg → commit-msg 后重新验证内容/消息。hook 改动计划内容时使原授权失效，不能偷偷接受。任意 hook 的副作用不是私有 index 能隔离的，信任/执行权限必须单独成立。
- 按策略通过受控 signer 生成 commit-tree 的签名对象，并在 CAS 前验证签名与消息、父/树 OID；签名器不可用或拒绝时停止，不自动降级为无签名。不更改全局签名配置。
- 适用的 reference-transaction hook 仍由 update-ref 执行，拒绝时不能强制跳过。post-commit 等成功通知在发布及 index 同步完成后处理，其失败不得反向声称已提交对象不存在。
- 若只能通过常规 porcelain commit 保持既有约束，必须另行设计和授权其 HEAD/index 副作用；不得悄悄作为本方案的失败回退。

### 6.4 CAS 与主 index 的非原子窗口

建议可审计流程：

1. 保存原 I、工作树观察和发布 Intent；准备不可变 C、目标 commit H' 与 I'，完成全部验证和签名。
2. 独占获取原生 index.lock，再验证原 index、HEAD symbolic 归属、旧 OID H、工作树和审批摘要。没有锁时不发布；锁不保护所有外部 ref 操作。
3. 持久化 PublishReady，使用 `update-ref <branch> H' H` 做 CAS；真实分支更新遵守正常 reflog/验证策略，与 Checkpoint 私有 ref 的无普通 reflog策略区分。
4. CAS 成功后持久化 RefPublished，安装已核验 I'，记录 IndexInstalled，并再次核验 ref、工作树和 index。S 外暂存内容及所有工作树字节不得改变。
5. 仅上述状态均确认后报告 Completed。最终状态未知时返回 operation_id 供核验，不让客户端重复创建提交。

第 3、4 步不是一个 Git 原子事务，即使提前获取 index.lock 仍可能崩溃。不能把“HEAD CAS 成功”直接当作整个提交成功，也不能在 ref 成功后拿新 HEAD 重新生成未经批准的 I'。

| 重启观察 | 恢复原则 |
| --- | --- |
| ref=H、index=I | 尚未发布或 CAS 未成功；保留候选对象，重新预检授权 |
| ref=H'、index=I | 已提交但 index 待同步；展示差异，在身份/摘要一致且显式确认后安装既定 I' |
| ref=H'、index=I' | 核验后补记完成，绝不重复提交 |
| ref 或 index 不等于任一期望值 | 外部变更/未知状态；停止自动恢复，保留 I、I'、H' 和日志供人工协调 |

禁止盲目把 ref CAS 回 H：外部可能已在 H' 上继续提交。禁止“恢复旧 index”覆盖用户后续暂存。主 index 安装失败不修改工作树，也不删除已发布提交；实际发布状态必须对用户可见。

## 7. 公共契约与实施拆分

下列类型名称是建议，不是当前可调用 API。库接口按职责分组、使用具体计划/结果类型、明确错误上下文；不泄漏锁/智能指针实现，也不为首版创建覆盖全部引擎的巨型 trait。

| 顺序 | 范围与 Owned Paths 建议 | 交付与边界 |
| --- | --- | --- |
| A：内部快照基础 | `crates/soxu-git/src/checkpoint/**`、必要的 `lib.rs` 重导出、该 crate 专项测试 | 私有 index/ref、目录记录、覆盖 manifest；保留四字段摘要；不改 RPC |
| B：独立公共契约 PR | `crates/soxu-protocol/src/{domain,rpc,events}.rs`、必要重导出与契约测试 | CheckpointBaseline/双端比较、Rewind/Commit 预检与授权请求、阶段/恢复结果事件；先合入再允许消费者依赖 |
| C：固定端点 Diff | `crates/soxu-git/src/diff/**`、必要 `diff.rs/lib.rs` 与专项测试 | 保持现有四基准，新增独立双端比较；不向现有入口开放任意 ref |
| D：安全 Rewind | `crates/soxu-git/src/rewind/**`、必要重导出与专项测试 | 双树 Safety、逐路径应用、独立 index 恢复与恢复日志；依赖 A 的稳定内部契约 |
| E：结构化提交 | `crates/soxu-git/src/commit/**`、必要重导出与专项测试 | 文件级计划、hooks/签名策略、CAS 与 index 协调，不承诺 hunk |
| F：编排与传输集成 | 独立分配 `crates/soxu-core/src/**`、`crates/soxu-daemon/src/**` 及测试 | B 合入后接授权、持久状态、幂等与事件；UI 路径另派设计任务 |

A/C/D/E 若并行，`lib.rs` 与现有 `diff.rs` 由一个集成人持有；其他工作者不得同时写同一文件。根 Cargo/lock 如确需变化先走协调或独立依赖 PR，不能借白名单扩大写入。

公共契约至少包含：工作树/会话归属、opaque checkpoint/operation ID、固定端点与 coverage、计划摘要与授权期限、阶段、稳定错误码、已发布/已应用范围、Safety 恢复句柄及幂等重查入口。错误需区分 Unsupported、Conflict、IdentityChanged、PolicyDenied、SafetyIncomplete、PartialNeedsRecovery、RefPublishedIndexPending；事件不携带凭据、原始文件内容或个人路径。

## 8. 专项测试矩阵

未来实现必须在临时夹具与故障注入环境覆盖；下表不是宣称现有测试已具备：

| 测试域 | 必测情况及断言 |
| --- | --- |
| 快照隔离 | staged/unstaged 同文件不同内容；主 index 字节、HEAD、工作树、普通 reflog 不变 |
| 覆盖边界 | untracked/ignored、超量文本/二进制、敏感路径、读取拒绝；普通遗漏明确，Safety 一项遗漏即阻断 |
| 仓库形态 | linked worktree、unborn、detached、SHA-1/SHA-256、bare 拒绝、目录身份替换 |
| 特殊路径 | NUL 清单、换行/非 UTF-8、前导连字符、大小写冲突、Windows ADS、symlink/reparse/hardlink、gitlink |
| Git 配置 | clean/process/LFS/textconv/ext-diff、fsmonitor、签名器、hooksPath；无未授权进程执行或环境注入 |
| 快照一致性 | 文件增长、捕获前后 HEAD/index/ignore 漂移、同 stat 内容替换、外部并发写；不发布撕裂结果 |
| Diff | 四基准回归、Checkpoint→W、A→B 固定端点、错误归属、过期 ref、遗漏提示及只读字节对照 |
| Safety | 同时恢复 W 与独立 I、原 index 不存在、碰撞 ignored、特殊 index 拒绝、磁盘满/flush 失败时零破坏 |
| Rewind | 占用文件、越界链接、部分替换/删除失败、取消/超时、重启、二次恢复前又有新修改；保留现场与 Safety |
| 提交选中集 | 未选中 staged/unstaged/untracked 不混入，重命名双端确认，旧选中暂存内容有备份，S 外 index 不变 |
| 发布窗口 | CAS 拒绝、ref 已发布但 index 未安装、外部 ref 前进、index 漂移、stale lock、重复请求；不盲回滚、不重复提交 |
| hooks/签名 | pre-commit/commit-msg/reference-transaction 拒绝、hook 改内容、签名失败/验签失败、post-commit 失败；不静默降级 |
| 资源与隐私 | 路径数/输出/时间/磁盘预算、日志脱敏、ref 镜像风险、Safety 保留与受控删除 |

### 8.1 本设计实际完成的良性夹具验证

2026-11-26，使用本机 Git 2.56.0.windows.4，在新建临时目录完成 **15 项断言**。全程不修改 Soxu 或参考仓库，不读取用户全局 Git 配置，不执行清理/强制回退命令；夹具保留而未自动删除。

- 私有 index 捕获后 HEAD、主 index 字节、工作树字节和 porcelain 状态均保持不变。
- 受控 untracked 被纳入，ignored 未纳入；关闭自动创建后私有 ref 没有普通 reflog。
- 完整 Checkpoint OID 到工作树的指定 tracked 路径 Diff 为空且不改主 index。
- 独立工作树/index Safety commit 保留了同文件不同的 worktree 与 staged 内容。
- 错误旧值的 ref CAS 被拒绝，原 ref 不变。
- 选中文件树不含未选中的 staged 内容或 untracked；HEAD CAS 不改工作树/主 index。
- CAS 后尚未同步主 index 的差异窗口可观察；按选中路径同步后，未选中原暂存内容和工作树内容保持不变。
- 失败 pre-commit hook 不被 commit-tree 调用，而常规 commit 会调用并拒绝；证明两者不能被视为等价验证路径。
- unborn SHA-256 仓库通过动态空树与无父 commit-tree 创建内部快照，HEAD 仍 unborn、主 index 仍不存在。

这些实验仅验证底层原语，不证明安全路径句柄、真实崩溃持久性、跨平台锁语义、生产 hooks 适配、签名器、凭据识别或完整恢复算法已实现。上述测试矩阵仍是后续 PR 的验收要求。

## 9. 参考来源、采用点与限制

参考根以本工作树 `.agents/local/reference-context.json` 的既有资产绑定；实现文档只记录仓库相对路径，不固化个人目录。参考资产从未被初始化、重建或维护。

| 来源与固定版本 | 定位 | 实际采用点 | 不采用/限制 |
| --- | --- | --- | --- |
| Soxu `63882d1` | `soxu-git/src/{lib,probe,project,status,diff,worktree_lifecycle,worktree_checkout}.rs`；`docs/dev/{git-readonly-diff,git-worktree-lifecycle,architecture}.md` | 保留摘要、身份绑定、四基准语义和失败留痕 | 新写入能力不能冒充现有实现 |
| Padu `fbe97a3` | `crates/padu-core/src/checkpoint.rs`、`daemon.rs` 的 rewind safety 流程；`git_commit.rs`；`crates/padu-protocol/src/checkpoint.rs` | 私有 index 捕获、轮次 ref、先 Safety 后 Rewind | 不移植 restore_ref 的 clean/reset 路径、成功后删除 safety、硬编码 SHA-1 空树或无完整边界的 add -A |
| Paseo `7bcd598` | `packages/server/src/server/diff/git.ts`、`utils/checkout-git.ts`、`server/agent/rewind/rewind.ts`、`plugin-provider.ts` | 私有 index 端点、固定 revision 解析；区分文件与对话/provider Rewind | 不把 provider 能力分派当作 Git Safety；图存在 18 个解析错误，ready=false，定位结果已用源码核对 |
| Codeg `028d30e` | `src-tauri/src/work_task/git.rs`、`commands/worktree_convergence.rs` | 提交前文件清单、私有 index 进程入口与任务交付边界 | commit_all 的全量 add 不满足选中集隔离；不能自动复用其重置/合并策略 |

只读盘点确认三参考声明的 AOCI 资产摘要一致；CodeGraph 均通过显式参考根查询，Codeg/Padu 当时无待处理变更和解析错误。摘要一致或图可查询不等于 AOCI 新鲜度、Whole-Index 认知或端到端安全证明。目标 Soxu 的认知维护与参考项目只读调查分开处理。

## 10. 交付与后续验收

本 PR 只交付本设计与受控 AOCI 维护，不新增 Rust 实现、RPC 或 UI。后续每个实现 PR 均须明确 Owned Paths、契约依赖、临时夹具、故障注入证据和平台能力限制。

文档交付门禁：项目锁定 mise 环境、独立 CARGO_TARGET_DIR、UTF-8 的子 shell 内运行 `pwsh -NoProfile -File scripts/check-all.ps1`；提交使用既有 hooks，不跳过验证。AOCI 的 Overview 交付状态与 Verify/Check/Guide 治理结果分别报告，不能互相代替。
