import { useState, useMemo } from 'react';
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
  Clock,
} from 'lucide-react';
import { NodeActions } from '@/components/nodes/NodeActions';
import { formatName, formatBlockHeight } from '@/lib/utils';
import { getNextRun, formatNextRun } from '@/lib/cron';
import type { NodeHealth, NodeConfig, NodeFilter } from '@/types';

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

interface NextOperation {
  type: string;
  schedule: string;
  nextRun: Date;
}

function getNextOperation(config: NodeConfig): NextOperation | null {
  const operations: NextOperation[] = [];

  if (config.pruning_enabled && config.pruning_schedule) {
    const nextRun = getNextRun(config.pruning_schedule);
    if (nextRun) {
      operations.push({ type: 'Pruning', schedule: config.pruning_schedule, nextRun });
    }
  }
  if (config.snapshots_enabled && config.snapshot_schedule) {
    const nextRun = getNextRun(config.snapshot_schedule);
    if (nextRun) {
      operations.push({ type: 'Snapshot', schedule: config.snapshot_schedule, nextRun });
    }
  }
  if (config.state_sync_enabled && config.state_sync_schedule) {
    const nextRun = getNextRun(config.state_sync_schedule);
    if (nextRun) {
      operations.push({ type: 'State Sync', schedule: config.state_sync_schedule, nextRun });
    }
  }

  if (operations.length === 0) return null;

  // Return the soonest operation
  operations.sort((a, b) => a.nextRun.getTime() - b.nextRun.getTime());
  return operations[0];
}

