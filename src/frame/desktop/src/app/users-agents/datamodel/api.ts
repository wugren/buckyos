import { fetchAppList } from '../../../api/app_mgr.ts'
import { mockSelfHostGroups } from '../../../api/self_host_groups.ts'
import {
  fetchAgentListWithRuntime,
  fetchUserDetail,
  fetchUserList,
} from '../../../api/user_mgr.ts'
import type { UserDetail } from '../../../api/user_mgr.ts'
import { buckyos } from 'buckyos'
import {
  mockAgent,
  mockCollections,
  mockContacts,
  mockLocalUsers,
  mockSelf,
} from '../mock/seed'
import type {
  Collection,
  ContactEntity,
  EntityGroupEntity,
  UsersAgentsSnapshot,
} from './types'
import {
  toAgentEntity,
  toContactEntity,
  toEntityGroupEntity,
  toLocalUserEntity,
  toSelfEntity,
} from './transforms'

interface ChatContactListResponse {
  items?: unknown[]
}

type GroupListByMemberResponse = unknown[]

interface FetchUsersAgentsSnapshotOptions {
  includeNetwork?: boolean
}

interface AccountInfo {
  user_name: string
  user_id: string
  user_type?: string
}

function accountDetail(account: AccountInfo): UserDetail {
  return {
    user_id: account.user_id,
    show_name: account.user_name || account.user_id,
    user_type: account.user_type || 'user',
    state: 'active',
    res_pool_id: '',
    profile: {
      display_name: account.user_name || account.user_id,
    },
    allow_password_change: false,
  }
}

async function fetchChatContactList(ownerDid?: string): Promise<{
  data: ChatContactListResponse | null
  error: unknown
}> {
  try {
    const rpcClient = buckyos.getServiceRpcClient('msg-center')
    const result = await rpcClient.call('contact.list_contacts', {
      query: { limit: 100 },
      contact_mgr_owner: ownerDid,
    })
    return { data: { items: Array.isArray(result) ? result : [] }, error: null }
  } catch (error) {
    console.warn('Failed to load contacts from msg-center.', error)
    return { data: null, error }
  }
}

async function fetchGroupListByMember(memberDid?: string): Promise<{
  data: GroupListByMemberResponse | null
  error: unknown
}> {
  if (!memberDid) {
    return { data: [], error: null }
  }

  try {
    const rpcClient = buckyos.getServiceRpcClient('msg-center')
    const result = await rpcClient.call('group.list_by_member', {
      member_did: memberDid,
      host_owner: memberDid,
    })
    return { data: Array.isArray(result) ? result : [], error: null }
  } catch (error) {
    console.warn('Failed to load users-agents groups from msg-center.', error)
    return { data: null, error }
  }
}

function buildCollections(
  baseCollections: Collection[],
  contacts: ContactEntity[],
  entityGroups: EntityGroupEntity[],
): Collection[] {
  const contactIds = contacts.map((contact) => contact.id)
  const joinedGroupIds = entityGroups
    .filter((group) => !group.isHostedBySelf)
    .map((group) => group.id)
  const validEntityIds = new Set([
    ...contactIds,
    ...entityGroups.map((group) => group.id),
  ])

  return baseCollections.map((collection) => {
    if (collection.id === 'col-contacts') {
      return { ...collection, entityIds: contactIds }
    }

    if (collection.id === 'col-friends') {
      return {
        ...collection,
        entityIds: contacts
          .filter((contact) => contact.relation === 'mutual' || contact.isVerified)
          .map((contact) => contact.id),
      }
    }

    if (collection.id === 'col-joined-groups') {
      return { ...collection, entityIds: joinedGroupIds }
    }

    if (collection.id === 'col-github-view') {
      return {
        ...collection,
        entityIds: contacts
          .filter((contact) =>
            contact.socialAccounts.some((account) => account.platform === 'github'),
          )
          .map((contact) => contact.id),
      }
    }

    return {
      ...collection,
      entityIds: collection.entityIds.filter((id) => validEntityIds.has(id)),
    }
  })
}

function realBaseCollections(): Collection[] {
  return structuredClone(mockCollections).filter((collection) => collection.isBuiltIn)
}

