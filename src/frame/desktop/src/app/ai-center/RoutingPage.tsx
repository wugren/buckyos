import { useMemo, useState } from 'react'
import type { ReactNode } from 'react'
import { useMediaQuery } from '@mui/material'
import {
  Activity,
  AlertTriangle,
  Braces,
  ChevronDown,
  ChevronUp,
  Cloud,
  Code2,
  Cpu,
  DollarSign,
  FileText,
  GitBranch,
  Image,
  Layers,
  MessageSquare,
  Route,
  Search,
  SlidersHorizontal,
  Sparkles,
  Zap,
} from 'lucide-react'
import { useI18n } from '../../i18n/provider'
import {
  useLocalModels,
  useProviders,
  useRouteTraces,
  useGlobalRoutingView,
} from './hooks/use-aicc-store'
import { StatusBadge } from './components/shared/StatusBadge'
import type { ApiType, LogicalNode, ModelMetadata, RouteTrace } from '../../api/aicc_mgr'

type FilterKey = 'provider' | 'apiType' | 'capability' | 'cost' | 'latency' | 'health' | 'location'

type RoutingFilters = Record<FilterKey, string>

type ScenarioView = {
  node: LogicalNode
  useCase: UseCaseKind
  title: string
  description: string
  selectedModel?: ModelMetadata
  selectedExactModel?: string
  trace?: RouteTrace
  candidates: ModelMetadata[]
  groups: ModelGroup[]
  score: number
}

type ModelGroup = {
  key: string
  primary: ModelMetadata
  variants: ModelMetadata[]
}

type UseCaseKind = 'chat' | 'code' | 'plan' | 'image' | 'embed' | 'vision' | 'audio' | 'other'

const DEFAULT_FILTERS: RoutingFilters = {
  provider: 'all',
  apiType: 'all',
  capability: 'all',
  cost: 'all',
  latency: 'all',
  health: 'all',
  location: 'all',
}

const USE_CASE_ORDER: UseCaseKind[] = ['chat', 'code', 'plan', 'image', 'embed', 'vision', 'audio', 'other']

export function RoutingPage() {
  const { t } = useI18n()
  const routingView = useGlobalRoutingView()
  const traces = useRouteTraces()
  const providers = useProviders()
  const localModels = useLocalModels()
  const isMobile = useMediaQuery('(max-width: 767px)')
  const [query, setQuery] = useState('')
  const [filters, setFilters] = useState<RoutingFilters>(DEFAULT_FILTERS)
  const [selectedPath, setSelectedPath] = useState<string | null>(null)

  const models = useMemo(() => [
    ...providers.flatMap((provider) => provider.status.discovered_models),
    ...localModels,
  ], [providers, localModels])

  const providerNames = useMemo(() => new Map([
    ...providers.map((provider) => [
      provider.config.provider_instance_name,
      provider.config.name,
    ] as const),
    ['local', t('aiCenter.routing.localProvider', 'Local runtime')] as const,
  ]), [providers, t])

  const scenarios = useMemo(() => buildScenarios(routingView.logical_tree, models, traces), [
    routingView.logical_tree,
    models,
    traces,
  ])
  const filterOptions = useMemo(() => buildFilterOptions(models), [models])
  const visibleScenarios = useMemo(() => {
    const normalizedQuery = query.trim().toLowerCase()
    return scenarios
      .filter((scenario) => scenarioMatchesQuery(scenario, normalizedQuery))
      .filter((scenario) => scenarioMatchesFilters(scenario, filters))
      .sort(compareScenario)
  }, [scenarios, query, filters])
  const selectedScenario = visibleScenarios.find((scenario) => scenario.node.path === selectedPath)
    ?? visibleScenarios[0]

  if (routingView.logical_tree.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center py-16 text-center">
        <GitBranch size={40} style={{ color: 'var(--cp-muted)' }} />
        <p className="text-sm mt-3" style={{ color: 'var(--cp-muted)' }}>
          {t('aiCenter.routing.notConfigured', 'No logical directory configured')}
        </p>
      </div>
    )
  }

  const updateFilter = (key: FilterKey, value: string) => {
    setFilters((current) => ({ ...current, [key]: value }))
  }

  return (
    <div className="flex flex-col gap-4">
      <RoutingHeader revision={routingView.revision} scenarioCount={visibleScenarios.length} />

      <RoutingFiltersBar
        query={query}
        filters={filters}
        options={filterOptions}
        onQueryChange={setQuery}
        onFilterChange={updateFilter}
      />

      <div className={isMobile ? 'flex flex-col gap-4' : 'grid grid-cols-[minmax(0,1.25fr)_minmax(320px,0.75fr)] gap-5 items-start'}>
        <section className="flex flex-col gap-3">
          {visibleScenarios.length > 0 ? visibleScenarios.map((scenario) => (
            <ScenarioCard
              key={scenario.node.path}
              scenario={scenario}
              providerNames={providerNames}
              selected={selectedScenario?.node.path === scenario.node.path}
              onSelect={() => setSelectedPath(scenario.node.path)}
            />
          )) : (
            <EmptyResults />
          )}
        </section>

        <aside className="flex flex-col gap-4">
          {selectedScenario && (
            <ScenarioInspector scenario={selectedScenario} providerNames={providerNames} />
          )}
          <TraceExplorer traces={traces} compact={isMobile} />
        </aside>
      </div>
    </div>
  )
}

