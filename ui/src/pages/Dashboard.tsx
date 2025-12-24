import { useMemo, useState, useEffect } from 'react';
import { TooltipProvider } from '@kostovster/ui';
import { Boxes, Server, Activity, Layers } from 'lucide-react';
import { MetricCard } from '@/components/dashboard/MetricCard';
import { ActivityFeed, ActivityItem } from '@/components/dashboard/ActivityFeed';
import { UpcomingSchedule } from '@/components/dashboard/UpcomingSchedule';
import { SnapshotStats } from '@/components/dashboard/SnapshotStats';
import { SnapshotDownloads } from '@/components/dashboard/SnapshotDownloads';
import { IssuesPanel } from '@/components/dashboard/IssuesPanel';
import { fetchActiveOperations } from '@/api/client';
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
  const [activities, setActivities] = useState<ActivityItem[]>([]);
  const [activitiesLoading, setActivitiesLoading] = useState(true);

  useEffect(() => {
    async function loadActivities() {
      try {
        const ops = await fetchActiveOperations();
        const mapped: ActivityItem[] = ops.map((op) => ({
          id: op.id,
          operation_type: op.operation_type,
          target_name: op.target_name,
          status: op.status === 'in_progress' ? 'in_progress' : op.error_message ? 'failed' : 'completed',
          started_at: op.started_at,
          completed_at: op.completed_at,
          error_message: op.error_message,
        }));
        setActivities(mapped);
      } catch (error) {
        console.error('Failed to fetch activities:', error);
      } finally {
        setActivitiesLoading(false);
      }
    }
    loadActivities();
    const interval = setInterval(loadActivities, 10000);
    return () => clearInterval(interval);
  }, []);

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

  // Active operations count
  const activeOperationsCount = useMemo(() => {
    return activities.filter((a) => a.status === 'in_progress').length;
  }, [activities]);

  const nodeNames = useMemo(() => nodes.map((n) => n.node_name), [nodes]);

  return (
    <TooltipProvider>
      <div className="space-y-8">
        {/* Issues Panel - Shows when there are problems */}
        <IssuesPanel
          nodes={nodes}
          hermes={hermes}
          etl={etl}
          isLoading={isLoading}
          onNavigateToNodes={onNavigateToNodes}
          onNavigateToServices={onNavigateToServices}
        />

        {/* Metrics Grid */}
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-6">
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
            title="Active Operations"
            value={activeOperationsCount}
            subtitle={activeOperationsCount === 0 ? 'No operations running' : 'Operations in progress'}
            icon={<Activity className="h-5 w-5" />}
            tooltip="Ongoing maintenance operations (pruning, snapshots, etc.)"
            variant={activeOperationsCount > 0 ? 'warning' : 'default'}
            isLoading={isLoading || activitiesLoading}
          />
          <MetricCard
            title="Active Servers"
            value={metrics.uniqueServers}
            subtitle={`${metrics.uniqueNetworks} networks`}
            icon={<Server className="h-5 w-5" />}
            tooltip="Unique servers running nodes and services"
            isLoading={isLoading}
          />
        </div>

        {/* Main Content Grid */}
        <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
          {/* Left Column - Schedules & Activity */}
          <div className="lg:col-span-2 space-y-6">
            <UpcomingSchedule 
              configs={nodeConfigs} 
              isLoading={isLoading}
              maxItems={6}
            />
            <ActivityFeed 
              activities={activities} 
              isLoading={isLoading || activitiesLoading}
              maxHeight="320px"
            />
          </div>

          {/* Right Column - Stats & Actions */}
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
