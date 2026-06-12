/**
 * Context-menu registry and the built-in (default) providers.
 *
 * `fileBrowserMenuRegistry` is the shared instance the file browser uses;
 * other apps/extensions extend the menu by registering providers on it, and
 * system settings shape it through `setConfig`.
 */

import type {
  FileMenuAction,
  FileMenuConfig,
  FileMenuContext,
  FileMenuLabel,
  FileMenuProvider,
  FileMenuSection,
} from './types'

export class FileMenuRegistry {
  private providers = new Map<string, FileMenuProvider>()
  private config: FileMenuConfig = {}

  /** Register a provider; returns a disposer that unregisters it. */
  register(provider: FileMenuProvider): () => void {
    this.providers.set(provider.id, provider)
    return () => {
      this.providers.delete(provider.id)
    }
  }

  unregister(id: string) {
    this.providers.delete(id)
  }

  setConfig(config: FileMenuConfig) {
    this.config = config
  }

  getConfig(): FileMenuConfig {
    return this.config
  }

  /** Resolve the menu for one invocation context. */
  build(context: FileMenuContext): FileMenuSection[] {
    const { hiddenProviders = [], hiddenItems = [], providerOrder = {} } = this.config
    return [...this.providers.values()]
      .filter((provider) => !hiddenProviders.includes(provider.id))
      .sort(
        (a, b) => (providerOrder[a.id] ?? a.order) - (providerOrder[b.id] ?? b.order),
      )
      .filter((provider) => !provider.when || provider.when(context))
      .flatMap((provider) => provider.build(context))
      .map((section) => section.filter((item) => !hiddenItems.includes(item.id)))
      .filter((section) => section.length > 0)
  }
}

function label(
  key: string,
  fallback: string,
  vars?: Record<string, string | number>,
): FileMenuLabel {
  return { key: `filebrowser.menu.${key}`, fallback, vars }
}

function action(
  id: string,
  labelValue: FileMenuLabel,
  extra?: Partial<Omit<FileMenuAction, 'type' | 'id' | 'label'>>,
): FileMenuAction {
  return { type: 'action', id, command: extra?.command ?? id, label: labelValue, ...extra }
}

const isItem = (ctx: FileMenuContext) => ctx.target === 'item'
const isSelection = (ctx: FileMenuContext) => ctx.target === 'selection'
const isView = (ctx: FileMenuContext) => ctx.target === 'view'

/** Open / preview — single item only. */
const openProvider: FileMenuProvider = {
  id: 'default.open',
  order: 100,
  when: isItem,
  build: (ctx) => {
    const entry = ctx.entries[0]
    if (!entry) return []
    if (entry.kind === 'folder') {
      const section: FileMenuSection = [
        action('open', label('open', 'Open'), { icon: 'open' }),
      ]
      if (ctx.capabilities.canOpenInNewTab) {
        section.push(
          action('open-new-tab', label('openNewTab', 'Open in new tab'), {
            icon: 'open-new-tab',
          }),
        )
      }
      if (ctx.capabilities.canOpenInRightPane) {
        section.push(
          action('open-right', label('openRight', 'Open in right pane'), {
            icon: 'open-right',
          }),
        )
      }
      return [section]
    }
    return [[action('open', label('preview', 'Preview'), { icon: 'preview' })]]
  },
}

/** Download / share — files and multi-selections. */
const transferProvider: FileMenuProvider = {
  id: 'default.transfer',
  order: 200,
  when: (ctx) => isItem(ctx) || isSelection(ctx),
  build: (ctx) => {
    const section: FileMenuSection = []
    const count = ctx.entries.length
    if (isSelection(ctx)) {
      section.push(
        action('download', label('downloadN', 'Download {{count}} items', { count }), {
          icon: 'download',
        }),
      )
    } else if (ctx.entries[0]?.kind !== 'folder') {
      section.push(action('download', label('download', 'Download'), { icon: 'download' }))
    }
    if (isItem(ctx)) {
      section.push(action('share', label('share', 'Share…'), { icon: 'share' }))
    }
    return section.length ? [section] : []
  },
}

