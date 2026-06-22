import { buckyos } from 'buckyos'
import { isMockRuntime } from '../runtime'
import { MockDataStore } from '../app/ai-center/mock/store'
import type {
  AIStatus,
  ApiNamespace,
  ApiType,
  LocalModel,
  LogicalNode,
  ModelHealthStatus,
  ModelMetadata,
  PricingMode,
  ProviderConfig,
  ProviderInventory,
  ProviderOrigin,
  ProviderRuntimeType,
  ProviderStatus,
  ProviderType,
  ProviderView,
  RoutePolicy,
  SchedulerProfile,
  GlobalRoutingView,
  StoreSnapshot,
  UsageSummary,
  UsageTrendPoint,
  ValidationResult,
  WizardDraft,
} from '../app/ai-center/mock/types'
import type { AiProviderCard } from './index'

export type {
  AIStatus,
  ApiNamespace,
  ApiType,
  AuthStatus,
  LocalModel,
  LogicalNode,
  ModelMetadata,
  ProviderView,
  ProtocolType,
  ProviderType,
  RoutePolicy,
  RouteTrace,
  GlobalRoutingView,
  StoreSnapshot,
  UsageEvent,
  UsageSummary,
  UsageTrendPoint,
  ValidationResult,
  WizardDraft,
} from '../app/ai-center/mock/types'

const EMPTY_USAGE_SUMMARY: UsageSummary = {
  total_tokens: 0,
  total_requests: 0,
  total_estimated_cost: 0,
  today_tokens: 0,
  this_month_tokens: 0,
  by_api_namespace: {
    llm: 0,
    embedding: 0,
    rerank: 0,
    image: 0,
    vision: 0,
    audio: 0,
    video: 0,
    agent: 0,
  },
  by_provider: {},
  by_model: {},
  by_app: {},
}

const EMPTY_SNAPSHOT: StoreSnapshot = {
  providers: [],
  usageEvents: [],
  routingView: {
    logical_tree: [],
    global_exact_model_weights: {},
    provider_weights: {},
    policy: {},
  },
  routeTraces: [],
  localModels: [],
  aiStatus: {
    state: 'disabled',
    provider_count: 0,
    model_count: 0,
    default_routing_ok: false,
    health_counts: {
      available: 0,
      degraded: 0,
      unavailable: 0,
    },
    quota_warnings: 0,
    inventory_ok: false,
  },
}

type Listener = () => void

type RawRecord = Record<string, unknown>

interface AiccRpcClient {
  call(method: string, params: Record<string, unknown>): Promise<unknown>
}

interface RawModelDirectory {
  providers?: RawProviderInventory[]
  directory?: Record<string, Record<string, RawLogicalRouteItem>>
  logical_definitions?: RawLogicalDefinition[]
  routing_settings?: RawRoutingSettings
  aliases?: unknown[]
}

interface RawProviderInventory {
  provider_instance_name?: unknown
  provider_type?: unknown
  provider_driver?: unknown
  provider_origin?: unknown
  provider_type_revision?: unknown
  version?: unknown
  inventory_revision?: unknown
  models?: RawModelMetadata[]
}

interface RawModelMetadata {
  provider_model_id?: unknown
  provider_actual_model_id?: unknown
  provider_options?: unknown
  exact_model?: unknown
  model_driver?: unknown
  parameter_scale?: unknown
  api_types?: unknown
  logical_mounts?: unknown
  capabilities?: unknown
  attributes?: unknown
  pricing?: unknown
  health?: unknown
  quota?: unknown
}

interface RawLogicalRouteItem {
  target?: unknown
  weight?: unknown
}

interface RawLogicalDefinition {
  path?: unknown
  api_type?: unknown
  min_line?: unknown
  fallback?: unknown
  route_policy?: unknown
  scheduler_profile?: unknown
}

interface RawRoutingSettings {
  global_exact_model_weights?: unknown
  provider_weights?: unknown
  policy?: unknown
  revision?: unknown
}

interface RawUsageAggregate {
  total_requests?: unknown
  input_tokens?: unknown
  output_tokens?: unknown
  total_tokens?: unknown
  request_units?: unknown
}

interface RawUsageGroupedRow {
  group?: Record<string, unknown>
  aggregate?: RawUsageAggregate
}

interface RawUsageBucketedRow {
  bucket_start_ms?: unknown
  group?: Record<string, unknown>
  aggregate?: RawUsageAggregate
}

interface RawUsageEvent {
  event_id?: unknown
  tenant_id?: unknown
  caller_app_id?: unknown
  task_id?: unknown
  capability?: unknown
  request_model?: unknown
  provider_model?: unknown
  input_tokens?: unknown
  output_tokens?: unknown
  total_tokens?: unknown
  request_units?: unknown
  finance_snapshot_json?: unknown
  created_at_ms?: unknown
}

interface RawUsageQueryResponse {
  total?: RawUsageAggregate
  grouped?: RawUsageGroupedRow[]
  buckets?: RawUsageBucketedRow[]
  events?: RawUsageEvent[]
}

interface AiccDataProvider {
  fetchSnapshot(): Promise<StoreSnapshot>
  addProvider(draft: WizardDraft): Promise<void>
  deleteProvider(id: string): Promise<void>
  refreshProviderModels(id: string): Promise<void>
  updateProviderKey(provider: ProviderView, apiKey: string): Promise<void>
  setProviderWeight(providerInstanceName: string, weight: number): Promise<void>
  validateConnection(draft: WizardDraft): Promise<ValidationResult>
  getUsageSummary(): UsageSummary
  getUsageTrend(granularity?: string): UsageTrendPoint[]
}

export interface AICCMgr {
  subscribe(listener: Listener): () => void
  getSnapshot(): StoreSnapshot
  getSnapshotVersion(): number
  refresh(): Promise<void>
  getUsageSummary(): UsageSummary
  getUsageTrend(granularity?: string): UsageTrendPoint[]
  addProvider(draft: WizardDraft): Promise<ProviderView>
  deleteProvider(id: string): Promise<void>
  refreshProviderModels(id: string): Promise<void>
  updateProviderKey(provider: ProviderView, apiKey: string): Promise<void>
  setProviderRoutingWeight(providerInstanceName: string, weight: number): Promise<void>
  validateConnection(draft: WizardDraft): Promise<ValidationResult>
}

export class AICCModelStore implements AICCMgr {
  private readonly provider: AiccDataProvider
  private snapshot: StoreSnapshot
  private snapshotVersion = 0
  private listeners = new Set<Listener>()

  constructor(provider: AiccDataProvider, initialSnapshot = EMPTY_SNAPSHOT) {
    this.provider = provider
    this.snapshot = initialSnapshot
  }

  subscribe = (listener: Listener): (() => void) => {
    this.listeners.add(listener)
    return () => this.listeners.delete(listener)
  }

