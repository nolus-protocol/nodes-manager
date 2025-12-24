import { Badge, Tooltip, TooltipContent, TooltipTrigger, TooltipProvider } from '@kostovster/ui';
import { Clock, Calendar } from 'lucide-react';
import { formatNextRun } from '@/lib/cron';

interface ScheduleItemProps {
  label: string;
  schedule: string | undefined;
  enabled?: boolean;
}

export function ScheduleItem({ label, schedule, enabled = true }: ScheduleItemProps) {
  if (!enabled) return null;
  
  return (
    <div className="flex flex-col gap-1">
      <span className="text-xs font-medium text-muted-foreground">
        {label}
      </span>
      {schedule ? (
        <TooltipProvider>
          <Tooltip>
            <TooltipTrigger asChild>
              <Badge variant="outline" className="w-fit font-mono text-xs cursor-help">
                <Clock className="h-3 w-3 mr-1" />
                {schedule}
              </Badge>
            </TooltipTrigger>
            <TooltipContent>
              <p>Cron expression: {schedule}</p>
            </TooltipContent>
          </Tooltip>
        </TooltipProvider>
      ) : (
        <span className="text-xs text-muted-foreground">Not scheduled</span>
      )}
    </div>
  );
}

interface NextRunItemProps {
  label: string;
  schedule: string | undefined;
  enabled?: boolean;
}

export function NextRunItem({ label, schedule, enabled = true }: NextRunItemProps) {
  if (!enabled) return null;
  
  const nextRun = formatNextRun(schedule);
  
  return (
    <div className="flex flex-col gap-1">
      <span className="text-xs font-medium text-muted-foreground">
        {label}
      </span>
      {nextRun ? (
        <span className="text-xs font-medium flex items-center gap-1">
          <Calendar className="h-3 w-3 text-muted-foreground" />
          {nextRun}
        </span>
      ) : (
        <span className="text-xs text-muted-foreground">Not scheduled</span>
      )}
    </div>
  );
}
