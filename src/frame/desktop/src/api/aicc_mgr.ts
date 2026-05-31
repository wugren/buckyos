import { buckyos } from 'buckyos'
import { isMockRuntime } from '../runtime'
import { MockDataStore } from '../app/ai-center/mock/store'
import type {
  AIStatus,
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
  SessionConfig,
  StoreSnapshot,
  UsageSummary,
  UsageTrendPoint,
  ValidationResult,
  WizardDraft,
} from '../app/ai-center/mock/types'

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
  SessionConfig,
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
  sessionConfig: {
    logical_tree: [],
    global_exact_model_weights: {},
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
  aliases?: unknown[]
  session_config?: RawSessionConfig
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
  exact_model?: unknown
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

interface RawSessionConfig {
  inherit?: unknown
  logical_tree?: Record<string, RawLogicalNode>
  global_exact_model_weights?: unknown
  policy?: unknown
  revision?: unknown
  ttl_seconds?: unknown
}

interface RawLogicalNode {
  children?: Record<string, RawLogicalNode>
  items?: Record<string, RawLogicalRouteItem>
  item_overrides?: Record<string, RawLogicalRouteItem>
  exact_model_weights?: unknown
  fallback?: unknown
  policy?: unknown
  route_policy_override?: unknown
  source?: unknown
}

interface AiccDataProvider {
  fetchSnapshot(): Promise<StoreSnapshot>
  addProvider(draft: WizardDraft): Promise<ProviderView>
  deleteProvider(id: string): Promise<void>
  refreshProviderModels(id: string): Promise<void>
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
    const provider = await this.provider.addProvider(draft)
    await this.refresh()
    return provider
  }

  async deleteProvider(id: string): Promise<void> {
    await this.provider.deleteProvider(id)
    await this.refresh()
  }

  async refreshProviderModels(id: string): Promise<void> {
    await this.provider.refreshProviderModels(id)
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

  async addProvider(draft: WizardDraft): Promise<ProviderView> {
    return this.store.addProvider(draft)
  }

  async deleteProvider(id: string): Promise<void> {
    this.store.deleteProvider(id)
  }

  async refreshProviderModels(id: string): Promise<void> {
    this.store.refreshProviderModels(id)
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
  private usageSummary = EMPTY_USAGE_SUMMARY
  private usageTrend: UsageTrendPoint[] = []

  async fetchSnapshot(): Promise<StoreSnapshot> {
    const directory = await this.call<RawModelDirectory>('models.list', {})
    const rawProviders = Array.isArray(directory.providers) ? directory.providers : []
    return toStoreSnapshot(directory, rawProviders)
  }

  async addProvider(draft: WizardDraft): Promise<ProviderView> {
    const validation = await this.validateConnection(draft)
    if (!validation.auth_valid || !validation.endpoint_reachable) {
      throw new Error(validation.errors[0] ?? 'aicc.provider_validation_failed')
    }
    throw new Error('aicc.provider_write_api_unavailable')
  }

  async deleteProvider(): Promise<void> {
    throw new Error('aicc.provider_write_api_unavailable')
  }

  async refreshProviderModels(): Promise<void> {
    await this.call('service.reload_settings', {})
  }

  async validateConnection(draft: WizardDraft): Promise<ValidationResult> {
    const errors: string[] = []
    const providerType = draft.provider_type ?? 'custom'
    const endpointReachable = providerType !== 'custom' || Boolean(draft.endpoint.trim())
    const authValid = providerType === 'sn_router' || Boolean(draft.api_key.trim())

    if (!endpointReachable) {
      errors.push('Endpoint URL is required for custom providers')
    }
    if (!authValid) {
      errors.push('API Key is required')
    }

    return {
      endpoint_reachable: endpointReachable,
      auth_valid: authValid,
      models_discovered: [],
      balance_available: providerType !== 'custom' && authValid,
      errors,
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

  private getClient(): AiccRpcClient {
    if (!this.client) {
      this.client = buckyos.getServiceRpcClient('aicc') as unknown as AiccRpcClient
    }
    return this.client
  }
}

function toStoreSnapshot(directory: RawModelDirectory, rawProviders: RawProviderInventory[]): StoreSnapshot {
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
  const sessionConfig = toSessionConfig(directory.session_config, directory.directory, models)

  return {
    providers: cloudProviders,
    usageEvents: [],
    sessionConfig,
    routeTraces: [],
    localModels,
    aiStatus: computeAIStatus(cloudProviders, localModels, sessionConfig),
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
      ? raw.models.map((model) => toModelMetadata(model, runtimeType))
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

function toModelMetadata(raw: RawModelMetadata, providerRuntimeType: ProviderRuntimeType): ModelMetadata {
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
    exact_model: exactModel,
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

function toSessionConfig(
  raw: RawSessionConfig | undefined,
  directory: RawModelDirectory['directory'],
  models: ModelMetadata[],
): SessionConfig {
  const modelIndex = buildModelIndex(models)
  const tree = raw?.logical_tree
    ? Object.entries(raw.logical_tree).map(([key, node]) => toLogicalNode(key, node, '', modelIndex, true))
    : logicalTreeFromDirectory(directory, modelIndex)
  const logicalTree = tree.length > 0 ? tree : logicalTreeFromModels(models)

  return {
    inherit: asOptionalString(raw?.inherit),
    logical_tree: logicalTree,
    global_exact_model_weights: asNumberRecord(raw?.global_exact_model_weights),
    policy: toRoutePolicy(raw?.policy),
    revision: asOptionalString(raw?.revision),
    ttl_seconds: asOptionalNumber(raw?.ttl_seconds),
  }
}

function toLogicalNode(
  key: string,
  raw: RawLogicalNode,
  parentPath: string,
  modelIndex: ModelIndex,
  treeNode: boolean,
): LogicalNode {
  const path = parentPath ? `${parentPath}.${key}` : key
  const items = toLogicalItems(raw.items ?? raw.item_overrides)
  const children = Object.entries(raw.children ?? {}).map(([childKey, child]) =>
    toLogicalNode(childKey, child, path, modelIndex, true),
  )
  const childPaths = new Set(children.map((child) => child.path))
  const apiType = inferApiType(path)
  const resolvedExactModel = resolveModelForPath(path, modelIndex)

  for (const item of Object.values(items)) {
    if (childPaths.has(item.target)) continue
    const itemModels = modelIndex.byMount.get(item.target) ?? []
    children.push(toMountNode(item.target, apiType, itemModels))
    childPaths.add(item.target)
  }

  const directModels = modelIndex.byMount.get(path) ?? []
  for (const model of directModels) {
    if (childPaths.has(model.exact_model)) continue
    children.push(toExactModelNode(model, apiType))
    childPaths.add(model.exact_model)
  }

  return {
    path,
    label: labelFromPath(path),
    level: treeNode ? 'L3' : 'L2',
    api_type: apiType,
    items,
    exact_model_weights: asNumberRecord(raw.exact_model_weights),
    fallback: toFallback(raw.fallback),
    policy: toRoutePolicy(raw.route_policy_override ?? raw.policy),
    resolved_exact_model: resolvedExactModel,
    locked: isLockedPolicy(raw.policy),
    children,
  }
}

function logicalTreeFromDirectory(
  directory: RawModelDirectory['directory'],
  modelIndex: ModelIndex,
): LogicalNode[] {
  if (!directory) return []
  return Object.entries(directory)
    .sort(([left], [right]) => left.localeCompare(right))
    .map(([path, items]) => {
      const apiType = inferApiType(path)
      const children = Object.values(toLogicalItems(items)).map((item) => {
        const models = modelIndex.byMount.get(item.target) ?? []
        return toMountNode(item.target, apiType, models)
      })

      return {
        path,
        label: labelFromPath(path),
        level: 'L3',
        api_type: apiType,
        items: toLogicalItems(items),
        exact_model_weights: {},
        fallback: { mode: 'parent' },
        policy: { profile: 'balanced', allow_fallback: true, runtime_failover: true },
        resolved_exact_model: resolveModelForPath(path, modelIndex),
        children,
      }
    })
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
}

function buildModelIndex(models: ModelMetadata[]): ModelIndex {
  const byMount = new Map<string, ModelMetadata[]>()
  for (const model of models) {
    for (const mount of model.logical_mounts) {
      const current = byMount.get(mount) ?? []
      current.push(model)
      byMount.set(mount, current)
    }
  }
  return { byMount }
}

function resolveModelForPath(path: string, modelIndex: ModelIndex): string | undefined {
  return modelIndex.byMount.get(path)?.[0]?.exact_model
}

function computeAIStatus(
  providers: ProviderView[],
  localModels: LocalModel[],
  sessionConfig: SessionConfig,
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
    default_routing_ok: sessionConfig.logical_tree.length > 0,
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
  const known = API_TYPES.find((type) => path === type || path.startsWith(`${type}.`))
  if (known) return known
  if (path.startsWith('llm.')) return 'llm.chat'
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

function isLockedPolicy(raw: unknown): boolean {
  const value = asRecord(raw)
  return Object.values(value).some((item) => asRecord(item).locked === true)
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
  return apiTypes.length > 0 ? apiTypes : ['llm.chat']
}

function toStringArray(value: unknown): string[] {
  return Array.isArray(value)
    ? value.filter((item): item is string => typeof item === 'string' && item.trim().length > 0)
    : []
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
  'llm.chat',
  'llm.completion',
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
