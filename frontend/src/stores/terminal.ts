// Persistent terminal output store
// Keeps terminal output in memory so it survives navigation

import { createSignal } from 'solid-js';

// Store terminal output as concatenated Uint8Arrays per session
const terminalBuffers = new Map<string, Uint8Array[]>();

// Maximum buffer size per session (prevent memory issues)
const MAX_BUFFER_SIZE = 500; // Max number of chunks to store

export function appendTerminalOutput(sessionId: string, data: Uint8Array) {
  if (!terminalBuffers.has(sessionId)) {
    terminalBuffers.set(sessionId, []);
  }

  const buffer = terminalBuffers.get(sessionId)!;
  buffer.push(data);

  // Trim if too large (keep most recent)
  if (buffer.length > MAX_BUFFER_SIZE) {
    buffer.splice(0, buffer.length - MAX_BUFFER_SIZE);
  }
}

export function getTerminalHistory(sessionId: string): Uint8Array[] {
  return terminalBuffers.get(sessionId) || [];
}

export function clearTerminalHistory(sessionId: string) {
  terminalBuffers.delete(sessionId);
}

export function hasTerminalHistory(sessionId: string): boolean {
  const buffer = terminalBuffers.get(sessionId);
  return !!buffer && buffer.length > 0;
}
