# TaskCenter_mgr 需求文档

> 文档状态：v2.0（根据 `src/frame/desktop/src/app/task-center` 实现同步更新）  
> 组件定位：任务中心（Task Center）— 任务跟踪 + 计划任务管理 + 系统事件查看器  
> 最后同步日期：2026-06-15  
> **已确认决策**：系统事件仅保留**生 / 死 / 关键拐点**三类任务事件条目；系统通知与事件均从任务状态派生，不是独立后端对象。

---

## 1. 背景与定位

TaskCenter 是系统级基础应用/组件，用于统一跟踪系统内重要长时间运行任务，以及作为关键系统通知与用户确认的统一入口。

它不是传统意义上的"下载管理器"或"通知中心"，而是一个偏系统基础设施的能力，承担以下职责：

1. 统一展示系统中长期运行任务的状态与结果。
2. 统一展示与管理计划任务（WorkflowSchedule）。
3. 统一承接系统级、严肃型通知与需要用户确认的操作。
4. 为系统内其他应用/模块提供标准的任务查看、状态展示与事件追踪能力。

整体产品定位：
- **任务中心（Task Center）**
- **计划任务管理器（Workflow Schedule Manager）**
- **系统事件查看器（System Event Viewer）**
- **任务详情兜底页 / 标准状态面板**

---

## 2. 设计目标

### 2.1 核心目标

1. **让用户能快速看到"当前最需要关注的任务"**  
   重点是运行中任务、刚失败的任务、刚结束的任务、需要立刻确认的系统通知。

2. **让系统内任意任务都可被统一追踪**  
   无论任务由系统自动发起，还是由用户手工创建，都应该可在 TaskCenter 中查询与查看。

3. **为计划任务提供专属管理视图**  
   计划任务（WorkflowSchedule）有独特的调度状态、触发时间、失败计数等属性，需要独立页面而非混入普通任务列表。

4. **让关键系统通知有统一、可信、严肃的处理入口**  
   TaskCenter 中出现的通知仅代表系统级重要通知，不承载泛滥的应用通知。

5. **控制信息噪音，保证可读性与追踪性平衡**  
   首页强调"现在要处理什么"；任务页强调"任务全量查询"；计划任务页强调"调度配置与触发记录"；系统事件强调"系统内发生过什么"。

### 2.2 非目标

1. 当前版本**不做泛应用通知中心**。
2. 当前版本**不支持以子任务为独立查询对象进行单独浏览**；用户查看的基本对象始终是"根任务"。
3. 当前版本**不要求替代业务应用自己的专属任务页面**；TaskCenter 提供的是统一入口与兜底展示。
4. 当前版本**不以社交/人际消息为主要承载场景**；人与人之间的消息/审批优先放在 MessageHub。
5. **TaskInfo Panel** 当前版本未实现，留在后续演进（见 §16）。

---

## 3. 目标用户与典型场景

### 3.1 目标用户

- 普通用户：查看下载、安装、大型工作流等任务进度与结果。
- 高级用户 / 运维型用户：管理计划任务、查看系统事件、追踪任务生命周期、回看系统通知处理记录。
- 应用/系统开发者：通过统一任务详情页接入任务状态展示能力。

### 3.2 典型场景

1. 用户安装一个体积很大的应用，需要持续跟踪安装状态。
2. 用户运行一个耗时很长的 workflow，需要查看运行进度、阶段变化、失败原因。
3. Agent 在执行过程中触发授权/二次确认（`WaitingForApproval`），需要系统级严肃通知承接。
4. 系统出现关键告警（如磁盘空间只剩 10%），需要用户显式确认已读。
5. 用户需要查看、暂停或归档一批定期执行的工作流计划任务。
6. 用户需要查看历史任务、筛选某类任务、搜索某个任务。

---

## 4. 核心概念定义

### 4.1 任务（Task）

任务是 TaskCenter 的核心对象。一个任务可以包含多个子任务，产品层面的默认查看对象始终是**根任务（Root Task）**。

#### 任务来源（TaskSource）

