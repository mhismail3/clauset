import { describe, it, expect, vi } from 'vitest';

describe('Test infrastructure', () => {
  it('should run basic assertions', () => {
    expect(1 + 1).toBe(2);
    expect('hello').toContain('ell');
  });

  it('should have access to mocked localStorage', () => {
    localStorage.setItem('test-key', 'test-value');
    expect(localStorage.getItem('test-key')).toBe('test-value');
  });

  it('should have access to mocked WebSocket', () => {
    const ws = new WebSocket('ws://localhost:8080');
    expect(ws.url).toBe('ws://localhost:8080');
    expect(ws.readyState).toBe(WebSocket.CONNECTING);
  });

  it('should be able to use vi.fn() for mocks', () => {
    const mockFn = vi.fn();
    mockFn('test');
    expect(mockFn).toHaveBeenCalledWith('test');
  });

  it('should handle async operations', async () => {
    const promise = Promise.resolve('async value');
    await expect(promise).resolves.toBe('async value');
  });
});

describe('JSON parsing helpers', () => {
  it('should parse MCP tool names correctly', () => {
    const parseMcpTool = (name: string): { server: string; tool: string } | null => {
      const match = name.match(/^mcp__(.+?)__(.+)$/);
      if (match) {
        return { server: match[1], tool: match[2] };
      }
      return null;
    };

    expect(parseMcpTool('mcp__greptile__list_pull_requests')).toEqual({
      server: 'greptile',
      tool: 'list_pull_requests',
    });

    expect(parseMcpTool('mcp__plugin_playwright_playwright__browser_navigate')).toEqual({
      server: 'plugin_playwright_playwright',
      tool: 'browser_navigate',
    });

    expect(parseMcpTool('Read')).toBeNull();
    expect(parseMcpTool('Bash')).toBeNull();
  });

  it('should parse TodoWrite status correctly', () => {
    const getStatusIcon = (status: string): string => {
      switch (status) {
        case 'completed':
          return '✓';
        case 'in_progress':
          return '◐';
        default:
          return '○';
      }
    };

    expect(getStatusIcon('pending')).toBe('○');
    expect(getStatusIcon('in_progress')).toBe('◐');
    expect(getStatusIcon('completed')).toBe('✓');
  });
});