export async function fetchUsersAgentsSnapshot(
  options: FetchUsersAgentsSnapshotOptions = {},
): Promise<UsersAgentsSnapshot> {
  const includeNetwork = options.includeNetwork ?? false
  const accountInfo = await buckyos.getAccountInfo() as AccountInfo | null
  const selfUserId = accountInfo?.user_id
  const [
    usersResult,
    selfResult,
    appsResult,
    agentsResult,
  ] = await Promise.all([
    fetchUserList(),
    fetchUserDetail(selfUserId ? { userId: selfUserId } : {}),
    fetchAppList(selfUserId ? { userId: selfUserId } : {}),
    fetchAgentListWithRuntime(),
  ])

  const selfDetail = selfResult.data ?? (accountInfo ? accountDetail(accountInfo) : null)
  const hasCoreData = Boolean(
    selfDetail ||
      usersResult.data ||
      appsResult.data ||
      agentsResult.data,
  )
  if (!hasCoreData) {
    throw new Error('users-agents core APIs unavailable')
  }

  const empty = createEmptyUsersAgentsSnapshot()
  const apps = appsResult.data?.apps ?? []
  const self = toSelfEntity(selfDetail, apps, empty.self)
  const now = new Date().toISOString()
  const localUsers = usersResult.data
    ? usersResult.data.users
        .filter((user) => user.user_id !== self.id)
        .map((user) => toLocalUserEntity(user, apps, now))
    : []

  const agentApps = apps.filter((app) => app.is_agent || app.app_type === 'agent')
  const firstAgentInfo = agentsResult.data?.agents[0] ?? null
  const firstAgentId = typeof firstAgentInfo?.agent_id === 'string'
    ? firstAgentInfo.agent_id
    : undefined
  const firstAgentApp =
    agentApps.find((app) => app.app_id === firstAgentId) ??
    agentApps[0] ??
    null
  const agent = toAgentEntity(firstAgentInfo, firstAgentApp, empty.agent)
  const contactsResult = includeNetwork
    ? await fetchChatContactList(self.did)
    : { data: null }
  const contacts = (contactsResult.data?.items ?? []).map(toContactEntity)
  const groupsResult = await fetchGroupListByMember(self.did)
  const entityGroups = (groupsResult.data ?? []).map(toEntityGroupEntity)

  return {
    self,
    agent,
    localUsers,
    contacts,
    entityGroups,
    collections: buildCollections(realBaseCollections(), contacts, entityGroups),
  }
}

export function createEmptyUsersAgentsSnapshot(): UsersAgentsSnapshot {
  const createdAt = '1970-01-01T00:00:00Z'
  const self: UsersAgentsSnapshot['self'] = {
    id: 'self',
    kind: 'self',
    displayName: 'Current User',
    socialAccounts: [],
    createdAt,
    info: {},
    settings: {},
    twoFactorEnabled: false,
    lastLogin: 'Unknown',
  }
  const agent: UsersAgentsSnapshot['agent'] = {
    id: 'agent-unavailable',
    kind: 'agent',
    displayName: 'No agent configured',
    agentType: 'agent',
    version: 'unknown',
    status: 'stopped',
    capabilities: [],
    socialAccounts: [],
    info: {},
    settings: {},
    runtime: {
      uptime: 'Offline',
      memoryUsage: 'Unknown',
      cpuUsage: 'Unknown',
      lastActive: 'Unknown',
      runningTasks: 0,
      queuedTasks: 0,
      healthStatus: 'offline',
      uiSessions: 0,
      workSessions: 0,
      workspaces: 0,
    },
    createdAt,
  }
  const contacts: ContactEntity[] = []
  const entityGroups: EntityGroupEntity[] = []

  return {
    self,
    agent,
    localUsers: [],
    contacts,
    entityGroups,
    collections: buildCollections(realBaseCollections(), contacts, entityGroups),
  }
}

export function createMockUsersAgentsSnapshot(): UsersAgentsSnapshot {
  const contacts = structuredClone(mockContacts)
  const entityGroups = mockSelfHostGroups.map(toEntityGroupEntity)
  return {
    self: structuredClone(mockSelf),
    agent: structuredClone(mockAgent),
    localUsers: structuredClone(mockLocalUsers),
    contacts,
    entityGroups,
    collections: buildCollections(
      structuredClone(mockCollections),
      contacts,
      entityGroups,
    ),
  }
}
