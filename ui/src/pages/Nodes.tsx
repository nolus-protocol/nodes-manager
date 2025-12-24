import { useState, useMemo, useEffect } from 'react';
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  Badge,
  Button,
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
  Input,
  Tooltip,
  TooltipContent,
  TooltipTrigger,
  TooltipProvider,
  Skeleton,
  ScrollArea,
  cn,
} from '@kostovster/ui';
import { 
  Boxes, 
  RefreshCw, 
  Search, 
  ArrowUpDown,
  Filter,
} from 'lucide-react';
import { ScheduleItem, NextRunItem } from '@/components/shared/CronDisplay';
import { NodeActions } from '@/components/nodes/NodeActions';
import { formatName, formatBlockHeight } from '@/lib/utils';
import { getCronSortValue } from '@/lib/cron';
import type { NodeHealth, NodeConfig, NodeFilter, SortConfig } from '@/types';

const nodeStatusConfig: Record<string, 'default' | 'secondary' | 'destructive' | 'outline'> = {
  synced: 'default',
  healthy: 'default',
  'catching up': 'secondary',
  catchingup: 'secondary',
  unhealthy: 'destructive',
  maintenance: 'outline',
};

interface NodesPageProps {
  nodes: NodeHealth[];
  configs: Record<string, NodeConfig>;
  onRefresh: () => void;
  isLoading?: boolean;
}

