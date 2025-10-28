<template>
  <div class="dashboard">
    <!-- Metrics Overview -->
    <div class="metrics-grid">
      <div class="metric-card">
        <div class="metric-header">
          <span class="metric-label">Total Components</span>
          <SvgIcon name="server" size="20" />
        </div>
        <div class="metric-value">{{ metrics.total_components }}</div>
        <div class="metric-subtitle">Nodes, Relayers & ETL</div>
      </div>

      <div class="metric-card">
        <div class="metric-header">
          <span class="metric-label">Operational</span>
          <SvgIcon name="check-circle" size="20" />
        </div>
        <div class="metric-value">{{ metrics.healthy_components }}</div>
        <div class="metric-subtitle">{{ metrics.health_percentage }}% healthy</div>
        <div class="metric-progress">
          <div 
            class="metric-progress-bar" 
            :style="{ width: `${metrics.health_percentage}%` }"
          />
        </div>
      </div>

      <div class="metric-card">
        <div class="metric-header">
          <span class="metric-label">Blockchain Nodes</span>
          <SvgIcon name="database" size="20" />
        </div>
        <div class="metric-value">{{ metrics.healthy_nodes }}/{{ metrics.total_nodes }}</div>
        <div class="metric-subtitle">Synced nodes</div>
      </div>

      <div class="metric-card">
        <div class="metric-header">
          <span class="metric-label">Servers</span>
          <SvgIcon name="server" size="20" />
        </div>
        <div class="metric-value">{{ metrics.total_servers }}</div>
        <div class="metric-subtitle">Infrastructure servers</div>
      </div>
    </div>

    <!-- Blockchain Nodes Section -->
    <div class="section-card">
      <div class="section-header">
        <h2>
          <button class="collapse-btn" @click="toggleCollapse('nodes')" :title="collapsed.nodes ? 'Expand' : 'Collapse'">
            <SvgIcon :name="collapsed.nodes ? 'chevron-right' : 'chevron-down'" size="20" />
          </button>
          Blockchain Nodes
        </h2>
        <div class="section-actions">
          <Input 
            v-model="searchNodes"
            placeholder="Search nodes..."
            type="text"
            class="search-input"
          />
          <Button @click="refreshNodes" :loading="loadingNodes" variant="secondary" size="sm">
            <SvgIcon name="refresh" size="16" />
            Refresh
          </Button>
        </div>
      </div>

      <!-- Status Filters -->
      <div v-if="!collapsed.nodes" class="section-filters">
        <span class="filter-label">Filter:</span>
        <div class="filter-badges">
          <Badge 
            :variant="nodeFilter === 'all' ? 'default' : 'default'"
            class="filter-badge"
            :class="{ active: nodeFilter === 'all' }"
            @click="setNodeFilter('all')"
          >
            All ({{ nodes.length }})
          </Badge>
          <Badge 
            variant="success"
            class="filter-badge"
            :class="{ active: nodeFilter === 'synced' }"
            @click="setNodeFilter('synced')"
          >
            Synced ({{ nodes.filter(n => n.status === 'Synced').length }})
          </Badge>
          <Badge 
            variant="warning"
            class="filter-badge"
            :class="{ active: nodeFilter === 'catching-up' }"
            @click="setNodeFilter('catching-up')"
          >
            Catching Up ({{ nodes.filter(n => n.status === 'Catching Up').length }})
          </Badge>
          <Badge 
            variant="error"
            class="filter-badge"
            :class="{ active: nodeFilter === 'unhealthy' }"
            @click="setNodeFilter('unhealthy')"
          >
            Unhealthy ({{ nodes.filter(n => n.status === 'Unhealthy').length }})
          </Badge>
          <Badge 
            variant="warning"
            class="filter-badge"
            :class="{ active: nodeFilter === 'maintenance' }"
            @click="setNodeFilter('maintenance')"
          >
            Maintenance ({{ nodes.filter(n => n.status === 'Maintenance').length }})
          </Badge>
        </div>
      </div>
      
      <div v-if="!collapsed.nodes">
        <div v-if="loadingNodes && !nodes.length" class="loading-state">
          <Spinner size="lg" />
          <p>Loading blockchain nodes...</p>
        </div>

        <div v-else-if="filteredAndSortedNodes.length === 0" class="empty-state">
          <p>No nodes found</p>
        </div>

        <div v-else class="table-container">
          <table class="data-table">
            <thead>
              <tr>
                <th @click="sortNodes('node_name')" class="sortable">
                  Node
                  <span class="sort-icon">{{ getSortIcon('nodes', 'node_name') }}</span>
                </th>
                <th @click="sortNodes('status')" class="sortable">
                  Status
                  <span class="sort-icon">{{ getSortIcon('nodes', 'status') }}</span>
                </th>
                <th @click="sortNodes('latest_block_height')" class="sortable">
                  Block Height
                  <span class="sort-icon">{{ getSortIcon('nodes', 'latest_block_height') }}</span>
                </th>
                <th>Network</th>
                <th>Server</th>
                <th>Schedule</th>
                <th>Actions</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="node in filteredAndSortedNodes" :key="node.node_name">
                <td>
                  <div class="node-info">
                    <span class="node-name">{{ node.node_name }}</span>
                    <span v-if="node.moniker" class="node-moniker">{{ node.moniker }}</span>
                    <!-- Auto-restore indicator -->
                    <Badge v-if="node.auto_restore_enabled" variant="success" size="sm" class="feature-badge">
                      <SvgIcon name="shield" size="12" />
                      Auto-Restore
                    </Badge>
                  </div>
                </td>
                <td>
                  <div class="status-cell">
                    <Badge :variant="getStatusVariant(node.status)">
                      {{ node.status }}
                    </Badge>
                    <!-- Maintenance status -->
                    <div v-if="node.maintenance_info" class="maintenance-status">
                      <Badge variant="warning" size="sm">
                        <SvgIcon name="cogwheel" size="12" />
                        {{ node.maintenance_info.operation_type }}
                      </Badge>
                      <span class="maintenance-time">{{ node.maintenance_info.elapsed_minutes }}m / {{ node.maintenance_info.estimated_duration_minutes }}m</span>
                    </div>
                  </div>
                </td>
                <td>
                  <span v-if="node.latest_block_height">
                    {{ node.latest_block_height.toLocaleString() }}
                  </span>
                  <span v-else class="text-muted">-</span>
                </td>
                <td>{{ node.network }}</td>
                <td>{{ node.server_host }}</td>
                <td>
                  <div class="schedule-info">
                    <div v-if="getNodeSchedule(node.node_name)?.snapshot" class="schedule-item">
                      <SvgIcon name="camera" size="12" />
                      <span class="schedule-text">{{ formatNextRun(getNodeSchedule(node.node_name)?.snapshot) }}</span>
                    </div>
                    <div v-if="getNodeSchedule(node.node_name)?.pruning" class="schedule-item">
                      <SvgIcon name="trash" size="12" />
                      <span class="schedule-text">{{ formatNextRun(getNodeSchedule(node.node_name)?.pruning) }}</span>
                    </div>
                    <span v-if="!getNodeSchedule(node.node_name)?.snapshot && !getNodeSchedule(node.node_name)?.pruning" class="text-muted">No schedule</span>
                  </div>
                </td>
                <td>
                  <div class="action-buttons">
                    <Button @click="restartNode(node.node_name)" size="sm" variant="secondary">
                      Restart
                    </Button>
                    <Dropdown>
                      <template #trigger>
                        <Button size="sm" variant="secondary">
                          Actions
                          <SvgIcon name="chevron-down" size="14" />
                        </Button>
                      </template>
                      <template #content>
                        <button @click="pruneNode(node.node_name)" class="dropdown-item">
                          <SvgIcon name="trash" size="14" />
                          Prune
                        </button>
                        <button @click="createSnapshot(node.node_name)" class="dropdown-item">
                          <SvgIcon name="camera" size="14" />
                          Snapshot
                        </button>
                        <button @click="restoreSnapshot(node.node_name)" class="dropdown-item">
                          <SvgIcon name="refresh" size="14" />
                          Restore
                        </button>
                        <button @click="stateSync(node.node_name)" class="dropdown-item">
                          <SvgIcon name="sync" size="14" />
                          State Sync
                        </button>
                      </template>
                    </Dropdown>
                  </div>
                </td>
              </tr>
            </tbody>
          </table>
        </div>
      </div>
    </div>

    <!-- Hermes Relayers Section -->
    <div class="section-card">
      <div class="section-header">
        <h2>
          <button class="collapse-btn" @click="toggleCollapse('hermes')" :title="collapsed.hermes ? 'Expand' : 'Collapse'">
            <SvgIcon :name="collapsed.hermes ? 'chevron-right' : 'chevron-down'" size="20" />
          </button>
          Hermes Relayers
        </h2>
        <div class="section-actions">
          <Input 
            v-model="searchHermes"
            placeholder="Search relayers..."
            type="text"
            class="search-input"
          />
          <Button @click="refreshHermes" :loading="loadingHermes" variant="secondary" size="sm">
            <SvgIcon name="refresh" size="16" />
            Refresh
          </Button>
        </div>
      </div>

      <div v-if="!collapsed.hermes">
        <div v-if="loadingHermes && !hermes.length" class="loading-state">
          <Spinner size="lg" />
          <p>Loading Hermes relayers...</p>
        </div>

        <div v-else-if="filteredAndSortedHermes.length === 0" class="empty-state">
          <p>No relayers found</p>
        </div>

        <div v-else class="table-container">
          <table class="data-table">
            <thead>
              <tr>
                <th @click="sortHermes('hermes_name')" class="sortable">
                  Relayer
                  <span class="sort-icon">{{ getSortIcon('hermes', 'hermes_name') }}</span>
                </th>
                <th @click="sortHermes('status')" class="sortable">
                  Status
                  <span class="sort-icon">{{ getSortIcon('hermes', 'status') }}</span>
                </th>
                <th @click="sortHermes('uptime_seconds')" class="sortable">
                  Uptime
                  <span class="sort-icon">{{ getSortIcon('hermes', 'uptime_seconds') }}</span>
                </th>
                <th>Server</th>
                <th>Schedule</th>
                <th>Actions</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="h in filteredAndSortedHermes" :key="h.hermes_name">
                <td>{{ h.hermes_name }}</td>
                <td>
                  <Badge :variant="h.is_active ? 'success' : 'error'">
                    {{ h.status }}
                  </Badge>
                </td>
                <td>
                  <span v-if="h.uptime_seconds">
                    {{ formatUptime(h.uptime_seconds) }}
                  </span>
                  <span v-else class="text-muted">-</span>
                </td>
                <td>{{ h.server_host }}</td>
                <td>
                  <div class="schedule-info">
                    <div v-if="getHermesSchedule(h.hermes_name)" class="schedule-item">
                      <SvgIcon name="refresh" size="12" />
                      <span class="schedule-text">{{ formatNextRun(getHermesSchedule(h.hermes_name)) }}</span>
                    </div>
                    <span v-else class="text-muted">No schedule</span>
                  </div>
                </td>
                <td>
                  <Button @click="restartHermes(h.hermes_name)" size="sm" variant="secondary">
                    Restart
                  </Button>
                </td>
              </tr>
            </tbody>
          </table>
        </div>
      </div>
    </div>

    <!-- ETL Services Section -->
    <div class="section-card">
      <div class="section-header">
        <h2>
          <button class="collapse-btn" @click="toggleCollapse('etl')" :title="collapsed.etl ? 'Expand' : 'Collapse'">
            <SvgIcon :name="collapsed.etl ? 'chevron-right' : 'chevron-down'" size="20" />
          </button>
          ETL Services
        </h2>
        <div class="section-actions">
          <Input 
            v-model="searchEtl"
            placeholder="Search services..."
            type="text"
            class="search-input"
          />
          <Button @click="refreshEtl" :loading="loadingEtl" variant="secondary" size="sm">
            <SvgIcon name="refresh" size="16" />
            Refresh
          </Button>
        </div>
      </div>

      <div v-if="!collapsed.etl">
        <div v-if="loadingEtl && !etl.length" class="loading-state">
          <Spinner size="lg" />
          <p>Loading ETL services...</p>
        </div>

        <div v-else-if="filteredAndSortedEtl.length === 0" class="empty-state">
          <p>No services found</p>
        </div>

        <div v-else class="table-container">
          <table class="data-table">
            <thead>
              <tr>
                <th @click="sortEtl('service_name')" class="sortable">
                  Service
                  <span class="sort-icon">{{ getSortIcon('etl', 'service_name') }}</span>
                </th>
                <th @click="sortEtl('status')" class="sortable">
                  Status
                  <span class="sort-icon">{{ getSortIcon('etl', 'status') }}</span>
                </th>
                <th @click="sortEtl('response_time_ms')" class="sortable">
                  Response Time
                  <span class="sort-icon">{{ getSortIcon('etl', 'response_time_ms') }}</span>
                </th>
                <th>URL</th>
                <th>Actions</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="e in filteredAndSortedEtl" :key="e.service_name">
                <td>
                  <div class="service-info">
                    <span class="service-name">{{ e.service_name }}</span>
                    <span v-if="e.description" class="service-desc">{{ e.description }}</span>
                  </div>
                </td>
                <td>
                  <Badge :variant="e.status === 'Healthy' ? 'success' : 'error'">
                    {{ e.status }}
                  </Badge>
                </td>
                <td>
                  <span v-if="e.response_time_ms">
                    {{ e.response_time_ms }}ms
                  </span>
                  <span v-else class="text-muted">-</span>
                </td>
                <td>
                  <a :href="e.url" target="_blank" class="url-link">{{ e.url }}</a>
                </td>
                <td>
                  <Button @click="restartEtl(e.service_name)" size="sm" variant="secondary">
                    Restart
                  </Button>
                </td>
              </tr>
            </tbody>
          </table>
        </div>
      </div>
    </div>

    <!-- Toast notifications -->
    <Toast 
      v-if="toast.show" 
      :variant="toast.variant" 
      :message="toast.message"
      @close="toast.show = false"
    />
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, watch } from 'vue'
import { 
  Button, 
  Badge, 
  Input, 
  Dropdown, 
  Spinner,
  Toast,
  SvgIcon
} from 'web-components'
import { api } from '@/services/api'
import type { NodeHealth, HermesHealth, EtlHealth, DashboardMetrics } from '@/types/api'

