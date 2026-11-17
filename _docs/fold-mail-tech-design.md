# Fold 客户端技术设计文档

> 版本 v1.0 · 2026-11-17
> 对应：`fold-mail-prd.md` / `fold-mail-brainstorm.md`
> 范围：Rust 客户端（桌面端 + CLI + 共用核心库）

---

## 1. 架构总览

### 1.1 核心约束
**UI 永远只读本地数据；任何网络操作异步完成再通知界面。** 打开邮件时从 SQLite 立即渲染，同步在后台跑——这是"快"的手感来源。

### 1.2 分层架构
```
┌──────────────────────────────────────────────────────────┐
│  fold-desktop (iced/GPUI)      foldctl (CLI)             │  ← 只管交互
│  UI 状态 / 视图 / 快捷键        命令行参数 / 输出格式化      │
└───────────────┬──────────────────────┬───────────────────┘
                │ 本地 API（进程内 / Unix Socket，无需网络）
┌───────────────▼──────────────────────────────────────────┐
│                      fold-core                           │
│  sync   send    search    rules    crypto    privacy     │
│  IMAP   SMTP    Tantivy   TOML     Sequoia   ammonia     │
│  store(SQLite) · model · config · account · error        │
└───────────────┬──────────────────────────────────────────┘
                │
        ┌───────▼────────────────────────────┐
        │ 本机 SQLite + 附件目录 + 索引目录    │
        │ $XDG_DATA_HOME/fold/                │
        └────────────────────────────────────┘
```

### 1.3 数据流原则
- **写路径**：网络 → 解析 → 清洗 → 入库 → 建索引 → 发事件通知 UI
- **读路径**：UI → 本地查询 → 立即渲染（永不等待网络）
- **事件**：core 通过 `tokio::sync::broadcast` 发领域事件，UI 订阅刷新

---

## 2. 技术选型

| 层 | 选型 | 理由 | 风险 / 备选 |
|---|---|---|---|
| 语言 / 构建 | Rust 2024 edition, cargo workspace | 核心库与 UI / CLI 分离 | — |
| 异步运行时 | `tokio` | 生态最全 | — |
| IMAP | `async-imap` + `rustls`, IDLE 推送 | 符合标准协议 | 各家实现有偏差，需适配层 |
| SMTP | `lettre` | 事实标准 | — |
| MIME 解析 | `mail-parser` | 快、容错好 | 畸形邮件需兜底 |
| OAuth2 | `oauth2` + `keyring` | 凭据不落明文 | keyring 不可用则拒绝保存 |
| 本地存储 | `rusqlite`（SQLite） | 单文件、可靠、好备份 | 附件存文件 + DB 记路径 |
| 全文索引 | `tantivy` | Rust 里的 Lucene，离线搜索质量高 | 索引重建成本 |
| 正文渲染 | `blitz` + `ammonia` | 无 WebView，脚本不执行 | **M0 重点验证**；失败回退纯文本 |
| 加密 | `sequoia-openpgp` + `sequoia-autocrypt` | 纯 Rust，不依赖 GnuPG | 兼容性有限 |
| 规则脚本 | TOML 声明式为主，`rhai` 可选 | 简单规则不写代码，复杂规则能写 | — |
| UI | `iced`（wgpu） | 纯 Rust、跨平台 | **M0 验证**；GPUI 更漂亮但 API 变动频繁 |
| 配置 | TOML + XDG 目录 | 可读、可 diff、可版本管理 | — |
| 错误 / 日志 | `thiserror` / `anyhow` + `tracing` | 常规组合 | — |
| 测试 | `insta` 快照 + `wiremock` 模拟协议 | 协议层必须可回归 | — |
| 发布 | `cargo-dist` | 生成各平台安装包 | — |

---

## 3. Workspace 结构

