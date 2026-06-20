import type {
  AgentEntity,
  AnyEntity,
  Collection,
  ContactEntity,
  EntityGroupEntity,
  LocalUserEntity,
  SelfEntity,
  SocialAccount,
  UsersAgentsSnapshot,
} from './types'
import {
  createMockUsersAgentsSnapshot,
  fetchUsersAgentsSnapshot,
} from './api'

export interface UsersAgentsStoreOptions {
  useMock?: boolean
}

const mockModeValues = new Set(['1', 'true', 'yes', 'mock'])

function defaultUseMock(): boolean {
  const value =
    import.meta.env.VITE_USERS_AGENTS_USE_MOCK ??
    import.meta.env.VITE_USE_MOCK
  return mockModeValues.has(String(value ?? '').toLowerCase())
}

export class UsersAgentsStore {
  private self: SelfEntity
  private agent: AgentEntity
  private localUsers: LocalUserEntity[]
  private contacts: ContactEntity[]
  private entityGroups: EntityGroupEntity[]
  private collections: Collection[]

  private snapshot: UsersAgentsSnapshot
  private listeners = new Set<() => void>()

  constructor(options: UsersAgentsStoreOptions = {}) {
    const initial = createMockUsersAgentsSnapshot()
    this.self = initial.self
    this.agent = initial.agent
    this.localUsers = initial.localUsers
    this.contacts = initial.contacts
    this.entityGroups = initial.entityGroups
    this.collections = initial.collections
    this.snapshot = this.buildSnapshot()

    if (!(options.useMock ?? defaultUseMock())) {
      void this.reload()
    }
  }

  subscribe = (listener: () => void) => {
    this.listeners.add(listener)
    return () => { this.listeners.delete(listener) }
  }

  getSnapshot = (): UsersAgentsSnapshot => this.snapshot

  async reload() {
    try {
      this.applySnapshot(await fetchUsersAgentsSnapshot())
      this.notify()
    } catch (error) {
      console.warn('Failed to load users-agents datamodel; using mock snapshot.', error)
    }
  }

  private applySnapshot(snapshot: UsersAgentsSnapshot) {
    this.self = snapshot.self
    this.agent = snapshot.agent
    this.localUsers = snapshot.localUsers
    this.contacts = snapshot.contacts
    this.entityGroups = snapshot.entityGroups
    this.collections = snapshot.collections
  }

  private notify() {
    this.snapshot = this.buildSnapshot()
    this.listeners.forEach((listener) => listener())
  }

  private buildSnapshot(): UsersAgentsSnapshot {
    return {
      self: this.self,
      agent: this.agent,
      localUsers: this.localUsers,
      contacts: this.contacts,
      entityGroups: this.entityGroups,
      collections: this.collections,
    }
  }

  findEntity(id: string): AnyEntity | undefined {
    if (this.self.id === id) return this.self
    if (this.agent.id === id) return this.agent
    return (
      this.localUsers.find((user) => user.id === id) ??
      this.contacts.find((contact) => contact.id === id) ??
      this.entityGroups.find((group) => group.id === id)
    )
  }

  addLocalUser(user: LocalUserEntity) {
    this.localUsers = [...this.localUsers, user]
    this.notify()
  }

  removeLocalUser(id: string) {
    this.localUsers = this.localUsers.filter((user) => user.id !== id)
    this.notify()
  }

  addContacts(contacts: ContactEntity[]) {
    this.contacts = [...this.contacts, ...contacts]
    const contactIds = contacts.map((contact) => contact.id)
    this.collections = this.collections.map((collection) =>
      collection.type === 'contacts'
        ? {
            ...collection,
            entityIds: Array.from(new Set([...collection.entityIds, ...contactIds])),
            updatedAt: new Date().toISOString(),
          }
        : collection,
    )
    this.notify()
  }

  removeContact(id: string) {
    this.contacts = this.contacts.filter((contact) => contact.id !== id)
    this.collections = this.collections.map((collection) => ({
      ...collection,
      entityIds: collection.entityIds.filter((entityId) => entityId !== id),
    }))
    this.notify()
  }

  mergeContacts(primaryId: string, mergedIds: string[]) {
    const mergedSet = new Set(mergedIds.filter((id) => id !== primaryId))
    if (mergedSet.size === 0) return

    const mergedNames = this.contacts
      .filter((contact) => mergedSet.has(contact.id))
      .map((contact) => contact.displayName)

    this.contacts = this.contacts
      .filter((contact) => !mergedSet.has(contact.id))
      .map((contact) =>
        contact.id === primaryId
          ? {
              ...contact,
              isMergeCandidate: false,
              tags: Array.from(new Set([...contact.tags, 'merged'])),
              notes: [
                contact.notes,
                mergedNames.length > 0
                  ? `Merged with ${mergedNames.join(', ')}.`
                  : undefined,
              ].filter(Boolean).join(' '),
            }
          : contact,
      )

    this.collections = this.collections.map((collection) => {
      const nextIds = collection.entityIds.map((id) =>
        mergedSet.has(id) ? primaryId : id,
      )
      return {
        ...collection,
        entityIds: Array.from(new Set(nextIds)),
        updatedAt: new Date().toISOString(),
      }
    })
    this.notify()
  }

