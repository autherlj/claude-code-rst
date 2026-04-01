当然，下面是这篇内容的中文翻译。我会尽量保持原文的语气、结构和信息量。

---

# 重要说明 - 更新

这个仓库**并不包含** Claude Code 专有 TypeScript 源代码的副本。
它是一个对 Claude Code 行为进行**洁净室（clean-room）Rust 重实现**的项目。

整个过程被明确分成两个阶段：

- **规格说明 [`spec/`](https://github.com/kuberwastaken/claude-code/tree/main/spec)**
  一个 AI 代理分析了源码，并产出了详尽的行为规格和改进说明，包括架构、数据流、工具契约、系统设计等。**没有任何源代码**被带入下一阶段。

- **实现 [`src-rust/`](https://github.com/kuberwastaken/claude-code/tree/main/src-rust)**
  另一个独立的 AI 代理仅依据规格进行实现，**从未参考原始 TypeScript**。最终产物是符合 Rust 习惯用法的代码，复现的是其**行为**，而非其**表达方式**。

这与 **Phoenix Technologies v. IBM (1984)** 所确立的 BIOS 洁净室工程法律先例一致，也符合 **Baker v. Selden (1879)** 所体现的原则：版权保护的是**表达**，不是**思想或行为**。

下面的分析是对公开可得软件的评论，受美国《合理使用》原则（17 U.S.C. § 107）保护。文中的代码摘录仅用于说明公开来源中的技术要点——整个过程没有涉及任何未授权访问。

---

# Claude Code 的完整源码因为 npm 里的 sourcemap 泄露了，我们来聊聊这件事

## 技术拆解

> **PS：** 我也把这篇分析发到了我的[博客](https://kuber.studio/blog/AI/Claude-Code's-Entire-Source-Code-Got-Leaked-via-a-Sourcemap-in-npm,-Let's-Talk-About-it)，阅读体验和界面会更好一点 :)

今天稍早（2026 年 3 月 31 日），Chaofan Shou 在 X 上发现了一件 Anthropic 大概绝不想让外界看到的事：
**Claude Code 的完整源代码**，Anthropic 官方的 AI 编程 CLI，竟然因为一个随 npm 包一起发布的 **sourcemap 文件**，堂而皇之地暴露在了 npm 注册表上。

这个仓库是那份泄露源码的一个备份，而这篇 README 则是对其中内容的完整技术解析：泄露是怎么发生的、里面都有什么，以及更重要的——我们因此知道了哪些原本绝不该公开的信息。

开始吧。

---

## 这到底是怎么发生的？

说实话，看到这里我的第一反应是：“……认真的吗？”

当你发布一个 JavaScript/TypeScript 包到 npm 时，构建工具链通常会生成 **source map 文件**（`.map` 文件）。这些文件的作用，是在生产环境代码与原始源码之间建立映射关系——这样当线上报错时，栈追踪就能指向**原始文件中的真实行号**，而不是压缩后某个不可读的“第 1 行，第 48293 列”。

但“精彩”的地方在于：**source map 里包含原始源代码**。
不是“某种引用”，不是“部分信息”，而是**完整、原样、逐文件嵌入的源码字符串**。

一个 `.map` 文件的结构大致如下：

```json
{
  "version": 3,
  "sources": ["../src/main.tsx", "../src/tools/BashTool.ts", "..."],
  "sourcesContent": ["// 每个文件的完整原始源码", "..."],
  "mappings": "AAAA,SAAS,OAAO..."
}
```

那个 `sourcesContent` 数组？就是全部内容。
每个文件。每条注释。每个内部常量。每段系统提示词。所有东西，全都在一个 npm 可以直接提供的 JSON 文件里。任何人只要运行 `npm pack`，甚至只是浏览包内容，就能拿到。

这并不是什么新型攻击手法。以前发生过，以后还会发生。

问题几乎总是一样的：有人忘了把 `*.map` 加进 `.npmignore`，或者没有配置 bundler 在生产构建时关闭 source map 生成。而 Claude Code 使用的 Bun bundler，默认就是会生成 sourcemap，除非你**显式关闭**。

最搞笑的是，代码里甚至还有一个完整的系统，叫 **“Undercover Mode（卧底模式）”**，专门用来防止 Anthropic 的内部信息泄露。

他们专门做了一整套机制，防止 AI 在 git commit 里不小心泄露内部代号……结果却通过一个 `.map` 文件把**整个源码**发了出去，而且大概率还是 Claude 自己打包发的。

---

## Claude 在底层到底长什么样？

如果你最近没关注，Claude Code 是 Anthropic 官方推出的 CLI 工具，也是目前最流行的 AI 编码代理之一。

从外表看，它像是一个打磨得不错、结构相对简单的终端工具。

从内部看，它实际上包含：

- 一个 **785KB 的 `main.tsx` 入口文件**
- 一个自定义的 React 终端渲染器
- 40+ 个工具
- 一个多代理编排系统
- 一个叫 **dream** 的后台记忆整合引擎
- 以及更多内容

少说废话，下面是我花了一下午深挖后发现的一些**真的很有意思**的部分。

---

## BUDDY —— 终端里的电子宠物

我不是在开玩笑。

Claude Code 里真的有一整套 **Tamagotchi 风格的伙伴宠物系统**，名字叫 **Buddy**。
它包含一个**确定性的扭蛋系统**，有物种稀有度、闪光变体、程序生成属性，甚至还有在首次孵化时由 Claude 写出的“灵魂描述”，有点像 OpenClaw。

整个系统在 `buddy/` 目录下，由 `BUDDY` 编译期开关控制。

### 扭蛋系统

你的 buddy 物种由一个 **Mulberry32 伪随机数生成器**决定。它是一个快速的 32 位 PRNG，种子来自你的 `userId` 哈希，再加上盐值 `'friend-2026-401'`：

```typescript
// Mulberry32 PRNG - 对每个用户都是确定且可复现的
function mulberry32(seed: number): () => number {
  return function() {
    seed |= 0; seed = seed + 0x6D2B79F5 | 0;
    var t = Math.imul(seed ^ seed >>> 15, 1 | seed);
    t = t + Math.imul(t ^ t >>> 7, 61 | t) ^ t;
    return ((t ^ t >>> 14) >>> 0) / 4294967296;
  }
}
```

也就是说，**同一个用户永远会得到同一只 buddy**。

### 18 个物种（在代码里做了混淆）

这些物种名称被用 `String.fromCharCode()` 数组隐藏了起来——Anthropic 显然不想让人通过字符串搜索轻易看到它们。解码后，完整列表如下：

| 稀有度 | 物种 |
|--------|------|
| **普通**（60%） | Pebblecrab、Dustbunny、Mossfrog、Twigling、Dewdrop、Puddlefish |
| **不常见**（25%） | Cloudferret、Gustowl、Bramblebear、Thornfox |
| **稀有**（10%） | Crystaldrake、Deepstag、Lavapup |
| **史诗**（4%） | Stormwyrm、Voidcat、Aetherling |
| **传说**（1%） | Cosmoshale、Nebulynx |

除此之外，还有一个**独立于稀有度之外的 1% 闪光概率**。
所以一只“闪光传说级 Nebulynx”的概率是 **0.01%**。真离谱。

### 属性、眼睛、帽子和灵魂

每只 buddy 都会程序生成：

- **5 项属性**：`DEBUGGING`、`PATIENCE`、`CHAOS`、`WISDOM`、`SNARK`（各自 0–100）
- **6 种眼睛样式**
- **8 种帽子选项**（部分受稀有度限制）
- **一个“灵魂”描述**：首次孵化时由 Claude 生成的性格文本，带角色风格

它们的精灵图用 **ASCII 艺术**渲染，高 5 行、宽 12 字符，并且有多帧动画。
包括待机动画、反应动画，而且会坐在你的输入提示符旁边。

### 世界观设定

代码中提到 **2026 年 4 月 1 日到 7 日**是一个“预热窗口”，而正式上线则被设定在 **2026 年 5 月**。

这个伙伴系统还配有一段系统提示，告诉 Claude：

> 一只名叫 {name} 的小 {species} 坐在用户输入框旁边，偶尔会在气泡里发表评论。你不是 {name}——它是一个独立的观察者。

所以它不只是装饰——这个 buddy 有自己的“人格”，并且在用户直接叫它名字时还会作出回应。
说真的，我希望他们真把这个功能发出来。

---

## KAIROS —— “常驻在线的 Claude”

在 `assistant/` 目录中，有一个完整的模式叫 **KAIROS**。
本质上，它是一个**持续运行、无需等你输入才工作的 Claude 助手**。它会观察、记录，并且对它注意到的事情**主动采取行动**。

这个功能由 `PROACTIVE` / `KAIROS` 编译期开关控制，在外部发布版本中完全不存在。

### 它是怎么工作的

KAIROS 会维护**按天追加写入的日志文件**——它会在一天中持续记录观察、决策和动作。

在固定时间间隔上，它会收到一种 `<tick>` 提示，让它决定是否需要主动行动，或者保持安静。

这个系统还有一个 **15 秒阻塞预算**：
如果某个主动动作会阻塞用户工作流超过 15 秒，它就会被延后执行。

这是 Claude 在努力做到“有帮助，但不烦人”。

### Brief 模式

当 KAIROS 启用时，会有一种特殊输出模式叫 **Brief**。
这是一个**极度简洁**的输出风格，专为“常驻型助手”设计，避免往终端里刷太多文字。

你可以把它理解为：不是一个话痨朋友，而是一个只有在有价值时才发言的专业助理。

### 专属工具

KAIROS 拥有普通 Claude Code 没有的工具：

| 工具 | 作用 |
|------|------|
| **SendUserFile** | 直接把文件发送给用户（通知、摘要等） |
| **PushNotification** | 向用户设备发送推送通知 |
| **SubscribePR** | 订阅并监控 Pull Request 活动 |

---

## ULTRAPLAN —— 长达 30 分钟的远程规划会话

这个从基础设施角度看很疯狂。

**ULTRAPLAN** 是一种模式：Claude Code 会把复杂的规划任务卸载到一个**远程 Cloud Container Runtime（CCR）会话**，用 **Opus 4.6** 跑，给它最多 **30 分钟**去思考，然后让你在浏览器里审批结果。

基本流程如下：

1. Claude Code 判断某个任务需要深度规划
2. 它通过 `tengu_ultraplan_model` 配置启动一个远程 CCR 会话
3. 你的终端进入轮询状态——每 **3 秒**检查一次结果
4. 同时，一个浏览器界面允许你观察整个规划过程，并批准或拒绝结果
5. 一旦批准，会使用一个特殊哨兵值 `__ULTRAPLAN_TELEPORT_LOCAL__` 将结果“传送”回本地终端

---

## “Dream” 系统 —— Claude 真的会“做梦”

这个真的是我觉得最酷的部分之一。

Claude Code 有一套叫 **autoDream** 的系统，位于 `services/autoDream/`。
它是一个后台记忆整合引擎，以**派生子代理**的形式运行。这个命名显然是故意的。Claude……真的在“做梦”。

这件事有趣到离谱，因为我上周给 LITMUS 想的也是类似思路——让 OpenClaw 子代理在空闲时做些“休闲研究”，挖掘有趣的新论文。

### 三重触发门槛

这个 dream 并不会随便运行。它有一个**三重门槛机制**：

1. **时间门槛**：距离上次 dream 至少 24 小时
2. **会话门槛**：自上次 dream 后至少发生了 5 次会话
3. **锁门槛**：成功获取 consolidation lock（防止并发 dream）

三者必须全部满足。
这样可以防止“做梦过多”或者“长期不做梦”。

### 四个阶段

当它运行时，会严格按照 `consolidationPrompt.ts` 中定义的四阶段流程：

**阶段 1 - 定向（Orient）**
执行 `ls` 查看记忆目录，读取 `MEMORY.md`，浏览已有主题文件，以了解和改进现有记忆结构。

**阶段 2 - 收集近期信号（Gather Recent Signal）**
寻找值得长期保留的新信息。优先级来源依次为：日常日志 → 漂移记忆 → transcript 搜索。

**阶段 3 - 整合（Consolidate）**
写入或更新记忆文件。把相对日期转换成绝对日期。删除被新事实推翻的旧内容。

**阶段 4 - 修剪与索引（Prune and Index）**
保持 `MEMORY.md` 不超过 200 行且约 25KB。移除陈旧指针。解决矛盾信息。

提示词里甚至直接写着：

> “你正在执行一次 dream——对记忆文件进行反思式整理。把你最近学到的内容综合成持久、结构化的记忆，以便未来会话能更快完成定向。”

这个 dream 子代理只拥有**只读 bash** 权限——它可以查看你的项目，但不能修改任何内容。它的唯一职责就是进行记忆整合。

---

## Undercover Mode —— “不要暴露你的身份”

这个从企业策略角度看非常有意思。

Anthropic 员工（由 `USER_TYPE === 'ant'` 标识）会用 Claude Code 在公共 / 开源仓库中工作。
**Undercover Mode（卧底模式）** 的存在，就是为了防止 AI 在 commit 和 PR 中意外暴露 Anthropic 内部信息。

开启后，它会向系统提示中注入如下内容：

```text
## UNDERCOVER MODE - CRITICAL

你正在一个公开 / 开源仓库中以“卧底”方式工作。你的 commit message、
PR 标题和 PR 内容绝不能包含任何 Anthropic 内部信息。不要暴露身份。

在 commit message 或 PR 描述中绝对不要包含：
- 内部模型代号（例如 Capybara、Tengu 等动物名）
- 未发布的模型版本号（例如 opus-4-7、sonnet-4-8）
- 内部仓库或项目名称
- 内部工具、Slack 频道或短链接（例如 go/cc、#claude-code-…）
- “Claude Code” 这个词，或任何表明你是 AI 的描述
- Co-Authored-By 行或其他任何归属信息
```

### 激活逻辑

- `CLAUDE_CODE_UNDERCOVER=1` 会**强制开启**
- 否则是**自动启用**：只要仓库远程地址**不匹配内部 allowlist**，就会开启
- 而且**没有强制关闭选项**
  ——“如果我们不能确定自己处于内部仓库，就保持卧底状态”

所以这基本确认了几件事：

1. **Anthropic 员工确实在积极使用 Claude Code 向开源仓库提交贡献**
2. AI 被明确要求**隐藏自己是 AI 的事实**
3. **内部模型代号确实是动物名**——Capybara、Tengu 等
4. **“Tengu”** 在代码中高频出现，作为功能开关和分析事件前缀，几乎可以确定是 **Claude Code 的内部项目代号**

这些内容在外部构建里都会被死代码消除掉。
但 sourcemap 可不会在乎什么 dead code elimination。

说实话，这让我很好奇：他们到底已经在多少开源仓库里“制造影响”了。

---

## 多代理编排 —— “Coordinator Mode”

Claude Code 还内置了一套完整的**多代理编排系统**，位于 `coordinator/`，通过环境变量 `CLAUDE_CODE_COORDINATOR_MODE=1` 激活。

开启后，Claude Code 会从单一代理变成一个**协调者**，负责并行地生成、指挥和管理多个 worker 代理。

`coordinatorMode.ts` 中的系统提示，几乎可以算是一份多代理设计范本：

| 阶段 | 执行者 | 目的 |
|------|--------|------|
| **Research** | Worker（并行） | 调查代码库、查找文件、理解问题 |
| **Synthesis** | **Coordinator** | 阅读结果、理解问题、制定规格 |
| **Implementation** | Worker | 根据规格进行有针对性的修改并提交 |
| **Verification** | Worker | 测试修改是否有效 |

这个提示词**明确教授并行性**：

> “并行性是你的超能力。Worker 是异步的。只要任务彼此独立，就应尽可能并发启动，不要把能并行的工作串行化。”

Worker 之间通过 `<task-notification>` 形式的 XML 消息通信。
还有一个共享的 **scratchpad 目录**（由 `tengu_scratch` 控制），用于跨 worker 存储可持久共享的知识。

提示词里还有一句很精彩的话，用来禁止偷懒式委派：

> 不要说“根据你的发现”——要真正去读这些发现，并明确指定该做什么。

系统还包含 **Agent Teams / Swarm** 能力（由 `tengu_amber_flint` 控制）：
包括用 `AsyncLocalStorage` 做上下文隔离的进程内队友、基于 tmux/iTerm2 窗格的进程型队友、团队记忆同步，以及用于视觉区分的颜色分配。

---

## Fast Mode 在内部叫“Penguin Mode”

是的，他们真的管它叫企鹅模式。

`utils/fastMode.ts` 里的 API endpoint 甚至直接写成了：

```typescript
const endpoint = `${getOauthConfig().BASE_API_URL}/api/claude_code_penguin_mode`
```

配置键叫 `penguinModeOrgEnabled`。
关闭开关叫 `tengu_penguins_off`。
失败分析事件叫 `tengu_org_penguin_mode_fetch_failed`。

真的是，从头到尾都是企鹅。

---

## 系统提示架构

Claude Code 的系统提示并不是多数应用那种“一整块字符串”。

它是由 `constants/` 下多个**模块化、可缓存的段落**在运行时拼接而成的。

架构中用了一个叫 `SYSTEM_PROMPT_DYNAMIC_BOUNDARY` 的标记，把提示词拆分成：

- **静态部分**：可在组织之间共享缓存，不会因用户变化而变化
- **动态部分**：与用户 / 会话相关，只要改变就会打破缓存

还有个函数叫 `DANGEROUS_uncachedSystemPromptSection()`，专门用于那些你**明确希望打破缓存**的易变段落。

光这个命名风格，就能看出有人在这件事上吃过亏。

### 网络安全风险指令

其中一个特别有意思的部分，是 `CYBER_RISK_INSTRUCTION`，文件头有一段非常醒目的警告：

```text
IMPORTANT: DO NOT MODIFY THIS INSTRUCTION WITHOUT SAFEGUARDS TEAM REVIEW
This instruction is owned by the Safeguards team (David Forsythe, Kyla Guru)
```

所以现在我们知道了：

- Anthropic 内部有一个 **Safeguards 团队**
- 这个团队负责安全边界相关决策
- 具体负责人名字都直接写在代码里

而这段指令本身也划定了清晰边界：
授权的安全测试是允许的，但破坏性技术和供应链攻击是不允许的。

---

## 完整工具注册表 —— 40+ 个工具

Claude Code 的工具系统位于 `tools/`。完整列表如下：

| 工具 | 作用 |
|------|------|
| **AgentTool** | 生成子代理 / 子智能体 |
| **BashTool** / **PowerShellTool** | Shell 执行（可选沙箱） |
| **FileReadTool** / **FileEditTool** / **FileWriteTool** | 文件操作 |
| **GlobTool** / **GrepTool** | 文件搜索（可优先使用原生 `bfs` / `ugrep`） |
| **WebFetchTool** / **WebSearchTool** / **WebBrowserTool** | Web 访问 |
| **NotebookEditTool** | Jupyter Notebook 编辑 |
| **SkillTool** | 调用用户定义技能 |
| **REPLTool** | 交互式 VM shell（裸模式） |
| **LSPTool** | Language Server Protocol 通信 |
| **AskUserQuestionTool** | 向用户提问 |
| **EnterPlanModeTool** / **ExitPlanModeV2Tool** | 规划模式控制 |
| **BriefTool** | 上传 / 摘要文件到 claude.ai |
| **SendMessageTool** / **TeamCreateTool** / **TeamDeleteTool** | 代理群组管理 |
| **TaskCreateTool** / **TaskGetTool** / **TaskListTool** / **TaskUpdateTool** / **TaskOutputTool** / **TaskStopTool** | 后台任务管理 |
| **TodoWriteTool** | 写待办事项（旧版） |
| **ListMcpResourcesTool** / **ReadMcpResourceTool** | MCP 资源访问 |
| **SleepTool** | 异步延迟 |
| **SnipTool** | 历史片段提取 |
| **ToolSearchTool** | 工具发现 |
| **ListPeersTool** | 列出同伴代理（UDS inbox） |
| **MonitorTool** | 监控 MCP 服务器 |
| **EnterWorktreeTool** / **ExitWorktreeTool** | Git worktree 管理 |
| **ScheduleCronTool** | 调度 cron 任务 |
| **RemoteTriggerTool** | 触发远程代理 |
| **WorkflowTool** | 执行工作流脚本 |
| **ConfigTool** | 修改设置（**仅内部**） |
| **TungstenTool** | 高级功能（**仅内部**） |
| **MCPTool** | 通用 MCP 工具执行 |
| **McpAuthTool** | MCP 服务器认证 |
| **SyntheticOutputTool** | 通过动态 JSON Schema 生成结构化输出 |
| **SuggestBackgroundPRTool** | 建议后台 PR（**仅内部**） |
| **VerifyPlanExecutionTool** | 验证规划执行（由 `CLAUDE_CODE_VERIFY_PLAN` 控制） |
| **CtxInspectTool** | 上下文窗口检查（由 `CONTEXT_COLLAPSE` 控制） |
| **TerminalCaptureTool** | 终端面板截图（由 `TERMINAL_PANEL` 控制） |
| **CronCreateTool** / **CronDeleteTool** / **CronListTool** | 更细粒度的 cron 管理（属于 `ScheduleCronTool/`） |
| **SendUserFile** / **PushNotification** / **SubscribePR** | KAIROS 专属工具 |

这些工具通过 `getAllBaseTools()` 注册，并依据功能开关、用户类型、环境变量和权限拒绝规则进行过滤。
还有一个 **tool schema cache**，用于缓存 JSON Schema，以提升提示词效率。

---

## 权限与安全系统

Claude Code 在 `tools/permissions/` 中的权限系统，比单纯的“允许 / 拒绝”复杂得多。

### 权限模式

- `default`：交互式询问
- `auto`：通过 transcript classifier 做基于 ML 的自动批准
- `bypass`：跳过检查
- `yolo`：拒绝所有（这个命名非常反讽）

### 风险分类

每个工具动作都会被分类为：

- **LOW**
- **MEDIUM**
- **HIGH**

还有一个 **YOLO classifier**——一个快速的、基于机器学习的权限决策系统，可以自动判断。

### 受保护文件

像 `.gitconfig`、`.bashrc`、`.zshrc`、`.mcp.json`、`.claude.json` 等文件都受到保护，不能被自动编辑。

### 路径穿越防护

包括：

- URL 编码路径穿越
- Unicode 归一化攻击
- 反斜杠注入
- 大小写不敏感路径操控

都做了处理。

### 权限解释器

还有一个独立的 LLM 调用，负责在用户批准前解释工具风险。
也就是说，当 Claude 告诉你“这个命令会修改你的 git config”时，那段解释本身也是由 Claude 生成的。

---

## 隐藏的 Beta Header 与未发布 API 功能

`constants/betas.ts` 暴露了 Claude Code 与 API 协商的全部 beta 特性：

```typescript
'interleaved-thinking-2025-05-14'      // 扩展思考
'context-1m-2025-08-07'                // 100 万 token 上下文窗口
'structured-outputs-2025-12-15'        // 结构化输出格式
'web-search-2025-03-05'                // Web 搜索
'advanced-tool-use-2025-11-20'         // 高级工具使用
'effort-2025-11-24'                    // 努力度控制
'task-budgets-2026-03-13'              // 任务预算管理
'prompt-caching-scope-2026-01-05'      // 提示缓存作用域
'fast-mode-2026-02-01'                 // 快速模式（Penguin）
'redact-thinking-2026-02-12'           // 思考内容脱敏
'token-efficient-tools-2026-03-28'     // 更省 token 的工具 schema
'afk-mode-2026-01-31'                  // AFK 模式
'cli-internal-2026-02-09'              // 仅内部（ant）
'advisor-tool-2026-03-01'              // 顾问工具
'summarize-connector-text-2026-03-13'  // 连接器文本摘要
```

其中 `redact-thinking`、`afk-mode` 和 `advisor-tool` 目前都还没有公开发布。

---

## 即将到来的模型 —— Capybara、Opus 4.7 和 Sonnet 4.8

代码库中还出现了对一些**尚未公开发布的 Anthropic 模型**的引用：

- **Claude “Capybara”** —— 一个新的模型家族，已经到 version 2
- 其中一个变体 `capybara-v2-fast` 正在准备支持 **100 万上下文窗口**
- Capybara 分为“fast”与常规思考两档
- **Opus 4.7** 和 **Sonnet 4.8** 也已经在代码中出现

### 围绕 Capybara 的生产工程处理

代码显示，Anthropic 观察到了一个**真实存在的线上故障模式**：
Capybara 在提示词结构看起来像“工具结果后的轮次边界”时，可能会**提前停止生成**。

他们没有等模型修复，而是直接通过**提示形状手术**来缓解：

1. **强制加入安全边界标记**（`Tool loaded.`），防止轮次边界歧义
2. **迁移有风险的相邻块**
3. **把提醒文本“揉进”工具结果中**，以保持生成连续性
4. **为空工具输出加入非空标记**，避免模型困惑

这一切都包裹在可熔断的 `tengu_*` 功能开关里，方便分阶段灰度和快速回滚。

代码注释里还包含了**具体的 A/B 测试证据**，而不是模糊描述。这通常意味着：

- 这一块是**上线关键路径**
- 被密切监控过
- `ant/internal users are canary lanes` 这样的注释说明**内部员工用户就是外部发布前的金丝雀流量**

最强的合理推断是：Anthropic 正在推进一个 **Capybara 模型家族**，其中包含一个快档版本 `capybara-v2-fast`，支持最多 **1M 上下文**。

当然，代码本身并不能确认正式发布日期或最终 SKU 命名，但整体实现痕迹非常像一个正在积极准备上线的模型家族。

---

## 功能门控 —— 内部构建 vs 外部构建

这是整个代码库里最有架构意味的部分之一。

Claude Code 使用 Bun 的 `feature()` 函数做**编译期开关**。Bundler 会对这些开关进行**常量折叠**，然后把未启用分支做**死代码消除**。已知功能开关包括：

| 开关 | 控制内容 |
|------|----------|
| `PROACTIVE` / `KAIROS` | 常驻主动型助手模式 |
| `KAIROS_BRIEF` | Brief 命令 |
| `BRIDGE_MODE` | 通过 claude.ai 远程控制 |
| `DAEMON` | 后台守护进程模式 |
| `VOICE_MODE` | 语音输入 |
| `WORKFLOW_SCRIPTS` | 工作流自动化 |
| `COORDINATOR_MODE` | 多代理编排 |
| `TRANSCRIPT_CLASSIFIER` | AFK 模式（ML 自动批准） |
| `BUDDY` | 宠物伙伴系统 |
| `NATIVE_CLIENT_ATTESTATION` | 客户端证明 |
| `HISTORY_SNIP` | 历史截取 |
| `EXPERIMENTAL_SKILL_SEARCH` | 技能发现 |

另外，`USER_TYPE === 'ant'` 还控制 Anthropic 内部特性：

- staging API 访问（`claude-ai.staging.ant.dev`）
- 内部 beta header
- Undercover mode
- `/security-review` 命令
- `ConfigTool`
- `TungstenTool`
- 将调试提示词导出到 `~/.config/claude/dump-prompts/`

运行时功能开关则由 **GrowthBook** 管理，采用激进缓存策略。
前缀为 `tengu_` 的 feature flag 控制着从 fast mode 到 memory consolidation 的各种行为。

很多检查都用 `getFeatureValue_CACHED_MAY_BE_STALE()`，以避免阻塞主循环——对于功能门控来说，“值可能稍旧”是可以接受的。

---

## 其他值得注意的发现

### 上游代理（Upstream Proxy）

`upstreamproxy/` 目录中有一个支持容器环境的代理中继系统，会使用 **`prctl(PR_SET_DUMPABLE, 0)`** 防止同 UID 进程通过 `ptrace` 读取堆内存。

它会从 CCR 容器中的 `/run/ccr/session_token` 读取 session token，下载 CA 证书，并启动本地的 CONNECT→WebSocket relay。
Anthropic API、GitHub、npmjs.org 和 pypi.org 被明确排除在代理之外。

### Bridge Mode

`bridge/` 中还有一个基于 JWT 认证的桥接系统，用来和 claude.ai 集成。
支持的工作模式包括：

- `'single-session'`
- `'worktree'`
- `'same-dir'`

还包含用于更高安全等级的“可信设备令牌”。

### 迁移中的模型代号

`migrations/` 目录暴露了内部代号演进史：

- `migrateFennecToOpus` —— **Fennec**（耳廓狐）曾是 Opus 的代号
- `migrateSonnet1mToSonnet45` —— 带 1M 上下文的 Sonnet 最终变成了 Sonnet 4.5
- `migrateSonnet45ToSonnet46` —— Sonnet 4.5 → Sonnet 4.6
- `resetProToOpusDefault` —— 某个阶段 Pro 用户曾被重置为默认使用 Opus

### 归因 Header

每个 API 请求都会带上这样一个 header：

```text
x-anthropic-billing-header: cc_version={VERSION}.{FINGERPRINT}; 
  cc_entrypoint={ENTRYPOINT}; cch={ATTESTATION_PLACEHOLDER}; cc_workload={WORKLOAD};
```

`NATIVE_CLIENT_ATTESTATION` 功能允许 Bun 的 HTTP 栈用计算出的哈希值覆盖 `cch=00000` 这个占位符。
本质上，这是一个**客户端真实性校验**，让 Anthropic 可以验证请求是否真的来自 Claude Code 安装实例。

### Computer Use —— “Chicago”

Claude Code 还包含了完整的 Computer Use 实现，内部代号 **“Chicago”**，构建在 `@ant/computer-use-mcp` 之上。

它支持：

- 截图捕获
- 点击 / 键盘输入
- 坐标变换

并且仅向 Max / Pro 订阅开放（内部员工则可绕过）。

### 价格

如果你关心这个：`utils/modelCost.ts` 里的所有定价都与 Anthropic 官网公开价格**完全一致**。没什么特别值得报道的。

---

## 最后的想法

毫不夸张地说，这是我们迄今为止对**生产级 AI 编程助手内部工作方式**最全面的一次窥视之一。
而且还是通过**真实源代码**看到的。

有几点特别明显：

### 1. 工程质量确实很强

这不是一个“周末项目加个 CLI 壳子”那么简单的东西。
多代理协作、dream 系统、三重门槛触发架构、编译期功能消除——这些都体现出很深的系统设计思考。

### 2. 他们手里还有很多没发布的东西

KAIROS（常驻 Claude）、ULTRAPLAN（30 分钟远程规划）、Buddy 宠物、Coordinator 模式、Agent Swarm、Workflow Scripts……
这个代码库明显比公开版本超前很多。大部分都被功能开关隐藏，在外部构建里不可见。

### 3. 内部文化也暴露出来了

动物代号（Tengu、Fennec、Capybara）、有趣的功能命名（Penguin Mode、Dream System）、一个带扭蛋机制的电子宠物系统。
可以看出，Anthropic 内部确实有人玩得挺开心。

如果这件事有什么核心启示，那就是：
**安全很难做。**
不过显然，`.npmignore` 对某些人来说更难。

---

Fork 翻译：隽戈