// Extended types for enhanced features
interface EnhancedNodeHealth extends NodeHealth {
  maintenance_info?: {
    operation_type: string
    started_at: string
    estimated_duration_minutes: number
    elapsed_minutes: number
  }
  auto_restore_enabled?: boolean
  snapshot_enabled?: boolean
  scheduled_snapshots_enabled?: boolean
  snapshot_retention_count?: number
}

interface NodeConfig {
  pruning_cron?: string
  snapshot_cron?: string
}

interface HermesConfig {
  restart_cron?: string
}

// State
const nodes = ref<EnhancedNodeHealth[]>([])
const hermes = ref<HermesHealth[]>([])
const etl = ref<EtlHealth[]>([])
const nodesConfig = ref<Record<string, NodeConfig>>({})
const hermesConfig = ref<Record<string, HermesConfig>>({})

const loadingNodes = ref(false)
const loadingHermes = ref(false)
const loadingEtl = ref(false)

// Search & Filter state (with localStorage persistence)
const searchNodes = ref(localStorage.getItem('searchNodes') || '')
const searchHermes = ref(localStorage.getItem('searchHermes') || '')
const searchEtl = ref(localStorage.getItem('searchEtl') || '')
const nodeFilter = ref(localStorage.getItem('nodeFilter') || 'all')

