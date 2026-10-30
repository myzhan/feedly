# Feedly 架构说明

Feedly 是一个 **Tauri 2** 桌面应用。核心设计原则：**Rust 承担全部业务逻辑，前端只是薄视图层**。前端不实现任何抓取、解析或存储逻辑，只负责渲染和触发 IPC 命令。

## 分层总览

```
┌─────────────────────────────────────────────┐
│  WebView (React + TS，src/)                  │
│  组件 → hooks (TanStack Query) → api.ts      │
└───────────────┬─────────────────────────────┘
                │ invoke(command, args) / event
┌───────────────▼─────────────────────────────┐
│  命令层 commands.rs（IPC 契约 + 线程调度）   │
├─────────────────────────────────────────────┤
│  领域层                                       │
│   feed/  抓取·解析·净化·OPML                  │
│   scheduler/  并发刷新·定时·进度事件          │
├─────────────────────────────────────────────┤
│  数据层 db/  连接池·迁移·feeds/articles/settings│
├─────────────────────────────────────────────┤
│  基础设施  state·models·error                 │
└─────────────────────────────────────────────┘
                │
          SQLite (WAL, app_data_dir/feedly.db)
```

## IPC 契约（唯一的前后端边界）

- 所有能力都通过 `#[tauri::command]` 暴露，注册在 [`lib.rs`](../src-tauri/src/lib.rs) 的 `generate_handler!` 中。
- **命名约定**：JS 侧 `invoke` 传 **camelCase** 参数键，Rust 侧命令参数用 **snake_case**（Tauri 自动映射）；跨层数据结构在 [`models.rs`](../src-tauri/src/models.rs) 用 `#[serde(rename_all = "camelCase")]`，前端 [`types.ts`](../src/lib/types.ts) 是其逐字镜像。
- **错误约定**：命令统一返回 `Result<T, String>`。内部用 `AppError`/`AppResult`（thiserror），在命令边界 `.map_err(Into::into)` 转成人类可读字符串（[`error.rs`](../src-tauri/src/error.rs)）。
- **线程约定**：每个命令体都是 `blocking(&state, |conn| …)`——通过 `tokio::task::spawn_blocking` 把同步 SQLite 调用移出异步事件循环，并从连接池借一条连接（[`commands.rs`](../src-tauri/src/commands.rs)）。

前端把这套契约封装成三块：[`api.ts`](../src/lib/api.ts)（invoke 薄封装）、[`queries.ts`](../src/lib/queries.ts)（React Query hooks，含缓存失效与乐观更新）、[`events.ts`](../src/lib/events.ts)（订阅刷新事件）。

## 存储层（db/）

- 连接池：`r2d2` + `r2d2_sqlite`，`max_size = 8`；每条连接初始化 PRAGMA：`journal_mode=WAL`、`foreign_keys=ON`、`busy_timeout=5000`、`synchronous=NORMAL`。
- 迁移：`PRAGMA user_version` 版本化，`SCHEMA_V1` 建三张表，未来按 `version < N` 递增。
- **三张表**：
  - `feeds`：订阅与文件夹统一存为一棵自引用树（`kind ∈ {feed, folder}`、`parent_id`）。`feed_url` 上有**部分唯一索引**（`WHERE feed_url IS NOT NULL`）防止重复订阅。`etag` / `last_modified` 支撑条件请求。
  - `articles`：`UNIQUE(feed_id, guid)` 实现按源去重；`read_at IS NULL` 即未读（派生状态，无冗余布尔列）；`content_html` 存净化后正文，`plain_text` 存列表摘要。
  - `settings`：KV 表（刷新间隔、主题等）。
- 数据访问按表拆分在 [`feeds.rs`](../src/db/feeds.rs)、[`articles.rs`](../src/db/articles.rs)、[`settings.rs`](../src/db/settings.rs)。订阅未读数用 `read_at IS NULL` 分组统计，文件夹未读数通过 `WITH RECURSIVE` 子树聚合后自底向上 roll-up。删除文件夹用 `ON DELETE SET NULL`（子节点上浮到根），删除订阅用 `ON DELETE CASCADE`（连带文章）。

