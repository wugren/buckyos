# 我们为什么做了 Behavior Loop

Agent Loop 这一层的设计,大部分实现共享着几个没被质疑过的假设:工具列表是一个固定集合、Loop 的结束由模型不返回 tool call 来隐式判断、状态机要么不存在要么是外挂框架。

这些假设在短任务里没问题。但任何做过 30 轮以上长任务的人都知道,这一层的协议是有结构性缺陷的 —— 不是某个具体实现不够好,是协议本身没给一些必要的语义留位置。

这篇想讲的就是:在 Loop 这一层,有三个被普遍焊死的耦合点,其实是可以拆开的。Behavior Loop 是我们拆完之后的样子。

---

## 一、Function 和 Action 应该分开

这是改动最深的一条,先讲。

传统 Agent Loop 里,工具列表是**一个集合**。你在 prompt 里塞什么,LLM 就看到什么,也就是它能调的全部。这个看似自然的设计,把两件本来不同的事情焊在了一起:

- **物理能力清单** —— 系统里所有可调用的原子能力
- **语义动作集** —— 当前推理步骤里,LLM 应该看到的、能用的动作

这两件事的焊死,直接导致了所谓的"死工具流":LLM 在 prompt 里看到 50 个工具,实际每次只用 2 个,剩下 48 个白白消耗 context 和注意力。更深的问题是,调度器没办法**临时收窄或扩展**LLM 的认知能力集 —— 因为根本没有"工具的引用"和"工具的执行"这两个分离的概念。

Behavior Loop 把这两层拆开:

- **Function 层**是物理能力清单,工程师管,后端怎么实现、参数是什么,跟 LLM 无关
- **Action 层**是当前 Behavior 暴露给 LLM 的语义动作集,可以是 Function 的子集、组合、或者重命名

这本质上是一种**读写分离**:执行走 Function 层,认知走 Action 层。同一个 `http_get` 可以在调研 Behavior 里以 `research_web` 的语义出现,在调试 Behavior 里以 `fetch_api_response` 出现 —— 后端没动,但 LLM 看到的"我现在能做什么"完全不同。

这个分离带来的连锁后果不止是 context 优化:

- Context 注入策略可以独立于 prompt 工程演化 —— Function 池不变,Action 视图按需裁剪,死工具流自然消失
- 后端能力升级不需要重写 prompt —— Function 实现可以替换,Action 语义不变,LLM 不需要"重新学"
- Action 层成了 Behavior 的语义边界 —— 不同 Behavior 共享同一个 Function 池但暴露不同的 Action 视图,这是一种比"换 prompt"更深的角色分化

---

## 二、状态机应该是 Loop 输出协议里的一个可选槽位

Agent 圈里有两个长期对立的流派:

**宪法派**相信 LLM 足够强,给一个好的角色提示词加一组工具,它自己会规划好。状态机是工程师不信任模型的拐杖。

**状态机派**相信 LLM 不够可靠,必须用外部状态机锁住执行路径。LangGraph、Temporal-style workflow 都是这一派的产物。

这两派互相看不上,但他们其实在共享一个错误前提:**状态机要么不存在,要么是 Loop 之外的外挂框架**。

LangGraph 这类外挂状态机的存在,本身就是 Loop 协议设计不够的证据 —— 如果 Loop 自己能表达状态迁移,你不需要在它外面再搭一层。

Behavior Loop 在 Step 的输出协议里留了一个字段:`next_behavior`。

- 不填,Loop 继续在当前 Behavior 里推理 —— 这时它就是一个朴素的 ReAct Loop,宪法派可以完全无视这个字段的存在
- 填了,就是显式跳转到下一个 Behavior —— 系统提示词切换、Action 视图切换,LLM 进入一个新的认知上下文

这一个字段消解了整个派系对立:

- 你想做宪法派?永远留空,你得到的就是单 Behavior 的纯推理 Loop
- 你想做状态机派?在 Behavior 之间显式跳转,你得到的就是一个 **LLM 自己驱动**的有限状态机 —— 状态是 Behavior(以及它的系统提示词、Action 视图),迁移是 LLM 在 Step 里输出的 `next_behavior`
- 关键是这两种模式用的是**同一个执行核** —— 不换框架、不换工具协议,只是同一个 Step Schema 上的不同使用风格