function RoutingHeader({ revision, scenarioCount }: { revision?: string; scenarioCount: number }) {
  const { t } = useI18n()
  return (
    <div className="flex flex-col gap-1">
      <h2 className="text-lg font-semibold" style={{ color: 'var(--cp-text)' }}>
        {t('aiCenter.routing.title', 'Routing by Scenario')}
      </h2>
      <p className="text-sm" style={{ color: 'var(--cp-muted)' }}>
        {t('aiCenter.routing.subtitle', 'Read-only view of which model each logical path will prefer now. Variants are folded under their base model.')}
      </p>
      <div className="text-xs" style={{ color: 'var(--cp-muted)' }}>
        {t('aiCenter.routing.revision', 'Revision')}: {revision ?? '-'} / {t('aiCenter.routing.scenarioCount', '{{count}} scenarios', { count: scenarioCount })}
      </div>
    </div>
  )
}

function RoutingFiltersBar({
  query,
  filters,
  options,
  onQueryChange,
  onFilterChange,
}: {
  query: string
  filters: RoutingFilters
  options: Record<FilterKey, string[]>
  onQueryChange: (value: string) => void
  onFilterChange: (key: FilterKey, value: string) => void
}) {
  const { t } = useI18n()
  return (
    <section
      className="rounded-xl p-3 flex flex-col gap-3"
      style={{ background: 'var(--cp-surface)', border: '1px solid var(--cp-border)' }}
    >
      <div className="flex items-center gap-2">
        <Search size={17} style={{ color: 'var(--cp-muted)' }} />
        <input
          value={query}
          onChange={(event) => onQueryChange(event.target.value)}
          placeholder={t('aiCenter.routing.search', 'Search logical path, model, provider, capability...')}
          className="min-h-10 flex-1 rounded-lg px-3 text-sm outline-none"
          style={{ background: 'var(--cp-bg)', color: 'var(--cp-text)', border: '1px solid var(--cp-border)' }}
        />
      </div>

      <div className="flex flex-wrap items-center gap-2">
        <div className="inline-flex items-center gap-1 text-xs" style={{ color: 'var(--cp-muted)' }}>
          <SlidersHorizontal size={14} />
          {t('aiCenter.routing.filters', 'Filters')}
        </div>
        <FilterSelect label={t('aiCenter.routing.provider', 'Provider')} value={filters.provider} options={options.provider} onChange={(value) => onFilterChange('provider', value)} />
        <FilterSelect label={t('aiCenter.routing.apiType', 'API Type')} value={filters.apiType} options={options.apiType} onChange={(value) => onFilterChange('apiType', value)} />
        <FilterSelect label={t('aiCenter.routing.capability', 'Capability')} value={filters.capability} options={options.capability} onChange={(value) => onFilterChange('capability', value)} />
        <FilterSelect label={t('aiCenter.routing.cost', 'Cost')} value={filters.cost} options={options.cost} onChange={(value) => onFilterChange('cost', value)} />
        <FilterSelect label={t('aiCenter.routing.latency', 'Latency')} value={filters.latency} options={options.latency} onChange={(value) => onFilterChange('latency', value)} />
        <FilterSelect label={t('aiCenter.routing.health', 'Health')} value={filters.health} options={options.health} onChange={(value) => onFilterChange('health', value)} />
        <FilterSelect label={t('aiCenter.routing.location', 'Local/Cloud')} value={filters.location} options={options.location} onChange={(value) => onFilterChange('location', value)} />
      </div>
    </section>
  )
}

function FilterSelect({
  label,
  value,
  options,
  onChange,
}: {
  label: string
  value: string
  options: string[]
  onChange: (value: string) => void
}) {
  return (
    <label className="flex items-center gap-1 text-xs" style={{ color: 'var(--cp-muted)' }}>
      <span>{label}</span>
      <select
        value={value}
        onChange={(event) => onChange(event.target.value)}
        className="min-h-8 rounded-md px-2 text-xs outline-none"
        style={{ background: 'var(--cp-bg)', color: 'var(--cp-text)', border: '1px solid var(--cp-border)' }}
      >
        <option value="all">All</option>
        {options.map((option) => (
          <option key={option} value={option}>{option}</option>
        ))}
      </select>
    </label>
  )
}