## 抓取与解析层（feed/）

1. **fetcher.rs**：`reqwest` 发起条件请求（带上 `If-None-Match` / `If-Modified-Since`），命中 `304` 直接短路；响应按 **5MB 流式上限**读取，超限即中断，防止异常大源拖垮内存。
2. **parse.rs**：`feed-rs` 统一 RSS 2.0 / Atom / RSS 1.0。字段有回落链：正文取 `content.body` → `summary.content` → 由标题+链接兜底生成；`guid` 取 `entry.id` → link → 内容哈希；`site_url` 取首个非 `rel=self` 的链接。
3. **content.rs**：`ammonia` 净化 HTML（默认白名单已排除 `script`/`style`/`on*`，仅额外放行 `class`），并生成列表用的纯文本摘要（截断加省略号）。
4. **opml.rs**：`roxmltree` 解析 OPML 导入（含文件夹嵌套、按 URL 去重跳过），导出反向生成 XML；有 round-trip 单测。

## 刷新与调度（scheduler/ + state/）

- **并发**：`stream::iter(...).buffer_unordered(8)` 并发抓取多个源；每个源写库在一个事务里完成（`INSERT OR IGNORE` 统计新增）。
- **重入锁**：`AppState.refreshing: AtomicBool`，`try_begin_refresh` / `release_refresh` 保证定时刷新与手动"全部刷新"不会叠加执行。
- **定时**：`scheduler::start` 启动后台 tokio 任务——启动约 5s 后首轮刷新，之后按 `refreshIntervalMinutes` 周期刷新；间隔为 0 表示仅手动。调度循环会周期性读取最新设置，改间隔即时生效。
- **事件**：刷新过程向前端 emit `refresh-started {total}` / `refresh-progress {feedId,newCount,notModified,error}` / `refresh-done {processed,total,newArticles}`，单源增量另发 `article-added`。前端 [`events.ts`](../src/lib/events.ts) 订阅并驱动进度条与缓存失效。

## 前端视图（src/）

- 三栏布局在 [`App.tsx`](../src/App.tsx) 组装：`Sidebar`（订阅树 + 未读徽标 + 折叠 + 增删改）｜`Topbar`（搜索防抖 + 状态分段 + 刷新/进度 + 全部已读 + OPML + 设置）｜`ArticleList`（无限滚动 + 选中即标读 + 星标）｜`ArticleView`（净化正文 `dangerouslySetInnerHTML` + 打开原文）。
- 状态管理交给 TanStack Query：`useArticles` 用 `useInfiniteQuery` 分页；已读/星标采用**乐观更新**（直接 patch 缓存中的行，避免整表重取导致滚动位置丢失），未读数则失效 feeds 查询。
- 主题用 Tailwind v4 的 `@custom-variant dark` 类模式，`App` 依据设置（含 `system` + `matchMedia`）切换 `<html>.dark`。
- 长列表用 CSS `content-visibility` 做原生离屏渲染跳过，作为轻量级虚拟化。
- 键盘快捷键在 `App` 的全局 `keydown` 中集中处理，输入框聚焦或有对话框打开时自动让位。

## 设计取舍

- **业务全在 Rust**：前端可替换（甚至换 WebView 框架），IPC 命令是唯一契约面，便于测试与演进。
- **派生未读而非冗余列**：`read_at IS NULL` 单一事实来源，避免状态不一致。
- **同步 SQLite + 连接池 + spawn_blocking**： rusqlite 是同步 API，用池化连接 + 阻塞线程卸载，比异步 ORM 更简单可控；写路径一律走事务。
- **条件请求 + 5MB 上限 + 并发限流**：兼顾省流、健壮与响应速度。
