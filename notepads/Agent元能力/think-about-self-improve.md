下面把前面语音推演整理成一份较完整的设计稿。
其中我把语音里的几个词统一了一下：

* “search” 多数地方应理解为 **session**。
* “self-in-proof / selfing improve” 统一写作 **self-improve**。
* “noolobject” 这里先按 **全局对象 / Object / Entity** 理解。
* `setmemory`、`setnotebook`、`updateSessionTopic`、`readSessionHistory` 保留为接口名风格。

---

# Agent Memory / Notebook / Session Topic / Self-Improve 设计整理

## 1. 核心结论

Agent Memory 不是传统意义上的知识库，也不是用来保存外部世界事实的数据库。

它的核心作用是：

```text
从 Agent 的 Session 记录中，
沉淀出 Agent 自己的内部状态，
并以“线索”的形式帮助未来 Session 召回相关上下文。
```

更准确地说：

```text
Session 是 Agent 与世界发生关系的记录。

Notebook 是 Agent 从这些记录中确认下来的事实性内部状态。

Memory 是 Agent 从这些记录中形成的线索、印象、关注点、关系和技能捷径。
```

整个系统的关键不是“把世界记住”，而是：

```text
让 Agent 知道自己过去参与过什么，
对什么对象形成过什么印象，
哪些历史上下文可能与当前任务有关，
需要时可以回到原始 Session 或外部对象继续读取。
```

---

# 2. 状态模型：外部状态与内部状态

## 2.1 Session 不是状态隔离，只是 LLM Context 隔离

Session 的本质是：

```text
LLM Context 的隔离
```

而不是：

```text
世界状态的隔离
```

如果两个 Session 都能访问同一个文件系统、同一个目录、同一个数据库，那么这些外部状态天然就是跨 Session 共享的。

例如：

```text
Session A 修改 /workspace/todo.md
Session B 再读取 /workspace/todo.md
```

Session B 看到的一定是修改后的文件。

所以：

```text
世界状态天然可以跨 Session 共享。
```

Memory / Notebook 要解决的不是这种外部世界状态共享问题。

它们要解决的是：

```text
Agent 自身内部状态如何跨 Session 延续。
```

---

## 2.2 外部状态

外部状态包括：

```text
文件系统
数据库
网页
邮件
日历
对象系统
项目状态
公开事实
实时数据
```

外部状态的特点是：

```text
天然存在于世界中
可以通过工具重新观察
会随时间变化
不能仅凭 Memory 作为事实依据
```

例如：

```text
某一年的超级碗冠军是谁
某家公司当前 CEO 是谁
某个网站上的价格是多少
某个文件当前内容是什么
```

这些都不应该被 Memory 当作事实保存。

需要时，Agent 应该通过正式工具重新读取、搜索、查询。

---

## 2.3 内部状态

内部状态是 Agent 自己形成的状态，包括：

```text
Agent 认为哪些对象重要
Agent 过去和哪些对象发生过关系
Agent 对某个人、项目、任务的印象
Agent 观察到的对象之间的关系
Agent 从重复任务中提炼出来的方法
Agent 对某类信息的更高效观察方式
```

这类状态不会天然存在于文件系统或网页里。

因此需要：

```text
Notebook
Memory
Session Topic
Self-Improve
```

共同维护。

---

# 3. Notebook 与 Memory 的区别

## 3.1 Notebook：记录稳定事实

Notebook 记录的是：

```text
确定的、事实性的、相对稳定的内部状态。
```

例如：

```text
用户正在设计 OpenDAN。
用户关注 Agent Memory 的跨 Session 召回问题。
用户倾向于保留推演过程，而不是只要结论。
```

Notebook 更像：

```text
Agent 的长期事实笔记。
```

它记录的是已经明确成立、可以直接引用的信息。

---

## 3.2 Memory：记录线索，而不是事实

Memory 记录的是：

```text
线索
感觉
印象
正在发生的事情
对象关系
可能值得未来召回的上下文入口
```

它不要求保存一个严肃、稳定、完整的事实。

它更像是在说：

```text
这里有一件事情发生过。
这里有一个对象值得关注。
这里有一段历史上下文可能与当前问题相关。
这里有一个更高效的观察或执行方法。
```