  getSnapshot = (): StoreSnapshot => this.snapshot

  getSnapshotVersion = (): number => this.snapshotVersion

  async refresh(): Promise<void> {
    this.snapshot = await this.provider.fetchSnapshot()
    this.snapshotVersion++
    this.emit()
  }

  getUsageSummary(): UsageSummary {
    return this.provider.getUsageSummary()
  }

  getUsageTrend(granularity = 'day'): UsageTrendPoint[] {
    return this.provider.getUsageTrend(granularity)
  }

  async addProvider(draft: WizardDraft): Promise<ProviderView> {
    const nextDraft = withProviderInstanceName(draft, this.snapshot)
    await this.provider.addProvider(nextDraft)
    const providerInstanceName = nextDraft.provider_instance_name
    for (let attempt = 0; attempt < 5; attempt += 1) {
      await this.refresh()
      const provider = this.snapshot.providers.find((item) =>
        item.config.provider_instance_name === providerInstanceName,
      )
      if (provider) {
        return provider
      }
      await delay(250)
    }
    throw new Error('aicc.provider_add_not_reflected')
  }

  async deleteProvider(id: string): Promise<void> {
    await this.provider.deleteProvider(id)
    await this.refresh()
  }

  async refreshProviderModels(id: string): Promise<void> {
    await this.provider.refreshProviderModels(id)
    await this.refresh()
  }

  async updateProviderKey(provider: ProviderView, apiKey: string): Promise<void> {
    await this.provider.updateProviderKey(provider, apiKey)
    await this.refresh()
  }

  async setProviderRoutingWeight(providerInstanceName: string, weight: number): Promise<void> {
    await this.provider.setProviderWeight(providerInstanceName, weight)
    await this.refresh()
  }

  validateConnection(draft: WizardDraft): Promise<ValidationResult> {
    return this.provider.validateConnection(draft)
  }

  private emit() {
    this.listeners.forEach((listener) => listener())
  }
}

export function createAICCMgr(options: { useMock?: boolean } = {}): AICCMgr {
  const useMock = options.useMock ?? isMockRuntime()
  if (useMock) {
    const provider = new MockAiccProvider()
    return new AICCModelStore(provider, provider.fetchSnapshotSync())
  }
  return new AICCModelStore(new BuckyOSAiccProvider())
}

class MockAiccProvider implements AiccDataProvider {
  private readonly store = new MockDataStore()

  fetchSnapshotSync(): StoreSnapshot {
    return this.store.getSnapshot()
  }

  async fetchSnapshot(): Promise<StoreSnapshot> {
    return this.store.getSnapshot()
  }

  async addProvider(draft: WizardDraft): Promise<void> {
    this.store.addProvider(draft)
  }

  async deleteProvider(id: string): Promise<void> {
    this.store.deleteProvider(id)
  }

  async refreshProviderModels(id: string): Promise<void> {
    this.store.refreshProviderModels(id)
  }

  async updateProviderKey(provider: ProviderView, _apiKey: string): Promise<void> {
    this.store.updateProviderKey(provider.config.id)
  }

  async setProviderWeight(providerInstanceName: string, weight: number): Promise<void> {
    this.store.setProviderWeight(providerInstanceName, weight)
  }

  async validateConnection(draft: WizardDraft): Promise<ValidationResult> {
    return this.store.validateConnection(draft)
  }

  getUsageSummary(): UsageSummary {
    return this.store.getUsageSummary()
  }

  getUsageTrend(granularity = 'day'): UsageTrendPoint[] {
    return this.store.getUsageTrend(granularity)
  }
}

class BuckyOSAiccProvider implements AiccDataProvider {
  private client: AiccRpcClient | null = null
  private controlPanelClient: AiccRpcClient | null = null
  private usageSummary = EMPTY_USAGE_SUMMARY
  private usageTrend: UsageTrendPoint[] = []

  async fetchSnapshot(): Promise<StoreSnapshot> {
    const [directory, usageByModel, usageByCapability, usageByApp, usageTrend, usageEvents, usageLastDay, usageThisMonth] = await Promise.all([
      this.call<RawModelDirectory>('models.list', {}),
      this.queryUsage({
        time_range: { kind: 'last30d' },
        filters: {},
        group_by: ['provider_model'],
        output_mode: 'summary',
      }),
      this.queryUsage({
        time_range: { kind: 'last30d' },
        filters: {},
        group_by: ['capability'],
        output_mode: 'summary',
      }),
      this.queryUsage({
        time_range: { kind: 'last30d' },
        filters: {},
        group_by: ['caller_app_id'],
        output_mode: 'summary',
      }),
      this.queryUsage({
        time_range: { kind: 'last30d' },
        filters: {},
        time_bucket: 'day',
        output_mode: 'summary',
      }),
      this.queryUsage({
        time_range: { kind: 'last30d' },
        filters: {},
        output_mode: 'events',
      }),
      this.queryUsage({
        time_range: { kind: 'last1d' },
        filters: {},
        output_mode: 'summary',
      }),
      this.queryUsage({
        time_range: currentMonthTimeRange(),
        filters: {},
        output_mode: 'summary',
      }),
    ])
    const convertedUsageEvents = toUsageEvents(usageEvents)
    this.usageSummary = toUsageSummary({
      byModel: usageByModel,
      byCapability: usageByCapability,
      byApp: usageByApp,
      lastDay: usageLastDay,
      thisMonth: usageThisMonth,
      usageEvents: convertedUsageEvents,
    })
    this.usageTrend = toUsageTrend(usageTrend, convertedUsageEvents)
    const rawProviders = Array.isArray(directory.providers) ? directory.providers : []
    return toStoreSnapshot(directory, rawProviders, convertedUsageEvents)
  }

  async addProvider(draft: WizardDraft): Promise<void> {
    const validation = await this.validateConnection(draft)
    if (!validation.auth_valid || !validation.endpoint_reachable) {
      throw new Error(validation.error_details?.[0]?.message ?? validation.errors[0] ?? 'aicc.provider_validation_failed')
    }
    const result = await this.call<{
      ok?: unknown
      reason?: unknown
      error?: unknown
      reload?: { ok?: unknown; error?: unknown; reason?: unknown }
    }>('provider.add', toProviderWritePayload(draft))
    if (result.ok !== true) {
      throw new Error(asNonEmptyString(result.reason, asNonEmptyString(result.error, 'aicc.provider_add_failed')))
    }
    if (result.reload?.ok === false) {
      throw new Error(asNonEmptyString(
        result.reload.error,
        asNonEmptyString(result.reload.reason, 'aicc.provider_reload_failed'),
      ))
    }
  }

