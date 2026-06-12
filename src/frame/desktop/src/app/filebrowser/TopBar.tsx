import clsx from 'clsx'
import { Divider, IconButton, ListItemText, Menu, MenuItem, TextField, Tooltip } from '@mui/material'
import {
  ArrowLeft,
  ArrowRight,
  ArrowUp,
  ChevronDown,
  ChevronRight,
  Copy,
  FolderPlus,
  LayoutGrid,
  List,
  Plus,
  Search,
  Upload,
  X,
} from 'lucide-react'
import { useEffect, useRef, useState } from 'react'
import { useI18n } from '../../i18n/provider'
import type { BrowserTab, ViewMode } from './types'

interface TopBarProps {
  tabs: BrowserTab[]
  activeTabId: string
  onSelectTab: (id: string) => void
  onCloseTab: (id: string) => void
  onNewTab?: () => void
  closedTabs?: BrowserTab[]
  onRestoreClosedTab?: (tab: BrowserTab) => void
  /** Show the new-tab button and the closed-tabs menu (hidden in the right pane). */
  showTabControls?: boolean
  /** Allow closing the last remaining tab (the right pane disappears when empty). */
  allowCloseLast?: boolean
  /** When provided, tabs get a context menu with a "Send to Right" action. */
  onSendTabToRight?: (id: string) => void
  canSendToRight?: boolean
  currentPath: string
  onNavigate: (path: string) => void
  onBack: () => void
  onForward: () => void
  onUp: () => void
  canBack: boolean
  canForward: boolean
  canUp: boolean
  viewMode: ViewMode
  onViewModeChange: (mode: ViewMode) => void
  searchQuery: string
  onSearchChange: (query: string) => void
  onCopyPath: () => void
  onUpload?: () => void
  onNewFolder?: () => void
}

function PathCrumbs({ path, onNavigate }: { path: string; onNavigate: (p: string) => void }) {
  const segments = path.split('/').filter(Boolean)
  const crumbs: { label: string; path: string }[] = [{ label: 'root', path: '/' }]
  let running = ''
  for (const segment of segments) {
    running += `/${segment}`
    crumbs.push({ label: segment, path: running })
  }

  return (
    <div className="flex min-w-0 flex-1 items-center gap-1 overflow-x-auto text-sm text-[color:var(--cp-muted)]">
      {crumbs.map((crumb, idx) => (
        <div key={crumb.path} className="flex shrink-0 items-center gap-1">
          <button
            type="button"
            className={clsx(
              'truncate rounded-md px-1.5 py-0.5 hover:bg-[color:color-mix(in_srgb,var(--cp-accent-soft)_14%,transparent)]',
              idx === crumbs.length - 1 && 'font-semibold text-[color:var(--cp-text)]',
            )}
            onClick={(event) => {
              event.stopPropagation()
              onNavigate(crumb.path)
            }}
          >
            {crumb.label}
          </button>
          {idx < crumbs.length - 1 ? <ChevronRight size={12} className="opacity-60" /> : null}
        </div>
      ))}
    </div>
  )
}

