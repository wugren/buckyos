import { useState } from 'react'
import { AlertTriangle, ChevronDown, ChevronRight, RefreshCw, ShieldCheck, Trash2 } from 'lucide-react'
import { useI18n } from '../../../../i18n/provider'
import { useAICCStore } from '../../hooks/use-aicc-store'
import { StatusBadge } from '../shared/StatusBadge'
import { ConfirmDialog } from '../shared/ConfirmDialog'
import type { AuthStatus, ModelMetadata, ProviderView } from '../../../../api/aicc_mgr'

function authStatusLabel(s: AuthStatus, t: (k: string, f: string) => string): string {
  switch (s) {
    case 'ok': return t('aiCenter.providers.authOk', 'Valid')
    case 'expired': return t('aiCenter.providers.authExpired', 'Expired')
    case 'invalid': return t('aiCenter.providers.authInvalid', 'Invalid')
    default: return t('aiCenter.providers.authUnknown', 'Unknown')
  }
}

function authStatusVariant(s: AuthStatus): 'ok' | 'warning' | 'error' | 'unknown' {
  switch (s) {
    case 'ok': return 'ok'
    case 'expired': return 'warning'
    case 'invalid': return 'error'
    default: return 'unknown'
  }
}

function modelHealthVariant(status: ModelMetadata['health']['status']): 'ok' | 'warning' | 'error' {
  if (status === 'available') return 'ok'
  if (status === 'degraded') return 'warning'
  return 'error'
}

interface ProviderDetailPanelProps {
  provider: ProviderView
  onDeleted: () => void
}

export function ProviderDetailPanel({ provider, onDeleted }: ProviderDetailPanelProps) {
  const { t } = useI18n()
  const store = useAICCStore()
  const [showModels, setShowModels] = useState(true)
  const [confirmDelete, setConfirmDelete] = useState(false)

  const { config, status, account, inventory } = provider
  const models = status.discovered_models
  const degradedCount = models.filter((m) => m.health.status === 'degraded').length
  const quotaWarningCount = models.filter((m) => m.health.quota_state === 'near_limit' || m.health.quota_state === 'exhausted').length

  const handleDelete = () => {
    void store.deleteProvider(config.id)
      .then(onDeleted)
      .catch((error) => console.error('aicc.deleteProvider failed', error))
  }

  const balanceDisplay = account.balance_supported && account.balance_value != null
    ? `${account.balance_unit === 'usd' ? '$' : ''}${account.balance_value}${account.balance_unit === 'credit' ? ' Credit' : ''}`
    : t('aiCenter.providers.usageOnly', 'Usage only')

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-col gap-1">
        <h2 className="text-lg font-semibold" style={{ color: 'var(--cp-text)' }}>
          {config.name}
        </h2>
        <div className="text-xs" style={{ color: 'var(--cp-muted)' }}>
          {config.provider_instance_name} · {config.provider_runtime_type} · {config.provider_origin}
        </div>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-3 gap-3">
        <Metric label={t('aiCenter.providers.inventoryModels', 'Inventory Models')} value={models.length.toString()} detail={inventory.inventory_revision} />
        <Metric label={t('aiCenter.providers.health', 'Health')} value={`${models.length - degradedCount}/${models.length}`} detail={t('aiCenter.providers.availableModels', 'available models')} />
        <Metric label={t('aiCenter.providers.quota', 'Quota')} value={quotaWarningCount ? `${quotaWarningCount}` : '0'} detail={quotaWarningCount ? t('aiCenter.providers.quotaWarning', 'needs attention') : t('aiCenter.providers.quotaNormal', 'normal')} />
      </div>

      <div
        className="rounded-xl p-4 flex flex-col gap-3"
        style={{ background: 'var(--cp-surface)', border: '1px solid var(--cp-border)' }}
      >
        <Row label={t('aiCenter.providers.driver', 'Driver')} value={config.provider_driver} />
        <Row label={t('aiCenter.providers.runtimeType', 'Runtime Type')} value={config.provider_runtime_type} />
        <Row label={t('aiCenter.providers.endpoint', 'Endpoint')} value={config.endpoint || t('aiCenter.providers.default', 'Default')} />
        <Row
          label={t('aiCenter.providers.auth', 'Authentication')}
          value={
            <span className="inline-flex items-center gap-2">
              {config.auth_mode ?? '—'}
              <StatusBadge status={authStatusVariant(status.auth_status)} label={authStatusLabel(status.auth_status, t)} />
            </span>
          }
        />
        <Row label={t('aiCenter.providers.pricingMode', 'Pricing Mode')} value={account.pricing_mode} />
        <Row label={t('aiCenter.providers.balance', 'Balance')} value={balanceDisplay} />
        <Row label={t('aiCenter.providers.lastSync', 'Last Sync')} value={status.last_model_sync_at?.slice(0, 16).replace('T', ' ') ?? '—'} />
        <Row
          label={t('aiCenter.providers.autoSync', 'Auto Sync')}
          value={config.auto_sync_models ? t('common.on', 'On') : t('common.off', 'Off')}
        />
      </div>

      {status.model_sync_status !== 'ok' && (
        <div
          className="flex items-start gap-2 rounded-lg p-3 text-sm"
          style={{
            background: 'color-mix(in oklch, var(--cp-warning), transparent 88%)',
            color: 'var(--cp-text)',
          }}
        >
          <AlertTriangle size={16} style={{ color: 'var(--cp-warning)' }} />
          <span>{t('aiCenter.providers.syncFailed', 'Last inventory sync failed. Existing models remain usable, but refresh is recommended.')}</span>
        </div>
      )}

      <button
        type="button"
        onClick={() => setShowModels(!showModels)}
        className="inline-flex items-center gap-1 text-sm self-start"
        style={{ color: 'var(--cp-accent)' }}
      >
        {showModels ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
        {t('aiCenter.providers.inventory', 'Provider Inventory')}
      </button>

      {showModels && (
        <div className="grid grid-cols-1 gap-3">
          {models.map((m) => (
            <ModelInventoryRow key={m.exact_model} model={m} />
          ))}
        </div>
      )}

      <div className="flex flex-wrap gap-2">
        <ActionButton
          icon={<ShieldCheck size={14} />}
          label={t('aiCenter.providers.updateKey', 'Update Key')}
          onClick={() => console.log('update key')}
        />
        <ActionButton
          icon={<RefreshCw size={14} />}
          label={t('aiCenter.providers.refreshModels', 'Refresh Models')}
          onClick={() => {
            void store.refreshProviderModels(config.id)
              .catch((error) => console.error('aicc.refreshProviderModels failed', error))
          }}
        />
        <ActionButton
          icon={<Trash2 size={14} />}
          label={t('aiCenter.providers.delete', 'Delete')}
          onClick={() => setConfirmDelete(true)}
          danger
        />
      </div>

      <ConfirmDialog
        open={confirmDelete}
        title={t('aiCenter.providers.deleteTitle', 'Delete Provider')}
        message={t('aiCenter.providers.deleteConfirm', 'Are you sure you want to delete this provider? This action cannot be undone.')}
        confirmLabel={t('aiCenter.providers.delete', 'Delete')}
        onConfirm={handleDelete}
        onCancel={() => setConfirmDelete(false)}
      />
    </div>
  )
}

