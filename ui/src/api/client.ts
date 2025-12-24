import type { 
  ApiResponse, 
  NodeConfig, 
  NodeHealth, 
  HermesConfig, 
  HermesHealth,
  EtlConfig,
  EtlHealth 
} from '@/types';

const BASE_URL = '';

async function fetchJSON<T>(url: string, options?: RequestInit): Promise<T> {
  const response = await fetch(`${BASE_URL}${url}`, {
    ...options,
    headers: {
      'Content-Type': 'application/json',
      ...options?.headers,
    },
  });
  
  if (!response.ok) {
    throw new Error(`HTTP error! status: ${response.status}`);
  }
  
  return response.json();
}

// Config endpoints
export async function fetchNodeConfigs(): Promise<Record<string, NodeConfig>> {
  const response = await fetchJSON<ApiResponse<{ nodes: Record<string, NodeConfig> }>>('/api/config/nodes');
  return response.success ? response.data.nodes : {};
}

export async function fetchHermesConfigs(): Promise<Record<string, HermesConfig>> {
  const response = await fetchJSON<ApiResponse<{ hermes: Record<string, HermesConfig> }>>('/api/config/hermes');
  return response.success ? response.data.hermes : {};
}

export async function fetchEtlConfigs(): Promise<Record<string, EtlConfig>> {
  const response = await fetchJSON<ApiResponse<{ etl: Record<string, EtlConfig> }>>('/api/config/etl');
  return response.success ? response.data.etl : {};
}

// Health endpoints
export async function fetchNodeHealth(includeDisabled = true): Promise<NodeHealth[]> {
  const response = await fetchJSON<ApiResponse<NodeHealth[]>>(
    `/api/health/nodes?include_disabled=${includeDisabled}`
  );
  return response.success ? response.data : [];
}

export async function fetchHermesHealth(): Promise<HermesHealth[]> {
  const response = await fetchJSON<ApiResponse<HermesHealth[]>>('/api/health/hermes');
  return response.success ? response.data : [];
}

export async function fetchEtlHealth(includeDisabled = true): Promise<EtlHealth[]> {
  const response = await fetchJSON<ApiResponse<EtlHealth[]>>(
    `/api/health/etl?include_disabled=${includeDisabled}`
  );
  return response.success ? response.data : [];
}

// Node operations
export async function pruneNode(nodeName: string): Promise<ApiResponse<unknown>> {
  return fetchJSON<ApiResponse<unknown>>(`/api/maintenance/nodes/${nodeName}/prune`, {
    method: 'POST',
  });
}

export async function restartNode(nodeName: string): Promise<ApiResponse<unknown>> {
  return fetchJSON<ApiResponse<unknown>>(`/api/maintenance/nodes/${nodeName}/restart`, {
    method: 'POST',
  });
}

export async function createSnapshot(nodeName: string): Promise<ApiResponse<unknown>> {
  return fetchJSON<ApiResponse<unknown>>(`/api/snapshots/${nodeName}/create`, {
    method: 'POST',
  });
}

export async function restoreSnapshot(nodeName: string): Promise<ApiResponse<unknown>> {
  return fetchJSON<ApiResponse<unknown>>(`/api/snapshots/${nodeName}/restore`, {
    method: 'POST',
  });
}

export async function executeStateSync(nodeName: string): Promise<ApiResponse<unknown>> {
  return fetchJSON<ApiResponse<unknown>>(`/api/state-sync/${nodeName}/execute`, {
    method: 'POST',
  });
}

// Hermes operations
export async function restartHermes(hermesName: string): Promise<ApiResponse<unknown>> {
  return fetchJSON<ApiResponse<unknown>>(`/api/maintenance/hermes/${hermesName}/restart`, {
    method: 'POST',
  });
}

// ETL operations
export async function refreshEtlService(serviceName: string): Promise<ApiResponse<unknown>> {
  return fetchJSON<ApiResponse<unknown>>(`/api/health/etl/${serviceName}`);
}

// Active operations
export interface ActiveOperation {
  id: string;
  operation_type: string;
  target_name: string;
  status: 'in_progress' | 'completed' | 'failed';
  started_at: string;
  completed_at?: string;
  error_message?: string;
}

export async function fetchActiveOperations(): Promise<ActiveOperation[]> {
  try {
    const response = await fetchJSON<ApiResponse<{ operations?: ActiveOperation[] }>>('/api/operations/active');
    if (response.success && response.data?.operations) {
      return response.data.operations;
    }
    return [];
  } catch {
    return [];
  }
}