  async deleteProvider(id: string): Promise<void> {
    const result = await this.call<{ ok?: unknown; reason?: unknown }>('provider.delete', {
      provider_instance_name: id,
    })
    if (result.ok === false) {
      throw new Error(asNonEmptyString(result.reason, 'aicc.provider_delete_failed'))
    }
  }

  async refreshProviderModels(id: string): Promise<void> {
    await this.call('provider.refresh_models', {
      provider_instance_name: id,
    })
  }

  async updateProviderKey(provider: ProviderView, apiKey: string): Promise<void> {
    const result = await this.callControlPanel<{ provider?: unknown; ok?: unknown; reason?: unknown }>('ai.provider.set', {
      provider: toAiProviderCard(provider),
      api_key: apiKey.trim(),
    })
    if (result.ok === false) {
      throw new Error(asNonEmptyString(result.reason, 'aicc.provider_key_update_failed'))
    }
    const reload = await this.callControlPanel<{ ok?: unknown; result?: { ok?: unknown }; reason?: unknown }>('ai.reload', {})
    if (reload.ok === false || reload.result?.ok === false) {
      throw new Error(asNonEmptyString(reload.reason, 'aicc.provider_reload_failed'))
    }
    await this.refreshProviderModels(provider.config.id)
  }

  async setProviderWeight(providerInstanceName: string, weight: number): Promise<void> {
    const result = await this.callControlPanel<{ ok?: unknown; reason?: unknown }>('ai.provider.weight.set', {
      provider_instance_name: providerInstanceName,
      weight,
    })
    if (result.ok !== true) {
      throw new Error(asNonEmptyString(result.reason, 'aicc.provider_weight_set_failed'))
    }
  }

  async validateConnection(draft: WizardDraft): Promise<ValidationResult> {
    const result = await this.call<Record<string, unknown>>('provider.validate', toProviderWritePayload(draft))
    return {
      endpoint_reachable: asBoolean(result.endpoint_reachable, false),
      auth_valid: asBoolean(result.auth_valid, false),
      models_discovered: toStringArray(result.models_discovered),
      balance_available: asBoolean(result.balance_available, false),
      errors: toStringArray(result.errors),
      error_details: toValidationErrorDetails(result.error_details, result.errors),
    }
  }

  getUsageSummary(): UsageSummary {
    return this.usageSummary
  }

  getUsageTrend(): UsageTrendPoint[] {
    return this.usageTrend
  }

  private async call<T>(method: string, params: Record<string, unknown>): Promise<T> {
    const result = await this.getClient().call(method, params)
    if (!isRecord(result)) {
      throw new Error(`Invalid ${method} response`)
    }
    return result as T
  }

  private async callControlPanel<T>(method: string, params: Record<string, unknown>): Promise<T> {
    const result = await this.getControlPanelClient().call(method, params)
    if (!isRecord(result)) {
      throw new Error(`Invalid ${method} response`)
    }
    return result as T
  }

  private async queryUsage(params: Record<string, unknown>): Promise<RawUsageQueryResponse> {
    try {
      return await this.call<RawUsageQueryResponse>('usage.query', params)
    } catch (error) {
      console.error('aicc.usage.query failed', error)
      return {}
    }
  }

  private getClient(): AiccRpcClient {
    if (!this.client) {
      this.client = buckyos.getServiceRpcClient('aicc') as unknown as AiccRpcClient
    }
    return this.client
  }

  private getControlPanelClient(): AiccRpcClient {
    if (!this.controlPanelClient) {
      this.controlPanelClient = buckyos.getServiceRpcClient('control-panel') as unknown as AiccRpcClient
    }
    return this.controlPanelClient
  }
}

function withProviderInstanceName(draft: WizardDraft, snapshot: StoreSnapshot): WizardDraft {
  if (draft.provider_instance_name?.trim()) return draft
  const providerType = draft.provider_type ?? 'custom'
  const base = defaultProviderInstanceName(providerType, draft.name)
  const used = new Set(snapshot.providers.map((provider) => provider.config.provider_instance_name))
  let candidate = base
  let suffix = 2
  while (used.has(candidate)) {
    candidate = `${base}-${suffix}`
    suffix += 1
  }
  return { ...draft, provider_instance_name: candidate }
}

function delay(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms))
}

function defaultProviderInstanceName(providerType: ProviderType, name: string): string {
  switch (providerType) {
    case 'sn_router': return 'sn-ai-provider-main'
    case 'openai': return 'openai-main'
    case 'anthropic': return 'claude-main'
    case 'google': return 'google-gemini-main'
    case 'openrouter': return 'openrouter-main'
    case 'custom': return `custom-${slugify(name || 'provider')}`
    default: return `${slugify(providerType)}-main`
  }
}

function slugify(value: string): string {
  const slug = value
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
  return slug || 'provider'
}

function defaultProviderEndpoint(providerType: ProviderType | null): string {
  switch (providerType) {
    case 'sn_router': return 'https://sn.buckyos.ai/api/v1/ai/'
    case 'openai': return 'https://api.openai.com/v1'
    case 'anthropic': return 'https://api.anthropic.com/v1'
    case 'google': return 'https://generativelanguage.googleapis.com/v1beta'
    case 'openrouter': return 'https://openrouter.ai/api/v1'
    default: return ''
  }
}

function toAiProviderCard(provider: ProviderView): AiProviderCard {
  const { config, status } = provider
  const defaultModel = status.discovered_models[0]?.provider_model_id ?? ''
  return {
    id: config.provider_instance_name,
    displayName: config.name,
    providerType: config.provider_type,
    status: status.auth_status === 'invalid'
      ? 'needs_setup'
      : status.model_sync_status === 'failed' || status.auth_status === 'expired'
        ? 'degraded'
        : 'healthy',
    endpoint: config.endpoint ?? '',
    authMode: config.auth_mode ?? 'api_key',
    credentialConfigured: true,
    availableModels: status.discovered_models.map((model) => model.provider_model_id),
    capabilities: Array.from(new Set(status.discovered_models.flatMap((model) => model.api_types))),
    defaultModel,
    note: '',
  }
}

function toProviderWritePayload(draft: WizardDraft): Record<string, unknown> {
  const providerType = draft.provider_type ?? 'custom'
  const endpoint = draft.endpoint.trim() || defaultProviderEndpoint(providerType)
  return {
    provider_instance_name: draft.provider_instance_name,
    provider_type: providerType,
    name: draft.name,
    endpoint,
    protocol_type: draft.protocol_type,
    api_key: draft.api_key,
    auto_sync_models: draft.auto_sync_models,
  }
}

function currentMonthTimeRange(): Record<string, unknown> {
  const start = new Date()
  start.setDate(1)
  start.setHours(0, 0, 0, 0)
  return {
    kind: 'explicit',
    start_time_ms: start.getTime(),
    end_time_ms: Date.now(),
  }
}

