# Git 只读 Diff：基准、安全边界与最小验收

> #43 / M3d 的设计评审产物，不是已实现的 Diff API。证据基线为主线提交 `7cea1c4` 的 M3a/M3b 源码；本机 Git `2.56.0.windows.4`，核验日期 2026-11-25。与 #42 的 Worktree 清单正交，不修改业务 crate、根依赖、协议、README 或运维看板。

## 1. 范围与证据等级

- **已观测事实**：本文引用的当前源码、随 Git 安装的官方 HTML 手册，以及第 6 节的小型临时仓库实测。Git 命令实测不是 Soxu 新 API 验收。
- **设计建议**：下一项实现的输入、执行限制、输出与失败关闭策略；尚未写入 `soxu-git`。
- **仍待实测**：跨平台真实路径、partial clone、子模块、竞态和大输出等矩阵项。不能由普通仓库的成功推导安全证明。

本次只设计已跟踪文件的未提交 Diff 与显式 `baseRef` Diff。不设计 Init/Clone 写入流程、Worktree 创建/修复、Checkpoint 存储、Safety Stash、回退或提交引擎，不引入 ACP 或新依赖。

### 当前入口与职责

| 源码 | 当前事实 | 对后续 Diff 的含义 |
| :--- | :--- | :--- |
| [`project.rs`](../../crates/soxu-git/src/project.rs) | `open_local_project` 返回 `LocalProject::Directory` 或 `GitWorktree(WorktreeIdentity { root, git_dir, common_dir })`，路径规范化；仅查身份，不扫描 status | 普通目录没有 Git Diff；linked worktree 必须使用自己的根和独立 index，不能切到 common dir 执行 |
| [`probe.rs`](../../crates/soxu-git/src/probe.rs) | `probe` 返回 `RepositorySnapshot`，包含身份、`HeadState::{Attached, Detached, Unborn}` 和状态 | status 是观察结果，不是不可变内容快照；不能把先读 status 后读 patch 当成同一时刻 |
| [`status.rs`](../../crates/soxu-git/src/status.rs) | `StatusSnapshot` 保留头部与 `StatusEntry::{Ordinary, Renamed, Unmerged, Untracked, Ignored}`；XY 分别为 index/worktree 状态 | 状态不能代替 Diff 内容，也不能直接复用 porcelain parser 解析 Diff |
| 同上 | `GitPath` 身份是原始字节；Unix 原样转换，Windows 拒绝不支持的编码/名字；转换不是访问沙箱 | 身份、展示文本、文件访问路径分开；不对有损显示字符串发起读取 |
| [`lib.rs`](../../crates/soxu-git/src/lib.rs) | `CheckpointSummary` 仍为原有摘要契约 | 摘要不是 Git tree 或快照，不能拿它当已实现的第三基准 |

## 2. 基准必须显式，不用一个“Diff”混指多个结果

令 `H` 为本工作树 HEAD 的 commit tree，`I` 为 index，`W` 为工作树内容，`B` 为显式解析成功的本地 `baseRef` commit tree。方向统一为左侧旧内容 → 右侧新内容。

| 建议模式 | 比较 | 命令尾部（公共安全前缀见第 3 节） | 含义 |
| :--- | :--- | :--- | :--- |
| 未暂存 | `I → W` | `diff <D> -- <paths>` | 只看 index 与磁盘差异 |
| 已暂存 | `H → I` | `diff <D> --cached <headOid> -- <paths>` | 只看下一次提交中的差异 |
| 未提交净变化 | `H → W` | `diff <D> <headOid> -- <paths>` | 净结果，不等于前两份 patch 的拼接；中间暂存后又恢复的内容可能抵消 |
| baseRef 至当前内容 | `B → W` | `diff <D> <baseOid> -- <paths>` | 包含相对 B 的已提交与未提交净变化；UI 必须标明“含工作树” |
| baseRef 至 HEAD | `B → H` | `diff <D> <baseOid> <headOid> -- <paths>` | 只含两个提交端点，不含 index/worktree 修改 |

