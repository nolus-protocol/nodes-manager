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
        <h2>Blockchain Nodes</h2>
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
      
      <div v-if="loadingNodes && !nodes.length" class="loading-state">
        <Spinner size="lg" />
        <p>Loading blockchain nodes...</p>
      </div>

      <div v-else-if="filteredNodes.length === 0" class="empty-state">
        <p>No nodes found</p>
      </div>

      <Table v-else :data="filteredNodes" class="nodes-table">
        <template #columns>
          <th>Node</th>
          <th>Status</th>
          <th>Block Height</th>
          <th>Network</th>
          <th>Server</th>
          <th>Actions</th>
        </template>
        
        <template #row="{ item }">
          <TableRow>
            <td>
              <div class="node-info">
                <span class="node-name">{{ item.node_name }}</span>
                <span v-if="item.moniker" class="node-moniker">{{ item.moniker }}</span>
              </div>
            </td>
            <td>
              <Badge :variant="getStatusVariant(item.status)">
                {{ item.status }}
              </Badge>
            </td>
            <td>
              <span v-if="item.latest_block_height">
                {{ item.latest_block_height.toLocaleString() }}
              </span>
              <span v-else class="text-muted">-</span>
            </td>
            <td>{{ item.network }}</td>
            <td>{{ item.server_host }}</td>
            <td>
              <div class="action-buttons">
                <Button @click="restartNode(item.node_name)" size="sm" variant="secondary">
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
                    <button @click="pruneNode(item.node_name)">Prune</button>
                    <button @click="createSnapshot(item.node_name)">Snapshot</button>
                    <button @click="restoreSnapshot(item.node_name)">Restore</button>
                    <button @click="stateSync(item.node_name)">State Sync</button>
                  </template>
                </Dropdown>
              </div>
            </td>
          </TableRow>
        </template>
      </Table>
    </div>

    <!-- Hermes Relayers Section -->
    <div class="section-card">
      <div class="section-header">
        <h2>Hermes Relayers</h2>
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

      <div v-if="loadingHermes && !hermes.length" class="loading-state">
        <Spinner size="lg" />
        <p>Loading Hermes relayers...</p>
      </div>

      <div v-else-if="filteredHermes.length === 0" class="empty-state">
        <p>No relayers found</p>
      </div>

      <Table v-else :data="filteredHermes" class="hermes-table">
        <template #columns>
          <th>Relayer</th>
          <th>Status</th>
          <th>Uptime</th>
          <th>Server</th>
          <th>Actions</th>
        </template>
        
        <template #row="{ item }">
          <TableRow>
            <td>{{ item.hermes_name }}</td>
            <td>
              <Badge :variant="item.is_active ? 'success' : 'error'">
                {{ item.status }}
              </Badge>
            </td>
            <td>
              <span v-if="item.uptime_seconds">
                {{ formatUptime(item.uptime_seconds) }}
              </span>
              <span v-else class="text-muted">-</span>
            </td>
            <td>{{ item.server_host }}</td>
            <td>
              <Button @click="restartHermes(item.hermes_name)" size="sm" variant="secondary">
                Restart
              </Button>
            </td>
          </TableRow>
        </template>
      </Table>
    </div>

    <!-- ETL Services Section -->
    <div class="section-card">
      <div class="section-header">
        <h2>ETL Services</h2>
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

      <div v-if="loadingEtl && !etl.length" class="loading-state">
        <Spinner size="lg" />
        <p>Loading ETL services...</p>
      </div>

      <div v-else-if="filteredEtl.length === 0" class="empty-state">
        <p>No services found</p>
      </div>

      <Table v-else :data="filteredEtl" class="etl-table">
        <template #columns>
          <th>Service</th>
          <th>Status</th>
          <th>Response Time</th>
          <th>URL</th>
          <th>Actions</th>
        </template>
        
        <template #row="{ item }">
          <TableRow>
            <td>
              <div class="service-info">
                <span class="service-name">{{ item.service_name }}</span>
                <span v-if="item.description" class="service-desc">{{ item.description }}</span>
              </div>
            </td>
            <td>
              <Badge :variant="item.status === 'Healthy' ? 'success' : 'error'">
                {{ item.status }}
              </Badge>
            </td>
            <td>
              <span v-if="item.response_time_ms">
                {{ item.response_time_ms }}ms
              </span>
              <span v-else class="text-muted">-</span>
            </td>
            <td>
              <a :href="item.url" target="_blank" class="url-link">{{ item.url }}</a>
            </td>
            <td>
              <Button @click="restartEtl(item.service_name)" size="sm" variant="secondary">
                Restart
              </Button>
            </td>
          </TableRow>
        </template>
      </Table>
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
import { ref, computed, onMounted } from 'vue'
import { 
  Button, 
  Badge, 
  Input, 
  Table, 
  TableRow, 
  Dropdown, 
  Spinner,
  Toast,
  SvgIcon
} from 'web-components'
import { api } from '@/services/api'
import type { NodeHealth, HermesHealth, EtlHealth, DashboardMetrics } from '@/types/api'

// State
const nodes = ref<NodeHealth[]>([])
const hermes = ref<HermesHealth[]>([])
const etl = ref<EtlHealth[]>([])
const loadingNodes = ref(false)
const loadingHermes = ref(false)
const loadingEtl = ref(false)
const searchNodes = ref('')
const searchHermes = ref('')
const searchEtl = ref('')

const toast = ref({
  show: false,
  variant: 'success' as 'success' | 'error' | 'warning',
  message: ''
})

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

  // Count unique servers
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
  if (!searchNodes.value) return nodes.value
  const search = searchNodes.value.toLowerCase()
  return nodes.value.filter(n => 
    n.node_name.toLowerCase().includes(search) ||
    n.server_host.toLowerCase().includes(search) ||
    n.network.toLowerCase().includes(search)
  )
})

const filteredHermes = computed(() => {
  if (!searchHermes.value) return hermes.value
  const search = searchHermes.value.toLowerCase()
  return hermes.value.filter(h => 
    h.hermes_name.toLowerCase().includes(search) ||
    h.server_host.toLowerCase().includes(search)
  )
})

const filteredEtl = computed(() => {
  if (!searchEtl.value) return etl.value
  const search = searchEtl.value.toLowerCase()
  return etl.value.filter(e => 
    e.service_name.toLowerCase().includes(search) ||
    e.url.toLowerCase().includes(search)
  )
})

// Methods
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

function showToast(message: string, variant: 'success' | 'error' | 'warning' = 'success') {
  toast.value = { show: true, message, variant }
  setTimeout(() => { toast.value.show = false }, 5000)
}

async function refreshNodes() {
  loadingNodes.value = true
  try {
    const response = await api.getNodesHealth()
    if (response.success) {
      nodes.value = response.data
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
}

.section-actions {
  display: flex;
  gap: 0.75rem;
  align-items: center;
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

.action-buttons {
  display: flex;
  gap: 0.5rem;
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
