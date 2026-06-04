# 设计文档评审与修复跟踪：PDF Debugger

> 评审对象：`docs/pdf-debugger-architecture.md` 与 `docs/product-shape.md`
> 方法：6 个维度并行专项评审（MuPDF/FFI 正确性、Rust 类型与线程模型、跨文档一致性、MCP 安全、产品/法务、完整性缺口），每条结论均对照原文逐字校验引用、并核验 MuPDF/Rust 技术断言的真伪后给出。
> 结果：原评审给出 **57 条结论**（主评审 47 条 + Rust 模型补充 10 条）并驳回 2 条断言。后续复核发现其中 1 条 raw stream 结论不成立，且少数严重级别/措辞需要收敛。本文保留评审记录，并标注修复后的判断。

> 当前状态：核心设计文档已按本评审修复 `fz_try` 范式、MuPDF context locks、对象 generation、取消 token、MCP allowlist、安全模式、报告/MCP 范围、授权/构建/隔离决策、`ObjectDetail`、C ABI wire DTO、分页语义、诊断码和 schema 版本等问题。
> 最新收敛：产品和架构已明确为 MuPDF-only；替代引擎/多引擎切换是 MVP 非目标。已完成 DTO ↔ C wire 全量映射审计，并补齐 `pdbg_diagnostic`、`pdbg_stream_summary`、显式 C enum、node-token registry、文本 DTO、诊断/stream/render accessor 与 M0 验收项。

> 注意：下文的行号是原评审时的定位；修复后请按章节标题重新定位。

---

## 总体结论

两份文档结构清晰、思路成熟：分层架构（egui → Rust → C shim → MuPDF）、惰性树数据模型、只读的 MCP 定位、里程碑规划都很扎实。问题集中在两处：

1. **MuPDF/Rust FFI 与线程边界的精确机制**——文档写得足够细，因此是"写错了"而不仅仅是"写得含糊"；
2. **安全/安全性控制只声明了"属性"却没给"强制执行机制"**。

## 最新 DTO ↔ Wire 审计结果

当前 `pdf-debugger-architecture.md` 已能从 §7 C wire/API 填出 §5 Rust DTO：

| Rust DTO | Wire 来源 |
| --- | --- |
| `DocumentSummary` | `pdbg_document_summary_out` + metadata/permissions/safety/diagnostics |
| `ObjectSummary` | `pdbg_dict_entry` + list context + node-token registry + diagnostics |
| `ObjectDetail` | `pdbg_object_detail_out` + paged children/dictionary entries + stream + diagnostics |
| `StreamSummary` | `pdbg_stream_summary` |
| `StreamChunk` | `pdbg_buffer_*` + request mode/offset + buffer diagnostics |
| `RenderResult` | `pdbg_image_*` + Rust-side duration + image diagnostics |
| `TextPage` / `TextSpan` | `pdbg_text_page_span_*` + normalized top-left page coordinates |
| `DiagnosticSummary` | `pdbg_diagnostic` + node-token registry fallback |

本轮修复关闭了两个实现级缺口：完整诊断缺少 wire 来源、`StreamSummary` 无法从 C 侧填充。M0 清单已追加 node-token registry、diagnostic wire、stream summary wire、显式 C enum、稳定 public string 的测试项。

---

## 🔴 原评审 Blocker

