# Agent Context Messages

说明在LLM Context + Agent Session的支持下，会构造出哪些典型的 Agent Context Message List

本文会从原始语义，说明相同目的下如何实现对LLM Context的Message List的管理

## 1个标准 Round

模式一 标准Agent Loop里的Message Pair: 通过一个Input User Message触发，得到Assistant Message结束。中间通常有多个tool call-result message pair

[system] [user [a-call u-result] [a-call u-result] [a-call u-result] assistant]@round1 [user [a-call u-result] [a-call u-result]  assistant]@round2 [user assistant]@round3 **user**

模式二 Behavior Loop里的一次beahvior run: 通过一个on_switch 触发，得到 behavior_result  (next_behavior有值)

[behavior-system] ([user:step0-step3-history + beahvior-on-switch]) [agent:intent user:last_step_result]@step4 [[a-call u-result] agent:intent user:last_step_result]@step5 **beahvior-on-switch + pending inputs** 

Behavior Step:通过last_behavior_results触发，得到下一个agent intent. 这个粒度比tool call->tool result要大 
[behavior-system] ([user:step0-step3-history + beahvior-on-switch]) [agent:intent user:last_step_result]@step4 [[a-call u-result] agent:intent user:last_step_result]@step5 agent:intent **user:last_step_result + pending inputs**


> Behavior Loop 可以选择在构造包含last_step_result的user_message时，读取session的pending-input,也可以只在on-switch（通常是一个激活节点）时读取pending input（就像传统AgentLoop必须得到一个assistant message之后才能读取pending input)


## User Message ： 推理的驱动力

这个章节从原理上讲，应该放到 Agent Session 里。因为它涉及到 Agent Session 的状态管理，即在什么情况下会让 Agent Session 继续，这与 Agent Session 的状态是强相关的。
我们在这里会更多地站在“什么样的 Message 序列是合法”的角度，来讨论驱动力的问题。

这也是我们前面讲的一个 Round 的重要的一点。

从原理上讲，在一个 Round 中间，我们是不推荐去插入 UserMessage 的。这也是我们要抽象 Round 概念的一个核心目标：这样我们每次其实都可以认为，一个 UserMessage 的目标就是进入 NextRound。

因为 Agent Loop 有 steps，它其实不会卡在传统的 Agent tool 的 call chain 中间。

在我们现在用的数据里，每个 step 结束并执行完 action 之后，会返回一个 last step result。但更完整的说法应该是，对 last step 的观察（observation）应该叫做 Last Step Observation。在这个观察过程中，其实原理上讲，它也是可以去读这个 Pending Inputs 的。

原理上可以，但我现在也在纠结：原理上可行的事情，到底合不合理？从实际的推演来看，在所谓的顶层或者叫做 Work Session 的 Default Behaviour 上，通常是合理的。

原因如下：
1. Default Behaviour 通常是用来做状态机的入口，作为 Loader 使用。
2. 真正干活的（比如专注在一个目标上的 Behaviour），通常是被 Fork 出来的。

所谓的 Sub Behaviour 通常是不必要的，因为这会破坏它的隔离语义。

另外一个就是 session 状态对驱动力的影响。

我们现在的 session 其实有很多状态，其中有些属于比较正常的状态，比如：
1. 在工作中
2. 处在 idle 状态

我们现在设置了一个 end 状态，很多时候是基于“轮”的概念。从某种设计上，或者说从现在 work session 的实现上讲，我们会把 default behavior 的 end 状态作为整个 session 的结束状态。这其实是一个 OpenDAN 在 searching 层的一个设计假设。

因为它的核心假设是 work session 有一个明确的 objective，只要这个 objective 达到之后，这个 session 就可以结束了。所谓的结束，就是它变成了一个只读的东西，不应该再进来任何消息了。

这也是我们为什么要做 work session 和 workspace 分离的原因：
1. Work session 代表的是一个确定的话题。只要话题的 objective 达到了，这个 session 就应该永远只用来做观察。
2. 举个例子，如果用户现在提了一个从需求一开始做一个工程的任务，那这个 work session 其实就 focus 在初始的框架搭建和这个任务一上。
3. 而 workspace 是可以一直改的。

