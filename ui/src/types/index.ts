// Node configuration from API
export interface NodeConfig {
  rpc_url: string;
  network: string;
  server_host: string;
  enabled: boolean;
  service_name: string;
  
  // Pruning
  pruning_enabled: boolean;
  pruning_schedule?: string;
  pruning_keep_blocks?: number;
  pruning_keep_versions?: number;
  
  // Snapshots
  snapshots_enabled: boolean;
  snapshot_schedule?: string;
  snapshot_retention_count?: number;
  auto_restore_enabled: boolean;
  
  // State sync
  state_sync_enabled: boolean;
  state_sync_schedule?: string;
  state_sync_rpc_sources?: string[];
  
  // Logs
  log_monitoring_enabled?: boolean;
  log_monitoring_patterns?: string[];
  truncate_logs_enabled?: boolean;
}

// Node health from API
export interface NodeHealth {
  node_name: string;
  server_host: string;
  status: string;
  latest_block_height: number | null;
  latest_block_time: string | null;
  catching_up: boolean;
  network: string;
  moniker: string | null;
  last_check: string;
  error_message: string | null;
}

// Hermes configuration
export interface HermesConfig {
  server_host: string;
  service_name: string;
  log_path: string;
  restart_schedule?: string;
  dependent_nodes: string[];
  truncate_logs_enabled?: boolean;
}

// Hermes health from API
export interface HermesHealth {
  name: string;
  server_host: string;
  status: string;
  uptime?: string;
  last_check: string;
}

// ETL configuration
export interface EtlConfig {
  server_host: string;
  host: string;
  port: number;
  endpoint?: string;
  enabled: boolean;
  timeout_seconds?: number;
  description?: string;
}

// ETL health from API
export interface EtlHealth {
  service_name: string;
  server_host: string;
  status: string;
  status_code?: number;
  response_time_ms?: number;
  service_url: string;
  last_check: string;
  error_message?: string;
  description?: string;
}

// API response wrapper
export interface ApiResponse<T> {
  success: boolean;
  data: T;
  message?: string;
}

// Sort configuration
export interface SortConfig {
  column: string;
  direction: 'asc' | 'desc';
}

// Node filter types
export type NodeFilter = 'all' | 'synced' | 'catching-up' | 'unhealthy' | 'maintenance';

// Status types for badges
export type NodeStatus = 'synced' | 'healthy' | 'catching up' | 'unhealthy' | 'maintenance' | 'unknown';
export type HermesStatus = 'running' | 'stopped' | 'failed' | 'unknown';
export type EtlStatus = 'healthy' | 'unhealthy' | 'unknown';