更值得说的是,这种状态机不是被强加在 LLM 之上的约束,而是承认了一个事实:**LLM 在每次推理里本来就在做状态决策**(我下一步该探索还是收敛?该交给用户还是继续自动?),只是传统 Loop 没给这个决策一个表达通道。Behavior Loop 不是给 Agent 加状态机,是把 LLM 一直在做的状态机让它显式说出来。

---

## 三、意图信号必须显式

前两条是结构性的改动。这一条是基础设施,但它解释了为什么前两条能成立。

传统 Loop 里,意图信号是**双向缺失**的。

**输出方向**:LLM 不返回 tool call 就算结束了。但"它结束了"和"它觉得自己应该结束"是两回事 —— 前者是隐式推断,后者才是意图。中间断了你不知道是真的完成了还是只是这一轮没调工具,恢复的时候只能把整个历史重新喂回去让 LLM 自己判断"我刚才到哪了"。这本质上是把**调度器的状态**藏在了**模型的注意力**里 —— 一个无状态系统假装自己有状态。

**输入方向**:LLM 在第 5 轮 tool call 时,它不知道自己处于什么意图阶段 —— 还在探索?在收敛?在等待用户?Message array 没给它这个信号,只能从历史里猜。

Behavior Loop 的 Step Schema 强制每次输出都 commit 一个意图状态:

```
Step:
  结论:   上一步动作的结果观察
  思考:   当前的推理
  动作:   要执行什么
  next_behavior: 留空(继续) 或 跳转目标(显式结束当前 Behavior)
```

这四个槽位每个都是双向意图通道 —— 既是 LLM 告诉调度器"我处于什么阶段",也是调度器和后续 Step 读到"上一步 LLM 处于什么阶段"。

有了这个基础,前两条才有放置的位置:Action 视图能按 Behavior 切换,是因为 `next_behavior` 让 Behavior 边界变得显式;状态机能内生于 Loop,是因为 Step 本身就是状态迁移的最小单元。

---

## 四、History、Attention 和 KV Cache 的取舍

Behavior Loop 不是 Chat Message Loop。它更接近一个 Work Session:围绕明确 Objective 持续推进,完成后结束。因此它的历史策略不追求无限累积对话,而是优先保证每轮推理时关键信息落在 LLM attention 的"U 型区域"两端:

- 头部:稳定的 system prompt,包含 objective、process rules、result protocol、当前 Behavior 暴露的 Action 视图和 skills
- 尾部:最近若干个完整 StepRound,也就是 LLM 上一步输出的 Intent 和系统执行后的 Action Results

这和 KV Cache 的最优命中天然存在张力。为了让旧历史逐渐从中部让位给新的完整 StepRound,历史会发生压缩;一旦压缩发生,严格的长前缀 cache 命中会被破坏。这个代价是有意接受的:对 Work Session 来说,让当前推理看到正确的任务头部和最近执行尾部,比维持一条永远 append-only 的 Chat transcript 更重要。

Behavior Loop 的压缩分两层。

第一层是常规的 StepRecord 分级压缩。StepRecord 仍然保留结构化语义,但历史 step 的 detail 会随着它滑入 context 中部而逐渐消失。旧 step 可以从完整的:

```
assistant: Step Intent
user:      Step Action Results
```

降级为更短的 compact record。这样做的效果是:某个中部 StepRecord 被压缩后,它后面一段历史的 detail 可能都会被重新布局,但系统因此又为未来几个 StepRound 腾出尾部空间,让新的 Intent + Action Results 可以完整进入模型输入。

第二层是触顶后的强制有损压缩。它不是普通的 compact render,而是把一批旧 StepRecord 折叠成固定大小的 History Summary 块:

- 不再保留原始 Step 结构
- 记录被压缩的 step 数量
- 记录起止 step index、起止时间戳、所属 behavior 范围
- 摘要这批 step 大致完成了什么、留下了什么约束或结论

这层压缩是最后手段。它的目的不是让模型完整复盘每个动作,而是在 context window 快触顶时重新制造一个稳定的历史前缀,让后续 N 个 StepRound 可以继续以尽量少破坏 KV Cache 的方式运行。

