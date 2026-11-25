# soxu-git：本地项目身份与 Git 只读探测

M3d 提供已跟踪文件的四种只读 Diff。M3c 提供 Git Worktree 只读注册清单。M3b 提供打开本地目录的项目身份。M3a 提供 Git 工作树的 HEAD 与状态快照。原有 `CheckpointSummary` 字段及 serde 契约保持不变。

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

## 只读与限制

所有子进程直接传参数并设置独立 `current_dir`，不启动 shell，不改变全局 cwd 或环境。子进程清除继承的 `GIT_*`，忽略系统/全局 Git 配置，保留仓库自身的语义配置；禁用 optional locks、fsmonitor、untracked cache、自动维护、子模块摘要和按需 fetch，关闭交互。单命令超时 15 秒，stdout/stderr 各上限 16 MiB；任一流读到上限加一字节立即返回 `OutputLimit`，不等待 EOF 或自然退出而退化成 `Timeout`。超限、超时或读取失败会终止并等待回收直接子进程，不承诺回收恶意入口派生的整个进程树。

`open_local_project` 只运行 `rev-parse`。只有 Git 明确报告没有仓库，并且沿途没有发现疑似被跳过的 Git 元数据，才返回普通目录。现存的 `.git` 文件、目录或链接，以及 `objects`、`refs` 与 `HEAD`、`config` 形成的独立 Git 目录特征，或成对出现的 `commondir` 和 `gitdir` 文件，都按疑似损坏保守拒绝。若 Git 跳过损坏的内层 `.git` 并找到外层工作树，入口也会拒绝打开。损坏判定限于发现身份所需的元数据。索引、对象库和引用完整性不在此检查范围内。已观察到的访问失败和文件系统边界等无法确定归属的诊断直接返回错误。

`status` 可能经 clean/process 过滤器或子模块配置执行外部程序，因此遇到仓库外部内容过滤器或索引 gitlink 时提前失败关闭。未初始化子模块也明确不支持；纯解析器仍完整支持子模块标记。过滤器查询显式使用 `config --includes`，覆盖本地配置、已启用 `extensions.worktreeConfig` 的 linked worktree 配置及其 include/生效的 includeIf；系统/用户配置不参与。配置查询失败会拒绝探测，不自动降级为忽略配置继续运行。探测没有 OS 级沙箱，不承诺面对恶意 Git 可执行文件、配置读取与启动之间的篡改竞态或并发 Git 变更时提供原子快照；调用方应串行化更改，并信任所绑定原生入口。

`probe` / `open_local_project` 暂不支持 bare 仓库和 `.git` 元数据目录；`list_worktrees` 可报告 bare 主注册条目。忽略文件只在纯解析器输入中支持，探测默认不枚举。无新第三方依赖，临时夹具使用标准库，构建缓存与 AOCI 运行资产沿用已有忽略规则。

## 验证与后续

`mise exec -- cargo test -p soxu-git --locked` 覆盖 unborn/attached/detached、子目录、linked worktree、暂存/未暂存、Unicode/空格、冲突、目录错误、fsmonitor 与过滤器拒绝，以及解析器的换行、非 UTF-8、复制、七类冲突、SHA-256 和非法输入。临时内嵌 Git 工作树实测 `--untracked-files=all` 仍返回带单个 `/` 后缀的 `?` 目录记录；忽略枚举返回对应 `!` 目录记录。仅这两类允许单个目录后缀并保留原始字节，跟踪、重命名源/目标和冲突路径仍拒绝目录后缀；所有类型均拒绝内部空分量、双斜线、绝对路径及 `.`/`..`。探测前后比较整个临时仓库的文件列表与字节，确保索引和对象未变。Unix 专项真实路径用例只在 Unix 执行；Windows 上执行纯字节解析与拒绝转换用例。

副作用夹具在临时根中放置可写 marker 脚本，逐一验证 clean/process/fsmonitor 的本地、include、条件 include、linked worktree 与 worktree include 共 15 种组合：探测后 marker 不存在且仓库字节不变；原生 Git diff（过滤器）或 status（fsmonitor）正向对照必须写出 marker，避免以不存在的命令冒充安全证据。输出夹具复用原生 Rust 测试二进制，以较小限额验证 stdout/stderr 超限后仍存活的进程返回 `OutputLimit` 而非 `Timeout`、精确上限合法、空闲进程超时，均检查直接子进程已被等待回收。

`mise exec -- cargo test -p soxu-git --locked --test project` 使用真实 Git 临时夹具验证项目身份、错误分类与只读边界。打开前后比较目录项、文件字节和修改时间。损坏索引与外部过滤器分别验证身份读取独立于状态查询。Windows 用独占文件句柄复现访问失败，Unix 在文件权限实际受限时断言拒绝。

剩余 M3 包括显式初始化与克隆、Worktree 池与损坏恢复、direct MCP 上下文挂载、Checkpoint Diff 与 Safety Stash、拆分式提交及预检。Daemon 与协议接线另行协调。