export function TopBar({
  tabs,
  activeTabId,
  onSelectTab,
  onCloseTab,
  onNewTab,
  closedTabs = [],
  onRestoreClosedTab,
  showTabControls = true,
  allowCloseLast = false,
  onSendTabToRight,
  canSendToRight = false,
  currentPath,
  onNavigate,
  onBack,
  onForward,
  onUp,
  canBack,
  canForward,
  canUp,
  viewMode,
  onViewModeChange,
  searchQuery,
  onSearchChange,
  onCopyPath,
  onUpload,
  onNewFolder,
}: TopBarProps) {
  const { t } = useI18n()
  const [searchOpen, setSearchOpen] = useState(false)
  const [tabMenuAnchor, setTabMenuAnchor] = useState<HTMLElement | null>(null)
  const [tabContextMenu, setTabContextMenu] = useState<
    { tabId: string; top: number; left: number } | null
  >(null)
  const [pathEditing, setPathEditing] = useState(false)
  const [pathDraft, setPathDraft] = useState(currentPath)
  const searchInputRef = useRef<HTMLInputElement | null>(null)
  const pathInputRef = useRef<HTMLInputElement | null>(null)
  const searchVisible = searchOpen || !!searchQuery
  const canCloseTab = tabs.length > 1 || allowCloseLast

  useEffect(() => {
    if (searchVisible) searchInputRef.current?.focus()
  }, [searchVisible])

  useEffect(() => {
    if (pathEditing) {
      pathInputRef.current?.focus()
      pathInputRef.current?.select()
    }
  }, [pathEditing])

  const startPathEdit = () => {
    if (searchVisible || pathEditing) return
    setPathDraft(currentPath)
    setPathEditing(true)
  }

  const commitPathEdit = () => {
    const next = pathDraft.trim()
    setPathEditing(false)
    if (next && next !== currentPath) onNavigate(next)
  }

  const closeSearch = () => {
    onSearchChange('')
    setSearchOpen(false)
  }

  const closeTabMenu = () => {
    setTabMenuAnchor(null)
  }

  const handleNewTab = () => {
    closeTabMenu()
    onNewTab?.()
  }

  const closeTabContextMenu = () => {
    setTabContextMenu(null)
  }

  return (
    <div className="flex flex-col gap-2 border-b border-[color:color-mix(in_srgb,var(--cp-border)_60%,transparent)] bg-[color:color-mix(in_srgb,var(--cp-surface)_88%,transparent)] px-3 py-2 sm:px-4">
      {/* Tabs row */}
      <div className="flex items-center gap-1 overflow-x-auto">
        {tabs.map((tab) => {
          const active = tab.id === activeTabId
          return (
            <div
              key={tab.id}
              className={clsx(
                'group flex shrink-0 items-center gap-2 rounded-t-[12px] border-b-2 px-3 py-1.5 text-sm',
                active
                  ? 'border-[color:var(--cp-accent)] bg-[color:color-mix(in_srgb,var(--cp-accent-soft)_18%,var(--cp-surface))] text-[color:var(--cp-text)]'
                  : 'border-transparent text-[color:var(--cp-muted)] hover:bg-[color:color-mix(in_srgb,var(--cp-accent-soft)_10%,transparent)]',
              )}
              onContextMenu={(event) => {
                if (!onSendTabToRight) return
                event.preventDefault()
                setTabContextMenu({ tabId: tab.id, top: event.clientY, left: event.clientX })
              }}
            >
              <button
                type="button"
                onClick={() => onSelectTab(tab.id)}
                className="max-w-[180px] truncate font-medium"
              >
                {tab.title}
              </button>
              {canCloseTab ? (
                <button
                  type="button"
                  onClick={() => onCloseTab(tab.id)}
                  className="opacity-0 transition group-hover:opacity-100"
                  aria-label={t('common.close', 'Close')}
                >
                  <X size={12} />
                </button>
              ) : null}
            </div>
          )
        })}
        {showTabControls ? (
        <>
        <Tooltip title={t('filebrowser.topbar.newTab', 'New tab')}>
          <button
            type="button"
            onClick={onNewTab}
            className="flex h-7 shrink-0 items-center gap-0.5 rounded-md px-1.5 text-[color:var(--cp-muted)] transition hover:bg-[color:color-mix(in_srgb,var(--cp-accent-soft)_12%,transparent)] hover:text-[color:var(--cp-text)] focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[color:var(--cp-accent)]"
            aria-label={t('filebrowser.topbar.newTab', 'New tab')}
          >
            <Plus size={14} />
          </button>
        </Tooltip>
        <Tooltip title={t('filebrowser.topbar.closedTabs', 'Recently closed tabs')}>
          <button
            type="button"
            onClick={(event) => setTabMenuAnchor(event.currentTarget)}
            className={clsx(
              'flex h-7 shrink-0 items-center rounded-md px-1 text-[color:var(--cp-muted)] transition hover:bg-[color:color-mix(in_srgb,var(--cp-accent-soft)_12%,transparent)] hover:text-[color:var(--cp-text)] focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[color:var(--cp-accent)]',
              tabMenuAnchor && 'bg-[color:color-mix(in_srgb,var(--cp-accent-soft)_12%,transparent)] text-[color:var(--cp-text)]',
            )}
            aria-label={t('filebrowser.topbar.closedTabs', 'Recently closed tabs')}
            aria-haspopup="menu"
            aria-expanded={Boolean(tabMenuAnchor)}
          >
            <ChevronDown size={14} />
          </button>
        </Tooltip>
        <Menu
          anchorEl={tabMenuAnchor}
          open={Boolean(tabMenuAnchor)}
          onClose={closeTabMenu}
          slotProps={{ paper: { sx: { minWidth: 220, maxWidth: 320 } } }}
        >
          <MenuItem onClick={handleNewTab}>
            <ListItemText primary={t('filebrowser.topbar.newTab', 'New tab')} />
          </MenuItem>
          {onSendTabToRight ? (
            <MenuItem
              disabled={!canSendToRight}
              onClick={() => {
                closeTabMenu()
                onSendTabToRight(activeTabId)
              }}
            >
              <ListItemText primary={t('filebrowser.topbar.sendToRight', 'Send to Right')} />
            </MenuItem>
          ) : null}
          <Divider />
          {closedTabs.length > 0 ? (
            closedTabs.map((tab) => (
              <MenuItem
                key={tab.id}
                onClick={() => {
                  closeTabMenu()
                  onRestoreClosedTab?.(tab)
                }}
              >
                <ListItemText
                  primary={tab.title}
                  secondary={tab.path}
                  primaryTypographyProps={{ noWrap: true, fontSize: 13 }}
                  secondaryTypographyProps={{ noWrap: true, fontSize: 12 }}
                />
              </MenuItem>
            ))
          ) : (
            <MenuItem disabled>
              <ListItemText primary={t('filebrowser.topbar.noClosedTabs', 'No recently closed tabs')} />
            </MenuItem>
          )}
        </Menu>
        </>
        ) : null}
        {onSendTabToRight ? (
          <Menu
            open={Boolean(tabContextMenu)}
            onClose={closeTabContextMenu}
            anchorReference="anchorPosition"
            anchorPosition={
              tabContextMenu
                ? { top: tabContextMenu.top, left: tabContextMenu.left }
                : undefined
            }
            slotProps={{ paper: { sx: { minWidth: 180 } } }}
          >
            <MenuItem
              disabled={!canSendToRight}
              onClick={() => {
                const tabId = tabContextMenu?.tabId
                closeTabContextMenu()
                if (tabId) onSendTabToRight(tabId)
              }}
            >
              <ListItemText primary={t('filebrowser.topbar.sendToRight', 'Send to Right')} />
            </MenuItem>
            <MenuItem
              disabled={!canCloseTab}
              onClick={() => {
                const tabId = tabContextMenu?.tabId
                closeTabContextMenu()
                if (tabId) onCloseTab(tabId)
              }}
            >
              <ListItemText primary={t('filebrowser.topbar.closeTab', 'Close tab')} />
            </MenuItem>
          </Menu>
        ) : null}
      </div>

      {/* Address row: history navigation + path / search input */}
      <div className="flex items-center gap-1.5">
        <div className="flex h-8 shrink-0 items-stretch overflow-hidden rounded-full border border-[color:color-mix(in_srgb,var(--cp-border)_60%,transparent)] bg-[color:color-mix(in_srgb,var(--cp-surface-2)_80%,transparent)]">
          <IconButton
            size="small"
            disabled={!canBack}
            onClick={onBack}
            aria-label="back"
            className="!h-auto !w-9 !rounded-none !p-0"
          >
            <ArrowLeft size={16} />
          </IconButton>
          <IconButton
            size="small"
            disabled={!canForward}
            onClick={onForward}
            aria-label="forward"
            className="!h-auto !w-9 !rounded-none !p-0"
          >
            <ArrowRight size={16} />
          </IconButton>
        </div>
        <IconButton size="small" disabled={!canUp} onClick={onUp} aria-label="up">
          <ArrowUp size={16} />
        </IconButton>

        <div
          className={clsx(
            'relative ml-1 flex h-8 min-w-0 flex-1 items-center gap-1 rounded-full border border-[color:color-mix(in_srgb,var(--cp-border)_60%,transparent)] bg-[color:color-mix(in_srgb,var(--cp-surface-2)_88%,transparent)] pl-2 pr-1.5',
            !searchVisible && !pathEditing && 'cursor-text',
          )}
          onClick={startPathEdit}
        >
          {searchVisible ? (
            <>
              <Search size={14} className="ml-1 shrink-0 text-[color:var(--cp-muted)]" />
              <TextField
                inputRef={searchInputRef}
                fullWidth
                variant="standard"
                placeholder={t(
                  'filebrowser.topbar.searchPlaceholder',
                  'Search across files, folders, AI summaries…',
                )}
                value={searchQuery}
                onChange={(event) => onSearchChange(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === 'Escape') closeSearch()
                }}
                InputProps={{ disableUnderline: true }}
                sx={{
                  '& .MuiInputBase-input': {
                    fontSize: 13,
                    padding: '2px 0',
                    color: 'var(--cp-text)',
                  },
                }}
              />
              <Tooltip title={t('common.close', 'Close')}>
                <IconButton size="small" onClick={closeSearch} aria-label="close-search">
                  <X size={13} />
                </IconButton>
              </Tooltip>
            </>
          ) : pathEditing ? (
            <input
              ref={pathInputRef}
              value={pathDraft}
              onChange={(event) => setPathDraft(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === 'Enter') commitPathEdit()
                if (event.key === 'Escape') setPathEditing(false)
              }}
              onBlur={() => setPathEditing(false)}
              spellCheck={false}
              aria-label="path"
              className="min-w-0 flex-1 bg-transparent px-1 font-mono text-[13px] outline-none"
              style={{ color: 'var(--cp-text)' }}
            />
          ) : (
            <>
              <PathCrumbs path={currentPath} onNavigate={onNavigate} />
              <Tooltip title={t('filebrowser.topbar.copyPath', 'Copy path')}>
                <button
                  type="button"
                  aria-label={t('filebrowser.topbar.copyPath', 'Copy path')}
                  onClick={(event) => {
                    event.stopPropagation()
                    onCopyPath()
                  }}
                  className="ml-auto flex shrink-0 items-center self-stretch pl-2 pr-1 text-[color:var(--cp-muted)] transition hover:text-[color:var(--cp-text)]"
                >
                  <Copy size={13} />
                </button>
              </Tooltip>
            </>
          )}
        </div>
      </div>

      {/* Toolbar row: file operations + view mode + search toggle */}
      <div className="flex items-center gap-1.5">
        {onUpload ? (
          <button
            type="button"
            onClick={onUpload}
            className="flex h-7 shrink-0 items-center gap-1.5 rounded-md px-2 text-xs text-[color:var(--cp-muted)] transition hover:bg-[color:color-mix(in_srgb,var(--cp-accent-soft)_12%,transparent)] hover:text-[color:var(--cp-text)]"
          >
            <Upload size={14} />
            {t('filebrowser.actions.upload', 'Upload')}
          </button>
        ) : null}
        {onNewFolder ? (
          <button
            type="button"
            onClick={onNewFolder}
            className="flex h-7 shrink-0 items-center gap-1.5 rounded-md px-2 text-xs text-[color:var(--cp-muted)] transition hover:bg-[color:color-mix(in_srgb,var(--cp-accent-soft)_12%,transparent)] hover:text-[color:var(--cp-text)]"
          >
            <FolderPlus size={14} />
            {t('filebrowser.actions.newFolder', 'New folder')}
          </button>
        ) : null}

        <div className="flex-1" />

        <div className="flex items-center rounded-full border border-[color:color-mix(in_srgb,var(--cp-border)_60%,transparent)] bg-[color:color-mix(in_srgb,var(--cp-surface-2)_80%,transparent)]">
          <IconButton
            size="small"
            onClick={() => onViewModeChange('list')}
            className={clsx(
              viewMode === 'list' &&
                '!bg-[color:color-mix(in_srgb,var(--cp-accent-soft)_28%,var(--cp-surface))] !text-[color:var(--cp-text)]',
            )}
            aria-label={t('filebrowser.view.list', 'List view')}
          >
            <List size={14} />
          </IconButton>
          <IconButton
            size="small"
            onClick={() => onViewModeChange('icon')}
            className={clsx(
              viewMode === 'icon' &&
                '!bg-[color:color-mix(in_srgb,var(--cp-accent-soft)_28%,var(--cp-surface))] !text-[color:var(--cp-text)]',
            )}
            aria-label={t('filebrowser.view.icon', 'Icon view')}
          >
            <LayoutGrid size={14} />
          </IconButton>
        </div>

        <Tooltip title={t('filebrowser.topbar.search', 'Search')}>
          <IconButton
            size="small"
            onClick={() => setSearchOpen((v) => !v)}
            className={clsx(
              searchVisible &&
                '!bg-[color:color-mix(in_srgb,var(--cp-accent-soft)_28%,var(--cp-surface))] !text-[color:var(--cp-text)]',
            )}
            aria-label={t('filebrowser.topbar.search', 'Search')}
          >
            <Search size={14} />
          </IconButton>
        </Tooltip>
      </div>
    </div>
  )
}
