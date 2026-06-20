import { fetchAppList } from '../../../api/app_mgr.ts'
import {
  fetchSelfHostGroupList,
  mockSelfHostGroups,
} from '../../../api/self_host_groups.ts'
import {
  fetchAgentListWithRuntime,
  fetchUserDetail,
  fetchUserList,
} from '../../../api/user_mgr.ts'
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
  toEntityGroupEntity,
  toLocalUserEntity,
  toSelfEntity,
} from './transforms'

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

    if (collection.id === 'col-joined-groups') {
      return { ...collection, entityIds: joinedGroupIds }
    }

    return {
      ...collection,
      entityIds: collection.entityIds.filter((id) => validEntityIds.has(id)),
    }
  })
}

export async function fetchUsersAgentsSnapshot(): Promise<UsersAgentsSnapshot> {
  const [
    usersResult,
    selfResult,
    appsResult,
    agentsResult,
    selfHostGroupsResult,
  ] = await Promise.all([
    fetchUserList(),
    fetchUserDetail(),
    fetchAppList(),
    fetchAgentListWithRuntime(),
    fetchSelfHostGroupList(),
  ])

  const hasCoreData = Boolean(
    usersResult.data ||
      selfResult.data ||
      appsResult.data ||
      agentsResult.data,
  )
  if (!hasCoreData) {
    throw new Error('users-agents core APIs unavailable')
  }

  const apps = appsResult.data?.apps ?? []
  const self = toSelfEntity(selfResult.data, apps, mockSelf)
  const now = new Date().toISOString()
  const localUsers = usersResult.data
    ? usersResult.data.users
        .filter((user) => user.user_id !== self.id)
        .map((user) => toLocalUserEntity(user, apps, now))
    : structuredClone(mockLocalUsers)

  const agentApps = apps.filter((app) => app.is_agent || app.app_type === 'agent')
  const firstAgentInfo = agentsResult.data?.agents[0] ?? null
  const firstAgentId = typeof firstAgentInfo?.agent_id === 'string'
    ? firstAgentInfo.agent_id
    : undefined
  const firstAgentApp =
    agentApps.find((app) => app.app_id === firstAgentId) ??
    agentApps[0] ??
    null
  const agent = toAgentEntity(firstAgentInfo, firstAgentApp, mockAgent)
  const contacts = structuredClone(mockContacts)
  const entityGroups = selfHostGroupsResult.data
    ? selfHostGroupsResult.data.groups.map(toEntityGroupEntity)
    : mockSelfHostGroups.map(toEntityGroupEntity)

  return {
    self,
    agent,
    localUsers,
    contacts,
    entityGroups,
    collections: buildCollections(mockCollections, contacts, entityGroups),
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