### B1. C 错误边界范式在 `fz_try` **内部** return —— 会破坏 MuPDF 异常栈
- **位置**：`pdf-debugger-architecture.md` §7.6，行 513–528
- **问题**：`fz_try`/`fz_always`/`fz_catch` 是基于 setjmp/longjmp 的宏，每个 context 维护一个异常帧栈；该帧只有在代码**顺序落到** catch 块时才会被弹出。范式里把 `return PDBG_OK;` 放在 `fz_try` 体内，跳过了弹栈，留下一个指向已销毁栈帧的 setjmp 缓冲区。该 context 上**下一次** `fz_throw` 就会 longjmp 进入垃圾内存 → UB/崩溃。这正是 shim 要防止的"longjmp 跨越 Rust 栈帧"失效，却被写进了每个 shim 函数都要照抄的范式里。
- **修复**：任何 `return`/`goto`/`longjmp` 都不得离开 `fz_try`/`fz_always` 块。`break` 只能按 MuPDF 文档用于退出当前宏块。改用局部变量：
  ```c
  pdbg_status status = PDBG_OK;
  fz_try(ctx) { /* MuPDF calls */ }
  fz_catch(ctx) { pdbg_fill_error(ctx, err); status = pdbg_map_error(ctx); }
  return status;
  ```
  并在文档中显式写明此规则。

### B2. 克隆出的 worker context 没有写明 `fz_locks_context` —— 共享的 store/分配器/字形缓存会发生数据竞争
- **位置**：§6.1，行 346–347
- **问题**：`fz_clone_context` **不会**让每个 worker 拥有独立状态；克隆体共享分配器、资源 store、字形缓存、色彩空间 store。MuPDF 只有在根 context 通过 `fz_new_context(alloc, locks, max_store)` 并传入非空的锁回调时，这些共享状态才线程安全。Rust 侧每个 `DocumentSession` 的互斥锁**覆盖不了**这一点。
- **修复**：明确根 context 安装 `fz_locks_context`（`FZ_LOCK_MAX` 个互斥锁数组），并写明克隆体共享 store、只有因此才安全。锁可由 `pdbg_context_new` 内部安装，不一定要暴露为 ABI 参数。

### B3. MCP 文件白名单只写了"谓词"没写"强制执行算法" —— MCP 上线前必须修
- **位置**：§8.3/§8.4，行 589–590、668
- **问题**："path must be inside configured allowlist roots" 这种措辞极易诱导出朴素的字符串前缀匹配，可被三种方式绕过：`../` 路径穿越；白名单目录内的符号链接指向 `/etc` 或 `~/.ssh`（MuPDF 的 `open()`/`fopen()` 会跟随符号链接）；以及 `/srv/pdfs-evil` 匹配到根 `/srv/pdfs`。而 `pdf_open` 是 agent 能读哪些文件的**唯一**闸门。
- **修复**：在检查前用 `std::fs::canonicalize`（解析 `..` 与符号链接）规范化请求路径；启动时一次性规范化各 root；要求规范化后的路径是 root 的**真实路径分量后代**（而非字符串前缀）；拒绝 root 内的符号链接或用 `openat`/`O_NOFOLLOW` + 传 fd 关闭 TOCTOU 窗口；规范化失败一律硬拒绝，不得回退到原始路径。

---

## 🟠 原评审 Major

### M1. 所有权规则遗漏 `pdf_obj` 引用计数与 borrowed/owned 纪律
- **位置**：§7.5，行 503–509
- **问题**：MuPDF 的生命周期是引用计数而非唯一所有权；`pdf_dict_get`/`pdf_array_get`/`pdf_resolve_indirect` 返回的是**借用**引用，未先 `pdf_keep_obj` 不得 drop。文档只覆盖了 FFI 边界，未覆盖 shim 在遍历对象图时内部的 keep/drop 纪律——这是 double-free/泄漏的头号根源。`pdbg_image`/`pdbg_buffer` 各自拥有哪些 `fz_*` 引用也未定义。
- **修复**：写明 borrowed/owned 分类与 keep/drop 纪律；定义每个句柄拥有的引用（全部在 `fz_try` 保护下，因为 `fz_drop_*` 会在错误路径上调用）。