| 值 | 说明 |
|---|---|
| `system` | 系统自动创建 |
| `user` | 用户手工创建 |
| `agent` | 由 Agent 触发 |
| `app` | 由应用触发 |

#### 任务类型（TaskType）

| 值 | 说明 |
|---|---|
| `one-time` | 一次性任务（默认回落） |
| `scheduled` | 计划任务（`workflow/schedule`） |
| `download` | 下载任务 |
| `sync` | 目录同步任务 |
| `install` | 应用安装任务 |
| `workflow` | Agent / 工作流任务 |

#### 任务状态（TaskStatus）

| 值 | 说明 | 后端原始值 |
|---|---|---|
| `pending` | 等待中 | `Pending`（默认） |
| `running` | 运行中 | `Running` |
| `paused` | 暂停 / 等待用户操作 | `Paused`、`WaitingForApproval` |
| `completed` | 已完成 | `Completed` |
| `failed` | 已失败 | `Failed` |
| `cancelled` | 已取消 | `Canceled`、`Cancelled` |

> **注意**：后端 `WaitingForApproval` 在 UI 层统一映射为 `paused`，并同时在首页生成系统通知卡片（见 §4.2）。

### 4.2 系统通知（System Notification）

系统通知**不是独立的后端对象**，而是由任务状态派生：处于 `WaitingForApproval` 状态的任务会在首页生成一条待处理通知卡片。

通知的 actions：

| action | 说明 |
|---|---|
| `approve` / `confirm` | 同意/确认 |
| `reject` / `dismiss` | 拒绝/忽略 |

用户操作后，前端向后端写回 `human_action: { kind, acted_at, source }` 字段，然后刷新任务列表；通知从首页消失。  
在系统事件页中，已处理的通知以 `notification_handled` 事件类型记录可追溯。

**通知严重程度（severity）**：`info` / `warning` / `critical`  
当前实现中，`WaitingForApproval` 派生的通知默认 severity 为 `warning`。

### 4.3 计划任务（WorkflowSchedule）

计划任务是 `task_type = workflow/schedule` 的特殊任务，有独立的调度元数据和状态体系：

#### 调度状态（WorkflowScheduleStatus）

| 值 | 说明 |
|---|---|
| `enabled` | 活跃，正常调度 |
| `paused` | 已暂停 |
| `error` | 连续失败进入错误状态 |
| `archived` | 已归档（完成或取消） |

调度状态从 TaskStatus 映射：`running → enabled`，`paused → paused`，`failed → error`，`completed/cancelled → archived`。

#### 调度规格（WorkflowScheduleSpec）

三种触发方式，通过 `kind` 字段区分：

| kind | 关键字段 |
|---|---|
| `cron` | `expr`（cron 表达式）、`timezone`、可选 `start_at`/`end_at`/`calendar` |
| `once` | `run_at`（一次性触发时间）、可选 `timezone` |
| `run_every` | `every_sec`（间隔秒数）、可选 `start_at`/`end_at`/`timezone` |

#### 调度目标（WorkflowScheduleTarget）

```
{
  task_type: string        // 触发的任务类型
  runner?: string          // 指定 runner
  name_template?: string   // 任务名模板
  data_template?: object   // 任务数据模板
}
```

#### 计划任务 payload 结构（WorkflowScheduleTaskPayload）

```
{
  request: {
    schedule_id: string         // 计划 ID
    name: string                // 计划名称
    status: WorkflowScheduleStatus
    schedule: WorkflowScheduleSpec
    target: WorkflowScheduleTarget
  }
  result: {
    next_fire_at?: string|number   // 下次触发时间（unix 或 ISO）
    last_fire_at?: string|number   // 上次触发时间
    last_task_id?: string|number   // 最近一次触发的任务 ID
    last_run_id?: string
    consecutive_failures: number   // 连续失败次数
    last_error?: unknown           // 最近一次错误
  }
}
```

### 4.4 系统事件（System Event）

系统事件**不是独立的后端对象**，而是从任务状态快照派生：每个任务根据当前 status 生成一条对应事件条目。

事件类型（SystemEventType）：

