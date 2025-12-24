import { 
  Card, 
  CardContent, 
  CardHeader, 
  CardTitle, 
  Progress, 
  Skeleton,
  Tooltip,
  TooltipContent,
  TooltipTrigger,
  TooltipProvider,
} from '@kostovster/ui';
import { Boxes, CheckCircle2, Clock, Server } from 'lucide-react';

interface MetricsGridProps {
  totalComponents: number;
  operationalComponents: number;
  nodesCount: number;
  hermesCount: number;
  etlCount: number;
  serverCount: number;
  isLoading?: boolean;
}

export function MetricsGrid({
  totalComponents,
  operationalComponents,
  nodesCount,
  hermesCount,
  etlCount,
  serverCount,
  isLoading = false,
}: MetricsGridProps) {
  const healthPercentage = totalComponents > 0 
    ? Math.round((operationalComponents / totalComponents) * 100) 
    : 0;

  if (isLoading) {
    return (
      <div className="grid grid-cols-4 gap-6 mb-8">
        {[...Array(4)].map((_, i) => (
          <Card key={i}>
            <CardHeader className="flex flex-row items-center justify-between pb-2">
              <Skeleton className="h-4 w-25" />
              <Skeleton className="h-5 w-5 rounded" />
            </CardHeader>
            <CardContent>
              <Skeleton className="h-8 w-15 mb-2" />
              <Skeleton className="h-4 w-30" />
            </CardContent>
          </Card>
        ))}
      </div>
    );
  }

  return (
    <TooltipProvider>
      <div className="grid grid-cols-4 gap-6 mb-8">
        <Card>
          <CardHeader className="flex flex-row items-center justify-between pb-2">
            <CardTitle className="text-xs font-semibold text-muted-foreground uppercase tracking-wider">
              Total Components
            </CardTitle>
            <Tooltip>
              <TooltipTrigger asChild>
                <Boxes className="h-5 w-5 text-primary cursor-help" />
              </TooltipTrigger>
              <TooltipContent>
                <p>All monitored infrastructure components</p>
              </TooltipContent>
            </Tooltip>
          </CardHeader>
          <CardContent>
            <div className="text-3xl font-bold">{totalComponents}</div>
            <p className="text-sm text-muted-foreground mt-1">
              {nodesCount} Nodes, {hermesCount} Relayers, {etlCount} ETL
            </p>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="flex flex-row items-center justify-between pb-2">
            <CardTitle className="text-xs font-semibold text-muted-foreground uppercase tracking-wider">
              Operational
            </CardTitle>
            <Tooltip>
              <TooltipTrigger asChild>
                <CheckCircle2 className="h-5 w-5 text-primary cursor-help" />
              </TooltipTrigger>
              <TooltipContent>
                <p>Components currently healthy and operational</p>
              </TooltipContent>
            </Tooltip>
          </CardHeader>
          <CardContent>
            <div className="text-3xl font-bold">{operationalComponents}</div>
            <p className="text-sm text-muted-foreground mt-1">
              {healthPercentage}% operational
            </p>
            <Progress value={healthPercentage} className="mt-3 h-1" />
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="flex flex-row items-center justify-between pb-2">
            <CardTitle className="text-xs font-semibold text-muted-foreground uppercase tracking-wider">
              Uptime
            </CardTitle>
            <Tooltip>
              <TooltipTrigger asChild>
                <Clock className="h-5 w-5 text-primary cursor-help" />
              </TooltipTrigger>
              <TooltipContent>
                <p>Current system availability percentage</p>
              </TooltipContent>
            </Tooltip>
          </CardHeader>
          <CardContent>
            <div className="text-3xl font-bold">{healthPercentage}%</div>
            <p className="text-sm text-muted-foreground mt-1">
              System availability
            </p>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="flex flex-row items-center justify-between pb-2">
            <CardTitle className="text-xs font-semibold text-muted-foreground uppercase tracking-wider">
              Servers
            </CardTitle>
            <Tooltip>
              <TooltipTrigger asChild>
                <Server className="h-5 w-5 text-primary cursor-help" />
              </TooltipTrigger>
              <TooltipContent>
                <p>Total infrastructure servers being managed</p>
              </TooltipContent>
            </Tooltip>
          </CardHeader>
          <CardContent>
            <div className="text-3xl font-bold">{serverCount}</div>
            <p className="text-sm text-muted-foreground mt-1">
              Infrastructure servers
            </p>
          </CardContent>
        </Card>
      </div>
    </TooltipProvider>
  );
}