/** Clipboard — copy paths / public URL. */
const clipboardProvider: FileMenuProvider = {
  id: 'default.clipboard',
  order: 300,
  when: (ctx) => isItem(ctx) || isSelection(ctx),
  build: (ctx) => {
    const count = ctx.entries.length
    const section: FileMenuSection = [
      isSelection(ctx)
        ? action('copy-path', label('copyPaths', 'Copy {{count}} paths', { count }), {
            icon: 'copy',
          })
        : action('copy-path', label('copyPath', 'Copy path'), { icon: 'copy' }),
    ]
    if (isItem(ctx) && ctx.entries[0]?.publicUrl) {
      section.push(
        action('copy-public-url', label('copyPublicUrl', 'Copy public URL'), {
          icon: 'link',
        }),
      )
    }
    return [section]
  },
}

/**
 * Rename / move — hidden inside topic views, where entries are aggregated
 * references rather than real children of the current folder.
 */
const organizeProvider: FileMenuProvider = {
  id: 'default.organize',
  order: 400,
  when: (ctx) => (isItem(ctx) || isSelection(ctx)) && !ctx.topic,
  build: (ctx) => {
    const section: FileMenuSection = []
    if (isItem(ctx)) {
      section.push(action('rename', label('rename', 'Rename…'), { icon: 'rename' }))
    }
    if (ctx.moveTargets.length) {
      section.push({
        type: 'submenu',
        id: 'move-to',
        label: label('moveTo', 'Move to'),
        icon: 'move',
        items: ctx.moveTargets.map((target) =>
          action(`move-to:${target.path}`, { key: '', fallback: target.label }, {
            command: 'move-to',
            args: { path: target.path },
            icon: 'open',
          }),
        ),
      })
    }
    return section.length ? [section] : []
  },
}

/** Destructive actions — last section; hidden inside topic views. */
const dangerProvider: FileMenuProvider = {
  id: 'default.danger',
  order: 900,
  when: (ctx) => (isItem(ctx) || isSelection(ctx)) && !ctx.topic,
  build: (ctx) => {
    const count = ctx.entries.length
    return [
      [
        isSelection(ctx)
          ? action('delete', label('deleteN', 'Delete {{count}} items', { count }), {
              icon: 'trash',
              danger: true,
            })
          : action('delete', label('delete', 'Delete'), { icon: 'trash', danger: true }),
      ],
    ]
  },
}

/** Blank-area: create — hidden inside topic views (aggregated, not a folder). */
const viewCreateProvider: FileMenuProvider = {
  id: 'default.view-create',
  order: 100,
  when: (ctx) => isView(ctx) && !ctx.topic,
  build: () => [
    [
      action('new-folder', label('newFolder', 'New folder'), { icon: 'new-folder' }),
      action('upload', label('upload', 'Upload…'), { icon: 'upload' }),
    ],
  ],
}

/** Blank-area: display mode + refresh. */
const viewDisplayProvider: FileMenuProvider = {
  id: 'default.view-display',
  order: 200,
  when: isView,
  build: (ctx) => [
    [
      action('view-list', label('viewList', 'View as list'), {
        icon: 'view-list',
        disabled: ctx.viewMode === 'list',
      }),
      action('view-icon', label('viewIcon', 'View as icons'), {
        icon: 'view-icon',
        disabled: ctx.viewMode === 'icon',
      }),
      action('refresh', label('refresh', 'Refresh'), { icon: 'refresh' }),
    ],
  ],
}

/** Blank-area: selection helpers + current path. */
const viewSelectProvider: FileMenuProvider = {
  id: 'default.view-select',
  order: 300,
  when: isView,
  build: () => [
    [
      action('select-all', label('selectAll', 'Select all'), { icon: 'select-all' }),
      action('copy-path', label('copyCurrentPath', 'Copy current path'), { icon: 'copy' }),
    ],
  ],
}

export function createDefaultFileMenuRegistry(): FileMenuRegistry {
  const registry = new FileMenuRegistry()
  registry.register(openProvider)
  registry.register(transferProvider)
  registry.register(clipboardProvider)
  registry.register(organizeProvider)
  registry.register(dangerProvider)
  registry.register(viewCreateProvider)
  registry.register(viewDisplayProvider)
  registry.register(viewSelectProvider)
  return registry
}

/** Shared registry instance — the file manager's extension point. */
export const fileBrowserMenuRegistry = createDefaultFileMenuRegistry()