Memory 的重点不是回答：

```text
事实到底是什么？
```

而是回答：

```text
哪里可能有相关信息？
我应该去哪个 Session、哪个对象、哪个工具入口继续看？
```

---

## 3.3 一个关键区分

可以这样理解：

```text
Notebook = 已确认事实
Memory   = 可追溯线索
```

或者：

```text
Notebook 保存内容。
Memory 保存入口。
```

更进一步：

```text
Notebook 偏静态。
Memory 偏动态。

Notebook 记录已经成立的事实。
Memory 追踪正在发生、曾经发生、可能相关的事件与关系。
```

---

# 4. 为什么不把 `setMemory` 暴露给常规 UI Session

## 4.1 常规 Session 中不应主动调用 `setMemory`

当前设计不计划把 `setMemory` 暴露给普通 UI Session 的聊天推理过程。

也就是说：

```text
常规推理中，Agent 不应该一边处理用户任务，
一边主动思考“我要不要 setMemory”。
```

原因是：

```text
Memory 记录的是线索，不是当前任务必须输出的事实。
```

如果让普通 Agent 在每轮对话中都主动维护 Memory，会带来几个问题：

```text
增加推理负担
干扰当前任务
让 prompt 复杂化
让 LLM 同时承担任务执行和记忆提炼两种职责
容易产生过度记录或错误记录
```

---

## 4.2 常规 Session 中应该暴露的是 `setNotebook`

普通 UI Session 中可以关注的是：

```text
setNotebook
```

因为 Notebook 记录的是相对明确的事实。

例如用户明确说：

```text
我现在主要在做 Agent Memory 的设计。
```

这类信息可以被记录为事实。

但 Memory 的线索提炼不应该依赖当前对话 Agent 主动完成。

---

## 4.3 Memory 的生成应从主推理流程中分离

当前倾向是：

```text
Memory 不在常规 Session 中直接 set。
Memory 通过 updateSessionTopic 和 self-improve 等机制间接形成。
```

也就是：

```text
UI Session
    -> 专注当前任务
    -> 必要时 setNotebook
    -> 周期性 updateSessionTopic

self-improve
    -> 扫描 Session 历史
    -> 识别对象、关系、印象、技能
    -> 维护 Memory 线索
```

---

# 5. Session 的核心地位

## 5.1 Session 是 Agent 活动的全部记录

Agent 不可能脱离 Session 推理。

因此：

```text
Session 是 Agent 与世界发生关系的记录。
```

Agent 看到什么、参与什么、被要求处理什么、形成什么判断，最终都出现在 Session 中。

所以 Agent 的内部状态，其事实来源必然来自 Session。

---

## 5.2 世界很大，但 Agent 只关心与自己相关的世界

外部世界中同时发生很多事情。

但对某个 Agent 来说，有意义的是：

```text
它参与过的
它观察过的
它被用户要求处理过的
它在 Session 中接触过的
```

因此 Memory / Notebook 并不是复制整个世界，而是记录：

```text
Agent 与世界发生过关系的部分。
```

---

## 5.3 Memory / Notebook 是从 Session 到世界的索引层

Memory 和 Notebook 可以理解为：

```text
Agent 内部状态
到 Session 历史
到外部对象
之间的索引层。
```

尤其是 Memory，它不保存完整历史，而保存：

```text
通向历史和对象的线索。
```

---

# 6. `updateSessionTopic`：轻量记忆入口

## 6.1 `updateSessionTopic` 的作用

`updateSessionTopic` 是当前设计里的一个关键函数。

它不等同于 `setMemory`，但它是 Memory 召回机制的基础入口。

它的核心输出是：

```text
一句话 topic
一组 tags
```

例如：

```text
topic:
讨论 Agent Memory、Notebook、Session Topic 与 Self-Improve 的关系。

tags:
Agent Memory
Notebook
Session Topic
Self-Improve
Cross Session State
OpenDAN
```

---

## 6.2 topic sentence 的作用

一句话 topic 的作用是：

```text
描述当前 Session 在某个时间点正在讨论什么。
```

它类似普通聊天系统里的 Session 标题，但更重要。