function ScenarioCard({
  scenario,
  providerNames,
  selected,
  onSelect,
}: {
  scenario: ScenarioView
  providerNames: Map<string, string>
  selected: boolean
  onSelect: () => void
}) {
  const { t } = useI18n()
  const primary = scenario.selectedModel
  const status = primary
    ? primary.health.status === 'available' ? 'ok' : primary.health.status === 'degraded' ? 'warning' : 'error'
    : 'warning'

  return (
    <button
      type="button"
      onClick={onSelect}
      className="w-full rounded-xl p-4 text-left"
      style={{
        background: selected ? 'var(--cp-surface-2)' : 'var(--cp-surface)',
        border: `1px solid ${selected ? 'var(--cp-accent)' : 'var(--cp-border)'}`,
      }}
    >
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0 flex items-start gap-3">
          <UseCaseIcon kind={scenario.useCase} />
          <div className="min-w-0">
            <div className="flex flex-wrap items-center gap-2">
              <h3 className="text-base font-semibold" style={{ color: 'var(--cp-text)' }}>{scenario.title}</h3>
              <span className="text-xs font-mono" style={{ color: 'var(--cp-muted)' }}>{scenario.node.path}</span>
            </div>
            <p className="text-sm mt-1" style={{ color: 'var(--cp-muted)' }}>{scenario.description}</p>
          </div>
        </div>
        <StatusBadge status={status} label={primary?.health.status ?? t('aiCenter.routing.unresolved', 'unresolved')} />
      </div>

      <div className="mt-4 rounded-lg p-3" style={{ background: 'var(--cp-bg)' }}>
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div className="min-w-0">
            <div className="text-xs" style={{ color: 'var(--cp-muted)' }}>
              {t('aiCenter.routing.currentPreferred', 'Current preferred model')}
            </div>
            <div className="text-sm font-medium truncate" style={{ color: 'var(--cp-text)' }}>
              {primary?.provider_model_id ?? scenario.selectedExactModel ?? t('aiCenter.routing.noModel', 'No model resolved')}
            </div>
            <div className="text-xs font-mono truncate" style={{ color: 'var(--cp-muted)' }}>
              {primary ? `${providerNames.get(providerFromExact(primary.exact_model)) ?? providerFromExact(primary.exact_model)} / ${primary.exact_model}` : scenario.node.fallback?.target ?? '-'}
            </div>
          </div>
          {primary && (
            <div className="flex flex-wrap items-center gap-2">
              <MetricChip icon={<Sparkles size={13} />} label="Q" value={formatQuality(primary.attributes.quality_score)} />
              <MetricChip icon={<Zap size={13} />} label="Latency" value={formatLatency(primary)} />
              <MetricChip icon={<DollarSign size={13} />} label="Cost" value={primary.attributes.cost_class} />
              <MetricChip icon={primary.attributes.local ? <Cpu size={13} /> : <Cloud size={13} />} label="Run" value={primary.attributes.local ? 'local' : 'cloud'} />
            </div>
          )}
        </div>
      </div>

      <div className="mt-3 flex flex-wrap items-center gap-2 text-xs" style={{ color: 'var(--cp-muted)' }}>
        <span>{scenario.node.policy?.profile ?? 'balanced'}</span>
        <span>/</span>
        <span>{scenario.node.api_type ?? 'mixed'}</span>
        <span>/</span>
        <span>{scenario.groups.length} {t('aiCenter.routing.baseModels', 'base models')}</span>
        <span>/</span>
        <span>{scenario.groups.reduce((count, group) => count + group.variants.length, 0)} {t('aiCenter.routing.foldedVariants', 'folded variants')}</span>
      </div>
    </button>
  )
}

