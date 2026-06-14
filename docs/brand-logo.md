# Scalpel Logo Design

本文记录 Scalpel 的 logo 设计理念与使用建议。当前品牌资产包括：

- [`docs/ui/scalpel-logo.png`](ui/scalpel-logo.png)：当前横版主 logo，用于 README、发布页、About 截图。
- [`docs/ui/scalpel-mark.png`](ui/scalpel-mark.png)：当前纯图形 mark，用于 app icon 和小尺寸场景参考。
- [`crates/pdbg-app/assets/icons/scalpel-mark.png`](../crates/pdbg-app/assets/icons/scalpel-mark.png)：带 macOS 视觉安全边距的透明背景 app icon，用于 native 窗口图标和后续打包。

## 品牌定位

Scalpel 的中文语义是“PDF 手术刀”：它不是普通阅读器，也不是编辑器，
而是面向工程师的 PDF 结构解剖工具。品牌视觉需要表达三件事：

- 精准：定位对象、流、xref、资源和渲染异常。
- 安全：在受控边界内打开复杂或损坏的 PDF。
- 结构化：把 PDF 从“页面外观”切开，看到内部对象关系。

## 图形概念

Logo 由三层隐喻组成，当前 PNG 资产直接采用参考稿里的“大手术刀斜切 PDF 页面 + 右侧字标”：

- 手术刀：青绿色长柄与银色弧形刀片是第一视觉，表示精细、精准、可控的结构解剖。
- PDF 页面：白色文档轮廓和折角退为低对比背景，代表被分析的 PDF 文件。
- 结构剖面：蓝色细虚线和小节点代表对象引用、内容流、xref 和资源依赖。

图形中的手术刀从页面左下切向右上，覆盖并切入蓝色结构线，暗示从视觉页面
精确进入底层对象结构。页面和节点都应服务于刀具轮廓，不应抢占主体；
小尺寸下也应优先保留刀柄、弧形刀片轮廓和一组蓝色结构节点。
这比单纯使用“PDF 文件图标”更能区分 Scalpel 的调试器属性。

## 字标

字标使用 `Scalpel`，不再使用 `pdbg` 或 `PDF Inspector` 作为用户可见品牌。
`pdbg-app` 仍作为内部 crate 名称保留；用户可见 binary / release 产物使用 `scalpel` / `Scalpel`。

推荐英文说明语：

```text
A PDF scalpel for dissecting document structure
```

推荐中文说明语：

```text
PDF 手术刀，用于解剖 PDF 的内部结构
```

## 色彩

Logo 使用当前 App 调色板中的核心色，保证 UI 与品牌一致：

| 用途 | 色值 | 含义 |
| --- | --- | --- |
| 主色 | `#087F8C` | 精准、工具感、可点击操作 |
| 高光 | `#48B8C4` | 刀身高光、技术感 |
| 刀片 | `#DBE7EE` | 精细、锋利、克制的手术刀轮廓 |
| 刀锋高光 | `#FFFFFF` | 精准切入点，强化锋利感 |
| 结构强调 | `#3B82F6` | 被剖开的对象节点和诊断线索 |
| 正文深色 | `#1F2933` | 工程工具的稳定感 |
| 页面白 | `#FFFDF8` | PDF 页面本体 |
| 背景灰 | `#E9EDF2` | App 画布背景 |

## 使用建议

- 顶栏品牌只使用文字 `Scalpel`，不要把完整 logo 放入工具栏，避免挤占操作空间。
- About、README、发布页和下载页优先使用 `scalpel-logo.png`，确保和参考稿一致。
- App icon 使用带透明安全边距的 `crates/pdbg-app/assets/icons/scalpel-mark.png`，不包含字标，避免在 macOS Dock 中显得过大。
- 小尺寸场景只保留“手术刀 + 页面背景 + 蓝色结构节点”，不增加标签 chip 或说明文字。
- 需要无损缩放、换色或单色版本时，再基于当前 PNG 方向重绘正式矢量资产。

## 不建议

- 不使用医疗红十字，避免把工具误解为医疗软件。
- 不使用血迹、刀痕等具象医疗元素；结构节点使用蓝色，避免被误读为血迹。
- 不把 PDF 字样作为主视觉，避免和普通 PDF 阅读器混淆。
- 不使用过度圆润或卡通化的图形，避免削弱诊断工具的专业感。

## 后续落地

后续如果正式更名为 Scalpel，可继续完成以下工作：

- 基于 `scalpel-mark.png` 重绘正式矢量 mark，并生成 `scalpel-icon.icns`、`scalpel-icon.ico`。
- 更新 release workflow 的 artifact 名称。
- 更新 README 标题和截图。
- 评估是否将内部 crate 从 `pdbg-app` 改名为 `scalpel`。
- 在 About 的 GitHub 链接附近加入 license / MuPDF attribution 信息。
