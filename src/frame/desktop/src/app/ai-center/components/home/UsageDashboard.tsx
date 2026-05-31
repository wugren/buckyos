import { Activity, CreditCard, DollarSign, Route, Wallet } from 'lucide-react'
import { useI18n } from '../../../../i18n/provider'
import { useAIStatus, useProviders, useRouteTraces, useUsageEvents, useUsageSummary, useUsageTrend } from '../../hooks/use-mock-store'
import { SummaryCard } from '../shared/SummaryCard'

function formatTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`
  return String(n)
}

function topEntries(record: Record<string, number>, limit = 4): Array<[string, number]> {
  return Object.entries(record).sort((a, b) => b[1] - a[1]).slice(0, limit)
}

export function UsageDashboard() {
  const { t } = useI18n()
  const status = useAIStatus()
  const providers = useProviders()
  const summary = useUsageSummary()
  const trend = useUsageTrend('day')
  const events = useUsageEvents()
  const traces = useRouteTraces()

  const snProvider = providers.find((p) => p.config.provider_type === 'sn_router')
  const snCredit = snProvider?.account.balance_value
  const balanceProviders = providers.filter((p) => p.account.balance_supported && p.account.balance_value != null)
  const maxTrendTokens = Math.max(...trend.map((p) => p.tokens), 1)
  const recentEvents = events.slice(0, 8)

  const balanceSubtitle = balanceProviders
    .map((p) => {
      const unit = p.account.balance_unit === 'usd' ? '$' : ''
      const suffix = p.account.balance_unit === 'credit' ? ' Credit' : ''
      return `${p.config.provider_instance_name}: ${unit}${p.account.balance_value}${suffix}`
    })
    .join(' · ')

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
          value={`${balanceProviders.length} Providers`}
          subtitle={balanceSubtitle || t('aiCenter.home.usageOnly', 'Usage only where balance is unavailable')}
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
                  title={`${point.timestamp}: ${formatTokens(point.tokens)} tokens`}
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
            {topEntries(summary.by_api_namespace, 8).map(([namespace, tokens]) => (
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
        <Breakdown title={t('aiCenter.home.byProvider', 'By Provider Instance')} rows={topEntries(summary.by_provider)} total={summary.total_tokens} />
        <Breakdown title={t('aiCenter.home.byModel', 'By Exact Model')} rows={topEntries(summary.by_model)} total={summary.total_tokens} />
        <Breakdown title={t('aiCenter.home.byApp', 'By App / Agent')} rows={topEntries(summary.by_app)} total={summary.total_tokens} />
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
                  {candidate.exact_model} · {candidate.final_score?.toFixed(2)}
                </span>
              ))}
            </div>
          </div>
        </section>
      )}

      <section className="rounded-xl overflow-hidden" style={{ border: '1px solid var(--cp-border)' }}>
        <div className="px-4 py-3" style={{ background: 'var(--cp-surface)' }}>
          <h3 className="text-sm font-medium" style={{ color: 'var(--cp-text)' }}>
            {t('aiCenter.home.detailTable', 'Usage Detail')}
          </h3>
        </div>
        <div className="overflow-x-auto">
          <table className="w-full min-w-[820px]">
            <thead>
              <tr style={{ background: 'var(--cp-bg)' }}>
                {['Time', 'Provider', 'Exact Model', 'API Type', 'App', 'Session', 'Tokens', 'Cost', 'Status'].map((h) => (
                  <th key={h} className="text-left text-xs font-medium px-4 py-2" style={{ color: 'var(--cp-muted)' }}>{h}</th>
                ))}
              </tr>
            </thead>
            <tbody>
              {recentEvents.map((event) => {
                const tokens = event.token_equivalent ?? (event.tokens_in ?? 0) + (event.tokens_out ?? 0)
                return (
                  <tr key={event.id} style={{ borderTop: '1px solid var(--cp-border)' }}>
                    <td className="px-4 py-2 text-xs" style={{ color: 'var(--cp-muted)' }}>{event.timestamp.slice(5, 16).replace('T', ' ')}</td>
                    <td className="px-4 py-2 text-xs" style={{ color: 'var(--cp-text)' }}>{event.provider_instance_name}</td>
                    <td className="px-4 py-2 text-xs font-mono" style={{ color: 'var(--cp-text)' }}>{event.exact_model}</td>
                    <td className="px-4 py-2 text-xs" style={{ color: 'var(--cp-text)' }}>{event.api_type}</td>
                    <td className="px-4 py-2 text-xs" style={{ color: 'var(--cp-text)' }}>{event.app_id ?? 'system'}</td>
                    <td className="px-4 py-2 text-xs" style={{ color: 'var(--cp-muted)' }}>{event.session_id}</td>
                    <td className="px-4 py-2 text-xs" style={{ color: 'var(--cp-text)' }}>{formatTokens(tokens)}</td>
                    <td className="px-4 py-2 text-xs" style={{ color: 'var(--cp-text)' }}>${(event.estimated_cost ?? 0).toFixed(4)}</td>
                    <td className="px-4 py-2 text-xs" style={{ color: event.status === 'success' ? 'var(--cp-success)' : 'var(--cp-danger)' }}>{event.status}</td>
                  </tr>
                )
              })}
            </tbody>
          </table>
        </div>
      </section>
    </div>
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

function MeterRow({ label, value, max }: { label: string; value: number; max: number }) {
  const percent = max > 0 ? Math.max(3, (value / max) * 100) : 0
  return (
    <div>
      <div className="flex justify-between text-xs mb-1">
        <span style={{ color: 'var(--cp-text)' }}>{label}</span>
        <span style={{ color: 'var(--cp-muted)' }}>{formatTokens(value)}</span>
      </div>
      <div className="h-2 rounded-full overflow-hidden" style={{ background: 'var(--cp-bg)' }}>
        <div className="h-full rounded-full" style={{ width: `${percent}%`, background: 'var(--cp-accent)' }} />
      </div>
    </div>
  )
}

function Breakdown({ title, rows, total }: { title: string; rows: Array<[string, number]>; total: number }) {
  return (
    <section className="rounded-xl p-4" style={{ background: 'var(--cp-surface)', border: '1px solid var(--cp-border)' }}>
      <h3 className="text-sm font-medium mb-3" style={{ color: 'var(--cp-text)' }}>{title}</h3>
      <div className="flex flex-col gap-3">
        {rows.map(([label, value]) => (
          <MeterRow key={label} label={label} value={value} max={total} />
        ))}
      </div>
    </section>
  )
}