// Collapse state (with localStorage persistence)
const collapsed = ref({
  nodes: localStorage.getItem('collapsed.nodes') === 'true',
  hermes: localStorage.getItem('collapsed.hermes') === 'true',
  etl: localStorage.getItem('collapsed.etl') === 'true',
})

// Sorting state (with localStorage persistence)
interface SortState {
  column: string
  direction: 'asc' | 'desc'
}

const sortState = ref<Record<string, SortState>>({
  nodes: JSON.parse(localStorage.getItem('sortNodes') || '{"column":"node_name","direction":"asc"}'),
  hermes: JSON.parse(localStorage.getItem('sortHermes') || '{"column":"hermes_name","direction":"asc"}'),
  etl: JSON.parse(localStorage.getItem('sortEtl') || '{"column":"service_name","direction":"asc"}'),
})

const toast = ref({
  show: false,
  variant: 'success' as 'success' | 'error' | 'warning',
  message: ''
})

// Watch for changes and persist to localStorage
watch(searchNodes, (val) => localStorage.setItem('searchNodes', val))
watch(searchHermes, (val) => localStorage.setItem('searchHermes', val))
watch(searchEtl, (val) => localStorage.setItem('searchEtl', val))
watch(nodeFilter, (val) => localStorage.setItem('nodeFilter', val))
watch(() => collapsed.value.nodes, (val) => localStorage.setItem('collapsed.nodes', String(val)))
watch(() => collapsed.value.hermes, (val) => localStorage.setItem('collapsed.hermes', String(val)))
watch(() => collapsed.value.etl, (val) => localStorage.setItem('collapsed.etl', String(val)))
watch(() => sortState.value.nodes, (val) => localStorage.setItem('sortNodes', JSON.stringify(val)), { deep: true })
watch(() => sortState.value.hermes, (val) => localStorage.setItem('sortHermes', JSON.stringify(val)), { deep: true })
watch(() => sortState.value.etl, (val) => localStorage.setItem('sortEtl', JSON.stringify(val)), { deep: true })