### M2. `DocumentSession { raw: NonNull<PdbgDoc> }` 的 `Send`/`Sync` 策略未定义
- **位置**：§6.2 行 357；§6.1/§6.3
- **问题**：含 `NonNull<T>` 的结构体自动 `!Send + !Sync`。如果最终设计要在线程间移动或共享 `DocumentSession`，必须显式定义 `Send`/`Sync` 不变式；如果 session 常驻 worker、UI 只发 command，则可以避免让整个 session 跨线程移动。
- **修复**：文档化选定策略。若使用 `unsafe impl Send for DocumentSession`，必须写明句柄仅在单一串行器临界区内被访问、绝不别名、每个 worker 用自己克隆的 context。page/device 句柄应在单次 worker 调用内创建并销毁。

### M3. "不执行 PDF JavaScript" 只限定在 MCP —— JS 必须在 `pdf_document` 层面、对整条管线禁用
- **位置**：§8.4 行 676
- **问题**：JS 执行是文档/context 的属性，与调用方无关；MuPDF 在启用时会运行表单字段 calc/format/validate 脚本与 OpenAction JS。若只在 MCP 路径抑制 JS，则**在 GUI 里打开恶意 PDF**即可执行攻击者 JS。
- **修复**：在打开时对所有 session 禁用 JS；绝不自动触发 OpenAction/AA；重新启用须是未来的、GUI 确认的逐文档操作。增加一个用例测试：打开含 calc 字段/OpenAction JS 的 PDF 并断言无执行。

### M4. shim 中无 `fz_cookie` 取消机制 —— "可取消渲染" 与 "每工具超时" 无法兑现
- **位置**：§7.4 `pdbg_page_render` 行 493–499；需求散见于行 87、333、350、670
- **问题**：MuPDF 唯一的在途取消机制是 `fz_cookie.abort`（由 device 轮询）；在 `fz_*` 调用中途杀线程会破坏 context。没有任何 shim 入口携带 cookie，因此病态页面无视超时跑到底 → MCP 的 DoS。
- **修复**：给每个长耗时 shim 入口（render、stream load、text extract、search）加 `fz_cookie`/abort 标志句柄；Rust 侧超时/取消设置 `abort`（原子）；映射到 `PDBG_ERROR_CANCELLED`。*（含并入 mupdf-ffi 维度的"缺取消机制"一条。）*

### M5. 只规定了有界**输出**，没规定有界**解码** —— 解压炸弹绕过所有限制
- **位置**：§8.4 行 671–673；§5.5；§13 本身就把该风险列为 Key Risk
- **问题**：50 KB 的 Flate 流可在 MuPDF 内部膨胀到数 GB（在任何 Rust 侧截断**之前**）；`offset`/`limit` 仍会物化整个解码缓冲。
- **修复**：在解压**过程中**限制解码字节数（计数/限流的 `fz_stream` 包装）；限制滤镜链膨胀比与递归/对象深度；惰性解码到 `offset..limit` 窗口；用 `fz_new_context` 的 `max_store` + 分配器上限来强制内存预算，而非事后测量 DTO。

### M6. 解析不可信 PDF 缺少进程隔离与崩溃/panic 防护
- **位置**：mcp-security + gaps 维度；§7、§13、product §14 行 553
- **问题**：两个关联问题：**(a)** 文档防止了 `longjmp` 进入 Rust，却从未管反向——`fz_try` 帧内的 Rust panic 跨越 C 栈展开 = `abort`/UB；**(b)** MuPDF 是大型 C 代码库，在图像/字体/滤镜解码器中有 CVE 历史，而整个应用是单一进程信任域，一个恶意文件即可拖垮所有已打开文档。本工具的明确职责恰恰是打开恶意/损坏 PDF 并暴露给 agent。
- **修复**：每个 `extern "C"` 边界用 `catch_unwind` 包裹（或 `panic=abort` 并说明理由）；把 MuPDF 解析/渲染放进低权限沙箱 worker 进程（Linux seccomp-bpf/Landlock，macOS App Sandbox，Windows restricted token）——这同时用 OS rlimit 约束了 §8 的各项限制，并使崩溃可恢复。与 §18 "内置 vs 独立伴随进程" 的开放问题挂钩。

