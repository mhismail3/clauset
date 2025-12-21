// Persistent terminal output store
// Keeps terminal output both in memory and localStorage for persistence across page reloads

const STORAGE_KEY_PREFIX = 'clauset_terminal_';
const MAX_STORAGE_SIZE = 500000; // ~500KB per session max
const MAX_CHUNKS_MEMORY = 1000; // Max chunks to keep in memory

// In-memory cache for fast access
const memoryBuffers = new Map<string, Uint8Array[]>();

// Encode Uint8Array to base64 for storage
function encodeChunk(chunk: Uint8Array): string {
  let binary = '';
  for (let i = 0; i < chunk.length; i++) {
    binary += String.fromCharCode(chunk[i]);
  }
  return btoa(binary);
}

// Decode base64 back to Uint8Array
function decodeChunk(base64: string): Uint8Array {
  const binary = atob(base64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) {
    bytes[i] = binary.charCodeAt(i);
  }
  return bytes;
}

// Load from localStorage into memory
function loadFromStorage(sessionId: string): Uint8Array[] {
  try {
    const stored = localStorage.getItem(STORAGE_KEY_PREFIX + sessionId);
    if (!stored) return [];

    const chunks: string[] = JSON.parse(stored);
    return chunks.map(decodeChunk);
  } catch (e) {
    console.warn('Failed to load terminal history from storage:', e);
    return [];
  }
}

// Save to localStorage (debounced by caller)
function saveToStorage(sessionId: string, chunks: Uint8Array[]) {
  try {
    // Only store recent chunks to stay within size limits
    const encoded = chunks.map(encodeChunk);
    let totalSize = encoded.reduce((sum, chunk) => sum + chunk.length, 0);

    // Trim from the beginning if too large
    while (totalSize > MAX_STORAGE_SIZE && encoded.length > 1) {
      const removed = encoded.shift();
      if (removed) {
        totalSize -= removed.length;
      }
    }

    localStorage.setItem(STORAGE_KEY_PREFIX + sessionId, JSON.stringify(encoded));
  } catch (e) {
    console.warn('Failed to save terminal history to storage:', e);
    // If quota exceeded, try clearing old data
    if (e instanceof DOMException && e.name === 'QuotaExceededError') {
      try {
        localStorage.removeItem(STORAGE_KEY_PREFIX + sessionId);
      } catch {
        // Ignore
      }
    }
  }
}

// Debounce save to avoid excessive writes
const saveDebounceTimers = new Map<string, number>();
function debouncedSave(sessionId: string, chunks: Uint8Array[]) {
  const existing = saveDebounceTimers.get(sessionId);
  if (existing) {
    clearTimeout(existing);
  }

  const timer = window.setTimeout(() => {
    saveToStorage(sessionId, chunks);
    saveDebounceTimers.delete(sessionId);
  }, 500);

  saveDebounceTimers.set(sessionId, timer);
}

export function appendTerminalOutput(sessionId: string, data: Uint8Array) {
  // Ensure we have a buffer (load from storage if needed)
  if (!memoryBuffers.has(sessionId)) {
    memoryBuffers.set(sessionId, loadFromStorage(sessionId));
  }

  const buffer = memoryBuffers.get(sessionId)!;

  // Create a copy of the data to store
  const copy = new Uint8Array(data.length);
  copy.set(data);
  buffer.push(copy);

  // Trim memory buffer if too large
  if (buffer.length > MAX_CHUNKS_MEMORY) {
    buffer.splice(0, buffer.length - MAX_CHUNKS_MEMORY);
  }

  // Debounced save to localStorage
  debouncedSave(sessionId, buffer);
}

export function getTerminalHistory(sessionId: string): Uint8Array[] {
  // Check memory first
  if (memoryBuffers.has(sessionId)) {
    return memoryBuffers.get(sessionId)!;
  }

  // Load from storage
  const history = loadFromStorage(sessionId);
  memoryBuffers.set(sessionId, history);
  return history;
}

export function clearTerminalHistory(sessionId: string) {
  memoryBuffers.delete(sessionId);
  try {
    localStorage.removeItem(STORAGE_KEY_PREFIX + sessionId);
  } catch {
    // Ignore
  }
}

export function hasTerminalHistory(sessionId: string): boolean {
  if (memoryBuffers.has(sessionId)) {
    const buffer = memoryBuffers.get(sessionId);
    return !!buffer && buffer.length > 0;
  }

  // Check storage
  try {
    const stored = localStorage.getItem(STORAGE_KEY_PREFIX + sessionId);
    return !!stored && stored !== '[]';
  } catch {
    return false;
  }
}

// Cleanup old sessions from storage (call periodically)
export function cleanupOldSessions(activeSessionIds: string[]) {
  try {
    const keysToRemove: string[] = [];

    for (let i = 0; i < localStorage.length; i++) {
      const key = localStorage.key(i);
      if (key?.startsWith(STORAGE_KEY_PREFIX)) {
        const sessionId = key.slice(STORAGE_KEY_PREFIX.length);
        if (!activeSessionIds.includes(sessionId)) {
          keysToRemove.push(key);
        }
      }
    }

    keysToRemove.forEach((key) => {
      localStorage.removeItem(key);
    });
  } catch {
    // Ignore cleanup errors
  }
}
