/* ── Binding management section ── */

import { useState } from 'react'
import { Link2, Plus, AlertCircle, Check, Clock } from 'lucide-react'
import { Button, Dialog, DialogActions, DialogContent, DialogTitle, IconButton } from '@mui/material'
import type { MessageTunnelBinding } from '../../mock/types'
import { useUsersAgentsStore } from '../../hooks/use-users-agents-store'

interface BindingsSectionProps {
  entityId?: string
  bindings: MessageTunnelBinding[]
}

const statusIcon = {
  active: Check,
  pending: Clock,
  error: AlertCircle,
}

const statusColor = {
  active: 'var(--cp-success)',
  pending: 'var(--cp-warning)',
  error: 'var(--cp-danger)',
}

export function BindingsSection({ entityId, bindings }: BindingsSectionProps) {
  const [open, setOpen] = useState(false)
  const store = useUsersAgentsStore()

  const handleAdd = (platform: string) => {
    if (!entityId) return
    store.addBinding(entityId, {
      id: `binding-${platform}-${Date.now()}`,
      platform,
      accountId: platform === 'telegram' ? '@new_channel' : 'new@example.com',
      displayId: platform === 'telegram' ? '@new_channel' : 'new@example.com',
      status: 'pending',
    })
    setOpen(false)
  }

  return (
    <div
      className="rounded-[22px] px-5 py-4"
      style={{
        background: 'color-mix(in srgb, var(--cp-surface-2) 40%, var(--cp-surface))',
        border: '1px solid color-mix(in srgb, var(--cp-border) 50%, transparent)',
      }}
    >
      <div className="flex items-center justify-between mb-3">
        <div className="flex items-center gap-2">
          <Link2 size={16} style={{ color: 'var(--cp-accent)' }} />
          <h3
            className="font-display text-sm font-semibold"
            style={{ color: 'var(--cp-text)' }}
          >
            Message Tunnel Bindings
          </h3>
        </div>
        <Button
          size="small"
          startIcon={<Plus size={14} />}
          variant="text"
          onClick={() => setOpen(true)}
        >
          Add
        </Button>
      </div>

      {bindings.length === 0 ? (
        <div className="text-sm py-3" style={{ color: 'var(--cp-muted)' }}>
          No bindings configured. Add a binding to connect external messaging channels.
        </div>
      ) : (
        <div className="space-y-2">
          {bindings.map((b) => {
            const StatusIcon = statusIcon[b.status]
            const color = statusColor[b.status]
            return (
              <div
                key={b.id}
                className="flex items-center gap-3 px-3 py-2.5 rounded-[14px]"
                style={{
                  background: 'color-mix(in srgb, var(--cp-surface) 80%, transparent)',
                  border: '1px solid color-mix(in srgb, var(--cp-border) 40%, transparent)',
                }}
              >
                <div
                  className="shrink-0 flex items-center justify-center rounded-full"
                  style={{
                    width: 28,
                    height: 28,
                    background: `color-mix(in srgb, ${color} 14%, var(--cp-surface))`,
                    color,
                  }}
                >
                  <StatusIcon size={14} />
                </div>

                <div className="flex-1 min-w-0">
                  <div className="text-sm font-medium capitalize" style={{ color: 'var(--cp-text)' }}>
                    {b.platform}
                  </div>
                  <div className="text-[11px] truncate" style={{ color: 'var(--cp-muted)' }}>
                    {b.displayId}
                  </div>
                </div>

                {b.lastSyncAt && (
                  <span className="text-[10px] shrink-0" style={{ color: 'var(--cp-muted)' }}>
                    {new Date(b.lastSyncAt).toLocaleDateString()}
                  </span>
                )}

                <IconButton size="small" aria-label="Remove binding">
                  <Link2 size={12} />
                </IconButton>
              </div>
            )
          })}
        </div>
      )}

      <Dialog open={open} onClose={() => setOpen(false)} fullWidth maxWidth="xs">
        <DialogTitle>Bind message tunnel</DialogTitle>
        <DialogContent>
          <div className="space-y-2 pt-1">
            {[
              ['telegram', 'Sync Telegram messages and contacts into MessageHub.'],
              ['email', 'Make this entity reachable through an email identity.'],
              ['did', 'Use a DID-native channel for internal BuckyOS messaging.'],
            ].map(([platform, body]) => (
              <button
                key={platform}
                type="button"
                className="w-full rounded-[14px] px-3 py-2 text-left"
                style={{
                  background: 'color-mix(in srgb, var(--cp-surface) 80%, transparent)',
                  border: '1px solid color-mix(in srgb, var(--cp-border) 40%, transparent)',
                }}
                onClick={() => handleAdd(platform)}
              >
                <div className="text-sm font-medium capitalize" style={{ color: 'var(--cp-text)' }}>
                  {platform}
                </div>
                <div className="mt-0.5 text-[12px] leading-5" style={{ color: 'var(--cp-muted)' }}>
                  {body}
                </div>
              </button>
            ))}
          </div>
        </DialogContent>
        <DialogActions>
          <Button onClick={() => setOpen(false)}>Cancel</Button>
        </DialogActions>
      </Dialog>
    </div>
  )
}