### M7. MuPDF AGPL 与拟议的商业版本相冲突 —— 标为风险却从未解决
- **位置**：arch §13 行 847；product §7（行 348、361–390）—— *两条结论同一问题*
- **问题**：MuPDF 为 AGPLv3 或商业双授权（Artifex）。在 AGPL MuPDF 上发行闭源 Pro/Team/AI 版本需购买商业授权；AGPL 构建则触发分发 + §13 **网络**条款义务，这直接波及只读 MCP 服务器。
- **修复**：新增 "Licensing & Distribution" 章节：明确路径（购买并预算 Artifex 商业授权，或以 AGPL 发行并附源码 + §13 合规）；将各版本映射到授权；针对网络 MCP 服务器分析 AGPL §13；§7 各版本以此决策为前提。本地化的 Community/Core MVP 可先在 AGPL 下推进。

### M8. 缺少 MuPDF C 的构建/vendoring/链接策略
- **位置**：§7；Milestone 0，行 794–800
- **问题**：产品命脉是构建/链接一个大型 C 库及其依赖（freetype、harfbuzz、jbig2dec、openjpeg、zlib），却没决定系统库 vs vendored 子模块 vs 钉版 tarball、`build.rs`+cc/cmake vs 外部 Makefile、静态 vs 动态链接（也影响 AGPL 义务）、版本钉定、shim crate 如何向 bindgen 暴露头文件。
- **修复**：新增 "Build & Vendoring" 子节覆盖上述各项；与 M7 的链接决策挂钩。

### M9. 报告导出在文档间自相矛盾
- **位置**：product §15 行 594（=MVP）；product §18 行 664（=开放问题）；product §16（=Preview 2）；arch §3.1（未列）
- **问题**：四处数据点不可能同时成立。
- **修复**：二选一。报告数据模型已存在（product §11），由现有 DTO 组合而成、无需新 shim，所以这是范围决策而非新架构。

### M10. 产品侧的"安全状态"在架构里没有支撑
- **位置**：product §14 行 556–566、§6.4；arch §3.1/§5/§7
- **问题**："safe mode"、"JavaScript disabled"、"embedded files detected"、"external file references detected"、"OCR disabled/enabled" 被声明为**必需**，工作流 §6.4 还"以 safe mode 打开"——但 `pdbg_document_open` 只收 path/password，诊断模型与文档摘要里没有这些检测，"safe mode" 也从未定义。
- **修复**：定义一次 "safe mode" 并把 open-options 参数贯穿架构；把 embedded-file/external-ref/JS 检测加入诊断模型；或将这些状态移至 post-MVP。

---

## 🟡 原评审 Minor

### 一致性 / 范围
- §8.2 "MVP MCP Tools" 标题滥用了 "MVP"——MCP 属于 Milestone 4 / Preview 4，被排除在产品 MVP 外。改名为 "Initial MCP Tool Set"。
- Search 在 arch §3.1/Milestone 3 属 MVP，但在 product 里是 Preview 2（非 Preview 1）——明确 "MVP" 是 Preview 1 还是全部 preview 之和；加一张 Milestone↔Preview 映射表（同时解决 render-preview 与 render-timing 的拆分错位）。
- HTML 报告导出（product §11）未出现在 MVP §15（仅 JSON/Markdown）——标记 HTML 为 post-MVP 或加入。
- 算子高亮出现在旗舰工作流 §6.1 第 7–8 步，却是明确的 MVP 非目标——把这两步标注为 Preview 5。
- 承诺了 "object count"（§3.1、product §11）但没有 `DocumentSummary` DTO 定义它；对修复过的文件还有歧义（xref 声明值 vs 实际可解析数）。补 DTO 并定义语义。