function Metric({ label, value, detail }: { label: string; value: string; detail?: string }) {
  return (
    <div className="rounded-xl p-3" style={{ background: 'var(--cp-surface)', border: '1px solid var(--cp-border)' }}>
      <div className="text-xs" style={{ color: 'var(--cp-muted)' }}>{label}</div>
      <div className="text-xl font-semibold" style={{ color: 'var(--cp-text)' }}>{value}</div>
      {detail && <div className="text-[11px] truncate" style={{ color: 'var(--cp-muted)' }}>{detail}</div>}
    </div>
  )
}

function ModelInventoryRow({ model }: { model: ModelMetadata }) {
  return (
    <div className="rounded-xl p-3" style={{ background: 'var(--cp-surface)', border: '1px solid var(--cp-border)' }}>
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div className="min-w-0">
          <div className="text-sm font-medium truncate" style={{ color: 'var(--cp-text)' }}>{model.exact_model}</div>
          <div className="text-xs truncate" style={{ color: 'var(--cp-muted)' }}>
            {model.logical_mounts.join(', ')} · {model.api_types.join(', ')}
          </div>
        </div>
        <StatusBadge status={modelHealthVariant(model.health.status)} label={model.health.status} />
      </div>
      <div className="grid grid-cols-2 md:grid-cols-4 gap-2 mt-3 text-xs">
        <Chip label="quality" value={model.attributes.quality_score?.toString() ?? '—'} />
        <Chip label="latency" value={`${model.attributes.latency_class}${model.health.p95_latency_ms ? ` p95 ${model.health.p95_latency_ms}ms` : ''}`} />
        <Chip label="cost" value={model.attributes.cost_class} />
        <Chip label="quota" value={model.health.quota_state} />
      </div>
    </div>
  )
}

function Chip({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-md px-2 py-1" style={{ background: 'var(--cp-bg)' }}>
      <span style={{ color: 'var(--cp-muted)' }}>{label}: </span>
      <span style={{ color: 'var(--cp-text)' }}>{value}</span>
    </div>
  )
}

function Row({ label, value }: { label: string; value: React.ReactNode }) {
  return (
    <div className="flex justify-between gap-4 items-center text-sm">
      <span style={{ color: 'var(--cp-muted)' }}>{label}</span>
      <span className="font-medium text-right break-all" style={{ color: 'var(--cp-text)' }}>{value}</span>
    </div>
  )
}

function ActionButton({ icon, label, onClick, danger }: { icon: React.ReactNode; label: string; onClick: () => void; danger?: boolean }) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="inline-flex items-center gap-2 px-3 py-1.5 rounded-lg text-xs font-medium transition-opacity hover:opacity-80"
      style={{
        border: `1px solid ${danger ? 'var(--cp-danger)' : 'var(--cp-border)'}`,
        color: danger ? 'var(--cp-danger)' : 'var(--cp-text)',
      }}
    >
      {icon}
      {label}
    </button>
  )
}