// Computed
const metrics = computed<DashboardMetrics>(() => {
  const totalNodes = nodes.value.length
  const healthyNodes = nodes.value.filter(n => n.status === 'Synced').length
  const totalHermes = hermes.value.length
  const activeHermes = hermes.value.filter(h => h.is_active).length
  const totalEtl = etl.value.length
  const healthyEtl = etl.value.filter(e => e.status === 'Healthy').length
  
  const totalComponents = totalNodes + totalHermes + totalEtl
  const healthyComponents = healthyNodes + activeHermes + healthyEtl
  const healthPercentage = totalComponents > 0 
    ? Math.round((healthyComponents / totalComponents) * 100) 
    : 0

  const servers = new Set([
    ...nodes.value.map(n => n.server_host),
    ...hermes.value.map(h => h.server_host),
  ])

  return {
    total_components: totalComponents,
    healthy_components: healthyComponents,
    total_nodes: totalNodes,
    healthy_nodes: healthyNodes,
    total_hermes: totalHermes,
    active_hermes: activeHermes,
    total_etl: totalEtl,
    healthy_etl: healthyEtl,
    total_servers: servers.size,
    health_percentage: healthPercentage
  }
})

const filteredNodes = computed(() => {
  let filtered = nodes.value

  // Apply status filter
  if (nodeFilter.value !== 'all') {
    const filterMap: Record<string, string> = {
      'synced': 'Synced',
      'catching-up': 'Catching Up',
      'unhealthy': 'Unhealthy',
      'maintenance': 'Maintenance'
    }
    filtered = filtered.filter(n => n.status === filterMap[nodeFilter.value])
  }

  // Apply search
  if (searchNodes.value) {
    const search = searchNodes.value.toLowerCase()
    filtered = filtered.filter(n => 
      n.node_name.toLowerCase().includes(search) ||
      n.server_host.toLowerCase().includes(search) ||
      n.network.toLowerCase().includes(search)
    )
  }

  return filtered
})

