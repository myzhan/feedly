# AGENTS.md — 面向 AI 开发代理的指南

本文件供大模型 / 编码代理在本仓库开发时遵循。只记录**原则与约定**；实现细节看代码与 [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md)，历史过程看 git log。动手前先读完本文件。

## 黄金法则

1. **业务逻辑全部在 Rust（`src-tauri/`）**。前端（`src/`）只是薄视图层：渲染状态 + 触发 IPC 命令。不要在前端实现抓取、解析、存储、调度或过滤。
2. **IPC 命令是唯一的前后端边界**。新能力先加 Rust 命令（`commands.rs` + `lib.rs` 的 `generate_handler!`），再在 `src/lib/api.ts` / `queries.ts` 对接。
3. **实证优先**：涉及依赖 API、平台行为或性能结论时，先复现/编译/测试验证，不凭记忆臆断。

## 目录结构

```
src/                    # React + TS 视图层（薄）
  components/           # 三栏 UI、对话框、图标
  lib/                  # api.ts(IPC 封装) / queries.ts(React Query) / types.ts(Rust 结构镜像) / events.ts(事件订阅) / utils.ts
src-tauri/              # Rust 后端（全部业务）
  src/
    commands.rs         # #[tauri::command] IPC 契约 + spawn_blocking 调度
    db/                 # mod.rs(连接池/迁移/schema) + feeds.rs / articles.rs / settings.rs
    feed/               # fetcher.rs(条件请求+流式) / parse.rs(feed-rs) / content.rs(净化) / opml.rs / mod.rs(抓取编排+事务)
    fulltext.rs         # 后台正文抽取（readabilityrs）
    image_cache.rs      # 正文图片本地缓存（sha256 + LRU）
    scheduler.rs        # 并发刷新 + 定时任务 + 进度事件
    models.rs           # 跨层数据结构（serde rename_all = "camelCase"）
    state.rs            # AppState：连接池 + reqwest Client + 信号量 + 刷新重入锁
    error.rs            # AppError / AppResult（thiserror）
  tauri.conf.json       # productName=Feedly, identifier=com.feedly.desktop, devUrl :1420
docs/ARCHITECTURE.md    # 分层与设计取舍
```

## 常用命令

前端命令在仓库根目录，Rust 命令在 `src-tauri/`。

```bash
pnpm install              # 前端依赖（包管理器是 pnpm，不要用 npm/yarn）
pnpm tauri dev            # 开发：Vite(:1420) + 原生窗口，热重载
pnpm typecheck            # tsc -b --noEmit
pnpm build                # tsc -b && vite build → dist/
cargo clippy --all-targets -- -D warnings   # 在 src-tauri/，CI 门禁：零告警
cargo test                # 在 src-tauri/，Rust 单元测试
```

改动后至少跑 `pnpm typecheck && pnpm build`；涉及 Rust 再跑 `cargo test` 与 `cargo clippy -D warnings`。

## 提交前自检（与 CI 同款）

`.github/workflows/ci.yml` 在 push/PR 到 `main` 时执行，任一失败都要再补一个 commit，所以**提交前本地全跑一遍**：

```bash
pnpm typecheck && pnpm build
cd src-tauri
cargo fmt --all --check                     # 不要只跑 cargo fmt，要确认无需再改动
cargo clippy --all-targets -- -D warnings
cargo test
```

- **跨平台告警本地看不见（最高频失败点）**：本地 macOS 跑 clippy 不会暴露 Linux/Windows 专属问题。凡是被 `#[cfg(target_os = "…")]` / `#[cfg(desktop)]` 隔离的代码，**只被它使用的 `const` / `fn` / 字段 / `use` 必须加上同样的 `cfg`**，否则其它平台的 CI 会以 `dead-code` 命中 `-D warnings` 而失败。平台专属的 `use` 也别放文件顶部。
- 新增依赖要过 `cargo fmt` 与 clippy 的 `-D warnings`；`-D warnings` 下未使用的导入、未使用的变量都是错误。
- 改了 `.github/workflows/*.yml` 要本地校验 YAML 语法（如 `npx js-yaml`）。
- 本地无法验证的 GUI / 原生行为（子 WebView 坐标、圆角裁剪、焦点与快捷键）必须在交付说明里标注“待实机验证”，不得当作已通过。