建议第一项实现仅开放前三个模式与 `B → W`；`B → H` 仅用于澄清语义和对照，后续是否开放另定。`B → I`、merge-base 和 Checkpoint 不塞入首项实现。

- **unborn**：没有 H，不伪造零 OID，也不硬编码 SHA-1 空树。Git 官方支持不传 commit 的 `diff --cached`，此时显示所有已暂存内容。首版允许 `I → W` 与这种已暂存视图；`H → W` 返回明确的无 HEAD 错误。显式 B 若存在，可运行 `B → W`，不依赖 HEAD；不存在就失败。
- **detached**：使用解析后的 HEAD OID，不要求分支名，也不切换分支。
- **linked**：`current_dir = identity.root`；index 属于 `git_dir`，对象和部分 refs 属于 `common_dir`。common dir 合法地在工作树外，但不是随意文件读取授权。
- **baseRef 不是范围**：`git diff A B` / `A..B` 比较两端；`A...B` 比较 merge-base(A,B) → B。首版拒绝 `..`、`...`、reflog 和任意 revision 表达式，不自动选择 merge-base，不自动 fetch “更新”远端跟踪引用。
- **冲突**：`Unmerged` 不是普通两端 patch；默认 combined Diff 的列数和阶段语义不同。首版发现相关 index 冲突即返回不支持，并保留状态用于展示，不把 combined 输出误解析为普通 hunk。
- **intent-to-add**：`git add -N` 是 index 条目，不是普通 untracked。Git 的 `--ita-invisible-in-index` 改变展示且为实验选项。首版须先通过专门测试确定策略，否则拒绝此类请求，不由 status 的 A 字符推导内容已经暂存。

## 3. 可执行命令与参数隔离

以下 PowerShell 仅表达独立 argv；实际库使用 `Command::new(可信原生 Git).args(...)`，绝不调用 shell 或接受任意 Git 选项。只允许在自建临时仓库复跑示例；不是让读者针对用户仓库执行初始化或写入。

```powershell
$gitExe = (Get-Command git.exe).Source
$root = $fixtureRepositoryRoot
$nullDevice = if ($IsWindows) { 'NUL' } else { '/dev/null' }
$prefix = @(
  '--no-pager', '--literal-pathspecs', '--no-optional-locks', '--no-replace-objects',
  '-c', 'core.fsmonitor=false', '-c', 'core.untrackedCache=false',
  '-c', 'core.hooksPath=', '-c', 'maintenance.auto=false', '-c', 'gc.auto=0',
  '-c', 'status.submoduleSummary=false', '-c', 'core.quotePath=false',
  '-c', 'diff.autoRefreshIndex=false', '-c', "core.attributesFile=$nullDevice"
)
$diffOptions = @(
  '--no-ext-diff', '--no-textconv', '--ignore-submodules=all', '--no-color',
  '--no-renames', '--no-relative', '--default-prefix', '--diff-algorithm=myers',
  '--no-indent-heuristic', '--unified=3', '--full-index'
)
$paths = @('tracked.txt')
& $gitExe -C $root @prefix config --includes --null --name-only --get-regexp '^filter\..*\.(clean|process)$'
& $gitExe -C $root @prefix ls-files --stage -z
& $gitExe -C $root @prefix rev-parse --verify --end-of-options 'HEAD^{commit}'
& $gitExe -C $root @prefix rev-parse --verify --end-of-options "${validatedBaseRef}^{commit}"
& $gitExe -C $root @prefix diff @diffOptions -- $paths
& $gitExe -C $root @prefix diff @diffOptions --cached $headOid -- $paths
& $gitExe -C $root @prefix diff @diffOptions $headOid -- $paths
& $gitExe -C $root @prefix diff @diffOptions $baseOid -- $paths
```

