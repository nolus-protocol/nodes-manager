import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  Badge,
  Button,
  ScrollArea,
  Skeleton,
  Tooltip,
  TooltipContent,
  TooltipTrigger,
  cn,
} from '@kostovster/ui';
import { 
  AlertTriangle, 
  AlertCircle,
  Server,
  Boxes,
  Clock,
  Activity,
  ChevronRight,
} from 'lucide-react';
import { formatName, formatTimeAgo, formatBlockHeight } from '@/lib/utils';
import type { NodeHealth, HermesHealth, EtlHealth } from '@/types';

interface Issue {
  id: string;
  type: 'node' | 'hermes' | 'etl';
  name: string;
  server: string;
  status: string;
  severity: 'critical' | 'warning';
  message: string;
  details: string[];
  lastCheck: string;
}

interface IssuesPanelProps {
  nodes: NodeHealth[];
  hermes: HermesHealth[];
  etl: EtlHealth[];
  isLoading?: boolean;
  onNavigateToNodes?: () => void;
  onNavigateToServices?: () => void;
}

export function IssuesPanel({ 
  nodes, 
  hermes, 
  etl, 
  isLoading = false,
  onNavigateToNodes,
  onNavigateToServices,
}: IssuesPanelProps) {
  // Collect all issues
  const issues: Issue[] = [];

  // Check nodes
  nodes.forEach(node => {
    const status = node.status.toLowerCase();
    if (status === 'unhealthy' || status === 'maintenance') {
      const details: string[] = [];
      
      if (node.error_message) {
        details.push(node.error_message);
      }
      if (node.latest_block_height) {
        details.push(`Last block: ${formatBlockHeight(node.latest_block_height)}`);
      }
      if (node.catching_up) {
        details.push('Node is catching up');
      }
      if (node.latest_block_time) {
        details.push(`Block time: ${formatTimeAgo(node.latest_block_time)}`);
      }

      issues.push({
        id: `node-${node.node_name}`,
        type: 'node',
        name: node.node_name,
        server: node.server_host,
        status: node.status,
        severity: status === 'unhealthy' ? 'critical' : 'warning',
        message: node.error_message || `Node is ${status}`,
        details,
        lastCheck: node.last_check,
      });
    }
  });

  // Check Hermes
  hermes.forEach(h => {
    const status = h.status.toLowerCase().replace(/[()]/g, '');
    if (status.includes('stopped') || status.includes('failed') || !status.includes('running')) {
      const details: string[] = [];
      
      if (h.uptime) {
        details.push(`Last uptime: ${h.uptime}`);
      }
      details.push(`Server: ${h.server_host}`);

      issues.push({
        id: `hermes-${h.name}`,
        type: 'hermes',
        name: h.name,
        server: h.server_host,
        status: h.status,
        severity: 'critical',
        message: `Hermes relayer is ${status}`,
        details,
        lastCheck: h.last_check,
      });
    }
  });

  // Check ETL
  etl.forEach(e => {
    if (e.status.toLowerCase() === 'unhealthy') {
      const details: string[] = [];
      
      if (e.error_message) {
        details.push(e.error_message);
      }
      if (e.status_code) {
        details.push(`HTTP Status: ${e.status_code}`);
      }
      if (e.response_time_ms) {
        details.push(`Response time: ${e.response_time_ms}ms`);
      }
      details.push(`URL: ${e.service_url}`);

      issues.push({
        id: `etl-${e.service_name}`,
        type: 'etl',
        name: e.service_name,
        server: e.server_host,
        status: e.status,
        severity: 'critical',
        message: e.error_message || 'ETL service is unhealthy',
        details,
        lastCheck: e.last_check,
      });
    }
  });

  // Sort by severity (critical first) then by name
  issues.sort((a, b) => {
    if (a.severity !== b.severity) {
      return a.severity === 'critical' ? -1 : 1;
    }
    return a.name.localeCompare(b.name);
  });

  if (isLoading) {
    return (
      <Card>
        <CardHeader className="pb-3">
          <CardTitle className="flex items-center gap-2 text-base">
            <AlertTriangle className="h-5 w-5" />
            System Issues
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="space-y-3">
            {[...Array(2)].map((_, i) => (
              <div key={i} className="p-4 rounded-lg border">
                <div className="flex items-start gap-3">
                  <Skeleton className="h-5 w-5 rounded" />
                  <div className="flex-1 space-y-2">
                    <Skeleton className="h-4 w-32" />
                    <Skeleton className="h-3 w-48" />
                    <Skeleton className="h-3 w-40" />
                  </div>
                </div>
              </div>
            ))}
          </div>
        </CardContent>
      </Card>
    );
  }

  if (issues.length === 0) {
    return (
      <Card>
        <CardHeader className="pb-3">
          <CardTitle className="flex items-center gap-2 text-base">
            <Activity className="h-5 w-5 text-green-500" />
            System Status
          </CardTitle>
        </CardHeader>
        <CardContent>
          <p className="text-sm text-muted-foreground">
            All Systems Operational. No issues detected. All {nodes.length} nodes, {hermes.length} Hermes relayers, and {etl.length} ETL services are running normally.
          </p>
        </CardContent>
      </Card>
    );
  }

  const criticalCount = issues.filter(i => i.severity === 'critical').length;
  const warningCount = issues.filter(i => i.severity === 'warning').length;

  const getTypeIcon = (type: Issue['type']) => {
    switch (type) {
      case 'node': return <Boxes className="h-4 w-4" />;
      case 'hermes': return <Server className="h-4 w-4" />;
      case 'etl': return <Server className="h-4 w-4" />;
    }
  };

  const handleNavigate = (issue: Issue) => {
    if (issue.type === 'node' && onNavigateToNodes) {
      onNavigateToNodes();
    } else if ((issue.type === 'hermes' || issue.type === 'etl') && onNavigateToServices) {
      onNavigateToServices();
    }
  };

  return (
    <Card className="border-destructive/50">
      <CardHeader className="pb-3">
        <div className="flex items-center justify-between">
          <CardTitle className="flex items-center gap-2 text-base">
            <AlertTriangle className="h-5 w-5 text-destructive" />
            System Issues
          </CardTitle>
          <div className="flex items-center gap-2">
            {criticalCount > 0 && (
              <Badge variant="destructive" className="text-xs">
                {criticalCount} critical
              </Badge>
            )}
            {warningCount > 0 && (
              <Badge variant="secondary" className="text-xs">
                {warningCount} warning
              </Badge>
            )}
          </div>
        </div>
      </CardHeader>
      <CardContent className="p-0">
        <ScrollArea className="h-80">
          <div className="p-4 pt-0 space-y-3">
            {issues.map(issue => (
              <div
                key={issue.id}
                className={cn(
                  "p-4 rounded-lg border transition-colors",
                  issue.severity === 'critical' 
                    ? "border-destructive/50 bg-destructive/5" 
                    : "border-yellow-500/50 bg-yellow-500/5"
                )}
              >
                <div className="flex items-start justify-between gap-3">
                  <div className="flex items-start gap-3 flex-1 min-w-0">
                    <div className={cn(
                      "mt-0.5 p-1.5 rounded",
                      issue.severity === 'critical' ? "bg-destructive/20" : "bg-yellow-500/20"
                    )}>
                      {issue.severity === 'critical' 
                        ? <AlertCircle className="h-4 w-4 text-destructive" />
                        : <AlertTriangle className="h-4 w-4 text-yellow-500" />
                      }
                    </div>
                    <div className="flex-1 min-w-0">
                      <div className="flex items-center gap-2 flex-wrap">
                        <span className="font-medium">{formatName(issue.name)}</span>
                        <Badge variant="outline" className="text-xs capitalize">
                          {getTypeIcon(issue.type)}
                          <span className="ml-1">{issue.type}</span>
                        </Badge>
                        <Badge 
                          variant={issue.severity === 'critical' ? 'destructive' : 'secondary'} 
                          className="text-xs"
                        >
                          {issue.status}
                        </Badge>
                      </div>
                      <p className="text-sm text-muted-foreground mt-1 line-clamp-2">
                        {issue.message}
                      </p>
                      {issue.details.length > 0 && (
                        <div className="mt-2 space-y-1">
                          {issue.details.slice(0, 3).map((detail, idx) => (
                            <p key={idx} className="text-xs text-muted-foreground flex items-center gap-1">
                              <span className="w-1 h-1 rounded-full bg-muted-foreground/50" />
                              {detail}
                            </p>
                          ))}
                        </div>
                      )}
                      <div className="flex items-center gap-3 mt-2 text-xs text-muted-foreground">
                        <span className="flex items-center gap-1">
                          <Clock className="h-3 w-3" />
                          {formatTimeAgo(issue.lastCheck)}
                        </span>
                        <span>{issue.server}</span>
                      </div>
                    </div>
                  </div>
                  <Tooltip>
                    <TooltipTrigger asChild>
                      <Button 
                        variant="ghost" 
                        size="icon" 
                        className="shrink-0"
                        onClick={() => handleNavigate(issue)}
                      >
                        <ChevronRight className="h-4 w-4" />
                      </Button>
                    </TooltipTrigger>
                    <TooltipContent>
                      <p>View in {issue.type === 'node' ? 'Nodes' : 'Services'}</p>
                    </TooltipContent>
                  </Tooltip>
                </div>
              </div>
            ))}
          </div>
        </ScrollArea>
      </CardContent>
    </Card>
  );
}