function ScenarioInspector({
  scenario,
  providerNames,
}: {
  scenario: ScenarioView
  providerNames: Map<string, string>
}) {
  const { t } = useI18n()
  const [expanded, setExpanded] = useState(false)
  const visibleGroups = expanded ? scenario.groups : scenario.groups.slice(0, 4)

  return (
    <section className="rounded-xl p-4" style={{ background: 'var(--cp-surface)', border: '1px solid var(--cp-border)' }}>
      <div className="flex items-start justify-between gap-3 mb-3">
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <UseCaseIcon kind={scenario.useCase} small />
            <h3 className="text-sm font-semibold" style={{ color: 'var(--cp-text)' }}>
              {t('aiCenter.routing.scenarioDetail', 'Scenario Detail')}
            </h3>
          </div>
          <div className="text-xs font-mono truncate mt-1" style={{ color: 'var(--cp-muted)' }}>
            {scenario.node.path}
          </div>
        </div>
        <StatusBadge status={scenario.selectedModel ? 'ok' : 'warning'} label={scenario.selectedModel ? 'resolved' : 'unresolved'} />
      </div>

      <div className="grid grid-cols-1 gap-2 text-sm">
        <Fact label={t('aiCenter.routing.useCase', 'Use case')} value={scenario.title} />
        <Fact label={t('aiCenter.routing.apiType', 'API Type')} value={scenario.node.api_type ?? 'mixed'} />
        <Fact label={t('aiCenter.routing.profile', 'Profile')} value={scenario.node.policy?.profile ?? 'balanced'} />
        <Fact label={t('aiCenter.routing.required', 'Required')} value={scenario.node.policy?.required_features?.join(', ') || 'none'} />
        <Fact label={t('aiCenter.routing.fallback', 'Fallback')} value={`${scenario.node.fallback?.mode ?? 'inherit'}${scenario.node.fallback?.target ? ` -> ${scenario.node.fallback.target}` : ''}`} />
      </div>

      <div className="mt-4">
        <div className="flex items-center justify-between gap-2 mb-2">
          <h4 className="text-xs font-medium" style={{ color: 'var(--cp-muted)' }}>
            {t('aiCenter.routing.rankedBaseModels', 'Ranked base models')}
          </h4>
          {scenario.groups.length > 4 && (
            <button
              type="button"
              onClick={() => setExpanded((value) => !value)}
              className="inline-flex items-center gap-1 text-xs"
              style={{ color: 'var(--cp-accent)' }}
            >
              {expanded ? <ChevronUp size={13} /> : <ChevronDown size={13} />}
              {expanded ? t('aiCenter.routing.showLess', 'Show less') : t('aiCenter.routing.showAll', 'Show all')}
            </button>
          )}
        </div>
        <div className="flex flex-col gap-2">
          {visibleGroups.map((group, index) => (
          <ModelGroupRow
              key={group.key}
              index={index}
              group={group}
              selected={modelGroupHasExact(group, scenario.selectedModel?.exact_model)}
              providerNames={providerNames}
            />
          ))}
          {scenario.groups.length === 0 && (
            <div className="flex items-start gap-2 rounded-lg p-3" style={{ background: 'var(--cp-bg)' }}>
              <AlertTriangle size={16} style={{ color: 'var(--cp-warning)' }} />
              <div className="text-sm" style={{ color: 'var(--cp-text)' }}>
                {t('aiCenter.routing.noCandidates', 'No discovered model currently matches this logical path and policy.')}
              </div>
            </div>
          )}
        </div>
      </div>
    </section>
  )
}

function ModelGroupRow({
  index,
  group,
  selected,
  providerNames,
}: {
  index: number
  group: ModelGroup
  selected: boolean
  providerNames: Map<string, string>
}) {
  const model = group.primary
  return (
    <article
      className="rounded-lg p-3"
      style={{
        background: selected ? 'var(--cp-surface-2)' : 'var(--cp-bg)',
        border: `1px solid ${selected ? 'var(--cp-accent)' : 'transparent'}`,
      }}
    >
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <span className="text-xs tabular-nums" style={{ color: 'var(--cp-muted)' }}>#{index + 1}</span>
            <span className="text-sm font-medium truncate" style={{ color: 'var(--cp-text)' }}>{model.provider_model_id}</span>
          </div>
          <div className="text-xs font-mono truncate mt-1" style={{ color: 'var(--cp-muted)' }}>
            {providerNames.get(providerFromExact(model.exact_model)) ?? providerFromExact(model.exact_model)} / {model.exact_model}
          </div>
        </div>
        <StatusBadge status={model.health.status === 'available' ? 'ok' : model.health.status === 'degraded' ? 'warning' : 'error'} label={model.health.status} />
      </div>
      <div className="flex flex-wrap gap-1.5 mt-3">
        <MetricChip icon={<Sparkles size={13} />} label="Q" value={formatQuality(model.attributes.quality_score)} />
        <MetricChip icon={<Zap size={13} />} label="Latency" value={formatLatency(model)} />
        <MetricChip icon={<DollarSign size={13} />} label="Cost" value={model.attributes.cost_class} />
        <MetricChip icon={<Layers size={13} />} label="Variants" value={group.variants.length.toString()} />
      </div>
      {group.variants.length > 0 && (
        <div className="mt-2 text-xs truncate" style={{ color: 'var(--cp-muted)' }}>
          {group.variants.map((variant) => variant.provider_model_id).join(', ')}
        </div>
      )}
    </article>
  )
}

