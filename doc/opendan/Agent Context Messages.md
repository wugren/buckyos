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

fork后sub context
[DO-behavior-system]  **user:StepRecord-Historys + DO-beahvior-on-switch** --多step推理--> agent:next_behavior=END (join)

如果选择不继承历史记录fork:(所有的共享状态都加载到DO-behavior-system中了)
[DO-behavior-system] **user:DO-beahvior-on-switch** --多step推理--> agent:next_behavior=END (join)
对sub-behavior来说，这是最context window负担最轻的方法，缺点就是要求上层共享状态的使用要非常精确

join后的主干：
[PLAN-behavior-system] plan-beahvior-on-switch [agent:intent user:last_step_result]@step1 [[a-call u-result] agent:intent user:last_step_result]@step2 agent:next_behavior=DO
**user:PLAN-beahvior-on-switch(可以包含do-behavior的last report)** , 继续前进


## 结论 专注于开发sub-behavior / llm_* tools

- 这两种模式的本质，都是把这个问题的边界划清楚
- 对于这个 LLM 的 Behavior 模式来讲，它其实跟基于 llm_* tool 开发也差不多，但它可以用到更多的session全局状态.