function toUsageSummary(raw: {
  byModel: RawUsageQueryResponse
  byCapability: RawUsageQueryResponse
  byApp: RawUsageQueryResponse
  lastDay: RawUsageQueryResponse
  thisMonth: RawUsageQueryResponse
  usageEvents: StoreSnapshot['usageEvents']
}): UsageSummary {
  const total = raw.byModel.total ?? {}
  const byModel: Record<string, number> = {}
  const byProvider: Record<string, number> = {}
  for (const row of groupedRows(raw.byModel)) {
    const model = asNonEmptyString(row.group?.provider_model, '')
    if (!model) continue
    const tokens = aggregateTokens(row.aggregate)
    byModel[model] = tokens
    const provider = providerInstanceFromExactModel(model)
    if (provider) {
      byProvider[provider] = (byProvider[provider] ?? 0) + tokens
    }
  }

  const byApiNamespace = emptyApiNamespaceUsage()
  for (const row of groupedRows(raw.byCapability)) {
    const capability = asNonEmptyString(row.group?.capability, '')
    const namespace = capabilityToApiNamespace(capability)
    byApiNamespace[namespace] += aggregateTokens(row.aggregate)
  }

  const byApp: Record<string, number> = {}
  for (const row of groupedRows(raw.byApp)) {
    const app = asNonEmptyString(row.group?.caller_app_id, 'system')
    byApp[app || 'system'] = aggregateTokens(row.aggregate)
  }

  return {
    ...EMPTY_USAGE_SUMMARY,
    by_api_namespace: byApiNamespace,
    total_tokens: aggregateTokens(total),
    total_requests: asNumber(total.total_requests, 0),
    total_estimated_cost: aggregateUsageFinanceCost(raw.usageEvents),
    today_tokens: aggregateTokens(raw.lastDay.total),
    this_month_tokens: aggregateTokens(raw.thisMonth.total),
    by_provider: byProvider,
    by_model: byModel,
    by_app: byApp,
  }
}

function groupedRows(raw: RawUsageQueryResponse): RawUsageGroupedRow[] {
  return Array.isArray(raw.grouped) ? raw.grouped : []
}

function emptyApiNamespaceUsage(): Record<ApiNamespace, number> {
  return {
    llm: 0,
    embedding: 0,
    rerank: 0,
    image: 0,
    vision: 0,
    audio: 0,
    video: 0,
    agent: 0,
  }
}

function capabilityToApiNamespace(value: string): ApiNamespace {
  const lower = value.toLowerCase()
  if (lower.startsWith('embedding')) return 'embedding'
  if (lower.startsWith('rerank')) return 'rerank'
  if (lower.startsWith('image')) return 'image'
  if (lower.startsWith('vision')) return 'vision'
  if (lower.startsWith('audio')) return 'audio'
  if (lower.startsWith('video')) return 'video'
  if (lower.startsWith('agent')) return 'agent'
  return 'llm'
}

function providerInstanceFromExactModel(model: string): string | undefined {
  const at = model.lastIndexOf('@')
  if (at < 0 || at === model.length - 1) return undefined
  return model.slice(at + 1)
}

function toUsageTrend(raw: RawUsageQueryResponse, usageEvents: StoreSnapshot['usageEvents']): UsageTrendPoint[] {
  const buckets = Array.isArray(raw.buckets) ? raw.buckets : []
  const costByDate = usageEvents.reduce<Record<string, number>>((acc, event) => {
    const date = event.timestamp.slice(0, 10)
    acc[date] = (acc[date] ?? 0) + usageFinanceAmount(event)
    return acc
  }, {})

  return buckets.map((bucket) => ({
    timestamp: new Date(asNumber(bucket.bucket_start_ms, 0)).toISOString().slice(0, 10),
    tokens: aggregateTokens(bucket.aggregate),
    estimated_cost: Number((costByDate[new Date(asNumber(bucket.bucket_start_ms, 0)).toISOString().slice(0, 10)] ?? 0).toFixed(4)),
  }))
}

function toUsageEvents(raw: RawUsageQueryResponse): StoreSnapshot['usageEvents'] {
  const events = Array.isArray(raw.events) ? raw.events : []
  return events.map((event) => {
    const providerModel = asNonEmptyString(event.provider_model, '')
    const tokensIn = asOptionalNumber(event.input_tokens)
    const tokensOut = asOptionalNumber(event.output_tokens)
    const tokenEquivalent = asOptionalNumber(event.total_tokens)
      ?? asOptionalNumber(event.request_units)
      ?? ((tokensIn ?? 0) + (tokensOut ?? 0))

    return {
      id: asNonEmptyString(event.event_id, asNonEmptyString(event.task_id, `usage-${asNumber(event.created_at_ms, 0)}`)),
      timestamp: new Date(asNumber(event.created_at_ms, 0)).toISOString(),
      provider_instance_name: providerInstanceFromExactModel(providerModel) ?? 'unknown-provider',
      exact_model: providerModel || 'unknown-model',
      requested_model: asNonEmptyString(event.request_model, providerModel || 'unknown-model'),
      api_type: capabilityToApiType(asNonEmptyString(event.capability, 'llm')),
      tenant_id: asNonEmptyString(event.tenant_id, 'default'),
      app_id: asOptionalString(event.caller_app_id),
      session_id: asOptionalString(event.task_id),
      tokens_in: tokensIn,
      tokens_out: tokensOut,
      token_equivalent: tokenEquivalent,
      estimated_cost: estimatedCost(event.finance_snapshot_json),
      finance_snapshot: toUsageFinanceSnapshot(event.finance_snapshot_json),
      status: 'success',
    }
  })
}

function capabilityToApiType(value: string): ApiType {
  const inferred = inferApiType(value)
  if (inferred) return inferred
  switch (capabilityToApiNamespace(value)) {
    case 'embedding': return 'embedding.text'
    case 'rerank': return 'rerank'
    case 'image': return 'image.txt2img'
    case 'vision': return 'vision.ocr'
    case 'audio': return 'audio.tts'
    case 'video': return 'video.txt2video'
    case 'agent': return 'agent.computer_use'
    case 'llm':
    default: return 'llm'
  }
}

function estimatedCost(value: unknown): number | undefined {
  return toUsageFinanceSnapshot(value)?.amount
}

function toUsageFinanceSnapshot(value: unknown): StoreSnapshot['usageEvents'][number]['finance_snapshot'] {
  const snapshot = asRecord(value)
  const amount = asOptionalNumber(snapshot.amount)
  const currency = asOptionalString(snapshot.currency)
  const providerTraceId = asOptionalString(snapshot.provider_trace_id)

  if (amount == null && currency == null && providerTraceId == null && snapshot.billing == null) {
    return undefined
  }

  return {
    amount,
    currency,
    provider_trace_id: providerTraceId,
    billing: snapshot.billing,
  }
}

