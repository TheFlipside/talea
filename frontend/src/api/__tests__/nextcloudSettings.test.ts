import { QueryClient } from '@tanstack/react-query';
import { describe, expect, it, vi } from 'vitest';

import { nextcloudMutation, nextcloudSetConfigOptions } from '../hooks';
import { queryKeys } from '../queryKeys';

const settings = { baseUrl: 'https://cloud.example.com', username: 'max', password: 'p' };

/** Runs a save, through the real mutation cache, that ends with `outcome`. */
async function save(client: QueryClient, outcome: () => Promise<boolean>): Promise<void> {
  const options = { ...nextcloudSetConfigOptions(client), mutationFn: outcome };
  const mutation = client.getMutationCache().build(client, options);
  await mutation.execute(settings).catch(() => undefined);
}

function client(): QueryClient {
  return new QueryClient();
}

describe('saving the Nextcloud settings', () => {
  it('refetches them after a save or an error, but not after a decline', async () => {
    for (const [outcome, refetched] of [
      [() => Promise.resolve(true), true],
      [() => Promise.reject(new Error('changed meanwhile')), true],
      [() => Promise.resolve(false), false],
    ] as const) {
      const queries = client();
      const invalidate = vi.spyOn(queries, 'invalidateQueries').mockResolvedValue();
      await save(queries, outcome);
      if (refetched) {
        expect(invalidate).toHaveBeenCalledWith({ queryKey: queryKeys.nextcloudConfig });
      } else {
        expect(invalidate).not.toHaveBeenCalled();
      }
    }
  });

  it('drops the app password from the cache within a second of finishing', () => {
    // The password rides in the save's variables; TanStack's default would
    // keep a finished save, and it, for five minutes.
    expect(nextcloudSetConfigOptions(client()).gcTime).toBeLessThanOrEqual(1_000);
  });

  it('counts as a running Nextcloud operation while it waits', async () => {
    const queries = client();
    let finish: (saved: boolean) => void = () => undefined;
    const pending = save(queries, () => new Promise((resolve) => (finish = resolve)));
    // What the screen's busy check asks, so a remounted screen still sees it.
    await vi.waitFor(() => expect(queries.isMutating({ mutationKey: nextcloudMutation })).toBe(1));
    finish(true);
    await pending;
    expect(queries.isMutating({ mutationKey: nextcloudMutation })).toBe(0);
  });
});