### Rust 模型（均在 `pdf-debugger-architecture.md` §5/§6/§10）
- `lock: Mutex<()>` + `task_queue` 暗示两条串行化路径；空元组互斥锁不保护任何数据。删掉互斥锁（单 worker 独占 `raw`），或把句柄包成 `Mutex<NonNull<…>>`。
- 同一 PDF 对象映射到三个不相等的 `NodeId` 变体（`XrefObject`/`IndirectRef`/`Stream`），且 `XrefObject`/`Stream` 丢掉了 generation（对象身份是 num+gen）。定义规范的 `ObjectId{num,gen}` 作为缓存/选中键，与基于路径的 `NodeId` 分离。
- `DictEntry`/`ArrayEntry` 用 `parent: Box<NodeId>` 且 derive `Hash`/`Eq`/`Clone` → 每次缓存查找、UI 状态哈希、clone 都是 O(深度)，外加逐层堆分配。把路径 intern 成 `Copy` 句柄，或用扁平的 `Arc<[PathSegment]>`；为 §11.4 压测用例规定环/深度策略。
- `ObjectDetail`（核心 `NodeModel` 方法的返回类型）与 C 侧 `pdbg_node_id`/`*_out`/`pdbg_render_options` 结构体只被引用、从未定义。递归的 `NodeId` 不能是平凡 C 结构体——指定 FFI 编码（不透明 token，或 扁平 tag + `ObjectId` + 有界路径数组）。
- 缓存："small decoded streams" 的 "small" 未定义；仅 render 缓存有 LRU（其余只有字节上限、到限时行为未定义）；`DocumentCache` 的线程安全契约未声明。
- 对象号用 `i32`/`int`、索引/计数用 `usize`/`size_t`——无受检转换契约；用 `try_into()` 而非 `as`；非负对象号考虑 `u32`。
- `DiagnosticSummary` 是 `Clone`、内含 `Option<NodeId>` + `String code`，又被缓存的 `ObjectSummary` 嵌套 → 递归深 clone。把 `code` 做成 `&'static str`/枚举（同时满足"稳定诊断码"）；考虑 `Arc<[…]>`。

### FFI / MuPDF
- `pdbg_context_new(out, err)` 必须在 context 存在**之前**报错，但 `pdbg_fill_error(ctx, err)` 需要活的 ctx——定义一个无 ctx 的错误填充函数，以及 `message[1024]` 的 NUL 终止/截断契约。
- "Raw stream = 压缩但已解密" 经复核是 MuPDF `pdf_load_raw_stream`/`pdf_open_raw_stream` 的语义，应保留；无需改成"仍加密"。仍需要保证 stream API 带 `ObjectId{num, gen}` 并在解码阶段执行字节上限。
- 文本抽取与搜索是 MVP 功能却不在 shim API 中——增加 `pdbg_page_extract_text`/search，返回拥有所有权且可 drop 的结果（`fz_stext_page`），带 cookie、UTF-8 保证与不可信内容标志。

### MCP 安全
- 不可信文本标注（§8.4）没有具体封装格式也没指明执行者——指定贯穿 `pdf_extract_text`/`pdf_get_stream` 文本模式/`pdf_search_text`/`pdf_get_object` preview 的专用 `untrusted: true` 结构化字段，由服务器输出；并说明该标注只是建议性的，真正边界是有界/只读保证。
- 渲染图 "artifact reference"（§8.3）没有定义生命周期/存储/访问控制——不可猜测的 ID、限定到签发会话、私有存储、按内存预算计的 TTL/淘汰、把取回绑定到同一客户端的访问校验。
- MCP 传输/监听地址/认证/多客户端，以及并发调用如何映射到单写者 session 都未定义——默认 stdio 或仅 localhost，任何网络传输都要认证（默认关闭），定义队列/背压，澄清各项限制是 per-call / per-client / global。