然后如果用户在第一个 work session 结束之后，通过 UI session 想要做第二个需求，按照我们的设计，他其实应该会启动 work session 2。

相当于在 work session 2 看来，它的唯一真相源（single source of truth）就是 workspace 里已经存在的文件。我们并不认为让他去了解 work session 1 里的工作历史记录会有多大好处，甚至可能还是个坏处。

我们在实际使用各种 code agent 的时候，其实也会不断遇到这个问题。这是手工的。从根本上讲，还是想做这个 Session 机。我们为什么要做 Session，其实就是想做上下文的隔离。
但这种设计其实是一种约束。实际上我们在真正跑的时候，发现这个边界并不好控制。

比如 Work Session 完成之后，用户在时间比较短的情况下，可能想了解一下产物的位置，或者在需求做完后打算做一下部署（原来可能只有构建工作）。这时候有些任务是强相关的，如果有上下文肯定是最好。

所以这就涉及到 Work Session 在这一层，要不要非常强烈地告诉用户这个 Session 已经结束了。虽然它的 Objective 已经达到了，但我们其实可以通过用户 Forward 消息到 Work Session 时的错误，明确地告诉大模型：必须开启一个新的 Session 来承载接下来的事情，也就是做硬隔离。硬隔离之后，就意味着所有的信息只能从 Workspace 拿。当然，我们现在所有的日志也都是以文本的形式存在这个 Worksession 里的。

也就是说，他开另外一个 Worksession，如果只是想调查一下产物，他也完全可以去读取上一个 Worksession 已经固化的历史记录。这里面很多关于 Work Session 的设计 trade-off 其实是 OpenDAN 专有的。

也就是说，它的根本目标是希望能够让 Work Session 可以结束，并且结束之后就不要再重启。但这是一个设计抉择。我们现在讨论的这个其实更加偏向于通用的 LLM Context。在这个层面，END 的概念其实只是一轮结束了而已，它并不会强烈地约束 default behavior 之后最后会怎么样。

换句话讲，对于一个 Work Session 来说，如果它属于 END 状态，即上一轮结束了，这是一个非常正常的状态。举个例子：
1. 如果用户还要往这个 Work Session 里面去放内容（不管是放 Event 还是 Message）；
2. 其实 Message 可以被认为是一种特殊的 Event，或者说我们可以认为所有的 Event 都是特殊的 Message（虽然这两者之间还是有蛮大区别的）；
3. 只要用户想放，这条 Message 完全可以成为一个驱动力。

也就是说，Agent 完全可以在 Session 处于 END 的状态下，基于新的 Pending Input 去创建一个新的 LLM Context，让它重新跑起来。

=====
另外一个就是刚刚提到的 input message 和 events 的区别。

关于 pending message 这件事，它其实是一个非常强烈的信号，也就是说它的消费是非常显性的，通常会成为一种驱动力。当一个消息到达并被分发到 Session 之后，它会进入该 Session 的 pending input 并被序列化，然后等待合适的时机。这个时机通常由 Session 的状态机决定。

对于 Pending Events 来说，我们现在支持一种 Ban 事件。

所谓 Ban 事件，说得更加直白一点，就是系统在某些状态下，只能被特定的 Event 驱动。

比如我现在处于 Wait Input Message 状态，即使我关心的其他事情（例如一个 Delegate Task）完成了，但因为我现在处于 Wait User Message 状态，可能是在等用户确认某件事情。

这时候，其实所有的事件都不会去触发 User Message。这是筛选层（Filter Layer）逻辑：
1. 筛选层通过当前状态生成一种掩码（Mask）。
2. 这种掩码只允许特定的 Input 产生推理驱动力。
3. 其他的改变只会老老实实地待在 Pending Input 区。

然后 Message 永远不会存在所谓的“半订阅”状态，我们的事件才会半订阅。

