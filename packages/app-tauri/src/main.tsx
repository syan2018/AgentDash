import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import App from './App'
import 'app-web/styles.css'
import './styles.css'
import '@agentdash/views/local-runtime.css'

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App />
  </StrictMode>,
)