export function NodesPage({ nodes, configs, onRefresh, isLoading = false }: NodesPageProps) {
  const [search, setSearch] = useState(() => localStorage.getItem('nodesSearch') || '');
  const [filter, setFilter] = useState<NodeFilter>(() => 
    (localStorage.getItem('nodesFilter') as NodeFilter) || 'all'
  );
  const [sort, setSort] = useState<SortConfig>(() => {
    try {
      return JSON.parse(localStorage.getItem('nodesSort') || '{"column":"next","direction":"asc"}');
    } catch {
      return { column: 'next', direction: 'asc' };
    }
  });

  useEffect(() => {
    localStorage.setItem('nodesSearch', search);
  }, [search]);
  
  useEffect(() => {
    localStorage.setItem('nodesFilter', filter);
  }, [filter]);
  
  useEffect(() => {
    localStorage.setItem('nodesSort', JSON.stringify(sort));
  }, [sort]);

  const filters: { value: NodeFilter; label: string; count: number }[] = useMemo(() => [
    { value: 'all', label: 'All', count: nodes.length },
    { value: 'synced', label: 'Synced', count: nodes.filter(n => ['synced', 'healthy'].includes(n.status.toLowerCase())).length },
    { value: 'catching-up', label: 'Catching Up', count: nodes.filter(n => n.status.toLowerCase().includes('catching')).length },
    { value: 'unhealthy', label: 'Unhealthy', count: nodes.filter(n => n.status.toLowerCase() === 'unhealthy').length },
    { value: 'maintenance', label: 'Maintenance', count: nodes.filter(n => n.status.toLowerCase() === 'maintenance').length },
  ], [nodes]);

  const filteredAndSortedNodes = useMemo(() => {
    let result = [...nodes];

    if (search) {
      const searchLower = search.toLowerCase();
      result = result.filter(node => 
        node.node_name.toLowerCase().includes(searchLower) ||
        node.server_host.toLowerCase().includes(searchLower) ||
        node.network.toLowerCase().includes(searchLower)
      );
    }

    if (filter !== 'all') {
      result = result.filter(node => {
        const status = node.status.toLowerCase().replace(/\s+/g, '-');
        return status === filter || 
          (filter === 'synced' && status === 'healthy') ||
          (filter === 'catching-up' && status === 'catchingup');
      });
    }

    result.sort((a, b) => {
      const configA = configs[a.node_name] || {};
      const configB = configs[b.node_name] || {};
      
      let comparison = 0;
      
      switch (sort.column) {
        case 'name':
          comparison = a.node_name.localeCompare(b.node_name);
          break;
        case 'status': {
          const statusOrder: Record<string, number> = { 
            'unhealthy': 0, 'maintenance': 1, 'catching up': 2, 'synced': 3, 'healthy': 3 
          };
          const statusA = statusOrder[a.status.toLowerCase()] ?? 4;
          const statusB = statusOrder[b.status.toLowerCase()] ?? 4;
          comparison = statusA - statusB;
          break;
        }
        case 'next': {
          const nextA = Math.min(
            configA.pruning_enabled ? getCronSortValue(configA.pruning_schedule) : Infinity,
            configA.snapshots_enabled ? getCronSortValue(configA.snapshot_schedule) : Infinity,
            configA.state_sync_enabled ? getCronSortValue(configA.state_sync_schedule) : Infinity
          );
          const nextB = Math.min(
            configB.pruning_enabled ? getCronSortValue(configB.pruning_schedule) : Infinity,
            configB.snapshots_enabled ? getCronSortValue(configB.snapshot_schedule) : Infinity,
            configB.state_sync_enabled ? getCronSortValue(configB.state_sync_schedule) : Infinity
          );
          comparison = nextA - nextB;
          break;
        }
        default:
          comparison = 0;
      }
      
      return sort.direction === 'asc' ? comparison : -comparison;
    });

    return result;
  }, [nodes, configs, search, filter, sort]);

  const handleSort = (column: string) => {
    setSort(prev => ({
      column,
      direction: prev.column === column && prev.direction === 'asc' ? 'desc' : 'asc',
    }));
  };

  const SortableHeader = ({ column, children }: { column: string; children: React.ReactNode }) => (
    <TableHead 
      className="cursor-pointer hover:bg-muted/50 transition-colors"
      onClick={() => handleSort(column)}
    >
      <div className="flex items-center gap-1">
        {children}
        <ArrowUpDown className={cn(
          "h-3 w-3 text-muted-foreground transition-opacity",
          sort.column === column ? "opacity-100" : "opacity-0"
        )} />
      </div>
    </TableHead>
  );

  const getStatusVariant = (status: string) => {
    const key = status.toLowerCase().replace(/\s+/g, '');
    return nodeStatusConfig[key] || nodeStatusConfig[status.toLowerCase()] || 'secondary';
  };

  return (
    <TooltipProvider>
      <div className="space-y-6">
        {/* Page Header */}
        <div className="flex items-center justify-end">
          <Button onClick={onRefresh} disabled={isLoading}>
            <RefreshCw className={cn("h-4 w-4 mr-2", isLoading && "animate-spin")} />
            Refresh
          </Button>
        </div>

        {/* Filters Card */}
        <Card>
          <CardContent className="p-4">
            <div className="flex flex-col sm:flex-row gap-4">
              {/* Search */}
              <div className="relative flex-1">
                <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground" />
                <Input
                  type="text"
                  value={search}
                  onChange={(e) => setSearch(e.target.value)}
                  placeholder="Search by name, server, or network..."
                  className="pl-9"
                />
              </div>
              
              {/* Filter Buttons */}
              <div className="flex items-center gap-2">
                <Filter className="h-4 w-4 text-muted-foreground" />
                <div className="flex gap-1">
                  {filters.map(f => (
                    <Button
                      key={f.value}
                      variant={filter === f.value ? 'default' : 'outline'}
                      size="sm"
                      onClick={() => setFilter(f.value)}
                      className="gap-1"
                    >
                      {f.label}
                      <Badge variant="secondary" className="ml-1 text-xs px-1.5">
                        {f.count}
                      </Badge>
                    </Button>
                  ))}
                </div>
              </div>
            </div>
          </CardContent>
        </Card>

        {/* Nodes Table */}
        <Card>
          <CardHeader className="pb-0">
            <div className="flex items-center justify-between">
              <CardTitle className="flex items-center gap-2 text-base">
                <Boxes className="h-5 w-5" />
                Nodes
              </CardTitle>
              <Badge variant="outline">
                {filteredAndSortedNodes.length} of {nodes.length}
              </Badge>
            </div>
          </CardHeader>
          <CardContent className="p-0 pt-4">
            {isLoading ? (
              <div className="p-6 space-y-4">
                {[...Array(5)].map((_, i) => (
                  <div key={i} className="flex items-center gap-4">
                    <Skeleton className="h-12 w-50" />
                    <Skeleton className="h-8 w-25" />
                    <Skeleton className="h-8 flex-1" />
                    <Skeleton className="h-8 w-36" />
                    <Skeleton className="h-8 w-10" />
                  </div>
                ))}
              </div>
            ) : filteredAndSortedNodes.length === 0 ? (
              <div className="p-12 text-center text-muted-foreground">
                <Boxes className="h-12 w-12 mx-auto mb-4 opacity-50" />
                <p className="font-medium text-lg">No nodes found</p>
                <p className="text-sm">Try adjusting your search or filter criteria</p>
              </div>
            ) : (
              <ScrollArea className="h-[600px]">
                <Table>
                  <TableHeader>
                    <TableRow>
                      <SortableHeader column="name">Node / Server</SortableHeader>
                      <SortableHeader column="status">Status / Block</SortableHeader>
                      <TableHead>Schedules</TableHead>
                      <SortableHeader column="next">Next Run</SortableHeader>
                      <TableHead className="w-20">Actions</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {filteredAndSortedNodes.map(node => {
                      const config = configs[node.node_name] || {} as NodeConfig;
                      return (
                        <TableRow key={node.node_name} className="group">
                          <TableCell>
                            <div className="flex flex-col gap-1">
                              <span className="font-medium">{formatName(node.node_name)}</span>
                              <span className="text-xs text-muted-foreground">{node.server_host}</span>
                              <div className="flex items-center gap-1.5">
                                {config.auto_restore_enabled && (
                                  <Badge variant="outline" className="text-xs">
                                    Auto restore
                                  </Badge>
                                )}
                                <Badge variant="secondary" className="text-xs">
                                  {node.network}
                                </Badge>
                              </div>
                            </div>
                          </TableCell>
                          <TableCell>
                            <div className="flex flex-col gap-1.5">
                              <Badge variant={getStatusVariant(node.status)}>
                                {node.status}
                              </Badge>
                              <Tooltip>
                                <TooltipTrigger asChild>
                                  <span className="text-xs text-muted-foreground cursor-help">
                                    Block: {formatBlockHeight(node.latest_block_height)}
                                  </span>
                                </TooltipTrigger>
                                <TooltipContent>
                                  <p>Latest synced block height</p>
                                </TooltipContent>
                              </Tooltip>
                            </div>
                          </TableCell>
                          <TableCell>
                            <div className="flex flex-col gap-2">
                              <ScheduleItem 
                                label="Pruning" 
                                schedule={config.pruning_schedule} 
                                enabled={config.pruning_enabled}
                              />
                              <ScheduleItem 
                                label="Snapshot" 
                                schedule={config.snapshot_schedule} 
                                enabled={config.snapshots_enabled}
                              />
                              <ScheduleItem 
                                label="State Sync" 
                                schedule={config.state_sync_schedule} 
                                enabled={config.state_sync_enabled}
                              />
                            </div>
                          </TableCell>
                          <TableCell>
                            <div className="flex flex-col gap-2">
                              <NextRunItem 
                                label="Pruning" 
                                schedule={config.pruning_schedule} 
                                enabled={config.pruning_enabled}
                              />
                              <NextRunItem 
                                label="Snapshot" 
                                schedule={config.snapshot_schedule} 
                                enabled={config.snapshots_enabled}
                              />
                              <NextRunItem 
                                label="State Sync" 
                                schedule={config.state_sync_schedule} 
                                enabled={config.state_sync_enabled}
                              />
                            </div>
                          </TableCell>
                          <TableCell>
                            <NodeActions 
                              nodeName={node.node_name} 
                              config={config}
                              onActionComplete={onRefresh}
                            />
                          </TableCell>
                        </TableRow>
                      );
                    })}
                  </TableBody>
                </Table>
              </ScrollArea>
            )}
          </CardContent>
        </Card>
      </div>
    </TooltipProvider>
  );
}