## IPC 契约约定

- **参数命名**：JS `invoke(cmd, { camelCaseKey })` ↔ Rust `snake_case`（Tauri 自动映射）。
- **返回结构体**：`models.rs` 所有跨层结构 `#[serde(rename_all = "camelCase")]`；`src/lib/types.ts` 逐字镜像，改一边同步改另一边。
- **错误处理**：命令一律返回 `Result<T, String>`，内部用 `AppError`，边界 `.map_err(Into::into)`；错误文案用中文。
- **线程模型**：SQLite 调用一律包在 `blocking(&state, |conn| …)`（`spawn_blocking`）里；禁止在 async 命令中同步查库。
- **写路径用事务**：多语句写入走 `conn.transaction()`。

## 关键依赖与已知坑

版本已锁定，不要随意升级或改 feature 名。

| 依赖 | 版本 | 约定 |
| --- | --- | --- |
| `feed-rs` | 3.0 | `Feed.entries`（非 `items`）；`Entry.id: String`；日期为 `chrono DateTime<Utc>`。 |
| `rusqlite` | 0.40 | 必须停在 0.40（0.41 移除 `r2d2`/`chrono` feature，`r2d2_sqlite` 锁 `^0.40`）。`bundled` 自带 SQLite。 |
| `r2d2_sqlite` | 0.35 | crate 名用下划线；pool builder 用 `.max_size(n)`。 |
| `reqwest` | 0.13 | TLS feature 名是 `rustls`（非 `rustls-tls`）。 |
| `ammonia` | 4.2 | 禁止对 `<a>` 手动 `add_tag_attributes("a", ["rel"])`（内部 panic）。默认白名单已足够，仅需 `.add_generic_attributes(&["class"])`。 |
| `tauri` | 2.x | 启用 asset 协议必须同时加 `features=["protocol-asset"]`，否则 tauri-build 构建脚本按 config/feature 一致性校验失败。叠加子 WebView（`Window::add_child` / `WebviewBuilder`）还需 `"unstable"` feature；`Webview` 的 `set_position/set_size/close` 只受 `cfg(desktop)` 限制，不需要 unstable。 |

Rust edition 2021，`rust-version = 1.80`。

## 存储 / Schema 约定

- 迁移用 `PRAGMA user_version` 版本化，新增表/列走新的 `SCHEMA_Vn` 段。
- `feeds` 是 feed+folder 统一自引用树（`kind`、`parent_id`）；`feed_url` 部分唯一索引去重。
- `articles` 用 `UNIQUE(feed_id, guid)` 去重；**未读是派生态**（`read_at IS NULL`），不加冗余布尔列。
- 全文抽取结果存 `articles.fulltext_html`；`fulltext_at` 非空表示已尝试（避免重试风暴，仅网络失败留 `NULL` 以便日后重试）。
- 图片本地缓存不进 DB：落 `app_data_dir/images/<sha256(url)>.<ext>`，LRU 上限 500MB。
- 文件夹未读数用 `WITH RECURSIVE` 子树 roll-up。

## 前端约定