此代码块是命令形态清单，**不是可一路忽略退出码执行的安全脚本**。后续 runner 必须清除子进程继承的全部 `GIT_*`，再设置 `GIT_CONFIG_NOSYSTEM=1`、`GIT_CONFIG_GLOBAL=<null>`、`GIT_OPTIONAL_LOCKS=0`、`GIT_NO_LAZY_FETCH=1`、`GIT_TERMINAL_PROMPT=0`、`LC_ALL=C`；另外建议 `GIT_ATTR_NOSYSTEM=1`，并屏蔽默认用户 attributes。不得在宿主进程中批量改环境或配置。实测使用上述核心禁用项；补充的输出固定选项与 attributes 隔离仍须纳入后续自动化测试。

### 执行顺序与失败关闭

1. 从显式绝对目录取得当前 `WorktreeIdentity`，确认是本次选择的工作树，普通目录/bare/损坏/元数据目录失败。
2. 查询生效配置与 index。过滤器查询退出 1 且无 stderr 才是“未配置”；匹配到任何 clean/process 键、查询失败或畸形输出即停止。与现有 `probe` 一样保守拒绝，即使当前 paths 看似没使用该过滤器。解析 `ls-files --stage -z`，遇到 mode `160000` 或首版不支持的冲突阶段即停止。不能只调用 `open_local_project` 便声称完成这些预检。
3. 验证请求模式、路径白名单与 baseRef；解析所需提交到完整 OID，再用 OID 发出 Diff。解析失败不回退到 HEAD 或任意目录文件比较。
4. 元数据和 patch 均走受限 runner，检查输出/退出码，最后复核观察身份。发现漂移返回需要刷新，不发起 stash/reset/clean 等“修复”。

### ref 边界

建议接受 `HEAD`、完整对象格式对应的十六进制 commit OID，或限定于 `refs/heads/`、`refs/tags/`、`refs/remotes/` 的完整 ref 名。ref 名先执行不带 `--branch` 的 `check-ref-format <name>` 校验，再用 `rev-parse --verify --end-of-options <name>^{commit}` peel 到 commit。不接受短名歧义、控制字符、前导 `-`、空值、`^{...}` 用户表达式、`@{...}`、冒号对象路径、范围或伪 ref 文件名。

`--end-of-options` 防止 ref 成为额外选项，但不代替词法白名单；成功输出须为一行完整 40/64 位 OID，格式和仓库对象格式一致。禁 lazy fetch 后缺失对象明确失败，不联网补对象。记录 `requestedBaseRef` 与 `resolvedBaseOid`；ref 后续移动不悄悄改变本次端点。

### path 边界

- 路径来自调用方明确授权的**精确文件集合**；只为用户选择的路径请求正文。全仓清单也是一次显式授权，不以“查询未提交”暗示有权读取任意文件内容。
- 使用原始 `GitPath` 字节验证：非空、无 NUL、非绝对/UNC/盘符路径、无空分量和 `.`/`..`；拒绝 Windows ADS、设备名、尾点/尾空格等归一化风险。`to_path_buf` 的已有规则可复用，但本身不足以建立文件访问边界。
- 参数置于独立 `--` 后，同时启用 `--literal-pathspecs`。只有 `--` 不能阻止 `:(glob)`、`:(exclude)`、`*` 的 pathspec 扩展。没有选择路径时返回空集合，不省略 path 参数而意外扩为全仓。
- **空集合在启动 Git Diff 前直接返回空结果**，绝不运行 `diff --`：即使传了 `--`，没有 paths 仍会比较全仓。
- `--literal-pathspecs` 只禁 magic/glob，**不保证目录 prefix 匹配变成精确文件集合**。拒绝目录 pathspec 和未经授权的前缀匹配；目录选择必须另行展开为有界精确文件清单，内嵌仓库的 `? inner/` 不递归展开。
- 白名单资格是“所选比较任一相关端点中已跟踪的路径”，不能只取当前 index。`B → W` 的候选元数据须包含 B tree 与当前 index 的路径并集；H 模式同理包含 H。B 中存在、index 中已删除的路径仍是 tracked-in-base，应显示删除项，而非当作 untracked 或被漏掉。通过有界 `ls-tree -r -z --full-tree <resolvedOid>` 与 `ls-files --stage -z` 获取字节路径/模式/OID 元数据，再与调用方授权精确集合相交；不为候选并集预读正文。树中是文件而当前变为目录的 prefix 也不能直接交给 pathspec 扩展，首版明确拒绝此类型变化。
- 执行前检查路径所有祖先与叶节点，拒绝 symlink、Windows reparse/junction 和类型变化；删除文件按 index/tree 身份验证，不能因文件不存在就取消边界校验。返回的元数据再次核对精确集合，越界记录失败，而不是只隐藏它们。
- literal pathspec 不等于文件系统沙箱：仍须控制 Git 读取工作树文件的链接、配置与 attributes。命令前后检查不能消除 TOCTOU；首版要求受信任仓库及宿主串行化，并诚实报告非原子结果。

