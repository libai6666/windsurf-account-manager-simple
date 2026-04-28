# HAR 样本目录

## 用途

这个目录放**真实浏览器操作**抓到的 HAR (HTTP Archive) 文件，用于跟我们 Rust 协议代码做字段级对比。

## 当前需要的样本

### `browser-bind-success.har`

**关键**：浏览器手动**绑卡成功**走完全流程的 HAR。

抓取步骤见 [`../protocol-bind-card-risk-audit.md`](../protocol-bind-card-risk-audit.md) § 5。

简要:

1. 在 Windsurf Account Manager 里生成新的试用链接（拿 `cs_live_...` URL）
2. Chrome/Edge 打开 → 按 F12 → Network → 勾 `Preserve log` + `Disable cache`
3. 手动填卡 + 地址 → 点 Pay → 等跳 `/subscription-pending`
4. 右键请求列表 → "Save all as HAR with content"
5. 保存为本目录下 `browser-bind-success.har`

## 隐私 / 安全警告

**HAR 文件里会包含**:
- 卡号、CVC、有效期（以明文出现在 `/v1/payment_methods` 请求体）
- Windsurf 认证 token (`auth1_...`, `devin-session-token$...`)
- 账号邮箱、姓名、地址
- 各种 cookie、session id

**不要** commit 这个 har 文件到 git 仓库公开发布。本项目的 `.gitignore` 已默认忽略 `docs/samples/*.har`。

给我（AI assistant）使用时，我会：
- 只提取**字段结构**（field name / 格式 / 长度分布）
- 不会把卡号/token 明文复制到诊断文档里
- 对比完后建议你**删除或归档**这个 har

## 如果怕卡号泄露，可以选择先脱敏

用这条 PowerShell 在发给我之前把卡号 mask 掉（4242… 只保留前 6 后 4）:

```powershell
$har = Get-Content .\browser-bind-success.har -Raw
# 把 card[number] 字段替换成脱敏版本（保留前 6 后 4）
$har = $har -replace '(card\[number\]=)(\d{6})\d+(\d{4})', '$1$2******$3'
# 把 card[cvc] 字段去掉
$har = $har -replace 'card\[cvc\]=\d+', 'card[cvc]=***'
Set-Content .\browser-bind-success-redacted.har $har
```

然后只发 `browser-bind-success-redacted.har` 给我即可。这个不影响字段级 diff 的分析质量。
