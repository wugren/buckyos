import { IconButton, useMediaQuery } from '@mui/material'
import clsx from 'clsx'
import { useCallback, useMemo, useState } from 'react'
import {
  Camera,
  ChevronRight,
  FolderPlus,
  Image as ImageIcon,
  LayoutGrid,
  List,
  Menu as MenuIcon,
  PanelRightClose,
  Plus,
  RefreshCw,
  Search,
  Upload as UploadIcon,
  X,
} from 'lucide-react'
import { useI18n } from '../../i18n/provider'
import {
  useMobileBackHandler,
  useMobileTitleOverride,
} from '../../desktop/windows/MobileNavContext'
import { MainContent } from './MainContent'
import { PreviewPanel } from './PreviewPanel'
import { SearchResultsPanel } from './SearchResultsPanel'
import { Sidebar } from './Sidebar'
import { StatusBar } from './StatusBar'
import { TopBar } from './TopBar'
import {
  defaultTabs,
  fileBrowserSnapshot,
  searchFiles,
} from './mock/data'
import type { BrowserTab, FileEntry, Topic, ViewMode } from './types'

interface HistoryState {
  back: string[]
  forward: string[]
}

const TOPIC_SCHEME = 'topic://'

function topicIdFromPath(path: string): string | null {
  return path.startsWith(TOPIC_SCHEME) ? path.slice(TOPIC_SCHEME.length) : null
}

function topicForPath(path: string): Topic | null {
  const topicId = topicIdFromPath(path)
  if (!topicId) return null
  return fileBrowserSnapshot.topics.find((t) => t.id === topicId) ?? null
}

function entriesForPath(path: string): FileEntry[] {
  const topic = topicForPath(path)
  if (topic) {
    const ids = new Set(topic.groups.flatMap((group) => group.fileIds))
    return Array.from(ids)
      .map((id) => fileBrowserSnapshot.entriesById[id])
      .filter((entry): entry is FileEntry => !!entry)
  }
  return fileBrowserSnapshot.entriesByPath[path] ?? []
}

interface DetachedTab {
  tab: BrowserTab
  history: HistoryState
}