function usageFinanceAmount(event: StoreSnapshot['usageEvents'][number]): number {
  return event.finance_snapshot?.amount ?? 0
}

function aggregateUsageFinanceCost(events: StoreSnapshot['usageEvents']): number {
  return Number(events.reduce((sum, event) => sum + usageFinanceAmount(event), 0).toFixed(2))
}

function aggregateTokens(raw?: RawUsageAggregate): number {
  if (!raw) return 0
  return asNumber(raw.total_tokens, 0)
    || asNumber(raw.input_tokens, 0) + asNumber(raw.output_tokens, 0)
    || asNumber(raw.request_units, 0)
}

function toStoreSnapshot(
  directory: RawModelDirectory,
  rawProviders: RawProviderInventory[],
  usageEvents: StoreSnapshot['usageEvents'] = [],
): StoreSnapshot {
  const inventories = rawProviders.map(toProviderInventory)
  const cloudProviders = inventories
    .filter((inventory) => inventory.provider_type !== 'local_inference')
    .map(toProviderView)
  const localModels = inventories
    .filter((inventory) => inventory.provider_type === 'local_inference')
    .flatMap(toLocalModels)
  const models = [
    ...cloudProviders.flatMap((provider) => provider.status.discovered_models),
    ...localModels,
  ]
  const routingView = toGlobalRoutingView(
    directory.routing_settings,
    directory.directory,
    directory.logical_definitions,
    models,
  )

  return {
    providers: cloudProviders,
    usageEvents,
    routingView,
    routeTraces: [],
    localModels,
    aiStatus: computeAIStatus(cloudProviders, localModels, routingView),
  }
}

function toProviderInventory(raw: RawProviderInventory): ProviderInventory {
  const instanceName = asNonEmptyString(raw.provider_instance_name, 'unknown-provider')
  const runtimeType = normalizeRuntimeType(raw.provider_type)
  const origin = normalizeProviderOrigin(raw.provider_origin)
  const driver = asNonEmptyString(raw.provider_driver, inferDriverFromInstance(instanceName))

  return {
    provider_instance_name: instanceName,
    provider_type: runtimeType,
    provider_driver: driver,
    provider_origin: origin,
    version: asOptionalString(raw.version),
    inventory_revision: asOptionalString(raw.inventory_revision),
    models: Array.isArray(raw.models)
      ? raw.models.map((model) => toModelMetadata(model, runtimeType, driver))
      : [],
  }
}

function toProviderView(inventory: ProviderInventory): ProviderView {
  const providerId = inventory.provider_instance_name
  const providerType = inferProviderType(inventory.provider_driver, inventory.provider_instance_name)
  const models = inventory.models
  const hasUnavailable = models.some((model) => model.health.status === 'unavailable')

  const config: ProviderConfig = {
    id: providerId,
    name: providerDisplayName(providerType, inventory.provider_instance_name),
    provider_type: providerType,
    provider_instance_name: inventory.provider_instance_name,
    provider_runtime_type: inventory.provider_type,
    provider_driver: inventory.provider_driver,
    provider_origin: inventory.provider_origin,
    auto_sync_models: true,
    created_at: new Date(0).toISOString(),
  }

  const status: ProviderStatus = {
    provider_id: providerId,
    is_connected: models.length > 0 && !models.every((model) => model.health.status === 'unavailable'),
    auth_status: models.length === 0 ? 'unknown' : hasUnavailable ? 'unknown' : 'ok',
    usage_supported: false,
    balance_supported: providerType === 'sn_router',
    discovered_models: models,
    model_sync_status: models.length > 0 ? 'ok' : 'failed',
  }

  return {
    config,
    inventory,
    status,
    account: {
      provider_instance_name: inventory.provider_instance_name,
      usage_supported: false,
      cost_supported: providerType !== 'sn_router',
      balance_supported: providerType === 'sn_router',
      pricing_mode: providerType === 'sn_router' ? 'free_quota' : inferPricingMode(models),
      balance_unit: providerType === 'sn_router' ? 'credit' : undefined,
      balance_value: providerType === 'sn_router' ? 15 : undefined,
    },
  }
}

function toLocalModels(inventory: ProviderInventory): LocalModel[] {
  return inventory.models.map((model) => ({
    ...model,
    id: model.exact_model,
    name: model.provider_model_id,
    status: model.health.status === 'unavailable' ? 'error' : 'ready',
  }))
}

function toModelMetadata(
  raw: RawModelMetadata,
  providerRuntimeType: ProviderRuntimeType,
  providerDriver: string,
): ModelMetadata {
  const providerModelId = asNonEmptyString(raw.provider_model_id, 'unknown-model')
  const exactModel = asNonEmptyString(raw.exact_model, providerModelId)
  const apiTypes = toApiTypes(raw.api_types)
  const rawHealth = asRecord(raw.health)
  const status = normalizeHealthStatus(
    typeof raw.health === 'string' ? raw.health : rawHealth.status,
  )
  const quotaState = normalizeQuotaState(rawHealth.quota_state ?? raw.quota)
  const capabilities = asRecord(raw.capabilities)
  const attributes = asRecord(raw.attributes)
  const pricing = asRecord(raw.pricing)
  const isLocal = providerRuntimeType === 'local_inference'

  return {
    provider_model_id: providerModelId,
    provider_actual_model_id: asOptionalString(raw.provider_actual_model_id),
    provider_options: raw.provider_options,
    exact_model: exactModel,
    model_driver: asNonEmptyString(raw.model_driver, providerDriver),
    parameter_scale: asOptionalString(raw.parameter_scale),
    api_types: apiTypes,
    logical_mounts: toStringArray(raw.logical_mounts),
    capabilities: {
      streaming: asBoolean(capabilities.streaming, apiTypes.some((type) => type.startsWith('llm.'))),
      tool_call: asBoolean(capabilities.tool_call, false),
      json_schema: asBoolean(capabilities.json_schema, false),
      web_search: asBoolean(capabilities.web_search, false),
      vision: asBoolean(capabilities.vision, apiTypes.some((type) => type.startsWith('vision.'))),
      max_context_tokens: asOptionalNumber(capabilities.max_context_tokens),
      max_output_tokens: asOptionalNumber(capabilities.max_output_tokens),
    },
    attributes: {
      local: asBoolean(attributes.local, isLocal),
      privacy: normalizePrivacy(attributes.privacy, providerRuntimeType),
      quality_score: asOptionalNumber(attributes.quality_score),
      latency_class: normalizeLatencyClass(attributes.latency_class),
      cost_class: normalizeCostClass(attributes.cost_class),
      tier: normalizeTier(attributes.tier),
    },
    pricing: {
      input_token_usd: asOptionalNumber(pricing.input_token_usd),
      output_token_usd: asOptionalNumber(pricing.output_token_usd),
      cache_input_token_usd: asOptionalNumber(pricing.cache_input_token_usd),
    },
    health: {
      status,
      p50_latency_ms: asOptionalNumber(rawHealth.p50_latency_ms),
      p95_latency_ms: asOptionalNumber(rawHealth.p95_latency_ms),
      error_rate_5m: asOptionalNumber(rawHealth.error_rate_5m),
      quota_state: quotaState,
    },
  }
}