  addCollection(name: string, description = '') {
    const collection: Collection = {
      id: `col-custom-${Date.now()}`,
      name,
      type: 'custom',
      mode: 'static',
      description,
      sourceType: 'manual',
      isBuiltIn: false,
      isReadOnly: false,
      entityIds: [],
      createdAt: new Date().toISOString(),
      updatedAt: new Date().toISOString(),
    }
    this.collections = [...this.collections, collection]
    this.notify()
    return collection
  }

  renameCollection(id: string, name: string) {
    this.collections = this.collections.map((collection) =>
      collection.id === id
        ? { ...collection, name, updatedAt: new Date().toISOString() }
        : collection,
    )
    this.notify()
  }

  removeCollection(id: string) {
    this.collections = this.collections.filter((collection) =>
      collection.id !== id || collection.isBuiltIn,
    )
    this.notify()
  }

  addToCollection(collectionId: string, entityId: string) {
    this.collections = this.collections.map((collection) =>
      collection.id === collectionId &&
      !collection.isReadOnly &&
      !collection.entityIds.includes(entityId)
        ? {
            ...collection,
            entityIds: [...collection.entityIds, entityId],
            updatedAt: new Date().toISOString(),
          }
        : collection,
    )
    this.notify()
  }

  removeFromCollection(collectionId: string, entityId: string) {
    this.collections = this.collections.map((collection) =>
      collection.id === collectionId
        ? {
            ...collection,
            entityIds: collection.entityIds.filter((id) => id !== entityId),
            updatedAt: new Date().toISOString(),
          }
        : collection,
    )
    this.notify()
  }

  removeManyFromCollection(collectionId: string, entityIds: string[]) {
    const removeSet = new Set(entityIds)
    this.collections = this.collections.map((collection) =>
      collection.id === collectionId && !collection.isReadOnly
        ? {
            ...collection,
            entityIds: collection.entityIds.filter((id) => !removeSet.has(id)),
            updatedAt: new Date().toISOString(),
          }
        : collection,
    )
    this.notify()
  }

  updateSelfAvatar(avatarUrl?: string) {
    this.self = { ...this.self, avatarUrl }
    this.notify()
  }

  updateSelfBio(bio: string) {
    this.self = {
      ...this.self,
      bio,
      info: {
        ...this.self.info,
        bio,
      },
    }
    this.notify()
  }

  updateSelfInfo(key: string, value: string) {
    this.self = {
      ...this.self,
      displayName: key === 'displayName' ? value : this.self.displayName,
      bio: key === 'bio' ? value : this.self.bio,
      info: {
        ...this.self.info,
        [key]: value,
      },
    }
    this.notify()
  }

  addSocialAccount(entityId: string, account: SocialAccount) {
    if (this.self.id === entityId) {
      this.self = { ...this.self, socialAccounts: [...this.self.socialAccounts, account] }
      this.notify()
      return
    }

    if (this.agent.id === entityId) {
      this.agent = { ...this.agent, socialAccounts: [...this.agent.socialAccounts, account] }
      this.notify()
      return
    }

    this.localUsers = this.localUsers.map((user) =>
      user.id === entityId
        ? { ...user, socialAccounts: [...user.socialAccounts, account] }
        : user,
    )
    this.contacts = this.contacts.map((contact) =>
      contact.id === entityId
        ? { ...contact, socialAccounts: [...contact.socialAccounts, account] }
        : contact,
    )
    this.entityGroups = this.entityGroups.map((group) =>
      group.id === entityId
        ? { ...group, socialAccounts: [...group.socialAccounts, account] }
        : group,
    )
    this.notify()
  }

  removeSocialAccount(entityId: string, accountId: string) {
    this.updateSocialAccounts(entityId, (accounts) =>
      accounts.filter((account) => account.id !== accountId),
    )
  }

  toggleSocialAccountVisibility(entityId: string, accountId: string) {
    this.updateSocialAccounts(entityId, (accounts) =>
      accounts.map((account) =>
        account.id === accountId
          ? { ...account, isPublic: !account.isPublic }
          : account,
      ),
    )
  }

  private updateSocialAccounts(
    entityId: string,
    updater: (accounts: SocialAccount[]) => SocialAccount[],
  ) {
    if (this.self.id === entityId) {
      this.self = { ...this.self, socialAccounts: updater(this.self.socialAccounts) }
      this.notify()
      return
    }

    if (this.agent.id === entityId) {
      this.agent = { ...this.agent, socialAccounts: updater(this.agent.socialAccounts) }
      this.notify()
      return
    }

    this.localUsers = this.localUsers.map((user) =>
      user.id === entityId
        ? { ...user, socialAccounts: updater(user.socialAccounts) }
        : user,
    )
    this.contacts = this.contacts.map((contact) =>
      contact.id === entityId
        ? { ...contact, socialAccounts: updater(contact.socialAccounts) }
        : contact,
    )
    this.entityGroups = this.entityGroups.map((group) =>
      group.id === entityId
        ? { ...group, socialAccounts: updater(group.socialAccounts) }
        : group,
    )
    this.notify()
  }
}
