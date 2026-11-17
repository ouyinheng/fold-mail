# Fold · 折页

> 本地优先、无 WebView、可脚本化的安静邮件工作台。

一个用 Rust 写的桌面邮件客户端，给开发者和内容重度用户。不做"又一个 Thunderbird 替代品"，而是打开就能用的小工具。

## 项目状态

**当前阶段：设计 / 文档阶段**（尚未进入代码实现）

| 目录 | 内容 |
|---|---|
| `_docs/` | 产品与技术文档 |
| `designs/` | 可交互 HTML UI 原型 |

## 文档

- [头脑风暴](./_docs/fold-mail-brainstorm.md) — 产品定位、功能设想、技术选型、路线图
- [产品需求文档 PRD](./_docs/fold-mail-prd.md) — 用户画像、UI 规范、功能需求（FR-01～62）、里程碑
- [Rust 客户端技术设计](./_docs/fold-mail-tech-design.md) — 架构、数据模型、核心模块、风险与验证

## UI 原型

打开 `designs/fold-fusion.html` 查看当前确定的设计（Aerion 风格：深色板岩 + 紫罗兰强调，四栏布局，全 SVG 图标）：

- 左侧图标栏 / 账号文件夹栏 / 会话列表 / 正文
- 命令面板 `⌘K`、快捷键 `j/k` `e` `s` `u` `r/a/f`
- 隐私报告、附件预览、快速回复
- 明暗主题切换

其余原型（`fold-foxmail.html`、`fold-style-gmail.html`）为设计过程中的参考版本。

## 核心原则

1. **本地优先**：已同步邮件、附件、索引全在本机，断网可读可搜可写草稿
2. **无 WebView**：正文用 Rust 渲染器绘制，脚本不执行，远程图片默认不加载
3. **键盘优先，但不劝退鼠标**：快捷键全覆盖，每个操作也有可见入口
4. **可脚本化**：规则、模板、自动分类用配置 + 小脚本表达
5. **v1 只做邮件**：日历、联系人、网盘往后放

## 技术栈（规划）

Rust 2024 · tokio · async-imap · lettre · mail-parser · rusqlite · Tantivy · blitz + ammonia · Sequoia/Autocrypt · iced（待 M0 验证）· TOML + XDG

详见 [技术设计文档](./_docs/fold-mail-tech-design.md)。

## 路线图

M0 技术验证 → M1 骨架 → M2 收发 → M3 差异化 → M4 加密与打磨 → M5 发布

里程碑详情见 [PRD](./_docs/fold-mail-prd.md#6-里程碑)。
