/**
 * Desktop renderer for the file context-menu model: a right-click popup
 * anchored at the cursor. Mobile renders the same sections as an action
 * sheet with its own component.
 */

import { Divider, ListItemIcon, ListItemText, Menu, MenuItem, Typography } from '@mui/material'
import {
  ChevronRight,
  Copy,
  Download,
  ExternalLink,
  Eye,
  FolderInput,
  FolderOpen,
  FolderPlus,
  LayoutGrid,
  Link2,
  List,
  PanelRight,
  PencilLine,
  RefreshCw,
  Share2,
  SquareDashedMousePointer,
  Trash2,
  Upload,
} from 'lucide-react'
import { useState } from 'react'
import { useI18n } from '../../../i18n/provider'
import type {
  FileMenuAction,
  FileMenuIcon,
  FileMenuLabel,
  FileMenuSection,
  FileMenuSubmenu,
} from './types'

const ICONS: Record<FileMenuIcon, React.ComponentType<{ size?: number | string }>> = {
  open: FolderOpen,
  'open-new-tab': ExternalLink,
  'open-right': PanelRight,
  preview: Eye,
  copy: Copy,
  link: Link2,
  share: Share2,
  download: Download,
  rename: PencilLine,
  move: FolderInput,
  trash: Trash2,
  'new-folder': FolderPlus,
  upload: Upload,
  refresh: RefreshCw,
  'select-all': SquareDashedMousePointer,
  'view-list': List,
  'view-icon': LayoutGrid,
}

export interface FileContextMenuProps {
  position: { top: number; left: number } | null
  sections: FileMenuSection[]
  onInvoke: (action: FileMenuAction) => void
  onClose: () => void
}

export function FileContextMenu({ position, sections, onInvoke, onClose }: FileContextMenuProps) {
  const { t } = useI18n()
  const [submenu, setSubmenu] = useState<{ anchor: HTMLElement; menu: FileMenuSubmenu } | null>(
    null,
  )

  const resolve = (label: FileMenuLabel) => t(label.key, label.fallback, label.vars)

  const close = () => {
    setSubmenu(null)
    onClose()
  }

  const invoke = (item: FileMenuAction) => {
    close()
    onInvoke(item)
  }

  const renderAction = (item: FileMenuAction) => {
    const Icon = item.icon ? ICONS[item.icon] : null
    return (
      <MenuItem
        key={item.id}
        dense
        disabled={item.disabled}
        onClick={() => invoke(item)}
        sx={item.danger ? { color: 'var(--cp-danger)' } : undefined}
      >
        {Icon ? (
          <ListItemIcon sx={item.danger ? { color: 'var(--cp-danger)' } : undefined}>
            <Icon size={15} />
          </ListItemIcon>
        ) : null}
        <ListItemText primary={resolve(item.label)} primaryTypographyProps={{ fontSize: 13 }} />
        {item.shortcut ? (
          <Typography variant="caption" sx={{ ml: 2, color: 'var(--cp-muted)' }}>
            {item.shortcut}
          </Typography>
        ) : null}
      </MenuItem>
    )
  }

  const renderSubmenu = (item: FileMenuSubmenu) => {
    const Icon = item.icon ? ICONS[item.icon] : null
    return (
      <MenuItem
        key={item.id}
        dense
        onClick={(event) => setSubmenu({ anchor: event.currentTarget, menu: item })}
      >
        {Icon ? (
          <ListItemIcon>
            <Icon size={15} />
          </ListItemIcon>
        ) : null}
        <ListItemText primary={resolve(item.label)} primaryTypographyProps={{ fontSize: 13 }} />
        <ChevronRight size={14} style={{ marginLeft: 12, opacity: 0.6 }} />
      </MenuItem>
    )
  }

  return (
    <>
      <Menu
        open={Boolean(position)}
        onClose={close}
        anchorReference="anchorPosition"
        anchorPosition={position ?? undefined}
        slotProps={{ paper: { sx: { minWidth: 210, maxWidth: 320 } } }}
      >
        {sections.flatMap((section, index) => [
          index > 0 ? <Divider key={`divider-${index}`} sx={{ my: 0.5 }} /> : null,
          ...section.map((item) =>
            item.type === 'action' ? renderAction(item) : renderSubmenu(item),
          ),
        ])}
      </Menu>
      <Menu
        open={Boolean(submenu)}
        onClose={() => setSubmenu(null)}
        anchorEl={submenu?.anchor ?? null}
        anchorOrigin={{ vertical: 'top', horizontal: 'right' }}
        transformOrigin={{ vertical: 'top', horizontal: 'left' }}
        slotProps={{ paper: { sx: { minWidth: 180, maxWidth: 320 } } }}
      >
        {submenu?.menu.items.map((item) => renderAction(item))}
      </Menu>
    </>
  )
}
