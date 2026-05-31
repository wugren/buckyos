import { useMemo, useState } from 'react'
import { useMediaQuery } from '@mui/material'
import {
  AlertTriangle,
  ChevronRight,
  FileCog,
  Folder,
  FolderOpen,
  GitBranch,
  HardDrive,
  Lock,
  Route,
} from 'lucide-react'
import { useI18n } from '../../i18n/provider'
import {
  useLocalModels,
  useProviders,
  useRouteTraces,
  useSessionConfig,
} from './hooks/use-aicc-store'
import { StatusBadge } from './components/shared/StatusBadge'
import type { LogicalNode, ModelMetadata, RouteTrace } from '../../api/aicc_mgr'

type DirectoryEntry = {
  node: LogicalNode
  kind: 'folder' | 'file'
  weight?: number
  itemName?: string
}

export function RoutingPage() {
  const { t } = useI18n()
  const sessionConfig = useSessionConfig()
  const traces = useRouteTraces()
  const providers = useProviders()
  const localModels = useLocalModels()
  const isMobile = useMediaQuery('(max-width: 767px)')
  const [currentPath, setCurrentPath] = useState<string | null>(sessionConfig.logical_tree[0]?.path ?? null)
  const [selectedFilePath, setSelectedFilePath] = useState<string | null>(null)

  const modelByExactName = useMemo(() => {
    const models = [
      ...providers.flatMap((provider) => provider.status.discovered_models),
      ...localModels,
    ]
    return new Map(models.map((model) => [model.exact_model, model]))
  }, [providers, localModels])

  const currentNode = findNode(sessionConfig.logical_tree, currentPath) ?? sessionConfig.logical_tree[0]
  const selectedFile = selectedFilePath ? findNode(sessionConfig.logical_tree, selectedFilePath) : undefined
  const selectedModel = selectedFilePath ? modelByExactName.get(selectedFilePath) : undefined
  const currentEntries = currentNode ? directoryEntries(currentNode) : []

  if (sessionConfig.logical_tree.length === 0 || !currentNode) {
    return (
      <div className="flex flex-col items-center justify-center py-16 text-center">
        <GitBranch size={40} style={{ color: 'var(--cp-muted)' }} />
        <p className="text-sm mt-3" style={{ color: 'var(--cp-muted)' }}>
          {t('aiCenter.routing.notConfigured', 'No logical directory configured')}
        </p>
      </div>
    )
  }

  const handleOpenFolder = (node: LogicalNode) => {
    setCurrentPath(node.path)
    setSelectedFilePath(null)
  }

  const handleSelectFile = (node: LogicalNode) => {
    setSelectedFilePath(node.path)
  }

  return (
    <div className="flex flex-col gap-4">
      <RoutingHeader revision={sessionConfig.revision} />

      <div className={isMobile ? 'flex flex-col gap-4' : 'grid grid-cols-[minmax(0,1.15fr)_minmax(320px,0.85fr)] gap-5 items-start'}>
        <section
          className="rounded-xl overflow-hidden"
          style={{ background: 'var(--cp-surface)', border: '1px solid var(--cp-border)' }}
        >
          <DirectoryToolbar
            currentNode={currentNode}
            roots={sessionConfig.logical_tree}
            onOpenFolder={handleOpenFolder}
          />
          <FolderInspector node={currentNode} compact={isMobile} />
          <DirectoryList
            entries={currentEntries}
            selectedFilePath={selectedFilePath}
            onOpenFolder={handleOpenFolder}
            onSelectFile={handleSelectFile}
          />
        </section>

        <aside className="flex flex-col gap-4">
          {selectedFile ? (
            <ModelInspector node={selectedFile} model={selectedModel} />
          ) : (
            <FolderDetail node={currentNode} />
          )}
          <TraceExplorer traces={traces} compact={isMobile} />
        </aside>
      </div>
    </div>
  )
}