export function NodesPage({ nodes, configs, onRefresh, isLoading = false }: NodesPageProps) {
  const [search, setSearch] = useState('');
  const [filter, setFilter] = useState<NodeFilter>('all');

  const filters: { value: NodeFilter; label: string; count: number }[] = useMemo(() => [
    { value: 'all', label: 'All', count: nodes.length },
    { value: 'synced', label: 'Synced', count: nodes.filter(n => ['synced', 'healthy'].includes(n.status.toLowerCase())).length },
    { value: 'catching-up', label: 'Catching Up', count: nodes.filter(n => n.status.toLowerCase().includes('catching')).length },
    { value: 'unhealthy', label: 'Unhealthy', count: nodes.filter(n => n.status.toLowerCase() === 'unhealthy').length },
    { value: 'maintenance', label: 'Maintenance', count: nodes.filter(n => n.status.toLowerCase() === 'maintenance').length },
  ], [nodes]);

  const filteredNodes = useMemo(() => {
    let result = [...nodes];

    if (search) {
      const searchLower = search.toLowerCase();
      result = result.filter(node => 
        (node.node_name || '').toLowerCase().includes(searchLower) ||
        (node.server_host || '').toLowerCase().includes(searchLower) ||
        (node.network || '').toLowerCase().includes(searchLower)
      );
    }

    if (filter !== 'all') {
      result = result.filter(node => {
        const status = (node.status || '').toLowerCase().replace(/\s+/g, '-');
        return status === filter || 
          (filter === 'synced' && status === 'healthy') ||
          (filter === 'catching-up' && status === 'catchingup');
      });
    }

    // Sort by name
    result.sort((a, b) => (a.node_name || '').localeCompare(b.node_name || ''));

    return result;
  }, [nodes, search, filter]);

  const getStatusVariant = (status: string) => {
    const key = status.toLowerCase().replace(/\s+/g, '');
    return nodeStatusConfig[key] || nodeStatusConfig[status.toLowerCase()] || 'secondary';
  };

  return (
    <TooltipProvider>
      <div className="space-y-6">
        {/* Header */}
        <div className="flex items-center justify-end">
          <Button onClick={onRefresh} disabled={isLoading}>
            <RefreshCw className={cn("h-4 w-4 mr-2", isLoading && "animate-spin")} />
            Refresh
          </Button>
        </div>

        {/* Nodes Table */}
        <Card>
          <CardHeader className="pb-4">
            <div className="flex items-center justify-between">
              <CardTitle className="flex items-center gap-2">
                <Boxes className="h-5 w-5" />
                Nodes
                <Badge variant="outline">{filteredNodes.length}</Badge>
              </CardTitle>
            </div>
            {/* Filters */}
            <div className="flex flex-col sm:flex-row items-start sm:items-center gap-4 mt-4">
              <div className="relative flex-1 max-w-sm">
                <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground" />
                <Input
                  type="text"
                  value={search}
                  onChange={(e) => setSearch(e.target.value)}
                  placeholder="Search by name, server, or network..."
                  className="pl-9"
                />
              </div>
              <div className="flex gap-1 flex-wrap">
                {filters.map(f => (
                  <Button
                    key={f.value}
                    variant={filter === f.value ? 'default' : 'outline'}
                    size="sm"
                    onClick={() => setFilter(f.value)}
                  >
                    {f.label}
                    <Badge variant="secondary" className="ml-1.5 text-xs">
                      {f.count}
                    </Badge>
                  </Button>
                ))}
              </div>
            </div>
          </CardHeader>
          <CardContent className="p-0">
            {isLoading ? (
              <div className="p-6 space-y-4">
                {[...Array(5)].map((_, i) => (
                  <div key={i} className="flex items-center gap-4">
                    <Skeleton className="h-10 w-32" />
                    <Skeleton className="h-5 w-20" />
                    <Skeleton className="h-5 w-24" />
                    <Skeleton className="h-5 flex-1" />
                    <Skeleton className="h-8 w-8" />
                  </div>
                ))}
              </div>
            ) : filteredNodes.length === 0 ? (
              <div className="p-12 text-center text-muted-foreground">
                <Boxes className="h-12 w-12 mx-auto mb-4 opacity-50" />
                <p className="font-medium">No nodes found</p>
                <p className="text-sm">Try adjusting your search or filter</p>
              </div>
            ) : (
              <ScrollArea className="h-[600px]">
                <Table>
                  <TableHeader>
                    <TableRow>
                      <TableHead>Node</TableHead>
                      <TableHead>Network</TableHead>
                      <TableHead>Server</TableHead>
                      <TableHead>Status</TableHead>
                      <TableHead>Block Height</TableHead>
                      <TableHead>Next Operation</TableHead>
                      <TableHead className="w-16">Actions</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {filteredNodes.map(node => {
                      const config = configs[node.node_name] || {} as NodeConfig;
                      const nextOp = getNextOperation(config);
                      
                      return (
                        <TableRow key={node.node_name}>
                          <TableCell>
                            <div className="flex flex-col gap-0.5">
                              <span className="font-medium">{formatName(node.node_name)}</span>
                              {config.auto_restore_enabled && (
                                <Badge variant="outline" className="w-fit text-xs">
                                  Auto restore
                                </Badge>
                              )}
                            </div>
                          </TableCell>
                          <TableCell>
                            <Badge variant="secondary" className="text-xs">
                              {node.network}
                            </Badge>
                          </TableCell>
                          <TableCell>
                            <span className="text-sm text-muted-foreground">{node.server_host}</span>
                          </TableCell>
                          <TableCell>
                            <Badge variant={getStatusVariant(node.status)}>
                              {node.status}
                            </Badge>
                          </TableCell>
                          <TableCell>
                            <span className="text-sm font-mono">
                              {formatBlockHeight(node.latest_block_height)}
                            </span>
                          </TableCell>
                          <TableCell>
                            {nextOp ? (
                              <Tooltip>
                                <TooltipTrigger asChild>
                                  <div className="flex items-center gap-1.5 text-sm cursor-help">
                                    <Clock className="h-3.5 w-3.5 text-muted-foreground" />
                                    <span>{nextOp.type}</span>
                                    <span className="text-muted-foreground">
                                      {formatNextRun(nextOp.schedule)}
                                    </span>
                                  </div>
                                </TooltipTrigger>
                                <TooltipContent>
                                  <p>Cron: {nextOp.schedule}</p>
                                </TooltipContent>
                              </Tooltip>
                            ) : (
                              <span className="text-sm text-muted-foreground">—</span>
                            )}
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