| 值 | 对应任务状态 / 来源 |
|---|---|
| `task_created` | `pending` |
| `task_completed` | `completed` |
| `task_failed` | `failed` |
| `task_cancelled` | `cancelled` |
| `task_milestone` | `running`、`paused`（关键拐点） |
| `notification_created` | 通知卡片生成 |
| `notification_handled` | 通知被用户处理 |

事件按 `occurredAt` 倒序排列，系统事件页按日期分组展示。

### 4.5 MessageHub 与 TaskCenter 的边界

- **MessageHub**：偏消息流、沟通流、Agent 发来的消息。
- **TaskCenter**：偏系统级状态、系统通知、需要明确处理动作的严肃入口。

尤其是系统安全拦截或双确认（double confirm）场景，在产品心智上应归入 TaskCenter 的系统通知能力。

---

## 5. 产品信息架构

TaskCenter 由以下**四个主页面**组成，通过响应式 Shell 统一容器承载：

| 页面 | 路径标识 | 定位 |
|---|---|---|
| 首页 | `home` | 当前最需要处理的任务与通知 |
| 任务页 | `tasks` | 全量任务列表，支持筛选与搜索 |
| 计划任务页 | `schedules` | 计划任务专属管理视图 |
| 系统事件页 | `events` | 系统事件时间线，归档与追踪 |

**任务详情页**不是独立导航项，由 `TaskCenterNav.taskId` 控制覆盖当前内容区，可从任务页或计划任务页进入，并根据来源显示对应返回标签。

### 5.1 响应式容器（TaskCenterShell）

- **桌面**：左侧固定侧边栏（Sidebar）+ 右侧内容区（最大宽度 1480px，圆角 28px 卡片式窗口）
- **移动**：顶部 App 标题栏 + 底部 Tab 栏（MobileTabBar）+ 中间内容滚动区

### 5.2 深链接支持

TaskCenter 支持通过 URL 参数直接打开任务详情页：

```
/task-center?taskid=<任意任务ID>
```

传入的 taskId 可以是根任务 ID 或任意子任务 ID，最终展示对应根任务的完整详情页。

---

## 6. 首页需求

### 6.1 定位

首页用于承接"用户现在最需要看到与处理的信息"，强调即时性与优先级，而不是全量信息浏览。

### 6.2 首页内容区块（按优先级从上到下）

1. **运行中的任务**（Running Tasks）
2. **最近完成/失败的任务**（Recently Finished，最多显示 3 条）
3. **待处理系统通知**（System Notifications）
4. **创建任务入口**（Create Task 按钮）

空状态：所有区块均为空时，展示"No active tasks or pending notifications."

### 6.3 任务卡片（TaskCard）

首页任务卡片展示内容：
- 任务状态图标 + 状态标签（带颜色区分）
- 来源（source）
- 任务名称（title）
- 摘要（summary）
- 进度条 + 百分比（若 progress 非空）
- 最近更新时间

点击任务卡片跳转到该任务的详情页。

### 6.4 系统通知卡片（NotificationCard）

来源：status 为 `WaitingForApproval` 的任务自动派生。

通知卡片展示内容：
- 严重程度图标（critical → Shield，其他 → AlertTriangle）+ 颜色区分
- 标题、摘要
- 操作按钮组（`approve` / `reject` 等），直接在首页操作，不需要进入详情页
- 创建时间

操作完成后：通知从首页消失，后台向任务写回 `human_action`，并刷新任务列表。

### 6.5 创建任务入口

首页底部保留 "Create Task" 按钮入口，当前版本为占位（功能待实现）。后续可扩展支持：
- 一次性任务
- 计划任务
- 通过系统扩展注册的手工任务类型（下载、目录同步等）

---

## 7. 完整任务页需求

### 7.1 定位

完整任务页用于查看系统内的**全部根任务**（计划任务除外，计划任务有独立页面），是全量任务管理入口。

> 注意：计划任务（`schemaType === 'workflow/schedule'`）在任务页中仍会出现，但主入口是计划任务页。

### 7.2 列表要求

1. 默认展示全部根任务，按 `updatedAt` 倒序排列。
2. 不支持将子任务作为独立列表项单独浏览。
3. 子任务只在详情页内展开查看。

