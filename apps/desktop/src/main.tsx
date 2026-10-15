/**
 * Desktop entry point: applies the persisted theme, loads the design system,
 * and mounts the shell.
 */
import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';

import { App } from './App';
import { applyTheme, resolveInitialTheme } from './features/theme/theme';
import './styles/design-system.css';
import './styles/app.css';

applyTheme(resolveInitialTheme());

const container = document.getElementById('root');
if (!container) {
  throw new Error('root container is missing');
}

createRoot(container).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
