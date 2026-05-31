import { useState } from 'react'
import { useMediaQuery } from '@mui/material'
import { ChevronDown, ChevronRight, GitBranch, Lock, Route } from 'lucide-react'
import { useI18n } from '../../i18n/provider'
import { useRouteTraces, useSessionConfig } from './hooks/use-mock-store'
import { StatusBadge } from './components/shared/StatusBadge'
import type { LogicalNode, RouteTrace } from './mock/types'

export function RoutingPage() {
  const { t } = useI18n()
  const sessionConfig = useSessionConfig()
  const traces = useRouteTraces()
  const isMobile = useMediaQuery('(max-width: 767px)')
  const [selectedPath, setSelectedPath] = useState<string | null>(sessionConfig.logical_tree[0]?.path ?? null)

  const selectedNode = findNode(sessionConfig.logical_tree, selectedPath)

  if (sessionConfig.logical_tree.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center py-16 text-center">
        <GitBranch size={40} style={{ color: 'var(--cp-muted)' }} />
        <p className="text-sm mt-3" style={{ color: 'var(--cp-muted)' }}>
          {t('aiCenter.routing.notConfigured', 'No logical directory configured')}
        </p>
      </div>
    )
  }

  if (isMobile) {
    return (
      <div className="flex flex-col gap-4">
        <RoutingHeader revision={sessionConfig.revision} />
        {flattenNodes(sessionConfig.logical_tree).map((node) => (
          <NodeCard key={node.path} node={node} selected={selectedPath === node.path} onSelect={() => setSelectedPath(node.path)} />
        ))}
        {selectedNode && <NodeInspector node={selectedNode} />}
        <TraceExplorer traces={traces} />
      </div>
    )
  }

  return (
    <div className="flex flex-col gap-5">
      <RoutingHeader revision={sessionConfig.revision} />
      <div className="grid grid-cols-1 gap-5">
        <section className="rounded-xl overflow-hidden" style={{ background: 'var(--cp-surface)', border: '1px solid var(--cp-border)' }}>
          <div className="px-4 py-3" style={{ borderBottom: '1px solid var(--cp-border)' }}>
            <h3 className="text-sm font-medium" style={{ color: 'var(--cp-text)' }}>
              {t('aiCenter.routing.directory', 'Logical Directory')}
            </h3>
          </div>
          <div className="p-2 max-h-80 overflow-y-auto">
            {sessionConfig.logical_tree.map((node) => (
              <TreeNode key={node.path} node={node} selectedPath={selectedPath} onSelect={setSelectedPath} />
            ))}
          </div>
        </section>
        {selectedNode && <NodeInspector node={selectedNode} />}
      </div>
      <TraceExplorer traces={traces} />
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
        {t('aiCenter.routing.subtitle', 'AICC resolves L3 task paths through L2 logical mounts into L1 exact models with weights, profile and fallback policy.')}
      </p>
      <div className="text-xs" style={{ color: 'var(--cp-muted)' }}>
        {t('aiCenter.routing.revision', 'Revision')}: {revision ?? '—'}
      </div>
    </div>
  )
}

