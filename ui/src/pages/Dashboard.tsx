import { useMemo } from 'react';
import { TooltipProvider } from '@kostovster/ui';
import { Boxes, Server, Layers } from 'lucide-react';
import { MetricCard } from '@/components/dashboard/MetricCard';
import { UpcomingSchedule } from '@/components/dashboard/UpcomingSchedule';
import { SnapshotStats } from '@/components/dashboard/SnapshotStats';
import { SnapshotDownloads } from '@/components/dashboard/SnapshotDownloads';
import { IssuesPanel } from '@/components/dashboard/IssuesPanel';
import type { NodeHealth, NodeConfig, HermesHealth, EtlHealth } from '@/types';

interface DashboardProps {
  nodes: NodeHealth[];
  nodeConfigs: Record<string, NodeConfig>;
  hermes: HermesHealth[];
  etl: EtlHealth[];
  isLoading?: boolean;
  onNavigateToNodes?: () => void;
  onNavigateToServices?: () => void;
}

export function Dashboard({ nodes, nodeConfigs, hermes, etl, isLoading = false, onNavigateToNodes, onNavigateToServices }: DashboardProps) {
  const metrics = useMemo(() => {
    const totalNodes = nodes.length;
    const healthyNodes = nodes.filter(
      (n) => n.status.toLowerCase() === 'synced' || n.status.toLowerCase() === 'healthy'
    ).length;
    const unhealthyNodes = nodes.filter(
      (n) => n.status.toLowerCase() === 'unhealthy'
    ).length;

    const totalHermes = hermes.length;
    const runningHermes = hermes.filter(
      (h) => h.status.toLowerCase().includes('running')
    ).length;

    const totalEtl = etl.length;
    const healthyEtl = etl.filter(
      (e) => e.status.toLowerCase() === 'healthy'
    ).length;

    // Services = Hermes + ETL only (not nodes)
    const totalServices = totalHermes + totalEtl;
    const healthyServices = runningHermes + healthyEtl;
    const serviceIssues = totalServices - healthyServices;

    const uniqueServers = new Set([
      ...nodes.map((n) => n.server_host),
      ...hermes.map((h) => h.server_host),
      ...etl.map((e) => e.server_host),
    ]).size;

    // Unique networks
    const uniqueNetworks = new Set(nodes.map((n) => n.network)).size;

    return {
      totalNodes,
      healthyNodes,
      unhealthyNodes,
      totalServices,
      healthyServices,
      serviceIssues,
      uniqueServers,
      uniqueNetworks,
    };
  }, [nodes, hermes, etl]);

  const nodeNames = useMemo(() => nodes.map((n) => n.node_name), [nodes]);

  return (
    <TooltipProvider>
      <div className="space-y-6">
        {/* Issues Panel - Shows system status */}
        <IssuesPanel
          nodes={nodes}
          hermes={hermes}
          etl={etl}
          isLoading={isLoading}
          onNavigateToNodes={onNavigateToNodes}
          onNavigateToServices={onNavigateToServices}
        />

        {/* Metrics Grid */}
        <div className="grid grid-cols-1 md:grid-cols-3 gap-6">
          <MetricCard
            title="Blockchain Nodes"
            value={metrics.totalNodes}
            subtitle={`${metrics.healthyNodes} healthy, ${metrics.unhealthyNodes} issues`}
            icon={<Boxes className="h-5 w-5" />}
            tooltip="Total blockchain nodes being monitored"
            variant={metrics.unhealthyNodes === 0 ? 'success' : 'danger'}
            isLoading={isLoading}
          />
          <MetricCard
            title="Services"
            value={metrics.totalServices}
            subtitle={`${metrics.healthyServices} healthy, ${metrics.serviceIssues} issues`}
            icon={<Layers className="h-5 w-5" />}
            tooltip="Hermes relayers and ETL services"
            variant={metrics.serviceIssues === 0 ? 'success' : 'warning'}
            isLoading={isLoading}
          />
          <MetricCard
            title="Infrastructure"
            value={metrics.uniqueServers}
            subtitle={`${metrics.uniqueNetworks} networks`}
            icon={<Server className="h-5 w-5" />}
            tooltip="Unique servers and blockchain networks"
            isLoading={isLoading}
          />
        </div>

        {/* Content Grid */}
        <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
          <UpcomingSchedule 
            configs={nodeConfigs} 
            isLoading={isLoading}
            maxItems={8}
          />
          <div className="space-y-6">
            <SnapshotStats 
              nodeNames={nodeNames} 
              isLoading={isLoading}
            />
            <SnapshotDownloads 
              nodeNames={nodeNames} 
              isLoading={isLoading}
            />
          </div>
        </div>
      </div>
    </TooltipProvider>
  );
}