function TraceExplorer({ traces, compact }: { traces: RouteTrace[]; compact: boolean }) {
  const { t } = useI18n()
  return (
    <section className="rounded-xl p-4" style={{ background: 'var(--cp-surface)', border: '1px solid var(--cp-border)' }}>
      <div className="flex items-center gap-2 mb-3">
        <Route size={16} style={{ color: 'var(--cp-accent)' }} />
        <h3 className="text-sm font-medium" style={{ color: 'var(--cp-text)' }}>{t('aiCenter.routing.routeTrace', 'Route Trace')}</h3>
      </div>
      <div className={compact ? 'flex flex-col gap-3' : 'grid grid-cols-1 gap-3'}>
        {traces.map((trace) => (
          <article key={trace.request_id} className="rounded-lg p-3" style={{ background: 'var(--cp-bg)' }}>
            <div className="flex flex-wrap items-center justify-between gap-2">
              <div className="text-sm font-mono" style={{ color: 'var(--cp-text)' }}>{trace.requested_model}</div>
              <StatusBadge status={trace.selected_exact_model ? (trace.fallback_applied ? 'warning' : 'ok') : 'error'} label={trace.scheduler_profile} />
            </div>
            <div className="text-xs mt-1" style={{ color: 'var(--cp-muted)' }}>
              {trace.selected_exact_model
                ? `${trace.selected_exact_model} / ${trace.selected_provider_instance_name}`
                : t('aiCenter.routing.noExactResolved', 'No exact model resolved')}
            </div>
            <p className="text-sm mt-2" style={{ color: 'var(--cp-text)' }}>{trace.user_summary?.reason_short}</p>
            <div className="flex flex-col gap-1 mt-3">
              {trace.ranked_candidates.length > 0 ? trace.ranked_candidates.map((candidate) => (
                <div key={candidate.exact_model} className="flex justify-between gap-3 text-xs">
                  <span className="min-w-0" style={{ color: candidate.selected ? 'var(--cp-accent)' : 'var(--cp-muted)' }}>
                    <span className="block truncate">{candidate.exact_model}</span>
                    <span className="block" style={{ color: 'var(--cp-muted)' }}>
                      {candidateWeightSummary(candidate)}
                    </span>
                  </span>
                  <span className="shrink-0" style={{ color: 'var(--cp-muted)' }}>{candidate.final_score?.toFixed(2)}</span>
                </div>
              )) : trace.filtered_candidates.map((candidate) => (
                <div key={candidate.exact_model} className="flex flex-col gap-0.5 text-xs">
                  <span style={{ color: 'var(--cp-warning)' }}>{candidate.exact_model}</span>
                  <span style={{ color: 'var(--cp-muted)' }}>{candidate.reason}</span>
                </div>
              ))}
            </div>
            {trace.warnings.length > 0 && (
              <div className="text-xs mt-2" style={{ color: 'var(--cp-warning)' }}>{trace.warnings.join(' / ')}</div>
            )}
          </article>
        ))}
      </div>
    </section>
  )
}

function EmptyResults() {
  const { t } = useI18n()
  return (
    <div className="rounded-xl p-8 text-center" style={{ background: 'var(--cp-surface)', border: '1px solid var(--cp-border)' }}>
      <Search size={32} className="mx-auto" style={{ color: 'var(--cp-muted)' }} />
      <p className="text-sm mt-3" style={{ color: 'var(--cp-muted)' }}>
        {t('aiCenter.routing.noMatches', 'No routing scenarios match the current filters.')}
      </p>
    </div>
  )
}

function UseCaseIcon({ kind, small = false }: { kind: UseCaseKind; small?: boolean }) {
  const size = small ? 15 : 19
  const icon = kind === 'chat' ? <MessageSquare size={size} />
    : kind === 'code' ? <Code2 size={size} />
      : kind === 'plan' ? <FileText size={size} />
        : kind === 'image' ? <Image size={size} />
          : kind === 'embed' ? <Braces size={size} />
            : kind === 'vision' ? <Activity size={size} />
              : kind === 'audio' ? <Zap size={size} />
                : <GitBranch size={size} />

  return (
    <div
      className={`${small ? 'h-7 w-7 rounded-md' : 'h-10 w-10 rounded-lg'} shrink-0 flex items-center justify-center`}
      style={{ background: 'var(--cp-bg)', color: 'var(--cp-accent)' }}
    >
      {icon}
    </div>
  )
}

function MetricChip({ icon, label, value }: { icon: ReactNode; label: string; value: string }) {
  return (
    <span
      className="inline-flex min-h-7 items-center gap-1 rounded-md px-2 text-xs"
      style={{ background: 'var(--cp-surface)', color: 'var(--cp-muted)', border: '1px solid var(--cp-border)' }}
    >
      {icon}
      <span>{label}</span>
      <span style={{ color: 'var(--cp-text)' }}>{value}</span>
    </span>
  )
}

function Fact({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex justify-between gap-3 rounded-lg px-3 py-2" style={{ background: 'var(--cp-bg)' }}>
      <span className="text-xs shrink-0" style={{ color: 'var(--cp-muted)' }}>{label}</span>
      <span className="text-xs text-right break-words" style={{ color: 'var(--cp-text)' }}>
        {value}
      </span>
    </div>
  )
}

