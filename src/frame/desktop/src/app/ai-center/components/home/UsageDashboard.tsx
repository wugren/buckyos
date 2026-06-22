import { useMemo, useRef, useState } from 'react'
import { Activity, CreditCard, DollarSign, HelpCircle, Route, Wallet } from 'lucide-react'
import { useI18n } from '../../../../i18n/provider'
import { useAIStatus, useProviders, useRouteTraces, useUsageEvents, useUsageSummary, useUsageTrend } from '../../hooks/use-aicc-store'
import { SummaryCard } from '../shared/SummaryCard'
import type { RouteTrace, UsageEvent } from '../../../../api/aicc_mgr'

function formatTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`
  return String(n)
}

function sortedEntries(record: Record<string, number>, limit?: number): Array<[string, number]> {
  const entries = Object.entries(record).sort((a, b) => b[1] - a[1])
  return limit == null ? entries : entries.slice(0, limit)
}

function candidateWeightSummary(candidate: RouteTrace['ranked_candidates'][number]): string {
  const inputs = candidate.preference_score_inputs
  const exact = inputs?.exact_model_weight ?? candidate.exact_model_weight ?? 1
  const provider = inputs?.provider_weight ?? candidate.provider_weight ?? 1
  const combined = inputs?.combined_weight ?? exact * provider
  return `exact ${formatWeight(exact)} · provider ${formatWeight(provider)} · combined ${formatWeight(combined)}`
}

function formatWeight(weight: number): string {
  return weight.toFixed(2).replace(/\.?0+$/, '')
}

function usageFinanceAmount(event: UsageEvent): number {
  return event.finance_snapshot?.amount ?? 0
}

function uniqueSorted(values: Array<string | undefined>): string[] {
  return Array.from(new Set(values.filter((value): value is string => Boolean(value)))).sort((a, b) => a.localeCompare(b))
}

type TimeRangeFilter = 'all' | '24h' | '7d' | '30d'
type BreakdownFilterTarget = 'provider' | 'model' | 'appAgent'

export function UsageDashboard() {
  const { t } = useI18n()
  const status = useAIStatus()
  const providers = useProviders()
  const summary = useUsageSummary()
  const trend = useUsageTrend('day')
  const events = useUsageEvents()
  const traces = useRouteTraces()
  const [timeRange, setTimeRange] = useState<TimeRangeFilter>('all')
  const [providerFilter, setProviderFilter] = useState('all')
  const [modelFilter, setModelFilter] = useState('all')
  const [appAgentFilter, setAppAgentFilter] = useState('all')
  const detailRef = useRef<HTMLElement | null>(null)

  const snProvider = providers.find((p) => p.config.provider_type === 'sn_router')
  const snCredit = snProvider?.account.balance_value
  const balanceProviders = providers.filter((p) => p.account.balance_supported && p.account.balance_value != null)
  const usageOnlyProviders = providers.filter((p) => p.account.usage_supported && !p.account.balance_supported)
  const maxTrendTokens = Math.max(...trend.map((p) => p.tokens), 1)
  const sortedEvents = useMemo(
    () => [...events].sort((a, b) => new Date(b.timestamp).getTime() - new Date(a.timestamp).getTime()),
    [events],
  )
  const providerOptions = useMemo(() => uniqueSorted(events.map((event) => event.provider_instance_name)), [events])
  const modelOptions = useMemo(() => uniqueSorted(events.map((event) => event.exact_model)), [events])
  const appAgentOptions = useMemo(
    () => uniqueSorted(events.flatMap((event) => [event.app_id ?? 'system', event.agent_id])),
    [events],
  )
  const filteredEvents = useMemo(() => {
    const now = Date.now()
    const minTime = timeRange === '24h'
      ? now - 24 * 60 * 60 * 1000
      : timeRange === '7d'
        ? now - 7 * 24 * 60 * 60 * 1000
        : timeRange === '30d'
          ? now - 30 * 24 * 60 * 60 * 1000
          : null

    return sortedEvents.filter((event) => {
      const eventTime = new Date(event.timestamp).getTime()
      if (minTime != null && eventTime < minTime) return false
      if (providerFilter !== 'all' && event.provider_instance_name !== providerFilter) return false
      if (modelFilter !== 'all' && event.exact_model !== modelFilter) return false
      if (appAgentFilter !== 'all' && (event.app_id ?? 'system') !== appAgentFilter && event.agent_id !== appAgentFilter) return false
      return true
    })
  }, [appAgentFilter, modelFilter, providerFilter, sortedEvents, timeRange])

  const balanceSubtitle = balanceProviders
    .map((p) => {
      const unit = p.account.balance_unit === 'usd' ? '$' : ''
      const suffix = p.account.balance_unit === 'credit' ? ' Credit' : ''
      return `${p.config.provider_instance_name}: ${unit}${p.account.balance_value}${suffix}`
    })
    .join(' · ')

  const usageOnlyNote = t(
    'aiCenter.home.usageOnlyProviderNote',
    '{{count}} usage-only provider(s) report usage/cost without balance.',
    { count: usageOnlyProviders.length },
  )
  const balanceOverviewValue = t(
    'aiCenter.home.balanceOverviewValue',
    '{{balanceCount}} balance / {{usageOnlyCount}} usage-only',
    { balanceCount: balanceProviders.length, usageOnlyCount: usageOnlyProviders.length },
  )
  const balanceOverviewSubtitle = balanceProviders.length > 0
    ? `${balanceSubtitle}. ${usageOnlyNote}`
    : t('aiCenter.home.usageOnlyExplain', 'No provider exposes balance yet. Usage-only providers can still report usage and cost.')

  const applyBreakdownFilter = (target: BreakdownFilterTarget, value: string) => {
    setTimeRange('all')
    setProviderFilter(target === 'provider' ? value : 'all')
    setModelFilter(target === 'model' ? value : 'all')
    setAppAgentFilter(target === 'appAgent' ? value : 'all')
    window.requestAnimationFrame(() => {
      detailRef.current?.scrollIntoView({ behavior: 'smooth', block: 'start' })
    })
  }

  return (
    <div className="flex flex-col gap-6">
      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
        <SummaryCard
          icon={<Activity size={18} />}
          title={t('aiCenter.home.status', 'AI Status')}
          value={status.state === 'disabled' ? t('aiCenter.home.disabled', 'Disabled') : t('aiCenter.home.enabled', 'Enabled')}
          subtitle={`${status.provider_count} Provider instances · ${status.model_count} Models · ${status.health_counts.degraded} degraded`}
        />
        <SummaryCard
          icon={<CreditCard size={18} />}
          title={t('aiCenter.home.credit', 'SN Credit')}
          value={snCredit != null ? `${snCredit} Credit` : '—'}
          subtitle={snProvider ? `${snProvider.account.pricing_mode} · top up available` : undefined}
        />
        <SummaryCard
          icon={<DollarSign size={18} />}
          title={t('aiCenter.home.estimatedCost', 'Est. Cost')}
          value={`$${summary.total_estimated_cost.toFixed(2)}`}
          subtitle={t('aiCenter.home.costEstimated', 'Estimated from usage events')}
        />
        <SummaryCard
          icon={<Wallet size={18} />}
          title={t('aiCenter.home.balanceOverview', 'Balance Overview')}
          value={balanceOverviewValue}
          subtitle={balanceOverviewSubtitle}
        />
      </div>

      <div className="grid grid-cols-1 xl:grid-cols-[minmax(0,2fr)_minmax(280px,1fr)] gap-4">
        <section className="rounded-xl p-4" style={{ background: 'var(--cp-surface)', border: '1px solid var(--cp-border)' }}>
          <div className="flex items-center justify-between mb-4">
            <h3 className="text-sm font-medium" style={{ color: 'var(--cp-text)' }}>
              {t('aiCenter.home.trendTitle', 'Usage Trend')}
            </h3>
            <span className="text-xs" style={{ color: 'var(--cp-muted)' }}>
              {t('aiCenter.home.last30Days', 'Last 30 days')}
            </span>
          </div>
          <div className="flex items-end gap-1 h-52">
            {trend.map((point) => (
              <div key={point.timestamp} className="flex-1 flex flex-col items-center gap-1 min-w-0">
                <div
                  className="w-full rounded-t-sm"
                  title={`${point.timestamp}: ${formatTokens(point.tokens)} tokens / $${point.estimated_cost.toFixed(4)}`}
                  style={{
                    height: `${Math.max(4, (point.tokens / maxTrendTokens) * 180)}px`,
                    background: point.tokens > 0 ? 'var(--cp-accent)' : 'var(--cp-border)',
                    opacity: point.tokens > 0 ? 0.78 : 0.4,
                  }}
                />
                <span className="text-[10px] hidden sm:block" style={{ color: 'var(--cp-muted)' }}>
                  {point.timestamp.slice(3)}
                </span>
              </div>
            ))}
          </div>
        </section>

        <section className="rounded-xl p-4" style={{ background: 'var(--cp-surface)', border: '1px solid var(--cp-border)' }}>
          <h3 className="text-sm font-medium mb-3" style={{ color: 'var(--cp-text)' }}>
            {t('aiCenter.home.categoryTitle', 'API Type Breakdown')}
          </h3>
          <div className="flex flex-col gap-2">
            {sortedEntries(summary.by_api_namespace, 8).map(([namespace, tokens]) => (
              <MeterRow key={namespace} label={namespace} value={tokens} max={summary.total_tokens} />
            ))}
          </div>
        </section>
      </div>

      <section className="rounded-xl p-4" style={{ background: 'var(--cp-surface)', border: '1px solid var(--cp-border)' }}>
        <h3 className="text-sm font-medium mb-3" style={{ color: 'var(--cp-text)' }}>
          {t('aiCenter.home.usageSummary', 'Usage Summary')}
        </h3>
        <div className="grid grid-cols-2 md:grid-cols-5 gap-4">
          <Stat label={t('aiCenter.home.today', 'Today')} value={`${formatTokens(summary.today_tokens)} tokens`} />
          <Stat label={t('aiCenter.home.thisMonth', 'This Month')} value={`${formatTokens(summary.this_month_tokens)} tokens`} />
          <Stat label={t('aiCenter.home.total', 'Total')} value={`${formatTokens(summary.total_tokens)} tokens`} />
          <Stat label={t('aiCenter.home.requests', 'Requests')} value={summary.total_requests.toString()} />
          <Stat label={t('aiCenter.home.totalCost', 'Total Est. Cost')} value={`$${summary.total_estimated_cost.toFixed(2)}`} />
        </div>
      </section>

      <div className="grid grid-cols-1 lg:grid-cols-3 gap-4">
        <Breakdown
          title={t('aiCenter.home.byProvider', 'By Provider Instance')}
          rows={sortedEntries(summary.by_provider)}
          total={summary.total_tokens}
          activeLabel={providerFilter !== 'all' ? providerFilter : undefined}
          onSelect={(label) => applyBreakdownFilter('provider', label)}
          viewAllLabel={t('aiCenter.home.viewAll', 'View all')}
          showLessLabel={t('aiCenter.home.showLess', 'Show less')}
          filterLabel={t('aiCenter.home.filterToDetail', 'Filter Usage Detail')}
          emptyLabel={t('aiCenter.home.noBreakdownData', 'No usage data yet.')}
        />
        <Breakdown
          title={t('aiCenter.home.byModel', 'By Exact Model')}
          rows={sortedEntries(summary.by_model)}
          total={summary.total_tokens}
          activeLabel={modelFilter !== 'all' ? modelFilter : undefined}
          onSelect={(label) => applyBreakdownFilter('model', label)}
          viewAllLabel={t('aiCenter.home.viewAll', 'View all')}
          showLessLabel={t('aiCenter.home.showLess', 'Show less')}
          filterLabel={t('aiCenter.home.filterToDetail', 'Filter Usage Detail')}
          emptyLabel={t('aiCenter.home.noBreakdownData', 'No usage data yet.')}
        />
        <Breakdown
          title={t('aiCenter.home.byApp', 'By App / Agent')}
          rows={sortedEntries(summary.by_app)}
          total={summary.total_tokens}
          activeLabel={appAgentFilter !== 'all' ? appAgentFilter : undefined}
          onSelect={(label) => applyBreakdownFilter('appAgent', label)}
          viewAllLabel={t('aiCenter.home.viewAll', 'View all')}
          showLessLabel={t('aiCenter.home.showLess', 'Show less')}
          filterLabel={t('aiCenter.home.filterToDetail', 'Filter Usage Detail')}
          emptyLabel={t('aiCenter.home.noBreakdownData', 'No usage data yet.')}
        />
      </div>

      {traces[0] && (
        <section className="rounded-xl p-4" style={{ background: 'var(--cp-surface)', border: '1px solid var(--cp-border)' }}>
          <div className="flex items-center gap-2 mb-3">
            <Route size={16} style={{ color: 'var(--cp-accent)' }} />
            <h3 className="text-sm font-medium" style={{ color: 'var(--cp-text)' }}>
              {t('aiCenter.home.recentRouteTrace', 'Recent Route Trace')}
            </h3>
          </div>
          <div className="grid grid-cols-1 md:grid-cols-[1.2fr_2fr] gap-4">
            <div>
              <div className="text-xs" style={{ color: 'var(--cp-muted)' }}>{traces[0].requested_model} → {traces[0].selected_exact_model}</div>
              <div className="text-sm mt-1" style={{ color: 'var(--cp-text)' }}>
                {traces[0].user_summary?.reason_short}
              </div>
            </div>
            <div className="flex flex-wrap gap-2">
              {traces[0].ranked_candidates.map((candidate) => (
                <span
                  key={candidate.exact_model}
                  className="text-xs rounded-md px-2 py-1"
                  style={{
                    background: candidate.selected ? 'color-mix(in oklch, var(--cp-accent), transparent 84%)' : 'var(--cp-bg)',
                    color: candidate.selected ? 'var(--cp-accent)' : 'var(--cp-muted)',
                  }}
                >
                  {candidate.exact_model} · {candidate.final_score?.toFixed(2)} · {candidateWeightSummary(candidate)}
                </span>
              ))}
            </div>
          </div>
        </section>
      )}

      <section ref={detailRef} className="rounded-xl overflow-hidden scroll-mt-4" style={{ border: '1px solid var(--cp-border)' }}>
        <div className="px-4 py-3 flex flex-col gap-3" style={{ background: 'var(--cp-surface)' }}>
          <div className="flex items-center justify-between gap-3">
            <h3 className="text-sm font-medium" style={{ color: 'var(--cp-text)' }}>
              {t('aiCenter.home.detailTable', 'Usage Detail')}
            </h3>
            <span className="text-xs" style={{ color: 'var(--cp-muted)' }}>
              {filteredEvents.length} / {events.length}
            </span>
          </div>
          <div className="grid grid-cols-1 sm:grid-cols-2 xl:grid-cols-4 gap-2">
            <FilterSelect
              label={t('aiCenter.home.filterTimeRange', 'Time Range')}
              value={timeRange}
              onChange={(value) => setTimeRange(value as TimeRangeFilter)}
              options={[
                ['all', t('aiCenter.home.allTime', 'All time')],
                ['24h', t('aiCenter.home.last24Hours', 'Last 24 hours')],
                ['7d', t('aiCenter.home.last7Days', 'Last 7 days')],
                ['30d', t('aiCenter.home.last30Days', 'Last 30 days')],
              ]}
            />
            <FilterSelect
              label={t('aiCenter.home.filterProvider', 'Provider')}
              value={providerFilter}
              onChange={setProviderFilter}
              options={[['all', t('aiCenter.home.allProviders', 'All providers')], ...providerOptions.map((item) => [item, item] as [string, string])]}
            />
            <FilterSelect
              label={t('aiCenter.home.filterModel', 'Model')}
              value={modelFilter}
              onChange={setModelFilter}
              options={[['all', t('aiCenter.home.allModels', 'All models')], ...modelOptions.map((item) => [item, item] as [string, string])]}
            />
            <FilterSelect
              label={t('aiCenter.home.filterAppAgent', 'App / Agent')}
              value={appAgentFilter}
              onChange={setAppAgentFilter}
              options={[['all', t('aiCenter.home.allAppsAgents', 'All apps / agents')], ...appAgentOptions.map((item) => [item, item] as [string, string])]}
            />
          </div>
        </div>
        <div className="overflow-x-auto">
          <table className="w-full min-w-[940px]">
            <thead>
              <tr style={{ background: 'var(--cp-bg)' }}>
                {['Time', 'Provider', 'Exact Model', 'API Type', 'App / Agent', 'Task / Session', 'Tokens', 'Cost', 'Status'].map((h) => (
                  <th key={h} className="text-left text-xs font-medium px-4 py-2" style={{ color: 'var(--cp-muted)' }}>
                    <span className="inline-flex items-center gap-1">
                      {h}
                      {h === 'Task / Session' && (
                        <span
                          className="inline-flex"
                          title={t('aiCenter.home.taskSessionTooltip', 'Task / Session is the AICC task id or Agent session id that produced this usage event.')}
                        >
                          <HelpCircle size={12} />
                        </span>
                      )}
                    </span>
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {filteredEvents.map((event) => {
                const tokens = event.token_equivalent ?? (event.tokens_in ?? 0) + (event.tokens_out ?? 0)
                return (
                  <tr key={event.id} style={{ borderTop: '1px solid var(--cp-border)' }}>
                    <td className="px-4 py-2 text-xs" style={{ color: 'var(--cp-muted)' }}>{event.timestamp.slice(5, 16).replace('T', ' ')}</td>
                    <td className="px-4 py-2 text-xs" style={{ color: 'var(--cp-text)' }}>
                      <TruncatedText value={event.provider_instance_name} className="max-w-[160px]" />
                    </td>
                    <td className="px-4 py-2 text-xs font-mono" style={{ color: 'var(--cp-text)' }}>
                      <TruncatedText value={event.exact_model} className="max-w-[220px]" />
                    </td>
                    <td className="px-4 py-2 text-xs" style={{ color: 'var(--cp-text)' }}>{event.api_type}</td>
                    <td className="px-4 py-2 text-xs" style={{ color: 'var(--cp-text)' }}>
                      <TruncatedText value={`${event.app_id ?? 'system'}${event.agent_id ? ` / ${event.agent_id}` : ''}`} className="max-w-[180px]" />
                    </td>
                    <td className="px-4 py-2 text-xs" style={{ color: 'var(--cp-muted)' }}>{event.session_id}</td>
                    <td className="px-4 py-2 text-xs" style={{ color: 'var(--cp-text)' }}>{formatTokens(tokens)}</td>
                    <td className="px-4 py-2 text-xs" style={{ color: 'var(--cp-text)' }}>${usageFinanceAmount(event).toFixed(4)}</td>
                    <td className="px-4 py-2 text-xs" style={{ color: event.status === 'success' ? 'var(--cp-success)' : 'var(--cp-danger)' }}>{event.status}</td>
                  </tr>
                )
              })}
              {filteredEvents.length === 0 && (
                <tr style={{ borderTop: '1px solid var(--cp-border)' }}>
                  <td className="px-4 py-8 text-center text-xs" colSpan={9} style={{ color: 'var(--cp-muted)' }}>
                    {t('aiCenter.home.noUsageEvents', 'No usage events match the current filters.')}
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
      </section>
    </div>
  )
}

function FilterSelect({
  label,
  value,
  onChange,
  options,
}: {
  label: string
  value: string
  onChange: (value: string) => void
  options: Array<[string, string]>
}) {
  return (
    <label className="flex flex-col gap-1 text-xs" style={{ color: 'var(--cp-muted)' }}>
      <span>{label}</span>
      <select
        value={value}
        onChange={(event) => onChange(event.target.value)}
        className="h-9 rounded-md px-2 text-xs outline-none"
        style={{
          background: 'var(--cp-bg)',
          border: '1px solid var(--cp-border)',
          color: 'var(--cp-text)',
        }}
      >
        {options.map(([optionValue, optionLabel]) => (
          <option key={optionValue} value={optionValue}>
            {optionLabel}
          </option>
        ))}
      </select>
    </label>
  )
}

function Stat({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <div className="text-xs" style={{ color: 'var(--cp-muted)' }}>{label}</div>
      <div className="text-base font-semibold" style={{ color: 'var(--cp-text)' }}>{value}</div>
    </div>
  )
}

function MeterRow({
  label,
  value,
  max,
  onClick,
  active,
  actionLabel,
}: {
  label: string
  value: number
  max: number
  onClick?: () => void
  active?: boolean
  actionLabel?: string
}) {
  const percent = max > 0 ? Math.max(3, (value / max) * 100) : 0
  const content = (
    <>
      <div className="flex items-center justify-between gap-3 text-xs mb-1 min-w-0">
        <TruncatedText value={label} className="flex-1" />
        <span className="shrink-0 tabular-nums" style={{ color: 'var(--cp-muted)' }}>{formatTokens(value)}</span>
      </div>
      <div className="h-2 rounded-full overflow-hidden" style={{ background: 'var(--cp-bg)' }}>
        <div className="h-full rounded-full" style={{ width: `${percent}%`, background: 'var(--cp-accent)' }} />
      </div>
    </>
  )

  if (!onClick) {
    return <div className="min-w-0">{content}</div>
  }

  return (
    <button
      type="button"
      onClick={onClick}
      title={`${actionLabel}: ${label}`}
      aria-label={`${actionLabel}: ${label}`}
      className="w-full min-w-0 rounded-md p-2 text-left outline-none transition hover:bg-[color:color-mix(in_srgb,var(--cp-accent)_8%,transparent)] focus-visible:ring-2 focus-visible:ring-[color:var(--cp-accent)]"
      style={{
        border: active ? '1px solid var(--cp-accent)' : '1px solid transparent',
        background: active ? 'color-mix(in oklch, var(--cp-accent), transparent 90%)' : undefined,
      }}
    >
      {content}
    </button>
  )
}

function Breakdown({
  title,
  rows,
  total,
  activeLabel,
  onSelect,
  viewAllLabel,
  showLessLabel,
  filterLabel,
  emptyLabel,
}: {
  title: string
  rows: Array<[string, number]>
  total: number
  activeLabel?: string
  onSelect: (label: string) => void
  viewAllLabel: string
  showLessLabel: string
  filterLabel: string
  emptyLabel: string
}) {
  const [expanded, setExpanded] = useState(false)
  const hiddenCount = Math.max(0, rows.length - 4)
  const visibleRows = expanded ? rows : rows.slice(0, 4)

  return (
    <section className="rounded-xl p-4" style={{ background: 'var(--cp-surface)', border: '1px solid var(--cp-border)' }}>
      <div className="mb-3 flex items-center justify-between gap-3">
        <h3 className="min-w-0 truncate text-sm font-medium" title={title} style={{ color: 'var(--cp-text)' }}>
          {title}
        </h3>
        <span className="shrink-0 text-xs" style={{ color: 'var(--cp-muted)' }}>
          {rows.length}
        </span>
      </div>
      <div className={`flex flex-col gap-1 ${expanded ? 'max-h-80 overflow-y-auto pr-1' : ''}`}>
        {visibleRows.map(([label, value]) => (
          <MeterRow
            key={label}
            label={label}
            value={value}
            max={total}
            active={activeLabel === label}
            actionLabel={filterLabel}
            onClick={() => onSelect(label)}
          />
        ))}
        {rows.length === 0 && (
          <div className="rounded-md px-2 py-6 text-center text-xs" style={{ color: 'var(--cp-muted)', background: 'var(--cp-bg)' }}>
            {emptyLabel}
          </div>
        )}
      </div>
      {hiddenCount > 0 && (
        <button
          type="button"
          onClick={() => setExpanded((value) => !value)}
          className="mt-3 h-8 w-full rounded-md text-xs font-medium outline-none transition hover:bg-[color:color-mix(in_srgb,var(--cp-accent)_8%,transparent)] focus-visible:ring-2 focus-visible:ring-[color:var(--cp-accent)]"
          style={{
            color: 'var(--cp-accent)',
            border: '1px solid var(--cp-border)',
          }}
        >
          {expanded ? showLessLabel : `${viewAllLabel} (${hiddenCount})`}
        </button>
      )}
    </section>
  )
}

function TruncatedText({ value, className = '' }: { value: string; className?: string }) {
  return (
    <span title={value} className={`block min-w-0 truncate ${className}`} style={{ color: 'var(--cp-text)' }}>
      {value}
    </span>
  )
}
