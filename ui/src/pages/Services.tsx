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
  Separator,
  CopyableText,
  cn,
} from '@kostovster/ui';
import { 
  Network, 
  Database,
  RefreshCw, 
  Search, 
  ArrowUpDown,
  RotateCcw,
  Clock,
} from 'lucide-react';
import { toast } from 'sonner';
import { ScheduleItem, NextRunItem } from '@/components/shared/CronDisplay';
import { ConfirmDialog } from '@/components/shared/ConfirmDialog';
import { restartHermes, refreshEtlService } from '@/api/client';
import { formatName, formatTimeAgo } from '@/lib/utils';
import { getCronSortValue } from '@/lib/cron';
import type { HermesHealth, HermesConfig, EtlHealth, SortConfig } from '@/types';

interface ServicesPageProps {
  hermes: HermesHealth[];
  hermesConfigs: Record<string, HermesConfig>;
  etl: EtlHealth[];
  onRefresh: () => void;
  isLoading?: boolean;
}

export function ServicesPage({ 
  hermes, 
  hermesConfigs, 
  etl, 
  onRefresh, 
  isLoading = false 
}: ServicesPageProps) {
  const [hermesSearch, setHermesSearch] = useState('');
  const [etlSearch, setEtlSearch] = useState('');
  const [hermesSort, setHermesSort] = useState<SortConfig>({ column: 'name', direction: 'asc' });
  const [etlSort, setEtlSort] = useState<SortConfig>({ column: 'name', direction: 'asc' });
  const [confirmRestart, setConfirmRestart] = useState<string | null>(null);
  const [isRestarting, setIsRestarting] = useState(false);
  const [refreshingEtl, setRefreshingEtl] = useState<string | null>(null);

  // Hermes filtering and sorting
  const filteredHermes = useMemo(() => {
    let result = [...hermes];

    if (hermesSearch) {
      const searchLower = hermesSearch.toLowerCase();
      result = result.filter(h => 
        h.name.toLowerCase().includes(searchLower) ||
        h.server_host.toLowerCase().includes(searchLower)
      );
    }

    result.sort((a, b) => {
      const configA = hermesConfigs[a.name] || {};
      const configB = hermesConfigs[b.name] || {};
      
      let comparison = 0;
      
      switch (hermesSort.column) {
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
      
      return hermesSort.direction === 'asc' ? comparison : -comparison;
    });

    return result;
  }, [hermes, hermesConfigs, hermesSearch, hermesSort]);

  // ETL filtering and sorting
  const filteredEtl = useMemo(() => {
    let result = [...etl];

    if (etlSearch) {
      const searchLower = etlSearch.toLowerCase();
      result = result.filter(s => 
        s.service_name.toLowerCase().includes(searchLower) ||
        s.server_host.toLowerCase().includes(searchLower) ||
        s.description?.toLowerCase().includes(searchLower)
      );
    }

    result.sort((a, b) => {
      let comparison = 0;
      
      switch (etlSort.column) {
        case 'name':
          comparison = a.service_name.localeCompare(b.service_name);
          break;
        case 'status': {
          const statusOrder: Record<string, number> = { 'unhealthy': 0, 'healthy': 1 };
          const statusA = statusOrder[a.status.toLowerCase()] ?? 2;
          const statusB = statusOrder[b.status.toLowerCase()] ?? 2;
          comparison = statusA - statusB;
          break;
        }
        case 'response':
          comparison = (a.response_time_ms || 0) - (b.response_time_ms || 0);
          break;
        default:
          comparison = 0;
      }
      
      return etlSort.direction === 'asc' ? comparison : -comparison;
    });

    return result;
  }, [etl, etlSearch, etlSort]);

  const handleHermesSort = (column: string) => {
    setHermesSort(prev => ({
      column,
      direction: prev.column === column && prev.direction === 'asc' ? 'desc' : 'asc',
    }));
  };

  const handleEtlSort = (column: string) => {
    setEtlSort(prev => ({
      column,
      direction: prev.column === column && prev.direction === 'asc' ? 'desc' : 'asc',
    }));
  };

  const HermesSortableHeader = ({ column, children }: { column: string; children: React.ReactNode }) => (
    <TableHead 
      className="cursor-pointer hover:bg-muted/50 transition-colors"
      onClick={() => handleHermesSort(column)}
    >
      <div className="flex items-center gap-1">
        {children}
        <ArrowUpDown className={cn(
          "h-3 w-3 text-muted-foreground transition-opacity",
          hermesSort.column === column ? "opacity-100" : "opacity-0"
        )} />
      </div>
    </TableHead>
  );

  const EtlSortableHeader = ({ column, children }: { column: string; children: React.ReactNode }) => (
    <TableHead 
      className="cursor-pointer hover:bg-muted/50 transition-colors"
      onClick={() => handleEtlSort(column)}
    >
      <div className="flex items-center gap-1">
        {children}
        <ArrowUpDown className={cn(
          "h-3 w-3 text-muted-foreground transition-opacity",
          etlSort.column === column ? "opacity-100" : "opacity-0"
        )} />
      </div>
    </TableHead>
  );

  const getHermesStatusVariant = (status: string) => {
    const clean = status.toLowerCase().replace(/[()]/g, '');
    if (clean.includes('running')) return 'default';
    if (clean.includes('stopped') || clean.includes('failed')) return 'destructive';
    return 'secondary';
  };

  const getEtlStatusVariant = (status: string) => {
    if (status.toLowerCase() === 'healthy') return 'default';
    if (status.toLowerCase() === 'unhealthy') return 'destructive';
    return 'secondary';
  };

  const handleHermesRestart = async () => {
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

  const handleEtlRefresh = async (serviceName: string) => {
    setRefreshingEtl(serviceName);
    try {
      await refreshEtlService(serviceName);
      toast.success(`${formatName(serviceName)} refreshed`);
      onRefresh();
    } catch (error) {
      toast.error(`Failed to refresh: ${error instanceof Error ? error.message : 'Unknown error'}`);
    } finally {
      setRefreshingEtl(null);
    }
  };

  return (
    <TooltipProvider>
      <div className="space-y-8">
        {/* Page Header */}
        <div className="flex items-center justify-end">
          <Button onClick={onRefresh} disabled={isLoading}>
            <RefreshCw className={cn("h-4 w-4 mr-2", isLoading && "animate-spin")} />
            Refresh All
          </Button>
        </div>

        {/* Hermes Section */}
        <Card>
          <CardHeader>
            <div className="flex items-center justify-between">
              <CardTitle className="flex items-center gap-2">
                <Network className="h-5 w-5" />
                Hermes Relayers
                <Badge variant="secondary">{hermes.length}</Badge>
              </CardTitle>
              <div className="relative w-64">
                <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground" />
                <Input
                  type="text"
                  value={hermesSearch}
                  onChange={(e) => setHermesSearch(e.target.value)}
                  placeholder="Search Hermes..."
                  className="pl-9"
                />
              </div>
            </div>
          </CardHeader>
          <CardContent className="p-0">
            {isLoading ? (
              <div className="p-6 space-y-4">
                {[...Array(3)].map((_, i) => (
                  <div key={i} className="flex items-center gap-4">
                    <Skeleton className="h-12 w-50" />
                    <Skeleton className="h-8 w-25" />
                    <Skeleton className="h-8 flex-1" />
                    <Skeleton className="h-8 w-10" />
                  </div>
                ))}
              </div>
            ) : filteredHermes.length === 0 ? (
              <div className="p-12 text-center text-muted-foreground">
                <Network className="h-12 w-12 mx-auto mb-4 opacity-50" />
                <p className="font-medium">No Hermes relayers found</p>
                <p className="text-sm">Try adjusting your search</p>
              </div>
            ) : (
              <ScrollArea className="h-80">
                <Table>
                  <TableHeader>
                    <TableRow>
                      <HermesSortableHeader column="name">Name / Server</HermesSortableHeader>
                      <HermesSortableHeader column="status">Status</HermesSortableHeader>
                      <TableHead>Schedule</TableHead>
                      <HermesSortableHeader column="next">Next Run</HermesSortableHeader>
                      <TableHead className="w-20">Actions</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {filteredHermes.map(h => {
                      const config = hermesConfigs[h.name] || {} as HermesConfig;
                      return (
                        <TableRow key={h.name}>
                          <TableCell>
                            <div className="flex flex-col gap-1">
                              <span className="font-medium">{formatName(h.name)}</span>
                              <span className="text-xs text-muted-foreground">{h.server_host}</span>
                            </div>
                          </TableCell>
                          <TableCell>
                            <Badge variant={getHermesStatusVariant(h.status)}>
                              {h.status.replace(/[()]/g, '')}
                            </Badge>
                          </TableCell>
                          <TableCell>
                            <ScheduleItem 
                              label="Restart" 
                              schedule={config.restart_schedule} 
                            />
                          </TableCell>
                          <TableCell>
                            <NextRunItem 
                              label="Restart" 
                              schedule={config.restart_schedule} 
                            />
                          </TableCell>
                          <TableCell>
                            <Tooltip>
                              <TooltipTrigger asChild>
                                <Button
                                  variant="outline"
                                  size="icon"
                                  onClick={() => setConfirmRestart(h.name)}
                                >
                                  <RotateCcw className="h-4 w-4" />
                                </Button>
                              </TooltipTrigger>
                              <TooltipContent>
                                <p>Restart Hermes</p>
                              </TooltipContent>
                            </Tooltip>
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

        <Separator />

        {/* ETL Section */}
        <Card>
          <CardHeader>
            <div className="flex items-center justify-between">
              <CardTitle className="flex items-center gap-2">
                <Database className="h-5 w-5" />
                ETL Services
                <Badge variant="secondary">{etl.length}</Badge>
              </CardTitle>
              <div className="relative w-64">
                <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground" />
                <Input
                  type="text"
                  value={etlSearch}
                  onChange={(e) => setEtlSearch(e.target.value)}
                  placeholder="Search ETL services..."
                  className="pl-9"
                />
              </div>
            </div>
          </CardHeader>
          <CardContent className="p-0">
            {isLoading ? (
              <div className="p-6 space-y-4">
                {[...Array(3)].map((_, i) => (
                  <div key={i} className="flex items-center gap-4">
                    <Skeleton className="h-12 w-50" />
                    <Skeleton className="h-8 w-25" />
                    <Skeleton className="h-8 flex-1" />
                    <Skeleton className="h-8 w-10" />
                  </div>
                ))}
              </div>
            ) : filteredEtl.length === 0 ? (
              <div className="p-12 text-center text-muted-foreground">
                <Database className="h-12 w-12 mx-auto mb-4 opacity-50" />
                <p className="font-medium">No ETL services found</p>
                <p className="text-sm">Try adjusting your search</p>
              </div>
            ) : (
              <ScrollArea className="h-80">
                <Table>
                  <TableHeader>
                    <TableRow>
                      <EtlSortableHeader column="name">Name / Server</EtlSortableHeader>
                      <EtlSortableHeader column="status">Status</EtlSortableHeader>
                      <TableHead>Service URL</TableHead>
                      <EtlSortableHeader column="response">Response Time</EtlSortableHeader>
                      <TableHead className="w-20">Actions</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {filteredEtl.map(e => (
                      <TableRow key={e.service_name}>
                        <TableCell>
                          <div className="flex flex-col gap-1">
                            <span className="font-medium">{formatName(e.service_name)}</span>
                            <span className="text-xs text-muted-foreground">{e.server_host}</span>
                            {e.description && (
                              <span className="text-xs text-muted-foreground">{e.description}</span>
                            )}
                          </div>
                        </TableCell>
                        <TableCell>
                          <div className="flex flex-col gap-1.5">
                            <Badge variant={getEtlStatusVariant(e.status)}>
                              {e.status}
                            </Badge>
                            {e.status_code && (
                              <span className="text-xs text-muted-foreground">
                                HTTP {e.status_code}
                              </span>
                            )}
                          </div>
                        </TableCell>
                        <TableCell>
                          <CopyableText 
                            text={e.service_url}
                            truncate={true}
                            truncateStart={35}
                            truncateEnd={0}
                            className="font-mono text-xs"
                            onCopy={(success) => {
                              if (success) {
                                toast.success('URL copied');
                              }
                            }}
                          />
                        </TableCell>
                        <TableCell>
                          <div className="flex items-center gap-1 text-sm">
                            <Clock className="h-3 w-3 text-muted-foreground" />
                            {e.response_time_ms ? `${e.response_time_ms}ms` : 'N/A'}
                          </div>
                          <span className="text-xs text-muted-foreground">
                            {formatTimeAgo(e.last_check)}
                          </span>
                        </TableCell>
                        <TableCell>
                          <Tooltip>
                            <TooltipTrigger asChild>
                              <Button
                                variant="outline"
                                size="icon"
                                onClick={() => handleEtlRefresh(e.service_name)}
                                disabled={refreshingEtl === e.service_name}
                              >
                                <RefreshCw className={cn(
                                  'h-4 w-4',
                                  refreshingEtl === e.service_name && 'animate-spin'
                                )} />
                              </Button>
                            </TooltipTrigger>
                            <TooltipContent>
                              <p>Refresh service</p>
                            </TooltipContent>
                          </Tooltip>
                        </TableCell>
                      </TableRow>
                    ))}
                  </TableBody>
                </Table>
              </ScrollArea>
            )}
          </CardContent>
        </Card>
      </div>

      {/* Confirm Dialog */}
      {confirmRestart && (
        <ConfirmDialog
          open={!!confirmRestart}
          onOpenChange={(open) => !open && !isRestarting && setConfirmRestart(null)}
          title="Restart Hermes"
          description={`Restart ${formatName(confirmRestart)}?\n\nThis will stop and start the Hermes relayer service. The relayer will be temporarily unavailable during restart.`}
          confirmText={isRestarting ? 'Restarting...' : 'Restart'}
          onConfirm={handleHermesRestart}
        />
      )}
    </TooltipProvider>
  );
}
