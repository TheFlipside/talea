import { onlineManager } from '@tanstack/react-query';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import { createQueryClient } from '../queryClient';

describe("the app's query client", () => {
  let wasOnline = true;
  beforeEach(() => {
    wasOnline = onlineManager.isOnline();
  });
  afterEach(() => {
    onlineManager.setOnline(wasOnline);
  });

  it('reads and saves while the device reports itself offline', async () => {
    // Every query and save is local IPC; with TanStack's default network mode
    // both would pause here until the device came back online.
    onlineManager.setOnline(false);
    const client = createQueryClient();
    const read = client.fetchQuery({ queryKey: ['local'], queryFn: () => Promise.resolve('read') });
    // Paused, the read would only show as a test timeout; this says why.
    expect(client.getQueryState(['local'])?.fetchStatus).toBe('fetching');
    await expect(read).resolves.toBe('read');
    const save = client.getMutationCache().build(client, {
      mutationFn: () => Promise.resolve('saved'),
    });
    await expect(save.execute(undefined)).resolves.toBe('saved');
  });
});