### 产品 / 缺口
- §18 开放问题部分已过时/关乎决策：hex viewer 已是 MVP（arch §3.1、product §5.3/§6.2）——删掉该问题；多文档标签在构造上已是多文档（每个 `NodeId` 都带 `doc`）——现在就定 UI 暴露方式（建议加简单标签切换器）。
- §17 成功标准（"打开大 PDF 不卡死"、"足够快"）没有数字——补目标（如 500 MB/100 万对象样本、首次可交互 < N s、无帧 > 16 ms、跳转 p95 < 100 ms）并接入 §11.4。
- 反复承诺 "stable" 的 DTO/MCP 契约/诊断码却无版本/兼容策略——新增 "Stability & Versioning" 章节（schema_version、公开 vs 内部、弃用策略；说明 `NodeId` 序列化是否属契约）。
- 一个捆绑 C + GPU UI 的原生应用未声明支持的操作系统——补一行（影响构建矩阵、打包/签名、白名单路径语义）。
- §11 对一个敌意输入解析器缺少模糊测试与 sanitizer——增加 cargo-fuzz 目标（open/decode/conversion）、对 shim+MuPDF 的 ASAN+UBSan CI 任务、以及恶意/CVE 语料库。
- 持久化（最近文件、MCP 白名单 root、设置、布局）的位置/格式/安全未定义——补 "Configuration & Persistence" 子节（白名单是安全敏感文件）。
- CJK/RTL 抽取正确性、egui 显示抽取文本的 CJK 字体回退、`RenderRequest` 中的 HiDPI/设备像素比缺失——写明预期或为 MVP 显式排除。
- 尽管承诺"大文件下保持响应"，却无具体缓存字节上限或每文档内存上限默认值。
- 攻击者可控文本经剪贴板/HTML-Markdown 导出（`Copy bounded excerpt`、HTML 报告）没有转义/净化策略，而 MCP 输出有——把 §14 扩展到 GUI 出口。
- 产品名 "MuPDF Debugger" 内嵌了 Artifex 商标——当作代号；用 "built on MuPDF" 的署名方式。

## ⚪ Nit
DocumentSession 的 drop 顺序 / 根 context 长于克隆体的不变式（已被内嵌 `doc->ctx` 缓解）；`parking_lot::Mutex` 不可重入 vs MuPDF 可重入回调（两层锁序——FZ_LOCK_* vs session 锁）；渲染分辨率上限应是总像素/字节上限并对 zoom 设界；`PdbgDoc`（驼峰）vs `pdbg_doc`（蛇形）命名——标准 FFI 习惯，写明别名即可；`ChildPage.total: Option<usize>`——定义 `None` 与翻页终止信号；只读主旨 vs §14 未来写操作（arch §3.2/§8.4 已列非目标，交叉引用即可）；渲染重模式 vs "非阅读器"（在 §3.2 加一行约束）；persona 与 MVP 覆盖（加覆盖表——Primary/Secondary 分类已基本覆盖）；AI-Extension 版本 vs MVP-MCP 措辞；版本名/产品名占位符。

---

## ✓ 已核查并驳回的两条断言
- *"Xref Table 面板在数据模型里没有取数路径"* —— **驳回**：`NodeModel::children(XrefRoot, range)` 已提供；xref 易变性警告是稳定 ID 设计的**理由**，不是矛盾。
- *"同意模型矛盾：架构允许 agent 打开 root 内任意文件，产品却暗示逐文档启用"* —— **驳回**：product §6.5 原文是 "Enable MCP for this document **OR allowlist root**"，两份文档描述的是**同一个**两层模型。

---

## 建议的下一步
1. **已处理** `pdf-debugger-architecture.md` 里的三个 blocker：`fz_try` 范式、`fz_locks_context`、MCP 白名单算法。
2. **已补** Licensing & Distribution、Build & Vendoring、Process Isolation & Crash Safety 的设计决策。
3. **已贯穿** 取消句柄到 shim ABI，并补充解码/渲染限制。
4. **已对齐** MVP、Preview、报告导出、MCP 和安全状态的范围。
5. **已补齐** 收尾规范项：`ObjectDetail`、`ObjectId` 一致化、`ChildPage.total` 语义、C ABI 扁平 wire format、稳定诊断码和公开 schema 版本。