/** Self-contained state for one browser pane (tabs, history, selection, search, view). */
function useBrowserPane(initialTabs: BrowserTab[]) {
  const [tabs, setTabs] = useState<BrowserTab[]>(initialTabs)
  const [activeTabId, setActiveTabId] = useState(initialTabs[0]?.id ?? '')
  const [history, setHistory] = useState<Record<string, HistoryState>>(() =>
    Object.fromEntries(initialTabs.map((tab) => [tab.id, { back: [], forward: [] }])),
  )
  const [viewMode, setViewMode] = useState<ViewMode>('list')
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [searchQuery, setSearchQuery] = useState('')

  const activeTab = tabs.find((tab) => tab.id === activeTabId) ?? tabs[0] ?? null
  const currentPath = activeTab?.path ?? '/home'
  const activeHistory = history[activeTabId] ?? { back: [], forward: [] }

  const updateTab = useCallback(
    (tabId: string, next: Partial<BrowserTab>) => {
      setTabs((prev) => prev.map((tab) => (tab.id === tabId ? { ...tab, ...next } : tab)))
    },
    [],
  )

  const pushHistory = useCallback((tabId: string, path: string) => {
    setHistory((prev) => {
      const curr = prev[tabId] ?? { back: [], forward: [] }
      return {
        ...prev,
        [tabId]: { back: [...curr.back, path], forward: [] },
      }
    })
  }, [])

  const navigate = useCallback(
    (path: string, options?: { suppressHistory?: boolean }) => {
      if (!activeTab) return
      if (path === currentPath) return
      if (!options?.suppressHistory) pushHistory(activeTab.id, currentPath)
      const title = path.split('/').filter(Boolean).pop() ?? 'root'
      updateTab(activeTab.id, { path, title })
      setSelectedId(null)
    },
    [activeTab, currentPath, pushHistory, updateTab],
  )

  const back = () => {
    if (!activeTab) return
    const hist = history[activeTab.id]
    if (!hist || hist.back.length === 0) return
    const previous = hist.back[hist.back.length - 1]
    setHistory((prev) => ({
      ...prev,
      [activeTab.id]: {
        back: hist.back.slice(0, -1),
        forward: [currentPath, ...hist.forward],
      },
    }))
    const title = previous.split('/').filter(Boolean).pop() ?? 'root'
    updateTab(activeTab.id, { path: previous, title })
    setSelectedId(null)
  }

  const forward = () => {
    if (!activeTab) return
    const hist = history[activeTab.id]
    if (!hist || hist.forward.length === 0) return
    const next = hist.forward[0]
    setHistory((prev) => ({
      ...prev,
      [activeTab.id]: {
        back: [...hist.back, currentPath],
        forward: hist.forward.slice(1),
      },
    }))
    const title = next.split('/').filter(Boolean).pop() ?? 'root'
    updateTab(activeTab.id, { path: next, title })
    setSelectedId(null)
  }

  const goUp = () => {
    const parent = currentPath.split('/').slice(0, -1).join('/') || '/'
    if (parent !== currentPath) navigate(parent)
  }

  /** Append a tab (optionally with its history) and make it active. */
  const adoptTab = useCallback((tab: BrowserTab, tabHistory?: HistoryState) => {
    setTabs((prev) => [...prev, tab])
    setHistory((prev) => ({ ...prev, [tab.id]: tabHistory ?? { back: [], forward: [] } }))
    setActiveTabId(tab.id)
    setSelectedId(null)
    setSearchQuery('')
  }, [])

  /** Remove a tab and hand back its data so it can be moved or remembered. */
  const detachTab = useCallback(
    (id: string): DetachedTab | null => {
      const tab = tabs.find((item) => item.id === id)
      if (!tab) return null
      const tabHistory = history[id] ?? { back: [], forward: [] }
      setHistory((prev) => {
        const next = { ...prev }
        delete next[id]
        return next
      })
      setTabs((prev) => {
        const closingIndex = prev.findIndex((item) => item.id === id)
        const next = prev.filter((item) => item.id !== id)
        if (id === activeTabId && next.length) {
          setActiveTabId(next[Math.min(closingIndex, next.length - 1)].id)
        }
        return next
      })
      if (id === activeTabId) {
        setSelectedId(null)
        setSearchQuery('')
      }
      return { tab, history: tabHistory }
    },
    [tabs, history, activeTabId],
  )

  return {
    tabs,
    activeTabId,
    setActiveTabId,
    activeTab,
    currentPath,
    activeHistory,
    viewMode,
    setViewMode,
    selectedId,
    setSelectedId,
    searchQuery,
    setSearchQuery,
    navigate,
    back,
    forward,
    goUp,
    updateTab,
    adoptTab,
    detachTab,
  }
}

type BrowserPane = ReturnType<typeof useBrowserPane>

