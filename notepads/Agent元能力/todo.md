# Agent元能力框架的TODO

站在Agent完整元能力的视角，梳理Todo

## Agent Session Runtime

Agent Session是Agent活着的容器，其历史记录是Agent存在过的证明

> TODO 用process-chain 来重新实现hook point


### UI(Input Route) Session 

UI(Input Route) 都是long life-time session,期望基于历史记录理解Input的真正含义

UI session会积极的update-session-topic

> TODO: 提示词里优化对Notebook 的使用
> TODO: 群聊支持
> TODO: 看到的更多的历史消息记录（其它Session Agent发送给用户的消息）
> TODO: UI Session中用命令处理Approve

### Input Route  

Input本身带有Target Session最好，这样就不用Route
否则: Input -> UI Session -> LLM -Route-> WorkSession

根本性的痛苦之一:LLM Route错了，目前这类错误造成的伤害是最大的

> TODO: UI Session中识别到机械Forward标签
> TODO: 重点优化和Route相关的提示词

### Work(Task) Session

Work(Task) Session 通常专注于一个确定具体的任务(不是long life-time session)，open DAN的世界观里，Agent应该由大量的worksession和少量的UI Session组成。复杂工作放在Work Session里，能从基本架构上规避Context Windows的问题。

在Plan阶段倾向于“了解更多”状态（有长上下文），并会更新
在DO阶段，专注于完成特定TODO：假设信息已经在Plan阶段收集够，不探索，专注完成目标，快速失败
处理Report
处理workspace与产物交付

> TODO: **Plan阶段的提示词需要基于元能力框架大幅度的升级**
> TODO: Report的范式需要调整

### Agent Notebook + Self-Check

Agent Notebook有2个核心目的

1）处理延迟任务: 
直接处理：Input->LLM->CreateTaskSession->TaskSession完成
延迟处理：Input->LLM->记录Notebook --延迟--> Self-Check:创建计划任务--延迟-->执行计划任务->CraeteTaskSession->TaskSession完成

2）记录被声明的事实

> 分类学讨论结论（坑已踩过，避免重推）：
> - 这两个"目的"是按**消费者**分的（self-check 半 vs 召回半），不是按 itemKind 分的。写入侧其实只有一种 kind：**一条强信号原句**。fact/plan 不是写入时身份，是 self-check 后置、可演化、允许 `unresolved` 的 Review 标注（"做完X再做Y"记下那刻就判不了，必须容忍悬而未决）。
> - **强/弱信号 = 投递保证 = 资源授权，同一根轴**。所以推断（弱信号）不能直接建计划任务——best-effort 投递没资格授权资源开销。弱信号留 Memory，靠"提纯"升级：Memory 高召回候选 + 确认握手做精度门，用户"好"才写入 Notebook。中间态留 Memory，别在 Notebook 里堆未确认脏条目。确认摩擦 ∝ 解锁的下游承诺（同 Governance approval）。
> - 计划类 item 建完任务**标记不删**：防重建 + 当查询锚点（是否建任务/是否执行/成没成）。注意两条正交生命周期别合并——note 自身 `active/stale/superseded`（指令还作不作数） vs 派生任务 `pending/done/failed`（挂 Schedule-Task，查询时投影）。
> - 计划半**不上自动召回**（自寻址、有 self-check 心跳兜底），只保留显式查询；事实/偏好半才走广播式 Hint 召回。

> TODO: 架子已经完成了，需要优化实现

## Global Object (World State)

通过通用的方法(Global Object)定义世界状态,解决Agent主动探索更多信息
需要定义观察/探索/互动/订阅的标准动作
这里的核心是read （返回的必然是一个引导文本（提示词）），要考虑风险

> TODO 建立下面标准抽象：

```text
read(object_id)
read(indexer_id, filter) //用read代替？
call(object_id, method, params)// 工具化？
subscribe(object_id, event_name, options)
unsubscribe(object_id, event_name)
read(alias_or_did_or_url) //用read代替？
```

### 索引支持

Read(indexid) -> 说明查询方法
Read(indexid?query=xxx)->返回查询结果

### 实体支持

Read(obj_did)
Sub(objid,event_name) / Unsub(objid,event_name) ： /objid/event_name
Call(objid,function_name,params)


### 数据(cyfs://)支持