function toGlobalRoutingView(
  raw: RawRoutingSettings | undefined,
  directory: RawModelDirectory['directory'],
  logicalDefinitions: RawLogicalDefinition[] | undefined,
  models: ModelMetadata[],
): GlobalRoutingView {
  const modelIndex = buildModelIndex(models)
  const tree = logicalTreeFromDirectory(directory, logicalDefinitions, modelIndex)
  const logicalTree = tree.length > 0 ? tree : logicalTreeFromModels(models)

  return {
    logical_tree: logicalTree,
    global_exact_model_weights: asNumberRecord(raw?.global_exact_model_weights),
    provider_weights: asNumberRecord(raw?.provider_weights),
    policy: toRoutePolicy(raw?.policy),
    revision: asOptionalString(raw?.revision),
  }
}

function logicalTreeFromDirectory(
  directory: RawModelDirectory['directory'],
  logicalDefinitions: RawLogicalDefinition[] | undefined,
  modelIndex: ModelIndex,
): LogicalNode[] {
  if (!directory && !logicalDefinitions?.length) return []
  const rootNodes: LogicalNode[] = []
  const nodesByPath = new Map<string, LogicalNode>()
  const definitionsByPath = buildLogicalDefinitionIndex(logicalDefinitions)

  const ensureDirectoryNode = (path: string): LogicalNode => {
    const existing = nodesByPath.get(path)
    if (existing) return existing

    const parentPath = parentLogicalPath(path)
    const definition = definitionsByPath.get(path)
    const policy = toLogicalDefinitionPolicy(definition)
    const node: LogicalNode = {
      path,
      label: labelFromPath(path),
      level: 'L3',
      api_type: apiTypeFromLogicalDefinition(definition) ?? inferApiType(path),
      exact_model_weights: {},
      fallback: toFallback(definition?.fallback) ?? { mode: 'parent' },
      policy: {
        profile: 'balanced',
        allow_fallback: true,
        runtime_failover: true,
        ...policy,
      },
      resolved_exact_model: resolveModelForPath(path, modelIndex),
      children: [],
    }
    nodesByPath.set(path, node)

    if (parentPath) {
      const parent = ensureDirectoryNode(parentPath)
      parent.children = appendUniqueNode(parent.children ?? [], node)
    } else {
      rootNodes.push(node)
    }

    return node
  }

  const entriesByPath = new Map<string, Record<string, RawLogicalRouteItem>>()
  Object.entries(directory ?? {}).forEach(([path, items]) => {
    entriesByPath.set(path, items)
  })
  for (const path of definitionsByPath.keys()) {
    if (!entriesByPath.has(path)) {
      entriesByPath.set(path, {})
    }
  }

  const entries = Array.from(entriesByPath.entries()).sort(([left], [right]) => {
    const depthDiff = left.split('.').length - right.split('.').length
    return depthDiff !== 0 ? depthDiff : left.localeCompare(right)
  })

  for (const [path, items] of entries) {
    const node = ensureDirectoryNode(path)
    const definition = definitionsByPath.get(path)
    node.items = toLogicalItems(items)
    node.api_type = apiTypeFromLogicalDefinition(definition) ?? inferApiType(path)
    node.fallback = toFallback(definition?.fallback) ?? node.fallback
    node.policy = {
      ...node.policy,
      ...toLogicalDefinitionPolicy(definition),
    }
    node.resolved_exact_model = resolveModelForPath(path, modelIndex)
  }

  for (const [path] of entries) {
    const node = ensureDirectoryNode(path)
    const children = node.children ?? []
    const childPaths = new Set(children.map((child) => child.path))
    const apiType = node.api_type ?? inferApiType(path)

    for (const item of Object.values(node.items ?? {})) {
      if (childPaths.has(item.target)) continue
      children.push(toDirectoryTargetNode(item.target, apiType, modelIndex))
      childPaths.add(item.target)
    }

    for (const model of modelIndex.byMount.get(path) ?? []) {
      if (childPaths.has(model.exact_model)) continue
      children.push(toExactModelNode(model, apiType))
      childPaths.add(model.exact_model)
    }

    node.children = children.sort((left, right) => {
      if (left.level !== right.level) return left.level === 'L1' ? 1 : -1
      return left.path.localeCompare(right.path)
    })
  }

  return rootNodes.sort(compareLogicalRoot)
}

function logicalTreeFromModels(models: ModelMetadata[]): LogicalNode[] {
  const modelIndex = buildModelIndex(models)
  const mountPaths = Array.from(modelIndex.byMount.keys()).sort()
  return mountPaths.map((path) => {
    const apiType = inferApiType(path)
    return toMountNode(path, apiType, modelIndex.byMount.get(path) ?? [])
  })
}

function toMountNode(path: string, apiType: ApiType | undefined, models: ModelMetadata[]): LogicalNode {
  return {
    path,
    label: labelFromPath(path),
    level: 'L2',
    api_type: apiType,
    exact_model_weights: {},
    resolved_exact_model: models[0]?.exact_model,
    children: models.map((model) => toExactModelNode(model, apiType)),
  }
}

function toDirectoryTargetNode(
  target: string,
  apiType: ApiType | undefined,
  modelIndex: ModelIndex,
): LogicalNode {
  const exactModel = modelIndex.byExact.get(target)
  if (exactModel) return toExactModelNode(exactModel, apiType)
  if (target.includes('@')) {
    return {
      path: target,
      label: labelFromPath(target),
      level: 'L1',
      api_type: apiType,
      exact_model_weights: {},
      resolved_exact_model: target,
      locked: true,
    }
  }
  return toMountNode(target, apiType, modelIndex.byMount.get(target) ?? [])
}

function toExactModelNode(model: ModelMetadata, apiType: ApiType | undefined): LogicalNode {
  return {
    path: model.exact_model,
    label: model.provider_model_id,
    level: 'L1',
    api_type: apiType ?? model.api_types[0],
    exact_model_weights: {},
    resolved_exact_model: model.exact_model,
    locked: true,
  }
}

type ModelIndex = {
  byMount: Map<string, ModelMetadata[]>
  byExact: Map<string, ModelMetadata>
}

