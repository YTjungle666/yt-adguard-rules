# yt-adguard-rules

YT 的 AdGuard Home 聚合规则仓库。

## 这是什么
这个仓库不是单纯手写规则，而是一个**聚合产物仓库**：

- 固定抓取当前选定的上游规则源
- 合并我自己的自定义黑白名单
- 自动去重
- 自动消除黑白冲突（白名单优先）
- 生成最终可直接导入 AdGuard Home 的规则文件

---

## 仓库文件说明

### 给 AdGuard Home 直接导入的成品文件
#### `blocklist.txt`
最终黑名单。

#### `allowlist.txt`
最终白名单。

> AdGuard Home 只需要导入这两个文件。

---

### 作为生成输入的源文件
#### `custom-blocklist.txt`
手工维护的自定义拦截规则源，也是本地自动整合时的**权威源文件**之一。

#### `custom-allowlist.txt`
手工维护的自定义放通规则源，也是本地自动整合时的**权威源文件**之一。

> 这两个文件中的规则，都会自动并入最终的 `blocklist.txt` / `allowlist.txt`。
> 它们是“源文件”，不是 AGH 最终订阅文件。

#### `sources.json`
固定上游规则源清单，也是本地自动整合时的**权威源文件**之一。

脚本每天会从这里记录的固定链接抓取规则，而不是去读取 AdGuard Home 当前启用的订阅项。

这意味着：

- 即使 AGH 前端最后只保留我自己的 `blocklist.txt` / `allowlist.txt`
- 每日自动整合仍然会正常工作
- 不会因为前端切换成自己的规则集而失效或递归套娃

---

## 当前工作方式
生成逻辑是：

```text
固定上游规则源
+ custom-blocklist.txt
+ custom-allowlist.txt
= 合并去重
= 白名单优先剔除黑名单冲突
= 生成 blocklist.txt / allowlist.txt
```

---

## AdGuard Home 导入链接
### 黑名单
`https://raw.githubusercontent.com/YTjungle666/yt-adguard-rules/main/blocklist.txt`

### 白名单
`https://raw.githubusercontent.com/YTjungle666/yt-adguard-rules/main/allowlist.txt`

---

## 自动更新
当前已配置每日中午 12:00 自动更新。

> 运行时以本地仓库中的 `sources.json`、`custom-blocklist.txt`、`custom-allowlist.txt` 为准；GitHub 用于发布、留档和对外提供 raw 导入链接，不作为运行时动态配置源。

更新流程：
1. 从 `sources.json` 里的固定上游源拉取最新规则
2. 合并 `custom-blocklist.txt` / `custom-allowlist.txt`
3. 去重并清理黑白冲突
4. 自动提交到 GitHub

---

## 规则冲突处理原则
- 白名单优先
- 如果某个域名同时出现在黑名单和白名单中，最终会从黑名单移除
- 目标是确保最终 `allowlist.txt` 中的内容不会在最终 `blocklist.txt` 中重复出现

---

## 使用建议
在 AdGuard Home 中：

- 导入 `blocklist.txt`
- 导入 `allowlist.txt`
- 验证没问题后，再移除原始上游规则源

不建议直接把 `custom-blocklist.txt` / `custom-allowlist.txt` 作为 AGH 主订阅文件。

---

## 备注
如果后续要新增或删除上游规则源，应修改：
- `sources.json`

如果后续要手工新增兼容性白名单或广告拦截规则，应修改：
- `custom-blocklist.txt`
- `custom-allowlist.txt`