const filteredAndSortedNodes = computed(() => {
  return sortData(filteredNodes.value, sortState.value.nodes)
})

const filteredHermes = computed(() => {
  if (!searchHermes.value) return hermes.value
  const search = searchHermes.value.toLowerCase()
  return hermes.value.filter(h => 
    h.hermes_name.toLowerCase().includes(search) ||
    h.server_host.toLowerCase().includes(search)
  )
})

const filteredAndSortedHermes = computed(() => {
  return sortData(filteredHermes.value, sortState.value.hermes)
})

const filteredEtl = computed(() => {
  if (!searchEtl.value) return etl.value
  const search = searchEtl.value.toLowerCase()
  return etl.value.filter(e => 
    e.service_name.toLowerCase().includes(search) ||
    e.url.toLowerCase().includes(search)
  )
})

const filteredAndSortedEtl = computed(() => {
  return sortData(filteredEtl.value, sortState.value.etl)
})

// Methods
function toggleCollapse(section: 'nodes' | 'hermes' | 'etl') {
  collapsed.value[section] = !collapsed.value[section]
}

function setNodeFilter(filter: string) {
  nodeFilter.value = filter
}

function sortNodes(column: string) {
  if (sortState.value.nodes.column === column) {
    sortState.value.nodes.direction = sortState.value.nodes.direction === 'asc' ? 'desc' : 'asc'
  } else {
    sortState.value.nodes = { column, direction: 'asc' }
  }
}