function RoutingHeader({ revision }: { revision?: string }) {
  const { t } = useI18n()
  return (
    <div className="flex flex-col gap-1">
      <h2 className="text-lg font-semibold" style={{ color: 'var(--cp-text)' }}>
        {t('aiCenter.routing.title', 'Routing Logical Directory')}
      </h2>
      <p className="text-sm" style={{ color: 'var(--cp-muted)' }}>
        {t('aiCenter.routing.subtitle', 'Browse logical folders and exact model files. Folders show routing requirements; files show physical model attributes.')}
      </p>
      <div className="text-xs" style={{ color: 'var(--cp-muted)' }}>
        {t('aiCenter.routing.revision', 'Revision')}: {revision ?? '—'}
      </div>
    </div>
  )
}

function DirectoryToolbar({
  currentNode,
  roots,
  onOpenFolder,
}: {
  currentNode: LogicalNode
  roots: LogicalNode[]
  onOpenFolder: (node: LogicalNode) => void
}) {
  const ancestors = findAncestors(roots, currentNode.path)

  return (
    <div className="p-4" style={{ borderBottom: '1px solid var(--cp-border)' }}>
      <div className="flex items-center gap-2 mb-3">
        <FolderOpen size={18} style={{ color: 'var(--cp-accent)' }} />
        <h3 className="text-base font-semibold font-mono truncate" style={{ color: 'var(--cp-text)' }}>
          {currentNode.path}
        </h3>
        {currentNode.locked && <Lock size={14} style={{ color: 'var(--cp-warning)' }} />}
      </div>

      <div className="flex flex-wrap items-center gap-1">
        {ancestors.map((node, index) => (
          <div key={node.path} className="flex items-center gap-1">
            {index > 0 && <ChevronRight size={13} style={{ color: 'var(--cp-muted)' }} />}
            <button
              type="button"
              onClick={() => onOpenFolder(node)}
              className="min-h-9 rounded-md px-2 text-xs font-mono"
              style={{
                background: node.path === currentNode.path ? 'var(--cp-surface-2)' : 'var(--cp-bg)',
                color: node.path === currentNode.path ? 'var(--cp-text)' : 'var(--cp-muted)',
              }}
            >
              {node.path}
            </button>
          </div>
        ))}
      </div>
    </div>
  )
}

function FolderInspector({ node, compact }: { node: LogicalNode; compact: boolean }) {
  return (
    <div className="p-4" style={{ borderBottom: '1px solid var(--cp-border)' }}>
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="text-xs mb-1" style={{ color: 'var(--cp-muted)' }}>
            Current folder requirements
          </div>
          <div className="text-sm" style={{ color: 'var(--cp-text)' }}>
            {node.label} · {node.api_type ?? 'mixed'} · resolved to {node.resolved_exact_model ?? 'none'}
          </div>
        </div>
        <StatusBadge status={node.resolved_exact_model ? 'ok' : 'warning'} label={node.resolved_exact_model ? 'resolved' : 'unresolved'} />
      </div>

      <div className={compact ? 'grid grid-cols-1 gap-2 mt-3' : 'grid grid-cols-3 gap-3 mt-3'}>
        <PolicyFact label="Profile" value={node.policy?.profile ?? 'inherit'} />
        <PolicyFact label="Fallback" value={`${node.fallback?.mode ?? 'inherit'}${node.fallback?.target ? ` -> ${node.fallback.target}` : ''}`} />
        <PolicyFact label="Required" value={node.policy?.required_features?.join(', ') || 'none'} />
      </div>
    </div>
  )
}

