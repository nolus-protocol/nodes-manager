import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  Badge,
  ScrollArea,
  Skeleton,
  Tooltip,
  TooltipContent,
  TooltipTrigger,
  cn,
} from '@kostovster/ui';
import { Calendar, Scissors, Camera, RefreshCw, Clock } from 'lucide-react';
import { formatName } from '@/lib/utils';
import { getNextRun, formatNextRun } from '@/lib/cron';
import type { NodeConfig } from '@/types';

interface ScheduleItem {
  nodeName: string;
  operationType: 'pruning' | 'snapshot' | 'state_sync';
  schedule: string;
  nextRun: Date;
}

interface UpcomingScheduleProps {
  configs: Record<string, NodeConfig>;
  isLoading?: boolean;
  maxItems?: number;
}

const operationConfig: Record<string, { icon: typeof Scissors; label: string; color: string }> = {
  pruning: { icon: Scissors, label: 'Pruning', color: 'text-orange-500' },
  snapshot: { icon: Camera, label: 'Snapshot', color: 'text-blue-500' },
  state_sync: { icon: RefreshCw, label: 'State Sync', color: 'text-purple-500' },
};

function getUpcomingSchedules(configs: Record<string, NodeConfig>, maxItems: number): ScheduleItem[] {
  const schedules: ScheduleItem[] = [];

  Object.entries(configs).forEach(([nodeName, config]) => {
    if (config.pruning_enabled && config.pruning_schedule) {
      const nextRun = getNextRun(config.pruning_schedule);
      if (nextRun) {
        schedules.push({ nodeName, operationType: 'pruning', schedule: config.pruning_schedule, nextRun });
      }
    }
    if (config.snapshots_enabled && config.snapshot_schedule) {
      const nextRun = getNextRun(config.snapshot_schedule);
      if (nextRun) {
        schedules.push({ nodeName, operationType: 'snapshot', schedule: config.snapshot_schedule, nextRun });
      }
    }
    if (config.state_sync_enabled && config.state_sync_schedule) {
      const nextRun = getNextRun(config.state_sync_schedule);
      if (nextRun) {
        schedules.push({ nodeName, operationType: 'state_sync', schedule: config.state_sync_schedule, nextRun });
      }
    }
  });

  return schedules
    .sort((a, b) => a.nextRun.getTime() - b.nextRun.getTime())
    .slice(0, maxItems);
}

function formatRelativeTime(date: Date): string {
  const now = new Date();
  const diffMs = date.getTime() - now.getTime();
  const diffMins = Math.floor(diffMs / 60000);
  const diffHours = Math.floor(diffMs / 3600000);
  const diffDays = Math.floor(diffMs / 86400000);

  if (diffMins < 60) return `in ${diffMins}m`;
  if (diffHours < 24) return `in ${diffHours}h`;
  return `in ${diffDays}d`;
}

export function UpcomingSchedule({ configs, isLoading = false, maxItems = 8 }: UpcomingScheduleProps) {
  const schedules = getUpcomingSchedules(configs, maxItems);

  if (isLoading) {
    return (
      <Card>
        <CardHeader className="pb-3">
          <CardTitle className="flex items-center gap-2 text-base">
            <Calendar className="h-5 w-5" />
            Upcoming Schedules
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="space-y-3">
            {[...Array(5)].map((_, i) => (
              <div key={i} className="flex items-center justify-between">
                <div className="flex items-center gap-3">
                  <Skeleton className="h-8 w-8 rounded" />
                  <div className="space-y-1">
                    <Skeleton className="h-4 w-32" />
                    <Skeleton className="h-3 w-20" />
                  </div>
                </div>
                <Skeleton className="h-5 w-12" />
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
            <Calendar className="h-5 w-5" />
            Upcoming Schedules
          </CardTitle>
          <Badge variant="secondary" className="text-xs">
            Next {schedules.length}
          </Badge>
        </div>
      </CardHeader>
      <CardContent>
        {schedules.length === 0 ? (
          <div className="py-8 text-center text-muted-foreground">
            <Calendar className="h-10 w-10 mx-auto mb-3 opacity-50" />
            <p className="font-medium">No scheduled operations</p>
            <p className="text-sm">Configure schedules in node settings</p>
          </div>
        ) : (
          <ScrollArea className="h-80">
            <div className="space-y-1 pr-4">
              {schedules.map((item, index) => {
                const config = operationConfig[item.operationType];
                const Icon = config.icon;

                return (
                  <div
                    key={`${item.nodeName}-${item.operationType}-${index}`}
                    className="flex items-center justify-between p-3 rounded-lg hover:bg-muted/50 transition-colors"
                  >
                    <div className="flex items-center gap-3">
                      <div className={cn('flex h-9 w-9 items-center justify-center rounded-lg bg-muted')}>
                        <Icon className={cn('h-4 w-4', config.color)} />
                      </div>
                      <div>
                        <div className="flex items-center gap-2">
                          <span className="font-medium text-sm">{formatName(item.nodeName)}</span>
                          <Badge variant="outline" className="text-xs">
                            {config.label}
                          </Badge>
                        </div>
                        <Tooltip>
                          <TooltipTrigger asChild>
                            <div className="flex items-center gap-1 text-xs text-muted-foreground cursor-help">
                              <Clock className="h-3 w-3" />
                              {formatNextRun(item.schedule)}
                            </div>
                          </TooltipTrigger>
                          <TooltipContent>
                            <p>Cron: {item.schedule}</p>
                          </TooltipContent>
                        </Tooltip>
                      </div>
                    </div>
                    <Badge variant="secondary" className="text-xs font-mono">
                      {formatRelativeTime(item.nextRun)}
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