export function FileBrowserView() {
  const { t } = useI18n()
  const isMobile = useMediaQuery('(max-width: 900px)')
  // Tailwind xl — the preview sidebar only exists at this width, so the
  // expand control in the status bar should follow the same gate.
  const isXl = useMediaQuery('(min-width: 1280px)')

  const left = useBrowserPane(defaultTabs)
  const right = useBrowserPane([])
  const [closedTabs, setClosedTabs] = useState<BrowserTab[]>([])
  const [focusedSide, setFocusedSide] = useState<'left' | 'right'>('left')

  const [advancedMode, setAdvancedMode] = useState(false)
  const [toast, setToast] = useState<string | null>(null)
  const [previewCollapsed, setPreviewCollapsed] = useState(false)

  // Mobile-only panel states
  const [mobileSidebarOpen, setMobileSidebarOpen] = useState(false)
  const [mobilePreviewOpen, setMobilePreviewOpen] = useState(false)
  const [mobileUploadOpen, setMobileUploadOpen] = useState(false)

  // The split layout is desktop-only; the right pane exists while it holds tabs.
  const splitActive = !isMobile && right.tabs.length > 0
  const focusedIsRight = splitActive && focusedSide === 'right'
  const focusedPane = focusedIsRight ? right : left

  const rememberClosedTab = (tab: BrowserTab) => {
    setClosedTabs((prev) => [tab, ...prev].slice(0, 10))
  }

  const handleNewTab = () => {
    left.adoptTab({ id: `tab-${Date.now()}`, title: 'Home', path: '/home' })
    setFocusedSide('left')
  }

  const handleCloseLeftTab = (id: string) => {
    if (left.tabs.length <= 1) return
    const detached = left.detachTab(id)
    if (detached) rememberClosedTab(detached.tab)
  }

  const handleCloseRightTab = (id: string) => {
    const detached = right.detachTab(id)
    if (detached) rememberClosedTab(detached.tab)
  }

  const handleSendToRight = (id: string) => {
    if (left.tabs.length <= 1) return
    const detached = left.detachTab(id)
    if (!detached) return
    right.adoptTab(detached.tab, detached.history)
    setFocusedSide('right')
  }

  const handleRestoreClosedTab = (tab: BrowserTab) => {
    setClosedTabs((prev) => prev.filter((item) => item.id !== tab.id))
    left.adoptTab({ ...tab, id: `tab-${Date.now()}` })
    setFocusedSide('left')
  }

  const showToast = (message: string) => {
    setToast(message)
    window.setTimeout(() => setToast(null), 2000)
  }

  const copyText = async (text: string) => {
    try {
      await navigator.clipboard.writeText(text)
      showToast(t('filebrowser.toast.copied', 'Copied to clipboard'))
    } catch {
      showToast(t('filebrowser.toast.copyFailed', 'Copy failed'))
    }
  }

  const leftEntries = useMemo(() => entriesForPath(left.currentPath), [left.currentPath])
  const rightEntries = useMemo(() => entriesForPath(right.currentPath), [right.currentPath])
  const leftTopic = topicForPath(left.currentPath)
  const rightTopic = topicForPath(right.currentPath)

  const leftSelectedEntry = left.selectedId
    ? fileBrowserSnapshot.entriesById[left.selectedId] ?? null
    : null
  const rightSelectedEntry = right.selectedId
    ? fileBrowserSnapshot.entriesById[right.selectedId] ?? null
    : null

  const leftSearchHits = useMemo(
    () => (left.searchQuery.trim() ? searchFiles(left.searchQuery) : []),
    [left.searchQuery],
  )
  const rightSearchHits = useMemo(
    () => (right.searchQuery.trim() ? searchFiles(right.searchQuery) : []),
    [right.searchQuery],
  )

  const selectTopicInPane = (pane: BrowserPane, topicId: string) => {
    if (!pane.activeTab) return
    const topic = fileBrowserSnapshot.topics.find((item) => item.id === topicId)
    pane.updateTab(pane.activeTab.id, {
      title: topic?.title ?? 'Topic',
      path: `${TOPIC_SCHEME}${topicId}`,
    })
    pane.setSelectedId(null)
    pane.setSearchQuery('')
  }

  const handleOpenEntry = (entry: FileEntry) => {
    left.setSelectedId(entry.id)
    if (isMobile) setMobilePreviewOpen(true)
  }

  const handleOpenFolder = (path: string) => {
    left.navigate(path)
    setMobileSidebarOpen(false)
  }

  const leftSearchActive = !!left.searchQuery.trim()
  const rightSearchActive = !!right.searchQuery.trim()

  const focusedSelectedEntry = focusedIsRight ? rightSelectedEntry : leftSelectedEntry

  const mobileTitleText = leftSelectedEntry?.name ?? left.activeTab?.title ?? 'root'
  const mobileSubtitleText =
    leftSelectedEntry?.summary ??
    leftTopic?.description ??
    (left.currentPath === '/'
      ? t('filebrowser.mobile.rootHint', 'Root directory')
      : left.currentPath)

  const mobileTitleOverride = useMemo(
    () => (isMobile ? { title: mobileTitleText, subtitle: mobileSubtitleText } : null),
    [isMobile, mobileTitleText, mobileSubtitleText],
  )
  useMobileTitleOverride(mobileTitleOverride)

  const canMobileBack = isMobile && left.activeHistory.back.length > 0
  useMobileBackHandler(canMobileBack ? left.back : null)

  // ─── Mobile layout ───
  if (isMobile) {
    const segments = left.currentPath.split('/').filter(Boolean)
    const crumbs: { label: string; path: string }[] = [{ label: 'root', path: '/' }]
    {
      let running = ''
      for (const segment of segments) {
        running += `/${segment}`
        crumbs.push({ label: segment, path: running })
      }
    }

    return (
      <div className="relative flex h-full w-full flex-col overflow-hidden" style={{ background: 'var(--cp-bg)' }}>
        {/* Operations bar: drawer toggle + search + view mode */}
        <div className="flex items-center gap-2 px-3 pt-2 pb-1">
          <IconButton
            size="small"
            onClick={() => setMobileSidebarOpen((v) => !v)}
            aria-label={t('filebrowser.mobile.places', 'Places')}
            className={clsx(
              mobileSidebarOpen &&
                '!bg-[color:color-mix(in_srgb,var(--cp-accent-soft)_28%,var(--cp-surface))] !text-[color:var(--cp-text)]',
            )}
          >
            <MenuIcon size={16} />
          </IconButton>
          <div className="relative flex min-w-0 flex-1 items-center gap-1 rounded-full border border-[color:color-mix(in_srgb,var(--cp-border)_60%,transparent)] bg-[color:color-mix(in_srgb,var(--cp-surface-2)_88%,transparent)] px-2 py-1">
            <Search size={14} className="ml-1 shrink-0 text-[color:var(--cp-muted)]" />
            <input
              type="text"
              value={left.searchQuery}
              onChange={(event) => left.setSearchQuery(event.target.value)}
              placeholder={t(
                'filebrowser.topbar.searchPlaceholder',
                'Search across files, folders, AI summaries…',
              )}
              className="min-w-0 flex-1 bg-transparent text-xs outline-none placeholder:text-[color:var(--cp-muted)]"
              style={{ color: 'var(--cp-text)' }}
            />
            {left.searchQuery ? (
              <IconButton
                size="small"
                onClick={() => left.setSearchQuery('')}
                aria-label={t('common.close', 'Close')}
              >
                <X size={12} />
              </IconButton>
            ) : null}
          </div>
          <div className="flex items-center rounded-full border border-[color:color-mix(in_srgb,var(--cp-border)_60%,transparent)] bg-[color:color-mix(in_srgb,var(--cp-surface-2)_80%,transparent)]">
            <IconButton
              size="small"
              onClick={() => left.setViewMode('list')}
              className={clsx(
                left.viewMode === 'list' &&
                  '!bg-[color:color-mix(in_srgb,var(--cp-accent-soft)_28%,var(--cp-surface))] !text-[color:var(--cp-text)]',
              )}
              aria-label={t('filebrowser.view.list', 'List view')}
            >
              <List size={14} />
            </IconButton>
            <IconButton
              size="small"
              onClick={() => left.setViewMode('icon')}
              className={clsx(
                left.viewMode === 'icon' &&
                  '!bg-[color:color-mix(in_srgb,var(--cp-accent-soft)_28%,var(--cp-surface))] !text-[color:var(--cp-text)]',
              )}
              aria-label={t('filebrowser.view.icon', 'Icon view')}
            >
              <LayoutGrid size={14} />
            </IconButton>
          </div>
        </div>

        {/* Address bar: path crumbs + refresh on the right */}
        <div className="flex items-center gap-2 px-3 pb-2 pt-1">
          <div className="flex min-w-0 flex-1 items-center gap-1 overflow-x-auto text-[13px] text-[color:var(--cp-muted)]">
            {crumbs.map((crumb, idx) => (
              <div key={crumb.path} className="flex shrink-0 items-center gap-1">
                <button
                  type="button"
                  className={clsx(
                    'truncate rounded-md px-1.5 py-1',
                    idx === crumbs.length - 1 && 'font-semibold text-[color:var(--cp-text)]',
                  )}
                  onClick={() => left.navigate(crumb.path)}
                >
                  {crumb.label}
                </button>
                {idx < crumbs.length - 1 ? (
                  <ChevronRight size={13} className="opacity-60" />
                ) : null}
              </div>
            ))}
          </div>
          <button
            type="button"
            onClick={() => left.navigate(left.currentPath)}
            aria-label={t('filebrowser.topbar.refresh', 'Refresh')}
            className="shrink-0 p-1 text-[color:var(--cp-muted)] hover:text-[color:var(--cp-text)]"
          >
            <RefreshCw size={16} />
          </button>
        </div>

        <div className="flex-1 overflow-hidden">
          {leftSearchActive ? (
            <SearchResultsPanel
              hits={leftSearchHits}
              query={left.searchQuery}
              onSelect={handleOpenEntry}
            />
          ) : (
            <MainContent
              entries={leftEntries}
              viewMode={left.viewMode}
              selectedId={left.selectedId}
              onSelect={handleOpenEntry}
              onOpenFolder={handleOpenFolder}
              currentPath={left.currentPath}
              topicContext={leftTopic}
              isMobile
            />
          )}
        </div>

        {/* Floating action button — upload */}
        <button
          type="button"
          onClick={() => setMobileUploadOpen(true)}
          aria-label={t('filebrowser.actions.upload', 'Upload')}
          className="absolute bottom-5 right-5 z-20 flex h-14 w-14 items-center justify-center rounded-full text-white shadow-[0_10px_28px_rgba(0,0,0,0.22)] transition active:scale-95"
          style={{ background: 'var(--cp-accent)' }}
        >
          <Plus size={26} />
        </button>

        {/* Upload action sheet */}
        {mobileUploadOpen ? (
          <div className="absolute inset-0 z-40 flex items-end">
            <div
              className="absolute inset-0 bg-black/50"
              onClick={() => setMobileUploadOpen(false)}
            />
            <div
              className="relative w-full rounded-t-[28px] border-t border-[color:var(--cp-border)] pb-5 pt-2"
              style={{ background: 'var(--cp-surface)' }}
            >
              <div className="mx-auto mb-2 h-1 w-10 rounded-full bg-[color:var(--cp-border)]" />
              <div className="px-5 pb-1 text-[13px] font-semibold text-[color:var(--cp-text)]">
                {t('filebrowser.upload.title', 'Add to this folder')}
              </div>
              <div className="grid grid-cols-4 gap-1 px-3 py-3">
                {[
                  {
                    key: 'files',
                    icon: <UploadIcon size={22} />,
                    label: t('filebrowser.upload.files', 'Files'),
                  },
                  {
                    key: 'photos',
                    icon: <ImageIcon size={22} />,
                    label: t('filebrowser.upload.photos', 'Photos'),
                  },
                  {
                    key: 'camera',
                    icon: <Camera size={22} />,
                    label: t('filebrowser.upload.camera', 'Camera'),
                  },
                  {
                    key: 'folder',
                    icon: <FolderPlus size={22} />,
                    label: t('filebrowser.upload.newFolder', 'New folder'),
                  },
                ].map((action) => (
                  <button
                    key={action.key}
                    type="button"
                    onClick={() => {
                      setMobileUploadOpen(false)
                      showToast(`${action.label} (mock)`)
                    }}
                    className="flex flex-col items-center gap-1.5 rounded-[16px] p-2 text-[11px] text-[color:var(--cp-text)] hover:bg-[color:color-mix(in_srgb,var(--cp-accent-soft)_18%,transparent)]"
                  >
                    <div
                      className="flex h-12 w-12 items-center justify-center rounded-full text-[color:var(--cp-accent)]"
                      style={{
                        background:
                          'color-mix(in srgb, var(--cp-accent-soft) 32%, var(--cp-surface))',
                      }}
                    >
                      {action.icon}
                    </div>
                    <span className="text-center leading-tight">{action.label}</span>
                  </button>
                ))}
              </div>
            </div>
          </div>
        ) : null}

        {/* Sidebar drawer */}
        {mobileSidebarOpen ? (
          <div className="absolute inset-0 z-40 flex">
            <div
              className="absolute inset-0 bg-black/50"
              onClick={() => setMobileSidebarOpen(false)}
            />
            <div
              className="relative flex h-full w-2/3 flex-col gap-3 border-r border-[color:var(--cp-border)] p-3"
              style={{ background: 'var(--cp-surface)' }}
            >
              <Sidebar
                dfsRoots={fileBrowserSnapshot.dfsRoots}
                devices={fileBrowserSnapshot.devices}
                topics={fileBrowserSnapshot.topics}
                activePath={left.currentPath}
                activeTopicId={leftTopic?.id ?? null}
                advancedMode={advancedMode}
                onToggleAdvanced={setAdvancedMode}
                onNavigate={left.navigate}
                onSelectTopic={(id) => selectTopicInPane(left, id)}
                compact
              />
            </div>
          </div>
        ) : null}

        {/* Preview bottom sheet */}
        {mobilePreviewOpen && leftSelectedEntry ? (
          <div className="absolute inset-0 z-30 flex items-end">
            <div
              className="absolute inset-0 bg-black/40"
              onClick={() => setMobilePreviewOpen(false)}
            />
            <div
              className="relative flex h-[78%] w-full flex-col rounded-t-[28px] border-t border-[color:var(--cp-border)]"
              style={{ background: 'var(--cp-surface)' }}
            >
              <div className="flex items-center justify-between px-4 py-2">
                <span className="shell-kicker">
                  {t('filebrowser.mobile.preview', 'Preview')}
                </span>
                <button
                  type="button"
                  onClick={() => setMobilePreviewOpen(false)}
                  className="rounded-full border border-[color:var(--cp-border)] px-2.5 py-1 text-xs"
                >
                  {t('common.close', 'Close')}
                </button>
              </div>
              <div className="flex-1 overflow-hidden">
                <PreviewPanel
                  entry={leftSelectedEntry}
                  topics={fileBrowserSnapshot.topics}
                  onJumpToTopic={(id) => {
                    selectTopicInPane(left, id)
                    setMobilePreviewOpen(false)
                  }}
                  onJumpToPath={(path) => {
                    left.navigate(path)
                    setMobilePreviewOpen(false)
                  }}
                  embedded
                />
              </div>
            </div>
          </div>
        ) : null}

        {toast ? (
          <div className="pointer-events-none absolute bottom-14 left-1/2 -translate-x-1/2 rounded-full bg-black/80 px-3 py-1.5 text-xs text-white">
            {toast}
          </div>
        ) : null}
      </div>
    )
  }

  // ─── Desktop layout ───
  // VS Code-like shell: full-height nav tree | one or two self-contained panes
  // (tabs + address row + toolbar + content + status bar) | collapsible sidebar.
  return (
    <div
      className="relative flex h-full w-full overflow-hidden"
      style={{ background: 'var(--cp-bg)' }}
    >
      <aside
        className="hidden w-[260px] shrink-0 flex-col overflow-hidden border-r border-[color:color-mix(in_srgb,var(--cp-border)_60%,transparent)] bg-[color:color-mix(in_srgb,var(--cp-surface)_82%,transparent)] px-2 pt-2 md:flex"
        onMouseDownCapture={() => setFocusedSide('left')}
      >
        <Sidebar
          dfsRoots={fileBrowserSnapshot.dfsRoots}
          devices={fileBrowserSnapshot.devices}
          topics={fileBrowserSnapshot.topics}
          activePath={left.currentPath}
          activeTopicId={leftTopic?.id ?? null}
          advancedMode={advancedMode}
          onToggleAdvanced={setAdvancedMode}
          onNavigate={left.navigate}
          onSelectTopic={(id) => selectTopicInPane(left, id)}
        />
      </aside>

      <main
        className="flex min-w-0 flex-1 flex-col"
        onMouseDownCapture={() => setFocusedSide('left')}
      >
        <TopBar
          tabs={left.tabs}
          activeTabId={left.activeTabId}
          onSelectTab={left.setActiveTabId}
          onCloseTab={handleCloseLeftTab}
          onNewTab={handleNewTab}
          closedTabs={closedTabs}
          onRestoreClosedTab={handleRestoreClosedTab}
          currentPath={left.currentPath}
          onNavigate={left.navigate}
          onBack={left.back}
          onForward={left.forward}
          onUp={left.goUp}
          canBack={left.activeHistory.back.length > 0}
          canForward={left.activeHistory.forward.length > 0}
          canUp={left.currentPath !== '/'}
          viewMode={left.viewMode}
          onViewModeChange={left.setViewMode}
          searchQuery={left.searchQuery}
          onSearchChange={left.setSearchQuery}
          onCopyPath={() => copyText(left.currentPath)}
          onSendTabToRight={handleSendToRight}
          canSendToRight={left.tabs.length > 1}
          onUpload={() => showToast(`${t('filebrowser.actions.upload', 'Upload')} (mock)`)}
          onNewFolder={() => showToast(`${t('filebrowser.actions.newFolder', 'New folder')} (mock)`)}
        />
        <div className="min-h-0 flex-1 overflow-hidden">
          {leftSearchActive ? (
            <SearchResultsPanel
              hits={leftSearchHits}
              query={left.searchQuery}
              onSelect={handleOpenEntry}
            />
          ) : (
            <MainContent
              entries={leftEntries}
              viewMode={left.viewMode}
              selectedId={left.selectedId}
              onSelect={handleOpenEntry}
              onOpenFolder={handleOpenFolder}
              currentPath={left.currentPath}
              topicContext={leftTopic}
            />
          )}
        </div>
        <StatusBar
          currentPath={left.currentPath}
          totalCount={leftEntries.length}
          selection={leftSelectedEntry}
          onCopy={copyText}
          onExpandSidebar={
            !splitActive && previewCollapsed && isXl
              ? () => setPreviewCollapsed(false)
              : undefined
          }
        />
      </main>

      {splitActive ? (
        <section
          className="flex min-w-0 flex-1 flex-col border-l border-[color:color-mix(in_srgb,var(--cp-border)_60%,transparent)]"
          onMouseDownCapture={() => setFocusedSide('right')}
        >
          <TopBar
            tabs={right.tabs}
            activeTabId={right.activeTabId}
            onSelectTab={right.setActiveTabId}
            onCloseTab={handleCloseRightTab}
            showTabControls={false}
            allowCloseLast
            currentPath={right.currentPath}
            onNavigate={right.navigate}
            onBack={right.back}
            onForward={right.forward}
            onUp={right.goUp}
            canBack={right.activeHistory.back.length > 0}
            canForward={right.activeHistory.forward.length > 0}
            canUp={right.currentPath !== '/'}
            viewMode={right.viewMode}
            onViewModeChange={right.setViewMode}
            searchQuery={right.searchQuery}
            onSearchChange={right.setSearchQuery}
            onCopyPath={() => copyText(right.currentPath)}
            onUpload={() => showToast(`${t('filebrowser.actions.upload', 'Upload')} (mock)`)}
            onNewFolder={() => showToast(`${t('filebrowser.actions.newFolder', 'New folder')} (mock)`)}
          />
          <div className="min-h-0 flex-1 overflow-hidden">
            {rightSearchActive ? (
              <SearchResultsPanel
                hits={rightSearchHits}
                query={right.searchQuery}
                onSelect={(entry) => right.setSelectedId(entry.id)}
              />
            ) : (
              <MainContent
                entries={rightEntries}
                viewMode={right.viewMode}
                selectedId={right.selectedId}
                onSelect={(entry) => right.setSelectedId(entry.id)}
                onOpenFolder={(path) => right.navigate(path)}
                currentPath={right.currentPath}
                topicContext={rightTopic}
              />
            )}
          </div>
          <StatusBar
            currentPath={right.currentPath}
            totalCount={rightEntries.length}
            selection={rightSelectedEntry}
            onCopy={copyText}
            onExpandSidebar={
              previewCollapsed && isXl ? () => setPreviewCollapsed(false) : undefined
            }
          />
        </section>
      ) : null}

      {!previewCollapsed ? (
        <aside className="hidden w-[320px] shrink-0 flex-col overflow-hidden border-l border-[color:color-mix(in_srgb,var(--cp-border)_60%,transparent)] bg-[color:color-mix(in_srgb,var(--cp-surface)_86%,transparent)] xl:flex">
          <div className="flex items-center justify-between border-b border-[color:color-mix(in_srgb,var(--cp-border)_60%,transparent)] px-4 py-2">
            <span className="shell-kicker">
              {t('filebrowser.preview.title', 'Preview & Meta')}
            </span>
            <IconButton
              size="small"
              onClick={() => setPreviewCollapsed(true)}
              aria-label={t('filebrowser.preview.collapse', 'Collapse sidebar')}
            >
              <PanelRightClose size={14} />
            </IconButton>
          </div>
          <div className="flex-1 overflow-hidden">
            <PreviewPanel
              entry={focusedSelectedEntry}
              topics={fileBrowserSnapshot.topics}
              onJumpToTopic={(id) => selectTopicInPane(focusedPane, id)}
              onJumpToPath={(path) => focusedPane.navigate(path)}
              embedded
            />
          </div>
        </aside>
      ) : null}

      {toast ? (
        <div className="pointer-events-none absolute bottom-8 left-1/2 -translate-x-1/2 rounded-full bg-black/80 px-3 py-1.5 text-xs text-white">
          {toast}
        </div>
      ) : null}
    </div>
  )
}
