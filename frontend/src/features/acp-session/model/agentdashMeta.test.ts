import { describe, expect, it } from 'vitest';
import type { SessionUpdate } from '@agentclientprotocol/sdk';

import { extractAgentDashMetaFromUpdate, parseAgentDashMeta } from './agentdashMeta';

describe('agentdashMeta', () => {
  it('parses _meta.agentdash v1', () => {
    const meta = {
      agentdash: {
        v: 1,
        source: { connectorId: 'vibe_kanban', connectorType: 'local_executor' },
      },
    };
    expect(parseAgentDashMeta(meta)?.v).toBe(1);
  });

  it('returns null for missing/invalid version', () => {
    expect(parseAgentDashMeta({})).toBeNull();
    expect(parseAgentDashMeta({ agentdash: { v: 2 } })).toBeNull();
  });

  it('extracts from update._meta', () => {
    const update = {
      sessionUpdate: 'session_info_update',
      _meta: { agentdash: { v: 1, event: { type: 'system_message', message: 'hook_started' } } },
    } as unknown as SessionUpdate;
    expect(extractAgentDashMetaFromUpdate(update)?.event?.type).toBe('system_message');
  });

  it('extracts from update.content._meta (chunk wrapper)', () => {
    const update = {
      sessionUpdate: 'agent_message_chunk',
      content: {
        type: 'text',
        text: 'hello',
        _meta: { agentdash: { v: 1, trace: { turnId: 't1' } } },
      },
    } as unknown as SessionUpdate;
    expect(extractAgentDashMetaFromUpdate(update)?.trace?.turnId).toBe('t1');
  });
});