- 状态管理交给 TanStack Query（`queries.ts`），不自建全局 store；列表用 `useInfiniteQuery`（`PAGE_SIZE=100`）。
- **列表不做去重**：跨源转载是合法独立记录，按原样全部显示；跨源区分靠 `ArticleList` 的 `showSource` 来源标签。
- **已读/星标用乐观更新**：直接 patch `['articles']` 缓存行（`patchArticles`），避免整表重取丢滚动位置。
- **进度与失效靠后端事件驱动**：`events.ts` 订阅 `refresh-*` / `article-added` / `fulltext-ready`，收到即失效对应查询。
- **正文相对 URL 必须在渲染层改写**：`ArticleView` 注入前用 `absolutizeUrls(contentHtml, article.link)` 绝对化；因 `upsert_articles` 是 `INSERT OR IGNORE`，后端解析改动对已入库旧数据无效，只能在前端处理。
- **正文点击路由**：点 `<img>` → 应用内 `Lightbox` 预览；带 `data-inline-original` 的裸网址（`linkifyBareUrls` 自动生成）→ 应用内卡片式原文视图；其它 `<a>` → `openExternal`。WebView 永不离开应用（桌面端无后退键）。
- **内嵌原文是原生子 WebView，不是 iframe**：rect 由宿主元素的 `getBoundingClientRect()` 给出（CSS 逻辑像素，与窗口内容区同原点），并用 `ResizeObserver` + `window resize` 同步；圆角由 Rust 侧 objc2 设 `layer.cornerRadius`（HTML 样式裁不到原生视图），改卡片圆角要同步改该常量。内嵌页获得焦点期间应用全局快捷键失效，因此返回入口必须留在应用自身顶栏。
- **正文图片按原分辨率显示**：`.article-body img` 用 `width:auto; max-width:100%` 覆盖 feed 内联表现属性，勿删 `width:auto`。
- **颜色一律用语义 token**（`@theme` 的 `--color-canvas` 等）：护眼绿随主题覆盖该变量，改主题只动 token，勿在组件里硬编码 `dark:bg-*`。
- 长列表行用 `.list-row`（`content-visibility: auto`）。键盘快捷键集中在 `App.tsx` 全局 `keydown`，输入框聚焦或有对话框打开时让位。

## 阅读提速约定

- **全文抓取只在后台做，不在点击时做**：进入列表即对未就绪文章后台预取（`fulltext.rs`，readabilityrs，仅当明显更优才入库）。点进来若还没就绪，**直接打开读订阅自带正文**——不转圈、不弹浏览器、不做点击插队。
- **`ArticleSummary.fulltext_ready`** = 应用内已可直接读（已存全文或源自带长正文）；列表星标左侧 `IconFulltext` 据此显示绿（就绪）/灰（未就绪）。
- 并发与超时集中在 `state.rs`（`fulltext_sem`、`web_client` 超时）；调优在此，勿散落各处。
- 图片本地缓存：渲染后 `api.cacheImages` 下载、`convertFileSrc` 改写到 asset 协议（前提见依赖表的 `protocol-asset`）。

## 启动优化（勿回退）

- `index.html` 内联 critical CSS 首帧直接涂主题色 + 按 `localStorage['feedly-theme']` 预置 `.dark`，避免白屏与闪烁。
- `vite.config.ts` 的 `optimizeDeps.include` 预声明全部运行时 ESM 依赖 + `server.warmup.clientFiles` 预热入口；新增依赖要同步进 include。
- 按需对话框用 `React.lazy` 拆出启动 chunk；后端首刷延后 5s 且走 `spawn_blocking`，勿把网络/DB 挪回主线程 `setup`。

## 测试要求

- DB 访问、解析、净化、OPML、调度/条件请求逻辑都应有单测；用 `db::tests::test_pool()`（临时文件）。
- 新增解析字段时补 RSS 2.0 + Atom 两个 fixture。
- 提交前按上文「提交前自检（与 CI 同款）」全部跑一遍。

## 不要做

- 不要在前端写业务逻辑或直接访问数据库/网络。
- 不要引入新依赖/插件而不核实与锁定版本的兼容性。
- 不要为了修复而删除现有测试或放宽 `-D warnings`。
- 不要把本文件写成实现过程 / 变更流水账——只沉淀原则与约定。
