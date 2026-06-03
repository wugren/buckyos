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

### Agent State(Agent的内部状态)


发现事件 : 
发现Object（主语或宾语）
探索Object之间的关系

整理Attention 热度（世界参与度）

当前G_State + Session History => 新状态
G_state的定义

寻找捷径（skills）


> TODO：Agent Memory能表达G_State么？（需要例子）


### Agent State的整理和演化 (Self-Improve)

特殊触发：当Agent Session的未处理History到达一定数量时，尝试触发
按时间逐个 分析这个时间窗口内的Session Hisotry,强调跨Session的总结


> TODO: 增加一个专门的组件来管理 Attention Signals
> TODO: 需要Agent视角的，对Object进行备注和状态管理的系统

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