function TreeNode({ node, selectedPath, onSelect, depth = 0 }: { node: LogicalNode; selectedPath: string | null; onSelect: (path: string) => void; depth?: number }) {
  const [open, setOpen] = useState(true)
  const hasChildren = !!node.children?.length
  const active = selectedPath === node.path

  return (
    <div>
      <button
        type="button"
        onClick={() => onSelect(node.path)}
        className="w-full flex items-center gap-2 rounded-lg px-2 py-2 text-left"
        style={{
          paddingLeft: `${8 + depth * 18}px`,
          background: active ? 'var(--cp-surface-2)' : 'transparent',
          color: active ? 'var(--cp-text)' : 'var(--cp-muted)',
        }}
      >
        {hasChildren ? (
          <span
            onClick={(event) => {
              event.stopPropagation()
              setOpen(!open)
            }}
          >
            {open ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
          </span>
        ) : (
          <span className="w-3.5" />
        )}
        <span className="text-[11px] rounded px-1.5 py-0.5" style={{ background: 'var(--cp-bg)' }}>{node.level}</span>
        <span className="text-sm font-mono truncate">{node.path}</span>
        {node.locked && <Lock size={12} />}
      </button>
      {hasChildren && open && node.children?.map((child) => (
        <TreeNode key={child.path} node={child} selectedPath={selectedPath} onSelect={onSelect} depth={depth + 1} />
      ))}
    </div>
  )
}

function NodeCard({ node, selected, onSelect }: { node: LogicalNode; selected: boolean; onSelect: () => void }) {
  return (
    <button
      type="button"
      onClick={onSelect}
      className="rounded-xl p-4 text-left"
      style={{
        background: selected ? 'var(--cp-surface-2)' : 'var(--cp-surface)',
        border: '1px solid var(--cp-border)',
      }}
    >
      <div className="flex justify-between gap-3">
        <span className="text-sm font-mono" style={{ color: 'var(--cp-text)' }}>{node.path}</span>
        <StatusBadge status={node.resolved_exact_model ? 'ok' : 'warning'} />
      </div>
      <div className="text-xs mt-1" style={{ color: 'var(--cp-muted)' }}>{node.resolved_exact_model ?? 'No candidate'}</div>
    </button>
  )
}

function NodeInspector({ node }: { node: LogicalNode }) {
  const itemRows = Object.entries(node.items ?? {})

  return (
    <section className="rounded-xl p-4" style={{ background: 'var(--cp-surface)', border: '1px solid var(--cp-border)' }}>
      <div className="flex flex-wrap items-start justify-between gap-3 mb-4">
        <div>
          <div className="flex items-center gap-2">
            <h3 className="text-base font-semibold font-mono" style={{ color: 'var(--cp-text)' }}>{node.path}</h3>
            {node.locked && (
              <span className="inline-flex items-center gap-1 text-xs" style={{ color: 'var(--cp-warning)' }}>
                <Lock size={12} /> locked
              </span>
            )}
          </div>
          <div className="text-xs mt-1" style={{ color: 'var(--cp-muted)' }}>
            {node.label} · {node.api_type ?? 'mixed'} · resolved to {node.resolved_exact_model ?? '—'}
          </div>
        </div>
        <StatusBadge status={node.resolved_exact_model ? 'ok' : 'warning'} label={node.resolved_exact_model ? 'resolved' : 'unresolved'} />
      </div>

      <div className="grid grid-cols-1 md:grid-cols-3 gap-3 mb-4">
        <PolicyFact label="Scheduler Profile" value={node.policy?.profile ?? 'inherit'} />
        <PolicyFact label="Fallback" value={`${node.fallback?.mode ?? 'inherit'}${node.fallback?.target ? ` → ${node.fallback.target}` : ''}`} />
        <PolicyFact label="Runtime Failover" value={node.policy?.runtime_failover === false ? 'off' : 'on'} />
      </div>

      {itemRows.length > 0 && (
        <div>
          <h4 className="text-sm font-medium mb-2" style={{ color: 'var(--cp-text)' }}>Weighted Items</h4>
          <div className="flex flex-col gap-2">
            {itemRows.map(([name, item]) => (
              <div key={name} className="rounded-lg p-3" style={{ background: 'var(--cp-bg)' }}>
                <div className="flex items-center justify-between gap-3">
                  <div className="min-w-0">
                    <div className="text-sm" style={{ color: 'var(--cp-text)' }}>{name}</div>
                    <div className="text-xs font-mono truncate" style={{ color: 'var(--cp-muted)' }}>{item.target}</div>
                  </div>
                  <div className="w-24">
                    <div className="text-xs text-right" style={{ color: 'var(--cp-muted)' }}>weight {item.weight}</div>
                    <div className="h-2 rounded-full overflow-hidden mt-1" style={{ background: 'var(--cp-surface)' }}>
                      <div className="h-full" style={{ width: `${Math.min(100, item.weight * 32)}%`, background: 'var(--cp-accent)' }} />
                    </div>
                  </div>
                </div>
              </div>
            ))}
          </div>
        </div>
      )}

      {Object.keys(node.exact_model_weights).length > 0 && (
        <div className="mt-4">
          <h4 className="text-sm font-medium mb-2" style={{ color: 'var(--cp-text)' }}>Exact Model Weights</h4>
          <div className="flex flex-wrap gap-2">
            {Object.entries(node.exact_model_weights).map(([model, weight]) => (
              <span key={model} className="text-xs rounded-md px-2 py-1" style={{ background: 'var(--cp-bg)', color: 'var(--cp-text)' }}>
                {model}: {weight}
              </span>
            ))}
          </div>
        </div>
      )}

      <div className="mt-4 text-xs" style={{ color: 'var(--cp-muted)' }}>
        Policy: required features {node.policy?.required_features?.join(', ') || 'none'} · local only {node.policy?.local_only ? 'yes' : 'no'} · allow fallback {node.policy?.allow_fallback === false ? 'no' : 'yes'}
      </div>
    </section>
  )
}

function TraceExplorer({ traces }: { traces: RouteTrace[] }) {
  return (
    <section className="rounded-xl p-4" style={{ background: 'var(--cp-surface)', border: '1px solid var(--cp-border)' }}>
      <div className="flex items-center gap-2 mb-3">
        <Route size={16} style={{ color: 'var(--cp-accent)' }} />
        <h3 className="text-sm font-medium" style={{ color: 'var(--cp-text)' }}>Route Trace Explorer</h3>
      </div>
      <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
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
      <div className="text-sm font-medium" style={{ color: 'var(--cp-text)' }}>{value}</div>
    </div>
  )
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

function flattenNodes(nodes: LogicalNode[]): LogicalNode[] {
  return nodes.flatMap((node) => [node, ...flattenNodes(node.children ?? [])])
}