因为未来当这个 Session 被另一个 Session 召回时，展示给当前 Agent 的 hint 里就会使用这句话。

也就是说：

```text
topic sentence 决定了这个 Session 被召回时“看起来是什么”。
```

---

## 6.3 tags 的作用

tags 用来做广泛召回。

通过 tags，可以召回：

```text
相关 Session
相关对象
相关项目
相关人物
相关技能
相关历史线索
```

tags 通常来自：

```text
当前讨论的主体
当前事情的类别
项目名
模块名
人物名
抽象主题
```

例如：

```text
Lucy
儿子
Agent Memory
NFL
OpenDAN
Self-Improve
```

---

## 6.4 Session Topic 是动态变化的，但每次更新都会被时间锚定

Session 的 topic 会随着对话变化。

但每次调用 `updateSessionTopic` 时，都会形成一个时间锚点：

```text
timestamp + topic sentence + tags + session_id
```

未来召回时，不是只召回一个最终 Session 标题，而是召回：

```text
某个时间点附近的 Session 状态。
```

然后可以通过这个时间点去读取原始上下文窗口。

---

# 7. Hint 的核心结构：时间 + 一句话 + ID

前面多次推演收敛出一个非常重要的数据结构：

```text
时间 + 一句话 + ID
```

这是每条线索的最小核心结构。

## 7.1 对 Session 线索来说

```text
time:
  这个 topic 出现的时间点

sentence:
  当时 Session 正在讨论什么

id:
  session_id
```

例如：

```text
2026-05-31 10:20
讨论 Agent Memory 与 Notebook 的边界，以及为什么 setMemory 不暴露给 UI Session。
session_id: sess_xxx
```

---

## 7.2 对 Object 线索来说

```text
time:
  这个对象被观察到或提炼出印象的时间点

sentence:
  关于这个对象的一句话描述或印象

id:
  object_id
```

例如：

```text
2026-05-31 10:45
“儿子”在用户上下文中通常指某个特定家庭成员对象。
object_id: person_xxx
```

---

## 7.3 对 Skill 线索来说

```text
time:
  这个方法被提炼出来的时间点

sentence:
  这个技能或捷径的一句话描述

id:
  skill_id
```

例如：

```text
2026-05-31 11:10
查询 NFL 历史数据时，可以优先使用权威 NFL 数据源，而不是每次从搜索引擎重新探索。
skill_id: skill_nfl_lookup
```

---

## 7.4 为什么这个结构重要

因为它同时满足三个目标：

```text
时间
    -> 提供上下文锚点，可回到原始历史窗口。

一句话
    -> 给当前 Agent 一个低成本判断依据。

ID
    -> 提供稳定入口，可继续 read / search / explore。
```

因此：

```text
Memory 不需要一次性注入大量内容。
它只需要注入足够好的线索。
```

---

# 8. 基于 Session Topic 的召回机制

## 8.1 当前 Session 推进一段时间后更新 topic

一个 UI Session 刚开始时，它并不知道自己正在讨论什么。

随着对话推进，系统触发：

```text
updateSessionTopic
```

LLM 生成：

```text
topic sentence
tags
```

---

## 8.2 函数内部机械召回相关 Session

`updateSessionTopic` 内部可以做一个非常机械的动作：

```text
根据 topic 和 tags 搜索相关 Session。
```

因为每个 Session 都有 topic 和 tags，所以可以得到一组候选历史线索：

```text
时间 | 一句话 topic | session_id
```

这组信息可以作为 hint 注入当前 Session。

---

## 8.3 Hint 不是总结，而是线索列表

注入当前 Session 的内容不是完整总结。

而是类似：

```text
可能相关的历史 Session：

1. 2026-05-28 21:30
   讨论 Notebook 与 Memory 的区别。
   session_id: sess_a

2. 2026-05-30 16:10
   讨论 updateSessionTopic 如何生成 tags 并召回对象。
   session_id: sess_b
```

它的作用是告诉当前 Agent：

```text
可能有相关历史，值得时可以继续查。
```

---

## 8.4 当前 Agent 按需读取原始历史

如果当前 Agent 判断某条 hint 重要，可以调用：

```text
readSessionHistory(session_id, timestamp, window)
```

读取该时间点附近的原始聊天记录。

