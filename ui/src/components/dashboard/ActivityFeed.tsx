import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  Badge,
  ScrollArea,
  Skeleton,
  cn,
} from '@kostovster/ui';
import { 
  Activity, 
  Scissors, 
  Camera, 
  RotateCcw, 
  RefreshCw,
  CheckCircle2,
  XCircle,
  Clock,
  Loader2,
} from 'lucide-react';
import { formatTimeAgo, formatName } from '@/lib/utils';

export interface ActivityItem {
  id: string;
  operation_type: string;
  target_name: string;
  status: 'completed' | 'failed' | 'in_progress' | 'pending';
  started_at: string;
  completed_at?: string;
  error_message?: string;
}

interface ActivityFeedProps {
  activities: ActivityItem[];
  isLoading?: boolean;
  maxHeight?: string;
}

const operationIcons: Record<string, typeof Scissors> = {
  pruning: Scissors,
  snapshot: Camera,
  restore: RotateCcw,
  state_sync: RefreshCw,
  restart: RotateCcw,
};

const statusConfig: Record<string, { icon: typeof CheckCircle2; variant: 'default' | 'secondary' | 'destructive' | 'outline'; color: string }> = {
  completed: { icon: CheckCircle2, variant: 'default', color: 'text-green-500' },
  failed: { icon: XCircle, variant: 'destructive', color: 'text-red-500' },
  in_progress: { icon: Loader2, variant: 'secondary', color: 'text-blue-500' },
  pending: { icon: Clock, variant: 'outline', color: 'text-muted-foreground' },
};

export function ActivityFeed({ activities, isLoading = false, maxHeight = '400px' }: ActivityFeedProps) {
  if (isLoading) {
    return (
      <Card>
        <CardHeader className="pb-3">
          <div className="flex items-center justify-between">
            <CardTitle className="flex items-center gap-2 text-base">
              <Activity className="h-5 w-5" />
              Recent Activity
            </CardTitle>
          </div>
        </CardHeader>
        <CardContent>
          <div className="space-y-4">
            {[...Array(5)].map((_, i) => (
              <div key={i} className="flex items-start gap-3">
                <Skeleton className="h-8 w-8 rounded-full" />
                <div className="flex-1 space-y-2">
                  <Skeleton className="h-4 w-48" />
                  <Skeleton className="h-3 w-24" />
                </div>
                <Skeleton className="h-5 w-16 rounded-full" />
              </div>
            ))}
          </div>
        </CardContent>
      </Card>
    );
  }

  return (
    <Card>
      <CardHeader className="pb-3">
        <div className="flex items-center justify-between">
          <CardTitle className="flex items-center gap-2 text-base">
            <Activity className="h-5 w-5" />
            Recent Activity
          </CardTitle>
          <Badge variant="secondary" className="text-xs">
            {activities.length} operations
          </Badge>
        </div>
      </CardHeader>
      <CardContent>
        {activities.length === 0 ? (
          <div className="py-8 text-center text-muted-foreground">
            <Activity className="h-10 w-10 mx-auto mb-3 opacity-50" />
            <p className="font-medium">No recent activity</p>
            <p className="text-sm">Operations will appear here when executed</p>
          </div>
        ) : (
          <ScrollArea style={{ height: maxHeight }}>
            <div className="space-y-1 pr-4">
              {activities.map((activity) => {
                const OperationIcon = operationIcons[activity.operation_type] || Activity;
                const status = statusConfig[activity.status] || statusConfig.pending;
                const StatusIcon = status.icon;

                return (
                  <div
                    key={activity.id}
                    className="flex items-start gap-3 p-3 rounded-lg hover:bg-muted/50 transition-colors"
                  >
                    <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-full bg-muted">
                      <OperationIcon className="h-4 w-4 text-muted-foreground" />
                    </div>
                    <div className="flex-1 min-w-0">
                      <div className="flex items-center gap-2">
                        <span className="font-medium text-sm truncate">
                          {formatName(activity.target_name)}
                        </span>
                        <span className="text-muted-foreground text-xs">
                          {activity.operation_type.replace('_', ' ')}
                        </span>
                      </div>
                      <div className="flex items-center gap-2 mt-0.5">
                        <StatusIcon className={cn('h-3 w-3', status.color, activity.status === 'in_progress' && 'animate-spin')} />
                        <span className="text-xs text-muted-foreground">
                          {formatTimeAgo(activity.started_at)}
                        </span>
                        {activity.error_message && (
                          <span className="text-xs text-destructive truncate max-w-48">
                            {activity.error_message}
                          </span>
                        )}
                      </div>
                    </div>
                    <Badge variant={status.variant} className="shrink-0 text-xs capitalize">
                      {activity.status.replace('_', ' ')}
                    </Badge>
                  </div>
                );
              })}
            </div>
          </ScrollArea>
        )}
      </CardContent>
    </Card>
  );
}