## 4. 逐项安全保证：当前 runner 与未来契约分开

| 边界 | 当前源码实际保证 | 后续 Diff 需要 / 尚未证明 |
| :--- | :--- | :--- |
| 原生入口与 shell | 绝对入口，Windows 拒绝非 EXE shim；独立 cwd，直接 argv | `discover` 不证明签名可信；固定 operation 与模式，不暴露任意 args；当前无公开 Diff 入口 |
| 系统/全局配置 | 清除继承 `GIT_*`，禁系统并指向空全局配置；保留仓库配置 | `config --includes` 可读取仓库 include 指向的外部文件，**不是配置读取范围沙箱**；需约定允许的本地/worktree/include/attributes 范围，范围外失败关闭。现有 runner 不能保证从不读取清单外元数据 |
| external diff | 当前只执行身份/status，没有 Diff 专用禁止选项 | 固定 `--no-ext-diff`，清除 `GIT_EXTERNAL_DIFF`；不能让 repo `diff.external` / `diff.<driver>.command` 启动程序 |
| textconv | 没有 Diff 专用禁用项 | 固定 `--no-textconv`，防默认开启转换和 `cachetextconv` 写缓存；不可把转换后的 patch 称为原始可应用 patch |
| clean/process | `probe` 先查生效 clean/process 键并拒绝，包括 include 和已启用 worktree config；身份入口不做此预检 | **禁 ext-diff/textconv 仍可能运行 clean/process**。必须复用同等拒绝策略；不能假设只读 diff 不需要过滤器检查；配置变化竞态未隔离 |
| optional locks / index 刷新 | `--no-optional-locks` 与 `GIT_OPTIONAL_LOCKS=0`，禁 untracked cache | 建议额外 `diff.autoRefreshIndex=false`；裸 porcelain diff 默认可刷新 stat cache，必须用实际字节对照验证，不以“无文本差异”为只读证明 |
| fsmonitor / hooks /维护 | 命令覆盖 `core.fsmonitor=false`、空 hooksPath、禁自动 gc/maintenance、禁子模块摘要 | 不是 OS 沙箱，不阻止恶意 Git 自身、被漏掉的外部机制或派生进程；hooksPath 禁用不能代替 textconv/过滤器限制 |
| lazy fetch /联网 | `GIT_NO_LAZY_FETCH=1`、禁交互，继承 Git remote 环境被清除 | 普通仓库不能证明 promisor 缺失对象不触发 remote helper；需带可执行 marker 的缺对象正向对照。不是全进程网络隔离 |
| 子模块 | `probe` 从 index 检出 gitlink 后拒绝，避免递归 status 执行 | Diff 固定 `--ignore-submodules=all` 且首版仍保守拒绝 index gitlink；检查 B/H tree gitlink 也须纳入实现。不能将被隐藏 gitlink 当作完整支持；不运行 submodule 命令 |
| attributes /对象语义 | 当前保留 repo 语义，清除继承 `GIT_ATTR_SOURCE` 等；未显式禁用户/系统 attributes，也未禁 replace refs | 建议屏蔽外部 attributes 并禁 replace objects，确认本地 attributes 读取范围；EOL/编码/ident 转换会影响 `I → W`，不能称 Git patch 为逐磁盘字节差异。alternates 指向的对象库也需可信范围约定 |
| 限时/限额 | 单命令 15 秒，stdout/stderr **各** 16 MiB；超限/超时/读取失败杀死并等待直接子进程，不返回截断结果 | 复用收集路径，但新增整体请求预算与有界文件数；恶意入口派生进程树不保证回收。50,000 行压力、stderr 超限、取消与内存峰值需实测 |
| 并发与快照 | 没有 OS 沙箱、配置 CAS 或原子内容快照 | 记录根/git_dir/common_dir、解析 OID、index 指纹和观察时间，复核后仍只声明非原子观察；无恢复/写入操作。检测不到的瞬时改变不被包装成强一致保证 |