function buildModelIndex(models: ModelMetadata[]): ModelIndex {
  const byMount = new Map<string, ModelMetadata[]>()
  const byExact = new Map<string, ModelMetadata>()
  for (const model of models) {
    byExact.set(model.exact_model, model)
    for (const mount of model.logical_mounts) {
      const current = byMount.get(mount) ?? []
      current.push(model)
      byMount.set(mount, current)
    }
  }
  return { byMount, byExact }
}

function resolveModelForPath(path: string, modelIndex: ModelIndex): string | undefined {
  return modelIndex.byMount.get(path)?.[0]?.exact_model
}

function buildLogicalDefinitionIndex(
  definitions: RawLogicalDefinition[] | undefined,
): Map<string, RawLogicalDefinition> {
  const result = new Map<string, RawLogicalDefinition>()
  for (const definition of definitions ?? []) {
    const path = asOptionalString(definition.path)
    if (path) {
      result.set(path, definition)
    }
  }
  return result
}

function apiTypeFromLogicalDefinition(definition?: RawLogicalDefinition): ApiType | undefined {
  const value = asOptionalString(definition?.api_type)
  return value && API_TYPES.includes(value as ApiType) ? value as ApiType : undefined
}

function toLogicalDefinitionPolicy(definition?: RawLogicalDefinition): RoutePolicy {
  if (!definition) return {}
  const policy = toRoutePolicy(definition.route_policy)
  const profile = normalizeSchedulerProfile(definition.scheduler_profile) ?? policy.profile
  const required = requiredFeatures(asRecord(definition.min_line))
  return {
    ...policy,
    ...(profile ? { profile } : {}),
    ...(required.length > 0 ? { required_features: required } : {}),
  }
}

function parentLogicalPath(path: string): string | undefined {
  const index = path.lastIndexOf('.')
  return index > 0 ? path.slice(0, index) : undefined
}

function appendUniqueNode(nodes: LogicalNode[], next: LogicalNode): LogicalNode[] {
  if (nodes.some((node) => node.path === next.path)) return nodes
  return [...nodes, next]
}

function compareLogicalRoot(left: LogicalNode, right: LogicalNode): number {
  const leftIndex = LOGICAL_ROOT_ORDER.indexOf(left.path)
  const rightIndex = LOGICAL_ROOT_ORDER.indexOf(right.path)
  if (leftIndex !== rightIndex) {
    if (leftIndex === -1) return 1
    if (rightIndex === -1) return -1
    return leftIndex - rightIndex
  }
  return left.path.localeCompare(right.path)
}

function computeAIStatus(
  providers: ProviderView[],
  localModels: LocalModel[],
  routingView: GlobalRoutingView,
): AIStatus {
  const models = [
    ...providers.flatMap((provider) => provider.status.discovered_models),
    ...localModels,
  ]
  const cloudProviderCount = providers.filter((provider) =>
    provider.config.provider_runtime_type === 'cloud_api' ||
    provider.config.provider_runtime_type === 'proxy_unknown',
  ).length
  const healthCounts: Record<ModelHealthStatus, number> = {
    available: models.filter((model) => model.health.status === 'available').length,
    degraded: models.filter((model) => model.health.status === 'degraded').length,
    unavailable: models.filter((model) => model.health.status === 'unavailable').length,
  }

  return {
    state: cloudProviderCount === 0 && models.length === 0
      ? 'disabled'
      : cloudProviderCount <= 1
        ? 'single_provider'
        : 'multi_provider',
    provider_count: cloudProviderCount,
    model_count: models.length,
    default_routing_ok: routingView.logical_tree.length > 0,
    health_counts: healthCounts,
    quota_warnings: models.filter((model) =>
      model.health.quota_state === 'near_limit' ||
      model.health.quota_state === 'exhausted',
    ).length,
    inventory_ok: providers.every((provider) => provider.status.model_sync_status === 'ok'),
  }
}

function toLogicalItems(raw: unknown): Record<string, { target: string; weight: number }> {
  const items = asRecord(raw)
  return Object.fromEntries(Object.entries(items).map(([key, value]) => {
    const item = asRecord(value)
    return [
      key,
      {
        target: asNonEmptyString(item.target, key),
        weight: asNumber(item.weight, 1),
      },
    ]
  }))
}

function toRoutePolicy(raw: unknown): RoutePolicy {
  const value = asRecord(raw)
  const profileValue = lockedValue(value.profile)
  const required = asRecord(lockedValue(value.required_features))
  return {
    profile: normalizeSchedulerProfile(profileValue),
    local_only: asOptionalBoolean(lockedValue(value.local_only)),
    allow_fallback: asOptionalBoolean(lockedValue(value.allow_fallback)),
    runtime_failover: asOptionalBoolean(lockedValue(value.runtime_failover)),
    required_features: requiredFeatures(required),
    allowed_provider_instances: toStringArray(lockedValue(value.allowed_provider_instances)),
    blocked_provider_instances: toStringArray(lockedValue(value.blocked_provider_instances)),
    max_estimated_cost_usd: asOptionalNumber(lockedValue(value.max_estimated_cost_usd)),
  }
}

function toFallback(raw: unknown): LogicalNode['fallback'] {
  const value = asRecord(raw)
  const mode = asOptionalString(value.mode)
  if (
    mode === 'parent' ||
    mode === 'strict' ||
    mode === 'disabled' ||
    mode === 'target_logical' ||
    mode === 'target_exact'
  ) {
    return {
      mode,
      target: asOptionalString(value.target),
    }
  }
  return undefined
}

function requiredFeatures(raw: RawRecord): string[] {
  const features: string[] = []
  if (raw.streaming === true) features.push('streaming')
  if (raw.tool_call === true) features.push('tool_calling')
  if (raw.json_schema === true) features.push('json_output')
  if (raw.web_search === true) features.push('web_search')
  if (raw.vision === true) features.push('vision')
  return features
}

function inferProviderType(driver: string, instanceName: string): ProviderType {
  const value = `${driver} ${instanceName}`.toLowerCase()
  if (value.includes('sn')) return 'sn_router'
  if (value.includes('openai')) return 'openai'
  if (value.includes('anthropic') || value.includes('claude')) return 'anthropic'
  if (value.includes('google') || value.includes('gimini') || value.includes('gemini')) return 'google'
  if (value.includes('openrouter')) return 'openrouter'
  return 'custom'
}

function inferDriverFromInstance(instanceName: string): string {
  return instanceName.split(/[._-]/)[0] || 'custom'
}

function providerDisplayName(providerType: ProviderType, instanceName: string): string {
  if (providerType === 'sn_router') return 'SN Router'
  if (providerType === 'openai') return 'OpenAI'
  if (providerType === 'anthropic') return 'Anthropic'
  if (providerType === 'google') return 'Google'
  if (providerType === 'openrouter') return 'OpenRouter'
  return labelFromPath(instanceName)
}