因此,Behavior Loop 的历史不是"越完整越好",而是按位置和阶段承担不同职责:

- 当前 Behavior 的最近 StepRound:完整、强可见、位于尾部
- 当前 Behavior 的较旧 StepRecord:结构化但分级压缩
- 跨 Behavior 继承的旧历史:必须降级为系统可解释的 history record 或 summary,不能继续占用当前 Behavior 的 hot tail
- 触顶后的长期历史:固定大小的 summary block

## 五、Behavior switch 的三种模式

`next_behavior` 不是普通的下一轮提示词变化,而是状态机边界。LLM 只在 Step 输出里声明"我要去哪个 behavior";具体怎么切换由 session class 的 `switch_mode` 决定。当前实现支持三种模式:

| 模式 | 心智模型 | 是否继承上一个 behavior 的 history | `END` 语义 |
| --- | --- | --- | --- |
| `normal` | 带历史的跳转 | 继承同一 session/process 的 step history | 结束当前 session/process |
| `fork` | 带历史的调用,结束后返回 | 子 behavior 继承 parent 的已解释 history | 子 behavior `END` 后恢复 parent |
| `independent` | 切到另一个独立历史流 | 不继承上一个 behavior 的 history;恢复目标 process 自己的 snapshot | 弹回上一 process;栈空才结束 |

三种模式共同遵守一个规则:切换 Behavior 时会同时更换 Work Session 的"头"和"尾"。

- 头部更换:新的 system prompt、生效的 process rules、Action 视图和 skills
- 尾部重置:新的 Behavior 开始自己的最近 StepRound hot area

因此跨 behavior 继承的历史只能作为系统解释过的 history record 进入新 behavior,不能继续占用新 behavior 的 hot tail。

### `normal`:同一历史流里的跳转

`normal` 是最直接的状态机跳转。Runtime 用新的 behavior 配置刷新 snapshot 的 request 侧:

- 替换 system prompt、objective、tool/action policy、model policy、budget、human/error/output policy。
- 保留同一 session/process 的 `steps`、`history_summaries`、`next_step_index` 和 `last_report`。
- 把旧 behavior 的 hot `last_step` 沉淀回 `steps`,新 behavior 不继承旧 hot tail。
- 后续推理继续在同一个 process 上运行,没有"返回调用方"概念。

如果从 `plan` normal 切到 `do`,那么 `plan` 的 StepRecord 会进入 `do` 的 `step_history`;`do` 自己随后产生的最近 step 才能作为 `assistant/user` hot pair 出现在尾部。

### `fork`:继承 history 的子调用

`fork` 是 fork-join 模型。Runtime 在切到 child behavior 前保存 parent snapshot,然后为 child 创建新的 request:

- child 使用自己的 system prompt、Action 视图和 hot tail。
- child 继承 parent 已沉淀的 `steps`、`history_summaries` 和 `next_step_index`。
- parent 当前 hot `last_step` 会先降级为 child 可读的 inherited StepRecord,不会作为 child 的 hot pair。
- child 结束时不把自己的完整 step stream 写回 parent;child stream 是一次性分支。
- child 的 `<report>` / join handoff 会作为 runtime history input 回到 parent。
- parent snapshot 被恢复,parent 从 fork 点之后继续推理。

因此 `fork` 和 `normal` 的共同点是"子/目标 behavior 能理解之前发生了什么";区别是 `fork` 有调用栈和返回点,且返回时只把子分支结果汇入 parent,不把 child 的全部执行历史并入 parent 的主干 hot tail。

### `independent`:独立历史流

`independent` 把每个 process entry 视为独立的 behavior 历史流。切换时:

- parent 的 terminal snapshot 写入自己的 `.meta/behavior_<entry>.snap`。
- target process 如果已有 snapshot,就恢复它自己的 snapshot;如果没有,就从 target behavior fresh request 开始。
- 不把 parent 的 `steps`、`history_summaries` 或 hot tail 复制给 target。
- 每个 process 有自己的 round/error budget 窗口。
- `END` 时保存当前 child process 的 terminal snapshot,再恢复 parent process snapshot;栈空时 session 才真正结束。

