<template>
  <div class="app">
    <header class="app-header">
      <div class="header-content">
        <h1>🖥️ Nodes Manager</h1>
        <div class="header-actions">
          <Badge :variant="systemHealthy ? 'success' : 'error'">
            {{ systemHealthy ? 'All Systems Operational' : 'System Issues' }}
          </Badge>
        </div>
      </div>
    </header>

    <main class="app-main">
      <DashboardView />
    </main>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { Badge } from 'web-components'
import DashboardView from './views/DashboardView.vue'
import { api } from './services/api'

const systemHealthy = ref(true)

onMounted(async () => {
  try {
    const [nodes, hermes, etl] = await Promise.all([
      api.getNodesHealth(),
      api.getHermesHealth(),
      api.getEtlHealth(),
    ])

    const totalUnhealthy = 
      nodes.data.filter(n => n.status === 'Unhealthy').length +
      hermes.data.filter(h => !h.is_active).length +
      etl.data.filter(e => e.status === 'Unhealthy').length

    systemHealthy.value = totalUnhealthy === 0
  } catch (error) {
    console.error('Failed to load system health:', error)
    systemHealthy.value = false
  }
})
</script>

<style scoped>
.app {
  width: 100%;
  min-height: 100vh;
  display: flex;
  flex-direction: column;
}

.app-header {
  background: var(--bg-secondary);
  border-bottom: 1px solid var(--border-color);
  padding: 1.5rem 0;
  box-shadow: 0 1px 2px 0 rgba(0, 0, 0, 0.05);
}

.header-content {
  max-width: 1400px;
  margin: 0 auto;
  padding: 0 2rem;
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.app-header h1 {
  font-size: 1.5rem;
  font-weight: 600;
  color: var(--text-primary);
}

.header-actions {
  display: flex;
  gap: 1rem;
  align-items: center;
}

.app-main {
  flex: 1;
  max-width: 1400px;
  width: 100%;
  margin: 0 auto;
  padding: 2rem;
}
</style>