因为很多时候事件的变化非常剧烈。最常见的就是时间（Timer）事件，如果你真的每次 Timer 触发都去驱动的话，系统可能永远都停不下来了。所以我们现在引入了一种“半订阅”机制。

所谓半订阅的意思是：
1. 这东西本身没有驱动力。
2. 当 UserMessage 构造时，会有一个 Background Environment 区域。
3. 这个 Background Environment 区域会观察当前 Session 所订阅的半订阅事件。
4. 当这些事件产生了真正的改变（通常是基于状态的改变）后，它会将这个新状态插入到 UserMessage 的段落中。

也就是说，我们的驱动力反复会出现这种情况：当驱动力到达并开始真正构造 UserMessage 时，它会通过一个模板结构，选择要不要植入一些系统可以提供的额外信息。

落到我们的配置开发层面，这其实就是我们开发的核心。在配置开发中，我们的 Hook Point 主要分为两类：
1. 渲染系统提示词（on_initial）
2. 渲染 user message

从驱动力的角度来看，user message 的渲染无非也是两类：
1. 第一类是来自于 pending input 触发。在一个处于等待状态的 message 中，触发一个构造使其从等待状态进入运行状态，这是一种驱动力构造。
2. 第二类是 LLM Context 本身已经处于运行过程中。在运行过程的某些环节里，它的提示词模板会主动尝试消费 pending input。

消费这个概念，其实跟过去的提示模板相比，是一个比较痛苦的概念。我们需要去定义什么叫做“精确的消费”。

按照我们之前的逻辑，只要进行过一次推理并产生了一个 element context，它就可以正确地把这个 input 给消费掉。但这其实是一个需要仔细设计的过程，因为在提示模板里面，它更多是事件驱动的。事件到了之后，系统是被推着走的。

比如有一个 on_user_message 这样的 hook point，你去渲染时，感觉就是：
1. 系统在 pending inputs 里面已经处于 idle 状态。
2. 只要系统里的 pending inputs 还有内容，我就只取一条出来。

这里有一个细节：到底应该是取全部还是只取一条？我们现在的默认逻辑会变成只取一条。也就是说系统会去做从 pending inputs 里面读一条消息，然后调用渲染模板，基于这个渲染模板得到最终的 user message 之后，驱动LLM content 引擎运行。

关于每一轮的时间点，这里有两个比较精细的点：

1. 标准的 Agent Loop 流程
   标准的 Agent Loop 是从一个 User Message 到另一个 Assistant Message，它本身不会自动产生 User Message。这意味着系统拿到input后，是否会构造User Message 完全是由 System 的 Session 配置算出来的。

2. Behavioral Loop 中的观察机制
   在我们刚讲的 Behavioral Loop 里面，其实是有 Step 的。所谓的每一轮其实是一个 Observation（观察）。在这个观察环节中，系统完全可以去读取 Pending Input。
   这里涉及到一个逻辑：如果我们在运行过程中去取了 Pending Input，那它就一定会取出来；如果没有去取，它就不会读取。



## 停止运行中的Round

外部信号（用户 `stop`/`cancel`、新 user input 到达、调度器决定换 behavior）在一个 round/step 进行中时切入。当前实现两种模式，分别对应不同的"代价 vs 痕迹"取舍。

抢占前共同形状（一个标准 round 正在跑，assistant 已发出多个 tool_use，部分 tool_result 还没回来）：

```
[system] [user [a-call u-result] [a-call ?in-flight?] **a-call pending** *partial assistant text*
```

### 模式 A：Graceful（温和收尾 / wind-down）

对应代码 `InterruptMode::Graceful`，由 `stop` 触发。

- **不打断**正在跑的 inference（即便 LLM 正在出 token 也让它出完）
- 给所有 pending 的 tool_use 补一个**合成的** tool_result = `Cancelled`
- 把 `max_rounds` 临时设为 0，让 LLM 不能再发新 tool_call
- 同 llm_context resume，让 LLM 在这个被截断的 round 里**自然吐一句 ack / 总结**收尾