```
fold-mail/
├── Cargo.toml                  # workspace 根，统一依赖版本
├── crates/
│   ├── fold-core/              # 核心库，无 UI 依赖
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── model/          # Account, Message, Thread, Attachment
│   │       ├── store/          # SQLite schema / repository
│   │       ├── sync/           # IMAP, IDLE, 适配层
│   │       ├── send/           # SMTP, outbox, send_at
│   │       ├── search/         # Tantivy 索引与查询
│   │       ├── rules/          # TOML 解析 + Rhai 运行时
│   │       ├── privacy/        # 清洗、追踪拦截、域名规则
│   │       ├── crypto/         # Autocrypt, OpenPGP
│   │       ├── config/         # TOML + XDG
│   │       ├── events.rs       # 领域事件定义
│   │       └── error.rs
│   ├── fold-ipc/               # UI 与 core 之间的 DTO / 命令协议
│   ├── fold-desktop/           # iced 应用
│   │   └── src/{main,ui,views,shortcuts,state}
│   └── foldctl/                # CLI
└── _docs/
```

**依赖方向**：`desktop`/`foldctl` → `fold-ipc` → `fold-core` → 存储与网络。`fold-core` 不反向依赖任何 UI。

---

## 4. 数据模型（SQLite）

### 4.1 核心表
```sql
CREATE TABLE accounts (
  id            INTEGER PRIMARY KEY,
  email         TEXT NOT NULL UNIQUE,
  display_name  TEXT,
  provider      TEXT,              -- gmail/outlook/qq/163/custom
  auth_type     TEXT NOT NULL,     -- oauth2 / app_password
  enabled       INTEGER NOT NULL DEFAULT 1,
  created_at    INTEGER NOT NULL
);

CREATE TABLE folders (
  id           INTEGER PRIMARY KEY,
  account_id   INTEGER NOT NULL REFERENCES accounts(id),
  remote_name  TEXT NOT NULL,      -- IMAP 原始名，如 INBOX
  local_name   TEXT,               -- 显示名
  kind         TEXT NOT NULL,      -- inbox/sent/draft/archive/trash/custom
  flags        TEXT,               -- JSON 数组
  UNIQUE(account_id, remote_name)
);

CREATE TABLE messages (
  id             INTEGER PRIMARY KEY,
  account_id     INTEGER NOT NULL REFERENCES accounts(id),
  folder_id      INTEGER REFERENCES folders(id),
  remote_uid     INTEGER,
  thread_id      INTEGER,
  from_addr      TEXT NOT NULL,
  from_name      TEXT,
  to_addrs       TEXT,             -- JSON
  cc_addrs       TEXT,             -- JSON
  subject        TEXT,
  preview        TEXT,             -- 纯文本摘要
  date           INTEGER NOT NULL, -- Unix 秒
  flags          INTEGER NOT NULL DEFAULT 0, -- seen/starred/answered/draft 位图
  has_attachment INTEGER NOT NULL DEFAULT 0,
  body_kind      TEXT NOT NULL,    -- html / text / render_failed
  body_cached    TEXT,             -- 清洗后的可渲染正文
  is_encrypted   INTEGER NOT NULL DEFAULT 0,
  created_at     INTEGER NOT NULL,
  updated_at     INTEGER NOT NULL,
  UNIQUE(account_id, folder_id, remote_uid)
);
CREATE INDEX idx_msg_date ON messages(date DESC);
CREATE INDEX idx_msg_thread ON messages(thread_id);
CREATE INDEX idx_msg_from ON messages(from_addr);

CREATE TABLE attachments (
  id           INTEGER PRIMARY KEY,
  message_id   INTEGER NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
  filename     TEXT NOT NULL,
  content_type TEXT,
  size         INTEGER,
  storage_path TEXT NOT NULL,      -- 相对附件目录
  cid          TEXT                -- 内联图片引用
);

CREATE TABLE threads (
  id              INTEGER PRIMARY KEY,
  subject         TEXT,
  last_message_at INTEGER,
  message_count   INTEGER
);

CREATE TABLE labels (
  id    INTEGER PRIMARY KEY,
  name  TEXT NOT NULL UNIQUE,
  color TEXT
);
CREATE TABLE message_labels (
  message_id INTEGER NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
  label_id   INTEGER NOT NULL REFERENCES labels(id) ON DELETE CASCADE,
  PRIMARY KEY (message_id, label_id)
);

CREATE TABLE outbox (
  id          INTEGER PRIMARY KEY,
  account_id  INTEGER NOT NULL REFERENCES accounts(id),
  from_addr   TEXT NOT NULL,
  to_addrs    TEXT NOT NULL,        -- JSON
  cc_addrs    TEXT, bcc_addrs TEXT,
  subject     TEXT, body TEXT,
  attachments TEXT,                 -- JSON 路径列表
  send_at     INTEGER NOT NULL,     -- 发送时刻；撤销即删除/改回草稿
  status      TEXT NOT NULL,        -- pending/sending/sent/failed
  created_at  INTEGER NOT NULL
);
CREATE INDEX idx_outbox_send_at ON outbox(send_at) WHERE status='pending';

CREATE TABLE sync_state (
  account_id   INTEGER NOT NULL REFERENCES accounts(id),
  folder_id    INTEGER NOT NULL REFERENCES folders(id),
  uidvalidity  INTEGER,
  last_uid     INTEGER,
  last_sync_at INTEGER,
  PRIMARY KEY (account_id, folder_id)
);

CREATE TABLE index_state (
  account_id  INTEGER NOT NULL,
  folder_id   INTEGER NOT NULL,
  last_uid    INTEGER,
  indexed_at  INTEGER,
  PRIMARY KEY (account_id, folder_id)
);

CREATE TABLE privacy_rules (
  id         INTEGER PRIMARY KEY,
  domain     TEXT NOT NULL UNIQUE,
  action     TEXT NOT NULL,         -- block / allow / load_image
  scope      TEXT NOT NULL,         -- global / per_sender
  created_at INTEGER NOT NULL
);

CREATE TABLE tracked_events (
  id         INTEGER PRIMARY KEY,
  message_id INTEGER NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
  domain     TEXT NOT NULL,
  kind       TEXT NOT NULL,         -- pixel / external_link
  action     TEXT NOT NULL          -- blocked / allowed
);
```