function sortHermes(column: string) {
  if (sortState.value.hermes.column === column) {
    sortState.value.hermes.direction = sortState.value.hermes.direction === 'asc' ? 'desc' : 'asc'
  } else {
    sortState.value.hermes = { column, direction: 'asc' }
  }
}

function sortEtl(column: string) {
  if (sortState.value.etl.column === column) {
    sortState.value.etl.direction = sortState.value.etl.direction === 'asc' ? 'desc' : 'asc'
  } else {
    sortState.value.etl = { column, direction: 'asc' }
  }
}

function sortData<T extends Record<string, any>>(data: T[], sort: SortState): T[] {
  return [...data].sort((a, b) => {
    const aVal = a[sort.column]
    const bVal = b[sort.column]
    
    if (aVal === undefined || aVal === null) return 1
    if (bVal === undefined || bVal === null) return -1
    
    let comparison = 0
    if (typeof aVal === 'string' && typeof bVal === 'string') {
      comparison = aVal.localeCompare(bVal)
    } else if (typeof aVal === 'number' && typeof bVal === 'number') {
      comparison = aVal - bVal
    }
    
    return sort.direction === 'asc' ? comparison : -comparison
  })
}

function getSortIcon(table: string, column: string): string {
  const sort = sortState.value[table]
  if (sort.column !== column) return '⇅'
  return sort.direction === 'asc' ? '↑' : '↓'
}

function getStatusVariant(status: string): 'success' | 'warning' | 'error' | 'default' {
  switch (status) {
    case 'Synced': return 'success'
    case 'Catching Up': return 'warning'
    case 'Unhealthy': return 'error'
    case 'Maintenance': return 'warning'
    default: return 'default'
  }
}

function formatUptime(seconds: number): string {
  const days = Math.floor(seconds / 86400)
  const hours = Math.floor((seconds % 86400) / 3600)
  const minutes = Math.floor((seconds % 3600) / 60)
  
  if (days > 0) return `${days}d ${hours}h`
  if (hours > 0) return `${hours}h ${minutes}m`
  return `${minutes}m`
}

function getNodeSchedule(nodeName: string): { snapshot?: string; pruning?: string } | null {
  const config = nodesConfig.value[nodeName]
  if (!config) return null
  
  return {
    snapshot: config.snapshot_cron,
    pruning: config.pruning_cron
  }
}

function getHermesSchedule(hermesName: string): string | null {
  return hermesConfig.value[hermesName]?.restart_cron || null
}

function formatNextRun(cron: string | undefined): string {
  if (!cron) return 'Not scheduled'
  // For now, just show the cron expression
  // In a full implementation, you'd parse the cron and calculate next run
  return cron
}

function showToast(message: string, variant: 'success' | 'error' | 'warning' = 'success') {
  toast.value = { show: true, message, variant }
  setTimeout(() => { toast.value.show = false }, 5000)
}

async function refreshNodes() {
  loadingNodes.value = true
  try {
    const response = await api.getNodesHealth()
    if (response.success) {
      nodes.value = response.data as EnhancedNodeHealth[]
    }
  } catch (error) {
    console.error('Failed to load nodes:', error)
    showToast('Failed to load nodes', 'error')
  } finally {
    loadingNodes.value = false
  }
}

async function refreshHermes() {
  loadingHermes.value = true
  try {
    const response = await api.getHermesHealth()
    if (response.success) {
      hermes.value = response.data
    }
  } catch (error) {
    console.error('Failed to load Hermes:', error)
    showToast('Failed to load Hermes relayers', 'error')
  } finally {
    loadingHermes.value = false
  }
}

async function refreshEtl() {
  loadingEtl.value = true
  try {
    const response = await api.getEtlHealth()
    if (response.success) {
      etl.value = response.data
    }
  } catch (error) {
    console.error('Failed to load ETL:', error)
    showToast('Failed to load ETL services', 'error')
  } finally {
    loadingEtl.value = false
  }
}