这个工具需要支持：

```text
以时间点为锚读取上下文窗口
向前翻页
向后翻页
扩大窗口
继续探索
```

这样形成渐进式召回：

```text
轻量 hint
    -> Agent 判断是否重要
        -> readSessionHistory
            -> 读取原始窗口
                -> 必要时继续翻页
```

---

## 8.5 这种模式不依赖高质量总结

这是一个重要优点。

近期 Session 可能还没有被 self-improve 提炼，也没有形成高质量总结。

但只要它曾经更新过 topic 和 tags，就可以作为线索被召回。

也就是说：

```text
轻量 topic/tag 索引
先于深度总结存在。
```

这让系统即使在没有完整 Memory 提炼的情况下，也具备一定跨 Session 召回能力。

---

# 9. Object / Entity 的召回机制

## 9.1 Tag 不仅召回 Session，也可以召回 Object

当当前上下文出现某些 tag，例如：

```text
Lucy
儿子
OpenDAN
NFL
某个项目名
某个模块名
```

系统不只应该召回相关 Session，也可以召回相关对象。

例如：

```text
当前 Session 提到“儿子”。

系统发现历史里多个 Session 都提到过“儿子”，
并且这个词可能对应一个全局 person 对象。
```

于是 hint 可以告诉 Agent：

```text
这里的“儿子”可能对应 object_id: person_xxx。
```

---

## 9.2 同一对象可以有多个叫法

系统要承认：

```text
同一个对象在不同 Session 中可能有不同称呼。
```

例如：

```text
儿子
孩子
Zihang
小朋友
```

可能都指向同一个对象。

所以 hint 不能只依赖自然语言名字。

它必须提供：

```text
唯一 object_id
```

---

## 9.3 Object Hint 的形式

Object hint 可以是：

```text
一句话描述 + object_id
```

例如：

```text
“儿子”通常指用户的孩子 Zihang。
object_id: person_xxx
```

或者：

```text
“OpenDAN”指用户正在设计的 Personal AI OS / Agent OS 项目。
object_id: project_xxx
```

---

## 9.4 `read(object_id)`：从内部线索到外部对象

当 Agent 获得 object_id 后，可以调用通用 read 函数：

```text
read(object_id)
```

读取对象的：

```text
metadata
当前状态
相关描述
关联历史
可用操作
```

这样完成一个转换：

```text
内部线索
    -> object_id
        -> 外部对象 / 全局对象实时状态
```

---

## 9.5 Memory 不复制对象状态

对象本身可能持续变化。

所以 Memory 不应该保存对象的完整状态。

Memory 应该保存：

```text
对象线索
对象印象
对象 ID
相关 Session 来源
```

对象的实时状态仍然通过：

```text
read(object_id)
```

获取。

---

# 10. Self-Improve：从 Session 中沉淀内部状态

## 10.1 Self-Improve 的根本定位

`self-improve` 是一个独立的 LLM 过程。

它的根本作用是：

```text
从 Agent 的 Session 记录出发，
管理 Agent 的内部状态。
```

它不是当前 UI Session 的一部分。

它是一个后台的、周期性的、反思性的过程。

暂定运行频率：

```text
每天两次。
```

---

## 10.2 Self-Improve 做增量扫描

`self-improve` 不需要每次完整扫描所有 Session。

它可以为每个 Session 保存一个已处理时间点：

```text
last_processed_time
```

每次运行时，只扫描：

```text
last_processed_time 之后的新记录。
```

这样可以同时处理：

```text
冷 Session
热 Session
仍在持续变化的 Session
长期运行的 Session
```

---

## 10.3 Self-Improve 的输入

输入来自：

```text
Agent 的 Session 历史
Session Topic 更新记录
Session tags
Notebook 记录
已有 Memory / Object / Skill 状态
```

它不是直接观察整个外部世界。

它观察的是：

```text
Agent 参与世界留下的记录。
```

---

## 10.4 Self-Improve 的输出

`self-improve` 可以维护几类内部状态：

```text
对象列表
对象印象
对象关系
事件线索
技能线索
观察方法
Notebook 候选事实
Memory 候选线索
```

但在当前 Memory 主题里，重点是：