### 4.2 设计说明
- **flags 用位图**：seen=1, answered=2, flagged=4, draft=8, deleted=16，单次 UPDATE 即可改多状态
- **body 存清洗后内容**：入库前完成 ammonia 清洗与内联图片处理，UI 只读不再解析
- **outbox 是发送中枢**：定时发送、10 秒撤销、失败重试全靠 `send_at` + `status`
- **附件与正文分离**：大文件不进 DB，DB 只存相对路径，备份时整体复制目录即可

---

## 5. 核心模块设计

### 5.1 sync（收信）
- 每个账号一个 `tokio` 任务，持连接池；IDLE 长连接 + 定时心跳
- 增量同步：按 `sync_state.last_uid` 拉新邮件；`UIDVALIDITY` 变化时全量重建该文件夹
- 弱网策略：指数退避重连；连续失败 N 次降级为轮询
- 适配层：`ProviderAdapter` trait，封装各家 IDLE / 搜索 / 文件夹命名差异
- 新邮件落库流程：`mail-parser` 解析 → MIME 拆附件 → 隐私清洗 → 写 SQLite → 更新 Tantivy → 发 `NewMessage` 事件

### 5.2 send（发信与撤销）
```
用户点发送
   ↓
写 outbox(status=pending, send_at=now+10s)   ← UI 立即显示"已发送"
   ↓
10 秒倒计时
   ├─ 用户按撤销 → 删除 outbox 记录 / 改回草稿，发 Toast
   └─ 到点 → status=sending → lettre 投递 → status=sent/failed
```
- 撤销本质是改本地 outbox，不接触远端——所以"10 秒撤销"对任何服务商都成立
- 失败进重试队列，指数退避；彻底失败保留草稿并通知用户

### 5.3 search（全文索引）
- Tantivy 索引字段：`from`、`subject`、`body`、`attachment_names`、`date`、`account_id`、`folder_id`
- 查询语法：`from:alice after:2026-11-01 has:attachment 关键词`
- 索引增量更新：sync 完成一批即更新 `index_state`；删除邮件时同步删索引
- 断网可用：索引文件全在本地，查询不触发网络

### 5.4 rules（分组与自动化）
- TOML 声明式规则优先：
```toml
[[rule]]
name = "通知归类"
match = { from_contains = ["notifications@", "noreply@"] }
action = { move_to = "通知", mark_read = false }

[[rule]]
name = "订阅归类"
match = { header_contains = { "List-Unsubscribe" = "" } }
action = { move_to = "订阅" }
```
- Rhai 作为进阶：`match_script = "rules/important.rhai"`，可写复杂条件
- 规则在入库前应用，分类结果写入 `labels` / `folders`