function DirectoryList({
  entries,
  selectedFilePath,
  onOpenFolder,
  onSelectFile,
}: {
  entries: DirectoryEntry[]
  selectedFilePath: string | null
  onOpenFolder: (node: LogicalNode) => void
  onSelectFile: (node: LogicalNode) => void
}) {
  if (entries.length === 0) {
    return (
      <div className="p-6 text-sm" style={{ color: 'var(--cp-muted)' }}>
        This logical folder has no child folders or exact model files.
      </div>
    )
  }

  return (
    <div className="p-2">
      {entries.map((entry) => {
        const selected = entry.kind === 'file' && selectedFilePath === entry.node.path
        return (
          <button
            key={`${entry.kind}-${entry.node.path}-${entry.itemName ?? ''}`}
            type="button"
            onClick={() => entry.kind === 'folder' ? onOpenFolder(entry.node) : onSelectFile(entry.node)}
            className="w-full min-h-14 rounded-lg px-3 py-2 text-left flex items-center gap-3"
            style={{
              background: selected ? 'var(--cp-surface-2)' : 'transparent',
              color: 'var(--cp-text)',
            }}
          >
            <EntryIcon entry={entry} />
            <div className="min-w-0 flex-1">
              <div className="flex items-center gap-2">
                <span className="text-sm font-mono truncate">{entry.node.path}</span>
                {entry.node.locked && <Lock size={12} style={{ color: 'var(--cp-warning)' }} />}
              </div>
              <div className="text-xs truncate" style={{ color: 'var(--cp-muted)' }}>
                {entry.kind === 'folder'
                  ? `${entry.node.label} · ${entry.node.resolved_exact_model ? `resolves ${entry.node.resolved_exact_model}` : 'no physical model'}`
                  : entry.node.label}
              </div>
            </div>
            <WeightPill weight={entry.weight} />
            {entry.kind === 'folder' && <ChevronRight size={16} style={{ color: 'var(--cp-muted)' }} />}
          </button>
        )
      })}
    </div>
  )
}

function EntryIcon({ entry }: { entry: DirectoryEntry }) {
  if (entry.kind === 'file') {
    return (
      <div className="h-9 w-9 shrink-0 rounded-lg flex items-center justify-center" style={{ background: 'var(--cp-bg)', color: 'var(--cp-accent)' }}>
        <FileCog size={17} />
      </div>
    )
  }

  return (
    <div className="h-9 w-9 shrink-0 rounded-lg flex items-center justify-center" style={{ background: 'var(--cp-bg)', color: entry.node.resolved_exact_model ? 'var(--cp-accent)' : 'var(--cp-warning)' }}>
      <Folder size={17} />
    </div>
  )
}

function WeightPill({ weight }: { weight?: number }) {
  return (
    <div className="w-20 shrink-0">
      <div className="text-[11px] text-right" style={{ color: 'var(--cp-muted)' }}>
        {weight == null ? 'weight -' : `weight ${weight}`}
      </div>
      <div className="h-1.5 rounded-full mt-1 overflow-hidden" style={{ background: 'var(--cp-bg)' }}>
        <div
          className="h-full rounded-full"
          style={{
            width: `${weight == null ? 0 : Math.min(100, weight * 34)}%`,
            background: 'var(--cp-accent)',
          }}
        />
      </div>
    </div>
  )
}

function FolderDetail({ node }: { node: LogicalNode }) {
  const itemRows = Object.entries(node.items ?? {})

  return (
    <section className="rounded-xl p-4" style={{ background: 'var(--cp-surface)', border: '1px solid var(--cp-border)' }}>
      <div className="flex items-center gap-2 mb-3">
        <FolderOpen size={16} style={{ color: 'var(--cp-accent)' }} />
        <h3 className="text-sm font-semibold" style={{ color: 'var(--cp-text)' }}>
          Folder Properties
        </h3>
      </div>
      <div className="grid grid-cols-1 gap-2 text-sm">
        <Fact label="Logical path" value={node.path} mono />
        <Fact label="API type" value={node.api_type ?? 'mixed'} />
        <Fact label="Resolved exact model" value={node.resolved_exact_model ?? 'none'} mono />
        <Fact label="Allow fallback" value={node.policy?.allow_fallback === false ? 'no' : 'yes'} />
        <Fact label="Runtime failover" value={node.policy?.runtime_failover === false ? 'off' : 'on'} />
        <Fact label="Locked" value={node.locked ? 'yes' : 'no'} />
      </div>

      {itemRows.length > 0 && (
        <div className="mt-4">
          <h4 className="text-xs font-medium mb-2" style={{ color: 'var(--cp-muted)' }}>
            Weight order in this folder
          </h4>
          <div className="flex flex-col gap-2">
            {itemRows
              .sort((a, b) => b[1].weight - a[1].weight)
              .map(([name, item]) => (
                <div key={name} className="rounded-lg px-3 py-2" style={{ background: 'var(--cp-bg)' }}>
                  <div className="flex items-center justify-between gap-3">
                    <div className="min-w-0">
                      <div className="text-sm" style={{ color: 'var(--cp-text)' }}>{name}</div>
                      <div className="text-xs font-mono truncate" style={{ color: 'var(--cp-muted)' }}>{item.target}</div>
                    </div>
                    <span className="text-xs" style={{ color: 'var(--cp-muted)' }}>{item.weight}</span>
                  </div>
                </div>
              ))}
          </div>
        </div>
      )}
    </section>
  )
}

