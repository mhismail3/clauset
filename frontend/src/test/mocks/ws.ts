import { vi } from 'vitest';

export interface MockWsMessage {
  type: string;
  [key: string]: unknown;
}

export interface MockWebSocketOptions {
  autoConnect?: boolean;
  simulateLatency?: number;
}

/**
 * Mock WebSocket for testing WebSocket-dependent code.
 * Provides methods to simulate server messages and connection states.
 */
export class TestWebSocket {
  url: string;
  readyState: number = WebSocket.CONNECTING;
  onopen: ((event: Event) => void) | null = null;
  onclose: ((event: CloseEvent) => void) | null = null;
  onmessage: ((event: MessageEvent) => void) | null = null;
  onerror: ((event: Event) => void) | null = null;

  private sentMessages: string[] = [];
  private options: MockWebSocketOptions;

  constructor(url: string, options: MockWebSocketOptions = {}) {
    this.url = url;
    this.options = { autoConnect: true, ...options };

    if (this.options.autoConnect) {
      this.simulateOpen();
    }
  }

  /** Simulate connection opening */
  simulateOpen(): void {
    const delay = this.options.simulateLatency ?? 0;
    setTimeout(() => {
      this.readyState = WebSocket.OPEN;
      if (this.onopen) {
        this.onopen(new Event('open'));
      }
    }, delay);
  }

  /** Simulate receiving a message from server */
  simulateMessage(data: MockWsMessage): void {
    if (this.readyState !== WebSocket.OPEN) {
      throw new Error('WebSocket is not open');
    }

    const delay = this.options.simulateLatency ?? 0;
    setTimeout(() => {
      if (this.onmessage) {
        this.onmessage(new MessageEvent('message', {
          data: JSON.stringify(data),
        }));
      }
    }, delay);
  }

  /** Simulate receiving binary data from server */
  simulateBinaryMessage(data: ArrayBuffer): void {
    if (this.readyState !== WebSocket.OPEN) {
      throw new Error('WebSocket is not open');
    }

    if (this.onmessage) {
      this.onmessage(new MessageEvent('message', { data }));
    }
  }

  /** Simulate connection error */
  simulateError(message: string = 'Connection error'): void {
    if (this.onerror) {
      const error = new ErrorEvent('error', { message });
      this.onerror(error);
    }
  }

  /** Simulate connection close */
  simulateClose(code: number = 1000, reason: string = ''): void {
    this.readyState = WebSocket.CLOSED;
    if (this.onclose) {
      this.onclose(new CloseEvent('close', { code, reason }));
    }
  }

  /** Send data (records for testing) */
  send(data: string | ArrayBuffer): void {
    if (this.readyState !== WebSocket.OPEN) {
      throw new Error('WebSocket is not open');
    }

    if (typeof data === 'string') {
      this.sentMessages.push(data);
    }
  }

  /** Close connection */
  close(_code?: number, _reason?: string): void {
    this.readyState = WebSocket.CLOSING;
    setTimeout(() => {
      this.simulateClose();
    }, 0);
  }

  /** Get all messages sent by client */
  getSentMessages(): string[] {
    return [...this.sentMessages];
  }

  /** Get parsed JSON messages sent by client */
  getSentJsonMessages<T = MockWsMessage>(): T[] {
    return this.sentMessages.map((msg) => JSON.parse(msg) as T);
  }

  /** Clear sent messages */
  clearSentMessages(): void {
    this.sentMessages = [];
  }
}

/**
 * Create a mock WebSocket factory for injecting into code under test.
 */
export function createMockWebSocketFactory() {
  const instances: TestWebSocket[] = [];

  const factory = vi.fn((url: string) => {
    const ws = new TestWebSocket(url);
    instances.push(ws);
    return ws;
  });

  return {
    factory,
    getInstances: () => instances,
    getLastInstance: () => instances[instances.length - 1],
    clearInstances: () => {
      instances.length = 0;
    },
  };
}

/**
 * Helper to wait for WebSocket to be in a specific state
 */
export async function waitForWsState(
  ws: TestWebSocket,
  state: number,
  timeout: number = 1000
): Promise<void> {
  const start = Date.now();

  while (ws.readyState !== state) {
    if (Date.now() - start > timeout) {
      throw new Error(`Timeout waiting for WebSocket state ${state}`);
    }
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
}

/**
 * Simulate a sequence of server messages with delays
 */
export async function simulateMessageSequence(
  ws: TestWebSocket,
  messages: MockWsMessage[],
  delayMs: number = 50
): Promise<void> {
  for (const msg of messages) {
    ws.simulateMessage(msg);
    await new Promise((resolve) => setTimeout(resolve, delayMs));
  }
}
