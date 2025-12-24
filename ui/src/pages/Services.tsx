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
  Tabs,
  TabsList,
  TabsTrigger,
  cn,
} from '@kostovster/ui';
import { 
  Server,
  RefreshCw, 
  Search, 
  RotateCcw,
  Network,
  Database,
} from 'lucide-react';
import { toast } from 'sonner';
import { ConfirmDialog } from '@/components/shared/ConfirmDialog';
import { restartHermes, refreshEtlService } from '@/api/client';
import { formatName, formatTimeAgo } from '@/lib/utils';
import type { HermesHealth, HermesConfig, EtlHealth } from '@/types';

interface ServiceItem {
  id: string;
  name: string;
  type: 'hermes' | 'etl';
  server: string;
  status: string;
  statusVariant: 'default' | 'secondary' | 'destructive';
  lastCheck: string;
  details?: string;
}

interface ServicesPageProps {
  hermes: HermesHealth[];
  hermesConfigs: Record<string, HermesConfig>;
  etl: EtlHealth[];
  onRefresh: () => void;
  isLoading?: boolean;
}

export function ServicesPage({ 
  hermes, 
  hermesConfigs: _hermesConfigs, 
  etl, 
  onRefresh, 
  isLoading = false 
}: ServicesPageProps) {
  const [search, setSearch] = useState('');
  const [typeFilter, setTypeFilter] = useState<'all' | 'hermes' | 'etl'>('all');
  const [confirmRestart, setConfirmRestart] = useState<string | null>(null);
  const [isRestarting, setIsRestarting] = useState(false);
  const [refreshingService, setRefreshingService] = useState<string | null>(null);

  // Combine all services into unified list
  const allServices: ServiceItem[] = useMemo(() => {
    const hermesServices: ServiceItem[] = hermes.map(h => {
      const statusClean = h.status.toLowerCase().replace(/[()]/g, '');
      let statusVariant: 'default' | 'secondary' | 'destructive' = 'secondary';
      if (statusClean.includes('running')) statusVariant = 'default';
      if (statusClean.includes('stopped') || statusClean.includes('failed')) statusVariant = 'destructive';
      
      return {
        id: `hermes-${h.name}`,
        name: h.name,
        type: 'hermes' as const,
        server: h.server_host,
        status: h.status.replace(/[()]/g, ''),
        statusVariant,
        lastCheck: h.last_check,
        details: h.uptime ? `Uptime: ${h.uptime}` : undefined,
      };
    });

    const etlServices: ServiceItem[] = etl.map(e => {
      const statusVariant = e.status.toLowerCase() === 'healthy' ? 'default' : 'destructive';
      const details = [
        e.status_code ? `HTTP ${e.status_code}` : null,
        e.response_time_ms ? `${e.response_time_ms}ms` : null,
      ].filter(Boolean).join(', ');
      
      return {
        id: `etl-${e.service_name}`,
        name: e.service_name,
        type: 'etl' as const,
        server: e.server_host,
        status: e.status,
        statusVariant,
        lastCheck: e.last_check,
        details: details || undefined,
      };
    });

    return [...hermesServices, ...etlServices];
  }, [hermes, etl]);

  // Filter services
  const filteredServices = useMemo(() => {
    let result = [...allServices];

    if (search) {
      const searchLower = search.toLowerCase();
      result = result.filter(s => 
        (s.name || '').toLowerCase().includes(searchLower) ||
        (s.server || '').toLowerCase().includes(searchLower)
      );
    }

    if (typeFilter !== 'all') {
      result = result.filter(s => s.type === typeFilter);
    }

    // Sort by type then name
    result.sort((a, b) => {
      if (a.type !== b.type) return (a.type || '').localeCompare(b.type || '');
      return (a.name || '').localeCompare(b.name || '');
    });

    return result;
  }, [allServices, search, typeFilter]);

  const counts = useMemo(() => ({
    all: allServices.length,
    hermes: hermes.length,
    etl: etl.length,
  }), [allServices.length, hermes.length, etl.length]);

  const handleAction = async (service: ServiceItem) => {
    if (service.type === 'hermes') {
      setConfirmRestart(service.name);
    } else {
      setRefreshingService(service.id);
      try {
        await refreshEtlService(service.name);
        toast.success(`${formatName(service.name)} refreshed`);
        onRefresh();
      } catch (error) {
        toast.error(`Failed to refresh: ${error instanceof Error ? error.message : 'Unknown error'}`);
      } finally {
        setRefreshingService(null);
      }
    }
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

  const TypeIcon = ({ type }: { type: 'hermes' | 'etl' }) => {
    return type === 'hermes' 
      ? <Network className="h-4 w-4 text-muted-foreground" />
      : <Database className="h-4 w-4 text-muted-foreground" />;
  };

  return (
    <TooltipProvider>
      <div className="space-y-6">
        {/* Services Table */}
        <Card>
          <CardHeader className="pb-4">
            <div className="flex items-center justify-between">
              <CardTitle className="flex items-center gap-2">
                <Server className="h-5 w-5" />
                Services
                <Badge variant="outline">{filteredServices.length}</Badge>
              </CardTitle>
              <Button variant="outline" size="sm" onClick={onRefresh} disabled={isLoading}>
                <RefreshCw className={cn("h-4 w-4", isLoading && "animate-spin")} />
              </Button>
            </div>
            {/* Filters */}
            <div className="flex items-center gap-4 mt-4">
              <div className="relative flex-1 max-w-sm">
                <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground" />
                <Input
                  type="text"
                  value={search}
                  onChange={(e) => setSearch(e.target.value)}
                  placeholder="Search services..."
                  className="pl-9"
                />
              </div>
              <Tabs value={typeFilter} onValueChange={(v) => setTypeFilter(v as 'all' | 'hermes' | 'etl')}>
                <TabsList>
                  {(['all', 'hermes', 'etl'] as const).map(type => (
                    <TabsTrigger key={type} value={type}>
                      {type === 'all' ? 'All' : type === 'hermes' ? 'Hermes' : 'ETL'}
                      <Badge variant="secondary" className="ml-1.5 text-xs">
                        {counts[type]}
                      </Badge>
                    </TabsTrigger>
                  ))}
                </TabsList>
              </Tabs>
            </div>
          </CardHeader>
          <CardContent className="p-0">
            {isLoading ? (
              <div className="p-6 space-y-4">
                {[...Array(5)].map((_, i) => (
                  <div key={i} className="flex items-center gap-4">
                    <Skeleton className="h-10 w-10 rounded" />
                    <Skeleton className="h-5 w-32" />
                    <Skeleton className="h-5 w-20" />
                    <Skeleton className="h-5 flex-1" />
                    <Skeleton className="h-8 w-8" />
                  </div>
                ))}
              </div>
            ) : filteredServices.length === 0 ? (
              <div className="p-12 text-center text-muted-foreground">
                <Server className="h-12 w-12 mx-auto mb-4 opacity-50" />
                <p className="font-medium">No services found</p>
                <p className="text-sm">Try adjusting your search or filter</p>
              </div>
            ) : (
              <ScrollArea className="h-125">
                <Table>
                  <TableHeader>
                    <TableRow>
                      <TableHead className="w-12"></TableHead>
                      <TableHead>Name</TableHead>
                      <TableHead>Type</TableHead>
                      <TableHead>Server</TableHead>
                      <TableHead>Status</TableHead>
                      <TableHead>Last Check</TableHead>
                      <TableHead className="w-16">Actions</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {filteredServices.map(service => (
                      <TableRow key={service.id}>
                        <TableCell>
                          <TypeIcon type={service.type} />
                        </TableCell>
                        <TableCell>
                          <span className="font-medium">{formatName(service.name)}</span>
                        </TableCell>
                        <TableCell>
                          <Badge variant="outline" className="text-xs capitalize">
                            {service.type}
                          </Badge>
                        </TableCell>
                        <TableCell>
                          <span className="text-sm text-muted-foreground">{service.server}</span>
                        </TableCell>
                        <TableCell>
                          <div className="flex flex-col gap-1">
                            <Badge variant={service.statusVariant}>
                              {service.status}
                            </Badge>
                            {service.details && (
                              <span className="text-xs text-muted-foreground">
                                {service.details}
                              </span>
                            )}
                          </div>
                        </TableCell>
                        <TableCell>
                          <span className="text-sm text-muted-foreground">
                            {formatTimeAgo(service.lastCheck)}
                          </span>
                        </TableCell>
                        <TableCell>
                          <Tooltip>
                            <TooltipTrigger asChild>
                              <Button
                                variant="ghost"
                                size="icon"
                                onClick={() => handleAction(service)}
                                disabled={refreshingService === service.id}
                              >
                                {service.type === 'hermes' ? (
                                  <RotateCcw className="h-4 w-4" />
                                ) : (
                                  <RefreshCw className={cn(
                                    'h-4 w-4',
                                    refreshingService === service.id && 'animate-spin'
                                  )} />
                                )}
                              </Button>
                            </TooltipTrigger>
                            <TooltipContent>
                              <p>{service.type === 'hermes' ? 'Restart' : 'Refresh'}</p>
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
          description={`Restart ${formatName(confirmRestart)}?\n\nThis will stop and start the Hermes relayer service.`}
          confirmText={isRestarting ? 'Restarting...' : 'Restart'}
          onConfirm={handleHermesRestart}
        />
      )}
    </TooltipProvider>
  );
}