```text
对象印象
对象之间的关系
技能/观察捷径
可召回线索
```

---

# 11. Self-Improve 关注的是 Agent 视角下的印象

## 11.1 不是客观对象状态

`self-improve` 不应该把自己当成外部对象数据库。

它不负责保存：

```text
对象的客观实时状态。
```

它负责保存：

```text
Agent 通过 Session 观察到的对象印象。
```

---

## 11.2 对象印象

例如：

```text
用户最近持续关注 Agent Memory 设计。
用户对跨 Session 状态共享问题非常敏感。
某个项目最近进入架构收敛阶段。
某个人在某个事件中表现出不满。
```

这些不是外部世界的完整事实，而是：

```text
Agent 在互动过程中形成的观察。
```

---

## 11.3 对象关系

Self-Improve 还可以记录对象之间的关系。

例如：

```text
用户 ↔ OpenDAN
用户 ↔ NFL
用户 ↔ 49ers
用户 ↔ Seahawks
用户A ↔ 用户B
项目A ↔ 模块B
```

这些关系可以是稳定的，也可以是阶段性的。

---

## 11.4 允许矛盾印象并存

现实关系是复杂的。

例如：

```text
在事件 A 中，用户 A 对用户 B 很愤怒。
在事件 B 中，用户 A 对用户 B 很关心。
```

这不应该被强行压缩成单一结论。

Memory 应该允许保留：

```text
不同情境下的不同印象。
```

关键是每条印象都要可追溯：

```text
source_session_id
timestamp
一句话描述
```

---

# 12. Memory 不保存外部事实，但可以保存观察方法

这是一个非常重要的边界。

## 12.1 不应该保存的内容

例如：

```text
2005 年超级碗冠军是谁。
2024 年超级碗冠军是谁。
某个公开数据当前是多少。
某个网页当前写了什么。
```

这些属于外部状态。

不应该进入 Memory，也不应该进入 Notebook。

需要时应该重新查。

---

## 12.2 可以保存的内容

但是可以保存：

```text
用户对 NFL 很感兴趣。
用户过去常问 NFL 相关问题。
查询 NFL 历史数据时，优先使用某个权威数据源。
```

这些属于 Agent 的内部状态或技能状态。

尤其是：

```text
如何更高效地观察某类外部状态。
```

这是一种技能。

---

## 12.3 例子

不保存：

```text
2005 年超级碗冠军是某队。
```

可以保存：

```text
查询超级碗历史冠军时，不必每次从搜索引擎重新探索，
可以优先使用 NFL 官方或权威体育数据源。
```

前者是外部事实。

后者是 Agent 的观察方法。

---

# 13. 技能也是 Memory 的一种沉淀

Self-Improve 除了维护对象印象，还可以提炼技能。

技能包括：

```text
做某类事情的方法
重复任务的捷径
更好的查询入口
更好的观察方式
常见工作流
```

例如：

```text
如何把语音推演整理成设计文档。
如何从 Session Topic 里提取 tags。
如何查 NFL 信息。
如何 review OpenDAN 架构讨论。
```

这类技能也不需要每次完整注入。

只需要以线索形式出现：

```text
这里可能有一个相关技能。
需要时可以 read(skill_id)。
```

---

# 14. 当前推荐的整体工作流

## 14.1 UI Session 中

```text
用户与 Agent 对话
    -> Agent 正常处理当前任务
    -> 必要时 setNotebook 记录明确事实
    -> 定期或按条件触发 updateSessionTopic
```

这里不暴露：

```text
setMemory
```

---

## 14.2 updateSessionTopic 中

```text
读取当前 Session 最近上下文
    -> 生成一句话 topic
    -> 生成 tags
    -> 保存 topic update 记录
    -> 根据 tags 机械召回相关 Session / Object / Skill
    -> 注入轻量 hints
```

Hint 形式：

```text
时间 + 一句话 + ID
```

---

## 14.3 当前 Agent 使用 hints

```text
Agent 看到 hints
    -> 判断是否与当前任务相关
    -> 如果相关：
          readSessionHistory(session_id, timestamp)
          或 read(object_id)
          或 read(skill_id)
    -> 按需深入探索
```

---