function buildScenarios(nodes: LogicalNode[], models: ModelMetadata[], traces: RouteTrace[]): ScenarioView[] {
  const flattened = flattenNodes(nodes)
  const modelByExact = new Map(models.map((model) => [model.exact_model, model]))
  const scenarios = flattened
    .filter((node) => node.level !== 'L1')
    .filter((node) => isScenarioNode(node))
    .map((node) => {
      const trace = traces.find((item) => item.resolved_logical_path === node.path || item.requested_model === node.path)
      const selectedExactModel = trace?.selected_exact_model ?? node.resolved_exact_model
      const selectedModel = selectedExactModel ? modelByExact.get(selectedExactModel) : undefined
      const candidates = scenarioCandidates(node, models, trace)
      const groups = groupModels(candidates)
      const useCase = useCaseFromPath(node.path, node.api_type)
      return {
        node,
        useCase,
        title: scenarioTitle(node, useCase),
        description: scenarioDescription(node, useCase),
        selectedModel,
        selectedExactModel,
        trace,
        candidates,
        groups,
        score: scenarioScore(node, selectedModel, trace),
      }
    })

  return scenarios
}

function scenarioCandidates(node: LogicalNode, models: ModelMetadata[], trace?: RouteTrace): ModelMetadata[] {
  const childPaths = new Set(flattenNodes(node.children ?? []).map((child) => child.path))
  const itemTargets = new Set(Object.values(node.items ?? {}).map((item) => item.target))
  const traceCandidates = new Set(trace?.ranked_candidates.map((candidate) => candidate.exact_model) ?? [])
  const required = new Set(node.policy?.required_features ?? [])

  return models
    .filter((model) => modelMatchesLogicalNode(model, node, childPaths, itemTargets, traceCandidates))
    .filter((model) => modelSupportsRequired(model, required))
    .sort((left, right) => compareModelForScenario(left, right, node, trace))
}

function modelMatchesLogicalNode(
  model: ModelMetadata,
  node: LogicalNode,
  childPaths: Set<string>,
  itemTargets: Set<string>,
  traceCandidates: Set<string>,
): boolean {
  if (traceCandidates.has(model.exact_model)) return true
  if (node.resolved_exact_model === model.exact_model) return true
  if (node.api_type && model.api_types.includes(node.api_type)) {
    if (model.logical_mounts.includes(node.path)) return true
    if (model.logical_mounts.some((mount) => childPaths.has(mount) || itemTargets.has(mount))) return true
  }
  return model.logical_mounts.some((mount) => mount === node.path || childPaths.has(mount) || itemTargets.has(mount))
}

function modelSupportsRequired(model: ModelMetadata, required: Set<string>): boolean {
  if (required.size === 0) return true
  if (required.has('streaming') && !model.capabilities.streaming) return false
  if ((required.has('tool_call') || required.has('tool_calling')) && !model.capabilities.tool_call) return false
  if ((required.has('json_schema') || required.has('json_output')) && !model.capabilities.json_schema) return false
  if (required.has('web_search') && !model.capabilities.web_search) return false
  if (required.has('vision') && !model.capabilities.vision) return false
  return true
}

function groupModels(models: ModelMetadata[]): ModelGroup[] {
  const groups = new Map<string, ModelMetadata[]>()
  const groupOrder = new Map<string, number>()
  models.forEach((model, index) => {
    const key = baseModelKey(model)
    if (!groupOrder.has(key)) groupOrder.set(key, index)
    groups.set(key, [...(groups.get(key) ?? []), model])
  })
  return Array.from(groups.entries())
    .map(([key, items]) => {
      const sorted = [...items].sort(compareModelPriority)
      const [primary, ...variants] = sorted
      return primary ? { key, primary, variants } : null
    })
    .filter((group): group is ModelGroup => group !== null)
    .sort((left, right) => (groupOrder.get(left.key) ?? 0) - (groupOrder.get(right.key) ?? 0))
}

function modelGroupHasExact(group: ModelGroup, exactModel?: string): boolean {
  if (!exactModel) return false
  return group.primary.exact_model === exactModel || group.variants.some((model) => model.exact_model === exactModel)
}

function buildFilterOptions(models: ModelMetadata[]): Record<FilterKey, string[]> {
  return {
    provider: uniqueSorted(models.map((model) => providerFromExact(model.exact_model))),
    apiType: uniqueSorted(models.flatMap((model) => model.api_types)),
    capability: uniqueSorted(models.flatMap(modelCapabilities)),
    cost: uniqueSorted(models.map((model) => model.attributes.cost_class)),
    latency: uniqueSorted(models.map((model) => model.attributes.latency_class)),
    health: uniqueSorted(models.map((model) => model.health.status)),
    location: ['local', 'cloud'],
  }
}

function scenarioMatchesQuery(scenario: ScenarioView, query: string): boolean {
  if (!query) return true
  const haystack = [
    scenario.node.path,
    scenario.title,
    scenario.description,
    scenario.node.api_type ?? '',
    scenario.node.policy?.profile ?? '',
    scenario.selectedExactModel ?? '',
    ...scenario.candidates.flatMap((model) => [
      model.provider_model_id,
      model.provider_actual_model_id ?? '',
      model.exact_model,
      providerFromExact(model.exact_model),
      ...model.logical_mounts,
      ...model.api_types,
      ...modelCapabilities(model),
      model.attributes.cost_class,
      model.attributes.latency_class,
      model.health.status,
    ]),
  ].join(' ').toLowerCase()
  return haystack.includes(query)
}