function ModelInspector({ node, model }: { node: LogicalNode; model?: ModelMetadata }) {
  return (
    <section className="rounded-xl p-4" style={{ background: 'var(--cp-surface)', border: '1px solid var(--cp-border)' }}>
      <div className="flex items-start justify-between gap-3 mb-3">
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <HardDrive size={16} style={{ color: model ? 'var(--cp-accent)' : 'var(--cp-warning)' }} />
            <h3 className="text-sm font-semibold" style={{ color: 'var(--cp-text)' }}>
              Physical Model File
            </h3>
          </div>
          <div className="text-xs font-mono truncate mt-1" style={{ color: 'var(--cp-muted)' }}>
            {node.path}
          </div>
        </div>
        <StatusBadge
          status={model ? (model.health.status === 'available' ? 'ok' : model.health.status === 'degraded' ? 'warning' : 'error') : 'warning'}
          label={model?.health.status ?? 'not discovered'}
        />
      </div>

      {model ? (
        <div className="grid grid-cols-1 gap-2 text-sm">
          <Fact label="Provider model id" value={model.provider_model_id} mono />
          <Fact label="Provider instance" value={providerNameFromExact(model.exact_model)} mono />
          <Fact label="API types" value={model.api_types.join(', ')} />
          <Fact label="Logical mounts" value={model.logical_mounts.join(', ')} mono />
          <Fact label="Privacy" value={model.attributes.privacy} />
          <Fact label="Quality score" value={model.attributes.quality_score?.toString() ?? 'unknown'} />
          <Fact label="Latency" value={`${model.attributes.latency_class}${model.health.p95_latency_ms ? ` · p95 ${model.health.p95_latency_ms}ms` : ''}`} />
          <Fact label="Cost class" value={model.attributes.cost_class} />
          <Fact label="Quota" value={model.health.quota_state} />
          <Fact label="Capabilities" value={capabilitySummary(model)} />
        </div>
      ) : (
        <div className="flex items-start gap-2 rounded-lg p-3" style={{ background: 'var(--cp-bg)' }}>
          <AlertTriangle size={16} style={{ color: 'var(--cp-warning)' }} />
          <div className="text-sm" style={{ color: 'var(--cp-text)' }}>
            This exact model file is part of the logical directory mock, but no provider inventory currently supplies metadata for it.
          </div>
        </div>
      )}
    </section>
  )
}