“不悄悄读清单外路径”针对业务正文授权；Git 的 index、对象库、当前目录祖先配置、attributes 和合法 linked 元数据是必要读取面，必须单独定义。现有实现允许 include 指向外部位置，且原生 Git 可能读取外部 attributes/alternates，**当前没有证明严格读取范围隔离**。首版不能只用 path 白名单宣称面对恶意仓库安全；若不能可靠限制必要元数据来源，应拒绝该仓库或将接口限定于显式信任的配置范围，而不是自行复制用户仓库、配置或凭据。

## 5. 输出与内容授权

### 已跟踪内容

建议先有界输出 `--raw -z --no-abbrev` 元数据，再为精确选择的单文件生成 patch。列表和正文使用同一端点/选项；两个命令之间仍可能发生变化，必须带观察身份，不缓存为不可变快照。

- 不用 `core.quotePath=false` 或按换行 split 还原路径；raw/name-status 的 NUL 记录才是身份来源。`-z` 不会把 patch hunk 内的文件名和内容变成 NUL 协议。**patch 文本不能作为单文件路径身份解析源**，即使只请求一个文件，也须绑定已验证的字节路径和 NUL 元数据；文本头仅用于审阅显示。
- 首版固定 `--no-renames`，以 D/A 表示，不做复制检测；避免全仓相似度搜索为了一个选中文件读取其它候选正文。`StatusEntry::Renamed` 仍可展示状态，不能强迫 Diff 采用同一分类。
- 未来若开放重命名：明确相似度阈值和 `-l` 上限，在授权候选集合内检测；源/目标均须授权。`--name-status -z` 的 rename 是状态、旧路径、新路径三个 NUL 字段，与 porcelain v2 的目标后接源路径布局不同。
- 二进制默认**仅返回元数据**：类型/模式/OID 和“二进制不同”，不返回内容 patch，不启用 `--text`、textconv 或 `--binary` 大块内联，不加载完整内容。`--numstat -z` 的 `-` 表示二进制，不是零行；其 rename 布局需独立 parser 测试。
- 非 UTF-8 身份保留字节；UI 转义显示，不从有损文本反查文件。Windows 无法表示时明确拒绝正文读取，Unix 真实路径验收另跑。内容也可能非 UTF-8，不能因名字有效就按文本强制解码。
- 新增/删除、mode/type/symlink 变化须结构化表达；Git symlink 内容是链接文本，不授权跟随到目标读取。首版拒绝工作树链接和不支持的类型。
- patch 是审阅结果，可能经过 Git 内建 EOL/编码转换，**不是可回退快照，也不授权应用**。不得调用 apply、checkout、reset、clean、stash、repair 或 prune。

### 未跟踪与忽略内容