### 7.3 搜索与筛选

搜索框 + 折叠式筛选面板（Filter 按钮切换显隐）。

#### 搜索字段
任务名称（title）、任务 ID（taskId）、摘要（summary）

#### 过滤维度

| 维度 | 可选值 |
|---|---|
| 状态（status） | `pending / running / paused / completed / failed / cancelled` |
| 类型（type） | `one-time / scheduled / download / sync / install / workflow` |
| 来源（source） | `system / user / agent / app` |

### 7.4 任务列表行（Task Row）

每行展示：
- 状态图标（带颜色）
- 任务名称（truncate）
- 状态标签 + 类型 + 来源
- 运行中/暂停状态下的进度条（max-width 200px）
- 最近更新时间
- 右侧 ChevronRight 指示可点击

### 7.5 任务数量提示

列表顶部显示当前筛选结果数量（`N tasks`）。

---

## 8. 计划任务页需求

### 8.1 定位

计划任务（WorkflowSchedule）独立成页，提供调度管理视角，重点展示调度规格、下次触发时间、触发历史与连续失败状态。

### 8.2 顶部统计摘要条

4 格横向卡片，展示：

| 格 | 指标 | 颜色 |
|---|---|---|
| Total | 全部计划任务数 | 默认文字色 |
| Enabled | 活跃数 | accent（蓝） |
| Paused | 暂停数 | warning（黄） |
| Errors | 连续失败数 | danger（红） |

### 8.3 搜索与筛选

搜索字段：名称（name）、计划 ID（scheduleId）、调度表达式（scheduleText）、目标类型（targetText）、时区（timezone）。

过滤维度：调度状态（`enabled / paused / archived / error`）。

### 8.4 计划任务卡片（ScheduleView）

每张卡片展示：
- 调度状态图标（Play / Pause / Archive / XCircle）+ 状态标签（带颜色）
- 计划名称 + 计划 ID（schedule_id）
- 4 格元信息行：
  - 调度表达式（cron / once / every N）
  - 下次触发时间（next_fire_at，无则显示 "No next fire"）
  - 目标任务类型（task_type · runner）
  - 上次触发时间（last_fire_at，无则显示 "Never fired"）
- 最后错误（last_error，仅 error 状态时显示，红色）
- 底部辅助信息：时区、最近触发任务 ID、连续失败次数

### 8.5 跳转行为

点击计划任务卡片进入任务详情页，backPage 为 `'schedules'`，详情页左上角显示 "Back to Scheduled Tasks"。

---

## 9. 任务详情页需求

### 9.1 定位

任务详情页是所有任务的标准兜底详情页，展示某个任务的完整信息。

### 9.2 入口与返回

- 入口：从任务页进入时，`backPage = 'tasks'`；从计划任务页进入时，`backPage = 'schedules'`。
- 左上角 ArrowLeft + 返回标签文字（根据 backPage 动态显示）。
- 不存在的 taskId：展示 AlertTriangle + "Task not found: {taskId}"。

### 9.3 页面结构

#### 区块一：Header

- 状态图标（40×40 圆角，带色彩背景）
- 任务名称（title）+ 状态标签
- 摘要（summary，若有）

#### 区块二：进度条

若 `progress` 非空，展示全宽进度条 + 百分比。

#### 区块三：错误信息

若 `status === 'failed'` 且 `payload.error` 非空，展示红色背景错误块。

#### 区块四：Task Information（基础信息列表）

| 字段 | 说明 |
|---|---|
| Task ID | taskId |
| Root Task ID | rootTaskId |
| Type | type |
| Source | source |
| Created | createdAt（精确到秒） |
| Started | startedAt |
| Ended | endedAt |
| Updated | updatedAt |
| Schema | schemaType（若非空） |

#### 区块五：Sub-tasks（子任务列表）

若 `children.length > 0`，展示子任务区块：
- 每行：状态图标 + 任务名 + 摘要（可截断）+ 状态标签 + 进度百分比
- 子任务按 `createdAt` 升序排列（构建树时已排序）

#### 区块六：Extended Data（原始 payload 兜底）

