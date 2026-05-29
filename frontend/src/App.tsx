import { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';

import './App.css';

/**
 * Mirrors the `SmokeInfo` payload returned by the Rust `smoke_check` command.
 * `sampleAmount` arrives as a *string*, never a number — money never crosses
 * the boundary as floating point.
 */
interface SmokeInfo {
  greeting: string;
  coreVersion: string;
  sampleAmount: string;
}

/**
 * `invoke<T>` does no runtime checking — the generic is a promise, not a
 * guarantee. Validate the shape so a Rust/TS contract drift (e.g. a renamed
 * field) surfaces as a clear error instead of silently rendering blanks, which
 * would defeat the purpose of a smoke check.
 */
function isSmokeInfo(value: unknown): value is SmokeInfo {
  if (typeof value !== 'object' || value === null) {
    return false;
  }
  const candidate = value as Record<string, unknown>;
  return (
    typeof candidate.greeting === 'string' &&
    typeof candidate.coreVersion === 'string' &&
    typeof candidate.sampleAmount === 'string'
  );
}

function App() {
  const [name, setName] = useState('');
  const [info, setInfo] = useState<SmokeInfo | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function runSmokeCheck(): Promise<void> {
    try {
      const result: unknown = await invoke('smoke_check', { name });
      if (!isSmokeInfo(result)) {
        throw new Error('smoke_check returned an unexpected payload shape');
      }
      setInfo(result);
      setError(null);
    } catch (cause) {
      // Tauri rejects with a string; JS failures arrive as Error objects.
      setError(cause instanceof Error ? cause.message : String(cause));
      setInfo(null);
    }
  }

  return (
    <main className="container">
      {/* Abstract ring — a nod to the planned home-screen widget. */}
      <div className="ring" aria-hidden="true" />

      <h1>Talea</h1>
      <p className="tagline">Local-first budgeting. Scaffold smoke screen.</p>

      <form
        className="row"
        onSubmit={(event) => {
          event.preventDefault();
          // `void`: errors are handled inside runSmokeCheck; the handler
          // itself cannot be async. Keep the void to satisfy no-floating-promises.
          void runSmokeCheck();
        }}
      >
        <input
          value={name}
          onChange={(event) => {
            setName(event.currentTarget.value);
          }}
          placeholder="Your name"
          aria-label="Your name"
        />
        <button type="submit">Run smoke check</button>
      </form>

      {info && (
        <section className="result">
          <p>{info.greeting}</p>
          <dl>
            <dt>core version</dt>
            <dd>{info.coreVersion}</dd>
            <dt>sample amount (from core, as string)</dt>
            <dd>{info.sampleAmount}</dd>
          </dl>
        </section>
      )}

      {error && <p className="error">{error}</p>}
    </main>
  );
}

export default App;