所以 `independent` 适合长期并列的独立工作流,不是"带上下文的分支执行"。

## 六、Step history 和推理输入形态

Behavior Loop 的下一轮输入不是简单 append chat transcript,而是由 request 头部、`step_history`、当前 behavior hot tail 和真实用户/event 输入共同构造。当前实现的核心 message 序列是:

```text
system: current behavior objective + process rules + action view + result protocol
optional user: real user/event input with background environment
optional user: <<step_history>> ... <</step_history>>
assistant: current behavior hot step intent
user:      current behavior hot step action results
assistant: current behavior hot step intent
user:      current behavior hot step action results
```

`step_history` 是一条 user message,承载已经不该占 hot tail 的历史语义。它可以同时包含:

- 跨 behavior 继承的 `<step_record>`
- 压缩后的 `<history_summary>`
- runtime 生成的 `<history_input>`,例如 `on_switch` 和 fork join handoff

示例:

````xml
<<step_history>>
<step_record behavior="plan" index="1" started_at_ms="..." ended_at_ms="..." compression="full">
<observation>Todos were created successfully.</observation>
<thought>The plan is ready, so execution should start.</thought>
<actions>
- Run todo add "T01"

```output
Created T01.
```
</actions>
</step_record>
<history_input source="opendan:on_switch" at_ms="...">Continue TASK_ANCHOR.</history_input>
<</step_history>>
````

这个例子里的 `Continue TASK_ANCHOR.` 不是一条裸 user turn。它是 runtime 根据目标 behavior 的 `on_switch` 模板生成的 handoff input,必须合并在 `step_history` 里,并且排在触发它的历史 StepRecord 后面。这样模型看到的是一段连续的、可解释的状态机历史,而不是两个用户消息突然相邻。

完整 step 仍然渲染为严格相邻的 hot pair:

````text
assistant: <response>...</response>
user:
<<last_step_action_results behavior="<behavior_name>" step="<step_index>">>
- AgentToolResult.title

```output
AgentToolResult.output | AgentToolResult.detail
```
<</last_step_action_results>>
````

这个 hot pair 只属于当前 behavior 的最近执行上下文。一旦切到另一个 behavior,它必须沉淀进 `step_history`,并携带至少这些元数据:

```text
behavior_name
step_index
started_at / ended_at
compression_level
```

随后推理产生新的 `assistant Step 0 Intent`;系统执行 Step 0 actions 后得到 `user Step 0 Action Results`;Step 0 成为当前 behavior 的 hot tail。再往后,它会逐渐进入当前 behavior 的 StepRecord history;如果发生 Behavior 切换,它会以 `step_record` 或 summary 的形式被继承,而不是继续作为新 Behavior 的完整 assistant/user hot round。

## 七、`on_switch`、fork join 和真实用户输入的区别

`on_switch` 是 behavior 配置里的 runtime 模板,不是用户真实发来的消息。当前实现按来源区分输入:

- 真实用户/peer message:作为本轮 user turn 进入 request tail,可附带 background environment。
- 业务 event:格式化为 user-visible wakeup,驱动本轮推理。
- `on_switch`:渲染为 `HistoryInputRecord`,进入 `step_history`。
- fork child `END`:恢复 parent snapshot 后,把 child report / join marker 渲染为 `HistoryInputRecord`,进入 parent 的 `step_history`。

这样做的原因是:真实用户输入会改变对话事实,应该作为本轮 tail;而 `on_switch` / fork join 是 Runtime 对状态机边界的解释,应该归入 StepRecord history,和触发它的 StepRecord 保持同一条时间线。


## 收束

这三条改动有一个共同的方法论:**好的抽象不是强制选择,而是提供可选维度**。

传统 Loop 的问题不是它选错了,而是它没让你选 —— 工具列表是固定的,结束信号是隐式的,状态机是外挂的。每一个被传统 Loop 焊死的决策,Behavior Loop 都重新打开成了一个可选项。

Behavior Loop 不是一个框架,是一组最小够用的语义槽位。这些槽位让原本需要外部框架才能表达的能力,变成 LLM 输出协议自身的一部分。