function scenarioMatchesFilters(scenario: ScenarioView, filters: RoutingFilters): boolean {
  if (Object.values(filters).every((value) => value === 'all')) return true
  return scenario.candidates.some((model) =>
    (filters.provider === 'all' || providerFromExact(model.exact_model) === filters.provider)
    && (filters.apiType === 'all' || model.api_types.includes(filters.apiType as ApiType))
    && (filters.capability === 'all' || modelCapabilities(model).includes(filters.capability))
    && (filters.cost === 'all' || model.attributes.cost_class === filters.cost)
    && (filters.latency === 'all' || model.attributes.latency_class === filters.latency)
    && (filters.health === 'all' || model.health.status === filters.health)
    && (filters.location === 'all' || (filters.location === 'local' ? model.attributes.local : !model.attributes.local)),
  )
}

function compareScenario(left: ScenarioView, right: ScenarioView): number {
  return USE_CASE_ORDER.indexOf(left.useCase) - USE_CASE_ORDER.indexOf(right.useCase)
    || right.score - left.score
    || left.node.path.localeCompare(right.node.path)
}

function compareModelForScenario(left: ModelMetadata, right: ModelMetadata, node: LogicalNode, trace?: RouteTrace): number {
  const traceDiff = traceCandidateScore(right, trace) - traceCandidateScore(left, trace)
  if (traceDiff !== 0) return traceDiff
  const profileDiff = profileModelScore(right, node) - profileModelScore(left, node)
  if (profileDiff !== 0) return profileDiff
  return compareModelPriority(left, right)
}

function compareModelPriority(left: ModelMetadata, right: ModelMetadata): number {
  return healthScore(right) - healthScore(left)
    || variantScore(right) - variantScore(left)
    || (right.attributes.quality_score ?? 0) - (left.attributes.quality_score ?? 0)
    || latencyScore(right.attributes.latency_class) - latencyScore(left.attributes.latency_class)
    || costScore(right.attributes.cost_class) - costScore(left.attributes.cost_class)
    || versionScore(right.provider_model_id) - versionScore(left.provider_model_id)
    || left.provider_model_id.localeCompare(right.provider_model_id)
}

function profileModelScore(model: ModelMetadata, node: LogicalNode): number {
  const profile = node.policy?.profile ?? 'balanced'
  let score = 0
  if (node.resolved_exact_model === model.exact_model) score += 100
  if (node.policy?.local_only && model.attributes.local) score += 40
  if ((profile === 'local_first' || profile === 'strict_local') && model.attributes.local) score += 25
  if (profile === 'quality_first') score += (model.attributes.quality_score ?? 0) / 2
  if (profile === 'latency_first') score += latencyScore(model.attributes.latency_class) * 8
  if (profile === 'cost_first') score += costScore(model.attributes.cost_class) * 8
  return score
}

function traceCandidateScore(model: ModelMetadata, trace?: RouteTrace): number {
  const candidate = trace?.ranked_candidates.find((item) => item.exact_model === model.exact_model)
  if (!candidate) return 0
  return (candidate.selected ? 100 : 0) + ((candidate.final_score ?? 0) * 10)
}

function scenarioScore(node: LogicalNode, model?: ModelMetadata, trace?: RouteTrace): number {
  if (!model) return 0
  return 100
    + profileModelScore(model, node)
    + traceCandidateScore(model, trace)
    + healthScore(model)
    + (model.attributes.quality_score ?? 0)
}

function isScenarioNode(node: LogicalNode): boolean {
  if (node.path === 'llm') return true
  if (node.level === 'L3') return true
  return Boolean(node.items && Object.keys(node.items).length > 0)
}

function flattenNodes(nodes: LogicalNode[]): LogicalNode[] {
  return nodes.flatMap((node) => [node, ...flattenNodes(node.children ?? [])])
}

function useCaseFromPath(path: string, apiType?: string): UseCaseKind {
  const value = `${path} ${apiType ?? ''}`.toLowerCase()
  if (value.includes('code') || value.includes('coder')) return 'code'
  if (value.includes('plan') || value.includes('reason')) return 'plan'
  if (value.includes('image')) return 'image'
  if (value.includes('embedding')) return 'embed'
  if (value.includes('vision') || value.includes('ocr')) return 'vision'
  if (value.includes('audio') || value.includes('tts') || value.includes('asr')) return 'audio'
  if (value.includes('llm') || value.includes('chat')) return 'chat'
  return 'other'
}

