import { useState, useEffect } from 'react';
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  Badge,
  Progress,
  Skeleton,
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from '@kostovster/ui';
import { HardDrive, Database, TrendingUp } from 'lucide-react';

interface SnapshotStatsData {
  total_snapshots: number;
  total_size_bytes: number;
  by_network: Record<string, number>;
}

interface SnapshotStatsProps {
  nodeNames: string[];
  isLoading?: boolean;
}

function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${parseFloat((bytes / Math.pow(k, i)).toFixed(1))} ${sizes[i]}`;
}

export function SnapshotStats({ nodeNames, isLoading = false }: SnapshotStatsProps) {
  const [stats, setStats] = useState<SnapshotStatsData | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    async function fetchStats() {
      if (nodeNames.length === 0) {
        setLoading(false);
        return;
      }

      try {
        // Fetch stats for ALL nodes and aggregate
        const allStats = await Promise.all(
          nodeNames.map(async (name) => {
            try {
              const response = await fetch(`/api/snapshots/${name}/stats`);
              if (!response.ok) return null;
              const data = await response.json();
              return data.success ? data.data : null;
            } catch {
              return null;
            }
          })
        );

        const validStats = allStats.filter(Boolean);
        
        if (validStats.length > 0) {
          const aggregated: SnapshotStatsData = {
            total_snapshots: validStats.reduce((sum, s) => sum + (s.total_snapshots || 0), 0),
            total_size_bytes: validStats.reduce((sum, s) => sum + (s.total_size_bytes || 0), 0),
            by_network: {},
          };

          validStats.forEach((s) => {
            if (s.by_network) {
              Object.entries(s.by_network).forEach(([network, count]) => {
                aggregated.by_network[network] = (aggregated.by_network[network] || 0) + (count as number);
              });
            }
          });

          setStats(aggregated);
        }
      } catch (error) {
        console.error('Failed to fetch snapshot stats:', error);
      } finally {
        setLoading(false);
      }
    }

    fetchStats();
  }, [nodeNames]);

  const showLoading = isLoading || loading;

  if (showLoading) {
    return (
      <Card>
        <CardHeader className="pb-3">
          <CardTitle className="flex items-center gap-2 text-base">
            <HardDrive className="h-5 w-5" />
            Snapshot Storage
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="space-y-4">
            <div className="flex items-center justify-between">
              <Skeleton className="h-8 w-24" />
              <Skeleton className="h-5 w-16" />
            </div>
            <Skeleton className="h-2 w-full" />
            <div className="grid grid-cols-2 gap-4">
              {[...Array(4)].map((_, i) => (
                <div key={i} className="space-y-1">
                  <Skeleton className="h-4 w-20" />
                  <Skeleton className="h-3 w-12" />
                </div>
              ))}
            </div>
          </div>
        </CardContent>
      </Card>
    );
  }

  if (!stats || stats.total_snapshots === 0) {
    return (
      <Card>
        <CardHeader className="pb-3">
          <CardTitle className="flex items-center gap-2 text-base">
            <HardDrive className="h-5 w-5" />
            Snapshot Storage
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="py-6 text-center text-muted-foreground">
            <Database className="h-10 w-10 mx-auto mb-3 opacity-50" />
            <p className="font-medium">No snapshots available</p>
            <p className="text-sm">Create snapshots to see storage stats</p>
          </div>
        </CardContent>
      </Card>
    );
  }

  const networks = Object.entries(stats.by_network).sort((a, b) => b[1] - a[1]);
  const maxCount = Math.max(...networks.map(([, count]) => count));

  return (
    <Card>
      <CardHeader className="pb-3">
        <div className="flex items-center justify-between">
          <CardTitle className="flex items-center gap-2 text-base">
            <HardDrive className="h-5 w-5" />
            Snapshot Storage
          </CardTitle>
          <Badge variant="secondary" className="text-xs">
            {stats.total_snapshots} snapshots
          </Badge>
        </div>
      </CardHeader>
      <CardContent>
        <div className="space-y-4">
          {/* Total Size */}
          <div className="flex items-center justify-between">
            <div>
              <div className="text-2xl font-bold">{formatBytes(stats.total_size_bytes)}</div>
              <p className="text-xs text-muted-foreground">Total storage used</p>
            </div>
            <Tooltip>
              <TooltipTrigger asChild>
                <div className="flex items-center gap-1 text-green-500 cursor-help">
                  <TrendingUp className="h-4 w-4" />
                </div>
              </TooltipTrigger>
              <TooltipContent>
                <p>Snapshot storage is healthy</p>
              </TooltipContent>
            </Tooltip>
          </div>

          {/* Network Breakdown */}
          {networks.length > 0 && (
            <div className="space-y-3 pt-2">
              <p className="text-xs font-medium text-muted-foreground uppercase tracking-wider">
                By Network
              </p>
              {networks.map(([network, count]) => (
                <div key={network} className="space-y-1">
                  <div className="flex items-center justify-between text-sm">
                    <span className="font-medium truncate">{network}</span>
                    <span className="text-muted-foreground">{count} snapshots</span>
                  </div>
                  <Progress value={(count / maxCount) * 100} className="h-1.5" />
                </div>
              ))}
            </div>
          )}
        </div>
      </CardContent>
    </Card>
  );
}