`Untracked` 单独保留为名称清单，不自动加入 patch；`Ignored` 默认不枚举、不读取。不执行 `git add -N`、不写 index、不用自动 `--no-index` 将未跟踪文件与空文件比较。`--no-index` 能比较工作树外路径，且差异退出码为 1，不属于首版允许的命令族。

未来确需未跟踪预览时，另设显式单文件读取请求：核对最新清单和根内精确路径，拒绝目录/内嵌仓库/链接/设备/reparse，限制读取字节并显示截断状态；此类内容标记为“未跟踪预览”，不冒充已跟踪 patch。首版不实现该入口。

### 大型输出与错误

建议继续沿用 16 MiB 每流 / 15 秒单命令硬失败语义，并增加文件数与请求累计预算。超限返回结构化限制错误，不把未完整收集的 patch 或元数据当成成功。用户可缩小明确选择重新请求；不以扩大授权或扫描其它目录补齐结果。patch 单文件分页、50,000 行渲染是后续 UI/性能任务。

不传 `--exit-code` / `--quiet` 时，正常有差异与无差异均可退出 0；若未来采用这些选项，必须专门处理 1 的含义，不能沿用当前 `run` 的所有非零皆错误规则。保留原始 stderr 供受控诊断，但公开摘要不泄露路径、内容或配置；不对错误自动重试联网或写入修复。

## 6. 本次真实 Git 夹具证据

全部实验只创建小型临时仓库，没有复制当前工程、参考仓或用户配置。夹具写入仅用于准备 index/commits、linked worktree、危险 marker 脚本及正向对照。每次受保护查询之前与之后递归快照**整个临时根**：目录列表、普通文件长度/SHA-256/mtime、链接目标，包括 index、对象、refs、配置、脚本和其它夹具；不比较 atime。正向对照刻意写 marker / index，单独记录，不计作只读通过。

实测前缀为第 3 节中已有 runner 禁用项，加 `--no-pager`、`--literal-pathspecs`、`--no-replace-objects`、`diff.autoRefreshIndex=false`；Diff 使用 `--no-ext-diff --no-textconv --ignore-submodules=all --no-color --no-renames`。补充格式/attributes 选项不混记为已验证。

| 实验 | 命令/准备 | 已观测结果 |
| :--- | :--- | :--- |
| 三个基准 | 同一文件 H=`head`、I=`index`、W=`worktree`，分别运行第 2 节前三种命令 | 分别得到 `index → worktree`、`head → index`、`head → worktree`，均退出 0 |
| 二进制与未跟踪 | NUL 二进制修改，另放 `untracked.txt`，运行 H → W | 二进制只提示 different；未跟踪文件不进 patch |
| unborn | 新仓暂存 `new.txt`；`diff --cached`；解析 `HEAD^{commit}` | 已暂存全新增；HEAD 解析退出 128，不能伪造 HEAD |
| linked + detached | 夹具 `worktree add --detach` 后改 linked 文件，读身份与 H → W | 独立 git_dir/shared common_dir，正确读取 linked 内容，无需切换分支 |
| 重命名格式对照 | old → new（含中文/空格），暂存；`--find-renames=50% -l100 --cached --name-status -z`，同时授权两端 | 输出 `R100\0old name.txt\0new 中文 name.txt\0`；只证明该小夹具，不证明首版需启用检测 |
| ref 选项隔离 | `rev-parse --verify --end-of-options --output=marker^{commit}` | 退出 128，无 marker；业务输入还应在执行前拒绝 |
| literal paths | diff 的 `--` 后传 `--output=marker` 与 `:(glob)**`，全局 literal 开启 | 均退出 0、空输出，无选项注入或 glob 展开；不是路径逃逸沙箱证明 |
| 提交端点对照 | `diff <B> <H>` 且 B=H，index/worktree 有修改 | 空输出；证实不能与 B → W 混称 |
| external diff | repo `diff.external=sh hook.sh`，脚本写 `external.marker` | 受保护 diff 不写 marker；裸 diff 正向对照确实写 `ran` |
| textconv | `.gitattributes` 指定 `diff=fixture`，repo 配置 textconv marker 脚本 | `--no-textconv` 不写 marker；裸 diff 正向对照确实写 `ran` |
| clean | attributes `filter=fixture` + `filter.fixture.clean` marker 脚本 | 生效键查询检出，预检停止，marker 不存在；裸 diff 确实执行脚本 |
| clean 防护误区 | 同一过滤器，仍带 `--no-ext-diff --no-textconv` 执行 diff，作为危险对照 | marker 被改为 `flags-still-run`；两项禁止不能代替 clean/process 预检 |
| fsmonitor | repo `core.fsmonitor=sh hook.sh`；脚本写 marker 并返回协议 token | 强制 false 的 status 不写 marker；裸 status 正向对照确实写 `ran` |
| stat-only 自动刷新 | 已提交文件内容不变，只改 mtime | 受保护 diff 无文件变化；裸 diff 退出 0 但 `.git/index` 字节真实改变 |