function scenarioTitle(node: LogicalNode, useCase: UseCaseKind): string {
  if (node.path === 'llm') return 'Chat'
  if (useCase === 'code') return 'Code'
  if (useCase === 'plan') return node.path.includes('reason') ? 'Reasoning / Plan' : 'Plan'
  if (useCase === 'image') return 'Image'
  if (useCase === 'embed') return 'Embedding'
  if (useCase === 'vision') return 'Vision'
  if (useCase === 'audio') return 'Audio'
  return node.label || node.path
}

function scenarioDescription(node: LogicalNode, useCase: UseCaseKind): string {
  const profile = node.policy?.profile ?? 'balanced'
  if (useCase === 'chat') return `General conversation and default LLM traffic, sorted with ${profile}.`
  if (useCase === 'code') return `Coding requests that need tool use and low-friction iteration, sorted with ${profile}.`
  if (useCase === 'plan') return `Planning or reasoning requests that favor stronger model quality, sorted with ${profile}.`
  if (useCase === 'image') return `Image generation and editing routes, sorted with ${profile}.`
  if (useCase === 'embed') return `Embedding routes for retrieval and semantic indexing, sorted with ${profile}.`
  if (useCase === 'vision') return `Vision and OCR routes, sorted with ${profile}.`
  if (useCase === 'audio') return `Speech and audio routes, sorted with ${profile}.`
  return `${node.label || node.path}, sorted with ${profile}.`
}

function modelCapabilities(model: ModelMetadata): string[] {
  const result: string[] = []
  if (model.capabilities.streaming) result.push('streaming')
  if (model.capabilities.tool_call) result.push('tool_call')
  if (model.capabilities.json_schema) result.push('json_schema')
  if (model.capabilities.web_search) result.push('web_search')
  if (model.capabilities.vision) result.push('vision')
  return result
}

function baseModelKey(model: ModelMetadata): string {
  return (model.provider_actual_model_id ?? model.provider_model_id)
    .replace(/@(.*)$/, '')
    .replace(/[-_.](reasoning|thinking|think|mini|nano|small|flash|lite|preview|latest|turbo|fast|low|high)$/i, '')
}

function variantScore(model: ModelMetadata): number {
  if (model.provider_actual_model_id && model.provider_actual_model_id !== model.provider_model_id) return 0
  return baseModelKey(model).toLowerCase() === model.provider_model_id.replace(/@(.*)$/, '').toLowerCase() ? 2 : 1
}

function healthScore(model: ModelMetadata): number {
  if (model.health.quota_state === 'exhausted') return -10
  if (model.health.status === 'available') return 3
  if (model.health.status === 'degraded') return 1
  return -5
}

function latencyScore(value: ModelMetadata['attributes']['latency_class']): number {
  if (value === 'fast') return 3
  if (value === 'normal') return 2
  if (value === 'slow') return 1
  return 0
}

function costScore(value: ModelMetadata['attributes']['cost_class']): number {
  if (value === 'low') return 3
  if (value === 'medium') return 2
  if (value === 'high') return 1
  return 0
}

function versionScore(value: string): number {
  const normalized = value
    .replace(/\bmini\b|\bnano\b|\bpreview\b|\blite\b|\bflash\b/gi, '')
  const matches = normalized.match(/\d+(?:\.\d+)?/g) ?? []
  return matches.reduce((score, item, index) => score + Number(item) / (index + 1), 0)
}

function providerFromExact(exactModel: string): string {
  return exactModel.split('@')[1] ?? 'unknown'
}

function formatQuality(value?: number): string {
  if (value == null) return '-'
  return value > 1 ? value.toFixed(0) : Math.round(value * 100).toString()
}

function formatLatency(model: ModelMetadata): string {
  return `${model.attributes.latency_class}${model.health.p95_latency_ms ? ` p95 ${model.health.p95_latency_ms}ms` : ''}`
}

function uniqueSorted(values: string[]): string[] {
  return Array.from(new Set(values.filter(Boolean))).sort((left, right) => left.localeCompare(right))
}

function candidateWeightSummary(candidate: RouteTrace['ranked_candidates'][number]): string {
  const inputs = candidate.preference_score_inputs
  const exact = inputs?.exact_model_weight ?? candidate.exact_model_weight ?? 1
  const provider = inputs?.provider_weight ?? candidate.provider_weight ?? 1
  const combined = inputs?.combined_weight ?? exact * provider
  const providerEffect = inputs?.provider_weight_effect ?? weightEffect(provider)
  return `exact ${formatWeight(exact)} / provider ${formatWeight(provider)} ${providerEffect} / combined ${formatWeight(combined)}`
}

function weightEffect(weight: number): string {
  if (weight <= 0) return 'disabled'
  if (weight < 1) return 'downweighted'
  if (weight > 1) return 'upweighted'
  return 'neutral'
}

function formatWeight(weight: number): string {
  return weight.toFixed(2).replace(/\.?0+$/, '')
}
