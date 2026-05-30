import React from 'react';
import ReactDOM from 'react-dom/client';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';

import App from './App';
import { LockGate } from './components/LockGate';
import './i18n';
import { AppProviders } from './state/AppProviders';
import './styles.css';

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      // Local IPC: retrying a validation/not-found error just delays display.
      retry: false,
      refetchOnWindowFocus: false,
      staleTime: 30_000,
    },
  },
});

const rootElement = document.getElementById('root');
if (!rootElement) {
  throw new Error('Root element #root not found');
}

ReactDOM.createRoot(rootElement).render(
  <React.StrictMode>
    <QueryClientProvider client={queryClient}>
      <AppProviders>
        <LockGate>
          <App />
        </LockGate>
      </AppProviders>
    </QueryClientProvider>
  </React.StrictMode>,
);
