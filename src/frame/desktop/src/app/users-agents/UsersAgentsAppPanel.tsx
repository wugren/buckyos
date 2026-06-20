/* ── Users & Agents – app panel entry point ── */

import { useState } from 'react'
import { UsersAgentsStoreContext } from './hooks/use-users-agents-store'
import { UsersAgentsMockStore } from './mock/store'
import { UsersAgentsShell } from './components/layout/UsersAgentsShell'

export function UsersAgentsAppPanel() {
  const [store] = useState(() => new UsersAgentsMockStore())

  return (
    <UsersAgentsStoreContext.Provider value={store}>
      <UsersAgentsShell />
    </UsersAgentsStoreContext.Provider>
  )
}
