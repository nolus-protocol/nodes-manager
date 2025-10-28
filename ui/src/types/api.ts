/**
 * TypeScript types matching the Rust backend API
 */

// ============================================================
// Node Health Types
// ============================================================

export interface NodeHealth {
  node_name: string;
  server_host: string;
  status: 'Synced' | 'Catching Up' | 'Unhealthy' | 'Maintenance';
  network: string;
  latest_block_height?: number;
  latest_block_time?: string;
  catching_up: boolean;
  validator_address?: string;
  moniker?: string;
  last_check_time: string;
  is_enabled: boolean;
  // Enhanced fields
  maintenance_info?: {
    operation_type: string;
    started_at: string;
    estimated_duration_minutes: number;
    elapsed_minutes: number;
  };
  auto_restore_enabled?: boolean;
  snapshot_enabled?: boolean;
  scheduled_snapshots_enabled?: boolean;
  snapshot_retention_count?: number;
}

export interface NodeHealthResponse {
  success: boolean;
  data: NodeHealth[];
  message?: string;
}

// ============================================================
// Hermes Types
// ============================================================

export interface HermesHealth {
  hermes_name: string;
  server_host: string;
  status: string; // e.g., "Running (5d 3h)", "Stopped", "Failed"
  is_active: boolean;
  uptime_seconds?: number;
  last_check_time: string;
  is_enabled: boolean;
}

export interface HermesHealthResponse {
  success: boolean;
  data: HermesHealth[];
  message?: string;
}

// ============================================================
// ETL Service Types
// ============================================================

export interface EtlHealth {
  service_name: string;
  server_host: string;
  status: 'Healthy' | 'Unhealthy';
  url: string;
  http_status?: number;
  response_time_ms?: number;
  last_check_time: string;
  is_enabled: boolean;
  description?: string;
}

export interface EtlHealthResponse {
  success: boolean;
  data: EtlHealth[];
  message?: string;
}

// ============================================================
// Configuration Types
// ============================================================

export interface NodeConfig {
  service_name: string;
  rpc_url: string;
  network: string;
  deploy_path?: string;
  log_path?: string;
  snapshot_backup_path?: string;
  pruning_enabled?: boolean;
  pruning_cron?: string;
  snapshot_enabled?: boolean;
  snapshot_cron?: string;
  snapshot_retention_count?: number;
  auto_restore_enabled?: boolean;
  state_sync_rpc_servers?: string[];
  is_enabled?: boolean;
}

export interface HermesConfig {
  service_name: string;
  config_path: string;
  dependent_nodes: string[];
  restart_cron?: string;
  min_uptime_minutes?: number;
  is_enabled?: boolean;
}

export interface EtlConfig {
  server_host: string;
  host: string;
  port: number;
  endpoint?: string;
  enabled: boolean;
  timeout_seconds?: number;
  description?: string;
}

export interface ServerConfig {
  host: string;
  agent_port: number;
  api_key: string;
  request_timeout_seconds?: number;
  max_concurrent_requests?: number;
}

export interface NodesConfigResponse {
  success: boolean;
  data: {
    nodes: Record<string, NodeConfig>;
  };
  message?: string;
}

export interface HermesConfigResponse {
  success: boolean;
  data: {
    hermes: Record<string, HermesConfig>;
  };
  message?: string;
}

export interface EtlConfigResponse {
  success: boolean;
  data: {
    etl: Record<string, EtlConfig>;
  };
  message?: string;
}

// ============================================================
// Operations Types
// ============================================================

export interface ActiveOperation {
  node_name: string;
  server_host: string;
  operation_type: string; // e.g., "Pruning", "Snapshot", "Restore", "StateSync"
  start_time: string;
  estimated_completion?: string;
}

export interface ActiveOperationsResponse {
  success: boolean;
  data: ActiveOperation[];
  message?: string;
}

// ============================================================
// Maintenance Types
// ============================================================

export interface MaintenanceOperation {
  id: number;
  node_name: string;
  server_host: string;
  operation_type: string;
  start_time: string;
  end_time?: string;
  success: boolean;
  error_message?: string;
}

export interface MaintenanceHistoryResponse {
  success: boolean;
  data: MaintenanceOperation[];
  message?: string;
}

// ============================================================
// Snapshot Types
// ============================================================

export interface Snapshot {
  name: string;
  network: string;
  date: string;
  block_height: string;
  size_mb?: number;
  created_at: string;
}

export interface SnapshotsResponse {
  success: boolean;
  data: Snapshot[];
  message?: string;
}

// ============================================================
// Action Request/Response Types
// ============================================================

export interface ActionResponse {
  success: boolean;
  message: string;
  job_id?: string;
}

export interface PruningRequest {
  node_name: string;
}

export interface SnapshotRequest {
  node_name: string;
}

export interface RestoreRequest {
  node_name: string;
  snapshot_name?: string; // Optional: if not provided, uses latest
}

export interface StateSyncRequest {
  node_name: string;
  rpc_server?: string; // Optional: if not provided, uses first configured RPC
}

export interface HermesRestartRequest {
  hermes_name: string;
}

export interface NodeRestartRequest {
  node_name: string;
}

// ============================================================
// Metrics Types
// ============================================================

export interface DashboardMetrics {
  total_components: number;
  healthy_components: number;
  total_nodes: number;
  healthy_nodes: number;
  total_hermes: number;
  active_hermes: number;
  total_etl: number;
  healthy_etl: number;
  total_servers: number;
  health_percentage: number;
}