## 14.4 Self-Improve 后台运行

```text
每天两次运行
    -> 扫描所有 Session 的增量记录
    -> 识别重要对象
    -> 维护对象别名和 object_id
    -> 形成对象印象
    -> 形成对象关系
    -> 提炼技能
    -> 更新 Memory 线索
    -> 必要时提出 Notebook 候选事实
```

---

# 15. 一个简化架构图

```text
外部世界
  ├── 文件系统
  ├── 数据库
  ├── Web
  ├── 邮件 / 日历
  └── 全局对象系统
        ↑
        │ read / search / query
        │
Agent Session
  ├── 当前对话
  ├── 工具调用记录
  ├── 用户表达
  ├── Agent 推理结果
  └── updateSessionTopic
        ├── topic sentence
        └── tags
              │
              ↓
        轻量召回 Hints
              │
              ├── Session Hint
              │      = time + sentence + session_id
              │
              ├── Object Hint
              │      = time + sentence + object_id
              │
              └── Skill Hint
                     = time + sentence + skill_id

后台 Self-Improve
  ├── 扫描 Session 增量记录
  ├── 维护对象印象
  ├── 维护对象关系
  ├── 提炼技能
  └── 更新 Memory 线索

Notebook
  └── 稳定事实

Memory
  └── 线索 / 印象 / 关系 / 技能入口
```

---

# 16. 关键数据结构草案

## 16.1 SessionTopicUpdate

```ts
type SessionTopicUpdate = {
  id: string
  session_id: string
  timestamp: number

  topic_sentence: string
  tags: string[]

  // 可选：LLM 生成 topic/tag 时的简短理由
  rationale?: string
}
```

---

## 16.2 HintItem

```ts
type HintItem = {
  type: "session" | "object" | "skill" | "relation"

  timestamp: number
  sentence: string

  id: string

  tags?: string[]

  source_session_id?: string
  source_timestamp?: number

  confidence?: number
}
```

核心仍然是：

```text
timestamp + sentence + id
```

---

## 16.3 ObjectMemory

```ts
type ObjectMemory = {
  object_id: string

  aliases: string[]

  one_sentence_description: string

  impressions: ObjectImpression[]

  related_sessions: {
    session_id: string
    timestamp: number
    sentence: string
  }[]
}
```

---

## 16.4 ObjectImpression

```ts
type ObjectImpression = {
  timestamp: number

  sentence: string

  source_session_id: string
  source_timestamp: number

  tags?: string[]

  // 例如：preference, emotion, relation, concern, project_state
  kind?: string

  confidence?: number
}
```

---

## 16.5 RelationMemory

```ts
type RelationMemory = {
  subject_object_id: string
  relation: string
  target_object_id: string

  impressions: {
    timestamp: number
    sentence: string
    source_session_id: string
    source_timestamp: number
  }[]
}
```

---

## 16.6 SkillMemory

```ts
type SkillMemory = {
  skill_id: string

  trigger_tags: string[]

  one_sentence_description: string

  method_hint: string

  sources: {
    session_id: string
    timestamp: number
    sentence: string
  }[]
}
```

---

# 17. 设计原则总结

## 原则一：Memory 不复制世界

```text
外部事实不进 Memory。
外部状态需要时重新观察。
```

---

## 原则二：Session 是一切内部状态的事实来源

```text
Agent 的内部状态必须能追溯到 Session。
```

---

## 原则三：Notebook 记录事实，Memory 记录线索

```text
Notebook = stable fact
Memory   = clue / signal / impression / relation / skill
```

---

## 原则四：常规 UI Session 不直接 setMemory

```text
UI Session 专注任务。
Memory 由 updateSessionTopic 和 self-improve 间接形成。
```

---

## 原则五：召回先轻量，深入按需

```text
先注入 hints。
Agent 判断相关后再 read 原始历史或对象。
```

---

## 原则六：每条线索都应是可追溯入口

```text
时间 + 一句话 + ID
```

是 Memory / Hint 的核心结构。

---

## 原则七：Self-Improve 管理 Agent 的内部沉淀

```text
Self-Improve 不管理世界。
Self-Improve 管理 Agent 对世界参与记录的观察、总结和反思。
```

---