async function loadConfigs() {
  try {
    const [nodesResp, hermesResp] = await Promise.all([
      api.getNodesConfig(),
      api.getHermesConfig()
    ])
    if (nodesResp.success) {
      nodesConfig.value = nodesResp.data.nodes
    }
    if (hermesResp.success) {
      hermesConfig.value = hermesResp.data.hermes
    }
  } catch (error) {
    console.error('Failed to load configs:', error)
  }
}

async function restartNode(nodeName: string) {
  try {
    const response = await api.restartNode({ node_name: nodeName })
    showToast(response.message, response.success ? 'success' : 'error')
    if (response.success) {
      setTimeout(refreshNodes, 2000)
    }
  } catch (error) {
    showToast('Failed to restart node', 'error')
  }
}

async function pruneNode(nodeName: string) {
  try {
    const response = await api.pruneNode({ node_name: nodeName })
    showToast(response.message, response.success ? 'success' : 'error')
  } catch (error) {
    showToast('Failed to prune node', 'error')
  }
}

async function createSnapshot(nodeName: string) {
  try {
    const response = await api.createSnapshot({ node_name: nodeName })
    showToast(response.message, response.success ? 'success' : 'error')
  } catch (error) {
    showToast('Failed to create snapshot', 'error')
  }
}

async function restoreSnapshot(nodeName: string) {
  try {
    const response = await api.restoreSnapshot({ node_name: nodeName })
    showToast(response.message, response.success ? 'success' : 'error')
  } catch (error) {
    showToast('Failed to restore snapshot', 'error')
  }
}

async function stateSync(nodeName: string) {
  try {
    const response = await api.stateSyncNode({ node_name: nodeName })
    showToast(response.message, response.success ? 'success' : 'error')
  } catch (error) {
    showToast('Failed to start state sync', 'error')
  }
}

async function restartHermes(hermesName: string) {
  try {
    const response = await api.restartHermes({ hermes_name: hermesName })
    showToast(response.message, response.success ? 'success' : 'error')
    if (response.success) {
      setTimeout(refreshHermes, 2000)
    }
  } catch (error) {
    showToast('Failed to restart Hermes', 'error')
  }
}

async function restartEtl(serviceName: string) {
  try {
    const response = await api.restartEtl(serviceName)
    showToast(response.message, response.success ? 'success' : 'error')
    if (response.success) {
      setTimeout(refreshEtl, 2000)
    }
  } catch (error) {
    showToast('Failed to restart ETL service', 'error')
  }
}

// Lifecycle
onMounted(() => {
  refreshNodes()
  refreshHermes()
  refreshEtl()
  loadConfigs()
  
  // Auto-refresh every 30 seconds
  setInterval(() => {
    refreshNodes()
    refreshHermes()
    refreshEtl()
  }, 30000)
})
</script>

<style scoped>
.dashboard {
  display: flex;
  flex-direction: column;
  gap: 2rem;
}

.metrics-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(250px, 1fr));
  gap: 1.5rem;
}

.metric-card {
  background: var(--bg-secondary);
  border: 1px solid var(--border-color);
  border-radius: 0.5rem;
  padding: 1.5rem;
  box-shadow: 0 1px 2px 0 rgba(0, 0, 0, 0.05);
}

.metric-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 1rem;
}

.metric-label {
  font-size: 0.75rem;
  font-weight: 600;
  color: var(--text-secondary);
  text-transform: uppercase;
  letter-spacing: 0.05em;
}

.metric-value {
  font-size: 2rem;
  font-weight: 700;
  color: var(--text-primary);
  margin-bottom: 0.5rem;
}

.metric-subtitle {
  font-size: 0.875rem;
  color: var(--text-muted);
}

.metric-progress {
  margin-top: 1rem;
  height: 4px;
  background: var(--bg-tertiary);
  border-radius: 2px;
  overflow: hidden;
}

.metric-progress-bar {
  height: 100%;
  background: var(--success-color);
  transition: width 0.5s ease;
}

.section-card {
  background: var(--bg-secondary);
  border: 1px solid var(--border-color);
  border-radius: 0.5rem;
  overflow: hidden;
  box-shadow: 0 1px 2px 0 rgba(0, 0, 0, 0.05);
}