抢占后形状：
```
[system] [user [a-call u-result] [a-call u-result=Cancelled] [a-call u-result=Cancelled] assistant: 收尾文本]
```

特点：

- partial assistant 完整保留
- 历史里能看到"被中断过"的痕迹（Cancelled 标记）
- 多花一次 wind-down 推理
- 后续 user input 走全新 round，但 KV cache 前缀保得最长

### 模式 B：Discard（硬截断）

对应代码 `InterruptMode::Discard`，由 `cancel` 触发。

- 立刻 fire `LLMContextInterruptHandle.interrupt(reason)` 把正在跑的 inference abort 掉
- 定位末尾那条带未完 tool_use 的 assistant turn，**整条切掉**
- 清空 `pending_tool_calls`，**不补任何 tool_result**
- 直接落盘截断后的 snapshot

抢占后形状：
```
[system] [user [a-call u-result]]
```
（末尾 assistant turn 连同它的 partial 文本和 tool_use 一起消失，像上一轮干净结束。）

特点：
- partial 工作完全丢失
- 历史看不出来被中断过
- 不花额外推理
- 截断点之后开新 round，KV cache 前缀变短（少了被切掉的那段）

### 选择维度

| 维度 | Graceful | Discard |
|---|---|---|
| in-flight inference | 跑完 | 即刻 abort |
| partial assistant | 保留 | 删除 |
| pending tool_use | 补 `Cancelled` 合成 result | 清空，无 result |
| 历史可读性 | 有中断痕迹 | 似无事发生 |
| 额外推理开销 | 1 次 wind-down | 0 |
| KV cache | 同 ctx 继续 append | 截断后下一轮 prefix 变短 |
| 典型场景 | 用户喊停但希望 agent 优雅交接 | 用户后悔了 / 调度器要 hard reset |


## 压缩 History Message

会把一部分处在U形注意力中间的message,压缩成压缩成更短的形式。压缩的结果通常有两种表现形式：要么是在原有的 message 上直接修改，要么就是把一组消息压缩成一个 消息对

压缩肯定会导致KV Cache失效，因此

- 压缩历史记录后，通常会留出较大的空间，支持未来的几轮
- 可以积极的在hot tail做压缩（对KV Cache影响较小）

### 机械压缩

Call-Result 压缩：
目的： 减少ToolResult在 Message List中的长度或则删除 call-result pair

- 基于Agent Tool Result协议，可以实现Tool Result的分级压缩
- 基于Agent Tool Result协议，可以及时丢弃不必要的call-result pair


Step Record History压缩：