# 18. 当前设计里最关键的三个机制

## 18.1 `updateSessionTopic`

作用：

```text
让当前 Session 知道自己正在讨论什么，
并生成可用于未来召回的 topic/tags。
```

输出：

```text
topic sentence
tags
timestamp
session_id
```

---

## 18.2 `readSessionHistory`

作用：

```text
根据 session_id + timestamp 读取原始聊天窗口。
```

这是 Memory 线索能够回到事实来源的关键。

---

## 18.3 `self-improve`

作用：

```text
周期性扫描 Session，
维护 Agent 的对象印象、对象关系、技能线索和 Memory 索引。
```

它是把 Session 活动沉淀成内部状态的主过程。

---

# 19. 可以形成的一句话定义

可以把整个设计收敛成下面这句话：

```text
Agent Memory 是从 Agent Session 中提炼出的、可追溯到原始上下文和外部对象的内部状态索引；它不保存世界事实，而保存 Agent 对世界参与过程中的线索、印象、关系和技能。
```

更短一点：

```text
Memory 不是知识，Memory 是线索。
Notebook 不是线索，Notebook 是事实。
Session 不是历史垃圾，Session 是 Agent 与世界发生关系的原始记录。
Self-Improve 不是总结器，Self-Improve 是 Agent 内部状态的维护过程。
```

---

# 20. 后续需要进一步明确的问题

下面这些是下一步设计中值得继续收敛的点：

## 20.1 `updateSessionTopic` 的触发条件

需要决定：

```text
按时间触发？
按 token 数触发？
按 topic drift 触发？
按工具调用后触发？
由 Agent 主动触发还是系统自动触发？
```

---

## 20.2 tags 的规范

需要决定：

```text
tag 是自由文本还是受控词表？
是否区分 object tag / topic tag / skill tag？
是否需要 tag alias？
tag 如何合并、去重、降噪？
```

---

## 20.3 hint 的排序与过滤

需要避免 hint 污染当前上下文。

需要定义：

```text
最多注入多少条？
按时间近还是语义相关？
对象 hint 和 session hint 怎么混排？
重复 hint 如何合并？
低置信度 hint 是否展示？
```

---

## 20.4 Object 归一机制

需要解决：

```text
“儿子”“孩子”“Zihang”是否是同一个对象？
“Lucy”是否可能有多个不同人？
缩写、昵称、项目代号如何映射到 object_id？
```

---

## 20.5 Self-Improve 的写入权限

需要明确：

```text
哪些结果可以直接写 Memory？
哪些只能作为候选？
哪些可以写 Notebook？
哪些需要用户确认？
```

---

## 20.6 Memory 的遗忘机制

Memory 是线索，所以应当有生命周期。

需要考虑：

```text
过期
降权
合并
归档
删除
被新观察覆盖
与旧印象并存
```

---

## 20.7 外部状态与内部状态的边界

需要继续明确：

```text
哪些用户偏好属于 Notebook？
哪些阶段性偏好属于 Memory？
哪些事实应该只通过外部工具查？
哪些对象关系可以沉淀为内部状态？
```

---

# 21. 最终抽象

整个设计可以抽象为四层：

```text
第一层：World State
    外部世界状态，通过工具观察。

第二层：Session Log
    Agent 与世界发生关系的原始记录。

第三层：Topic / Hint Index
    用 topic、tags、时间、ID 建立轻量召回入口。

第四层：Internal State
    Notebook 记录事实。
    Memory 记录线索、印象、关系、技能。
```

其中最关键的运行路径是：

```text
Session 产生记录
    -> updateSessionTopic 形成 topic/tags
        -> 当前和未来 Session 可以轻量召回

Session 增量记录
    -> self-improve 后台扫描
        -> 维护对象、关系、印象、技能
            -> 形成 Memory 线索

当前 Session 看到 hint
    -> 按需 readSessionHistory / read(object_id) / read(skill_id)
        -> 回到原始记录或实时对象状态
```

这套机制的核心价值是：

```text
既不把所有历史都塞进上下文，
也不幻想一次性总结出完美记忆；

而是通过“时间 + 一句话 + ID”的线索结构，
让 Agent 能在需要时回到正确的历史位置和对象入口。
```