### 5.5 privacy（隐私报告）
- 解析阶段记录所有远程资源 URL（`<img src>`、CSS `url()`）
- 已知 tracker 域名库匹配 → 阻断并计数；未匹配的外链默认不加载
- 用户点「允许此域名」→ 写 `privacy_rules`，重渲染该邮件
- 所有决策写入 `tracked_events`，可在 UI 中回看

### 5.6 crypto（加密）
- Autocrypt：收发时在邮件头交换密钥，自动发现对方是否支持
- UI 状态必须诚实区分三态：`已端到端加密` / `对方支持但未配置` / `不支持加密`
- 支持手动导入 OpenPGP 公钥；私钥存系统 keyring
- **不承诺**自动加密所有邮件（覆盖率有限）

### 5.7 UI（iced）
- 四栏视图对应四个 `Element`，栏宽可配置（54 / 206 / 352 / flex）
- 只读 `fold-core` 的快照数据；所有命令经 `fold-ipc` 发给 core 执行
- 命令面板：命令注册表 + 模糊匹配，命令与快捷键共用同一套 action 枚举
- 正文渲染：`blitz` 画布组件 + `ammonia` 预清洗；失败降级纯文本 + "外部浏览器打开"按钮
- 主题 / 密度 / 字体为全局样式状态，从命令面板切换

### 5.8 foldctl（CLI）
```bash
foldctl list --account all --unread
foldctl search "from:alice 合同"
foldctl reply <id> --body "收到"
foldctl export <id> --format eml -o ./mail.eml
```
- 与桌面端共用 `fold-core`，直接读写同一本地库（文件锁协调）

---

## 6. 关键流程时序

### 6.1 打开邮件（读路径，必须快）
```
UI 点击 → 从 SQLite 读 body_cached → 立即渲染（<100ms）
        → 异步发 MarkSeen 命令 → core 更新 flags → 广播事件 → UI 刷新列表
```

### 6.2 新邮件到达（写路径）
```
IDLE 推送 → 拉 UID → 解析 MIME → 抽附件写文件
    → 隐私清洗 + 记录 tracked_events
    → 应用 rules 分类
    → 事务写 SQLite（messages/attachments/labels）
    → 更新 Tantivy 索引
    → 广播 NewMessage → UI 增量刷新列表
```

### 6.3 搜索
```
输入 → 命令面板/搜索框 → Tantivy 查询 → 结果 ID 列表
     → SQLite 取摘要 → 渲染（全程无网络）
```

---

## 7. 并发与线程模型

| 组件 | 模型 |
|---|---|
| UI | 单线程事件循环（iced），不阻塞 |
| core | `tokio` 多线程 runtime |
| 每账号 sync | 独立任务 + 连接池；互不阻塞 |
| send | 独立调度任务，扫描 `outbox` 待发队列 |
| DB | `rusqlite` 连接放 `tokio::task::spawn_blocking`，或每任务独立连接 + WAL |
| 事件 | `tokio::sync::broadcast<DomainEvent>`，UI 侧订阅 |

SQLite 配置：`PRAGMA journal_mode=WAL`、`synchronous=NORMAL`、`busy_timeout=5000`。

---

## 8. 错误处理与日志

- 内部错误用 `thiserror` 定义枚举（`StoreError`、`SyncError`、`SendError`、`RenderError`）
- 边界 / CLI 用 `anyhow` 聚合
- 所有错误携带上下文（`with_context`），用户看到的是可行动提示而非原始报错
- `tracing` 分层：`ERROR` 落文件、`INFO` 关键流程、`DEBUG` 协议细节（可开关）
- 同步失败不丢数据：原始邮件先落临时表，成功后再提交

---

## 9. 安全设计

| 项 | 措施 |
|---|---|
| 凭据 | `keyring` 系统级存储；不可用时拒绝保存，不落明文 |
| 正文 | `ammonia` 清洗 + 沙箱渲染；`<script>`、表单、事件属性、危险 URL 剥离 |
| 远程图片 | 默认阻断；允许列表存 `privacy_rules` |
| 本地数据 | 文件权限 0600；目录 0700 |
| 传输 | 全链路 TLS（rustls）；OAuth2 走系统浏览器 |
| 渲染进程 | 不用 WebView，无 JS 执行环境（若引入 iced 画布亦无脚本能力） |

