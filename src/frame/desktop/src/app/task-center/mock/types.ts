/* ── TaskCenter mock types ── */

export type TaskStatus =
  | 'pending'
  | 'running'
  | 'paused'
  | 'completed'
  | 'failed'
  | 'cancelled'

export type TaskType = 'one-time' | 'scheduled' | 'download' | 'sync' | 'install' | 'workflow'

export type TaskSource = 'system' | 'user' | 'agent' | 'app'

export type WorkflowScheduleStatus = 'enabled' | 'paused' | 'archived' | 'error'

export type WorkflowScheduleSpec =
  | {
      kind: 'cron'
      expr: string
      timezone: string
      calendar?: string | null
      start_at?: string | number | null
      end_at?: string | number | null
    }
  | {
      kind: 'once'
      run_at: string | number
      timezone?: string | null
    }
  | {
      kind: 'run_every'
      every_sec: number
      timezone?: string | null
      start_at?: string | number | null
      end_at?: string | number | null
    }

export interface WorkflowScheduleTarget {
  task_type: string
  runner?: string | null
  name_template?: string
  data_template?: Record<string, unknown>
}

export interface WorkflowScheduleTaskPayload extends Record<string, unknown> {
  request?: {
    schedule_id?: string
    name?: string
    status?: WorkflowScheduleStatus
    schedule?: WorkflowScheduleSpec
    target?: WorkflowScheduleTarget
  }
  result?: {
    next_fire_at?: string | number | null
    last_fire_at?: string | number | null
    last_task_id?: string | number | null
    last_run_id?: string | null
    consecutive_failures?: number
    last_error?: unknown
  }
}

export interface Task {
  rootTaskId: string
  taskId: string
  parentTaskId: string | null
  source: TaskSource
  type: TaskType
  status: TaskStatus
  title: string
  summary: string
  createdAt: string
  updatedAt: string
  startedAt: string | null
  endedAt: string | null
  progress: number | null
  schemaType: string | null
  payload: Record<string, unknown>
  children: Task[]
}

export type SystemNotificationAction = 'confirm' | 'dismiss' | 'approve' | 'reject'

export interface SystemNotification {
  id: string
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

export type SystemEventType =
  | 'task_created'
  | 'task_completed'
  | 'task_failed'
  | 'task_cancelled'
  | 'task_milestone'
  | 'notification_created'
  | 'notification_handled'

export interface SystemEvent {
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