若 `payload` 非空对象，以 JSON pretty-print 展示（兜底能力，schema 未知时也能看到原始数据）。

---

## 10. 系统事件页需求

### 10.1 定位

系统事件页是系统事件的完整归档与追踪页，面向低频但高价值的查看场景，偏高级用户、偏系统追踪、偏调试用途。

与"任务页"不同：
- **任务页**关注"有哪些任务"
- **系统事件页**关注"系统里发生过什么"

### 10.2 事件来源

当前版本系统事件**从任务状态派生**，不是独立的后端事件流：

- 每个任务根据当前 status 生成一条事件条目
- 按 `updatedAt` 倒序排列
- 事件 ID 格式：`task-{taskId}-{eventType}`

**注意**：当前实现下，同一任务在不同时刻的状态快照不会产生多条历史事件记录（派生是单次状态映射）；"生 / 死 / 关键拐点"保留策略在独立事件流接入后才真正生效。

### 10.3 时间线展示

事件按 **日期分组**，日期标题行粘性定位于滚动区顶部。每条事件展示：
- 事件类型图标（带颜色背景小方块）
- 事件标题（title）
- 事件类型标签（带颜色） + 来源（source） + 时间
- 事件摘要（summary，若有）
- 若关联任务存在（relatedRootTaskId 非空），点击跳转到该根任务详情页；否则不可点击

### 10.4 搜索与过滤

- 搜索字段：标题（title）、摘要（summary）、来源（source）
- 过滤维度：事件类型（`task_created / task_completed / task_failed / task_cancelled / task_milestone / notification_created / notification_handled`）

### 10.5 任务事件展示策略（已确认）

系统事件中同一任务**保留多条记录**，但只保留：
1. **生**：`task_created`（任务创建/开始）
2. **死**：`task_completed / task_failed / task_cancelled`（任务终态）
3. **关键拐点**：`task_milestone`（等待用户授权、被系统阻塞、进入关键阶段、触发显著告警等）

不进入事件流的噪音示例：高频进度刷新、内部中间态、对用户不可感知的短暂状态切换。

### 10.6 系统通知历史

用户在首页处理过的系统通知以 `notification_handled` 事件类型在系统事件页可追溯：
- 事件标题反映通知内容
- 可关联跳转到对应任务详情页

---

## 11. 交互与行为规则

### 11.1 首页行为

1. 未处理的系统通知保留在首页。
2. 用户完成操作（approve/reject 等）后，通知从首页消失，同步向后端写回 `human_action`，然后刷新任务列表。
3. 写回失败时，本地 `handled` 状态回滚，通知重新出现。
4. 首页不承担历史归档功能，历史统一进入系统事件页。

### 11.2 任务跳转行为

1. 任意页面通过 `?taskid=<ID>` URL 参数可直接打开任务详情页。
2. TaskCenterShell 初始化时若检测到 `initialTaskId`，自动切换到对应任务详情视图。
3. `getTaskById` 支持传入子任务 ID，前端查找到后以该任务的 `rootTaskId` 展示完整根任务详情（当前实现中：`flattenTasks` 展开全量任务树后匹配 taskId，找到哪个就展示哪个的完整信息）。

### 11.3 展示一致性

1. 首页、任务页、计划任务页、系统事件页中的同一任务，status 与颜色体系保持一致。
2. 状态颜色规范：running → accent（蓝）、paused → warning（黄）、completed → success（绿）、failed → danger（红）、其他 → muted（灰）。

---

## 12. 数据模型

### 12.1 Task（任务对象）

```typescript
interface Task {
  rootTaskId: string
  taskId: string
  parentTaskId: string | null
  source: TaskSource           // 'system' | 'user' | 'agent' | 'app'
  type: TaskType               // 'one-time' | 'scheduled' | 'download' | 'sync' | 'install' | 'workflow'
  status: TaskStatus           // 'pending' | 'running' | 'paused' | 'completed' | 'failed' | 'cancelled'
  title: string
  summary: string
  createdAt: string            // ISO 8601
  updatedAt: string
  startedAt: string | null
  endedAt: string | null
  progress: number | null      // 0–100，仅当有意义时非空
  schemaType: string | null    // 'workflow/schedule' 或自定义
  payload: Record<string, unknown>  // 原始扩展数据
  children: Task[]             // 直接子任务，按 createdAt 升序
}
```