Read(objid) -> 最简单，读取一个对象(如果是容器会返回分页访问的方法） objid是hash

### 工具支持(Agent Tool体系)

tmux session + 意图引擎

> TODO: 支持agent tool索引器，可以了解当前的环境（构造工具的环境+可安装的工具+已有的工具）

## Agent State & Hint Recall

定义Agent的内部状态

定义Recall的时机，Recall的结果是Hint 通过Hint的适当披露解决Agent Session里"不知道自己不知道“和塞入一大堆无用信息占领Context Window的问题.在Session中通过update-session-topic + 半订阅recall

### Hint Recall

Hint = 时间 + 一句话 + 对象ID

固定下现在支持的hint架构，并打通从hint->事实的路径
- AgentSession
- Global Object
	
确认基于tag的机械召回路径
- 有tag的对象的匹配
- 传统的，基于FT5的倒排索引的召回（对象的那些属性会进？）
	
	
确认基于LLM的半订阅调用（目前缺失的一环），触发边界在哪？订阅太多触发？能否机械的判断当前session topic应该订阅哪些global object?

### Agent State Self Improve

Stage1: 从Session History中发现Attention Singal
发现事件 :
发现Object（主语或宾语）
探索Object之间的关系

Stage2：整理Attention 热度（世界参与度）
更新 Agent Memory Graph State (G_State): 当前G_State + Attention Signals => 新G_State 

Stage3:
寻找捷径（skills）： 独立流程


搭建框架的主要工作还是完成定时的触发，按照设计，定时触发主要分为以下两个机制：

1. 定时检测与触发
   第一步是用来解析 Session History。系统每 24 小时会进行一次检测，如果 Session History 有更新，那至少会触发一次。
   这类触发一般是凌晨3点进行检查


2. 阈值触发
   当 Session History 的累计量到达一个阈值的时候，也会触发更新。
   注意 Self-Improve所在的Session是特殊Session，其History不会触发Self-Improve


3. 触发后：
    UnImprove Session->Stage1 LLM->Attention Signals

4. 第二阶段是独立触发。也就是说，当第一阶段完成之后，只要满足以下两个条件，第二阶段就可以触发：

- 第一阶段已经完成了消费，并且明确标明它已经完成了一次处理。
- 存在第一阶段的结果。此时，Attention Signal 到 State Miner 的这个阶段就可以触发。
    Attention Signals->Stage2 LLM->Set Memory
    按时间逐个 分析这个时间窗口内的Session Hisotry,强调跨Session的总结


> TODO: 增加一个专门的组件来管理 Attention Signals OK 
> TODO: 补齐 Attention Signal / Session History 管理操作的 bash CLI 支持
>
> 背景：Agent 行为层原则上不继续扩展 action surface，复杂管理操作应通过 Agent Tool bash CLI 暴露给 `exec_bash` 使用。当前 `self_improve_signals` Stage1 需要通过 CLI 完成 session-history 读取、extraction window 管理、Discover* 写入、progress commit；Stage2 也需要通过 CLI list/mark attention signals。现状里这些工具多为 `LLM | ACTION` 或只注册在 live OpenDAN tool manager 中，不能保证 `exec_bash` 命令可用。
>
> 目标：Code Agent 实现完整 CLI/BASH 支持，并回改 Jarvis self-improve 行为配置，使 Stage1/Stage2 主要使用 bash CLI，而不是新增 action。
>
> 需要支持的 CLI 命令：
> - `read_session_history`
> - `commit_session_history_improved`
> - `BeginAttentionSignalExtraction`
> - `CompleteAttentionSignalExtraction`
> - `DiscoverEvent`
> - `DiscoverObjectObservation`
> - `DiscoverRelationship`
> - `ListPendingAttentionSignals`
> - `MarkAttentionSignalConsumed`
>
> 实现要求：
> - 优先复用现有类型和 store：`src/frame/agent_tool/src/agent_attention_signal.rs`、`src/frame/opendan/src/buildin_tool.rs`、`src/frame/opendan/src/round_history.rs`。
> - CLI 可以接受一整个 JSON object 参数，例如 `DiscoverEvent '{"title":"...","phase":"active","evidence":[...],"confidence":0.9}'`；简单命令可以同时支持 `key=value`。
> - 不能只改 prompt。必须让 `exec_bash` 在实际 session PATH 中能找到这些命令并成功 dispatch。
> - 如需给 `agent_tool` CLI registry 增加实现，必须从 `OPENDAN_AGENT_ROOT` / `OPENDAN_SESSION_ID` 推导 root、session、attention_signals store；不能依赖 live `AIAgent` 指针。
> - `read_session_history` 必须能读取 `<agent_root>/sessions/<session_id>/round_history`，并保持 round completeness、`from_already_improved`、`commit_round_index` 语义。
> - `commit_session_history_improved` 必须更新目标 session `.meta/session.json` 中的 already_improved 状态。
> - `BeginAttentionSignalExtraction` / `Discover*` / `CompleteAttentionSignalExtraction` 在 CLI 形态下必须能维护同一次 Stage1 run 的 extraction runtime 状态。可用 session-local runtime 文件保存当前 window，不要依赖进程内全局状态。
> - `ListPendingAttentionSignals` / `MarkAttentionSignalConsumed` 必须对 `<agent_root>/attention_signals` 的 store 生效。
> - 把上述命令加入 session bin 自动链接列表或等价机制，确保 Jarvis 的 `exec_bash` 可以直接调用。
> - 回改 `src/rootfs/bin/buckyos_jarvis/behaviors/self_improve_signals.toml`：`action_whitelist` 只保留 `exec_bash`，prompt 中列出 CLI 命令和 JSON 用法。
> - 同步检查 `src/rootfs/bin/buckyos_jarvis/behaviors/self_improve_set_memory.toml`，如果 Stage2 仍依赖 `ListPendingAttentionSignals` / `MarkAttentionSignalConsumed` action，也改为 CLI。
>
> 验收标准：
> - 在一个 Agent session 的 `exec_bash` 中，所有上述命令都能 `command -v` 找到。
> - 能用 CLI 完成最小 Stage1 流程：read history -> begin extraction -> DiscoverObjectObservation -> complete extraction -> commit progress。
> - 能用 CLI 完成最小 Stage2 管理流程：list pending signals -> mark one consumed。
> - `self_improve_signals.toml` 不再暴露 attention/session-history action。
> - 必须补充或更新 Rust 单元测试，至少覆盖 CLI 参数解析、store 写入、pending list、consumed mark、session history progress commit。
> - 验证命令至少包括：
>   - `cargo test -p agent_tool agent_attention_signal`
>   - `cargo test -p agent_tool_cli_dev`
>   - `cargo test -p opendan behavior_cfg --lib`
>   - 如改动 OpenDAN runtime wiring，再跑相关 `opendan` 单测。
>
> 风险点：
> - 不要把 CLI 支持做成只在 provider tool/action 里可用。
> - 不要让 CLI Discover* 丢失 source/evidence/extraction_window_id。
> - 不要扫描 self_improve session 自己的 history。
> - 不要在 Stage1 直接写 Agent Memory。
> TODO: 如何重点观察skill的两种信号？
    1）使用了skill的session -> 看report
    2) 使用了skills mgr的selector,但没有选中任何skill