.section-header {
  padding: 1.25rem 1.5rem;
  border-bottom: 1px solid var(--border-color);
  background: var(--bg-tertiary);
  display: flex;
  justify-content: space-between;
  align-items: center;
  flex-wrap: wrap;
  gap: 1rem;
}

.section-header h2 {
  font-size: 1.125rem;
  font-weight: 600;
  color: var(--text-primary);
  display: flex;
  align-items: center;
  gap: 0.5rem;
}

.collapse-btn {
  background: none;
  border: none;
  padding: 0.25rem;
  cursor: pointer;
  color: var(--text-secondary);
  display: flex;
  align-items: center;
  border-radius: 0.25rem;
  transition: all 0.2s ease;
}

.collapse-btn:hover {
  background: var(--bg-secondary);
  color: var(--text-primary);
}

.section-actions {
  display: flex;
  gap: 0.75rem;
  align-items: center;
}

.section-filters {
  padding: 1rem 1.5rem;
  border-bottom: 1px solid var(--border-color);
  display: flex;
  align-items: center;
  gap: 1rem;
  flex-wrap: wrap;
}

.filter-label {
  font-size: 0.875rem;
  font-weight: 600;
  color: var(--text-secondary);
}

.filter-badges {
  display: flex;
  gap: 0.5rem;
  flex-wrap: wrap;
}

.filter-badge {
  cursor: pointer;
  user-select: none;
  opacity: 0.6;
  transition: opacity 0.2s ease;
}

.filter-badge:hover {
  opacity: 0.8;
}

.filter-badge.active {
  opacity: 1;
  box-shadow: 0 0 0 2px var(--accent-color);
}

.search-input {
  width: 250px;
}

.loading-state,
.empty-state {
  padding: 3rem;
  text-align: center;
  color: var(--text-muted);
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 1rem;
}

.table-container {
  overflow-x: auto;
}

.data-table {
  width: 100%;
  border-collapse: collapse;
  font-size: 0.875rem;
}

.data-table thead th {
  padding: 0.75rem 1rem;
  text-align: left;
  border-bottom: 2px solid var(--border-color);
  background: var(--bg-tertiary);
  font-weight: 600;
  font-size: 0.75rem;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: var(--text-secondary);
  white-space: nowrap;
}

.data-table thead th.sortable {
  cursor: pointer;
  user-select: none;
}

.data-table thead th.sortable:hover {
  background: var(--border-hover);
}

.sort-icon {
  margin-left: 0.5rem;
  opacity: 0.5;
  font-size: 0.875rem;
}

.data-table tbody td {
  padding: 1rem;
  border-bottom: 1px solid var(--border-color);
}

.data-table tbody tr:hover {
  background: var(--bg-tertiary);
}

.node-info,
.service-info {
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
}

.node-name,
.service-name {
  font-weight: 600;
  color: var(--text-primary);
}

.node-moniker,
.service-desc {
  font-size: 0.75rem;
  color: var(--text-muted);
}

.feature-badge {
  margin-top: 0.25rem;
  width: fit-content;
}

.status-cell {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}

.maintenance-status {
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
}

.maintenance-time {
  font-size: 0.7rem;
  color: var(--text-muted);
}

.schedule-info {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}

.schedule-item {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  font-size: 0.75rem;
  color: var(--text-secondary);
}

.schedule-text {
  font-family: 'SF Mono', 'Monaco', 'Menlo', monospace;
  background: var(--bg-tertiary);
  padding: 0.125rem 0.375rem;
  border-radius: 0.25rem;
}

.action-buttons {
  display: flex;
  gap: 0.5rem;
}

.dropdown-item {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0.625rem 1rem;
  border: none;
  background: none;
  width: 100%;
  text-align: left;
  font-size: 0.875rem;
  color: var(--text-primary);
  cursor: pointer;
  transition: background 0.15s ease;
}

.dropdown-item:hover {
  background: var(--bg-tertiary);
}

.text-muted {
  color: var(--text-muted);
}

.url-link {
  color: var(--accent-color);
  text-decoration: none;
}

.url-link:hover {
  text-decoration: underline;
}
</style>