function inferPricingMode(models: ModelMetadata[]): PricingMode {
  return models.some((model) =>
    model.pricing.input_token_usd != null ||
    model.pricing.output_token_usd != null,
  ) ? 'per_token' : 'unknown'
}

function inferApiType(path: string): ApiType | undefined {
  const exact = API_TYPES.find((type) => path === type)
  if (exact) return exact
  const known = API_TYPES.find((type) => path.startsWith(`${type}.`))
  if (known) return known
  if (path.startsWith('llm.')) return 'llm'
  if (path.startsWith('embedding.')) return 'embedding.text'
  if (path.startsWith('image.')) return 'image.txt2img'
  if (path.startsWith('vision.')) return 'vision.ocr'
  if (path.startsWith('audio.')) return 'audio.tts'
  if (path.startsWith('video.')) return 'video.txt2video'
  if (path.startsWith('agent.')) return 'agent.computer_use'
  return undefined
}

function labelFromPath(path: string): string {
  return path
    .split(/[.@_-]/)
    .filter(Boolean)
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(' ')
}

function lockedValue(value: unknown): unknown {
  const record = asRecord(value)
  return 'value' in record ? record.value : value
}

function normalizeRuntimeType(value: unknown): ProviderRuntimeType {
  if (value === 'local_inference' || value === 'cloud_api' || value === 'proxy_unknown') {
    return value
  }
  return 'proxy_unknown'
}

function normalizeProviderOrigin(value: unknown): ProviderOrigin {
  if (
    value === 'system_config' ||
    value === 'user_config' ||
    value === 'builtin' ||
    value === 'provider_claimed'
  ) {
    return value
  }
  return 'provider_claimed'
}

function normalizeHealthStatus(value: unknown): ModelHealthStatus {
  if (value === 'degraded' || value === 'unavailable') return value
  return 'available'
}

function normalizeQuotaState(value: unknown): ModelMetadata['health']['quota_state'] {
  if (value === 'normal' || value === 'near_limit' || value === 'exhausted') return value
  return 'unknown'
}

function normalizePrivacy(value: unknown, runtimeType: ProviderRuntimeType): ModelMetadata['attributes']['privacy'] {
  if (
    value === 'local' ||
    value === 'cloud' ||
    value === 'private_safe' ||
    value === 'public_cloud' ||
    value === 'unknown'
  ) {
    return value
  }
  if (runtimeType === 'local_inference') return 'local'
  if (runtimeType === 'proxy_unknown') return 'private_safe'
  return 'public_cloud'
}

function normalizeLatencyClass(value: unknown): ModelMetadata['attributes']['latency_class'] {
  if (value === 'fast' || value === 'normal' || value === 'slow') return value
  return 'unknown'
}

function normalizeCostClass(value: unknown): ModelMetadata['attributes']['cost_class'] {
  if (value === 'low' || value === 'medium' || value === 'high') return value
  return 'unknown'
}

function normalizeTier(value: unknown): ModelMetadata['attributes']['tier'] {
  if (value === 'flagship' || value === 'mid' || value === 'nano') return value
  return undefined
}

function normalizeSchedulerProfile(value: unknown): SchedulerProfile | undefined {
  if (
    value === 'cost_first' ||
    value === 'latency_first' ||
    value === 'quality_first' ||
    value === 'balanced' ||
    value === 'local_first' ||
    value === 'strict_local'
  ) {
    return value
  }
  return undefined
}

function toApiTypes(value: unknown): ApiType[] {
  const apiTypes = toStringArray(value).filter((item): item is ApiType =>
    API_TYPES.includes(item as ApiType),
  )
  return apiTypes.length > 0 ? apiTypes : ['llm']
}

function toStringArray(value: unknown): string[] {
  return Array.isArray(value)
    ? value.filter((item): item is string => typeof item === 'string' && item.trim().length > 0)
    : []
}

function toValidationErrorDetails(
  value: unknown,
  fallbackErrors: unknown,
): ValidationResult['error_details'] {
  if (Array.isArray(value)) {
    const details = value
      .map((item) => {
        const record = asRecord(item)
        const kind = asOptionalString(record.kind)
        const message = asOptionalString(record.message)
        if (
          !message ||
          (kind !== 'endpoint' && kind !== 'auth' && kind !== 'models')
        ) {
          return null
        }
        return { kind, message }
      })
      .filter((item): item is NonNullable<ValidationResult['error_details']>[number] => item !== null)
    if (details.length > 0) return details
  }

  return toStringArray(fallbackErrors).map((message) => {
    const lower = message.toLowerCase()
    const kind = lower.includes('api_key') || lower.includes('auth') || lower.includes('token')
      ? 'auth'
      : lower.includes('endpoint') || lower.includes('url') || lower.includes('connect')
        ? 'endpoint'
        : 'models'
    return { kind, message }
  })
}

function asNumberRecord(value: unknown): Record<string, number> {
  const record = asRecord(value)
  return Object.fromEntries(Object.entries(record)
    .map(([key, item]) => [key, asOptionalNumber(item)])
    .filter((entry): entry is [string, number] => entry[1] != null))
}

function asRecord(value: unknown): RawRecord {
  return isRecord(value) ? value : {}
}

function isRecord(value: unknown): value is RawRecord {
  return value !== null && typeof value === 'object' && !Array.isArray(value)
}

function asNonEmptyString(value: unknown, fallback: string): string {
  return typeof value === 'string' && value.trim() ? value : fallback
}

function asOptionalString(value: unknown): string | undefined {
  return typeof value === 'string' && value.trim() ? value : undefined
}

function asNumber(value: unknown, fallback: number): number {
  return typeof value === 'number' && Number.isFinite(value) ? value : fallback
}

function asOptionalNumber(value: unknown): number | undefined {
  return typeof value === 'number' && Number.isFinite(value) ? value : undefined
}

function asBoolean(value: unknown, fallback: boolean): boolean {
  return typeof value === 'boolean' ? value : fallback
}

function asOptionalBoolean(value: unknown): boolean | undefined {
  return typeof value === 'boolean' ? value : undefined
}

const API_TYPES: ApiType[] = [
  'llm',
  'embedding.text',
  'embedding.multimodal',
  'rerank',
  'image.txt2img',
  'image.img2img',
  'image.inpaint',
  'image.upscale',
  'image.bg_remove',
  'vision.ocr',
  'vision.caption',
  'vision.detect',
  'vision.segment',
  'audio.tts',
  'audio.asr',
  'audio.music',
  'audio.enhance',
  'video.txt2video',
  'video.img2video',
  'video.video2video',
  'video.extend',
  'video.upscale',
  'agent.computer_use',
]

const LOGICAL_ROOT_ORDER = ['llm', 'image', 'audio', 'video', 'embedding', 'rerank', 'agent', 'agent_runtime', 'multimodal']
