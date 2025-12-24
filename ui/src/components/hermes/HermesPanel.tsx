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
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
  Input,
  Tooltip,
  TooltipContent,
  TooltipTrigger,
  TooltipProvider,
  Skeleton,
  cn,
} from '@kostovster/ui';
import { ChevronDown, Network, RefreshCw, RotateCcw, Search, ArrowUpDown } from 'lucide-react';
import { toast } from 'sonner';
import { ScheduleItem, NextRunItem } from '@/components/shared/CronDisplay';
import { ConfirmDialog } from '@/components/shared/ConfirmDialog';
import { restartHermes } from '@/api/client';
import { formatName } from '@/lib/utils';
import { getCronSortValue } from '@/lib/cron';
import type { HermesHealth, HermesConfig, SortConfig } from '@/types';

interface HermesPanelProps {
  instances: HermesHealth[];
  configs: Record<string, HermesConfig>;
  onRefresh: () => void;
  isLoading?: boolean;
}

export function HermesPanel({ instances, configs, onRefresh, isLoading = false }: HermesPanelProps) {
  const [isOpen, setIsOpen] = useState(() => {
    const saved = localStorage.getItem('hermesPanel-open');
    return saved !== 'false';
  });
  const [search, setSearch] = useState(() => localStorage.getItem('hermesSearch') || '');
  const [sort, setSort] = useState<SortConfig>(() => {
    try {
      return JSON.parse(localStorage.getItem('hermesSort') || '{"column":"next","direction":"asc"}');
    } catch {
      return { column: 'next', direction: 'asc' };
    }
  });
  const [confirmRestart, setConfirmRestart] = useState<string | null>(null);
  const [isRestarting, setIsRestarting] = useState(false);

  useEffect(() => {
    localStorage.setItem('hermesPanel-open', String(isOpen));
  }, [isOpen]);
  
  useEffect(() => {
    localStorage.setItem('hermesSearch', search);
  }, [search]);
  
  useEffect(() => {
    localStorage.setItem('hermesSort', JSON.stringify(sort));
  }, [sort]);

  const filteredAndSortedInstances = useMemo(() => {
    let result = [...instances];

    if (search) {
      const searchLower = search.toLowerCase();
      result = result.filter(h => 
        h.name.toLowerCase().includes(searchLower) ||
        h.server_host.toLowerCase().includes(searchLower)
      );
    }

    result.sort((a, b) => {
      const configA = configs[a.name] || {};
      const configB = configs[b.name] || {};
      
      let comparison = 0;
      
      switch (sort.column) {
        case 'name':
          comparison = a.name.localeCompare(b.name);
          break;
        case 'status': {
          const statusOrder: Record<string, number> = { 'stopped': 0, 'failed': 0, 'running': 1 };
          const statusA = statusOrder[a.status.toLowerCase().replace(/[()]/g, '')] ?? 2;
          const statusB = statusOrder[b.status.toLowerCase().replace(/[()]/g, '')] ?? 2;
          comparison = statusA - statusB;
          break;
        }
        case 'next':
          comparison = getCronSortValue(configA.restart_schedule) - getCronSortValue(configB.restart_schedule);
          break;
        default:
          comparison = 0;
      }
      
      return sort.direction === 'asc' ? comparison : -comparison;
    });

    return result;
  }, [instances, configs, search, sort]);

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
    const clean = status.toLowerCase().replace(/[()]/g, '');
    if (clean.includes('running')) return 'default';
    if (clean.includes('stopped') || clean.includes('failed')) return 'destructive';
    return 'secondary';
  };

  const handleRestart = async () => {
    if (!confirmRestart) return;

    setIsRestarting(true);
    try {
      const response = await restartHermes(confirmRestart);
      if (response.success) {
        toast.success(`${formatName(confirmRestart)} restarted successfully`);
        onRefresh();
      } else {
        throw new Error(response.message || 'Restart failed');
      }
    } catch (error) {
      toast.error(`Failed to restart: ${error instanceof Error ? error.message : 'Unknown error'}`);
    } finally {
      setIsRestarting(false);
      setConfirmRestart(null);
    }
  };

  return (
    <TooltipProvider>
      <Collapsible open={isOpen} onOpenChange={setIsOpen}>
        <Card>
          <CardHeader className="bg-muted/50">
            <div className="flex items-center justify-between">
              <CollapsibleTrigger asChild>
                <button className="flex items-center gap-2 hover:opacity-80 transition-opacity">
                  <ChevronDown className={cn(
                    'h-5 w-5 transition-transform duration-200',
                    !isOpen && '-rotate-90'
                  )} />
                  <Network className="h-5 w-5" />
                  <CardTitle>Hermes Relayers</CardTitle>
                  <Badge variant="secondary" className="ml-2">
                    {instances.length}
                  </Badge>
                </button>
              </CollapsibleTrigger>
              <Button variant="outline" size="sm" onClick={onRefresh} disabled={isLoading}>
                <RefreshCw className={cn("h-4 w-4 mr-2", isLoading && "animate-spin")} />
                Refresh
              </Button>
            </div>
          </CardHeader>
          
          <CollapsibleContent>
            <div className="p-4 border-b">
              <div className="relative flex-1 min-w-50">
                <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground" />
                <Input
                  type="text"
                  value={search}
                  onChange={(e) => setSearch(e.target.value)}
                  placeholder="Search Hermes by name or server..."
                  className="pl-9"
                />
              </div>
            </div>
            
            <CardContent className="p-0">
              {isLoading ? (
                <div className="p-4 space-y-4">
                  {[...Array(2)].map((_, i) => (
                    <div key={i} className="flex items-center gap-4">
                      <Skeleton className="h-12 w-50" />
                      <Skeleton className="h-8 w-25" />
                      <Skeleton className="h-8 flex-1" />
                      <Skeleton className="h-8 w-36" />
                      <Skeleton className="h-8 w-10" />
                    </div>
                  ))}
                </div>
              ) : filteredAndSortedInstances.length === 0 ? (
                <div className="p-8 text-center text-muted-foreground">
                  <Network className="h-12 w-12 mx-auto mb-4 opacity-50" />
                  <p className="font-medium">No Hermes relayers found</p>
                  <p className="text-sm">Try adjusting your search</p>
                </div>
              ) : (
                <Table>
                  <TableHeader>
                    <TableRow>
                      <SortableHeader column="name">Name / Server</SortableHeader>
                      <SortableHeader column="status">Status</SortableHeader>
                      <TableHead>Schedule</TableHead>
                      <SortableHeader column="next">Next Run</SortableHeader>
                      <TableHead className="w-20">Actions</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {filteredAndSortedInstances.map(hermes => {
                      const config = configs[hermes.name] || {} as HermesConfig;
                      return (
                        <TableRow key={hermes.name}>
                          <TableCell>
                            <div className="flex flex-col gap-1">
                              <span className="font-medium">{formatName(hermes.name)}</span>
                              <span className="text-xs text-muted-foreground">{hermes.server_host}</span>
                            </div>
                          </TableCell>
                          <TableCell>
                            <Badge variant={getStatusVariant(hermes.status)}>
                              {hermes.status.replace(/[()]/g, '')}
                            </Badge>
                          </TableCell>
                          <TableCell>
                            <ScheduleItem 
                              label="Restart Schedule" 
                              schedule={config.restart_schedule} 
                            />
                          </TableCell>
                          <TableCell>
                            <NextRunItem 
                              label="Next Restart" 
                              schedule={config.restart_schedule} 
                            />
                          </TableCell>
                          <TableCell>
                            <Tooltip>
                              <TooltipTrigger asChild>
                                <Button
                                  variant="outline"
                                  size="icon"
                                  onClick={() => setConfirmRestart(hermes.name)}
                                >
                                  <RotateCcw className="h-4 w-4" />
                                </Button>
                              </TooltipTrigger>
                              <TooltipContent>
                                <p>Restart Hermes relayer</p>
                              </TooltipContent>
                            </Tooltip>
                          </TableCell>
                        </TableRow>
                      );
                    })}
                  </TableBody>
                </Table>
              )}
            </CardContent>
          </CollapsibleContent>
        </Card>
      </Collapsible>

      {confirmRestart && (
        <ConfirmDialog
          open={!!confirmRestart}
          onOpenChange={(open) => !open && !isRestarting && setConfirmRestart(null)}
          title="Restart Hermes"
          description={`Restart ${formatName(confirmRestart)}?\n\nThis will stop and start the Hermes relayer service. The relayer will be temporarily unavailable during restart.\n\nProceed with restart?`}
          confirmText={isRestarting ? 'Restarting...' : 'Restart'}
          onConfirm={handleRestart}
        />
      )}
    </TooltipProvider>
  );
}
