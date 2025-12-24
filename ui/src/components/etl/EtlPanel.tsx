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
  CopyableText,
  cn,
} from '@kostovster/ui';
import { ChevronDown, Database, RefreshCw, Search, ArrowUpDown, Clock } from 'lucide-react';
import { toast } from 'sonner';
import { refreshEtlService } from '@/api/client';
import { formatName, formatTimeAgo } from '@/lib/utils';
import type { EtlHealth, SortConfig } from '@/types';

// Status configuration for ETL
const etlStatusConfig: Record<string, 'default' | 'secondary' | 'destructive' | 'outline'> = {
  healthy: 'default',
  unhealthy: 'destructive',
};

interface EtlPanelProps {
  services: EtlHealth[];
  onRefresh: () => void;
  isLoading?: boolean;
}

export function EtlPanel({ services, onRefresh, isLoading = false }: EtlPanelProps) {
  const [isOpen, setIsOpen] = useState(() => {
    const saved = localStorage.getItem('etlPanel-open');
    return saved !== 'false';
  });
  const [search, setSearch] = useState(() => localStorage.getItem('etlSearch') || '');
  const [sort, setSort] = useState<SortConfig>(() => {
    try {
      return JSON.parse(localStorage.getItem('etlSort') || '{"column":"name","direction":"asc"}');
    } catch {
      return { column: 'name', direction: 'asc' };
    }
  });
  const [refreshingService, setRefreshingService] = useState<string | null>(null);

  useEffect(() => {
    localStorage.setItem('etlPanel-open', String(isOpen));
  }, [isOpen]);
  
  useEffect(() => {
    localStorage.setItem('etlSearch', search);
  }, [search]);
  
  useEffect(() => {
    localStorage.setItem('etlSort', JSON.stringify(sort));
  }, [sort]);

  const filteredAndSortedServices = useMemo(() => {
    let result = [...services];

    if (search) {
      const searchLower = search.toLowerCase();
      result = result.filter(s => 
        s.service_name.toLowerCase().includes(searchLower) ||
        s.server_host.toLowerCase().includes(searchLower) ||
        (s.description?.toLowerCase().includes(searchLower))
      );
    }

    result.sort((a, b) => {
      let comparison = 0;
      
      switch (sort.column) {
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
        case 'server':
          comparison = a.server_host.localeCompare(b.server_host);
          break;
        default:
          comparison = 0;
      }
      
      return sort.direction === 'asc' ? comparison : -comparison;
    });

    return result;
  }, [services, search, sort]);

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
    return etlStatusConfig[status.toLowerCase()] || 'secondary';
  };

  const handleRefreshService = async (serviceName: string) => {
    setRefreshingService(serviceName);
    try {
      await refreshEtlService(serviceName);
      toast.success(`${formatName(serviceName)} refreshed`);
      onRefresh();
    } catch (error) {
      toast.error(`Failed to refresh: ${error instanceof Error ? error.message : 'Unknown error'}`);
    } finally {
      setRefreshingService(null);
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
                  <Database className="h-5 w-5" />
                  <CardTitle>ETL Services</CardTitle>
                  <Badge variant="secondary" className="ml-2">
                    {services.length}
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
                  placeholder="Search ETL services by name..."
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
                      <Skeleton className="h-8 w-25" />
                      <Skeleton className="h-8 w-10" />
                    </div>
                  ))}
                </div>
              ) : filteredAndSortedServices.length === 0 ? (
                <div className="p-8 text-center text-muted-foreground">
                  <Database className="h-12 w-12 mx-auto mb-4 opacity-50" />
                  <p className="font-medium">No ETL services found</p>
                  <p className="text-sm">Try adjusting your search</p>
                </div>
              ) : (
                <Table>
                  <TableHeader>
                    <TableRow>
                      <SortableHeader column="name">Name / Server</SortableHeader>
                      <SortableHeader column="status">Status</SortableHeader>
                      <TableHead>Service URL</TableHead>
                      <TableHead>Last Check</TableHead>
                      <TableHead className="w-20">Actions</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {filteredAndSortedServices.map(etl => (
                      <TableRow key={etl.service_name}>
                        <TableCell>
                          <div className="flex flex-col gap-1">
                            <span className="font-medium">{formatName(etl.service_name)}</span>
                            <span className="text-xs text-muted-foreground">{etl.server_host}</span>
                            {etl.description && (
                              <span className="text-xs text-muted-foreground">{etl.description}</span>
                            )}
                          </div>
                        </TableCell>
                        <TableCell>
                          <div className="flex flex-col gap-1.5">
                            <Badge variant={getStatusVariant(etl.status)}>
                              {etl.status}
                            </Badge>
                            {etl.status_code && (
                              <span className="text-xs text-muted-foreground">
                                HTTP {etl.status_code}
                              </span>
                            )}
                            {etl.response_time_ms && (
                              <span className="text-xs text-muted-foreground flex items-center gap-1">
                                <Clock className="h-3 w-3" />
                                {etl.response_time_ms}ms
                              </span>
                            )}
                          </div>
                        </TableCell>
                        <TableCell>
                          <CopyableText 
                            text={etl.service_url}
                            truncate={true}
                            truncateStart={30}
                            truncateEnd={0}
                            className="font-mono text-xs"
                            onCopy={(success) => {
                              if (success) {
                                toast.success('URL copied to clipboard');
                              }
                            }}
                          />
                        </TableCell>
                        <TableCell>
                          <div className="flex flex-col gap-1">
                            <span className="text-sm">
                              {formatTimeAgo(etl.last_check)}
                            </span>
                            {etl.error_message && (
                              <Tooltip>
                                <TooltipTrigger asChild>
                                  <span className="text-xs text-destructive truncate max-w-36 cursor-help">
                                    {etl.error_message}
                                  </span>
                                </TooltipTrigger>
                                <TooltipContent side="left" className="max-w-75">
                                  <p>{etl.error_message}</p>
                                </TooltipContent>
                              </Tooltip>
                            )}
                          </div>
                        </TableCell>
                        <TableCell>
                          <Tooltip>
                            <TooltipTrigger asChild>
                              <Button
                                variant="outline"
                                size="icon"
                                onClick={() => handleRefreshService(etl.service_name)}
                                disabled={refreshingService === etl.service_name}
                              >
                                <RefreshCw className={cn(
                                  'h-4 w-4',
                                  refreshingService === etl.service_name && 'animate-spin'
                                )} />
                              </Button>
                            </TooltipTrigger>
                            <TooltipContent>
                              <p>Refresh service health</p>
                            </TooltipContent>
                          </Tooltip>
                        </TableCell>
                      </TableRow>
                    ))}
                  </TableBody>
                </Table>
              )}
            </CardContent>
          </CollapsibleContent>
        </Card>
      </Collapsible>
    </TooltipProvider>
  );
}
