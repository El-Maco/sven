import type { DeskState, SvenCommand, SvenInfo } from './types';

export class ApiError extends Error {
  status: number;

  constructor(message: string, status: number) {
    super(message);
    this.name = 'ApiError';
    this.status = status;
  }
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(path, {
    ...init,
    headers: {
      'Content-Type': 'application/json',
      ...init?.headers,
    },
    signal: AbortSignal.timeout(5000),
  });

  if (!response.ok) {
    const text = await response.text().catch(() => '');
    throw new ApiError(text || `HTTP ${response.status}`, response.status);
  }

  return (await response.json()) as T;
}

export function fetchState(): Promise<DeskState> {
  return request<DeskState>('/api/state');
}

export function fetchInfo(): Promise<SvenInfo> {
  return request<SvenInfo>('/api/info');
}

export function sendCommand(
  command: SvenCommand,
  value: number,
): Promise<{ accepted: boolean }> {
  return request<{ accepted: boolean }>('/api/command', {
    method: 'POST',
    body: JSON.stringify({ command, value }),
  });
}

export function stopDesk(): Promise<{ accepted: boolean }> {
  return request<{ accepted: boolean }>('/api/stop', {
    method: 'POST',
  });
}