function TraceExplorer({ traces, compact }: { traces: RouteTrace[]; compact: boolean }) {
  return (
    <section className="rounded-xl p-4" style={{ background: 'var(--cp-surface)', border: '1px solid var(--cp-border)' }}>
      <div className="flex items-center gap-2 mb-3">
        <Route size={16} style={{ color: 'var(--cp-accent)' }} />
        <h3 className="text-sm font-medium" style={{ color: 'var(--cp-text)' }}>Route Trace</h3>
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
                ? `${trace.selected_exact_model} · ${trace.selected_provider_instance_name}`
                : 'No exact model resolved'}
            </div>
            <p className="text-sm mt-2" style={{ color: 'var(--cp-text)' }}>{trace.user_summary?.reason_short}</p>
            <div className="flex flex-col gap-1 mt-3">
              {trace.ranked_candidates.length > 0 ? trace.ranked_candidates.map((candidate) => (
                <div key={candidate.exact_model} className="flex justify-between gap-3 text-xs">
                  <span style={{ color: candidate.selected ? 'var(--cp-accent)' : 'var(--cp-muted)' }}>{candidate.exact_model}</span>
                  <span style={{ color: 'var(--cp-muted)' }}>{candidate.final_score?.toFixed(2)}</span>
                </div>
              )) : trace.filtered_candidates.map((candidate) => (
                <div key={candidate.exact_model} className="flex flex-col gap-0.5 text-xs">
                  <span style={{ color: 'var(--cp-warning)' }}>{candidate.exact_model}</span>
                  <span style={{ color: 'var(--cp-muted)' }}>{candidate.reason}</span>
                </div>
              ))}
            </div>
            {trace.warnings.length > 0 && (
              <div className="text-xs mt-2" style={{ color: 'var(--cp-warning)' }}>{trace.warnings.join(' · ')}</div>
            )}
          </article>
        ))}
      </div>
    </section>
  )
}

function PolicyFact({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-lg p-3" style={{ background: 'var(--cp-bg)' }}>
      <div className="text-xs" style={{ color: 'var(--cp-muted)' }}>{label}</div>
      <div className="text-sm font-medium break-words" style={{ color: 'var(--cp-text)' }}>{value}</div>
    </div>
  )
}

function Fact({ label, value, mono }: { label: string; value: string; mono?: boolean }) {
  return (
    <div className="flex justify-between gap-3 rounded-lg px-3 py-2" style={{ background: 'var(--cp-bg)' }}>
      <span className="text-xs shrink-0" style={{ color: 'var(--cp-muted)' }}>{label}</span>
      <span className={mono ? 'text-xs font-mono text-right break-all' : 'text-xs text-right break-words'} style={{ color: 'var(--cp-text)' }}>
        {value}
      </span>
    </div>
  )
}

function directoryEntries(node: LogicalNode): DirectoryEntry[] {
  const children = node.children ?? []
  const weightByTarget = new Map<string, { weight: number; itemName: string }>()
  Object.entries(node.items ?? {}).forEach(([itemName, item]) => {
    weightByTarget.set(item.target, { weight: item.weight, itemName })
  })

  return children
    .map((child) => {
      const item = weightByTarget.get(child.path)
      return {
        node: child,
        kind: child.level === 'L1' ? 'file' as const : 'folder' as const,
        weight: item?.weight ?? node.exact_model_weights[child.path],
        itemName: item?.itemName,
      }
    })
    .sort((a, b) => {
      const weightDiff = (b.weight ?? -1) - (a.weight ?? -1)
      if (weightDiff !== 0) return weightDiff
      if (a.kind !== b.kind) return a.kind === 'folder' ? -1 : 1
      return a.node.path.localeCompare(b.node.path)
    })
}

function findNode(nodes: LogicalNode[], path: string | null): LogicalNode | undefined {
  if (!path) return undefined
  for (const node of nodes) {
    if (node.path === path) return node
    const child = findNode(node.children ?? [], path)
    if (child) return child
  }
  return undefined
}

function findAncestors(nodes: LogicalNode[], path: string): LogicalNode[] {
  for (const node of nodes) {
    if (node.path === path) return [node]
    const childPath = findAncestors(node.children ?? [], path)
    if (childPath.length > 0) return [node, ...childPath]
  }
  return []
}

function providerNameFromExact(exactModel: string): string {
  return exactModel.split('@')[1] ?? 'unknown'
}

function capabilitySummary(model: ModelMetadata): string {
  const caps = []
  if (model.capabilities.streaming) caps.push('streaming')
  if (model.capabilities.tool_call) caps.push('tool call')
  if (model.capabilities.json_schema) caps.push('json schema')
  if (model.capabilities.web_search) caps.push('web search')
  if (model.capabilities.vision) caps.push('vision')
  return caps.length > 0 ? caps.join(', ') : 'none'
}
