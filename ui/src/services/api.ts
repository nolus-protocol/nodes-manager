/**
 * API Service for communicating with the Rust backend
 */

import type {
  NodeHealthResponse,
  HermesHealthResponse,
  EtlHealthResponse,
  NodesConfigResponse,
  HermesConfigResponse,
  EtlConfigResponse,
  ActiveOperationsResponse,
  MaintenanceHistoryResponse,
  SnapshotsResponse,
  ActionResponse,
  PruningRequest,
  SnapshotRequest,
  RestoreRequest,
  StateSyncRequest,
  HermesRestartRequest,
  NodeRestartRequest,
} from '@/types/api';

const API_BASE = '/api';

class ApiService {
  private async fetch<T>(endpoint: string, options?: RequestInit): Promise<T> {
    const response = await fetch(`${API_BASE}${endpoint}`, {
      headers: {
        'Content-Type': 'application/json',
        ...options?.headers,
      },
      ...options,
    });

    if (!response.ok) {
      throw new Error(`API error: ${response.statusText}`);
    }

    return response.json();
  }

  // ============================================================
  // Health Endpoints
  // ============================================================

  async getNodesHealth(includeDisabled = true): Promise<NodeHealthResponse> {
    return this.fetch<NodeHealthResponse>(
      `/health/nodes?include_disabled=${includeDisabled}`
    );
  }

  async getHermesHealth(): Promise<HermesHealthResponse> {
    return this.fetch<HermesHealthResponse>('/health/hermes');
  }

  async getEtlHealth(includeDisabled = true): Promise<EtlHealthResponse> {
    return this.fetch<EtlHealthResponse>(
      `/health/etl?include_disabled=${includeDisabled}`
    );
  }

  // ============================================================
  // Configuration Endpoints
  // ============================================================

  async getNodesConfig(): Promise<NodesConfigResponse> {
    return this.fetch<NodesConfigResponse>('/config/nodes');
  }

  async getHermesConfig(): Promise<HermesConfigResponse> {
    return this.fetch<HermesConfigResponse>('/config/hermes');
  }

  async getEtlConfig(): Promise<EtlConfigResponse> {
    return this.fetch<EtlConfigResponse>('/config/etl');
  }

  // ============================================================
  // Operations Endpoints
  // ============================================================

  async getActiveOperations(): Promise<ActiveOperationsResponse> {
    return this.fetch<ActiveOperationsResponse>('/operations/active');
  }

  async getMaintenanceHistory(
    nodeName?: string,
    limit = 100
  ): Promise<MaintenanceHistoryResponse> {
    const params = new URLSearchParams();
    if (nodeName) params.append('node_name', nodeName);
    params.append('limit', limit.toString());

    return this.fetch<MaintenanceHistoryResponse>(
      `/maintenance/history?${params}`
    );
  }

  // ============================================================
  // Snapshot Endpoints
  // ============================================================

  async getSnapshots(nodeName: string): Promise<SnapshotsResponse> {
    return this.fetch<SnapshotsResponse>(`/snapshots/list/${nodeName}`);
  }

  // ============================================================
  // Node Action Endpoints
  // ============================================================

  async pruneNode(request: PruningRequest): Promise<ActionResponse> {
    return this.fetch<ActionResponse>('/maintenance/prune', {
      method: 'POST',
      body: JSON.stringify(request),
    });
  }

  async createSnapshot(request: SnapshotRequest): Promise<ActionResponse> {
    return this.fetch<ActionResponse>('/snapshots/create', {
      method: 'POST',
      body: JSON.stringify(request),
    });
  }

  async restoreSnapshot(request: RestoreRequest): Promise<ActionResponse> {
    return this.fetch<ActionResponse>('/snapshots/restore', {
      method: 'POST',
      body: JSON.stringify(request),
    });
  }

  async stateSyncNode(request: StateSyncRequest): Promise<ActionResponse> {
    return this.fetch<ActionResponse>('/state-sync/execute', {
      method: 'POST',
      body: JSON.stringify(request),
    });
  }

  async restartNode(request: NodeRestartRequest): Promise<ActionResponse> {
    return this.fetch<ActionResponse>('/maintenance/restart', {
      method: 'POST',
      body: JSON.stringify(request),
    });
  }

  // ============================================================
  // Hermes Action Endpoints
  // ============================================================

  async restartHermes(request: HermesRestartRequest): Promise<ActionResponse> {
    return this.fetch<ActionResponse>('/hermes/restart', {
      method: 'POST',
      body: JSON.stringify(request),
    });
  }

  // ============================================================
  // ETL Action Endpoints
  // ============================================================

  async restartEtl(serviceName: string): Promise<ActionResponse> {
    return this.fetch<ActionResponse>('/etl/restart', {
      method: 'POST',
      body: JSON.stringify({ service_name: serviceName }),
    });
  }
}

export const api = new ApiService();
