import { z } from 'zod'

/* ── Users & Agents – type definitions ── */

// ── Entity types ──

export type EntityKind = 'self' | 'agent' | 'local-user' | 'contact' | 'entity-group'

export interface MessageTunnelBinding {
  id: string
  platform: string          // e.g. 'telegram', 'email', 'wechat'
  accountId: string
  displayId: string
  status: 'active' | 'pending' | 'error'
  lastSyncAt?: string
}

export interface EntityBase {
  id: string
  kind: EntityKind
  displayName: string
  avatarUrl?: string
  did?: string
  bindings: MessageTunnelBinding[]
  createdAt: string
}

// ── Self ──

export interface SelfEntity extends EntityBase {
  kind: 'self'
  bio?: string
  email?: string
  phone?: string
  info: Record<string, string>          // lightweight public profile
  settings: Record<string, string>
  didDocument?: Record<string, unknown> // serious identity data
  twoFactorEnabled: boolean
  lastLogin: string
}

// ── Agent ──

export interface AgentEntity extends EntityBase {
  kind: 'agent'
  agentType: string
  version: string
  status: 'running' | 'stopped' | 'error'
  capabilities: string[]
  info: Record<string, string>
  settings: Record<string, string>
  didDocument?: Record<string, unknown>
  runtime: {
    uptime: string
    memoryUsage: string
    cpuUsage: string
    lastActive: string
    uiSessions: number
    workSessions: number
    workspaces: number
  }
}

// ── Local space user ──

export type ZoneUserSource = 'primary-did' | 'local-account'
export type ZoneUserType = 'admin' | 'user' | 'limited'
export type ZoneUserStatus = 'active' | 'pending-invitation' | 'suspended'
export type CredentialStatus = 'invite-pending' | 'password-set' | 'passkey-ready'

export interface ZoneInvitation {
  inviteUrl: string
  targetZone: string
  requestedDid: string
  expiresAt: string
  bindedZoneListKey: 'binded_zone_list'
}

export interface LocalUserEntity extends EntityBase {
  kind: 'local-user'
  role: ZoneUserType
  source: ZoneUserSource
  status: ZoneUserStatus
  credentialStatus: CredentialStatus
  canChangePassword: boolean
  storageUsed: string
  storageQuota: string
  lastActive: string
  isOnline: boolean
  availableApps: string[]
  defaultGroup: string
  profile: Record<string, string>
  settings: Record<string, string>
  invitation?: ZoneInvitation
}

// ── Contact ──

export interface ContactEntity extends EntityBase {
  kind: 'contact'
  source: 'manual' | 'imported' | 'telegram' | 'email' | 'discovered'
  sourceLabel?: string     // e.g. 'Bob.telegram'
  isVerified: boolean      // bidirectional relationship
  relation: 'one-way' | 'mutual'
  lastSyncedAt?: string
  importBatch?: string
  isMergeCandidate?: boolean
  tags: string[]
  notes?: string
  lastInteraction?: string
}

// ── Entity group ──

export interface EntityGroupEntity extends EntityBase {
  kind: 'entity-group'
  description?: string
  memberCount: number
  memberIds: string[]
  ownerName?: string
  isHostedBySelf: boolean
  canMessage: boolean
}

// ── Union type ──

export type AnyEntity =
  | SelfEntity
  | AgentEntity
  | LocalUserEntity
  | ContactEntity
  | EntityGroupEntity

// ── Collections ──

export type CollectionType = 'friends' | 'groups' | 'custom'

export interface Collection {
  id: string
  name: string
  type: CollectionType
  sourceType: 'built-in' | 'manual' | 'import' | 'sync'
  isBuiltIn: boolean
  entityIds: string[]
  createdAt: string
  updatedAt: string
}

// ── View state ──

export type ViewMode = 'entity-detail' | 'collection-browse'

export type SidebarSelection =
  | { kind: 'entity'; entityId: string }
  | { kind: 'collection'; collectionId: string }

// ── Store snapshot ──

export interface UsersAgentsSnapshot {
  self: SelfEntity
  agent: AgentEntity
  localUsers: LocalUserEntity[]
  contacts: ContactEntity[]
  entityGroups: EntityGroupEntity[]
  collections: Collection[]
}

// ── New user wizard ──

export const userAppOptions = ['Files', 'MessageHub', 'HomeStation', 'Settings'] as const
export const storageQuotaOptions = ['1 GB', '5 GB', '10 GB', '50 GB', '100 GB'] as const

export const newZoneUserInputSchema = z
  .object({
    source: z.enum(['primary-did', 'local-account']),
    identifier: z.string().trim().min(2).max(80),
    displayName: z.string().trim().min(2).max(64),
    userType: z.enum(['admin', 'user', 'limited']),
    storageQuota: z.enum(storageQuotaOptions),
    invitationExpiresIn: z.enum(['24h', '7d', '30d']),
    localPassword: z.string().max(128).optional(),
    availableApps: z.array(z.enum(userAppOptions)).min(1),
  })
  .superRefine((value, ctx) => {
    if (value.source === 'primary-did') {
      const isDid = value.identifier.startsWith('did:')
      const isBns = /^[a-z0-9][a-z0-9.-]{1,78}$/i.test(value.identifier)
      if (!isDid && !isBns) {
        ctx.addIssue({
          code: 'custom',
          path: ['identifier'],
          message: 'Enter a primary DID or BNS name.',
        })
      }
      return
    }

    if (!/^[a-z][a-z0-9_-]{1,31}$/i.test(value.identifier)) {
      ctx.addIssue({
        code: 'custom',
        path: ['identifier'],
        message: 'Use 2-32 letters, numbers, underscores, or hyphens.',
      })
    }

    if (!value.localPassword || value.localPassword.length < 8) {
      ctx.addIssue({
        code: 'custom',
        path: ['localPassword'],
        message: 'Local account password must be at least 8 characters.',
      })
    }
  })

export type NewZoneUserInput = z.infer<typeof newZoneUserInputSchema>

export interface NewUserDraft {
  step: number
  source: ZoneUserSource
  identifier: string
  displayName: string
  role: ZoneUserType
  invitationExpiresIn: '24h' | '7d' | '30d'
  initialPassword?: string
  storageQuota: string
  availableApps: string[]
}
