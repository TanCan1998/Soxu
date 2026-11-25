# soxu-git：M3a 只读探测底座

本阶段仅探测现有本地工作树，不创建、克隆、初始化或回退用户仓库。原有 `CheckpointSummary` 字段及 serde 契约保持不变。

## 消费接口

- `GitProbe::new(absolute_git_executable)`：绑定调用方信任的原生 Git 绝对路径，Windows 拒绝脚本 shim。
- `GitProbe::discover()`：从 PATH 的绝对目录查找原生入口；发现不代表签名或可信性证明，安全敏感宿主应使用 `new`。
- `GitProbe::probe(absolute_directory).await`：接受工作树根或其子目录，返回 `RepositorySnapshot` 的绝对 `root`、独立 `git_dir`、共享 `common_dir`、`head` 与 `status`。linked worktree 的两个 Git 目录分开返回。
- `HeadState::{Attached, Detached, Unborn}`：分支名保留原始字节；对象标识为完整 SHA-1 或 SHA-256，unborn 不伪造 HEAD。
- `parse_status(&[u8])`：纯函数，解析 `git status --porcelain=v2 -z`。`StatusEntry` 区分普通变更、重命名/复制、冲突、未跟踪与忽略文件；保留 XY、子模块四位状态、模式、对象标识与相似度。未知记录类型、非法字段或截断均返回 `ParseError`，不返回部分快照；同一流的头部、记录间及记录内 OID 长度必须一致，不能混用 SHA-1/SHA-256，零 OID 同样校验。未来头部字段保留原始键值。
- `GitPath::as_bytes()`：原始 Git 路径身份；serde 输出字节数组而非有损字符串。`to_path_buf()` 在 Unix 保留非 UTF-8，Windows 对非 UTF-8、特殊字符及可能归一化的保留名/尾随字符明确拒绝。该转换不是文件访问沙箱，不负责链接、大小写或路径竞态裁决。
- `GitError`：区分入口/目录错误、非仓库、无工作树、外部过滤器、子模块、命令失败、超时、输出超限、路径编码及解析失败。命令失败保留原始 stderr，Display 不输出仓库路径或 stderr。

## 只读与限制

所有子进程直接传参数并设置独立 `current_dir`，不启动 shell，不改变全局 cwd 或环境。子进程清除继承的 `GIT_*`，忽略系统/全局 Git 配置，保留仓库自身的语义配置；禁用 optional locks、fsmonitor、untracked cache、自动维护、子模块摘要和按需 fetch，关闭交互。仅执行 `rev-parse`、只读 `config`、`ls-files` 和 `status`。单命令超时 15 秒，stdout/stderr 各上限 16 MiB；任一流读到上限加一字节立即返回 `OutputLimit`，不等待 EOF 或自然退出而退化成 `Timeout`。超限、超时或读取失败会终止并等待回收直接子进程，不承诺回收恶意入口派生的整个进程树。

`status` 可能经 clean/process 过滤器或子模块配置执行外部程序，因此遇到仓库外部内容过滤器或索引 gitlink 时提前失败关闭。未初始化子模块也明确不支持；纯解析器仍完整支持子模块标记。过滤器查询显式使用 `config --includes`，覆盖本地配置、已启用 `extensions.worktreeConfig` 的 linked worktree 配置及其 include/生效的 includeIf；系统/用户配置不参与。配置查询失败会拒绝探测，不自动降级为忽略配置继续运行。探测没有 OS 级沙箱，不承诺面对恶意 Git 可执行文件、配置读取与启动之间的篡改竞态或并发 Git 变更时提供原子快照；调用方应串行化更改，并信任所绑定原生入口。

bare 仓库和 `.git` 元数据目录暂不支持。忽略文件只在纯解析器输入中支持，探测默认不枚举。无新第三方依赖，临时夹具使用标准库，构建缓存与 AOCI 运行资产沿用已有忽略规则。

## 验证与后续

`mise exec -- cargo test -p soxu-git --locked` 覆盖 unborn/attached/detached、子目录、linked worktree、暂存/未暂存、Unicode/空格、冲突、目录错误、fsmonitor 与过滤器拒绝，以及解析器的换行、非 UTF-8、复制、七类冲突、SHA-256 和非法输入。临时内嵌 Git 工作树实测 `--untracked-files=all` 仍返回带单个 `/` 后缀的 `?` 目录记录；忽略枚举返回对应 `!` 目录记录。仅这两类允许单个目录后缀并保留原始字节，跟踪、重命名源/目标和冲突路径仍拒绝目录后缀；所有类型均拒绝内部空分量、双斜线、绝对路径及 `.`/`..`。探测前后比较整个临时仓库的文件列表与字节，确保索引和对象未变。Unix 专项真实路径用例只在 Unix 执行；Windows 上执行纯字节解析与拒绝转换用例。

副作用夹具在临时根中放置可写 marker 脚本，逐一验证 clean/process/fsmonitor 的本地、include、条件 include、linked worktree 与 worktree include 共 15 种组合：探测后 marker 不存在且仓库字节不变；原生 Git diff（过滤器）或 status（fsmonitor）正向对照必须写出 marker，避免以不存在的命令冒充安全证据。输出夹具复用原生 Rust 测试二进制，以较小限额验证 stdout/stderr 超限后仍存活的进程返回 `OutputLimit` 而非 `Timeout`、精确上限合法、空闲进程超时，均检查直接子进程已被等待回收。

剩余 M3：三合一接入、Worktree 池/损坏恢复、direct MCP 上下文挂载、三基准 Diff、Checkpoint 与 Safety Stash、拆分式提交及预检；Daemon/协议消费接线需独立契约协调。本阶段不宣称这些任务已完成。
