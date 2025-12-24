import { useState, useEffect, useCallback, useMemo } from 'react';
import { Skeleton } from '@kostovster/ui';
import { Header } from '@/components/layout/Header';
import { MetricsGrid } from '@/components/layout/MetricsGrid';
import { NodesPanel } from '@/components/nodes/NodesPanel';
import { HermesPanel } from '@/components/hermes/HermesPanel';
import { EtlPanel } from '@/components/etl/EtlPanel';
import {
  fetchNodeConfigs,
  fetchHermesConfigs,
  fetchNodeHealth,
  fetchHermesHealth,
  fetchEtlHealth,
} from '@/api/client';
import type { NodeConfig, NodeHealth, HermesConfig, HermesHealth, EtlHealth } from '@/types';

const REFRESH_INTERVAL = 30000;

function App() {
  const [nodeConfigs, setNodeConfigs] = useState<Record<string, NodeConfig>>({});
  const [hermesConfigs, setHermesConfigs] = useState<Record<string, HermesConfig>>({});
  
  const [nodeHealth, setNodeHealth] = useState<NodeHealth[]>([]);
  const [hermesHealth, setHermesHealth] = useState<HermesHealth[]>([]);
  const [etlHealth, setEtlHealth] = useState<EtlHealth[]>([]);
  
  const [isLoading, setIsLoading] = useState(true);
  const [isRefreshing, setIsRefreshing] = useState(false);

  const loadAllData = useCallback(async (showRefreshing = false) => {
    if (showRefreshing) setIsRefreshing(true);
    
    try {
      const [nConfigs, hConfigs, nHealth, hHealth, eHealth] = await Promise.all([
        fetchNodeConfigs(),
        fetchHermesConfigs(),
        fetchNodeHealth(),
        fetchHermesHealth(),
        fetchEtlHealth(),
      ]);
      
      setNodeConfigs(nConfigs);
      setHermesConfigs(hConfigs);
      setNodeHealth(nHealth);
      setHermesHealth(hHealth);
      setEtlHealth(eHealth);
    } catch (error) {
      console.error('Failed to load data:', error);
    } finally {
      setIsLoading(false);
      setIsRefreshing(false);
    }
  }, []);

  useEffect(() => {
    loadAllData();
    
    const interval = setInterval(() => loadAllData(false), REFRESH_INTERVAL);
    
    const handleFocus = () => loadAllData(false);
    window.addEventListener('focus', handleFocus);
    
    return () => {
      clearInterval(interval);
      window.removeEventListener('focus', handleFocus);
    };
  }, [loadAllData]);

  const metrics = useMemo(() => {
    const operationalNodes = nodeHealth.filter(n => {
      const status = n.status.toLowerCase();
      return status === 'synced' || status === 'catching up' || status === 'healthy';
    }).length;

    const operationalHermes = hermesHealth.filter(h => 
      h.status.toLowerCase().replace(/[()]/g, '').includes('running')
    ).length;

    const operationalEtl = etlHealth.filter(e => 
      e.status.toLowerCase() === 'healthy'
    ).length;

    const maintenanceNodes = nodeHealth.filter(n => 
      n.status.toLowerCase() === 'maintenance'
    ).length;

    const servers = new Set<string>();
    [...nodeHealth, ...hermesHealth, ...etlHealth].forEach(item => {
      if ('server_host' in item && item.server_host) {
        servers.add(item.server_host);
      }
    });

    const total = nodeHealth.length + hermesHealth.length + etlHealth.length;
    const operational = operationalNodes + operationalHermes + operationalEtl;
    const healthPct = total > 0 ? Math.round((operational / total) * 100) : 0;

    return {
      totalComponents: total,
      operationalComponents: operational,
      nodesCount: nodeHealth.length,
      hermesCount: hermesHealth.length,
      etlCount: etlHealth.length,
      serverCount: servers.size,
      healthPercentage: healthPct,
      maintenanceCount: maintenanceNodes,
    };
  }, [nodeHealth, hermesHealth, etlHealth]);

  const systemStatus = useMemo(() => {
    if (metrics.maintenanceCount > 0) {
      return {
        status: 'maintenance' as const,
        message: `${metrics.maintenanceCount} System${metrics.maintenanceCount === 1 ? '' : 's'} in Maintenance`,
      };
    }
    if (metrics.healthPercentage === 100) {
      return { status: 'healthy' as const, message: 'All Systems Operational' };
    }
    if (metrics.healthPercentage >= 80) {
      return { status: 'warning' as const, message: 'Minor Issues Detected' };
    }
    return { status: 'error' as const, message: 'System Issues Detected' };
  }, [metrics]);

  const handleRefresh = useCallback(() => {
    loadAllData(true);
  }, [loadAllData]);

  if (isLoading) {
    return (
      <div className="min-h-screen bg-background">
        {/* Header skeleton */}
        <header className="border-b bg-card">
          <div className="max-w-7xl mx-auto px-8 py-6 flex justify-between items-center">
            <div className="flex items-center gap-3">
              <Skeleton className="h-6 w-6 rounded" />
              <Skeleton className="h-6 w-50" />
            </div>
            <div className="flex items-center gap-4">
              <Skeleton className="h-6 w-36 rounded-full" />
              <Skeleton className="h-9 w-9 rounded" />
            </div>
          </div>
        </header>
        
        <main className="max-w-7xl mx-auto px-8 py-8">
          {/* Metrics grid skeleton */}
          <div className="grid grid-cols-4 gap-6 mb-8">
            {[...Array(4)].map((_, i) => (
              <div key={i} className="border rounded-lg p-6 bg-card">
                <div className="flex justify-between items-center mb-4">
                  <Skeleton className="h-4 w-25" />
                  <Skeleton className="h-5 w-5 rounded" />
                </div>
                <Skeleton className="h-8 w-15 mb-2" />
                <Skeleton className="h-4 w-30" />
              </div>
            ))}
          </div>
          
          {/* Panels skeleton */}
          <div className="space-y-6">
            {[...Array(3)].map((_, i) => (
              <div key={i} className="border rounded-lg bg-card">
                <div className="p-6 border-b bg-muted/50">
                  <div className="flex justify-between items-center">
                    <div className="flex items-center gap-2">
                      <Skeleton className="h-5 w-5 rounded" />
                      <Skeleton className="h-5 w-5 rounded" />
                      <Skeleton className="h-6 w-36" />
                      <Skeleton className="h-5 w-8 rounded-full" />
                    </div>
                    <Skeleton className="h-9 w-25 rounded" />
                  </div>
                </div>
              </div>
            ))}
          </div>
        </main>
      </div>
    );
  }

  return (
    <div className="min-h-screen bg-background">
      <Header 
        systemStatus={systemStatus.status} 
        statusMessage={systemStatus.message} 
      />
      
      <main className="max-w-7xl mx-auto px-8 py-8">
        <MetricsGrid
          totalComponents={metrics.totalComponents}
          operationalComponents={metrics.operationalComponents}
          nodesCount={metrics.nodesCount}
          hermesCount={metrics.hermesCount}
          etlCount={metrics.etlCount}
          serverCount={metrics.serverCount}
        />
        
        <div className="space-y-6">
          <NodesPanel
            nodes={nodeHealth}
            configs={nodeConfigs}
            onRefresh={handleRefresh}
            isLoading={isRefreshing}
          />
          
          <HermesPanel
            instances={hermesHealth}
            configs={hermesConfigs}
            onRefresh={handleRefresh}
            isLoading={isRefreshing}
          />
          
          <EtlPanel
            services={etlHealth}
            onRefresh={handleRefresh}
            isLoading={isRefreshing}
          />
        </div>
      </main>
    </div>
  );
}

export default App;