> TODO: 需要Agent视角的，对Object进行备注和状态管理的系统 对象观察


## Skills 

> Skills是Agent与世界交互的捷径

安装Skills + 安装Agent Tool 是用户对Agent能力进行扩展的最主要的方法

### skill的标准定义 

skill的分类 
1）信息获取类（比如下载youtube video)
2) 交付类，对于内部系统，不给skill不可能被发现
3）流程类，做成一类事情的捷径
4）范式类：比如如何写一个好的BP，这类skill的存在主要是弥补LLM的不足，容易被LLM的发展给取代

一个（结晶出来的） skill 至少必须有：

- trigger / when_to_use
- procedure
- dependencies / required tools
- pitfalls
- verification
- source_event_ids
- risk_level
- owner_scope
- lifecycle_state
- verification_status

> TODO 需要重新定义skill的格式

### skills的使用 

plan模式进行选择） + report汇报效果

> TODO 需要在标准定义的基础上优化提示词
> TODO 如何在UI Session使用skill?有必要么

### skills的安装 (llm_install_skill)

> TODO 需要实现

### skills的结晶和整理 (improve-skill)

> TODO 需要实现

## Agent的价值观

给自己主动布置什么任务？如何影响找捷径的方法？如何在多个同类事物中做选择
> TODO 暂时不支持

## Governance Runtime (统一信用管理？）

根据理论，似乎是Plan阶段对Do阶段的权限管理？

- 通过硬边界限制能力 （目前）
- 建立信用架构：如何定义？

identity
owner
authorization
capability
risk classification
trust policy
approval policy
value / priority
contract / delegation
audit