- 将多个[agent:intent user:last_step_result] 合并成一个(还是2个）? step-record-history message
- 在step-record-history中，可以对action-result进行降级，直到完全丢弃
- 通过保留 每个step的 观察-思考 等关键信息

通过分级机制，由机会完全丢弃ActionResults,只保留

### 在Input触发推理前，通过LLM 压缩，释放Context Window

目的：将一组中间的 Message Pair,压缩成一个Message Pair
[system] [user [a-call u-result] [a-call u-result] [a-call u-result] assistant]@round1 .... [user [a-call u-result] [a-call u-result]  assistant]@round22 [user assistant]@round23  **user**
压缩后再开始推理
[system] [user [a-call u-result] [a-call u-result] [a-call u-result] assistant]@round1 [user:压缩需求1 agent:压缩结果1] [user:压缩需求2 agent:压缩结果2] [user assistant]@round3 **user**

这种机制基本上是模式无关的，只要边界切对就好

## 状态机切换（仅限 Behavior Loop)

以session的共同状态为基础，sessionn在完成不同任务（关键是系统提示词不同）的多个LLM Context中切换。让每个LLM Context在运行时，能有独立的 Message List。

> do->check->do->check->end

### 普通切换（注意：这是反模式）


[do-behavior-system] ([user:step0-step3-history + beahvior-on-switch]) [agent:intent user:last_step_result]@step4 [[a-call u-result] agent:intent user:last_step_result]@step5 agent:next_behavior=CHECK

执行切换后,填入on_switch构造的user_message

[check-behavior-system] ([user:step0-step3-history + beahvior-on-switch]) [agent:intent user:last_step_result]@step4 [[a-call u-result] agent:intent user:last_step_result]@step5 agent:next_behavior=CHECK **check-beahvior-on-switch** --继续推理-->

相当于agent的next_beahvior回触发一个特殊的on-switch调用，构造一个user message插入并推动进入下一个step. 这个由[agent:behavior-switch + user:on-switch]的Message Pair是一个特殊的的状态机切换Message Pair

> 这是从状态机切换可用路径中推导出来的理论模式，但实际上不可用

### 独立切换 

[do-behavior-system] **do-beahvior-on-switch** [agent:intent user:last_step_result]@step1 [[a-call u-result] agent:intent user:last_step_result]@step2 agent:next_behavior=CHECK

执行切换后，Resume target behavior的llm_context,并填入on_switch构造的user_message:

[check-behavior-system] **check-beahvior-on-switch**  （从头开始）

check模式 LLM推理,切换回DO:

[check-behavior-system] check-beahvior-on-switch [agent:intent user:last_step_result]@step1 [agent:intent user:last_step_result]@step2 agent:next_behavior=DO

执行切换，DO从上一个点回复,并继续推理到CHECK

[do-behavior-system] do-beahvior-on-switch [agent:intent user:last_step_result]@step1 [[a-call u-result] agent:intent user:last_step_result]@step2 [agent:next_behavior=CHECK **do-beahvior-on-switch**]  [agent:intent user:last_step_result]@step3 agent:next_behavior=CHECK

执行切换，CHECK恢复独立的LLM Context，并继续执行到END

[check-behavior-system] check-beahvior-on-switch  [agent:intent user:last_step_result]@step1 [agent:intent user:last_step_result]@step2 [agent:next_behavior=DO **check-beahvior-on-switch**] [agent:intent user:last_step_result]@step3 [agent:intent user:last_step_result]@step4 agent:next_behavior=END

> 该模式其实就是LangChain 的状态机实现


### Fork模式切换

下文介绍

## Fork 创建旁路LLM

旁路LLM可以使用不同的系统提示词+继承历史记录的方式专注于一个特定的任务，任务完成后，只把结果join回主干，最终保障了主干的Context Windows的大小

### 在标准AgentLoop中fork是一次常规的tool call触发

fork前
[system] [user [a-call u-result] [a-call u-result] [a-call u-result] assistant]@round1 [user [a-call u-result] a-call **决定fork**

思路一 重新渲染：把parent context的history，渲染进入tool的user message （目前这种用的比较多，兼容性强）

[tool-system] **[user [a-call u-result] [a-call u-result] [a-call u-result] assistant]@round1 [user [a-call u-result] + tool user** --推理--> assistant message(u-result)

如果旁路压根不渲染任何parent context的记录，这是context window负担最轻的方法，缺点是要求上层对参数的使用要非常精确：

[tool-system] **user tool-params** --推理--> assistant message(u-result)

join后主干：[system] [user [a-call u-result] [a-call u-result] [a-call u-result] assistant]@round1 [user [a-call u-result] a-call u-result
看起来就是一次标准的tool call完成

思路二 直接继承：插入两条消息继续推理 （注意：这是一条反模式）

[system] [user [a-call u-result] [a-call u-result] [a-call u-result] assistant]@round1  [user [a-call u-result] **插入固定assistant message**] **插入固定user message** --推理--> assistant message(u-result)
join后主干 [system] [user [a-call u-result] [a-call u-result] [a-call u-result] assistant]@round1 [user [a-call u-result] a-call u-result
看起来也是一次标准的tool call完成

> 插入 **插入固定assistant message** 这一条通常会带来语义漂移，不好控制


### 在Behavior Loop中,fork是一次特殊的beahvior状态切换

plan
 +->DO->END
 |
 last_do_report
 |
 +->DO->END
 |
 last_do_report
 |
 END

fork前
[PLAN-behavior-system] **plan-beahvior-on-switch** [agent:intent user:last_step_result]@step1 [[a-call u-result] agent:intent user:last_step_result]@step2 agent:next_behavior=DO

fork 后的 sub-ctx 入口是**一条** user message，内部分两段（XML section 或类似形式）：一段是从父继承的 StepRecord history，一段是 on_switch payload：

[DO-behavior-system] [user: \<inherited_steps\>...\</inherited_steps\> \<on_switch\>DO-beahvior-on-switch\</on_switch\>] --多step推理--> agent:next_behavior=END (join)

> sub 看到的是**结构化的 StepRecord history**（thought / observation / next_behavior / self_report 等字段），不是父的 message 序列原样回放。这是 fork 跟"原样继承消息"路线的根本区别。

**继承粒度是个谱，由 fork 模板决定，不是 0/1**：

| 粒度 | 内容 | 适用 |
|---|---|---|
| 全套 StepRecord | thought + observation + actions + action_results + next_behavior + self_report | sub 需要看父的完整决策过程 |
| 只到上次 behavior 边界 | 只当前 behavior 段内的 step | sub 不关心更早阶段 |
| 只 thought + next_behavior | 决策链路骨架，扔掉 tool 噪声 | sub 只要"父为什么走到这一步" |
| 只 self_report | 每段 behavior 的最终交接点 | sub 只要"父做完了什么" |

最薄的"只 self_report"等价于下面"不继承历史"——self_report 本来就是父 behavior 的交接 statement。

如果选择不继承历史记录 fork（所有需要的共享状态都打包进 DO-behavior-system 或 on_switch payload）:
[DO-behavior-system] [user: \<on_switch\>DO-beahvior-on-switch\</on_switch\>] --多step推理--> agent:next_behavior=END (join)

对 sub-behavior 来说，这是 context window 负担最轻的方法，缺点就是要求上层共享状态的使用要非常精确。

join 后的主干：
[PLAN-behavior-system] plan-beahvior-on-switch [agent:intent user:last_step_result]@step1 [[a-call u-result] agent:intent user:last_step_result]@step2 agent:next_behavior=DO
**user:PLAN-beahvior-on-switch(可以包含do-behavior的last report)** , 继续前进

> sub 跑出来的 step 只在内存里，join 时**整段丢弃**，只有 last_report 通过 on_switch payload 流回父。

#### 跟"独立切换首次进入"的形状对照

注意 fork 后的 sub-ctx 形状跟独立切换**首次**进入某 behavior 时几乎一样：

|  | fork sub-ctx 入口 | 独立切换首次进入 |
|---|---|---|
| 形状 | `[DO-system] [user: 继承payload + on_switch]` | `[DO-system] [user: on_switch]` |
| sub 的 step 流向 | 内存，join 时丢弃 | 自己的 `.snap` 落盘 |
| 再次进入 | 每次都是全新 sub | resume 自己之前的 stream |

**结构相近，所有权不同**。读者注意区分。

#### Fork 的隔离边界

fork 的 invariant 只在 **message list 一层**：

- ✅ Message list 隔离：sub 的 step 不进父的 history
- ❌ 文件系统操作：不隔离
- ❌ worklog 事件：不隔离
- ❌ messages_sent（对外发消息）：不隔离
- ❌ session 全局状态：不隔离

也就是说 fork **不是沙箱**。如果想用 sub 做 dry-run，必须自己在 sub-system / 工具集层面构造隔离，waist 不提供。

#### Fork 嵌套

sub-ctx 自己也能 fork 出 sub-sub-ctx，形成调用栈，每层独立 join。栈深通常很浅（1-2 层），语义上不限。


## 结论 专注于开发sub-behavior / llm_* tools

- 这两种模式的本质，都是把这个问题的边界划清楚
- 对于这个 LLM 的 Behavior 模式来讲，它其实跟基于 llm_* tool 开发也差不多，但它可以用到更多的session全局状态.