### 12.2 SystemNotification（系统通知）

```typescript
type SystemNotificationAction = 'confirm' | 'dismiss' | 'approve' | 'reject'

interface SystemNotification {
  id: string                   // 格式：'task-approval-{taskId}'
  source: 'system'
  title: string
  summary: string
  severity: 'info' | 'warning' | 'critical'
  createdAt: string
  actions: SystemNotificationAction[]
  handled: boolean
  handledAction?: SystemNotificationAction
  handledAt?: string
}
```

### 12.3 SystemEvent（系统事件）

```typescript
type SystemEventType =
  | 'task_created' | 'task_completed' | 'task_failed'
  | 'task_cancelled' | 'task_milestone'
  | 'notification_created' | 'notification_handled'

interface SystemEvent {
  eventId: string
  eventType: SystemEventType
  source: string
  relatedRootTaskId: string | null
  relatedTaskId: string | null
  title: string
  summary: string
  occurredAt: string
  actionState: 'none' | 'handled'
  actionAt: string | null
  payload: Record<string, unknown>
}
```

### 12.4 TaskCenterModel（前端模型接口）

```typescript
interface TaskCenterModel {
  getSnapshot(): number
  subscribe(listener: () => void): () => void
  refresh(): Promise<void>
  getAllTasks(): Task[]
  getRunningTasks(): Task[]           // 排除 workflow/schedule，status in [running, paused]
  getRecentFinishedTasks(): Task[]    // 排除 workflow/schedule，status in terminal
  getScheduledTasks(): Task[]         // schemaType === 'workflow/schedule' 或 type === 'scheduled'
  getTaskById(taskId: string): Task | null  // 在扁平化全量树中查找
  filterTasks(opts: TaskCenterFilter): Task[]
  getPendingNotifications(): SystemNotification[]
  handleNotification(id: string, action: string): void
  getEvents(): SystemEvent[]
}
```

### 12.5 后端 RawTask → Task 转换规则

| RawTask 字段 | 映射到 | 备注 |
|---|---|---|
| `id` | taskId | String 化 |
| `root_id` | rootTaskId | 无时等于 taskId |
| `parent_id` | parentTaskId | null 表示根任务 |
| `task_type` | type | 见 §4.1 类型枚举 |
| `status` | status | 见 §4.1 状态枚举 |
| `progress` | progress | clamp 0–100 |
| `created_at` / `updated_at` | createdAt / updatedAt | unix timestamp → ISO |
| `data.title` 或 `name` | title | 优先 data.title |
| `message` 或 `data.summary` | summary | 优先 message |
| `app_id` / `runner` / `user_id` | source | 推导逻辑见实现 |

时间戳规范：后端 unix 秒级时间戳（< 10,000,000,000）自动转为毫秒；前端统一用 ISO 8601 字符串。

---

## 13. 权限与来源约束

1. **首页中的严肃通知仅允许系统发送**（source 强制为 `'system'`）。
2. 应用层如需通知用户，当前应优先走 MessageHub，未来可走 App Events（非当前版本）。
3. TaskCenter 不应演变为传统移动端那种可被任意应用滥发通知的中心。

---

## 14. 非功能性要求

### 14.1 可扩展性

1. `TaskType` 枚举当前固定，但 `toTaskType()` 规则可按任务 `task_type` 字符串扩展匹配。
2. 计划任务的 payload schema（WorkflowScheduleTaskPayload）当前固定，未来扩展需同步更新 `normalizeSchedulePayload`。
3. 事件类型需要支持扩展，但仍受系统级约束控制。

### 14.2 信息密度控制

1. 首页只保留当前最重要的信息（running + 最近 3 条 finished + pending 通知）。
2. 系统事件对任务状态变化做里程碑压缩（生 / 死 / 关键拐点）。
3. 避免高频状态与噪音事件污染主视图。

### 14.3 兜底可用性