共 **18 次受保护 Git 查询**前后快照完全相同（包含过滤器预检、身份/ref 和 Diff），所有危险正向对照均真实触发。没有将原生 Git argv 实验包装成新增 Rust API 测试；未实测项见下节。现有自动化 clean/process/include/worktree 来源与 runner 超时/限额证据可参阅 [`soxu-git/README.md`](../../crates/soxu-git/README.md) 和 [`tests/probe.rs`](../../crates/soxu-git/tests/probe.rs)，不等于新增 Diff 覆盖。

## 7. 最小后续实现 Issue 建议

**建议标题**：`[Task] M3d 实现已跟踪只读 Diff 的四种显式基准`。

**建议 Owned Paths**（须由主控在 #42 收口后分配，本文不取得写权限）：

- 新增 `crates/soxu-git/src/diff.rs`、`crates/soxu-git/tests/diff.rs`；按实际 parser 体量再拆子模块。
- 必要时限量修改 `probe.rs`（共享安全预检/runner）、`lib.rs`（重导出）、crate README；#42 的代码不得并发覆盖。
- 必要 AOCI 托管资产；根 Cargo、协议、其它业务 crate 冻结；看板由主控另派更新。

**最小交付**：crate 内接口，不接 Daemon/RPC/UI；支持 `I → W`、`H → I`（unborn 特例）、`H → W` 与显式 `B → W`；精确授权的已跟踪文件集合、有界 NUL 元数据与按需单文件审阅 patch。明确拒绝冲突、未验证的 intent-to-add、子模块、危险配置和路径；缺对象、漂移、限额均失败关闭。默认不检测重命名、不读取 untracked/ignored，不改任何仓库内容，不添加新依赖。

### 专项验收矩阵

