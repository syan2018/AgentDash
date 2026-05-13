import { useMemo } from 'react'
import { LocalRuntimeView } from '@agentdash/views/local-runtime'
import { createTauriLocalRuntimeClient } from './runtimeApi'

function App() {
  const client = useMemo(() => createTauriLocalRuntimeClient(), [])

  return <LocalRuntimeView client={client} />
}

export default App