---

## 10. 配置与目录（XDG）

```
$XDG_CONFIG_HOME/fold/
  ├── config.toml          # 全局设置（主题、密度、字体）
  ├── accounts.toml        # 账号列表（不含凭据）
  └── rules/               # TOML 规则 + Rhai 脚本
$XDG_DATA_HOME/fold/
  ├── fold.sqlite          # 主库
  ├── attachments/         # 附件文件
  └── index/               # Tantivy 索引
$XDG_STATE_HOME/fold/
  └── logs/                # 运行日志
```

macOS 用 `~/Library/Application Support/fold` 等对应路径。

---

## 11. 测试策略

| 层 | 方法 |
|---|---|
| 单元 | 解析器、规则引擎、隐私清洗的纯函数测试 |
| 协议回归 | `wiremock` 模拟 IMAP/SMTP 服务器，快照断言交互序列（`insta`） |
| 渲染 | 100 封真实 HTML 邮件样本，验证 `blitz` 成功率与纯文本回退 |
| 搜索 | 10 万～100 万封样本压测，验证 <500ms |
| 端到端 | 两个 Fold 实例互发，验证 Autocrypt 建立与 10 秒撤销 |
| UI | 快照测试 + 键盘流自动化（参考原型验证方式） |

---

## 12. 构建与发布

- `cargo-dist` 生成 macOS（dmg/arm64+x64）、Windows（msi/exe）、Linux（deb/rpm/AppImage）
- CI：fmt / clippy / test / 依赖审计（`cargo audit`）
- 目标：可复现构建（`-Z build-std` + 固定工具链）
- 版本号跟随里程碑（M1=0.1, M5=1.0）

---

## 13. 性能目标与验证

| 指标 | 目标 | 验证方式 |
|---|---|---|
| 冷启动 | < 1s | 计时脚本 |
| 打开缓存邮件 | < 100ms | UI 埋点 |
| 搜索（100 万封） | < 500ms | Tantivy benchmark |
| 空闲内存 | < 150MB | 进程采样 |
| 同步 1 万封增量 | < 30s | wiremock 压测 |

---

## 14. 风险与技术验证（M0 必须做）

1. **iced 成熟度**：复杂列表滚动、富文本选区是否要自己补。写一个原型验证；若不行评估 GPUI。
2. **blitz 渲染真实邮件**：这是最大技术风险。用 100 封真实邮件（含 QQ / Gmail / 新闻邮件）测试，统计失败率；必须准备纯文本回退和"外部浏览器打开"出口。
3. **IMAP 现实现状**：各家 IDLE、搜索、文件夹命名偏差大；先做 Gmail / QQ / 163 三家适配。
4. **Autocrypt 兼容**：覆盖率有限，UI 文案要诚实；不承诺自动加密所有邮件。

> 决策门：M0 两项预研任一项不通过，必须在写 M1 代码前重新评估技术选型。

---

## 15. 里程碑与技术交付对应

| 里程碑 | 技术交付 |
|---|---|
| M0 | iced / blitz 预研结论；workspace 骨架 |
| M1 | `fold-core` 存储层；IMAP 增量同步；只读 UI（四栏 + SVG 图标） |
| M2 | SMTP 发送；outbox + 撤销；草稿；会话；附件；账号向导 |
| M3 | Tantivy 搜索；规则引擎；隐私报告；命令面板；`foldctl` |
| M4 | Autocrypt / OpenPGP；快捷键全覆盖；性能与无障碍达标 |
| M5 | `cargo-dist` 打包；autoconfig 覆盖；文档与截图 |

---

## 附录：与 UI 原型的对应

原型 `designs/fold-fusion.html`（Aerion 风格）已经确定的界面约束，技术实现必须满足：
- 四栏布局；**无独立顶栏、无底部常驻工具条**
- 搜索在列表栏顶部、写邮件在左栏、命令/主题在图标栏
- 图标全部 SVG；深色默认 + 紫罗兰强调
- 隐私状态条可展开、域名可允许
- 行尾星标可切换
- 密度与字体切换通过命令面板（不设常驻 UI）