1. 即便任务 schema 未被系统识别，TaskCenter 也必须能够展示其基础信息与状态（payload 原始 JSON 兜底）。
2. 即便业务应用未提供自定义详情页，用户也必须能在 TaskCenter 完成查看与追踪。

### 14.4 React 状态同步

TaskCenterModel 实现 `subscribe + getSnapshot` 接口，通过 `useSyncExternalStore` 集成 React 渲染，确保任何 `emitChange()` 后 UI 自动更新。

---

## 15. 当前实现状态（P0 完成情况）

| 功能 | 状态 |
|---|---|
| 首页：运行中任务展示 | ✅ 已实现 |
| 首页：最近失败/结束任务（top 3） | ✅ 已实现 |
| 首页：系统通知直接处理（approve/reject） | ✅ 已实现，含后端写回与失败回滚 |
| 首页：创建任务入口 | ⚠️ 入口已有，功能待实现 |
| 任务页：全量列表 + 状态/类型/来源过滤器 | ✅ 已实现 |
| 任务页：搜索（名称/ID/摘要） | ✅ 已实现 |
| 计划任务页：独立页面 + 统计摘要条 | ✅ 已实现（PRD 新增） |
| 计划任务页：调度规格展示（cron/once/run_every） | ✅ 已实现 |
| 任务详情页：根任务信息 + 时间字段 | ✅ 已实现 |
| 任务详情页：子任务结构展示 | ✅ 已实现 |
| 任务详情页：Raw payload 兜底 | ✅ 已实现 |
| 任务详情页：深链接（?taskid=） | ✅ 已实现 |
| 系统事件页：时间线（按日期分组） | ✅ 已实现 |
| 系统事件页：事件类型过滤 + 搜索 | ✅ 已实现 |
| 系统事件页：点击跳转到对应任务详情 | ✅ 已实现 |
| 响应式 Shell（桌面侧边栏 + 移动 Tab 栏） | ✅ 已实现 |
| TaskInfo Panel（悬浮任务状态面板） | ❌ 未实现（P1） |
| 创建任务完整流程 | ❌ 未实现（P1） |

---

## 16. 后续演进方向（P1 / P2）

1. **TaskInfo Panel**：供其他页面悬浮嵌入的轻量任务状态面板，支持通过任务 ID / 子任务 ID 打开，并支持跳转完整详情页。
2. **创建任务完整流程**：支持一次性任务、计划任务（含 cron / once / run_every 三种规格）、以及通过系统扩展注册的手工任务类型。
3. **下载 / 同步类任务专属展示器**：在详情页提供更结构化的下载进度、文件列表等展示。
4. **独立后端事件流接入**：当后端提供真实事件流后，系统事件页将切换为消费真实事件，"生 / 死 / 关键拐点"压缩策略在服务端落地。
5. **应用事件（App Events）板块**：偏可读性、信息流风格，承接应用级事件展示。
6. **计划任务管理操作**：暂停/恢复/归档计划任务（当前页面只读）。
7. **更强的事件检索与导出能力**：时间范围筛选、事件导出。

---

## 17. 结论

TaskCenter 是一个系统级"任务中心 + 计划任务管理器 + 系统事件查看器"基础组件。  
当前实现在原 v1 PRD（3页）基础上增加了**计划任务专属页**，形成 4 页导航结构，并落地了完整的数据模型与响应式 Shell。

### 关键产品决策汇总

1. **计划任务独立成页**：WorkflowSchedule 的调度状态、触发时间、失败计数等属性与普通任务差异显著，混入任务列表会降低可用性，独立成第 3 个导航项。
2. **系统通知从任务状态派生**：`WaitingForApproval` 状态的任务自动生成通知卡片，无需独立通知后端；处理动作写回 `human_action` 字段后刷新任务。
3. **系统事件当前为任务状态快照派生**：真实事件流接入前，事件页展示任务当前状态的单条映射；完整的"生 / 死 / 关键拐点"多条记录策略在独立事件流接入后真正生效。
4. **任务树构建**：后端返回扁平 RawTask 列表，前端通过 `parent_id` 在本地构建树结构，根任务为展示基本单元。