| 维度 | 必须覆盖 | 本次证据 / 后续门禁 |
| :--- | :--- | :--- |
| 基准 | staged-only、unstaged-only、同文件 MM、反向抵消、新增/删除、B 与 H 不同、有/无差异、仅存在 B/H tree 且 index 已删除的路径 | 本次三基准及 B=H 对照；后续自动化验证四模式与跨端点候选并集，不漏 tracked-in-base 删除项 |
| HEAD/身份 | unborn、attached、detached、linked 独立 index、普通目录/bare/损坏失败 | 本次 unborn/linked-detached；现有身份测试；新增端点关联断言 |
| index 特例 | 全部冲突阶段、intent-to-add、split index、sparse index/checkout、SHA-256 | 本次未执行；首版不支持的情形须可确定拒绝，不静默漏文件 |
| 路径/授权 | 空集合不启动 Diff、literal 目录前缀仍匹配的负向对照、两文件只选一、中文/空格/前导 `-`、换行、非 UTF-8、magic/glob/exclude、绝对/UNC/盘符/ADS/`..`、内嵌仓库目录 | 本次中文/空格与 argv 隔离；Windows 拒绝与 Unix 真实字节路径分开跑；还须证明未读取未选正文，不能靠输出隐藏证明 |
| 文件系统 | 叶/祖先 symlink、junction/reparse、删除、mode/type 变化、外部目标、并发替换 | 本次未执行；marker/访问拒绝夹具及 TOCTOU 限制说明 |
| refs | 完整本地 ref/OID/tag peel、短名歧义、选项/范围/reflog/冒号注入、失效或移动 ref、replace refs | 本次 ref 选项失败；其它验证及完整 OID pin 测试待实现 |
| 外部执行 | external/textconv/clean/process/fsmonitor；local/include/includeIf/worktree/worktree include；正向对照与保护查询 | 本次四种真实 marker + clean 误区；process 与来源组合沿用现有证据但新增 Diff 必须覆盖 |
| 读取边界 | 用户/系统 attributes、越界 include、alternates、Git 元数据来源及配置竞态 | 本次未证明严格范围隔离；须限制来源并测试拒绝，不以禁全局 config 替代 |
| 子模块/缺对象 | index 与 B/H tree gitlink、未初始化/脏子模块、子模块自定义程序；promisor 缺 blob/tree/commit、remote helper marker | 本次未执行；保护时 marker 不出现，正向对照真实触发；不能只测正常仓库 |
| 输出/parser | NUL 截断/畸形、多路径 rename、二进制、非 UTF-8 内容、CRLF/编码转换、stdout/stderr 上限与精确边界 | 本次二进制与 R100 格式；新增 parser 与退出码测试，不复用 porcelain parser |
| 资源/取消 | 50,000 行、有界候选数、请求累计预算、超时/取消、直接子进程回收、峰值内存、stat-only 刷新 | 本次 index 刷新正向对照；现有收集路径专项不代替 Diff 压力测试 |
| 零写入 | 每次保护查询整个夹具的目录/文件字节及 mtime不变，无新锁/缓存/对象/marker | 本次 18 次通过；每项危险机制均有独立可写正向对照，不对真实用户仓库做写入试验 |

执行后续 Issue 时先专项 `cargo test -p soxu-git --locked`，再在独立 Worktree target 运行完整 `pwsh -NoProfile -File scripts/check-all.ps1`，维护必要 AOCI 并通过 Verify/Check/Guide。生成的冻结预览若出现差异，报主控裁决，不将其纳入本单。提交推送和 PR 与“合并/归档”分开，执行代理不自行合并或回收。

## 8. 官方行为核验入口

本次直接只读核验随本机 Git 安装的 `share/doc/git-doc` HTML，无需网络拉取、整仓复制或额外参考仓。在线同名入口便于复查，不能用滚动在线版本替代本次版本证据：

- [git-diff](https://git-scm.com/docs/git-diff)：比较端点、unborn cached、`--no-index`、`--no-ext-diff`、`--no-textconv`、`--ignore-submodules`、NUL/raw/numstat、intent-to-add，以及 `diff.autoRefreshIndex`。
- [git](https://git-scm.com/docs/git)：`--literal-pathspecs`、`--no-optional-locks`、`--no-lazy-fetch`、`--no-replace-objects`、Git 环境变量。
- [git-rev-parse](https://git-scm.com/docs/git-rev-parse)：`--verify`、`--end-of-options`、`^{commit}` 的对象类型约束。
- [git-config](https://git-scm.com/docs/git-config) / [gitattributes](https://git-scm.com/docs/gitattributes)：include、fsmonitor、external/textconv、clean/process、用户 attributes 与内建转换。

验收结论：本文给出可执行的 Git 命令形态、真实副作用对照和一个有限的后续实现边界；**只读 Diff 业务能力仍未实现，严格文件读取范围、恶意配置竞态、子模块/partial clone 与跨平台资源安全仍须按矩阵闭合**。
